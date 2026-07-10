//! The yap MCP server: state management and tool implementations.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use chrono::Utc;
use rmcp::{
    RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use language_utils::{
    Course, Gram, Language, SpurGram, dictionary_entry_slug, language_pack::LanguagePack,
};
use lasso::Spur;
use weapon::data_model::{EventStore, EventType};
use yap_frontend_rs::{
    CardIndicator, CardSummary, Context, Deck, DeckEvent, LanguageEvent, LanguageEventContent,
    Rating,
    dictionary::{GramDictionaryDefinition, GramDictionaryEntry},
};

use crate::deck::{PackCache, build_deck, detect_course, insert_rows, new_store};
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

pub struct YapState {
    supabase: SupabaseAuth,
    http: reqwest::Client,
    user_id: String,
    context: Context,
    store: EventStore<String, String>,
    last_fetched_id: Option<i64>,
    last_fetch: Instant,
    deck: Option<Deck>,
    /// How many of our device's reviews events the server has confirmed received.
    uploaded_count: usize,
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
            deck: None,
            uploaded_count: 0,
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

    /// Pull down events written by other devices since our last fetch.
    async fn refresh(&mut self) -> anyhow::Result<()> {
        if self.last_fetch.elapsed() < REFRESH_INTERVAL {
            return Ok(());
        }
        let (new_rows, max_id) = fetch_events(
            &self.http,
            &self.supabase,
            &self.user_id,
            self.last_fetched_id,
        )
        .await?;
        self.last_fetch = Instant::now();
        self.last_fetched_id = max_id.or(self.last_fetched_id);

        // Echoes of our own uploads confirm the server has them (the store
        // itself dedups the events).
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
        Ok(())
    }

    /// Append an event to the reviews stream and upload it (plus anything
    /// still pending from a previously failed upload).
    async fn append_event(&mut self, content: LanguageEventContent) -> anyhow::Result<()> {
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

        // Upload everything not yet confirmed, in order, so the per-device
        // index sequence on the server never has gaps.
        let pending = self
            .store
            .get_raw(REVIEWS_STREAM.to_string())
            .expect("reviews stream registered in new_store")
            .jsons(&DEVICE_ID.to_string(), self.uploaded_count);
        upload_events(
            &self.http,
            &self.supabase,
            &self.user_id,
            REVIEWS_STREAM,
            DEVICE_ID,
            &pending,
        )
        .await?;
        self.uploaded_count += pending.len();
        Ok(())
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

        let render = |sentence: &Spur| {
            let text = pack.string_rodeo.resolve(sentence);
            let translations: Vec<&str> = pack
                .translations
                .get(sentence)
                .map(|ts| ts.iter().map(|t| pack.string_rodeo.resolve(t)).collect())
                .unwrap_or_default();
            json!({
                "text": text,
                "translations": translations,
                "sources": self.sentence_sources(sentence),
            })
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
    comprehensible: Vec<serde_json::Value>,
    other: Vec<serde_json::Value>,
    total_comprehensible: usize,
    total_containing: usize,
}

fn ok_json(value: serde_json::Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&value).expect("json serializes"),
    )])
}

/// Like ok_json, but also sets MCP structured content — ChatGPT's search/fetch
/// contract wants results both ways.
fn ok_json_structured(value: serde_json::Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&value).expect("json serializes"),
    )]);
    result.structured_content = Some(value);
    result
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

/// The card shape shared by get_due_cards and unlock_cards: everything the
/// LLM needs to quiz the user and pass the card back to log_review.
fn card_summary_json(language: &serde_json::Value, summary: &CardSummary) -> serde_json::Value {
    let indicator = summary.card_indicator();
    json!({
        "language": language,
        "card": serde_json::to_value(&indicator).expect("card serializes"),
        "text": summary.card_text(),
        "subtitle": summary.card_subtitle(),
        "kind": card_kind(&indicator),
        "fsrs_state": summary.state(),
        "due": chrono::DateTime::from_timestamp_millis(summary.due_timestamp_ms() as i64)
            .map(|d| d.to_rfc3339()),
    })
}

fn card_kind(card: &CardIndicator<Gram<String>, String>) -> &'static str {
    match card {
        CardIndicator::WrittenGram { .. } => "written",
        CardIndicator::ListeningGram { .. } => "listening",
        CardIndicator::LetterPronunciation { .. } => "pronunciation",
    }
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

#[derive(Clone)]
pub struct YapMcp {
    source: StateSource,
}

impl YapMcp {
    pub fn new(state: YapState) -> Self {
        Self {
            source: StateSource::Single(Arc::new(tokio::sync::Mutex::new(state))),
        }
    }

