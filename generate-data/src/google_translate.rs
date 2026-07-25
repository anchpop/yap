use anyhow::Context;
use dashmap::DashMap;
use futures::StreamExt;
use gcp_auth::TokenProvider;
use html_escape::decode_html_entities;
use language_utils::Language;
use rand::RngExt;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use xxhash_rust::xxh3::xxh3_64;

/// The Translation LLM quota is counted in *requests* per minute, not characters
/// (`translation-llm` has no content quota), so packing many sentences into one
/// request is the way to make throughput. A request is bounded by two per-call
/// limits — at most 1024 strings, and at most 30,000 code points of content —
/// whichever is hit first. `MAX_REQUEST_CHARS` leaves margin under the 30k cap.
const MAX_REQUEST_CHARS: usize = 28_000;
const MAX_BATCH_STRINGS: usize = 64;

/// Estimated Translation LLM price, USD per million characters, billed on input
/// and output *separately*. Rates as published ~2026 — verify before trusting the
/// dollar figure; the cost display is explicitly an estimate.
const PRICE_PER_MILLION_INPUT: f64 = 10.0;
const PRICE_PER_MILLION_OUTPUT: f64 = 10.0;
/// How many batched requests to have in flight at once. The rate limiter still
/// gates the per-minute request count; this just decides how quickly the window
/// fills.
const BATCH_CONCURRENCY: usize = 8;
/// Retries for a single request on 429 / 5xx before giving up on it.
const MAX_RETRIES: u32 = 5;

/// Two-stage spend fuse for translation (the only significant paid API cost),
/// overridable via `TRANSLATE_BUDGET_USD` (the threshold) and
/// `TRANSLATE_BUDGET_PER_LANGUAGE_USD` (the fallback cap); set either to 0 to
/// disable that stage.
///
/// Below the threshold, translation runs unclamped — a legitimate bursty
/// workflow (e.g. warming a brand-new language pair from scratch, which can cost
/// well over the per-pair cap on its own) is expected and must not be throttled.
/// Once cumulative spend crosses the threshold, that's past normal bursts, so a
/// per-language-pair cap kicks in as a runaway guard. Only *paid* calls are
/// affected; cached lookups are always free. Scope is one process run (the
/// counter starts at 0 each invocation).
const DEFAULT_GLOBAL_THRESHOLD_USD: f64 = 20.0;
const DEFAULT_PER_LANGUAGE_BUDGET_USD: f64 = 10.0;

/// Process-wide translation spend in micro-USD (millionths of a dollar), summed
/// across every language pair, compared against the global threshold.
static TOTAL_SPENT_MICRO_USD: AtomicU64 = AtomicU64::new(0);

/// Sliding-window rate limiter: at most `max_requests` per `window`.
struct RateLimiter {
    max_requests: usize,
    window: Duration,
    timestamps: Mutex<VecDeque<Instant>>,
}

impl RateLimiter {
    fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            timestamps: Mutex::new(VecDeque::with_capacity(max_requests)),
        }
    }

    async fn acquire(&self) {
        loop {
            let sleep_for = {
                let mut ts = self.timestamps.lock().await;
                let now = Instant::now();
                while let Some(&front) = ts.front() {
                    if now.duration_since(front) >= self.window {
                        ts.pop_front();
                    } else {
                        break;
                    }
                }
                if ts.len() < self.max_requests {
                    ts.push_back(now);
                    return;
                }
                // Need to wait until the oldest entry exits the window.
                let oldest = *ts.front().unwrap();
                self.window - now.duration_since(oldest)
            };
            tokio::time::sleep(sleep_for).await;
        }
    }
}

pub struct GoogleTranslator {
    client: reqwest::Client,
    source_language: String,
    target_language: String,
    auth: Arc<dyn TokenProvider>,
    project_id: String,
    cache: DashMap<u64, String>, // hash -> translation
    cache_dir: PathBuf,
    master_cache_file: PathBuf,
    rate_limiter: RateLimiter,
    api_calls: AtomicU64,
    /// Billable characters actually sent to / received from the API (cache hits
    /// are free and not counted), for the cost estimate.
    input_chars: AtomicU64,
    output_chars: AtomicU64,
    /// Two-stage spend fuse in micro-USD; `0` disables that stage. Once process-wide
    /// spend ([`TOTAL_SPENT_MICRO_USD`]) crosses `global_threshold_micro`, this
    /// pair is clamped to `per_language_budget_micro`.
    global_threshold_micro: u64,
    per_language_budget_micro: u64,
    /// One-shot latch so the "cap reached" notice prints once per language pair.
    budget_warned: AtomicBool,
}

