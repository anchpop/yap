//! Supabase event fetch/upload for the MCP server.
//!
//! Events are stored per `(user_id, stream_id, device_id)` with a monotonically
//! increasing `within_device_events_index`. This server owns the `yap-mcp`
//! device id, so it can append events without coordinating with other devices;
//! the web app picks them up on its next sync.
//!
//! **Single-writer requirement.** Because every yap-mcp process shares the one
//! `yap-mcp` device id, there must be exactly one process writing for a given
//! account at a time. Two concurrent processes each allocate the next index
//! from their own in-memory view, so they can assign the *same* index to
//! *different* events; the unique constraint
//! `(user_id, stream_id, device_id, within_device_events_index)` lets one win,
//! and the loser — since events are immutable and echo-confirmation matches
//! only on `(device, index)` — silently treats the winner's row as its own and
//! drops its event.
//!
//! The remote deploy holds to one instance: CI runs `flyctl deploy --ha=false`
//! (so a deploy never spins up a second machine) followed by
//! `flyctl scale count 1` (which destroys any extras). The realistic ways to
//! end up with two concurrent writers are a manual `fly scale count > 1` or two
//! stdio sessions for the same account. None of this is a *hard* guarantee —
//! that would need an exclusivity lease (e.g. a Postgres advisory lock) — but
//! the residual risk is a narrow window with low, self-correcting harm (the
//! loser drops one event; a review just comes due again), so we accept it.

use anyhow::{Context as _, bail};
use weapon::data_model::Timestamped;
use weapon::supabase::SyncableEvent;

/// The device id this server writes events under. Shared by every yap-mcp
/// process, so exactly one may write for an account at a time — see the
/// single-writer note in the module docs.
pub const DEVICE_ID: &str = "yap-mcp";

pub const REVIEWS_STREAM: &str = "reviews";
pub const DECK_SELECTION_STREAM: &str = "deck_selection";

/// Credentials for Supabase REST calls. The stdio server authenticates as the
/// service role; the remote server authenticates as the user themself (anon
/// apikey + the user's JWT), staying inside RLS.
#[derive(Clone)]
pub struct SupabaseAuth {
    pub url: String,
    /// Sent as the `apikey` header.
    pub apikey: String,
    /// Sent as the `Authorization: Bearer` token.
    pub bearer: String,
}

impl SupabaseAuth {
    pub fn service_role(url: String, service_role_key: String) -> Self {
        SupabaseAuth {
            url,
            apikey: service_role_key.clone(),
            bearer: service_role_key,
        }
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("apikey", &self.apikey)
            .header("Authorization", format!("Bearer {}", &self.bearer))
    }
}

/// One row from the `events` table.
#[derive(Clone)]
pub struct EventRow {
    pub stream_id: String,
    pub device_id: String,
    pub event: Timestamped<serde_json::Value>,
}

/// Look up a user's id by email via the Supabase admin API.
pub async fn find_user_id(
    client: &reqwest::Client,
    cfg: &SupabaseAuth,
    email: &str,
) -> anyhow::Result<String> {
    for page in 1..=50 {
        let resp: serde_json::Value = cfg
            .auth(client.get(format!(
                "{}/auth/v1/admin/users?page={page}&per_page=50",
                cfg.url
            )))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let users = resp["users"].as_array().context("expected users array")?;
        if users.is_empty() {
            break;
        }
        if let Some(user) = users.iter().find(|u| u["email"].as_str() == Some(email)) {
            return Ok(user["id"]
                .as_str()
                .context("user row missing id")?
                .to_string());
        }
    }
    bail!("no user found with email {email}")
}

/// Fetch event rows for a user, optionally only those with a global id greater
/// than `after`. Returns the rows and the highest global id seen.
pub async fn fetch_events(
    client: &reqwest::Client,
    cfg: &SupabaseAuth,
    user_id: &str,
    after: Option<i64>,
) -> anyhow::Result<(Vec<EventRow>, Option<i64>)> {
    let mut rows = Vec::new();
    let mut max_id = after;
    let page_size = 1000;
    let mut offset = 0;
    loop {
        let mut url = format!(
            "{}/rest/v1/events?user_id=eq.{user_id}&order=id.asc&limit={page_size}&offset={offset}",
            cfg.url
        );
        if let Some(after) = after {
            url.push_str(&format!("&id=gt.{after}"));
        }
        let page: Vec<serde_json::Value> = cfg
            .auth(client.get(url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        for row in &page {
            if let Some(id) = row["id"].as_i64() {
                max_id = Some(max_id.map_or(id, |m| m.max(id)));
            }
            let stream_id = row["stream_id"].as_str().unwrap_or("reviews").to_string();
            let device_id = row["device_id"].as_str().unwrap_or("unknown").to_string();
            // The event column is JSONB but old rows may hold a JSON string
            let event_value = match &row["event"] {
                serde_json::Value::String(s) => serde_json::from_str(s)
                    .with_context(|| format!("failed to parse stringified event: {s}"))?,
                v => v.clone(),
            };
            let event: Timestamped<serde_json::Value> = serde_json::from_value(event_value)
                .context("failed to parse event as Timestamped")?;
            rows.push(EventRow {
                stream_id,
                device_id,
                event,
            });
        }

        if page.len() < page_size {
            break;
        }
        offset += page_size;
    }
    Ok((rows, max_id))
}

/// Upload events (as returned by `StreamStore::jsons`) to the `events` table.
/// Uses the same row shape as the web app's sync (see `weapon::supabase`).
pub async fn upload_events(
    client: &reqwest::Client,
    cfg: &SupabaseAuth,
    user_id: &str,
    stream_id: &str,
    device_id: &str,
    events: &[Timestamped<serde_json::Value>],
) -> anyhow::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let payload: Vec<SyncableEvent> = events
        .iter()
        .map(|event| SyncableEvent {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            created_at: event.timestamp.to_string(),
            within_device_events_index: event.within_device_events_index,
            event: serde_json::to_value(event).expect("Timestamped<Value> serializes"),
            stream_id: stream_id.to_string(),
        })
        .collect();

    let resp = cfg
        .auth(client.post(format!("{}/rest/v1/events", cfg.url)))
        .header("Prefer", "return=minimal")
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "failed to upload {} event(s): {status} {body}",
            events.len()
        );
    }
    Ok(())
}
