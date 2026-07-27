//! The yap MCP server: state management and tool implementations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use chrono::Utc;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use language_utils::{
    Course, Gram, Language, SpurGram, autograde, dictionary_entry_slug,
    language_pack::LanguagePack, text_cleanup::find_closest_match,
};
use lasso::Spur;
use weapon::data_model::{EventStore, EventType};
use yap_frontend_rs::{
    CardContext, CardIndicator, CardSummary, Challenge, Context, Deck, DeckEvent, LanguageEvent,
    LanguageEventContent, Rating, TranslateComprehensibleSentence, autograde_translation,
    dictionary::{GramDictionaryDefinition, GramDictionaryEntry},
    translation_is_perfect,
};

use crate::deck::{PackCache, build_deck, detect_course, insert_rows, new_store};
use crate::output::*;
use crate::sync::{
    DEVICE_ID, REVIEWS_STREAM, SupabaseAuth, fetch_events, find_user_id, upload_events,
};

/// How long fetched events stay fresh before a tool call triggers a re-fetch.
const REFRESH_INTERVAL: Duration = Duration::from_secs(20);

pub struct Config {
    pub supabase: SupabaseAuth,
    pub email: String,
    pub out_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let email = std::env::var("YAP_USER_EMAIL")
            .context("YAP_USER_EMAIL env var required (the account to operate on)")?;
        let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
            .context("SUPABASE_SERVICE_ROLE_KEY env var required (in the repo .env)")?;
        let url = std::env::var("SUPABASE_URL")
            .unwrap_or_else(|_| "https://eearwzqotpfoderpfrqx.supabase.co".to_string());
        let out_dir = std::env::var("YAP_OUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../out")));
        Ok(Config {
            supabase: SupabaseAuth::service_role(url, service_role_key),
            email,
            out_dir,
        })
    }
}

/// Whether a recorded review event reached the server or is still pending a
/// retry. Either way the event is recorded in the local working store;
/// `PendingSync` just means the server copy will catch up on a later call.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncStatus {
    Synced,
    PendingSync,
}

/// Result of an idempotency-token lookup.
enum Replay {
    /// Seen before for this presentation — replay the cached response.
    Hit(CallToolResult),
    /// Seen before for a *different* presentation — reject (one nonce = one
    /// operation).
    IdentityConflict,
    /// Not seen — record normally.
    Miss,
}

/// How many recent idempotency tokens to remember. A review session is a
/// few dozen actions; this is a generous ceiling before the oldest are
/// dropped wholesale.
const IDEMPOTENCY_CACHE_MAX: usize = 256;

/// Returned when a nonce is reused for a different card/sentence — the real
/// widget never does this (each presentation mints its own nonce).
const IDEMPOTENCY_CONFLICT_MSG: &str = "that idempotency token was already used for a different card — pass the token from the \
     current challenge, or omit it";

pub struct YapState {
    supabase: SupabaseAuth,
    http: reqwest::Client,
    user_id: String,
    context: Context,
    store: EventStore<String, String>,
    last_fetched_id: Option<i64>,
    last_fetch: Instant,
    /// Set when an upload failed, to make the next sync fetch regardless of
    /// the throttle: a re-fetch echo-confirms any event whose ack we lost and
    /// advances uploaded_count past it, so we stop re-sending a committed
    /// index (which the unique constraint would reject).
    force_fetch: bool,
    deck: Option<Deck>,
    /// How many of our device's reviews events the server has confirmed received.
    uploaded_count: usize,
    /// Responses already served for a given idempotency token, so a retried
    /// review (widget resubmit, or a call whose response was lost in transit
    /// after the event was recorded) replays the original outcome instead of
    /// recording a second event. The token is a per-presentation nonce; each
    /// entry is bound to the presentation's immutable identity (card /
    /// language / sentence), so a retry — even with an edited answer — replays
    /// that one review, while a nonce paired with a *different* presentation
    /// records the real request instead of replaying a stale one.
    idempotency_cache: HashMap<String, (String, CallToolResult)>,
}

impl YapState {
    /// stdio mode: resolve the account by email using the service role key.
    pub async fn load(config: Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::new();
        log::info!("looking up user {}", config.email);
        let user_id = find_user_id(&http, &config.supabase, &config.email).await?;
        let packs = PackCache::new(config.out_dir);
        Self::for_user(config.supabase, user_id, &packs).await
    }

    /// Fetch a user's events, detect their course, and replay their deck.
    pub async fn for_user(
        supabase: SupabaseAuth,
        user_id: String,
        packs: &PackCache,
    ) -> anyhow::Result<Self> {
        let http = reqwest::Client::new();

        log::info!("fetching events for {user_id}");
        let (rows, last_fetched_id) = fetch_events(&http, &supabase, &user_id, None).await?;
        log::info!("fetched {} events", rows.len());

        let mut store = new_store();
        insert_rows(&mut store, rows);

        let course = detect_course(&store)?;
        log::info!(
            "detected course: {} for {} speakers",
            course.target_language,
            course.native_language
        );

        let language_pack = packs.get(&course).await?;
        let timezone = *chrono::Local::now().offset();
        let context = Context {
            language_pack,
            course,
            timezone,
        };

        let mut state = YapState {
            supabase,
            http,
            user_id,
            context,
            store,
            last_fetched_id,
            last_fetch: Instant::now(),
            force_fetch: false,
            deck: None,
            uploaded_count: 0,
            idempotency_cache: HashMap::new(),
        };
        // Everything fetched is by definition already on the server.
        state.uploaded_count = state.my_reviews_len();
        let deck = state.deck();
        log::info!(
            "deck ready: {} cards, {} reviews",
            deck.num_cards_added(),
            deck.stats().total_reviews
        );
        Ok(state)
    }

    /// Swap in the bearer token from the current request (remote mode: user
    /// access tokens rotate on refresh).
    pub fn set_bearer(&mut self, bearer: String) {
        self.supabase.bearer = bearer;
    }

    fn pack(&self) -> &LanguagePack {
        &self.context.language_pack
    }

    fn deck(&mut self) -> &Deck {
        if self.deck.is_none() {
            self.deck = Some(build_deck(&self.store, &self.context));
        }
        self.deck.as_ref().expect("just built")
    }

    /// How many reviews-stream events exist under our device id.
    fn my_reviews_len(&self) -> usize {
        self.store
            .get::<EventType<DeckEvent>>(REVIEWS_STREAM.to_string())
            .map(|stream| stream.len_device(&DEVICE_ID.to_string()))
            .unwrap_or(0)
    }

    /// Pull down events written by other devices since our last fetch, and
    /// flush any locally-recorded reviews the server hasn't confirmed yet.
    /// Called at the top of every tool handler (including read-only ones),
    /// since a pending upload should reach Supabase on the user's next
    /// action rather than waiting for their next write.
    async fn sync(&mut self) -> anyhow::Result<()> {
        if self.force_fetch || self.last_fetch.elapsed() >= REFRESH_INTERVAL {
            let (new_rows, max_id) = fetch_events(
                &self.http,
                &self.supabase,
                &self.user_id,
                self.last_fetched_id,
            )
            .await?;
            self.last_fetch = Instant::now();
            self.force_fetch = false;
            self.last_fetched_id = max_id.or(self.last_fetched_id);

            // Echoes of our own uploads confirm the server has them (the store
            // itself dedups the events). This also advances uploaded_count past
            // any event whose upload ack we lost, so the flush below won't
            // pointlessly re-send an index the server already has.
            for row in &new_rows {
                if row.device_id == DEVICE_ID && row.stream_id == REVIEWS_STREAM {
                    self.uploaded_count = self
                        .uploaded_count
                        .max(row.event.within_device_events_index + 1);
                }
            }
            if insert_rows(&mut self.store, new_rows) > 0 {
                self.deck = None;
            }
        }

        // Retry any locally-recorded reviews the server hasn't confirmed yet,
        // so a review whose upload failed still reaches Supabase on the user's
        // next action (even a read-only one) rather than waiting for their
        // next write.
        self.flush_pending_uploads().await;
        Ok(())
    }

    /// Record a review event locally and attempt to sync it.
    ///
    /// The local event store is the working source of truth (this mirrors the
    /// app's offline-first model), so recording always succeeds and the
    /// upload is best-effort: a failure leaves the event pending for the next
    /// `flush_pending_uploads` to retry rather than surfacing an error. That
    /// matters because the review widgets let the user retry on error — if
    /// this returned `Err`, a retry would record a *second* event (a fresh
    /// per-device index), double-reviewing the card. Returning
    /// [`SyncStatus`] instead means the widget always sees success and never
    /// retries; the pending event self-heals on the next call.
    ///
    /// Caveat: this store is in-memory (rebuilt from Supabase on start), so a
    /// pending event is lost if the process exits or the per-user state is
    /// evicted before a later call flushes it — a rare, low-harm edge (the
    /// card simply comes due again) that the old error-returning path shared,
    /// minus the double-record.
    async fn append_event(&mut self, content: LanguageEventContent) -> SyncStatus {
        let deck_event = DeckEvent::Language(LanguageEvent {
            target_language: self.context.course.target_language,
            native_language: self.context.course.native_language,
            content,
        });
        self.store.add_raw_event(
            REVIEWS_STREAM.to_string(),
            DEVICE_ID.to_string(),
            deck_event,
            None,
            self.context.timezone,
        );
        self.deck = None;
        self.flush_pending_uploads().await
    }

