//! Sentence translation with a pluggable backend behind one shared cache dir.
//!
//! Two backends exist, chosen in code (see the [`Translator::new`] call in
//! `main.rs`): the Google Cloud Translation API (`translation-llm` model) and
//! an OpenAI chat model via tysm (e.g. `gpt-5.6-luna`, which is ~50x cheaper
//! than Google's per-character rates).
//!
//! ## Cache keys and provenance
//!
//! Every translation lives in the shared osmo store under `translate/{hash}`,
//! where the hash input gives each model its own key so we always know (by
//! key) which system produced a cached translation:
//!
//! - `{src}::{tgt}::{text}` — the legacy Google key. All historical Google
//!   translations (NMT-era and `translation-llm`) live here, and the Google
//!   backend still reads/writes it.
//! - `{src}::{tgt}::{model}::{text}` — one key per OpenAI model.
//!
//! The OpenAI backend checks twice on lookup: its own model key first, then
//! the legacy Google key — so everything already translated is reused for
//! free, and only true misses are sent to the model. Legacy hits are *not*
//! copied under the model key; provenance stays truthful.
//!
//! The OpenAI backend deliberately does not use tysm's own response cache:
//! this cache is the single source of truth for translations.

use anyhow::Context;
use dashmap::DashMap;
use futures::StreamExt;
use gcp_auth::TokenProvider;
use html_escape::decode_html_entities;
use language_utils::Language;
use rand::RngExt;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tysm::chat_completions::ChatClient;
use xxhash_rust::xxh3::xxh3_64;

/// The Translation LLM quota is counted in *requests* per minute, not characters
/// (`translation-llm` has no content quota), so packing many sentences into one
/// request is the way to make throughput. A request is bounded by two per-call
/// limits — at most 1024 strings, and at most 30,000 code points of content —
/// whichever is hit first. `MAX_REQUEST_CHARS` leaves margin under the 30k cap.
/// Google-only; the OpenAI backend sends one sentence per request.
const MAX_REQUEST_CHARS: usize = 28_000;
const MAX_BATCH_STRINGS: usize = 64;

/// Estimated Translation LLM price, USD per million characters, billed on input
/// and output *separately*. Rates as published ~2026 — verify before trusting the
/// dollar figure; the cost display is explicitly an estimate. (The OpenAI
/// backend instead uses tysm's token-based price table.)
const PRICE_PER_MILLION_INPUT: f64 = 10.0;
const PRICE_PER_MILLION_OUTPUT: f64 = 10.0;
/// How many batched Google requests to have in flight at once. The rate
/// limiter still gates the per-minute request count; this just decides how
/// quickly the window fills.
const BATCH_CONCURRENCY: usize = 8;
/// Retries for a single live request on transient failures before giving up.
const MAX_RETRIES: u32 = 5;
/// Sentences per OpenAI Batch API job. Well under the 50k-requests-per-batch
/// cap, and keeps each job's enqueued-token footprint modest. Chunking is
/// deterministic (pending texts keep first-seen order), which matters because
/// tysm reattaches to an in-flight/completed batch whose request set hashes
/// the same — a killed run resumes its jobs instead of paying twice.
const OPENAI_BATCH_CHUNK: usize = 10_000;

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
///
/// Caveat: OpenAI *Batch API* spend is invisible to the fuse — tysm doesn't
/// report token usage for batch responses — so only Google and live OpenAI
/// calls count. Batch translation at luna prices is cents per course, so the
/// fuse guarding it matters much less than it does for Google.
const DEFAULT_GLOBAL_THRESHOLD_USD: f64 = 20.0;
const DEFAULT_PER_LANGUAGE_BUDGET_USD: f64 = 10.0;

/// Process-wide translation spend in micro-USD (millionths of a dollar), summed
/// across every language pair, compared against the global threshold.
static TOTAL_SPENT_MICRO_USD: AtomicU64 = AtomicU64::new(0);