impl GoogleTranslator {
    pub async fn new(
        source_language: Language,
        target_language: Language,
        cache_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let creds_path = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
            .context("GOOGLE_APPLICATION_CREDENTIALS not set")?;
        let creds_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&creds_path)?)?;
        let project_id = creds_json["project_id"]
            .as_str()
            .context("No project_id in service account JSON")?
            .to_string();

        let auth = gcp_auth::provider()
            .await
            .context("Failed to initialize Google Cloud auth")?;

        std::fs::create_dir_all(&cache_dir)?;

        let master_cache_file = cache_dir.join("master_cache.json");
        let cache: DashMap<u64, String> = if master_cache_file.exists() {
            let master_content = std::fs::read_to_string(&master_cache_file)?;
            serde_json::from_str(&master_content).unwrap_or_default()
        } else {
            DashMap::new()
        };

        let rpm: usize = std::env::var("TRANSLATE_RPM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);

        let budget_micro = |var: &str, default: f64| -> u64 {
            let dollars = std::env::var(var)
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(default);
            (dollars.max(0.0) * 1_000_000.0) as u64
        };
        let global_threshold_micro =
            budget_micro("TRANSLATE_BUDGET_USD", DEFAULT_GLOBAL_THRESHOLD_USD);
        let per_language_budget_micro = budget_micro(
            "TRANSLATE_BUDGET_PER_LANGUAGE_USD",
            DEFAULT_PER_LANGUAGE_BUDGET_USD,
        );

        let res = Self {
            client: reqwest::Client::new(),
            source_language: source_language.iso_639_1().to_string(),
            target_language: target_language.iso_639_1().to_string(),
            auth,
            project_id,
            cache,
            cache_dir,
            master_cache_file,
            rate_limiter: RateLimiter::new(rpm, Duration::from_secs(60)),
            api_calls: AtomicU64::new(0),
            input_chars: AtomicU64::new(0),
            output_chars: AtomicU64::new(0),
            global_threshold_micro,
            per_language_budget_micro,
            budget_warned: AtomicBool::new(false),
        };
        res.consolidate_cache();
        Ok(res)
    }

    pub fn api_calls(&self) -> u64 {
        self.api_calls.load(Ordering::Relaxed)
    }

    /// Estimated USD spent on billable translations so far (cache hits excluded).
    /// An estimate: characters are counted as Unicode scalar values and the price
    /// constants may drift — see their definition.
    pub fn cost_estimate_usd(&self) -> f64 {
        self.spent_micro_usd() as f64 / 1_000_000.0
    }

    /// Micro-USD spent by *this* language pair so far.
    fn spent_micro_usd(&self) -> u64 {
        (self.input_chars.load(Ordering::Relaxed) as f64 * PRICE_PER_MILLION_INPUT
            + self.output_chars.load(Ordering::Relaxed) as f64 * PRICE_PER_MILLION_OUTPUT)
            as u64
    }

    /// Whether new *paid* translations should be skipped. Two-stage: while total
    /// process spend is below the global threshold, never — bursty single-pair
    /// warm-ups run freely. Once total spend crosses the threshold, this pair is
    /// clamped to its per-language cap. Cached lookups are never affected. Emits a
    /// one-time notice per pair when the clamp first trips it.
    fn over_budget(&self) -> bool {
        // Below the threshold (or threshold disabled): unclamped.
        let threshold_crossed = self.global_threshold_micro > 0
            && TOTAL_SPENT_MICRO_USD.load(Ordering::Relaxed) >= self.global_threshold_micro;
        if !threshold_crossed {
            return false;
        }
        // Past the threshold: fall back to the per-language cap (0 disables it).
        let over = self.per_language_budget_micro > 0
            && self.spent_micro_usd() >= self.per_language_budget_micro;
        if over && !self.budget_warned.swap(true, Ordering::Relaxed) {
            eprintln!(
                "⚠ Past the ~${:.0} translation-spend threshold (~${:.2} total); {}→{} hit its \
                 ~${:.2} per-pair fallback cap, skipping its further paid translations. Cached \
                 translations still work. Tune TRANSLATE_BUDGET_USD / \
                 TRANSLATE_BUDGET_PER_LANGUAGE_USD (0 disables either stage).",
                self.global_threshold_micro as f64 / 1_000_000.0,
                TOTAL_SPENT_MICRO_USD.load(Ordering::Relaxed) as f64 / 1_000_000.0,
                self.source_language,
                self.target_language,
                self.per_language_budget_micro as f64 / 1_000_000.0,
            );
        }
        over
    }

    async fn get_token(&self) -> anyhow::Result<String> {
        let token = self
            .auth
            .token(&["https://www.googleapis.com/auth/cloud-translation"])
            .await
            .context("Failed to get access token")?;
        Ok(token.as_str().to_string())
    }

    /// Cache key for a text under the current language pair.
    fn hash(&self, text: &str) -> u64 {
        let hash_input = format!("{}::{}::{text}", self.source_language, self.target_language);
        xxh3_64(hash_input.as_bytes())
    }

    /// Persist a single translation to the cache (in-memory + its own `{hash}.json`
    /// file, the unit the crash-safe cache is rebuilt from on next startup).
    async fn store(&self, hash: u64, translation: &str) {
        self.cache.insert(hash, translation.to_string());
        let cache_file = self.cache_dir.join(format!("{hash}.json"));
        let _ = tokio::fs::write(&cache_file, translation).await;
    }

    fn is_retryable(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }

    /// Exponential backoff with jitter: ~1s, 2s, 4s, 8s, 16s, capped, plus a
    /// random 0–500ms so concurrent batches retrying at once don't thunder.
    async fn backoff(attempt: u32) {
        let base = 1000u64 << attempt.min(4);
        let jitter = rand::rng().random_range(0..500);
        tokio::time::sleep(Duration::from_millis(base + jitter)).await;
    }

    /// Issue one `translateText` request for a batch of texts and return their
    /// translations positionally aligned with the input. The caller must keep the
    /// batch within the per-request limits (see the `MAX_*` constants). Retries on
    /// 429 / 5xx with backoff. Does not consult or write the cache — that is the
    /// caller's responsibility, so this stays a pure "translate these N strings".
    async fn translate_request(&self, texts: &[&str]) -> anyhow::Result<Vec<String>> {
        let url = format!(
            "https://translation.googleapis.com/v3/projects/{}/locations/us-central1:translateText",
            self.project_id
        );
        let model = format!(
            "projects/{}/locations/us-central1/models/general/translation-llm",
            self.project_id
        );
        let body_json = serde_json::json!({
            "sourceLanguageCode": self.source_language,
            "targetLanguageCode": self.target_language,
            "contents": texts,
            "mimeType": "text/plain",
            "model": model,
        });

        let mut attempt = 0u32;
        loop {
            // Each attempt is a real request against the per-minute quota.
            self.rate_limiter.acquire().await;
            self.api_calls.fetch_add(1, Ordering::Relaxed);

            let token = self.get_token().await?;
            let sent = self
                .client
                .post(&url)
                .bearer_auth(&token)
                .json(&body_json)
                .send()
                .await;

            let (status, body) = match sent {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp
                        .text()
                        .await
                        .context("Failed to read Google Translate response")?;
                    (status, body)
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        Self::backoff(attempt).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(e).context("Failed to call Google Translate API");
                }
            };

            if status.is_success() {
                let value: serde_json::Value = serde_json::from_str(&body)
                    .context("Failed to parse Google Translate response")?;
                let arr = value["translations"]
                    .as_array()
                    .context("Google Translate response had no translations array")?;
                anyhow::ensure!(
                    arr.len() == texts.len(),
                    "Google Translate returned {} translations for {} inputs",
                    arr.len(),
                    texts.len()
                );
                let out: Vec<String> = arr
                    .iter()
                    .map(|t| {
                        t["translatedText"]
                            .as_str()
                            .map(|s| decode_html_entities(s).to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                // Bill for what was actually sent and returned on this successful call.
                let input_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
                let output_chars: usize = out.iter().map(|t| t.chars().count()).sum();
                self.input_chars
                    .fetch_add(input_chars as u64, Ordering::Relaxed);
                self.output_chars
                    .fetch_add(output_chars as u64, Ordering::Relaxed);
                let micro = (input_chars as f64 * PRICE_PER_MILLION_INPUT
                    + output_chars as f64 * PRICE_PER_MILLION_OUTPUT)
                    as u64;
                TOTAL_SPENT_MICRO_USD.fetch_add(micro, Ordering::Relaxed);
                return Ok(out);
            }

            if Self::is_retryable(status) && attempt < MAX_RETRIES {
                eprintln!(
                    "Google Translate {status} (attempt {}/{MAX_RETRIES}, {} texts); backing off",
                    attempt + 1,
                    texts.len()
                );
                Self::backoff(attempt).await;
                attempt += 1;
                continue;
            }
            anyhow::bail!("Google Translate API error ({status}): {body}");
        }
    }

    pub async fn translate(&self, text: &str) -> anyhow::Result<String> {
        let hash = self.hash(text);

        // Check in-memory cache (includes master cache loaded on startup)
        if let Some(t) = self.cache.get(&hash) {
            return Ok(t.clone());
        }

        if generate_data::cache_only() {
            anyhow::bail!(
                "Google Translate cache miss for '{text}' ({}→{}); cache-only mode is enabled",
                self.source_language,
                self.target_language,
            );
        }

        if self.over_budget() {
            // Spend cap hit: skip the paid call and leave this sentence uncached.
            // An empty result means "no machine translation" to the caller (same
            // as a filtered-out translation) — deliberately not an error, so the
            // run finishes cleanly instead of logging one failure per sentence.
            return Ok(String::new());
        }

        let translation = self
            .translate_request(std::slice::from_ref(&text))
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();

        if translation.trim().is_empty() {
            // Don't cache empty/failed translations so they can be retried
            anyhow::bail!("Google Translate returned empty result for '{text}'");
        }

        self.store(hash, &translation).await;
        Ok(translation)
    }

    /// Warm the cache for many texts using batched requests, so that a subsequent
    /// per-sentence [`translate`](Self::translate) pass is all cache hits. Uncached
    /// texts are grouped into requests bounded by the per-request limits and
    /// translated concurrently — this is what turns the requests-per-minute quota
    /// into hundreds of sentences per request instead of one.
    ///
    /// Best-effort: a failed batch is logged, not fatal. Anything it leaves
    /// uncached (a failed batch, or an empty individual result) simply stays a
    /// miss, and the later per-sentence path will retry and report it as before.
    pub async fn prime(&self, texts: &[String]) {
        if generate_data::cache_only() {
            return;
        }

        // Unique, still-uncached texts in first-seen order.
        let mut seen = std::collections::HashSet::new();
        let mut pending: Vec<&str> = Vec::new();
        for t in texts {
            let hash = self.hash(t);
            if self.cache.contains_key(&hash) {
                continue;
            }
            if seen.insert(hash) {
                pending.push(t.as_str());
            }
        }
        if pending.is_empty() {
            return;
        }

        // Pack into batches within the per-request budget. A single text longer
        // than the budget still goes out alone (subtitle lines never approach it).
        let mut batches: Vec<Vec<&str>> = Vec::new();
        let mut cur: Vec<&str> = Vec::new();
        let mut cur_chars = 0usize;
        for &t in &pending {
            let len = t.chars().count();
            if !cur.is_empty()
                && (cur.len() >= MAX_BATCH_STRINGS || cur_chars + len > MAX_REQUEST_CHARS)
            {
                batches.push(std::mem::take(&mut cur));
                cur_chars = 0;
            }
            cur.push(t);
            cur_chars += len;
        }
        if !cur.is_empty() {
            batches.push(cur);
        }

        let pb = indicatif::ProgressBar::new(pending.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} priming translation cache ({per_sec}, {msg}, {eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("~$0.0000");
        pb.enable_steady_tick(Duration::from_millis(100));

        futures::stream::iter(batches.into_iter().map(|batch| {
            let pb = pb.clone();
            async move {
                if self.over_budget() {
                    // Cap hit: stop launching paid batches. Remaining texts stay
                    // uncached and get skipped by the per-sentence pass too.
                    pb.inc(batch.len() as u64);
                    return;
                }
                match self.translate_request(&batch).await {
                    Ok(translations) => {
                        for (text, t) in batch.iter().zip(translations) {
                            if !t.trim().is_empty() {
                                self.store(self.hash(text), &t).await;
                            }
                        }
                    }
                    Err(e) => eprintln!("Batch translate failed ({} texts): {e}", batch.len()),
                }
                pb.inc(batch.len() as u64);
                pb.set_message(format!("~${:.4}", self.cost_estimate_usd()));
            }
        }))
        .buffer_unordered(BATCH_CONCURRENCY)
        .collect::<Vec<()>>()
        .await;

        pb.finish_and_clear();
    }

    fn consolidate_cache(&self) {
        // Collect individual cache files to delete after consolidation
        let mut files_to_delete = Vec::new();

        // Scan the cache directory for individual cache files and merge them
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Skip if it's the master cache file or not a JSON file
                if path == self.master_cache_file
                    || path.extension().and_then(|s| s.to_str()) != Some("json")
                {
                    continue;
                }

                // Extract hash from filename
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str())
                    && let Ok(hash) = filename.parse::<u64>()
                {
                    // Read the translation from the file
                    if let Ok(translation) = std::fs::read_to_string(&path) {
                        // Add to consolidated cache if not already present
                        self.cache.entry(hash).or_insert_with(|| translation);
                        // Mark this file for deletion
                        files_to_delete.push(path);
                    }
                }
            }
        }

        // Skip rewriting the master cache if nothing changed
        if files_to_delete.is_empty() {
            return;
        }

        // Write the consolidated cache to the master file
        // Convert to BTreeMap for deterministic serialization order
        let sorted_cache: std::collections::BTreeMap<_, _> = self
            .cache
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();

        if let Ok(json) = serde_json::to_string_pretty(&sorted_cache)
            && std::fs::write(&self.master_cache_file, json).is_ok()
        {
            // Only delete individual files if the master cache was written successfully
            for file in files_to_delete {
                let _ = std::fs::remove_file(file);
            }
        }
    }
}

impl Drop for GoogleTranslator {
    fn drop(&mut self) {
        self.consolidate_cache();
    }
}