    /// Upload every locally-recorded event the server hasn't confirmed yet,
    /// in order so the per-device index sequence never has gaps. Best-effort:
    /// on failure the events stay pending (uploaded_count isn't advanced) and
    /// the next call retries them. A committed index that we think is still
    /// pending (a lost upload ack) would be rejected by the unique constraint
    /// on re-send, so a failure sets `force_fetch`: the next sync fetches,
    /// echo-confirms that index, and advances past it before we'd re-send.
    async fn flush_pending_uploads(&mut self) -> SyncStatus {
        let pending = self
            .store
            .get_raw(REVIEWS_STREAM.to_string())
            .expect("reviews stream registered in new_store")
            .jsons(&DEVICE_ID.to_string(), self.uploaded_count);
        if pending.is_empty() {
            return SyncStatus::Synced;
        }
        match upload_events(
            &self.http,
            &self.supabase,
            &self.user_id,
            REVIEWS_STREAM,
            DEVICE_ID,
            &pending,
        )
        .await
        {
            Ok(()) => {
                self.uploaded_count += pending.len();
                SyncStatus::Synced
            }
            Err(e) => {
                log::warn!(
                    "upload failed, {} review event(s) recorded locally and pending retry: {e:#}",
                    pending.len()
                );
                // Force the next sync to fetch: if this failure was a lost
                // ack (the server actually has the event), the echo-confirm
                // there trims it from pending so we stop hitting the unique
                // constraint on re-send.
                self.force_fetch = true;
                SyncStatus::PendingSync
            }
        }
    }

    /// Look up a prior response for this tool's idempotency token. A nonce
    /// identifies exactly one presentation, so a hit must also match that
    /// presentation's identity; a nonce seen with a *different* identity is a
    /// conflict (a misbehaving client — the real widget always pairs a nonce
    /// with its own challenge). Tokens are per-account (this state) and
    /// per-server-lifetime — the retry window we care about is a live widget
    /// (seconds to minutes), so a token lost to restart or the 256-entry
    /// bound just misses and records normally.
    fn replay_idempotent(&self, tool: &str, token: Option<&str>, identity: &str) -> Replay {
        let Some(token) = token else {
            return Replay::Miss;
        };
        match self.idempotency_cache.get(&idempotency_key(tool, token)) {
            Some((id, result)) if id == identity => Replay::Hit(result.clone()),
            Some(_) => Replay::IdentityConflict,
            None => Replay::Miss,
        }
    }

    /// Remember a successful response under its idempotency token and
    /// presentation identity so a later retry replays it. Errors aren't
    /// cached — a retry should be free to succeed. Keyed by tool so a token
    /// can't cross tools.
    fn remember_idempotent(
        &mut self,
        tool: &str,
        token: Option<&str>,
        identity: &str,
        result: &CallToolResult,
    ) {
        let Some(token) = token else { return };
        if result.is_error.unwrap_or(false) {
            return;
        }
        if self.idempotency_cache.len() >= IDEMPOTENCY_CACHE_MAX {
            self.idempotency_cache.clear();
        }
        self.idempotency_cache.insert(
            idempotency_key(tool, token),
            (identity.to_string(), result.clone()),
        );
    }

    /// The course's target language, serialized the way tool outputs carry it.
    fn target_language_value(&self) -> serde_json::Value {
        serde_json::to_value(self.context.course.target_language).expect("Language serializes")
    }

    /// Reject references whose language doesn't match the course this server
    /// is connected to.
    fn check_language(&self, language: &str) -> Result<(), String> {
        let expected = self.context.course.target_language;
        let parsed: Option<Language> =
            serde_json::from_value(serde_json::Value::String(language.to_string())).ok();
        if parsed != Some(expected) {
            return Err(format!(
                "this server is connected to a {expected} course, but got language '{language}'. \
                 Pass the language exactly as returned by search_dictionary or get_due_cards."
            ));
        }
        Ok(())
    }

    /// Parse a gram (the word/lemma/part-of-speech token sequence that uniquely
    /// identifies a dictionary entry) from tool input, requiring it to name a
    /// gram that actually exists in this course — the server never guesses.
    fn parse_gram(&self, value: &serde_json::Value) -> Result<(Gram<String>, SpurGram), String> {
        let gram: Gram<String> = serde_json::from_value(value.clone()).map_err(|e| {
            format!(
                "invalid gram (pass it exactly as returned by search_dictionary or get_due_cards): {e}"
            )
        })?;
        match self.pack().course_gram(&gram) {
            Some(interned) => Ok((gram, interned)),
            None => Err(format!(
                "no dictionary entry in this course matches that gram for '{}' — the full \
                 word/lemma/part-of-speech sequence must match exactly. Use search_dictionary \
                 to find real entries.",
                gram.to_display_string(self.context.course.target_language)
            )),
        }
    }

    /// Parse a card from tool input (the exact JSON returned by get_due_cards).
    fn parse_card(
        &self,
        value: &serde_json::Value,
    ) -> Result<CardIndicator<Gram<String>, String>, String> {
        serde_json::from_value(value.clone()).map_err(|e| {
            format!("invalid card (pass it exactly as returned by get_due_cards): {e}")
        })
    }

    /// Resolve a written-gram card plus an optional explicit sentence into
    /// the full translation-challenge payload. Explicit sentences are
    /// verified against the corpus (a model- or widget-supplied sentence is
    /// only accepted verbatim); omitted ones use the app's own selection.
    fn translation_challenge_payload(
        &mut self,
        card: &CardIndicator<Gram<String>, String>,
        sentence: Option<&str>,
    ) -> Result<(Spur, TranslateComprehensibleSentence), String> {
        let CardIndicator::WrittenGram { gram } = card else {
            return Err(
                "translation challenges quiz written gram cards — present listening and \
                 pronunciation cards with present_card instead"
                    .to_string(),
            );
        };
        let display = gram.to_display_string(self.context.course.target_language);
        let Some(interned) = self.pack().course_gram(gram) else {
            return Err("that card's gram isn't a dictionary entry in this course".to_string());
        };
        let spur = match sentence {
            Some(text) => {
                let text = text.trim();
                let pack = self.pack();
                let Some(spur) = pack.string_rodeo.get(text) else {
                    return Err(
                        "that sentence isn't in yap's corpus — pass one exactly as returned by \
                         get_sentences, or omit it to let the server pick"
                            .to_string(),
                    );
                };
                let contains = pack
                    .sentences_containing_gram_index
                    .get(&interned)
                    .is_some_and(|sentences| sentences.contains(&spur));
                if !contains {
                    return Err(format!(
                        "that sentence doesn't contain «{display}» — pick one from get_sentences \
                         for this word, or omit it"
                    ));
                }
                spur
            }
            None => match self.deck().pick_translation_sentence(&interned) {
                Some(spur) => spur,
                None => {
                    return Err(format!(
                        "no comprehensible sentence contains «{display}» yet — quiz it with \
                         present_card instead"
                    ));
                }
            },
        };
        match self
            .deck()
            .translation_challenge_for_sentence(interned, spur)
        {
            Some(challenge) => Ok((spur, challenge)),
            None => Err("could not build a translation challenge for that sentence".to_string()),
        }
    }

    /// Human-readable provenance for a sentence.
    fn sentence_sources(&self, sentence: &lasso::Spur) -> Vec<String> {
        let pack = self.pack();
        let Some(source) = pack.sentence_sources.get(sentence) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if source.from_anki {
            out.push("Anki deck".to_string());
        }
        if source.from_tatoeba {
            out.push("Tatoeba".to_string());
        }
        if source.from_manual {
            out.push("manually curated".to_string());
        }
        if source.from_song {
            out.push("song lyrics".to_string());
        }
        for movie_id in &source.movie_ids {
            match pack.movies.get(movie_id) {
                Some(movie) => match movie.year {
                    Some(year) => out.push(format!("movie: {} ({year})", movie.title)),
                    None => out.push(format!("movie: {}", movie.title)),
                },
                None => out.push(format!("movie: {movie_id}")),
            }
        }
        out
    }