/// Which translation system a [`Translator`] should use for cache misses.
#[derive(Debug, Clone)]
pub enum TranslationBackend {
    /// Google Cloud Translation (`translation-llm` model). Reads and writes
    /// the legacy model-agnostic cache key.
    // Currently unselected in main.rs, kept so it can be swapped back in.
    #[allow(dead_code)]
    Google,
    /// An OpenAI chat model (e.g. `"gpt-5.6-luna"`) via tysm, one sentence
    /// per request, bulk-warmed through the OpenAI Batch API. Reads its own
    /// model-specific cache key, falling back to the legacy Google key.
    OpenAi { model: String },
}

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

enum Backend {
    Google {
        client: reqwest::Client,
        auth: Arc<dyn TokenProvider>,
        project_id: String,
        rate_limiter: RateLimiter,
    },
    OpenAi {
        client: Box<ChatClient>,
        /// Cumulative `client.cost()` already folded into the spend counters,
        /// in micro-USD. tysm only exposes a running total per client, so each
        /// live or batch request syncs the delta under this lock.
        recorded_micro: std::sync::Mutex<u64>,
    },
}

/// The response shape requested from the OpenAI backend, one per sentence.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct TranslationResponse {
    /// The translated sentence, and nothing else.
    translation: String,
}

pub struct Translator {
    backend: Backend,
    source_language: Language,
    target_language: Language,
    /// ISO 639-1 codes; part of the cache keys, so these must stay stable.
    source_code: String,
    target_code: String,
    /// In-memory read-through layer over `store` (key hash → translation).
    cache: DashMap<u64, String>,
    /// The persistent cache, under `translate/{hash}` keys.
    store: osmo::Store,
    api_calls: AtomicU64,
    /// Estimated micro-USD spent by this language pair (cache hits are free
    /// and not counted).
    spent_micro: AtomicU64,
    /// Two-stage spend fuse in micro-USD; `0` disables that stage. Once process-wide
    /// spend ([`TOTAL_SPENT_MICRO_USD`]) crosses `global_threshold_micro`, this
    /// pair is clamped to `per_language_budget_micro`.
    global_threshold_micro: u64,
    per_language_budget_micro: u64,
    /// One-shot latch so the "cap reached" notice prints once per language pair.
    budget_warned: AtomicBool,
}

impl Translator {
    pub async fn new(
        source_language: Language,
        target_language: Language,
        store: osmo::Store,
        backend: TranslationBackend,
    ) -> anyhow::Result<Self> {
        let backend = match backend {
            TranslationBackend::Google => {
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

                let rpm: usize = std::env::var("TRANSLATE_RPM")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(200);

                Backend::Google {
                    client: reqwest::Client::new(),
                    auth,
                    project_id,
                    rate_limiter: RateLimiter::new(rpm, Duration::from_secs(60)),
                }
            }
            TranslationBackend::OpenAi { model } => {
                // No `with_cache_directory`: the translation cache below is
                // the only cache (cache-only mode is enforced by our own
                // guards, before any request is made).
                let client = ChatClient::from_env(&model)
                    .context("OPENAI_API_KEY not set for the OpenAI translation backend")?
                    .with_reasoning_effort("low");
                Backend::OpenAi {
                    client: Box::new(client),
                    recorded_micro: std::sync::Mutex::new(0),
                }
            }
        };

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

        Ok(Self {
            backend,
            source_language,
            target_language,
            source_code: source_language.iso_639_1().to_string(),
            target_code: target_language.iso_639_1().to_string(),
            cache: DashMap::new(),
            store,
            api_calls: AtomicU64::new(0),
            spent_micro: AtomicU64::new(0),
            global_threshold_micro,
            per_language_budget_micro,
            budget_warned: AtomicBool::new(false),
        })
    }

    pub fn api_calls(&self) -> u64 {
        self.api_calls.load(Ordering::Relaxed)
    }

