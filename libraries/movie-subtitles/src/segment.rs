//! Sentence segmentation of subtitle tracks.
//!
//! A subtitle breaks long sentences across cues for display and packs two
//! speakers or two sentences into one cue; a sentence-level consumer (course
//! ingestion, the subtitle corpus's clip mapping) needs the sentences back.
//! Cues are regrouped into passages — one speaker turn plus the cues that
//! continue its unfinished sentence — and each passage is split by parsley's
//! sentence segmenter, which knows about abbreviations and quotes. Both
//! consumers share this one implementation so they segment identically.

use std::sync::LazyLock;

use anyhow::Context;
use language_utils::Language;
use regex::Regex;

use crate::SubtitleLine;

/// The lexide language for a course language, or `None` when lexide has no
/// pipeline for it (Traditional Chinese).
pub fn lexide_language(language: Language) -> Option<lexide::Language> {
    Some(match language {
        Language::French => lexide::Language::French,
        Language::English => lexide::Language::English,
        Language::Spanish => lexide::Language::Spanish,
        Language::Korean => lexide::Language::Korean,
        Language::German => lexide::Language::German,
        Language::Italian => lexide::Language::Italian,
        Language::Portuguese => lexide::Language::Portuguese,
        Language::Russian => lexide::Language::Russian,
        Language::Hindi => lexide::Language::Hindi,
        Language::Japanese => lexide::Language::Japanese,
        Language::ChineseSimplified => lexide::Language::ChineseSimplified,
        Language::Thai => lexide::Language::Thai,
        Language::ChineseTraditional => return None,
    })
}

/// Longest silence after an unfinished cue across which the next cue can
/// still continue its sentence.
///
/// Subtitles break sentences across cues for display, and the next cue
/// typically appears within a few hundred milliseconds — occasionally longer
/// when a line is held over a pause. Beyond this the cues are different
/// utterances.
const PASSAGE_GAP_MS: u32 = 2_000;

/// A dialogue dash that opens a speaker turn: at the start of the cue or after
/// whitespace (where a line break used to be), with or without a following
/// space. `-` before a digit is a number, and a hyphen inside a word
/// (`cache-col`) never follows whitespace, so neither is touched.
static DIALOGUE_DASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)[-–—]\s*(?P<rest>[^\s\d-])").unwrap());

/// Sentence segmentation for subtitle ingestion: the parsley segmenter when the
/// language has one, else each speaker turn taken as it is.
pub enum SubtitleSegmenter {
    Parsley {
        segmenter: Box<lexide::Segmenter>,
        language: lexide::Language,
    },
    PerCue,
}

impl SubtitleSegmenter {
    /// The segmenter for `language`, loading parsley's weights from the HF
    /// cache (or `LEXIDE_MODEL_DIR`) on first use.
    pub fn for_language(language: Language) -> anyhow::Result<Self> {
        Ok(match lexide_language(language) {
            Some(lexide_language) => Self::Parsley {
                segmenter: Box::new(
                    lexide::Segmenter::from_pretrained()
                        .context("loading the parsley sentence segmenter")?,
                ),
                language: lexide_language,
            },
            None => Self::PerCue,
        })
    }

    /// Split one passage into sentences.
    ///
    /// The segmenter is trained on prose and can return nothing at all for a
    /// very short passage (`Calme-toi!`), or leave a stretch of the passage
    /// outside every sentence. Whatever it doesn't claim is kept as a
    /// sentence of its own rather than lost: the passage builder already
    /// guarantees a passage holds text that belongs together.
    pub fn segment(&self, passage: &str) -> Vec<String> {
        match self {
            Self::Parsley {
                segmenter,
                language,
            } => {
                // The model costs ~1M multiply-adds per byte; most passages
                // are one short sentence with no boundary to find. Only run
                // it when a terminal mark sits somewhere before the end.
                if !has_internal_boundary(passage) {
                    return vec![passage.trim().to_string()];
                }
                let chars: Vec<char> = passage.chars().collect();
                let mut out = Vec::new();
                let mut cursor = 0;
                let claim_gap = |out: &mut Vec<String>, from: usize, to: usize| {
                    let gap: String = chars[from..to].iter().collect();
                    if gap.chars().any(char::is_alphanumeric) {
                        out.push(gap.trim().to_string());
                    }
                };
                for sentence in segmenter.segment_detailed(passage, Some(*language)) {
                    if sentence.start > cursor {
                        claim_gap(&mut out, cursor, sentence.start);
                    }
                    out.push(sentence.text);
                    cursor = sentence.end.max(cursor);
                }
                if cursor < chars.len() {
                    claim_gap(&mut out, cursor, chars.len());
                }
                out
            }
            Self::PerCue => passage.lines().map(str::to_string).collect(),
        }
    }
}

/// Could this passage hold more than one sentence? True when a terminal mark
/// (`.!?…`) is followed by further text — `M. Godefroy !` included, which is
/// exactly the case the segmenter is for.
fn has_internal_boundary(passage: &str) -> bool {
    let trimmed = passage.trim_end();
    let body = trimmed.trim_end_matches(['.', '!', '?', '…', '"', '»', ')']);
    body.contains(['.', '!', '?', '…'])
}