    pub fn new_remote(app: Arc<crate::remote::RemoteApp>) -> Self {
        Self {
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
        description = "Search the yap dictionary for words and phrases. Each match includes its language and gram — the token sequence (word + lemma + part of speech) that uniquely identifies it. Other tools take these verbatim; search first rather than constructing grams by hand.",
        annotations(
            title = "Search the dictionary",
            read_only_hint = true,
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
        }
        let language = state.target_language_value();
        let deck = state.deck();
        let pack = &deck.context().language_pack;
        let entries = deck.get_gram_dictionary_entries(Some(query.clone()), limit);
        let results: Vec<serde_json::Value> = entries
            .iter()
            .map(|entry| {
                let gram = pack
                    .gram_frequencies
                    .entries
                    .get_index(entry.frequency_index())
                    .map(|(spur, _)| {
                        serde_json::to_value(pack.resolve_gram(spur)).expect("gram serializes")
                    });
                json!({
                    "language": language,
                    "gram": gram,
                    "display_text": entry.display_text(),
                    "frequency_rank": entry.frequency_index() + 1,
                    "is_phrase": entry.is_phrase(),
                    "in_deck": entry.is_in_deck(),
                    "definition": entry.definition(),
                })
            })
            .collect();
        ok_json(json!({
            "query": query,
            "results": results,
            "note": "frequency_rank 1 is the most common word in the course. Pass language + gram verbatim to add_cards or get_sentences.",
        }))
    }

    #[tool(
        title = "Add flashcards",
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
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
        if !to_add.is_empty() {
            let sentence_list = deck.current_sentence_list();
            let content = LanguageEventContent::AddCards {
                cards: to_add.into_iter().map(|(card, _)| card).collect(),
                sentence_list,
            };
            if let Err(e) = state.append_event(content).await {
                return error(format!("failed to save: {e:#}"));
            }
        }

        let deck = state.deck();
        ok_json(json!({
            "added": added,
            "already_in_deck": already_in_deck,
            "total_cards_in_deck": deck.num_cards_added(),
        }))
    }

    #[tool(
        title = "List due flashcards",
        description = "List the user's currently-due yap flashcards, most overdue first. Each entry carries its language and card object; pass both verbatim to log_review after quizzing the user. Gram cards can be quizzed with example sentences from get_sentences.",
        annotations(
            title = "List due flashcards",
            read_only_hint = true,
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
        }
        let language = state.target_language_value();
        let now_ms = Utc::now().timestamp_millis() as f64;
        let deck = state.deck();
        let due = deck.due_card_summaries(now_ms);
        let total_due = due.len();
        let locked = deck.locked_count();

        let cards: Vec<serde_json::Value> = due
            .iter()
            .take(limit)
            .map(|summary| card_summary_json(&language, summary))
            .collect();