    /// Estimated USD spent on billable translations so far (cache hits excluded).
    /// An estimate: Google spend is counted in Unicode scalar values against
    /// possibly-drifted price constants; live OpenAI spend comes from tysm's
    /// price table, including its discounted Batch API accounting.
    pub fn cost_estimate_usd(&self) -> f64 {
        self.spent_micro.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Record `micro` micro-USD of fresh spend against both the per-pair and
    /// process-wide counters.
    fn add_spend(&self, micro: u64) {
        self.spent_micro.fetch_add(micro, Ordering::Relaxed);
        TOTAL_SPENT_MICRO_USD.fetch_add(micro, Ordering::Relaxed);
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
            && self.spent_micro.load(Ordering::Relaxed) >= self.per_language_budget_micro;
        if over && !self.budget_warned.swap(true, Ordering::Relaxed) {
            eprintln!(
                "⚠ Past the ~${:.0} translation-spend threshold (~${:.2} total); {}→{} hit its \
                 ~${:.2} per-pair fallback cap, skipping its further paid translations. Cached \
                 translations still work. Tune TRANSLATE_BUDGET_USD / \
                 TRANSLATE_BUDGET_PER_LANGUAGE_USD (0 disables either stage).",
                self.global_threshold_micro as f64 / 1_000_000.0,
                TOTAL_SPENT_MICRO_USD.load(Ordering::Relaxed) as f64 / 1_000_000.0,
                self.source_code,
                self.target_code,
                self.per_language_budget_micro as f64 / 1_000_000.0,
            );
        }
        over
    }

    /// Hash of the legacy, model-agnostic cache key — the Google cache. All
    /// pre-existing translations live under this key, so it must never change.
    fn google_hash(&self, text: &str) -> u64 {
        let hash_input = format!("{}::{}::{text}", self.source_code, self.target_code);
        xxh3_64(hash_input.as_bytes())
    }

    /// Hash of the cache key the current backend writes to: the legacy key for
    /// Google, a model-qualified key for OpenAI.
    fn primary_hash(&self, text: &str) -> u64 {
        match &self.backend {
            Backend::Google { .. } => self.google_hash(text),
            Backend::OpenAi { client, .. } => {
                let hash_input = format!(
                    "{}::{}::{}::{text}",
                    self.source_code, self.target_code, client.model
                );
                xxh3_64(hash_input.as_bytes())
            }
        }
    }

    /// Look one hash up: the in-memory layer first, then the persistent store
    /// (populating the in-memory layer on a hit).
    async fn cached_by_hash(&self, hash: u64) -> Option<String> {
        if let Some(t) = self.cache.get(&hash) {
            return Some(t.clone());
        }
        let bytes = self.store.read(&format!("translate/{hash}")).await?;
        let t = String::from_utf8(bytes).ok()?;
        self.cache.insert(hash, t.clone());
        Some(t)
    }

    /// Look `text` up in the cache: the backend's own key first, then (for
    /// OpenAI) the legacy Google key. A legacy hit is returned as-is, not
    /// copied under the model key — the key records which system translated it.
    async fn cached(&self, text: &str) -> Option<String> {
        if let Some(t) = self.cached_by_hash(self.primary_hash(text)).await {
            return Some(t);
        }
        if matches!(self.backend, Backend::OpenAi { .. }) {
            return self.cached_by_hash(self.google_hash(text)).await;
        }
        None
    }

    /// Persist a single translation (in-memory + the store; the store's append
    /// log is what makes this crash-safe).
    async fn store(&self, hash: u64, translation: &str) {
        self.cache.insert(hash, translation.to_string());
        if let Err(e) = self
            .store
            .write(&format!("translate/{hash}"), translation.as_bytes())
            .await
        {
            eprintln!("translate: failed to persist cache entry {hash}: {e}");
        }
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

    /// Issue one Google `translateText` request for a batch of texts and return
    /// their translations positionally aligned with the input. The caller must
    /// keep the batch within the per-request limits (see the `MAX_*` constants).
    /// Retries on 429 / 5xx with backoff. Does not consult or write the cache —
    /// that is the caller's responsibility, so this stays a pure "translate
    /// these N strings".
    async fn google_request(&self, texts: &[&str]) -> anyhow::Result<Vec<String>> {
        let Backend::Google {
            client,
            auth,
            project_id,
            rate_limiter,
        } = &self.backend
        else {
            unreachable!("google_request called on a non-Google backend");
        };
        let url = format!(
            "https://translation.googleapis.com/v3/projects/{project_id}/locations/us-central1:translateText"
        );
        let model =
            format!("projects/{project_id}/locations/us-central1/models/general/translation-llm");
        let body_json = serde_json::json!({
            "sourceLanguageCode": self.source_code,
            "targetLanguageCode": self.target_code,
            "contents": texts,
            "mimeType": "text/plain",
            "model": model,
        });

        let mut attempt = 0u32;
        loop {
            // Each attempt is a real request against the per-minute quota.
            rate_limiter.acquire().await;
            self.api_calls.fetch_add(1, Ordering::Relaxed);

            let token = auth
                .token(&["https://www.googleapis.com/auth/cloud-translation"])
                .await
                .context("Failed to get access token")?;
            let sent = client
                .post(&url)
                .bearer_auth(token.as_str())
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
                let micro = (input_chars as f64 * PRICE_PER_MILLION_INPUT
                    + output_chars as f64 * PRICE_PER_MILLION_OUTPUT)
                    as u64;
                self.add_spend(micro);
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

    /// The system prompt for the OpenAI backend. Shared by the live and Batch
    /// API paths so the two produce interchangeable results (and so tysm's
    /// batch-reuse hash stays stable across code paths).
    fn openai_system_prompt(&self) -> String {
        format!(
            "You are translating sentences for a language-learning app. Translate the {} \
             sentence the user sends into natural, idiomatic {}. Output only the translated \
             text — no quotes, no commentary.",
            self.source_language, self.target_language,
        )
    }

    /// Translate one sentence with a live (non-batch) OpenAI request, retrying
    /// transient failures. Used for the stragglers the Batch API pass missed.
    async fn openai_request_one(&self, text: &str) -> anyhow::Result<String> {
        let Backend::OpenAi {
            client,
            recorded_micro,
        } = &self.backend
        else {
            unreachable!("openai_request_one called on a non-OpenAI backend");
        };
        let system_prompt = self.openai_system_prompt();

        let mut attempt = 0u32;
        loop {
            self.api_calls.fetch_add(1, Ordering::Relaxed);
            let result: Result<TranslationResponse, _> =
                client.chat_with_system_prompt(&system_prompt, text).await;

            // Fold whatever this attempt cost into the spend counters, success
            // or not. tysm only exposes a running per-client total, so record
            // the delta since the last sync.
            if let Some(cost) = client.cost() {
                let total_micro = (cost * 1_000_000.0) as u64;
                let mut recorded = recorded_micro.lock().unwrap();
                if total_micro > *recorded {
                    self.add_spend(total_micro - *recorded);
                    *recorded = total_micro;
                }
            }

            match result {
                Ok(response) => return Ok(response.translation),
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        eprintln!(
                            "{} translation error (attempt {}/{MAX_RETRIES}): {e}; backing off",
                            client.model,
                            attempt + 1,
                        );
                        Self::backoff(attempt).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(e).with_context(|| {
                        format!("{} translation failed after retries", client.model)
                    });
                }
            }
        }
    }

    pub async fn translate(&self, text: &str) -> anyhow::Result<String> {
        if let Some(t) = self.cached(text).await {
            return Ok(t);
        }

        if generate_data::cache_only() {
            anyhow::bail!(
                "Translation cache miss for '{text}' ({}→{}); cache-only mode is enabled",
                self.source_code,
                self.target_code,
            );
        }

        if self.over_budget() {
            // Spend cap hit: skip the paid call and leave this sentence uncached.
            // An empty result means "no machine translation" to the caller (same
            // as a filtered-out translation) — deliberately not an error, so the
            // run finishes cleanly instead of logging one failure per sentence.
            return Ok(String::new());
        }

        let translation = match &self.backend {
            Backend::Google { .. } => self
                .google_request(std::slice::from_ref(&text))
                .await?
                .into_iter()
                .next()
                .unwrap_or_default(),
            Backend::OpenAi { .. } => self.openai_request_one(text).await?,
        };

        if translation.trim().is_empty() {
            // Don't cache empty/failed translations so they can be retried
            anyhow::bail!("Translation backend returned empty result for '{text}'");
        }

        self.store(self.primary_hash(text), &translation).await;
        Ok(translation)
    }

    /// Warm the cache for many texts in bulk, so that a subsequent per-sentence
    /// [`translate`](Self::translate) pass is (almost) all cache hits.
    ///
    /// Google: uncached texts are packed into multi-sentence requests (the
    /// quota is requests-per-minute) and sent concurrently. OpenAI: each
    /// uncached sentence becomes its own request in an OpenAI Batch API job —
    /// half price, no alignment ambiguity, but results can take a while
    /// (usually minutes, worst-case 24h per job; tysm polls until done and
    /// reattaches to matching in-flight jobs after a restart).
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
            if self.cached(t).await.is_some() {
                continue;
            }
            if seen.insert(self.primary_hash(t)) {
                pending.push(t.as_str());
            }
        }
        if pending.is_empty() {
            return;
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

        match &self.backend {
            Backend::Google { .. } => self.prime_google(pending, &pb).await,
            Backend::OpenAi { .. } => self.prime_openai(pending, &pb).await,
        }

        pb.finish_and_clear();
    }

    async fn prime_google(&self, pending: Vec<&str>, pb: &indicatif::ProgressBar) {
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

        futures::stream::iter(batches.into_iter().map(|batch| {
            let pb = pb.clone();
            async move {
                if self.over_budget() {
                    // Cap hit: stop launching paid batches. Remaining texts stay
                    // uncached and get skipped by the per-sentence pass too.
                    pb.inc(batch.len() as u64);
                    return;
                }
                match self.google_request(&batch).await {
                    Ok(translations) => {
                        for (text, t) in batch.iter().zip(translations) {
                            if !t.trim().is_empty() {
                                self.store(self.primary_hash(text), &t).await;
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
    }

    async fn prime_openai(&self, pending: Vec<&str>, pb: &indicatif::ProgressBar) {
        let Backend::OpenAi {
            client,
            recorded_micro,
        } = &self.backend
        else {
            unreachable!("prime_openai called on a non-OpenAI backend");
        };
        let system_prompt = self.openai_system_prompt();
        pb.set_message(format!(
            "{} Batch API, ~${:.4}",
            client.model,
            self.cost_estimate_usd()
        ));

        // Jobs run sequentially: each one is already maximally parallel on
        // OpenAI's side, and sequential submission keeps the enqueued-token
        // footprint bounded.
        for chunk in pending.chunks(OPENAI_BATCH_CHUNK) {
            let chunk_start = pb.position();
            if self.over_budget() {
                pb.inc(chunk.len() as u64);
                continue;
            }
            self.api_calls
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            match client
                .batch_chat_with_system_prompt::<TranslationResponse>(
                    &system_prompt,
                    chunk.to_vec(),
                    |batch| {
                        generate_data::report_batch_progress(pb, chunk_start, chunk.len(), batch)
                    },
                )
                .await
            {
                Ok(results) => {
                    for (text, result) in chunk.iter().zip(results) {
                        match result {
                            Ok(r) if !r.translation.trim().is_empty() => {
                                self.store(self.primary_hash(text), &r.translation).await;
                            }
                            // Empty translations stay uncached so the live
                            // pass can retry them.
                            Ok(_) => {}
                            Err(e) => eprintln!("Batch item failed for '{text}': {e}"),
                        }
                    }
                    pb.set_position(chunk_start + chunk.len() as u64);
                }
                Err(e) => {
                    eprintln!("OpenAI batch translate failed ({} texts): {e}", chunk.len());
                    pb.set_position(chunk_start + chunk.len() as u64);
                }
            }

            if let Some(cost) = client.cost() {
                let total_micro = (cost * 1_000_000.0) as u64;
                let mut recorded = recorded_micro.lock().unwrap();
                if total_micro > *recorded {
                    self.add_spend(total_micro - *recorded);
                    *recorded = total_micro;
                }
            }
            pb.set_message(format!(
                "{} Batch API, ~${:.4}",
                client.model,
                self.cost_estimate_usd()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMOKE_MODEL: &str = "gpt-5.6-luna";

    async fn smoke_translator(cache_dir: &std::path::Path) -> Option<Translator> {
        dotenvy::dotenv().ok();
        if std::env::var("OPENAI_API_KEY").is_err() {
            eprintln!("OPENAI_API_KEY not set; skipping");
            return None;
        }
        Some(
            Translator::new(
                Language::French,
                Language::English,
                osmo::Store::open(cache_dir),
                TranslationBackend::OpenAi {
                    model: SMOKE_MODEL.to_string(),
                },
            )
            .await
            .unwrap(),
        )
    }

    fn scratch_cache_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yap-translate-{name}-{}", std::process::id()))
    }

    /// Offline: an OpenAI-backed translator must fall back to the legacy
    /// Google cache key ("check twice"), without copying the hit under its
    /// own model key or touching the network.
    #[tokio::test]
    async fn openai_falls_back_to_legacy_google_cache() {
        let cache_dir = scratch_cache_dir("fallback");
        let Some(translator) = smoke_translator(&cache_dir).await else {
            return;
        };

        let text = "Une phrase qui n'existe que dans ce test.";
        translator
            .store(translator.google_hash(text), "cached by google, once")
            .await;

        let got = translator.translate(text).await.unwrap();
        assert_eq!(got, "cached by google, once");
        assert_eq!(translator.api_calls(), 0, "must not hit the network");
        assert!(
            !translator
                .cache
                .contains_key(&translator.primary_hash(text)),
            "legacy hit must not be copied under the model key"
        );

        drop(translator);
        let _ = std::fs::remove_dir_all(cache_dir);
    }

    /// Live smoke test for the OpenAI live (non-batch) path — hits the real
    /// API (well under a cent). Run with:
    ///
    /// ```sh
    /// cargo test -p generate-data --bin generate-data -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "hits the live OpenAI API"]
    async fn openai_live_smoke() {
        let cache_dir = scratch_cache_dir("live");
        let Some(translator) = smoke_translator(&cache_dir).await else {
            panic!("OPENAI_API_KEY required for this test");
        };

        // Single-sentence live path, then a cache hit for the same text.
        let single = translator.translate("Le chien dort.").await.unwrap();
        println!("single: {single}");
        assert!(single.to_lowercase().contains("dog"));
        let calls_before = translator.api_calls();
        let cached = translator.translate("Le chien dort.").await.unwrap();
        assert_eq!(single, cached);
        assert_eq!(
            translator.api_calls(),
            calls_before,
            "second call must be a cache hit"
        );

        println!("estimated cost: ${:.6}", translator.cost_estimate_usd());
        assert!(
            translator.cost_estimate_usd() > 0.0,
            "spend tracking recorded nothing"
        );

        drop(translator);
        let _ = std::fs::remove_dir_all(cache_dir);
    }

    /// Live smoke test for the OpenAI Batch API path used by `prime`. Cheap,
    /// but can take minutes — the Batch API has no latency guarantee below 24h.
    #[tokio::test]
    #[ignore = "hits the live OpenAI Batch API; latency is minutes+"]
    async fn openai_batch_smoke() {
        let cache_dir = scratch_cache_dir("batch");
        let Some(translator) = smoke_translator(&cache_dir).await else {
            panic!("OPENAI_API_KEY required for this test");
        };

        let sentences: Vec<String> = [
            "Bonjour, comment ça va ?",
            "J'ai deux chats.",
            "Il pleut aujourd'hui.",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        translator.prime(&sentences).await;

        // Everything primed must now be served from cache, no live calls.
        let calls_after_prime = translator.api_calls();
        for s in &sentences {
            let t = translator.translate(s).await.unwrap();
            println!("{s} → {t}");
            assert!(!t.trim().is_empty());
        }
        assert_eq!(
            translator.api_calls(),
            calls_after_prime,
            "post-prime translations must all be cache hits"
        );

        drop(translator);
        let _ = std::fs::remove_dir_all(cache_dir);
    }
}
