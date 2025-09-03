use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

#[cfg(target_arch = "wasm32")]
use js_sys;
#[cfg(target_arch = "wasm32")]
use web_sys::BroadcastChannel;

use opfs::{
    DirectoryEntry, DirectoryHandle as _, FileHandle as _, WritableFileStream as _,
    persistent::{self, DirectoryHandle, FileHandle},
};

use crate::data_model::{Clock, EventStore, IndexedEvent, ListenerKey, SyncTarget, Timestamped};
use futures::{Stream, StreamExt};

#[allow(dead_code)]
#[derive(Debug)]
enum EventReadError {
    Opfs(persistent::Error),
    InvalidJson(serde_json::Error),
    Serde(serde_json::Error),
}

impl EventStore<String, String> {
    /// Full OPFS sync wrapper: marks lifecycle, runs inner sync, records result.
    pub async fn sync_with_opfs(
        store: &RefCell<EventStore<String, String>>,
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
        store: &RefCell<EventStore<String, String>>,
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
        store: &RefCell<EventStore<String, String>>,
        user_directory: &UserDirectory,
        stream_id: String,
        modifier: Option<ListenerKey>,
    ) -> Result<(), persistent::Error> {
        let stream_directory = user_directory.get_stream_directory(&stream_id).await?;
        let mut device_directories = stream_directory.device_directories().await?;
        while let Some((device_id, device_directory)) = device_directories.next().await {
            let current_num_events = store
                .borrow()
                .get_raw(stream_id.clone())
                .and_then(|s| s.num_events_per_device().get(&device_id).copied())
                .unwrap_or(0);

            // Load fresh events from storage
            let fresh_events = device_directory
                .read_device_events(current_num_events)
                .await
                .inspect_err(|e| log::error!("Failed to reload from local storage: {e:?}"))?
                .collect::<Vec<_>>()
                .await;

            // Add fresh events to current state
            store.borrow_mut().add_device_events_jsons(
                stream_id.clone(),
                device_id.to_string(),
                fresh_events,
                modifier,
            );
        }

        Ok(())
    }

