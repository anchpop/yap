use std::{
    cell::RefCell,
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};

use opfs::{
    DirectoryEntry, DirectoryHandle as _, FileHandle as _, WritableFileStream as _,
    persistent::{self, DirectoryHandle, FileHandle},
};

use crate::data_model::{
    Clock, EventStoreWithListeners, IndexedEvent, ListenerKey, Listeners, SyncTarget, Timestamped,
};
use futures::{Stream, StreamExt};

const EVENTS_FILE_NAME: &str = "events.blob";
const EVENT_LOG_MAGIC: &[u8] = b"WEAPONLG";
const EVENT_LOG_VERSION: u32 = 1;
const EVENT_LOG_HEADER_LEN: usize = EVENT_LOG_MAGIC.len() + 4;

impl<L: Listeners<String>> EventStoreWithListeners<String, String, L> {
    /// Full OPFS sync wrapper: marks lifecycle, runs inner sync, records result.
    pub async fn sync_with_opfs(
        store: &RefCell<Self>,
        user_directory: &UserDirectory,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<(), persistent::Error> {
        store.borrow_mut().mark_sync_started(SyncTarget::Opfs);

        let result =
            Self::sync_with_opfs_inner(store, user_directory, stream_id_to_sync.clone(), modifier)
                .await;

        match &result {
            Ok(()) => store
                .borrow_mut()
                .mark_sync_finished(SyncTarget::Opfs, None),
            Err(e) => store
                .borrow_mut()
                .mark_sync_finished(SyncTarget::Opfs, Some(format!("{e:?}"))),
        }

        result
    }

    /// Performs OPFS load/save for either a specific stream or all streams, then
    /// refreshes and records the OPFS clock in the sync state.
    async fn sync_with_opfs_inner(
        store: &RefCell<Self>,
        user_directory: &UserDirectory,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<(), persistent::Error> {
        // 1) Load fresh events from OPFS into memory
        if let Some(stream_id) = stream_id_to_sync.clone() {
            Self::load_from_local_storage(store, user_directory, stream_id.clone(), modifier)
                .await?;
        } else {
            let mut streams = user_directory.event_stream_directories().await?;
            while let Some((stream_id, _)) = streams.next().await {
                Self::load_from_local_storage(store, user_directory, stream_id.clone(), modifier)
                    .await?;
            }
        }

        // 2) Save any in-memory events to OPFS
        if let Some(stream_id) = stream_id_to_sync.clone() {
            let _ = Self::save_to_local_storage(store, user_directory, stream_id.clone()).await?;
        } else {
            // Persist all streams present in the store
            let stream_ids: Vec<String> =
                store.borrow().iter().map(|(sid, _)| sid.clone()).collect();
            for stream_id in stream_ids {
                let _ =
                    Self::save_to_local_storage(store, user_directory, stream_id.clone()).await?;
            }
        }

        // 3) Refresh OPFS remote clock and record it in sync state
        let final_clock = get_opfs_clock(user_directory, stream_id_to_sync.as_deref()).await?;
        store
            .borrow_mut()
            .update_sync_clock(SyncTarget::Opfs, final_clock);

        Ok(())
    }
    /// Reload events from local storage and merge with current state
    pub async fn load_from_local_storage(
        store: &RefCell<Self>,
        user_directory: &UserDirectory,
        stream_id: String,
        modifier: Option<ListenerKey>,
    ) -> Result<(), persistent::Error> {
        let stream_directory = user_directory.get_stream_directory(&stream_id).await?;
        let event_log_file = stream_directory.get_event_log_file().await?;

        let mut counts: BTreeMap<String, usize> = {
            let store_ref = store.borrow();
            store_ref
                .get_raw(stream_id.clone())
                .map(|s| {
                    s.num_events_per_device()
                        .into_iter()
                        .map(|(device, count)| (device.clone(), count))
                        .collect()
                })
                .unwrap_or_default()
        };

        let stored_events = event_log_file
            .read_records(&counts)
            .await
            .inspect_err(|e| log::error!("Failed to reload from local storage: {e:?}"))?;

        let mut events_to_add: BTreeMap<String, Vec<Timestamped<serde_json::Value>>> =
            BTreeMap::new();

        for record in stored_events {
            let device_id = record.device_id;
            let event_index = record.within_device_events_index;
            let event = record.event;

            let entry = counts.entry(device_id.clone()).or_insert(0);
            let expected_index = *entry;
            if event_index < expected_index {
                log::error!(
                    "OPFS log backtrack detected for stream {stream_id} device {device_id}: expected index {expected_index}, found {event_index}",
                );
                continue;
            }

            if event_index > expected_index {
                log::error!(
                    "OPFS log gap detected for stream {stream_id} device {device_id}: expected index {expected_index}, found {event_index}",
                );
            }

            events_to_add
                .entry(device_id.clone())
                .or_default()
                .push(event);
            *entry = event_index + 1;
        }

        if events_to_add.is_empty() {
            return Ok(());
        }

        let mut store_mut = store.borrow_mut();
        for (device_id, events) in events_to_add {
            store_mut.add_device_events_jsons(stream_id.clone(), device_id, events, modifier);
        }

        Ok(())
    }

    /// Save events to local storage
    pub async fn save_to_local_storage(
        store: &RefCell<Self>,
        user_directory: &UserDirectory,
        stream_id: String,
    ) -> Result<usize, persistent::Error> {
        let _guard = weblocks::acquire(
            &format!("opfs-save-to-local-storage-{stream_id}"),
            weblocks::AcquireOptions::exclusive(),
        )
        .await?;
        let mut total_written: usize = 0;

        // Local desired counts per device for this stream
        let Some(device_events) = store.borrow().vector_clock().remove(&stream_id) else {
            log::warn!("Stream {stream_id} not found in store, skipping save");
            return Ok(0);
        };

        let stream_directory = user_directory.get_stream_directory(&stream_id).await?;
        let event_log_file = stream_directory.get_event_log_file().await?;

        // On-disk clock for this stream (asserts contiguity of indices 0..=n-1)
        let opfs_clock = get_opfs_clock(user_directory, Some(&stream_id)).await?;
        let device_counts_on_disk = opfs_clock.get(&stream_id).cloned().unwrap_or_default();

        let mut records_to_append: Vec<EventLogRecord> = Vec::new();

        for (device_id, _num_events_in_memory) in device_events {
            let device_events_on_disk = device_counts_on_disk.get(&device_id).copied().unwrap_or(0);

            let events_to_write: Vec<Timestamped<serde_json::Value>> = {
                let store_ref = store.borrow();
                let Some(stream) = store_ref.get_raw(stream_id.clone()) else {
                    log::error!(
                        "Stream {stream_id} not found in store, which should be impossible as we already checked for it"
                    );
                    continue;
                };
                stream.jsons(&device_id, device_events_on_disk)
            };

            for event in events_to_write {
                records_to_append.push(EventLogRecord {
                    device_id: device_id.clone(),
                    within_device_events_index: event.within_device_events_index(),
                    event,
                });
            }
        }

        if !records_to_append.is_empty() {
            event_log_file.append_records(&records_to_append).await?;
            total_written += records_to_append.len();
        }

        // Tell the other instances of the app (browser tabs) to reload this stream.
        if total_written > 0 {
            #[derive(serde::Serialize)]
            struct Written<'a> {
                r#type: &'static str,
                stream_id: &'a str,
            }
            log::info!("Broadcasting opfs-written message for stream: {stream_id}");
            if let Err(error) = bridgerton::platform::broadcast(
                "weapon-opfs-sync",
                &Written {
                    r#type: "opfs-written",
                    stream_id: &stream_id,
                },
            ) {
                log::error!("Failed to broadcast: {error}");
            }
        }

        Ok(total_written)
    }

    /// Import events from the logged-out user directory into the current user's directory.
    /// This is used when a user first logs in so their offline data is preserved.
    pub async fn import_logged_out_user_data(
        mut weapon_directory: DirectoryHandle,
        mut user_events_directory: DirectoryHandle,
        current_user_directory: &UserDirectory,
    ) -> Result<(), persistent::Error> {
        // Attempt to get the logged-out directory. If it doesn't exist, there's nothing to do.
        let logged_out_directory = match user_events_directory
            .get_directory_handle_with_options(
                "user__logged-out-unknown-user",
                &opfs::GetDirectoryHandleOptions { create: false },
            )
            .await
        {
            Ok(dir) => UserDirectory {
                directory_handle: dir,
            },
            Err(_) => return Ok(()),
        };

        // If the current user directory already has data, skip the import.
        let mut existing_streams = current_user_directory.event_stream_directories().await?;
        if existing_streams.next().await.is_some() {
            return Ok(());
        }

        // Move all streams/devices/events from the logged-out directory.
        let mut streams = logged_out_directory.event_stream_directories().await?;
        while let Some((stream_id, stream_dir)) = streams.next().await {
            let target_stream_dir = current_user_directory
                .get_stream_directory(&stream_id)
                .await?;
            let source_log = stream_dir.get_event_log_file().await?;
            let events = source_log
                .read_records(&BTreeMap::new())
                .await
                .inspect_err(|e| log::error!("Failed to reload from local storage: {e:?}"))?;

            if events.is_empty() {
                continue;
            }

            let target_log = target_stream_dir.get_event_log_file().await?;
            target_log.append_records(&events).await?;
        }

        let _ = weapon_directory.remove_entry("device-id-logged-out").await;

        // Remove the logged-out user directory itself now that everything is moved.
        let _ = user_events_directory
            .remove_entry_with_options(
                "user__logged-out-unknown-user",
                &opfs::FileSystemRemoveOptions { recursive: true },
            )
            .await
            .inspect_err(|e| log::error!("Failed to remove logged-out user directory: {e:?}"));

        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct UserDirectory {
    directory_handle: DirectoryHandle,
}

#[derive(Debug, Clone)]
pub struct StreamDirectory {
    directory_handle: DirectoryHandle,
}

#[derive(Debug, Clone)]
pub struct EventLogFile {
    file_handle: FileHandle,
}

#[derive(Debug, Clone)]
pub struct EventLogRecord {
    pub device_id: String,
    pub within_device_events_index: usize,
    pub event: Timestamped<serde_json::Value>,
}

impl UserDirectory {
    pub async fn new(parent: &DirectoryHandle, user_id: &str) -> Result<Self, persistent::Error> {
        Ok(Self {
            directory_handle: parent
                .get_directory_handle_with_options(
                    &format!("user__{user_id}"),
                    &opfs::GetDirectoryHandleOptions { create: true },
                )
                .await?,
        })
    }

    #[allow(dead_code)]
    async fn event_stream_directories(
        &self,
    ) -> Result<impl Stream<Item = (String, StreamDirectory)>, persistent::Error> {
        Ok(self.directory_handle.entries().await?.filter_map(|entry| {
            let (directory_name, stream_directory) = match entry {
                Ok(res) => res,
                Err(e) => {
                    log::error!("Failed to get stream directory: {e:?}");
                    return futures::future::ready(None);
                }
            };
            let Some(stream_id) = directory_name.strip_prefix("stream__") else {
                return futures::future::ready(None);
            };
            let DirectoryEntry::Directory(stream_directory) = stream_directory else {
                return futures::future::ready(None);
            };
            let stream_directory = StreamDirectory {
                directory_handle: stream_directory,
            };
            futures::future::ready(Some((stream_id.to_string(), stream_directory)))
        }))
    }

    async fn get_stream_directory(
        &self,
        stream_id: &str,
    ) -> Result<StreamDirectory, persistent::Error> {
        Ok(StreamDirectory {
            directory_handle: self
                .directory_handle
                .get_directory_handle_with_options(
                    &format!("stream__{stream_id}"),
                    &opfs::GetDirectoryHandleOptions { create: true },
                )
                .await?,
        })
    }
}

impl StreamDirectory {
    async fn get_event_log_file(&self) -> Result<EventLogFile, persistent::Error> {
        Ok(EventLogFile {
            file_handle: self
                .directory_handle
                .get_file_handle_with_options(
                    EVENTS_FILE_NAME,
                    &opfs::GetFileHandleOptions { create: true },
                )
                .await?,
        })
    }
}

impl EventLogFile {
    async fn read_records(
        &self,
        skip_counts: &BTreeMap<String, usize>,
    ) -> Result<Vec<EventLogRecord>, persistent::Error> {
        let bytes = self.file_handle.read().await?;
        Ok(parse_event_log_records_with_skip(&bytes, skip_counts))
    }

    async fn append_records(&self, records: &[EventLogRecord]) -> Result<(), persistent::Error> {
        if records.is_empty() {
            return Ok(());
        }

        let existing_size = self.file_handle.size().await?;
        let mut file_handle = self.file_handle.clone();
        let mut writable = file_handle
            .create_writable_with_options(&opfs::CreateWritableOptions {
                keep_existing_data: true,
            })
            .await?;

        if existing_size < EVENT_LOG_HEADER_LEN {
            writable.truncate(0).await?;
            let header = event_log_header_bytes();
            writable.write_at_cursor_pos(&header).await?;
            writable.seek(EVENT_LOG_HEADER_LEN).await?;
        } else {
            writable.seek(existing_size).await?;
        }

        for record in records {
            if let Some(bytes) = encode_event_log_record(record) {
                writable.write_at_cursor_pos(&bytes).await?;
            }
        }

        writable.close().await?;

        Ok(())
    }

    async fn device_counts(&self) -> Result<BTreeMap<String, usize>, persistent::Error> {
        let bytes = self.file_handle.read().await?;
        Ok(parse_device_counts(&bytes))
    }
}

/// Build a clock of on-disk counts per stream/device in OPFS.
async fn get_opfs_clock(
    user_directory: &UserDirectory,
    only_stream: Option<&str>,
) -> Result<Clock<String, String>, persistent::Error> {
    let mut clock: Clock<String, String> = BTreeMap::new();

    if let Some(stream_id) = only_stream {
        let stream_dir = user_directory.get_stream_directory(stream_id).await?;
        let device_counts = stream_dir
            .get_event_log_file()
            .await?
            .device_counts()
            .await?;
        clock.insert(stream_id.to_string(), device_counts);
        return Ok(clock);
    }

    let mut streams = user_directory.event_stream_directories().await?;
    while let Some((stream_id, stream_dir)) = streams.next().await {
        let device_counts = stream_dir
            .get_event_log_file()
            .await?
            .device_counts()
            .await?;
        clock.insert(stream_id, device_counts);
    }

    Ok(clock)
}

fn event_log_header_bytes() -> Vec<u8> {
    let mut header = Vec::with_capacity(EVENT_LOG_HEADER_LEN);
    header.extend_from_slice(EVENT_LOG_MAGIC);
    header.extend_from_slice(&EVENT_LOG_VERSION.to_le_bytes());
    header
}

fn encode_event_log_record(record: &EventLogRecord) -> Option<Vec<u8>> {
    let payload = match serde_json::to_vec(&record.event) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::error!(
                "Failed to serialize event for device {}: {e:?}",
                record.device_id
            );
            return None;
        }
    };

    let device_id_bytes = record.device_id.as_bytes();
    let device_id_len: u32 = match device_id_bytes.len().try_into() {
        Ok(len) => len,
        Err(_) => {
            log::error!(
                "Device ID too long to encode for device {} ({} bytes)",
                record.device_id,
                device_id_bytes.len()
            );
            return None;
        }
    };

    let payload_len: u32 = match payload.len().try_into() {
        Ok(len) => len,
        Err(_) => {
            log::error!(
                "Event payload too large to encode for device {} ({} bytes)",
                record.device_id,
                payload.len()
            );
            return None;
        }
    };

    let body_len = std::mem::size_of::<u64>()
        + std::mem::size_of::<u32>()
        + device_id_bytes.len()
        + std::mem::size_of::<u32>()
        + payload.len();

    let record_len: u32 = match body_len.try_into() {
        Ok(len) => len,
        Err(_) => {
            log::error!(
                "Record too large to encode for device {} ({} bytes)",
                record.device_id,
                body_len
            );
            return None;
        }
    };

    let mut buffer = Vec::with_capacity(std::mem::size_of::<u32>() + body_len);
    buffer.extend_from_slice(&record_len.to_le_bytes());
    buffer.extend_from_slice(&(record.within_device_events_index as u64).to_le_bytes());
    buffer.extend_from_slice(&device_id_len.to_le_bytes());
    buffer.extend_from_slice(device_id_bytes);
    buffer.extend_from_slice(&payload_len.to_le_bytes());
    buffer.extend_from_slice(&payload);

    Some(buffer)
}

struct RawEventLogRecord<'a> {
    within_device_events_index: u64,
    device_id_bytes: &'a [u8],
    payload_bytes: &'a [u8],
}

