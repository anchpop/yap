//! Utilities for syncing against a Supabase database.
use std::{cell::RefCell, collections::BTreeMap};

use crate::data_model::{Clock, EventStore, ListenerKey, SyncTarget, Timestamped};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[derive(serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SupabaseConfig {
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

impl EventStore<String, String> {
    /// Sync with the server
    /// Return Ok(Some(new_events)) if we got new events from the server.
    /// Return Ok(None) if we didn't get new events from the server.
    /// Return Err(JsValue) if there was an error.
    pub async fn sync_with_supabase(
        store: &RefCell<EventStore<String, String>>,
        access_token: &str,
        supabase_config: SupabaseConfig,
        user_id: &str,
        device_id: &String,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<SupabaseSyncResult, JsValue> {
        store.borrow_mut().mark_sync_started(SyncTarget::Supabase);

        match Self::sync_with_supabase_inner(
            store,
            access_token,
            supabase_config,
            user_id,
            device_id,
            stream_id_to_sync,
            modifier,
        )
        .await
        {
            Ok((res, final_remote_clock)) => {
                store
                    .borrow_mut()
                    .mark_sync_finished(SyncTarget::Supabase, None);
                store
                    .borrow_mut()
                    .update_sync_clock(SyncTarget::Supabase, final_remote_clock);
                Ok(res)
            }
            Err(e) => {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                store
                    .borrow_mut()
                    .mark_sync_finished(SyncTarget::Supabase, Some(msg));
                Err(e)
            }
        }
    }

    async fn sync_with_supabase_inner(
        store: &RefCell<EventStore<String, String>>,
        access_token: &str,
        supabase_config: SupabaseConfig,
        user_id: &str,
        device_id: &String,
        stream_id_to_sync: Option<String>,
        modifier: Option<ListenerKey>,
    ) -> Result<(SupabaseSyncResult, Clock<String, String>), JsValue> {
        let mut sync_result = SupabaseSyncResult {
            uploaded_to_supabase: 0,
            downloaded_from_supabase: 0,
        };

        use fetch_happen::Client;
        use serde_json::json;
        use std::collections::HashMap;

        let SupabaseConfig {
            supabase_url,
            supabase_anon_key,
        } = &supabase_config;

        let vector_clock = store.borrow_mut().vector_clock();
        // If a stream_id_to_sync is provided, narrow the vector clock to just that stream.
        let vector_clock = if let Some(stream_id_to_sync) = stream_id_to_sync {
            let mut vector_clock = vector_clock;
            let narrowed_state = vector_clock.remove(&stream_id_to_sync).unwrap_or_default();
            let mut state = BTreeMap::new();
            state.insert(stream_id_to_sync, narrowed_state);
            state
        } else {
            vector_clock
        };

        // Download new events from server
        let sync_url = format!("{supabase_url}/rest/v1/rpc/sync_events");
        // Create multi-stream request format - wrapped in sync_request parameter
        let payload = json!({
            "sync_request": vector_clock.iter().map(|(stream_id, device_events)| {
                (stream_id, json!({
                    "last_synced_ids": device_events
                }))
            }).collect::<HashMap<_, _>>()
        });

        let client = Client;
        let response = client
            .post(&sync_url)
            .header("apikey", supabase_anon_key)
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&payload)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?
            .send()
            .await
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        if !response.ok() {
            return Err(JsValue::from_str(&format!(
                "Sync failed with status: {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        // Parse the multi-stream response format
        #[allow(clippy::type_complexity)]
        let sync_response: HashMap<
            String,
            HashMap<String, Vec<SyncEventResponse<Timestamped<serde_json::Value>>>>,
        > = serde_json::from_str(&body).map_err(|e| {
            JsValue::from_str(&format!(
                "Failed to parse sync response: {e}\nResponse body: {body}"
            ))
        })?;

        for (stream, device_events) in sync_response {
            for (device, events) in device_events {
                let events = events.into_iter().map(|event| event.event).collect();
                sync_result.downloaded_from_supabase += store.borrow_mut().add_device_events_jsons(
                    stream.clone(),
                    device,
                    events,
                    modifier,
                );
            }
        }

        // Fetch remote event counts for all streams/devices in one RPC
        let remote_clock = get_clock(&client, &supabase_config, access_token, user_id).await?;

        // upload local events if needed
        // first, collect them into a vector to avoid holding the lock across an .await
        let events_to_upload = store
            .borrow()
            .iter()
            .flat_map(|(stream_id, stream_events)| {
                let events_to_upload = {
                    let device_events_on_db: usize = remote_clock
                        .get(stream_id)
                        .and_then(|device_map| device_map.get(&device_id.to_string()).copied())
                        .unwrap_or(0);
                    stream_events.jsons(device_id, device_events_on_db)
                };

                events_to_upload
                    .into_iter()
                    .map(|event| SyncableEvent {
                        user_id: user_id.to_string(),
                        device_id: device_id.to_string(),
                        created_at: event.timestamp.to_string(),
                        within_device_events_index: event.within_device_events_index,
                        event: serde_json::to_value(&event).unwrap(),
                        stream_id: stream_id.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        if !events_to_upload.is_empty() {
            log::info!("Uploading {} events", events_to_upload.len());

            let upload_url = format!("{supabase_url}/rest/v1/events");

            let upload_response = client
                .post(&upload_url)
                .header("apikey", supabase_anon_key)
                .header("Authorization", format!("Bearer {access_token}"))
                .json(&events_to_upload)
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?
                .send()
                .await
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

            if !upload_response.ok() {
                let status = upload_response.status();
                let error_body = upload_response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                log::error!("Failed to upload events: {status} - {error_body}");
            } else {
                log::info!("Successfully uploaded events");
                sync_result.uploaded_to_supabase += events_to_upload.len();
            }
        }

        // Refresh the remote clock after potential uploads and record it.
        // This captures the authoritative counts on the server post-sync.
        let final_remote_clock =
            get_clock(&client, &supabase_config, access_token, user_id).await?;

        log::info!("Sync complete");

        Ok((sync_result, final_remote_clock))
    }
}

fn deserialize_event<'de, E, D>(deserializer: D) -> Result<E, D::Error>
where
    D: serde::de::Deserializer<'de>,
    E: serde::de::DeserializeOwned,
{
    use serde::Deserialize;
    use serde::de::Error;

    // First try to deserialize directly
    let value = serde_json::Value::deserialize(deserializer)?;

    // If it's already an object, try to deserialize it directly
    if value.is_object() {
        return serde_json::from_value(value).map_err(D::Error::custom);
    }

    // If it's a string, parse it as JSON
    if let Some(s) = value.as_str() {
        return serde_json::from_str(s).map_err(D::Error::custom);
    }

    // Otherwise, fail with an appropriate error
    Err(D::Error::custom(
        "Expected either a JSON object or a JSON string",
    ))
}

#[derive(Debug, serde::Deserialize)]
#[serde(bound(deserialize = "Event: serde::de::DeserializeOwned"))]
struct SyncEventResponse<Event> {
    #[expect(unused)]
    id: u64,
    #[serde(deserialize_with = "deserialize_event")]
    event: Event,
    #[expect(unused)]
    // we will use this later, once we stop duplicating the within_device_events_index in Event itself
    within_device_events_index: u32,
}

#[derive(serde::Serialize, serde::Deserialize, tsify::Tsify, Debug)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SyncableEvent {
    pub user_id: String,
    pub device_id: String,
    pub event: serde_json::Value,
    pub created_at: String,
    pub within_device_events_index: usize,
    pub stream_id: String,
}

// Returns: { "<stream_id>": { "<device_id>": <event_count> } }
async fn get_clock(
    client: &fetch_happen::Client,
    supabase_config: &SupabaseConfig,
    access_token: &str,
    user_id: &str,
) -> Result<Clock<String, String>, JsValue> {
    use serde_json::json;

    let SupabaseConfig {
        supabase_url,
        supabase_anon_key,
    } = supabase_config;

    let url = format!("{supabase_url}/rest/v1/rpc/get_clock");
    let body = json!({ "p_user_id": user_id });

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key)
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&body)
        .map_err(|e| JsValue::from_str(&format!("{e:?}")))?
        .send()
        .await
        .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "get_clock RPC failed with status: {}",
            resp.status()
        )));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

    let m: Clock<String, String> = serde_json::from_str(&text).map_err(|e| {
        JsValue::from_str(&format!(
            "Failed to parse get_clock response: {e}. Body: {text}"
        ))
    })?;

    Ok(m)
}

#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct SupabaseSyncResult {
    pub uploaded_to_supabase: usize,
    pub downloaded_from_supabase: usize,
}