/// Split one cue into speaker turns at its dialogue dashes.
///
/// `- Where? - Nobody!` and `-Ah, d'accord.` both mark a change of speaker;
/// the dash itself is dropped so the turn reads as a plain sentence.
fn speaker_turns(cue: &str) -> Vec<String> {
    let marked = DIALOGUE_DASH.replace_all(cue, "\n$rest");
    marked
        .split('\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Regroup cues into passages — the units handed to the segmenter.
///
/// A passage is one speaker turn plus any following cues that continue its
/// unfinished sentence (see the rule in the body). Passages are deliberately
/// small: the segmenter is trained on prose, and given a whole scene of
/// staccato dialogue it will happily keep `Si. Non !` as one sentence, so it
/// is only ever asked to split what a single turn contains.
pub fn subtitle_passages(subtitles: &[SubtitleLine]) -> Vec<String> {
    timed_passages(subtitles)
        .into_iter()
        .map(|p| p.text)
        .collect()
}

/// A passage with the time span of the cues it was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// [`subtitle_passages`], keeping each passage's cue timing — a consumer
/// that goes back to the audio (the subtitle corpus's clip mapping) needs
/// to know roughly *when* a passage was spoken.
pub fn timed_passages(subtitles: &[SubtitleLine]) -> Vec<Passage> {
    let mut passages = Vec::new();
    let mut passage = Passage {
        text: String::new(),
        start_ms: 0,
        end_ms: 0,
    };
    let mut flush = |passage: &mut Passage| {
        if !passage.text.is_empty() {
            passages.push(std::mem::replace(
                passage,
                Passage {
                    text: String::new(),
                    start_ms: 0,
                    end_ms: 0,
                },
            ));
        }
    };

    let mut prev_end_ms = 0u32;
    for cue in subtitles {
        for (t, turn) in speaker_turns(&cue.sentence).into_iter().enumerate() {
            // A turn continues the open passage only when it reads as the
            // rest of an unfinished sentence: same speaker (first turn of the
            // cue), soon after, the passage not yet closed by punctuation, and
            // the turn not opening a new sentence with a capital or a digit.
            // Everything else — a closed sentence, a long pause, a new speaker,
            // an unpunctuated caption followed by a capitalised line — starts
            // its own passage, so the segmenter only ever sees text that
            // belongs together.
            let continues = t == 0
                && !passage.text.is_empty()
                && !passage.text.ends_with(['.', '!', '?', '…'])
                && cue.start_ms.saturating_sub(prev_end_ms) <= PASSAGE_GAP_MS
                && turn
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_uppercase() && !c.is_numeric());
            if continues {
                passage.text.push(' ');
            } else {
                flush(&mut passage);
                passage.start_ms = cue.start_ms;
            }
            passage.text.push_str(&turn);
            passage.end_ms = cue.end_ms;
        }
        prev_end_ms = cue.end_ms;
    }
    flush(&mut passage);
    passages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(sentence: &str, start_ms: u32, end_ms: u32) -> SubtitleLine {
        SubtitleLine {
            sentence: sentence.to_string(),
            start_ms,
            end_ms,
        }
    }

    #[test]
    fn dialogue_dashes_split_speaker_turns() {
        assert_eq!(
            speaker_turns("- Where? - Nobody!"),
            vec!["Where?", "Nobody!"]
        );
        assert_eq!(
            speaker_turns("-Ah, d'accord. -Bon."),
            vec!["Ah, d'accord.", "Bon."]
        );
        // Hyphenated words and negative numbers are not dashes.
        assert_eq!(
            speaker_turns("Roger, ton cache-col. Il fait -5."),
            vec!["Roger, ton cache-col. Il fait -5."]
        );
    }

    #[test]
    fn unfinished_cues_are_rejoined_into_one_passage() {
        let cues = [
            cue("Il était entouré de vaillants chevaliers", 1_000, 2_500),
            cue("qui croyaient en Dieu et aux forces du Mal.", 2_800, 4_000),
            cue("Cris en allemand", 4_100, 5_000),
            cue("A genoux !", 5_200, 6_000),
            cue("Si tu n'as point de peur,", 20_000, 21_000),
            cue("bois.", 21_100, 22_000),
        ];
        assert_eq!(
            subtitle_passages(&cues),
            vec![
                "Il était entouré de vaillants chevaliers qui croyaient en Dieu et aux forces du Mal.",
                // An unpunctuated caption followed by a capitalised line is
                // two passages, not one glued sentence.
                "Cris en allemand",
                "A genoux !",
                "Si tu n'as point de peur, bois.",
            ]
        );
    }

    #[test]
    fn a_long_pause_ends_the_passage() {
        let cues = [
            cue("Mais je veux bien vous donner votre chance,", 0, 1_000),
            cue("mais va falloir bosser.", 1_000 + PASSAGE_GAP_MS + 1, 5_000),
        ];
        assert_eq!(subtitle_passages(&cues).len(), 2);
    }

    #[test]
    fn a_new_speaker_never_continues_the_previous_turn() {
        let cues = [
            cue("Je veux bien vous donner votre chance,", 0, 1_000),
            cue("- mais va falloir bosser. - oui.", 1_100, 2_000),
        ];
        assert_eq!(
            subtitle_passages(&cues),
            vec![
                "Je veux bien vous donner votre chance, mais va falloir bosser.",
                "oui."
            ]
        );
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::has_internal_boundary;

    #[test]
    fn only_passages_with_a_mark_before_the_end_need_the_model() {
        assert!(!has_internal_boundary("Calme-toi!"));
        assert!(!has_internal_boundary("Où suis-je ?!"));
        assert!(!has_internal_boundary(
            "Il était entouré de vaillants chevaliers"
        ));
        assert!(has_internal_boundary("Si. Non !"));
        assert!(has_internal_boundary("M. Godefroy !"));
        assert!(has_internal_boundary("Alors… C’est pas vrai ça !"));
    }
}