fn event_log_records_iter(bytes: &[u8]) -> impl Iterator<Item = RawEventLogRecord<'_>> {
    struct EventLogIterator<'a> {
        bytes: &'a [u8],
        offset: usize,
        validated: bool,
    }

    impl<'a> Iterator for EventLogIterator<'a> {
        type Item = RawEventLogRecord<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            if !self.validated {
                if self.bytes.is_empty() {
                    return None;
                }
                if self.bytes.len() < EVENT_LOG_HEADER_LEN {
                    log::warn!("Event log header too small ({} bytes)", self.bytes.len());
                    return None;
                }
                if !self.bytes.starts_with(EVENT_LOG_MAGIC) {
                    log::warn!("Event log magic bytes did not match");
                    return None;
                }
                let version_offset = EVENT_LOG_MAGIC.len();
                let version = u32::from_le_bytes(
                    self.bytes[version_offset..version_offset + 4]
                        .try_into()
                        .unwrap(),
                );
                if version != EVENT_LOG_VERSION {
                    log::warn!("Unsupported event log version {version}");
                    return None;
                }
                self.offset = EVENT_LOG_HEADER_LEN;
                self.validated = true;
            }

            if self.offset + std::mem::size_of::<u32>() > self.bytes.len() {
                return None;
            }

            let record_len = u32::from_le_bytes(
                self.bytes[self.offset..self.offset + std::mem::size_of::<u32>()]
                    .try_into()
                    .unwrap(),
            ) as usize;
            self.offset += std::mem::size_of::<u32>();

            if self.offset + record_len > self.bytes.len() {
                log::warn!(
                    "Event log record length {} exceeds remaining bytes {}",
                    record_len,
                    self.bytes.len() - self.offset
                );
                return None;
            }

            let record_end = self.offset + record_len;
            if record_len < std::mem::size_of::<u64>() + 2 * std::mem::size_of::<u32>() {
                log::warn!("Event log record too small ({record_len} bytes)");
                self.offset = record_end;
                return self.next();
            }

            let within_device = u64::from_le_bytes(
                self.bytes[self.offset..self.offset + std::mem::size_of::<u64>()]
                    .try_into()
                    .unwrap(),
            );
            self.offset += std::mem::size_of::<u64>();

            let device_len = u32::from_le_bytes(
                self.bytes[self.offset..self.offset + std::mem::size_of::<u32>()]
                    .try_into()
                    .unwrap(),
            ) as usize;
            self.offset += std::mem::size_of::<u32>();

            if self.offset + device_len > record_end {
                log::warn!("Device ID length {device_len} exceeds record bounds");
                self.offset = record_end;
                return self.next();
            }
            let device_id_bytes = &self.bytes[self.offset..self.offset + device_len];
            self.offset += device_len;

            let payload_len = u32::from_le_bytes(
                self.bytes[self.offset..self.offset + std::mem::size_of::<u32>()]
                    .try_into()
                    .unwrap(),
            ) as usize;
            self.offset += std::mem::size_of::<u32>();

            if self.offset + payload_len > record_end {
                log::warn!("Payload length {payload_len} exceeds record bounds");
                self.offset = record_end;
                return self.next();
            }
            let payload_bytes = &self.bytes[self.offset..self.offset + payload_len];
            self.offset = record_end;

            Some(RawEventLogRecord {
                within_device_events_index: within_device,
                device_id_bytes,
                payload_bytes,
            })
        }
    }

    EventLogIterator {
        bytes,
        offset: 0,
        validated: false,
    }
}

