//! Reports active-user counts and a per-signup usage breakdown straight from
//! Supabase. Run with `cargo run -p user-metrics -- [days]`, where `days` is
//! the signup window (default 30), or pass `--summary-json` for aggregate-only
//! machine output.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Event kinds that represent the user actually studying, as opposed to
/// onboarding or deck bookkeeping.
const REVIEW_KINDS: [&str; 3] = [
    "ReviewCard",
    "TranslationChallenge",
    "TranscriptionChallenge",
];

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let argument = std::env::args().nth(1);
    let summary_json = argument.as_deref() == Some("--summary-json");
    let signup_days: i64 = match argument.filter(|arg| arg != "--summary-json") {
        Some(arg) => arg
            .parse()
            .context("signup window must be a number of days")?,
        None => 30,
    };

    let supabase = Supabase::from_env()?;
    let now = Utc::now();

    // One fetch covers both the activity windows and the per-signup usage.
    let window_days = signup_days.max(30);
    let window_start = now - Duration::days(window_days);
    let activity = supabase.fetch_activity_since(window_start).await?;
    let weekly_start = now - Duration::days(7);
    let weekly_active_users = activity
        .iter()
        .filter(|event| event.created_at >= weekly_start)
        .map(|event| event.user_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let signup_start = now - Duration::days(signup_days);
    let signups = supabase.fetch_signups_since(signup_start).await?;

    if summary_json {
        println!(
            "{}",
            serde_json::json!({
                "weeklyActiveUsers": weekly_active_users,
                "signupsPastMonth": signups.len(),
                "generatedAt": now.to_rfc3339(),
                "activityWindowStart": weekly_start.to_rfc3339(),
                "signupWindowStart": signup_start.to_rfc3339(),
            })
        );
        return Ok(());
    }

    println!("Active users (unique users with any event):");
    for days in [1, 7, 30] {
        let since = now - Duration::days(days);
        let users: HashSet<&str> = activity
            .iter()
            .filter(|e| e.created_at >= since)
            .map(|e| e.user_id.as_str())
            .collect();
        println!("  last {days:>2} day(s): {}", users.len());
    }

    println!(
        "\nSignups in the last {signup_days} days (since {}): {}",
        signup_start.format("%Y-%m-%d"),
        signups.len()
    );
    if signups.is_empty() {
        return Ok(());
    }

    let signup_ids: HashSet<&str> = signups.iter().map(|s| s.id.as_str()).collect();
    let onboarding = supabase.fetch_onboarding_since(signup_start).await?;

    let mut usage: HashMap<&str, Usage> = HashMap::new();
    for event in activity
        .iter()
        .filter(|e| signup_ids.contains(e.user_id.as_str()))
    {
        let entry = usage.entry(event.user_id.as_str()).or_default();
        entry.total += 1;
        if REVIEW_KINDS.contains(&event.kind.as_deref().unwrap_or_default()) {
            entry.reviews += 1;
        }
        entry.active_days.insert(event.created_at.date_naive());
        entry.last_active = entry.last_active.max(Some(event.created_at));
    }

    let mut heard_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for signup in &signups {
        let heard = onboarding
            .get(&signup.id)
            .and_then(|o| o.heard_about.as_deref())
            .unwrap_or("(no answer)");
        *heard_counts.entry(heard).or_default() += 1;
    }
    println!("\nHow they heard about Yap:");
    let mut sorted: Vec<_> = heard_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (source, count) in sorted {
        println!("  {count:>4}  {source}");
    }

    println!("\nPer-signup usage:");
    println!(
        "  {:<10}  {:<16}  {:<10}  {:>7}  {:>6}  {:>6}  {:<10}  email",
        "signed up", "heard about", "target", "reviews", "events", "days", "last seen"
    );
    let mut rows: Vec<_> = signups.iter().collect();
    rows.sort_by_key(|s| std::cmp::Reverse(usage.get(s.id.as_str()).map_or(0, |u| u.reviews)));
    for signup in rows {
        let usage = usage.get(signup.id.as_str()).cloned().unwrap_or_default();
        let onboarding = onboarding.get(&signup.id);
        println!(
            "  {:<10}  {:<16}  {:<10}  {:>7}  {:>6}  {:>6}  {:<10}  {}",
            signup.created_at.format("%Y-%m-%d"),
            onboarding
                .and_then(|o| o.heard_about.as_deref())
                .unwrap_or("-"),
            onboarding
                .and_then(|o| o.target_language.as_deref())
                .unwrap_or("-"),
            usage.reviews,
            usage.total,
            usage.active_days.len(),
            usage
                .last_active
                .map(|t| t.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "never".to_string()),
            signup.email.as_deref().unwrap_or("(no email)"),
        );
    }

    let never_reviewed = signups
        .iter()
        .filter(|s| usage.get(s.id.as_str()).is_none_or(|u| u.reviews == 0))
        .count();
    println!(
        "\n{never_reviewed} of {} signups never did a single review.",
        signups.len()
    );

    Ok(())
}

