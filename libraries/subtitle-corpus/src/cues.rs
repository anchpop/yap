//! Shared subtitle-corpus film machinery: discovery, SRT/transcript parsing,
//! the transcript-verification (verbatim) gate, sampling, and clip cutting.
//!
//! Lifted from `bin/phoneme-corpus-eval.rs` so the training-clip extractor
//! (`bin/subtitle-corpus-extract.rs`) shares one implementation of the
//! selection logic instead of drifting. The eval bin's behavior is unchanged:
//! every constant and algorithm here is a verbatim move, with two purely
//! additive extensions (`Spoken.speaker` is now parsed, and `Candidate`
//! carries the span's diarized speaker).

use anyhow::{bail, Context, Result};

pub use crate::sync::{parse_cues, Cue};
pub use crate::transcript::{Kind, Spoken};
use language_utils::Language;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Inventory language names → course codes, restricted to languages whose
/// phoneme labels genuinely come from espeak.
///
/// Derived from [`Language::phoneme_label_source`] rather than hand-listed, so
/// it cannot drift from the table that defines which G2P source each language
/// is allowed to use. Hindi is excluded despite espeak having a `hi` voice —
/// its labels come from `schwa-stress-hin`, and scoring hin against espeak was
/// measurably wrong (see the enum's docs). This is the phoneme-scoring gate;
/// the clip extractor uses [`course_code_full`], since selection is
/// transcript-driven and needs no phoneme reference at all.
pub fn course_code_espeak(original_language: &str) -> Option<&'static str> {
    let code = course_code_full(original_language)?;
    let language = Language::from_code(code)?;
    language.phoneme_label_source().espeak_voice().map(|_| code)
}

/// Full mapping to pronunciation-corpus lang codes, mirroring
/// `subtitle-corpus/src/library.rs::course_dir` — except **Cantonese**, which
/// that function folds into `zho-hans`: labeling Cantonese audio with
/// Mandarin g2p would be systematically wrong, so it maps to `None` here.
/// Korean is included (`kor`) even though training has no kor config yet;
/// callers that can't use it should skip it.
pub fn course_code_full(original_language: &str) -> Option<&'static str> {
    Some(match original_language {
        "English" => "eng",
        "French" => "fra",
        "German" => "deu",
        "Spanish" => "spa",
        "Italian" => "ita",
        "Portuguese" => "por",
        "Russian" => "rus",
        "Hindi" => "hin",
        "Japanese" => "jpn",
        "Korean" => "kor",
        "Thai" => "tha",
        "Chinese" | "Mandarin" => "zho-hans",
        _ => return None,
    })
}

pub fn repair_latin_homoglyphs(text: &str) -> String {
    let latin = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let confusable = text
        .chars()
        .filter(|c| ('\u{0370}'..='\u{03FF}').contains(c) || ('\u{0400}'..='\u{04FF}').contains(c))
        .count();
    if confusable == 0 || confusable * 3 > latin.max(1) {
        return text.to_string();
    }
    text.chars()
        .map(|c| match c {
            // Greek capitals that are visual twins of Latin ones.
            'Α' => 'A',
            'Β' => 'B',
            'Ε' => 'E',
            'Ζ' => 'Z',
            'Η' => 'H',
            'Ι' => 'I',
            'Κ' => 'K',
            'Μ' => 'M',
            'Ν' => 'N',
            'Ο' => 'O',
            'Ρ' => 'P',
            'Τ' => 'T',
            'Υ' => 'Y',
            'Χ' => 'X',
            'ο' => 'o',
            'ν' => 'v',
            'ι' => 'i',
            // Cyrillic capitals likewise.
            'А' => 'A',
            'В' => 'B',
            'Е' => 'E',
            'К' => 'K',
            'М' => 'M',
            'Н' => 'H',
            'О' => 'O',
            'Р' => 'P',
            'С' => 'C',
            'Т' => 'T',
            'Х' => 'X',
            'У' => 'Y',
            'а' => 'a',
            'е' => 'e',
            'о' => 'o',
            'с' => 'c',
            'р' => 'p',
            'х' => 'x',
            other => other,
        })
        .collect()
}