pub fn parse_event_log_records(bytes: &[u8]) -> Vec<EventLogRecord> {
    parse_event_log_records_with_skip(bytes, &BTreeMap::new())
}

fn parse_event_log_records_with_skip(
    bytes: &[u8],
    skip_counts: &BTreeMap<String, usize>,
) -> Vec<EventLogRecord> {
    let mut records = Vec::new();

    for raw_record in event_log_records_iter(bytes) {
        let within_device_index = match usize::try_from(raw_record.within_device_events_index) {
            Ok(index) => index,
            Err(_) => {
                log::warn!(
                    "within_device_events_index {} overflowed usize",
                    raw_record.within_device_events_index
                );
                continue;
            }
        };

        let device_id = match String::from_utf8(raw_record.device_id_bytes.to_vec()) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("Device ID was not valid UTF-8: {e:?}");
                continue;
            }
        };

        // Skip events we already have
        let skip_count = skip_counts.get(&device_id).copied().unwrap_or(0);
        if within_device_index < skip_count {
            // This event is already known, skip parsing the payload
            continue;
        }

        match serde_json::from_slice::<Timestamped<serde_json::Value>>(raw_record.payload_bytes) {
            Ok(event) => records.push(EventLogRecord {
                device_id,
                within_device_events_index: within_device_index,
                event,
            }),
            Err(e) => {
                log::warn!("Failed to deserialize event payload: {e:?}");
            }
        }
    }

    records
}

