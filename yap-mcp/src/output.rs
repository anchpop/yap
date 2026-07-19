//! Typed tool outputs. Every tool's response is built from one of these
//! structs, and its declared MCP `outputSchema` derives from the same type
//! (schemars) — so the schema and the payload cannot drift. Doc comments
//! become schema descriptions the model can read.

use language_utils::Language;
use schemars::JsonSchema;
use serde::Serialize;
use yap_frontend_rs::{VoiceActorInfo, dictionary::GramDictionaryDefinition};

/// One card as listed by get_due_cards / unlock_cards.
#[derive(Serialize, JsonSchema)]
pub struct CardSummaryOut {
    pub language: Language,
    /// Opaque card object — pass back verbatim to log_review / present_card
    /// / present_translation.
    pub card: serde_json::Value,
    /// The card's display text (the word, or the pronunciation pattern).
    pub text: String,
    /// Extra display context, e.g. a lemma or part of speech.
    pub subtitle: Option<String>,
    /// "written" (recognize the written word), "listening" (normally an
    /// audio card), or "pronunciation" (a letter-sound pattern).
    pub kind: String,
    /// FSRS scheduling state: "New", "Learning", "Review", or "Relearning".
    pub fsrs_state: String,
    /// RFC 3339 timestamp when the card was/is due.
    pub due: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct GetDueCardsOut {
    /// Total cards due right now (may exceed the number shown).
    pub total_due: usize,
    /// How many cards are included in `cards`.
    pub showing: usize,
    pub cards: Vec<CardSummaryOut>,
    /// Cards set aside in lockup (see note).
    pub cards_in_lockup: usize,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lockup_note: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct UnlockCardsOut {
    /// Cards released back into the due queue.
    pub unlocked: Vec<CardSummaryOut>,
    /// Cards still set aside in lockup.
    pub cards_in_lockup: usize,
    pub note: String,
    /// Whether the unlock event reached the server (present only when an
    /// unlock actually happened).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
pub struct AddCardsOut {
    /// Display text of each newly added card.
    pub added: Vec<String>,
    /// Requested words that were already in the deck (not re-added).
    pub already_in_deck: Vec<String>,
    pub total_cards_in_deck: usize,
    /// Whether the change reached the server (false = queued locally).
    pub synced: bool,
}

/// One dictionary match from search_dictionary.
#[derive(Serialize, JsonSchema)]
pub struct DictionaryMatchOut {
    pub language: Language,
    /// The exact gram (word + lemma + part-of-speech token sequence)
    /// identifying this entry — opaque; pass verbatim to add_cards,
    /// get_sentences, or log_review.
    pub gram: Option<serde_json::Value>,
    pub display_text: String,
    /// 1 is the most common word in the course.
    pub frequency_rank: usize,
    pub is_phrase: bool,
    /// Whether the user's deck already has a card for this entry.
    pub in_deck: bool,
    pub definition: GramDictionaryDefinition,
}

#[derive(Serialize, JsonSchema)]
pub struct SearchDictionaryOut {
    pub query: String,
    pub results: Vec<DictionaryMatchOut>,
    pub note: String,
}

/// An example sentence with translations and provenance.
#[derive(Serialize, JsonSchema)]
pub struct SentenceOut {
    pub text: String,
    pub translations: Vec<String>,
    /// Where the sentence comes from (Anki deck, Tatoeba, a movie, …).
    pub sources: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct GetSentencesOut {
    /// Display text of the requested gram.
    pub word: String,
    /// Sentences composed only of words the user already knows.
    pub comprehensible_sentences: Vec<SentenceOut>,
    pub total_comprehensible: usize,
    /// Sampled from everything containing the word; may include unknown
    /// words and may lack translations.
    pub other_sentences: Vec<SentenceOut>,
    pub total_containing_word: usize,
    pub note: String,
}

#[derive(Serialize, JsonSchema)]
pub struct TierOut {
    pub name: String,
    /// 1-indexed tier number.
    pub tier: u32,
    /// 1-indexed level within this tier.
    pub level: u32,
    pub total_levels: u32,
    /// Percentage of this tier's words the user knows.
    pub percent_known: f64,
}

#[derive(Serialize, JsonSchema)]
pub struct DayOut {
    /// ISO date (YYYY-MM-DD).
    pub date: Option<String>,
    pub reviews: u32,
    pub time_spent_seconds: u32,
    pub new_cards: u32,
    /// Cards that graduated from Learning to Review that day.
    pub learned_cards: u32,
}

#[derive(Serialize, JsonSchema)]
pub struct GetStatsOut {
    /// e.g. "French for English speakers".
    pub course: String,
    pub daily_streak: u32,
    pub xp: f64,
    pub total_reviews: u64,
    /// RFC 3339 timestamp of the user's first review.
    pub started: Option<String>,
    pub cards_in_deck: usize,
    pub cards_due_now: usize,
    /// Cards set aside in lockup.
    pub cards_locked_away: usize,
    pub tier: TierOut,
    /// The last 7 days of activity, most recent first.
    pub recent_days: Vec<DayOut>,
}

#[derive(Serialize, JsonSchema)]
pub struct LogReviewOut {
    /// Display text of the reviewed card.
    pub reviewed: String,
    /// The rating that was recorded.
    pub rating: String,
    /// Whether the card was actually due when reviewed.
    pub was_due: bool,
    /// FSRS state after the review.
    pub fsrs_state: Option<String>,
    /// RFC 3339 timestamp when the card is next due.
    pub next_due: Option<String>,
    pub remaining_due: usize,
    pub total_reviews: u64,
    /// Whether the review reached the server (false = queued locally).
    pub synced: bool,
}

/// Per-word grade in a translation, aligned with the sentence's words.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LiteralGradeOut {
    Remembered,
    Forgot,
}

#[derive(Serialize, JsonSchema)]
pub struct GradeTranslationOut {
    /// True when the translation was graded fully correct.
    pub perfect: bool,
    /// The reference translation closest to the user's submission.
    pub correct_translation: Option<String>,
    pub encouragement: Option<String>,
    pub explanation: Option<String>,
    /// Set when AI grading failed and a heuristic fallback was used.
    pub autograding_error: Option<String>,
    /// One entry per word in the sentence; null = not graded.
    pub literal_grades: Vec<Option<LiteralGradeOut>>,
    pub phrases_remembered: Vec<String>,
    pub phrases_forgot: Vec<String>,
    pub remaining_due: usize,
    /// Whether the review reached the server (false = queued locally).
    pub synced: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct GetAudioOut {
    /// The audio clip, base64-encoded. Intended for the review widget, not
    /// for reading in conversation.
    pub audio_base64: String,
    /// e.g. "audio/mpeg" or "audio/ogg".
    pub mime_type: String,
    /// Set when the clip is a human voice-actor recording.
    pub voice_actor: Option<VoiceActorInfo>,
    /// "human_recording" or "tts".
    pub source: String,
}

/// One search hit, per the standard search/fetch contract.
#[derive(Serialize, JsonSchema)]
pub struct SearchResultOut {
    /// Pass to fetch for the full entry.
    pub id: String,
    pub title: String,
    /// Citable public dictionary URL.
    pub url: String,
}

#[derive(Serialize, JsonSchema)]
pub struct SearchOut {
    pub results: Vec<SearchResultOut>,
}

#[derive(Serialize, JsonSchema)]
pub struct FetchMetadataOut {
    pub language: Language,
    /// The exact gram identifying this entry in deck tools — opaque; pass
    /// verbatim to add_cards, get_sentences, or log_review.
    pub gram: Option<serde_json::Value>,
    pub frequency_rank: usize,
    pub in_deck: bool,
    pub is_phrase: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct FetchOut {
    pub id: String,
    pub title: String,
    /// The full entry rendered as readable text.
    pub text: String,
    /// Citable public dictionary URL.
    pub url: String,
    pub metadata: FetchMetadataOut,
}

/// Result of presenting an interactive card widget.
#[derive(Serialize, JsonSchema)]
pub struct PresentOut {
    /// The widget's render payload. Consumed by the review widget iframe,
    /// not meant to be interpreted in conversation; the widget reports the
    /// review outcome back when the user grades the card.
    pub challenge: serde_json::Value,
}