#[derive(Clone, Default)]
struct Usage {
    total: usize,
    reviews: usize,
    active_days: BTreeSet<NaiveDate>,
    last_active: Option<DateTime<Utc>>,
}

struct ActivityEvent {
    user_id: String,
    created_at: DateTime<Utc>,
    /// The `LanguageEventContent` variant, absent for onboarding events.
    kind: Option<String>,
}

struct Signup {
    id: String,
    email: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Default)]
struct Onboarding {
    heard_about: Option<String>,
    target_language: Option<String>,
}

struct Supabase {
    client: reqwest::Client,
    url: String,
    key: String,
}

impl Supabase {
    fn from_env() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            url: std::env::var("SUPABASE_URL")
                .unwrap_or_else(|_| "https://eearwzqotpfoderpfrqx.supabase.co".to_string()),
            key: std::env::var("SUPABASE_SERVICE_ROLE_KEY")
                .context("SUPABASE_SERVICE_ROLE_KEY env var required")?,
        })
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        Ok(self
            .client
            .get(format!("{}{path}", self.url))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Runs a PostgREST query, following pagination until the table is exhausted.
    async fn select(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        const PAGE_SIZE: usize = 1000;
        let mut rows = Vec::new();
        loop {
            let page = self
                .get(&format!(
                    "/rest/v1/{query}&order=id.asc&limit={PAGE_SIZE}&offset={}",
                    rows.len()
                ))
                .await?;
            let page = page.as_array().cloned().unwrap_or_default();
            let count = page.len();
            rows.extend(page);
            if count < PAGE_SIZE {
                return Ok(rows);
            }
        }
    }

    async fn fetch_activity_since(&self, since: DateTime<Utc>) -> Result<Vec<ActivityEvent>> {
        let rows = self
            .select(&format!(
                "events?select=user_id,created_at,kind:event->event->User->content->>type\
                 &created_at=gte.{}",
                since.format("%Y-%m-%dT%H:%M:%SZ")
            ))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(ActivityEvent {
                    user_id: row["user_id"].as_str()?.to_string(),
                    created_at: row["created_at"].as_str()?.parse().ok()?,
                    kind: row["kind"].as_str().map(str::to_string),
                })
            })
            .collect())
    }

    async fn fetch_signups_since(&self, since: DateTime<Utc>) -> Result<Vec<Signup>> {
        const PER_PAGE: usize = 1000;
        let mut signups = Vec::new();
        for page in 1.. {
            let resp = self
                .get(&format!(
                    "/auth/v1/admin/users?page={page}&per_page={PER_PAGE}"
                ))
                .await?;
            let users = resp["users"].as_array().cloned().unwrap_or_default();
            let count = users.len();
            for user in &users {
                let Some(created_at) = user["created_at"]
                    .as_str()
                    .and_then(|c| c.parse::<DateTime<Utc>>().ok())
                else {
                    continue;
                };
                let Some(id) = user["id"].as_str() else {
                    continue;
                };
                if created_at >= since {
                    signups.push(Signup {
                        id: id.to_string(),
                        email: user["email"].as_str().map(str::to_string),
                        created_at,
                    });
                }
            }
            if count < PER_PAGE {
                break;
            }
        }
        signups.sort_by_key(|s| s.created_at);
        Ok(signups)
    }

    /// Onboarding answers, keyed by user. Deck-selection events are written at
    /// signup, so bounding by the signup window loses nothing.
    async fn fetch_onboarding_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<HashMap<String, Onboarding>> {
        let rows = self
            .select(&format!(
                "events?stream_id=eq.deck_selection&select=user_id,event&created_at=gte.{}",
                since.format("%Y-%m-%dT%H:%M:%SZ")
            ))
            .await?;
        let mut onboarding: HashMap<String, Onboarding> = HashMap::new();
        for row in &rows {
            let Some(user_id) = row["user_id"].as_str() else {
                continue;
            };
            let event = &row["event"]["event"]["User"];
            let entry = onboarding.entry(user_id.to_string()).or_default();
            if let Some(heard) = event["SetHeardAbout"]["heard_about"].as_str() {
                entry.heard_about = Some(heard.to_string());
            }
            if let Some(target) = event["SelectBothLanguages"]["target"]
                .as_str()
                .or_else(|| event["SelectTargetLanguage"]["target"].as_str())
            {
                entry.target_language = Some(target.to_string());
            }
        }
        Ok(onboarding)
    }
}