pub fn parse_device_counts(bytes: &[u8]) -> BTreeMap<String, usize> {
    let start_time = web_time::Instant::now();
    let bytes_len = bytes.len();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut record_count = 0usize;

    for raw_record in event_log_records_iter(bytes) {
        record_count += 1;

        let within_device_index = match usize::try_from(raw_record.within_device_events_index) {
            Ok(index) => index,
            Err(_) => {
                log::warn!(
                    "within_device_events_index {} overflowed usize",
                    raw_record.within_device_events_index
                );
                continue;
            }
        };

        let device_id = match String::from_utf8(raw_record.device_id_bytes.to_vec()) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("Device ID was not valid UTF-8: {e:?}");
                continue;
            }
        };

        let entry = counts.entry(device_id.clone()).or_insert(0);
        let expected = *entry;
        if within_device_index != expected {
            log::error!(
                "OPFS index gap for device {device_id}: expected {expected}, found {within_device_index}"
            );
            panic!("OPFS device indices not contiguous");
        }
        *entry += 1;
    }

    let duration = start_time.elapsed().as_secs_f64() * 1000.0;
    if duration > 0.0 {
        let throughput = bytes_len as f64 / duration / 1024.0; // KB/ms
        log::info!(
            "parse_device_counts: processed {} records from {} bytes in {:.2}ms ({:.2} KB/ms, {:.2} records/ms)",
            record_count,
            bytes_len,
            duration,
            throughput,
            record_count as f64 / duration
        );
    }

    counts
}