        let mut response = json!({
            "total_due": total_due,
            "showing": cards.len(),
            "cards": cards,
            "cards_in_lockup": locked,
            "note": "kind 'written' = recognize the written word; 'listening' = normally an audio card (quiz in text as best you can); 'pronunciation' = a letter-sound pattern. A card's gram field can be passed to get_sentences.",
        });
        if locked > 0 {
            response["lockup_note"] = json!(format!(
                "{locked} cards are set aside in lockup — when a big backlog is due at once, \
                 yap keeps a manageable handful active and sets the rest aside to be practiced \
                 later. unlock_cards releases the next batch; reviewing a locked card also \
                 unlocks it."
            ));
        }
        ok_json(response)
    }

    #[tool(
        title = "Unlock cards",
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
        }
        let language = state.target_language_value();

        let (content, unlocked) = {
            let deck = state.deck();
            match deck.get_release_offer() {
                None => {
                    return ok_json(json!({
                        "unlocked": [],
                        "cards_in_lockup": 0,
                        "note": "no cards are in lockup",
                    }));
                }
                Some(offer) => {
                    let unlocked: Vec<serde_json::Value> = offer
                        .release_preview()
                        .iter()
                        .map(|summary| card_summary_json(&language, summary))
                        .collect();
                    let DeckEvent::Language(event) = offer.unlock_event();
                    (event.content, unlocked)
                }
            }
        };
        if let Err(e) = state.append_event(content).await {
            return error(format!("failed to save: {e:#}"));
        }

        let remaining = state.deck().locked_count();
        ok_json(json!({
            "unlocked": unlocked,
            "cards_in_lockup": remaining,
            "note": "these cards are back in the due queue and will show up in get_due_cards.",
        }))
    }

    #[tool(
        title = "Log a review",
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
        let mut state = slot.lock().await;
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
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
        if let Err(e) = state.append_event(content).await {
            return error(format!("failed to save review: {e:#}"));
        }

        let deck = state.deck();
        let after = deck.find_card_summary(&card);
        let remaining_due = deck
            .due_card_summaries(Utc::now().timestamp_millis() as f64)
            .len();
        ok_json(json!({
            "reviewed": before.card_text(),
            "rating": params.rating,
            "was_due": was_due,
            "fsrs_state": after.as_ref().map(|a| a.state()),
            "next_due": after.and_then(|a| {
                chrono::DateTime::from_timestamp_millis(a.due_timestamp_ms() as i64)
                    .map(|d| d.to_rfc3339())
            }),
            "remaining_due": remaining_due,
            "total_reviews": deck.stats().total_reviews,
        }))
    }

    #[tool(
        title = "Get example sentences",
        description = "Get random example sentences (with translations and source attribution) containing a given gram. Returns two lists: comprehensible_sentences, which are otherwise composed only of words the user already knows (great for quizzing), and other_sentences, sampled from everything containing the gram regardless of difficulty.",
        annotations(
            title = "Get example sentences",
            read_only_hint = true,
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
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
        ok_json(json!({
            "word": display,
            "comprehensible_sentences": sampled.comprehensible,
            "total_comprehensible": sampled.total_comprehensible,
            "other_sentences": sampled.other,
            "total_containing_word": sampled.total_containing,
            "note": "comprehensible_sentences use only words the user already knows; other_sentences are sampled from everything containing the word and may include unknown words (and may lack translations).",
        }))
    }

    #[tool(
        title = "Get learning stats",
        description = "Get the user's yap stats: streak, XP, review counts, deck size, due cards, comprehension tier, and recent daily activity.",
        annotations(
            title = "Get learning stats",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_stats(&self, ctx: RequestContext<RoleServer>) -> CallToolResult {
        let slot = match self.state_slot(&ctx).await {
            Ok(slot) => slot,
            Err(e) => return error(e),
        };
        let mut state = slot.lock().await;
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
        }
        let course = state.context.course;
        let deck = state.deck();
        let stats = deck.stats();
        let tier = deck.get_current_tier();
        let now_ms = Utc::now().timestamp_millis() as f64;
        let due = deck.due_card_summaries(now_ms).len();

        let past_days: Vec<serde_json::Value> = stats
            .past_days
            .iter()
            .rev()
            .take(7)
            .map(|(day, summary)| {
                let date = chrono::NaiveDate::from_num_days_from_ce_opt(*day as i32)
                    .map(|d| d.to_string());
                json!({
                    "date": date,
                    "reviews": summary.reviews,
                    "time_spent_seconds": summary.time_spent_seconds,
                    "new_cards": summary.new_cards,
                    "learned_cards": summary.learned_cards,
                })
            })
            .collect();

        ok_json(json!({
            "course": format!("{} for {} speakers", course.target_language, course.native_language),
            "daily_streak": deck.get_daily_streak(),
            "xp": stats.xp,
            "total_reviews": stats.total_reviews,
            "started": stats.start_time.map(|t| t.to_rfc3339()),
            "cards_in_deck": deck.num_cards_added(),
            "cards_due_now": due,
            "cards_locked_away": deck.locked_count(),
            "tier": {
                "name": tier.name,
                "tier": tier.tier,
                "level": tier.level,
                "total_levels": tier.total_levels,
                "percent_known": tier.percent_known,
            },
            "recent_days": past_days,
        }))
    }

    #[tool(
        title = "Search dictionary pages",
        description = "Search the dictionary of the user's yap course. Returns matching entry pages as results with id, title, and url (a citable yap.town dictionary link); pass an id to fetch for the full entry. For deck operations that need exact grams (add_cards, get_sentences, log_review), use search_dictionary instead.",
        annotations(
            title = "Search dictionary pages",
            read_only_hint = true,
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
        }
        let course = state.context.course;
        let results: Vec<serde_json::Value> = state
            .deck()
            .get_gram_dictionary_entries(Some(query), 10)
            .iter()
            .map(|entry| {
                json!({
                    "id": entry_id(&course, entry),
                    "title": entry_title(entry),
                    "url": entry_url(&course, &entry.display_text()),
                })
            })
            .collect();
        ok_json_structured(json!({ "results": results }))
    }

    #[tool(
        title = "Fetch a dictionary page",
        description = "Fetch a dictionary entry page by an id returned from the search tool: definitions, frequency rank, whether it's in the user's deck, and example sentences with translations and source attribution. Returns readable text plus a citable url.",
        annotations(
            title = "Fetch a dictionary page",
            read_only_hint = true,
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
        if let Err(e) = state.refresh().await {
            log::warn!("refresh failed, using possibly-stale events: {e:#}");
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
        let language = state.target_language_value();
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
                let _ = write!(
                    text,
                    "• “{}”",
                    sentence["text"].as_str().unwrap_or_default()
                );
                if let Some(translation) = sentence["translations"].get(0).and_then(|t| t.as_str())
                {
                    let _ = write!(text, " — “{translation}”");
                }
                let sources: Vec<&str> = sentence["sources"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
                    .unwrap_or_default();
                if !sources.is_empty() {
                    let _ = write!(text, " (source: {})", sources.join(", "));
                }
                let _ = writeln!(text);
            }
        }
        let _ = writeln!(text, "\nDictionary page: {url}");

        ok_json_structured(json!({
            "id": params.id,
            "title": entry_title(&entry),
            "text": text,
            "url": url,
            "metadata": {
                "language": language,
                "gram": gram_value,
                "frequency_rank": entry.frequency_index() + 1,
                "in_deck": entry.is_in_deck(),
                "is_phrase": entry.is_phrase(),
            },
        }))
    }
}

#[tool_handler]
impl ServerHandler for YapMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
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
             you need exact grams for the deck tools."
                .to_string(),
        );
        info
    }
}