    /// Save events to local storage
    // todo: this should use the WebLocks API to prevent multiple saves from happening at once
    pub async fn save_to_local_storage(
        store: &RefCell<EventStore<String, String>>,
        user_directory: &UserDirectory,
        stream_id: String,
    ) -> Result<usize, persistent::Error> {
        let mut total_written: usize = 0;

        // Local desired counts per device for this stream
        let Some(device_events) = store.borrow().vector_clock().remove(&stream_id) else {
            log::warn!("Stream {stream_id} not found in store, skipping save");
            return Ok(0);
        };

        let stream_directory = user_directory.get_stream_directory(&stream_id).await?;

        // On-disk clock for this stream (asserts contiguity of indices 0..=n-1)
        let opfs_clock = get_opfs_clock(user_directory, Some(&stream_id)).await?;
        let device_counts_on_disk = opfs_clock.get(&stream_id).cloned().unwrap_or_default();

        for (device_id, _num_events) in device_events {
            let device_directory = stream_directory.get_device_directory(&device_id).await?;
            let device_events_on_disk = device_counts_on_disk.get(&device_id).copied().unwrap_or(0);

            // Collect events with index >= device_events_on_disk (contiguous range guaranteed)
            let events_to_write: Vec<Timestamped<serde_json::Value>> = {
                // contortions to avoid holding the lock across an .await
                let store = store.borrow();
                let Some(stream) = store.get_raw(stream_id.clone()) else {
                    log::error!(
                        "Stream {stream_id} not found in store, which should be impossible as we already checked for it"
                    );
                    continue;
                };
                stream.jsons(&device_id, device_events_on_disk)
            };

            for event in events_to_write {
                device_directory.write_event_file(&event).await?;
                total_written += 1;
            }
        }

        // If we wrote anything, broadcast a message to other tabs
        #[cfg(target_arch = "wasm32")]
        if total_written > 0 {
            match BroadcastChannel::new("weapon-opfs-sync") {
                Ok(channel) => {
                    // Create a simple JS object directly
                    let obj = js_sys::Object::new();
                    js_sys::Reflect::set(&obj, &"type".into(), &"opfs-written".into()).unwrap();
                    js_sys::Reflect::set(&obj, &"stream_id".into(), &stream_id.as_str().into())
                        .unwrap();

                    log::info!(
                        "Broadcasting opfs-written message for stream: {}",
                        stream_id
                    );
                    match channel.post_message(&obj) {
                        Ok(_) => log::info!("Message posted successfully"),
                        Err(e) => log::error!("Failed to post message: {:?}", e),
                    }
                }
                Err(e) => {
                    log::error!("Failed to create BroadcastChannel: {:?}", e);
                }
            }
        }

        Ok(total_written)
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
pub struct DeviceDirectory {
    directory_handle: DirectoryHandle,
}

#[derive(Debug, Clone)]
pub struct EventFile {
    file_handle: FileHandle,
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
    async fn device_directories(
        &self,
    ) -> Result<impl Stream<Item = (String, DeviceDirectory)>, persistent::Error> {
        Ok(self.directory_handle.entries().await?.filter_map(|entry| {
            let (directory_name, device_directory) = match entry {
                Ok(res) => res,
                Err(e) => {
                    log::error!("Failed to get device directory: {e:?}");
                    return futures::future::ready(None);
                }
            };
            let DirectoryEntry::Directory(device_directory) = device_directory else {
                return futures::future::ready(None);
            };
            let Some(device_id) = directory_name.strip_prefix("device__") else {
                return futures::future::ready(None);
            };
            let device_directory = DeviceDirectory {
                directory_handle: device_directory,
            };
            futures::future::ready(Some((device_id.to_string(), device_directory)))
        }))
    }

    async fn get_device_directory(
        &self,
        device_id: &str,
    ) -> Result<DeviceDirectory, persistent::Error> {
        Ok(DeviceDirectory {
            directory_handle: self
                .directory_handle
                .get_directory_handle_with_options(
                    &format!("device__{device_id}"),
                    &opfs::GetDirectoryHandleOptions { create: true },
                )
                .await?,
        })
    }
}

impl DeviceDirectory {
    async fn read_device_events(
        &self,
        at_or_above: usize,
    ) -> Result<impl Stream<Item = Timestamped<serde_json::Value>>, persistent::Error> {
        Ok(self.events().await?.filter_map(move |result| async move {
            let (event_index, event_file) = result;
            if event_index < at_or_above {
                return None;
            }
            event_file.read().await.ok()
        }))
    }

    async fn events(&self) -> Result<impl Stream<Item = (usize, EventFile)>, persistent::Error> {
        Ok(self.directory_handle.entries().await?.filter_map(|entry| {
            let (file_name, file) = match entry {
                Ok(res) => res,
                Err(e) => {
                    log::error!("Failed to get event file: {e:?}");
                    return futures::future::ready(None);
                }
            };
            let DirectoryEntry::File(file) = file else {
                return futures::future::ready(None);
            };
            let Some(event_index) = file_name.strip_suffix(".json") else {
                return futures::future::ready(None);
            };
            let Ok(event_index) = event_index.parse::<usize>() else {
                return futures::future::ready(None);
            };
            let event_file = EventFile { file_handle: file };
            futures::future::ready(Some((event_index, event_file)))
        }))
    }

    async fn get_existing_event_indices(&self) -> Result<BTreeSet<usize>, persistent::Error> {
        Ok(self
            .events()
            .await?
            .map(|(index, _)| index)
            .collect::<BTreeSet<_>>()
            .await)
    }

    async fn write_event_file(
        &self,
        event: &Timestamped<serde_json::Value>,
    ) -> Result<(), persistent::Error> {
        let filename = format!(
            "{:0width$}.json",
            event.within_device_events_index(),
            width = 10
        );
        let event_json = serde_json::to_value(event).unwrap();
        let json_str = serde_json::to_string(&event_json).unwrap(); // will not panic

        // Create the file
        let mut file_handle = self
            .directory_handle
            .get_file_handle_with_options(&filename, &opfs::GetFileHandleOptions { create: true })
            .await?;

        // Write the data
        let mut writable = file_handle
            .create_writable_with_options(&opfs::CreateWritableOptions {
                keep_existing_data: false,
            })
            .await?;

        let bytes = json_str.as_bytes();

        writable.write_at_cursor_pos(bytes.to_vec()).await?;
        writable.close().await?;

        Ok(())
    }
}

impl EventFile {
    async fn read(&self) -> Result<Timestamped<serde_json::Value>, EventReadError> {
        let bytes: Vec<u8> = self
            .file_handle
            .read()
            .await
            .map_err(EventReadError::Opfs)?;

        let value = match serde_json::from_slice::<Timestamped<serde_json::Value>>(&bytes) {
            Ok(value) => value,
            Err(e) => {
                log::warn!("Event file was not valid JSON: {e:?}");
                return Err(EventReadError::InvalidJson(e));
            }
        };
        Ok(value)
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
        let mut devices = stream_dir.device_directories().await?;
        let mut device_counts: BTreeMap<String, usize> = BTreeMap::new();
        while let Some((device_id, device_dir)) = devices.next().await {
            let indices = device_dir.get_existing_event_indices().await?;
            // Assert contiguity: must be exactly 0..len-1
            for (expected, idx) in indices.iter().enumerate() {
                if *idx != expected {
                    log::error!(
                        "OPFS index gap for stream {stream_id} device {device_id}: expected {expected}, found {idx}",
                    );
                    panic!("OPFS device indices not contiguous");
                }
            }
            device_counts.insert(device_id, indices.len());
        }
        clock.insert(stream_id.to_string(), device_counts);
        return Ok(clock);
    }

    let mut streams = user_directory.event_stream_directories().await?;
    while let Some((stream_id, stream_dir)) = streams.next().await {
        let mut devices = stream_dir.device_directories().await?;
        let mut device_counts: BTreeMap<String, usize> = BTreeMap::new();
        while let Some((device_id, device_dir)) = devices.next().await {
            let indices = device_dir.get_existing_event_indices().await?;
            // Assert contiguity: must be exactly 0..len-1
            for (expected, idx) in indices.iter().enumerate() {
                if *idx != expected {
                    log::error!(
                        "OPFS index gap for stream {stream_id} device {device_id}: expected {expected}, found {idx}",
                    );
                    panic!("OPFS device indices not contiguous");
                }
            }
            device_counts.insert(device_id, indices.len());
        }
        clock.insert(stream_id, device_counts);
    }

    Ok(clock)
}
