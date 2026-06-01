use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap, HashSet};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .context("SUPABASE_SERVICE_ROLE_KEY env var required")?;
    let supabase_url = std::env::var("SUPABASE_URL")
        .unwrap_or_else(|_| "https://eearwzqotpfoderpfrqx.supabase.co".to_string());

    let client = reqwest::Client::new();

    let wau_since = chrono::Utc::now() - chrono::Duration::days(7);
    let wau = fetch_active_users(&client, &supabase_url, &service_role_key, wau_since).await?;
    println!(
        "Unique users with events in the last 7 days (since {}): {}",
        wau_since.format("%Y-%m-%dT%H:%M:%SZ"),
        wau.len()
    );

    let signups_since = chrono::Utc::now() - chrono::Duration::days(14);
    let signups =
        fetch_signups_since(&client, &supabase_url, &service_role_key, signups_since).await?;
    println!(
        "\nSignups in the last 14 days (since {}): {}",
        signups_since.format("%Y-%m-%d"),
        signups.len()
    );

    if !signups.is_empty() {
        let heard_about =
            fetch_heard_about_for_users(&client, &supabase_url, &service_role_key, &signups)
                .await?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for uid in &signups {
            let key = heard_about
                .get(uid)
                .cloned()
                .unwrap_or_else(|| "(no answer)".to_string());
            *counts.entry(key).or_insert(0) += 1;
        }
        println!("How they heard about Yap:");
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in sorted {
            println!("  {v:>4}  {k}");
        }
    }

    Ok(())
}

async fn fetch_active_users(
    client: &reqwest::Client,
    supabase_url: &str,
    key: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<HashSet<String>> {
    let since_str = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut users: HashSet<String> = HashSet::new();
    let page_size = 1000usize;
    let mut offset = 0usize;

    loop {
        let url = format!(
            "{supabase_url}/rest/v1/events?select=user_id&created_at=gte.{since_str}&order=id.asc&limit={page_size}&offset={offset}"
        );
        let page: Vec<serde_json::Value> = client
            .get(&url)
            .header("apikey", key)
            .header("Authorization", format!("Bearer {key}"))
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
    Ok(users)
}

async fn fetch_signups_since(
    client: &reqwest::Client,
    supabase_url: &str,
    key: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<String>> {
    let mut user_ids = Vec::new();
    let per_page = 1000usize;
    let mut page = 1usize;

    loop {
        let url = format!("{supabase_url}/auth/v1/admin/users?page={page}&per_page={per_page}");
        let resp: serde_json::Value = client
            .get(&url)
            .header("apikey", key)
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let users = resp["users"].as_array().cloned().unwrap_or_default();
        let count = users.len();
        for u in &users {
            let (Some(created), Some(id)) = (u["created_at"].as_str(), u["id"].as_str()) else {
                continue;
            };
            let Ok(created_dt) = created.parse::<chrono::DateTime<chrono::Utc>>() else {
                continue;
            };
            if created_dt >= since {
                user_ids.push(id.to_string());
            }
        }
        if count < per_page {
            break;
        }
        page += 1;
    }
    Ok(user_ids)
}

async fn fetch_heard_about_for_users(
    client: &reqwest::Client,
    supabase_url: &str,
    key: &str,
    user_ids: &[String],
) -> Result<HashMap<String, String>> {
    let mut result: HashMap<String, String> = HashMap::new();
    for chunk in user_ids.chunks(100) {
        let ids_csv = chunk.join(",");
        let page_size = 1000usize;
        let mut offset = 0usize;
        loop {
            let url = format!(
                "{supabase_url}/rest/v1/events?stream_id=eq.deck_selection&user_id=in.({ids_csv})&select=user_id,event&order=id.desc&limit={page_size}&offset={offset}"
            );
            let page: Vec<serde_json::Value> = client
                .get(&url)
                .header("apikey", key)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let count = page.len();
            for row in &page {
                let Some(uid) = row["user_id"].as_str() else {
                    continue;
                };
                if result.contains_key(uid) {
                    continue;
                }
                if let Some(ha) =
                    row["event"]["event"]["User"]["SetHeardAbout"]["heard_about"].as_str()
                {
                    result.insert(uid.to_string(), ha.to_string());
                }
            }
            if count < page_size {
                break;
            }
            offset += page_size;
        }
    }
    Ok(result)
}