    /// Sample sentences containing an interned gram: up to `count` composed
    /// only of words the user already knows, and up to `count` more from the
    /// whole corpus, each rendered with translations and attribution.
    fn sample_sentences(&mut self, interned: SpurGram, count: usize) -> SampledSentences {
        let comprehensible_pool = {
            let deck = self.deck();
            let comprehensible = deck.comprehensible_written_grams(false);
            deck.context()
                .language_pack
                .comprehensible_sentences(Some(&interned), |g| comprehensible.contains(g))
        };

        use rand::seq::IndexedRandom as _;
        let chosen_comprehensible: Vec<Spur> = comprehensible_pool
            .choose_multiple(&mut rand::rng(), count)
            .copied()
            .collect();

        let pack = self.pack();
        let all_containing = pack
            .sentences_containing_gram_index
            .get(&interned)
            .cloned()
            .unwrap_or_default();
        let other_pool: Vec<Spur> = all_containing
            .iter()
            .copied()
            .filter(|sentence| !chosen_comprehensible.contains(sentence))
            .collect();
        let chosen_other: Vec<Spur> = other_pool
            .choose_multiple(&mut rand::rng(), count)
            .copied()
            .collect();

        let render = |sentence: &Spur| SentenceOut {
            text: pack.string_rodeo.resolve(sentence).to_string(),
            translations: pack
                .translations
                .get(sentence)
                .map(|ts| {
                    ts.iter()
                        .map(|t| pack.string_rodeo.resolve(t).to_string())
                        .collect()
                })
                .unwrap_or_default(),
            sources: self.sentence_sources(sentence),
        };
        SampledSentences {
            comprehensible: chosen_comprehensible.iter().map(render).collect(),
            other: chosen_other.iter().map(render).collect(),
            total_comprehensible: comprehensible_pool.len(),
            total_containing: all_containing.len(),
        }
    }
}

struct SampledSentences {
    comprehensible: Vec<SentenceOut>,
    other: Vec<SentenceOut>,
    total_comprehensible: usize,
    total_containing: usize,
}

/// Success result carrying the value both as readable text and as MCP
/// structured content.
fn ok_json_structured(value: serde_json::Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&value).expect("json serializes"),
    )]);
    result.structured_content = Some(value);
    result
}

/// Success result from a typed output (see [`crate::output`]): the same
/// value becomes both the readable text and the structured content, so the
/// payload always matches the tool's declared outputSchema.
fn ok_typed<T: Serialize>(value: &T) -> CallToolResult {
    ok_json_structured(serde_json::to_value(value).expect("output serializes"))
}

/// The public dictionary, canonical regardless of where the server runs.
const DICTIONARY_BASE_URL: &str = "https://yap.town/d";

/// A short native-language gloss for titles/citations.
fn entry_gloss(entry: &GramDictionaryEntry) -> String {
    match entry.definition() {
        GramDictionaryDefinition::Dictionary { definitions } => definitions
            .iter()
            .take(2)
            .map(|d| d.native.clone())
            .collect::<Vec<_>>()
            .join("; "),
        GramDictionaryDefinition::Phrasebook { meaning, .. } => meaning,
    }
}

fn entry_title(entry: &GramDictionaryEntry) -> String {
    let gloss = entry_gloss(entry);
    if gloss.is_empty() {
        entry.display_text()
    } else {
        format!("{} — {}", entry.display_text(), gloss)
    }
}

/// Stable id for the search/fetch pair, e.g. "french-to-english:42".
fn entry_id(course: &Course, entry: &GramDictionaryEntry) -> String {
    format!("{}:{}", course.dictionary_slug(), entry.frequency_index())
}

/// Best-effort public URL of the entry's dictionary page. Entries whose
/// display text collides with another's get a numeric suffix at site build
/// time that we can't reproduce here, so rare homographs may 404.
fn entry_url(course: &Course, display_text: &str) -> String {
    format!(
        "{DICTIONARY_BASE_URL}/{}/{}/",
        course.dictionary_slug(),
        dictionary_entry_slug(display_text)
    )
}

fn error(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

/// Resolved audio, cached process-wide: clips aren't user-specific, and the
/// widget re-requests the same sentences often. Bounded by wholesale clears.
#[derive(Clone)]
struct CachedAudio {
    bytes: Vec<u8>,
    voice_actor: Option<yap_frontend_rs::VoiceActorInfo>,
}

static AUDIO_CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, CachedAudio>>> =
    std::sync::Mutex::new(None);
const AUDIO_CACHE_MAX: usize = 256;

fn audio_cache_get(key: &str) -> Option<CachedAudio> {
    AUDIO_CACHE
        .lock()
        .expect("audio cache poisoned")
        .as_ref()
        .and_then(|cache| cache.get(key).cloned())
}

fn audio_cache_put(key: &str, audio: CachedAudio) {
    let mut guard = AUDIO_CACHE.lock().expect("audio cache poisoned");
    let cache = guard.get_or_insert_with(Default::default);
    if cache.len() >= AUDIO_CACHE_MAX {
        cache.clear();
    }
    cache.insert(key.to_string(), audio);
}

fn audio_result(audio: CachedAudio) -> CallToolResult {
    use base64::Engine as _;
    let mime_type = yap_frontend_rs::audio_mime_type(&audio.bytes);
    let source = if audio.voice_actor.is_some() {
        "human_recording"
    } else {
        "tts"
    };
    let out = GetAudioOut {
        audio_base64: base64::engine::general_purpose::STANDARD.encode(&audio.bytes),
        mime_type: mime_type.to_string(),
        voice_actor: audio.voice_actor,
        source: source.to_string(),
    };
    // The widget reads structuredContent; the text block is only narration,
    // so a summary beats duplicating the whole base64 payload.
    let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
        "{} audio, {} bytes ({source}) — in structuredContent for the widget",
        out.mime_type,
        audio.bytes.len(),
    ))]);
    result.structured_content = Some(serde_json::to_value(&out).expect("output serializes"));
    result
}

/// The card shape shared by get_due_cards and unlock_cards: everything the
/// LLM needs to quiz the user and pass the card back to log_review.
fn card_summary_out(language: Language, summary: &CardSummary) -> CardSummaryOut {
    let indicator = summary.card_indicator();
    CardSummaryOut {
        language,
        card: serde_json::to_value(&indicator).expect("card serializes"),
        text: summary.card_text(),
        subtitle: summary.card_subtitle(),
        kind: card_kind(&indicator).to_string(),
        fsrs_state: summary.state().to_string(),
        due: chrono::DateTime::from_timestamp_millis(summary.due_timestamp_ms() as i64)
            .map(|d| d.to_rfc3339()),
    }
}

fn card_kind(card: &CardIndicator<Gram<String>, String>) -> &'static str {
    match card {
        CardIndicator::WrittenGram { .. } => "written",
        CardIndicator::ListeningGram { .. } => "listening",
        CardIndicator::LetterPronunciation { .. } => "pronunciation",
    }
}

/// A fresh random nonce identifying one card/challenge presentation. The
/// widget echoes it back as the review's idempotency token, so a retried
/// submit — including one after the widget iframe reloads and replays the
/// same tool result — records the review exactly once.
fn presentation_nonce() -> String {
    format!("{:032x}", rand::random::<u128>())
}

/// Namespace an idempotency token by the tool that issued it, so the same
/// token can never replay another tool's (or payload's) cached response.
fn idempotency_key(tool: &str, token: &str) -> String {
    format!("{tool}:{token}")
}