/// How subtitle/transcript text is split into comparable agreement units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tokenization {
    /// Lowercased alphanumeric runs — for languages written with spaces.
    Words,
    /// Individual alphanumeric characters — for spaceless scripts
    /// (jpn/zho-hans/tha), where "word" tokens would be whole clauses and
    /// every comparison saturates. ElevenLabs transcript units for these
    /// languages are already near-character-sized, so both witnesses land
    /// in the same unit space.
    Chars,
}

/// The tokenization a pronunciation-corpus lang code needs.
pub fn tokenization_for(code: &str) -> Tokenization {
    match code {
        "jpn" | "zho-hans" | "tha" => Tokenization::Chars,
        _ => Tokenization::Words,
    }
}

/// Comparable agreement tokens under the given tokenization.
pub fn agreement_tokens(text: &str, tokenization: Tokenization) -> Vec<String> {
    match tokenization {
        Tokenization::Words => text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect(),
        Tokenization::Chars => text
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .map(String::from)
            .collect(),
    }
}

/// Edit distance of `cue` against the best-matching *contiguous run* of
/// `heard` (approximate substring matching: free start and end in `heard`).
/// The transcript window deliberately overshoots the cue span, so scoring
/// against the whole window would bill the cue for its neighbors' words —
/// the pilot mislabeled a verbatim cue as a mismatch exactly that way.
pub fn substring_edit_distance(cue: &[String], heard: &[String]) -> usize {
    let (m, n) = (cue.len(), heard.len());
    // dp[j] = min cost to match cue[..i] ending anywhere at heard[..j];
    // row 0 is all zeros (a match may start at any heard position).
    let mut prev = vec![0usize; n + 1];
    let mut cur = vec![0usize; n + 1];
    for i in 1..=m {
        cur[0] = i;
        for j in 1..=n {
            let cost = usize::from(cue[i - 1] != heard[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    // Free end: best score over all end positions.
    prev.into_iter().min().unwrap_or(m)
}

/// Plain symmetric token Levenshtein — extra tokens on either side count.
pub fn levenshtein(a: &[String], b: &[String]) -> usize {
    let n = b.len();
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0usize; n + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[n]
}

/// How far either side of a cue's span transcript words still count as "this
/// cue" — absorbs residual sync error and word-boundary rounding.
pub const MATCH_SLOP_MS: i64 = 500;
/// Speech within this margin *outside* the cue means the clip can't be cut
/// clean; recorded as a flag, not an exclusion.
pub const NEIGHBOR_MARGIN_MS: i64 = 750;
/// Padding added to each side of the audio slice sent to the model.
pub const AUDIO_PAD_MS: i64 = 150;
/// Cue length bounds — outside these, clips are degenerate or unmanageable.
pub const MIN_CUE_MS: i64 = 400;
pub const MAX_CUE_MS: i64 = 12_000;
/// WER at or below which the transcript confirms the cue.
pub const POS_WER: f64 = 0.12;
/// WER at or above which the transcript contradicts it.
pub const NEG_WER: f64 = 0.6;
/// Both labels need at least this many subtitle tokens to mean anything.
pub const MIN_TOKENS: usize = 3;
/// A film must yield at least this many verbatim positives to participate.
/// Below it, the subtitle is desynced, a different cut, or a forced-subs
/// track (The Producers sat ~60s off; Phantom Menace had a 38-cue forced
/// track) — and every "negative" it contributes is a sync artifact rather
/// than a subtitling one. Self-contained, unlike the whisper-check verdicts,
/// which can be stale relative to the current subtitle.srt.
pub const MIN_FILM_POSITIVES: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueLabel {
    /// Transcript confirms the subtitle text — spoken verbatim, and nothing
    /// *else* spoken inside the clip span (low WER both ways, no audio event).
    Pos,
    /// The subtitle's words were spoken, but the clip span carries extra
    /// speech beyond them — a condensed subtitle or a talkative neighbor.
    /// The classic subs2srs failure mode, kept as its own negative class.
    NegExtraSpeech,
    /// Transcript heard different words.
    NegMismatch,
    /// Transcript heard (nearly) nothing where the subtitle claims speech.
    NegSilent,
}

/// A cue that passed labeling.
pub struct Candidate {
    pub cue_index: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub cleaned_text: String,
    pub label: CueLabel,
    pub agreement_wer: f64,
    pub exact_wer: f64,
    pub heard_text: String,
    pub audio_event_overlap: bool,
    pub neighbor_speech: bool,
    /// The diarized speaker of the padded clip span, when every span word
    /// agrees on one; `None` for multi-speaker spans (a within-speaker
    /// analysis must not merge two voices, so ambiguity means no label).
    pub span_speaker: Option<String>,
    /// A transcript word from OUTSIDE the padded span (midpoint beyond it)
    /// still overlaps the padded audio in time — its onset/tail will bleed
    /// into the cut clip. Unlike `neighbor_speech` (a 750 ms proximity flag)
    /// this marks actual audio contamination of the clip itself.
    pub edge_bleed: bool,
}

pub fn label_cues(
    cues: &[Cue],
    transcript: &[Spoken],
    tokenization: Tokenization,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (i, cue) in cues.iter().enumerate() {
        let dur = cue.end_ms - cue.start_ms;
        if !(MIN_CUE_MS..=MAX_CUE_MS).contains(&dur) {
            continue;
        }
        let cleaned = repair_latin_homoglyphs(&movie_subtitles::cleanup_subtitle_text(&cue.text));
        // Digits poison both witnesses: the transcript writes numbers out as
        // words ("9" vs "neuf") so agreement fails spuriously, and espeak's
        // digit expansion adds its own mismatch surface. Rare enough to skip.
        if cleaned.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let cue_tokens = agreement_tokens(&cleaned, tokenization);
        if cue_tokens.len() < MIN_TOKENS {
            continue;
        }

        let lo = cue.start_ms - MATCH_SLOP_MS;
        let hi = cue.end_ms + MATCH_SLOP_MS;
        let window: Vec<&Spoken> = transcript
            .iter()
            .filter(|w| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
            .collect();
        let heard_text = window
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let heard_tokens = agreement_tokens(&heard_text, tokenization);

        let audio_event_overlap = transcript.iter().any(|w| {
            w.kind == Kind::AudioEvent && w.at_ms < cue.end_ms && w.until_ms > cue.start_ms
        });
        let neighbor_speech = transcript.iter().any(|w| {
            w.kind == Kind::Word
                && ((w.until_ms > cue.start_ms - NEIGHBOR_MARGIN_MS
                    && w.at_ms < cue.start_ms - MATCH_SLOP_MS)
                    || (w.at_ms < cue.end_ms + NEIGHBOR_MARGIN_MS
                        && w.until_ms > cue.end_ms + MATCH_SLOP_MS))
        });

        let dist = substring_edit_distance(&cue_tokens, &heard_tokens);
        let wer = dist as f64 / cue_tokens.len() as f64;

        // What the model will actually hear: the words inside the padded
        // span the audio is cut from. A cue can match the transcript as a
        // *substring* and still be a bad clip because the actor said more —
        // condensed subtitles are the norm, not the exception, in some films.
        let span_words: Vec<&Spoken> = transcript
            .iter()
            .filter(|w| {
                w.kind == Kind::Word
                    && (w.at_ms + w.until_ms) / 2 >= cue.start_ms - AUDIO_PAD_MS
                    && (w.at_ms + w.until_ms) / 2 <= cue.end_ms + AUDIO_PAD_MS
            })
            .collect();
        let span_tokens = agreement_tokens(
            &span_words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            tokenization,
        );
        let exact_dist = levenshtein(&cue_tokens, &span_tokens);
        let exact_wer = exact_dist as f64 / cue_tokens.len().max(span_tokens.len()).max(1) as f64;

        let edge_bleed = transcript.iter().any(|w| {
            w.kind == Kind::Word
                && w.at_ms < cue.end_ms + AUDIO_PAD_MS
                && w.until_ms > cue.start_ms - AUDIO_PAD_MS
                && !((w.at_ms + w.until_ms) / 2 >= cue.start_ms - AUDIO_PAD_MS
                    && (w.at_ms + w.until_ms) / 2 <= cue.end_ms + AUDIO_PAD_MS)
        });

        let mut span_speakers: Vec<&str> = span_words
            .iter()
            .filter_map(|w| w.speaker.as_deref())
            .collect();
        span_speakers.dedup();
        let span_speaker = match span_speakers.as_slice() {
            [only] => Some((*only).to_string()),
            _ => None,
        };

        let label = if wer <= POS_WER && exact_wer <= POS_WER && !audio_event_overlap {
            CueLabel::Pos
        } else if wer <= POS_WER && exact_wer >= 0.3 {
            CueLabel::NegExtraSpeech
        } else if wer >= NEG_WER && heard_tokens.len() < cue_tokens.len() / 2 {
            CueLabel::NegSilent
        } else if wer >= NEG_WER {
            CueLabel::NegMismatch
        } else {
            continue;
        };

        out.push(Candidate {
            cue_index: i,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text.clone(),
            cleaned_text: cleaned,
            label,
            agreement_wer: wer,
            exact_wer,
            heard_text,
            audio_event_overlap,
            neighbor_speech,
            span_speaker,
            edge_bleed,
        });
    }
    out
}

/// Take up to `quota` of `label`, spread evenly across the film rather than
/// clustered at the start.
pub fn sample(cands: &[Candidate], label: CueLabel, quota: usize) -> Vec<&Candidate> {
    let matching: Vec<&Candidate> = cands.iter().filter(|c| c.label == label).collect();
    if matching.len() <= quota {
        return matching;
    }
    (0..quota)
        .map(|k| matching[k * matching.len() / quota])
        .collect()
}

/// Cut `[start-pad, end+pad]` out of the film's opus audio as WAV bytes.
/// WAV (not opus) because the prediction cache is keyed on these bytes and
/// WAV output is byte-reproducible; opus is not.
///
/// ffmpeg writes to a temp FILE, not stdout: piped WAV output leaves the
/// RIFF/data chunk sizes unfinalized (ffmpeg cannot seek a pipe), which
/// strict readers (Rust `hound`, e.g. lexide's vad_compute) reject as
/// "data chunk length is not a multiple of sample size". A seekable file
/// gets its header patched on close and is equally byte-reproducible.
pub fn slice_wav(audio: &Path, start_ms: i64, end_ms: i64) -> Result<Vec<u8>> {
    slice_wav_padded(audio, start_ms, end_ms, AUDIO_PAD_MS, AUDIO_PAD_MS)
}

/// [`slice_wav`] with the padding chosen per side — so a clip can take as
/// much room as the silence around it allows and no more.
pub fn slice_wav_padded(
    audio: &Path,
    start_ms: i64,
    end_ms: i64,
    pad_before_ms: i64,
    pad_after_ms: i64,
) -> Result<Vec<u8>> {
    let start = (start_ms - pad_before_ms).max(0);
    let dur = end_ms + pad_after_ms - start;
    let tmp = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile()
        .context("creating temp wav")?;
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-ss"])
        .arg(format!("{:.3}", start as f64 / 1000.0))
        .args(["-t", &format!("{:.3}", dur as f64 / 1000.0)])
        .arg("-i")
        .arg(audio)
        .args(["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-f", "wav"])
        .arg(tmp.path())
        .status()
        .context("ffmpeg failed to start")?;
    let bytes = std::fs::read(tmp.path()).context("reading temp wav")?;
    if !status.success() || bytes.is_empty() {
        bail!(
            "ffmpeg could not cut {start_ms}..{end_ms} from {}",
            audio.display()
        );
    }
    Ok(bytes)
}

pub fn load_transcript(path: &Path) -> Result<Vec<Spoken>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        // The provenance first line isn't a Spoken and fails to parse — skipped.
        .filter_map(|l| serde_json::from_str::<Spoken>(l).ok())
        .collect())
}

/// Where a sentence sits in the transcript: the run of transcript words that
/// best matches it, and how well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceMatch {
    /// Indices into the window the sentence was aligned against, inclusive.
    pub first: usize,
    pub last: usize,
    /// Token edit distance between the sentence and that run.
    pub distance: usize,
}

/// Align a sentence's tokens to a window of transcript words and return the
/// contiguous run of words that spells it best.
///
/// The same free-start / free-end edit alignment as
/// [`substring_edit_distance`], with the path recovered so the caller gets
/// *which* words matched — and from them, when the sentence was actually
/// spoken. Sentence timing from the transcript's word stamps is what makes a
/// clip trustworthy: a cue's own times are display times, authored for
/// reading, and a cue holding two sentences has no per-sentence timing at
/// all. `None` when either side is empty.
pub fn align_sentence(sentence: &[String], heard: &[String]) -> Option<SentenceMatch> {
    let (m, n) = (sentence.len(), heard.len());
    if m == 0 || n == 0 {
        return None;
    }
    // cost[i][j]: best cost matching sentence[..i] to a run of heard ending
    // at j; start[i][j]: where that run began. Row 0 is free.
    let mut cost = vec![vec![0usize; n + 1]; m + 1];
    let mut start = vec![vec![0usize; n + 1]; m + 1];
    for (j, slot) in start[0].iter_mut().enumerate() {
        *slot = j;
    }
    for i in 1..=m {
        cost[i][0] = i;
        start[i][0] = 0;
        for j in 1..=n {
            let sub = cost[i - 1][j - 1] + usize::from(sentence[i - 1] != heard[j - 1]);
            let del = cost[i - 1][j] + 1; // sentence token unheard
            let ins = cost[i][j - 1] + 1; // extra heard token inside the run
            let (c, s) = if sub <= del && sub <= ins {
                (sub, start[i - 1][j - 1])
            } else if del <= ins {
                (del, start[i - 1][j])
            } else {
                (ins, start[i][j - 1])
            };
            cost[i][j] = c;
            start[i][j] = s;
        }
    }
    // Free end: the best-scoring end position. Ties go to the run whose
    // length is closest to the sentence's (a cheap partial match that stops
    // early costs the same as full coverage with two substitutions), then to
    // the earliest, so a sentence repeated later in the window doesn't steal
    // the match.
    let (last, distance) = (1..=n)
        .map(|j| (j, cost[m][j]))
        .min_by_key(|&(j, c)| (c, (j - start[m][j]).abs_diff(m), j))?;
    let first = start[m][last];
    if first >= last {
        return None;
    }
    Some(SentenceMatch {
        first,
        last: last - 1,
        distance,
    })
}

#[cfg(test)]
mod align_tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        agreement_tokens(s, Tokenization::Words)
    }

    #[test]
    fn finds_the_run_inside_a_longer_window() {
        let heard = toks("oui bien sûr je vous supplie de ne pas chercher à nous retrouver merci");
        let m = align_sentence(
            &toks("Je vous supplie de ne pas chercher à nous retrouver."),
            &heard,
        )
        .unwrap();
        assert_eq!((m.first, m.last, m.distance), (3, 12, 0));
    }

    #[test]
    fn tolerates_a_misheard_word_and_reports_it() {
        let heard = toks("plus haut jusqu'aux genoux oh");
        let m = align_sentence(&toks("Plus haut, jusqu'au genou !"), &heard).unwrap();
        assert_eq!((m.first, m.last), (0, 4));
        assert_eq!(m.distance, 2);
    }

    #[test]
    fn prefers_the_first_of_two_identical_runs() {
        let heard = toks("bois bois bois attends bois bois");
        let m = align_sentence(&toks("Bois ! Bois !"), &heard).unwrap();
        assert_eq!((m.first, m.last, m.distance), (0, 1, 0));
    }
}
