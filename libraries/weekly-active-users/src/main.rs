use anyhow::{Context, Result};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .context("SUPABASE_SERVICE_ROLE_KEY env var required")?;
    let supabase_url = std::env::var("SUPABASE_URL")
        .unwrap_or_else(|_| "https://eearwzqotpfoderpfrqx.supabase.co".to_string());

    let since = chrono::Utc::now() - chrono::Duration::days(7);
    let since_str = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let client = reqwest::Client::new();
    let mut users: HashSet<String> = HashSet::new();
    let page_size = 1000usize;
    let mut offset = 0usize;

    loop {
        let url = format!(
            "{supabase_url}/rest/v1/events?select=user_id&created_at=gte.{since_str}&order=id.asc&limit={page_size}&offset={offset}"
        );
        let page: Vec<serde_json::Value> = client
            .get(&url)
            .header("apikey", &service_role_key)
            .header("Authorization", format!("Bearer {service_role_key}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let count = page.len();
        for row in &page {
            if let Some(uid) = row["user_id"].as_str() {
                users.insert(uid.to_string());
            }
        }
        if count < page_size {
            break;
        }
        offset += page_size;
    }

    println!(
        "Unique users with events in the last 7 days (since {since_str}): {}",
        users.len()
    );
    Ok(())
}