fn parse_rating(rating: &str) -> Result<Rating, String> {
    match rating {
        "again" => Ok(Rating::Again),
        "hard" => Ok(Rating::Hard),
        "good" => Ok(Rating::Good),
        "easy" => Ok(Rating::Easy),
        "remembered" => Ok(Rating::Remembered),
        other => Err(format!(
            "invalid rating '{other}': expected one of again, hard, good, easy, remembered"
        )),
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SearchDictionaryParams {
    /// Text to search for: a target-language word/phrase or a native-language meaning.
    /// Accent-insensitive.
    query: String,
    /// Maximum number of results (default 20).
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AddCardsParams {
    /// The target language the grams belong to, e.g. "French". Must match the
    /// course this server is connected to.
    language: String,
    /// The grams (words/phrases) to add, each the exact gram JSON returned by
    /// search_dictionary: a sequence of tokens with word, lemma, and part of
    /// speech.
    grams: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetDueCardsParams {
    /// Maximum number of due cards to return (default 10).
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LogReviewParams {
    /// The target language of the card, e.g. "French".
    language: String,
    /// The card object exactly as returned by get_due_cards.
    card: serde_json::Value,
    /// How the review went: "again" (forgot), "hard", "good", or "easy".
    /// "remembered" is a simple success when finer grading doesn't apply.
    rating: String,
    /// Optional idempotency key for one review. Supplied by the review
    /// widget so that a retried submit (or a call whose response was lost
    /// after the event was recorded) records the review only once. Omit for
    /// conversational reviews — a fresh key each call.
    #[serde(default)]
    idempotency_token: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetSentencesParams {
    /// The target language of the gram, e.g. "French".
    language: String,
    /// The exact gram JSON returned by search_dictionary or get_due_cards.
    gram: serde_json::Value,
    /// How many sentences of each kind to return (default 5, max 20).
    #[serde(default)]
    count: Option<usize>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PresentCardParams {
    /// The target language of the card, e.g. "French".
    language: String,
    /// The card object exactly as returned by get_due_cards or unlock_cards.
    card: serde_json::Value,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PresentTranslationParams {
    /// The target language of the card, e.g. "French".
    language: String,
    /// The written-gram card being reviewed, exactly as returned by
    /// get_due_cards or unlock_cards.
    card: serde_json::Value,
    /// Optional: the exact text of a corpus sentence containing this card's
    /// gram, chosen from get_sentences — prefer comprehensible_sentences,
    /// since the grade reviews every word in the sentence, not just this
    /// card's. Omit to let the server pick the least-reviewed comprehensible
    /// sentence, the same selection the yap app uses. Sentences the model
    /// writes itself are rejected.
    #[serde(default)]
    sentence: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GradeTranslationParams {
    /// The target language of the challenge, e.g. "French".
    language: String,
    /// The card object exactly as given in the widget data.
    card: serde_json::Value,
    /// The challenge sentence exactly as given in the widget data.
    sentence: String,
    /// The user's typed translation.
    submission: String,
    /// Optional idempotency key for one submission, supplied by the review
    /// widget so a retried submit (or a lost response after the review was
    /// recorded) grades and records only once — and doesn't re-run the LLM.
    #[serde(default)]
    idempotency_token: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetAudioParams {
    /// The TTS request exactly as provided in widget data: text, language,
    /// and optional speed/is_ssml/instructions.
    request: serde_json::Value,
    /// The TTS provider to synthesize with when no human recording exists,
    /// exactly as provided in widget data (e.g. "ElevenLabs").
    provider: serde_json::Value,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// What to look up: a word or phrase in the language being learned, or a
    /// native-language meaning. Accent-insensitive.
    query: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct FetchParams {
    /// A result id exactly as returned by the search tool, e.g.
    /// "french-to-english:42".
    id: String,
}

/// Where a tool call's deck state comes from.
#[derive(Clone)]
enum StateSource {
    /// stdio mode: one account, resolved at startup.
    Single(Arc<tokio::sync::Mutex<YapState>>),
    /// Remote mode: per-user state, resolved from the request's bearer token.
    PerUser(Arc<crate::remote::RemoteApp>),
}

/// Base URI of the review widget, the interactive card UI hosts render
/// inline (MCP Apps extension). The URI hosts serve carries a content hash
/// suffix ([`Widget::uri`]) so a rebuilt widget gets a fresh URI — hosts
/// (claude.ai) cache `ui://` resources by URI and would otherwise keep
/// serving a stale iframe after every widget change.
pub const WIDGET_URI_BASE: &str = "ui://yap/review.html";
const WIDGET_MIME: &str = "text/html;profile=mcp-app";

/// The built review widget together with its content-addressed URI.
pub struct Widget {
    pub html: String,
    /// `ui://yap/review.html@<12 hex of sha256(html)>`.
    pub uri: String,
}

/// Content-addressed URI for a widget build: base + first 12 hex of the
/// HTML's sha256. Distinct HTML ⇒ distinct URI ⇒ hosts refetch it.
fn versioned_widget_uri(html: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(html.as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{WIDGET_URI_BASE}@{hex}")
}

/// Load the built review widget. Absent (not built) is fine: widget tools
/// degrade to text and the resource simply isn't listed.
pub fn load_widget_html() -> Option<Arc<Widget>> {
    let path = std::env::var("YAP_WIDGET_HTML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/widget/dist/index.html"
            ))
        });
    match std::fs::read_to_string(&path) {
        Ok(html) => {
            let uri = versioned_widget_uri(&html);
            log::info!(
                "review widget loaded from {} ({} bytes) as {uri}",
                path.display(),
                html.len()
            );
            Some(Arc::new(Widget { html, uri }))
        }
        Err(e) => {
            log::warn!(
                "review widget not available ({}: {e}) — widget tools degrade to text",
                path.display()
            );
            None
        }
    }
}

fn meta_from(value: serde_json::Value) -> rmcp::model::Meta {
    rmcp::model::Meta(
        value
            .as_object()
            .expect("meta literals are objects")
            .clone(),
    )
}

/// `_meta` for the widget resource: a fully closed CSP (the widget talks to
/// the world only through the host bridge) plus theming and the legacy
/// OpenAI aliases.
fn widget_resource_meta() -> rmcp::model::Meta {
    meta_from(json!({
        "ui": {
            "csp": { "connectDomains": [], "resourceDomains": [] },
            "prefersBorder": false,
        },
        "openai/widgetDescription": "An interactive yap flashcard: plays the audio, reveals the answer, and records the user's own grade.",
        "openai/widgetCSP": { "connect_domains": [], "resource_domains": [] },
    }))
}

/// Per-tool `_meta` when the widget is available: template linkage for
/// present_card, widget-only visibility for get_audio, and widget
/// callability for the tools the iframe invokes.
fn widget_tool_meta(tool_name: &str, widget_uri: &str) -> Option<rmcp::model::Meta> {
    match tool_name {
        "present_card" => Some(meta_from(json!({
            "ui": { "resourceUri": widget_uri },
            "openai/outputTemplate": widget_uri,
            "openai/toolInvocation/invoking": "Dealing a card…",
            "openai/toolInvocation/invoked": "Card ready",
        }))),
        "present_translation" => Some(meta_from(json!({
            "ui": { "resourceUri": widget_uri },
            "openai/outputTemplate": widget_uri,
            "openai/toolInvocation/invoking": "Dealing a sentence…",
            "openai/toolInvocation/invoked": "Challenge ready",
        }))),
        "grade_translation" => Some(meta_from(json!({
            "ui": { "visibility": ["app"] },
            "openai/widgetAccessible": true,
        }))),
        "get_audio" => Some(meta_from(json!({
            "ui": { "visibility": ["app"] },
            "openai/widgetAccessible": true,
        }))),
        "log_review" => Some(meta_from(json!({
            "openai/widgetAccessible": true,
        }))),
        _ => None,
    }
}

#[derive(Clone)]
pub struct YapMcp {
    source: StateSource,
    widget: Option<Arc<Widget>>,
}

impl YapMcp {
    pub fn new(state: YapState, widget: Option<Arc<Widget>>) -> Self {
        Self {
            source: StateSource::Single(Arc::new(tokio::sync::Mutex::new(state))),
            widget,
        }
    }

    pub fn new_remote(app: Arc<crate::remote::RemoteApp>) -> Self {
        Self {
            widget: app.widget.clone(),
            source: StateSource::PerUser(app),
        }
    }

    /// Resolve the deck state for this call. In remote mode the authenticated
    /// user travels in the propagated HTTP request parts.
    async fn state_slot(
        &self,
        ctx: &RequestContext<RoleServer>,
    ) -> Result<Arc<tokio::sync::Mutex<YapState>>, String> {
        match &self.source {
            StateSource::Single(state) => Ok(state.clone()),
            StateSource::PerUser(app) => {
                let user = ctx
                    .extensions
                    .get::<http::request::Parts>()
                    .and_then(|parts| parts.extensions.get::<crate::remote::AuthedUser>())
                    .ok_or("unauthenticated: no user on this request")?
                    .clone();
                app.state_for_user(&user)
                    .await
                    .map_err(|e| format!("failed to load your yap account: {e:#}"))
            }
        }
    }
}

#[tool_router]
impl YapMcp {
    #[tool(
        title = "Search the dictionary",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::SearchDictionaryOut>(),
        description = "Search the yap dictionary for words and phrases. Each match includes its language and gram — the token sequence (word + lemma + part of speech) that uniquely identifies it. Other tools take these verbatim; search first rather than constructing grams by hand.",
        annotations(
            title = "Search the dictionary",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search_dictionary(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<SearchDictionaryParams>,
    ) -> CallToolResult {
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return error("query must not be empty");
        }
        let limit = params.limit.unwrap_or(20).min(100);

        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let language = state.context.course.target_language;
        let deck = state.deck();
        let pack = &deck.context().language_pack;
        let entries = deck.get_gram_dictionary_entries(Some(query.clone()), limit);
        let results: Vec<DictionaryMatchOut> = entries
            .iter()
            .map(|entry| {
                let gram = pack
                    .gram_frequencies
                    .entries
                    .get_index(entry.frequency_index())
                    .map(|(spur, _)| {
                        serde_json::to_value(pack.resolve_gram(spur)).expect("gram serializes")
                    });
                DictionaryMatchOut {
                    language,
                    gram,
                    display_text: entry.display_text(),
                    frequency_rank: entry.frequency_index() + 1,
                    is_phrase: entry.is_phrase(),
                    in_deck: entry.is_in_deck(),
                    definition: entry.definition(),
                }
            })
            .collect();
        ok_typed(&SearchDictionaryOut {
            query,
            results,
            note: "frequency_rank 1 is the most common word in the course. Pass language + gram verbatim to add_cards or get_sentences.".to_string(),
        })
    }

    #[tool(
        title = "Add flashcards",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::AddCardsOut>(),
        description = "Add words/phrases to the user's yap deck as new flashcards. Takes (language, gram) pairs, normally obtained from search_dictionary; anything that doesn't name a real dictionary entry is rejected. Confirm with the user before adding.",
        annotations(
            title = "Add flashcards",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn add_cards(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<AddCardsParams>,
    ) -> CallToolResult {
        if params.grams.is_empty() {
            return error("grams must not be empty");
        }
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }

        // Validate everything before touching the deck: reject the whole call
        // on any unknown gram rather than adding a best guess.
        let target_language = state.context.course.target_language;
        let mut resolved = Vec::new();
        let mut errors = Vec::new();
        for value in &params.grams {
            match state.parse_gram(value) {
                Ok((gram, _)) => resolved.push(gram),
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            return error(format!("no cards were added:\n{}", errors.join("\n")));
        }

        let candidates: Vec<(CardIndicator<Gram<String>, String>, String)> = resolved
            .into_iter()
            .map(|gram| {
                let display = gram.to_display_string(target_language);
                (CardIndicator::WrittenGram { gram }, display)
            })
            .collect();

        let deck = state.deck();
        let mut to_add = Vec::new();
        let mut already_in_deck = Vec::new();
        for (card, display) in candidates {
            if deck.find_card_summary(&card).is_some() {
                already_in_deck.push(display);
            } else {
                to_add.push((card, display));
            }
        }

        let added: Vec<String> = to_add.iter().map(|(_, display)| display.clone()).collect();
        let mut synced = SyncStatus::Synced;
        if !to_add.is_empty() {
            let sentence_list = deck.current_sentence_list();
            let content = LanguageEventContent::AddCards {
                cards: to_add.into_iter().map(|(card, _)| card).collect(),
                sentence_list,
            };
            synced = state.append_event(content).await;
        }

        let deck = state.deck();
        ok_typed(&AddCardsOut {
            added,
            already_in_deck,
            total_cards_in_deck: deck.num_cards_added(),
            synced: synced == SyncStatus::Synced,
        })
    }

    #[tool(
        title = "List due flashcards",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::GetDueCardsOut>(),
        description = "List the user's currently-due yap flashcards, most overdue first. Each entry carries its language and card object; pass both verbatim to log_review after quizzing the user. Gram cards can be quizzed with example sentences from get_sentences.",
        annotations(
            title = "List due flashcards",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_due_cards(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<GetDueCardsParams>,
    ) -> CallToolResult {
        let limit = params.limit.unwrap_or(10).min(50);
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let language = state.context.course.target_language;
        let now_ms = Utc::now().timestamp_millis() as f64;
        let deck = state.deck();
        let due = deck.due_card_summaries(now_ms);
        let total_due = due.len();
        let locked = deck.locked_count();

        let cards: Vec<CardSummaryOut> = due
            .iter()
            .take(limit)
            .map(|summary| card_summary_out(language, summary))
            .collect();

        ok_typed(&GetDueCardsOut {
            total_due,
            showing: cards.len(),
            cards,
            cards_in_lockup: locked,
            note: "kind 'written' = recognize the written word; 'listening' = normally an audio card (quiz in text as best you can); 'pronunciation' = a letter-sound pattern. A card's gram field can be passed to get_sentences.".to_string(),
            lockup_note: (locked > 0).then(|| format!(
                "{locked} cards are set aside in lockup — when a big backlog is due at once, \
                 yap keeps a manageable handful active and sets the rest aside to be practiced \
                 later. unlock_cards releases the next batch; reviewing a locked card also \
                 unlocks it."
            )),
        })
    }

    #[tool(
        title = "Unlock cards",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::UnlockCardsOut>(),
        description = "Release the next batch of set-aside cards from lockup, putting them back in the due queue. Lockup is how yap keeps sessions manageable: when a big backlog is due at once, it keeps a handful of cards active and sets the rest aside to be practiced later. Reviewing a locked card also unlocks it. Confirm with the user before calling.",
        annotations(
            title = "Unlock cards",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn unlock_cards(&self, ctx: RequestContext<RoleServer>) -> CallToolResult {
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let language = state.context.course.target_language;

        let (content, unlocked) = {
            let deck = state.deck();
            match deck.get_release_offer(Utc::now().timestamp_millis() as f64) {
                None => {
                    let locked = deck.locked_count();
                    let note = if locked == 0 {
                        "no cards are in lockup"
                    } else {
                        "locked cards exist but none are due yet; try again once they're due"
                    };
                    return ok_typed(&UnlockCardsOut {
                        unlocked: vec![],
                        cards_in_lockup: locked,
                        note: note.to_string(),
                        synced: None,
                    });
                }
                Some(offer) => {
                    let unlocked: Vec<CardSummaryOut> = offer
                        .release_preview()
                        .iter()
                        .map(|summary| card_summary_out(language, summary))
                        .collect();
                    let DeckEvent::Language(event) = offer.unlock_event();
                    (event.content, unlocked)
                }
            }
        };
        let synced = state.append_event(content).await;

        ok_typed(&UnlockCardsOut {
            unlocked,
            cards_in_lockup: state.deck().locked_count(),
            note: "these cards are back in the due queue and will show up in get_due_cards."
                .to_string(),
            synced: Some(synced == SyncStatus::Synced),
        })
    }

    #[tool(
        title = "Present a card",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::PresentOut>(),
        description = "Present one card to the user as an interactive widget: a written/listening card quizzes the single gram (word) with audio (a human recording when the course has one) and a reveal; a pronunciation card shows the letter-sound pattern with example words and their audio. The widget records the user's own grade via log_review — so after presenting, do NOT log a review for this card yourself; the outcome is reported back to you. For a written card the user has seen before, prefer present_translation instead, like the yap app does.",
        annotations(
            title = "Present a card",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn present_card(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<PresentCardParams>,
    ) -> CallToolResult {
        if self.widget.is_none() {
            return error(
                "the interactive review widget isn't available on this server build — quiz the user in conversation instead (get_due_cards + get_sentences + log_review)",
            );
        }
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }
        let card = match state.parse_card(&params.card) {
            Ok(card) => card,
            Err(e) => return error(e),
        };
        let target_language = state.context.course.target_language;

        // Pronunciation cards have no gram: build the app's own
        // PronunciationChallenge (guide + per-example audio) and ship that.
        if let CardIndicator::LetterPronunciation { pattern, position } = &card {
            let now_ms = Utc::now().timestamp_millis() as f64;
            let challenge = {
                let deck = state.deck();
                let not_in_deck = || {
                    error(
                        "that card isn't in the user's deck — pass a card exactly as returned by get_due_cards",
                    )
                };
                let Some(pattern_spur) = deck.context().language_pack.string_rodeo.get(pattern)
                else {
                    return not_in_deck();
                };
                let interned = CardIndicator::LetterPronunciation {
                    pattern: pattern_spur,
                    position: *position,
                };
                let Some(card_ctx) = CardContext::new(deck, interned) else {
                    return not_in_deck();
                };
                let review_info = deck.get_review_info(vec![], now_ms);
                match review_info.pronunciation_challenge(deck, &card_ctx, pattern_spur, *position)
                {
                    Some(challenge) => challenge,
                    None => {
                        return error(
                            "no pronunciation guide exists for that pattern in this course — quiz it in conversation instead",
                        );
                    }
                }
            };
            let Challenge::PronunciationChallenge {
                pattern,
                guide,
                audio_requests,
                is_new,
                times_type_seen,
                ..
            } = challenge
            else {
                unreachable!("pronunciation_challenge builds a PronunciationChallenge");
            };
            let language = state.target_language_value();
            let structured = json!({
                "challenge": {
                    "type": "pronunciation",
                    "nonce": presentation_nonce(),
                    "language": language,
                    "is_new": is_new,
                    "times_type_seen": times_type_seen,
                    "card": params.card,
                    "pattern": pattern,
                    "guide": guide,
                    "audio_requests": audio_requests,
                    // The spoken "as in" connector, precomputed here so the
                    // widget needs no WASM to render the app's component.
                    "connector": target_language.pronunciation_connector(),
                },
            });
            let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
                "Presented the pronunciation pattern «{pattern}» to the user as an interactive card. The widget plays the example audio and records the user's own grade via log_review — do not call log_review for this card yourself; the outcome will be reported back here."
            ))]);
            result.structured_content = Some(structured);
            return result;
        }

        let gram = match &card {
            CardIndicator::WrittenGram { gram } | CardIndicator::ListeningGram { gram } => {
                gram.clone()
            }
            CardIndicator::LetterPronunciation { .. } => unreachable!("returned above"),
        };
        let display = gram.to_display_string(target_language);
        let kind = card_kind(&card);
        let Some(interned) = state.pack().course_gram(&gram) else {
            return error("that card's gram isn't a dictionary entry in this course");
        };

        // Mint the exact FlashCard the app renders — same builders over the same
        // deck + language pack — so the widget can render the app's Flashcard
        // component verbatim (full card back, morphology, homophone grid) with
        // no widget-specific projection of the content.
        let now_ms = Utc::now().timestamp_millis() as f64;
        let (flashcard, is_new, times_type_seen, total_count) = {
            let deck = state.deck();
            let review_info = deck.get_review_info(vec![], now_ms);
            let interned_indicator = match &card {
                CardIndicator::WrittenGram { .. } => CardIndicator::WrittenGram { gram: interned },
                CardIndicator::ListeningGram { .. } => {
                    CardIndicator::ListeningGram { gram: interned }
                }
                CardIndicator::LetterPronunciation { .. } => unreachable!("returned above"),
            };
            let Some(ctx) = CardContext::new(deck, interned_indicator) else {
                return error(
                    "that card isn't in the user's deck — pass a card exactly as returned by get_due_cards",
                );
            };
            let flashcard = match &card {
                CardIndicator::WrittenGram { .. } => {
                    review_info.written_gram_flashcard(deck, interned)
                }
                CardIndicator::ListeningGram { .. } => {
                    review_info.listening_gram_flashcard(deck, interned)
                }
                CardIndicator::LetterPronunciation { .. } => unreachable!("returned above"),
            };
            (
                flashcard,
                ctx.is_new,
                ctx.times_type_seen,
                review_info.total_count(),
            )
        };

        let language = state.target_language_value();
        let native_language = serde_json::to_value(state.context.course.native_language)
            .expect("Language serializes");
        let structured = json!({
            "challenge": {
                "type": "flashcard",
                "nonce": presentation_nonce(),
                "language": language,
                "native_language": native_language,
                "kind": kind,
                "is_new": is_new,
                "total_count": total_count,
                "times_type_seen": times_type_seen,
                "card": params.card,
                "content": serde_json::to_value(&flashcard.content).expect("content serializes"),
                "audio": serde_json::to_value(&flashcard.audio).expect("audio serializes"),
            },
        });
        let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
            "Presented «{display}» to the user as an interactive card. The widget plays the audio and records the user's own grade via log_review — do not call log_review for this card yourself; the outcome will be reported back here."
        ))]);
        result.structured_content = Some(structured);
        result
    }

    #[tool(
        title = "Present a translation challenge",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::PresentOut>(),
        description = "Present a sentence-translation challenge for a written-gram card, as an interactive widget: the user reads a corpus sentence containing the word, types a translation, and the server autogrades it and logs the review — every word in the sentence gets reviewed, exactly like translation challenges in the yap app. Prefer this over present_card for written cards the user has seen before (not new). After presenting, do NOT call log_review or grade_translation yourself; the outcome is reported back to you. Optionally pass the exact text of a sentence from get_sentences (prefer comprehensible_sentences — every word in it gets reviewed) to control which sentence is used; omit it to let the server pick the way the app does.",
        annotations(
            title = "Present a translation challenge",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn present_translation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<PresentTranslationParams>,
    ) -> CallToolResult {
        if self.widget.is_none() {
            return error(
                "the interactive review widget isn't available on this server build — quiz the user in conversation instead (get_due_cards + get_sentences + log_review)",
            );
        }
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }
        let card = match state.parse_card(&params.card) {
            Ok(card) => card,
            Err(e) => return error(e),
        };
        let is_new = {
            let Some(summary) = state.deck().find_card_summary(&card) else {
                return error(
                    "that card isn't in the user's deck — pass a card exactly as returned by get_due_cards",
                );
            };
            summary.state() == "New"
        };
        let (spur, challenge) =
            match state.translation_challenge_payload(&card, params.sentence.as_deref()) {
                Ok(payload) => payload,
                Err(e) => return error(e),
            };

        let target_language = state.context.course.target_language;
        let display = challenge
            .primary_expression
            .to_display_string(target_language);
        // Send the full literals (word + word_type + whitespace) so the widget
        // renders the app's shared ChallengeSentence verbatim, including the
        // heteronym-aware colouring.
        let literals =
            serde_json::to_value(&challenge.target_language_literals).expect("literals serialize");

        let structured = json!({
            "challenge": {
                "type": "translation",
                "nonce": presentation_nonce(),
                "language": state.target_language_value(),
                "card": params.card,
                "is_new": is_new,
                "sentence": {
                    "text": challenge.target_language,
                    "literals": literals,
                    "sources": state.sentence_sources(&spur),
                },
                "audio": serde_json::to_value(&challenge.audio).expect("audio serializes"),
            },
        });
        let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
            "Presented a translation challenge for «{display}»: the user is translating «{}». The widget autogrades the submission and logs the review itself — do not call log_review or grade_translation for it; the outcome will be reported back here.",
            challenge.target_language
        ))]);
        result.structured_content = Some(structured);
        result
    }

    #[tool(
        title = "Grade a translation",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::GradeTranslationOut>(),
        description = "Autograde the user's typed translation of a presented challenge sentence and log the review — every word in the sentence gets reviewed. Called by the review widget when the user submits; never call it directly, since only the user's own typed submission should be graded.",
        annotations(
            title = "Grade a translation",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn grade_translation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<GradeTranslationParams>,
    ) -> CallToolResult {
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        const TOOL: &str = "grade_translation";
        // The presentation's immutable identity — the mutable submission is
        // deliberately excluded so an edited retry still replays.
        let identity = json!([params.card, params.language, params.sentence]).to_string();
        let mut state = slot.lock().await;
        // Sync first, so a retry after a failed upload drives the pending
        // flush before we short-circuit on the cached response.
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        // A retry of this presentation (same nonce) replays the first grade —
        // no second review event, and no second (paid, nondeterministic) LLM
        // grading — even if the user edited their answer before retrying.
        match state.replay_idempotent(TOOL, params.idempotency_token.as_deref(), &identity) {
            Replay::Hit(cached) => return cached,
            Replay::IdentityConflict => return error(IDEMPOTENCY_CONFLICT_MSG),
            Replay::Miss => {}
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }
        let card = match state.parse_card(&params.card) {
            Ok(card) => card,
            Err(e) => return error(e),
        };
        let submission = params.submission.trim().to_string();
        if submission.is_empty() {
            return error("the submission is empty — the user needs to type a translation first");
        }
        let (_, challenge) =
            match state.translation_challenge_payload(&card, Some(&params.sentence)) {
                Ok(payload) => payload,
                Err(e) => return error(e),
            };

        let course = state.context.course;
        let native_language = course.native_language;
        // The exact same grading the app runs (perfect-match short-circuit,
        // AI backend via fetch-happen, heuristic fallback) — fetch-happen's
        // native transport lets us call it directly instead of reimplementing
        // it. Anonymous token, like the widget's TTS path.
        let response = autograde_translation(
            challenge.target_language.clone(),
            submission.clone(),
            challenge.native_translations.clone(),
            challenge.target_language_literals.clone(),
            challenge.unique_target_language_phrases.clone(),
            None,
            course,
            autograde::GramDefinitions(challenge.gram_definitions_for_lookup.clone()),
            challenge.literal_gram_indices.clone(),
            autograde::GramDefinitions(challenge.phrase_definitions.clone()),
            challenge.primary_expression.clone(),
        )
        .await;

        // The promotion rule is shared Rust — see translation_is_perfect.
        let perfect =
            translation_is_perfect(challenge.target_language_literals.clone(), response.clone());

        let event = if perfect {
            state
                .deck()
                .translate_sentence_perfect(vec![], challenge.target_language.clone())
        } else {
            state.deck().translate_sentence_wrong(
                challenge.target_language.clone(),
                submission.clone(),
                autograde::LiteralGrades(response.literal_grades.clone()),
                vec![],
                response.phrases_remembered.clone(),
                response.phrases_forgot.clone(),
            )
        };
        let Some(DeckEvent::Language(event)) = event else {
            return error("could not build the review event for that sentence");
        };
        let synced = state.append_event(event.content).await;

        let remaining_due = state
            .deck()
            .due_card_summaries(Utc::now().timestamp_millis() as f64)
            .len();
        let target_language = course.target_language;
        let correct_translation =
            find_closest_match(&submission, &challenge.native_translations, native_language)
                .or_else(|| challenge.native_translations.first().cloned());

        let result = ok_typed(&GradeTranslationOut {
            perfect,
            correct_translation,
            encouragement: response.encouragement,
            explanation: response.explanation,
            autograding_error: response.autograding_error,
            literal_grades: response
                .literal_grades
                .iter()
                .map(|grade| match grade {
                    Some(autograde::Remembered::Remembered) => Some(LiteralGradeOut::Remembered),
                    Some(autograde::Remembered::Forgot) => Some(LiteralGradeOut::Forgot),
                    None => None,
                })
                .collect(),
            phrases_remembered: response
                .phrases_remembered
                .iter()
                .map(|phrase| phrase.to_display_string(target_language))
                .collect(),
            phrases_forgot: response
                .phrases_forgot
                .iter()
                .map(|phrase| phrase.to_display_string(target_language))
                .collect(),
            remaining_due,
            synced: synced == SyncStatus::Synced,
        });
        state.remember_idempotent(
            TOOL,
            params.idempotency_token.as_deref(),
            &identity,
            &result,
        );
        result
    }

    #[tool(
        title = "Get audio",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::GetAudioOut>(),
        description = "Fetch pronunciation audio for a widget: the course's human voice-actor recording when one exists, otherwise synthesized speech — the same resolution the yap app uses. Returns base64 audio. Intended for the review widget; not useful in conversation.",
        annotations(
            title = "Get audio",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_audio(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<GetAudioParams>,
    ) -> CallToolResult {
        let request: language_utils::TtsRequest = match serde_json::from_value(params.request) {
            Ok(request) => request,
            Err(e) => {
                return error(format!(
                    "invalid audio request (pass it exactly as given in the widget data): {e}"
                ));
            }
        };
        let provider: language_utils::TtsProvider = match serde_json::from_value(params.provider) {
            Ok(provider) => provider,
            Err(e) => {
                return error(format!(
                    "invalid provider (pass it exactly as given in the widget data): {e}"
                ));
            }
        };

        // Authenticated per-user state must exist before we spend TTS money,
        // and loading it registers the course pack's human recordings so the
        // lookup below can find them.
        if let Err(e) = self.state_slot(&ctx).await {
            return error(e);
        }

        let cache_key = format!(
            "{}|{}",
            serde_json::to_string(&request).expect("request serializes"),
            serde_json::to_string(&provider).expect("provider serializes"),
        );
        if let Some(cached) = audio_cache_get(&cache_key) {
            return audio_result(cached);
        }

        let resolved = if yap_frontend_rs::human_audio_applies(&request)
            && let Some(human) =
                yap_frontend_rs::lookup_human_audio(request.language, &request.text)
        {
            CachedAudio {
                bytes: human.bytes,
                voice_actor: Some(yap_frontend_rs::VoiceActorInfo {
                    name: human.actor_name,
                    compensation: human.compensation,
                }),
            }
        } else {
            match yap_frontend_rs::fetch_tts(&request, &provider, None).await {
                Ok(bytes) => CachedAudio {
                    bytes,
                    voice_actor: None,
                },
                Err(e) => return error(format!("audio synthesis failed: {e}")),
            }
        };
        audio_cache_put(&cache_key, resolved.clone());
        audio_result(resolved)
    }

    #[tool(
        title = "Log a review",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::LogReviewOut>(),
        description = "Record the result of reviewing one card. This updates real spaced-repetition scheduling on the user's account, so only call it after actually quizzing the user, with an honest rating.",
        annotations(
            title = "Log a review",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn log_review(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<LogReviewParams>,
    ) -> CallToolResult {
        let rating = match parse_rating(&params.rating) {
            Ok(r) => r,
            Err(e) => return error(e),
        };
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        const TOOL: &str = "log_review";
        // The card under review is the presentation's immutable identity; the
        // mutable rating is excluded so a retry replays the first review.
        let identity = json!([params.card, params.language]).to_string();
        let mut state = slot.lock().await;
        // Sync first, so a retry after a failed upload drives the pending
        // flush before we short-circuit on the cached response.
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        // A retried log of the same presentation (same nonce) replays the
        // first result instead of recording the card as reviewed twice.
        match state.replay_idempotent(TOOL, params.idempotency_token.as_deref(), &identity) {
            Replay::Hit(cached) => return cached,
            Replay::IdentityConflict => return error(IDEMPOTENCY_CONFLICT_MSG),
            Replay::Miss => {}
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }
        let card = match state.parse_card(&params.card) {
            Ok(c) => c,
            Err(e) => return error(e),
        };
        let deck = state.deck();
        let Some(before) = deck.find_card_summary(&card) else {
            return error(
                "that card is not an active card in the deck. Use get_due_cards to see reviewable cards.",
            );
        };
        let now_ms = Utc::now().timestamp_millis() as f64;
        let was_due = before.due_timestamp_ms() <= now_ms;

        let content = LanguageEventContent::ReviewCard {
            reviewed: card.clone(),
            rating,
        };
        let synced = state.append_event(content).await;

        let deck = state.deck();
        let after = deck.find_card_summary(&card);
        let remaining_due = deck
            .due_card_summaries(Utc::now().timestamp_millis() as f64)
            .len();
        let result = ok_typed(&LogReviewOut {
            reviewed: before.card_text(),
            rating: params.rating.clone(),
            was_due,
            fsrs_state: after.as_ref().map(|a| a.state().to_string()),
            next_due: after.and_then(|a| {
                chrono::DateTime::from_timestamp_millis(a.due_timestamp_ms() as i64)
                    .map(|d| d.to_rfc3339())
            }),
            remaining_due,
            total_reviews: deck.stats().total_reviews,
            synced: synced == SyncStatus::Synced,
        });
        state.remember_idempotent(
            TOOL,
            params.idempotency_token.as_deref(),
            &identity,
            &result,
        );
        result
    }

    #[tool(
        title = "Get example sentences",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::GetSentencesOut>(),
        description = "Get random example sentences (with translations and source attribution) containing a given gram. Returns two lists: comprehensible_sentences, which are otherwise composed only of words the user already knows (great for quizzing), and other_sentences, sampled from everything containing the gram regardless of difficulty.",
        annotations(
            title = "Get example sentences",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_sentences(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<GetSentencesParams>,
    ) -> CallToolResult {
        let count = params.count.unwrap_or(5).clamp(1, 20);
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        if let Err(e) = state.check_language(&params.language) {
            return error(e);
        }
        let (gram, interned) = match state.parse_gram(&params.gram) {
            Ok(pair) => pair,
            Err(e) => return error(e),
        };
        let display = gram.to_display_string(state.context.course.target_language);

        let sampled = state.sample_sentences(interned, count);
        ok_typed(&GetSentencesOut {
            word: display,
            comprehensible_sentences: sampled.comprehensible,
            total_comprehensible: sampled.total_comprehensible,
            other_sentences: sampled.other,
            total_containing_word: sampled.total_containing,
            note: "comprehensible_sentences use only words the user already knows; other_sentences are sampled from everything containing the word and may include unknown words (and may lack translations).".to_string(),
        })
    }

    #[tool(
        title = "Get learning stats",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::GetStatsOut>(),
        description = "Get the user's yap stats: streak, XP, review counts, deck size, due cards, comprehension tier, and recent daily activity.",
        annotations(
            title = "Get learning stats",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_stats(&self, ctx: RequestContext<RoleServer>) -> CallToolResult {
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let course = state.context.course;
        let deck = state.deck();
        let stats = deck.stats();
        let tier = deck.get_current_tier();
        let now_ms = Utc::now().timestamp_millis() as f64;
        let due = deck.due_card_summaries(now_ms).len();

        let past_days: Vec<DayOut> = stats
            .past_days
            .iter()
            .rev()
            .take(7)
            .map(|(day, summary)| DayOut {
                date: chrono::NaiveDate::from_num_days_from_ce_opt(*day as i32)
                    .map(|d| d.to_string()),
                reviews: summary.reviews,
                time_spent_seconds: summary.time_spent_seconds,
                new_cards: summary.new_cards,
                learned_cards: summary.learned_cards,
            })
            .collect();

        ok_typed(&GetStatsOut {
            course: format!(
                "{} for {} speakers",
                course.target_language, course.native_language
            ),
            daily_streak: deck.get_daily_streak(),
            xp: stats.xp,
            total_reviews: stats.total_reviews,
            started: stats.start_time.map(|t| t.to_rfc3339()),
            cards_in_deck: deck.num_cards_added(),
            cards_due_now: due,
            cards_locked_away: deck.locked_count(),
            tier: TierOut {
                name: tier.name,
                tier: tier.tier,
                level: tier.level,
                total_levels: tier.total_levels,
                percent_known: tier.percent_known,
            },
            recent_days: past_days,
        })
    }

    #[tool(
        title = "Search dictionary pages",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::SearchOut>(),
        description = "Search the dictionary of the user's yap course. Returns matching entry pages as results with id, title, and url (a citable yap.town dictionary link); pass an id to fetch for the full entry. For deck operations that need exact grams (add_cards, get_sentences, log_review), use search_dictionary instead.",
        annotations(
            title = "Search dictionary pages",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<SearchParams>,
    ) -> CallToolResult {
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return error("query must not be empty");
        }
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let course = state.context.course;
        let results: Vec<SearchResultOut> = state
            .deck()
            .get_gram_dictionary_entries(Some(query), 10)
            .iter()
            .map(|entry| SearchResultOut {
                id: entry_id(&course, entry),
                title: entry_title(entry),
                url: entry_url(&course, &entry.display_text()),
            })
            .collect();
        ok_typed(&SearchOut { results })
    }

    #[tool(
        title = "Fetch a dictionary page",
        output_schema = rmcp::handler::server::common::schema_for_type::<crate::output::FetchOut>(),
        description = "Fetch a dictionary entry page by an id returned from the search tool: definitions, frequency rank, whether it's in the user's deck, and example sentences with translations and source attribution. Returns readable text plus a citable url.",
        annotations(
            title = "Fetch a dictionary page",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn fetch(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<FetchParams>,
    ) -> CallToolResult {
        let parsed = params
            .id
            .rsplit_once(':')
            .and_then(|(slug, idx)| Some((slug.to_string(), idx.parse::<usize>().ok()?)));
        let Some((course_slug, index)) = parsed else {
            return error(
                "invalid id — pass an id exactly as returned by the search tool, e.g. \"french-to-english:42\"",
            );
        };

        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.sync().await {
            log::warn!("sync failed, using possibly-stale events: {e:#}");
        }
        let course = state.context.course;
        if course_slug != course.dictionary_slug() {
            return error(format!(
                "that id belongs to the '{course_slug}' dictionary, but this account's course is '{}'",
                course.dictionary_slug()
            ));
        }
        let Some(entry) = state.deck().gram_dictionary_entry(index) else {
            return error(
                "no dictionary entry with that id — use an id returned by the search tool",
            );
        };

        let interned: SpurGram = *state
            .pack()
            .gram_frequencies
            .entries
            .get_index(index)
            .expect("index valid: entry was just built from it")
            .0;
        let gram_value =
            serde_json::to_value(state.pack().resolve_gram(&interned)).expect("gram serializes");
        let language = state.context.course.target_language;
        let sampled = state.sample_sentences(interned, 3);

        let display = entry.display_text();
        let url = entry_url(&course, &display);
        use std::fmt::Write as _;
        let mut text = String::new();
        let _ = writeln!(text, "{display}");
        match entry.definition() {
            GramDictionaryDefinition::Dictionary { definitions } => {
                for def in &definitions {
                    let _ = write!(text, "• {}", def.native);
                    if let Some(note) = &def.note {
                        let _ = write!(text, " ({note})");
                    }
                    let _ = writeln!(text);
                    if !def.example_sentence_target_language.is_empty() {
                        let _ = writeln!(
                            text,
                            "  e.g. “{}” — “{}”",
                            def.example_sentence_target_language,
                            def.example_sentence_native_language
                        );
                    }
                }
            }
            GramDictionaryDefinition::Phrasebook {
                meaning,
                target_language_example,
                native_language_example,
            } => {
                let _ = writeln!(text, "• {meaning}");
                if let (Some(t), Some(n)) = (target_language_example, native_language_example) {
                    let _ = writeln!(text, "  e.g. “{t}” — “{n}”");
                }
            }
        }
        let _ = writeln!(
            text,
            "\nFrequency rank {} (1 = most common in this course). {}",
            entry.frequency_index() + 1,
            if entry.is_in_deck() {
                "Already in the user's deck."
            } else {
                "Not in the user's deck."
            }
        );
        for (label, list) in [
            (
                "Example sentences the user can already fully understand:",
                &sampled.comprehensible,
            ),
            (
                "More sentences containing it (may use words the user hasn't learned):",
                &sampled.other,
            ),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(text, "\n{label}");
            for sentence in list {
                let _ = write!(text, "• “{}”", sentence.text);
                if let Some(translation) = sentence.translations.first() {
                    let _ = write!(text, " — “{translation}”");
                }
                if !sentence.sources.is_empty() {
                    let _ = write!(text, " (source: {})", sentence.sources.join(", "));
                }
                let _ = writeln!(text);
            }
        }
        let _ = writeln!(text, "\nDictionary page: {url}");

        ok_typed(&FetchOut {
            id: params.id,
            title: entry_title(&entry),
            text,
            url,
            metadata: FetchMetadataOut {
                language,
                gram: Some(gram_value),
                frequency_rank: entry.frequency_index() + 1,
                in_deck: entry.is_in_deck(),
                is_phrase: entry.is_phrase(),
            },
        })
    }
}

#[tool_handler]
impl ServerHandler for YapMcp {
    /// Hand-written (the macro defers to it): injects the MCP Apps `_meta`
    /// onto widget-related tools, only when the widget is actually built.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = Self::tool_router().list_all();
        if let Some(widget) = &self.widget {
            for tool in &mut tools {
                if let Some(meta) = widget_tool_meta(&tool.name, &widget.uri) {
                    tool.meta = Some(meta);
                }
            }
        }
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut result = ListResourcesResult::default();
        if let Some(widget) = &self.widget {
            let mut resource = Resource::new(widget.uri.clone(), "yap review card");
            resource.title = Some("yap review card".to_string());
            resource.description =
                Some("Interactive flashcard widget: audio, reveal, and grading.".to_string());
            resource.mime_type = Some(WIDGET_MIME.to_string());
            resource.size = Some(widget.html.len() as u64);
            resource.meta = Some(widget_resource_meta());
            result.resources.push(resource);
        }
        Ok(result)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        // Any version of the widget URI (bare, current, or a stale hash) gets
        // the current build. Hosts cache tool metadata across deploys — after
        // a widget rebuild, ChatGPT still asks for the previous hash, and a
        // not-found here surfaces as "Failed to fetch template" on every
        // present_* call until the host refreshes the schema. Serving current
        // content for a stale URI is strictly better than an error; the hash
        // still does its cache-busting job by changing the URI hosts fetch.
        let is_widget_uri = request.uri == WIDGET_URI_BASE
            || request
                .uri
                .strip_prefix(WIDGET_URI_BASE)
                .is_some_and(|rest| rest.starts_with('@'));
        match &self.widget {
            Some(widget) if is_widget_uri => {
                let mut result = ReadResourceResult::new(vec![]);
                result.contents = vec![ResourceContents::TextResourceContents {
                    // Echo the requested URI: hosts key the fetched content
                    // by the URI they asked for.
                    uri: request.uri.clone(),
                    mime_type: Some(WIDGET_MIME.to_string()),
                    text: widget.html.clone(),
                    meta: Some(widget_resource_meta()),
                }];
                Ok(result)
            }
            _ => Err(McpError::resource_not_found(
                format!("unknown resource: {}", request.uri),
                None,
            )),
        }
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        info.instructions = Some(
            "Access to the user's yap.town language-learning account: their flashcard deck, \
             spaced-repetition reviews, dictionary, and stats.\n\
             \n\
             Typical review session: get_due_cards, then for each card quiz the user \
             (get_sentences can supply an example sentence that the user should fully \
             understand), then log_review with an honest rating.\n\
             \n\
             Words are identified by (language, gram), where a gram is the exact token \
             sequence — word + lemma + part of speech — returned by search_dictionary and \
             get_due_cards. Pass those objects back verbatim; the server rejects anything \
             that doesn't name a real dictionary entry. To add new words: search_dictionary \
             first, show the user what you found, then pass the matches' language + gram to \
             add_cards.\n\
             \n\
             search and fetch are a standard browse/cite pair over the public dictionary at \
             yap.town/d/ — use them to look up and cite entries; use search_dictionary when \
             you need exact grams for the deck tools.\n\
             \n\
             When the interactive widgets are available, prefer them for quizzing, and pick \
             the one the yap app would use: present_translation for a written card the user \
             has seen before (the user translates a corpus sentence containing the word; the \
             server autogrades and logs it), and present_card for new cards, listening \
             cards, pronunciation cards, and words present_translation says have no \
             comprehensible sentence yet (a card the user grades themselves). Never call \
             log_review or \
             grade_translation for a card you presented — the widget logs the review and \
             the outcome is reported back to you."
                .to_string(),
        );
        info
    }
}
