//! Sentence → clip mapping: for every sentence a film's subtitle contains,
//! the span of audio in which it is spoken, verified twice.
//!
//! A sentence earns a clip only when two independent witnesses agree with
//! the subtitle:
//!
//! 1. **The transcript.** The sentence's tokens are aligned to the full-film
//!    word-timed transcript ([`cues::align_sentence`]); the run of words that
//!    spells it must do so near-verbatim, sit clear of neighbouring speech and
//!    of any audio event, and its word stamps give the clip its bounds. Cue
//!    times are display times, authored for reading; word stamps come from
//!    something that heard the speech, and a cue holding two sentences has no
//!    per-sentence timing at all. The cut's edges and its clear margins come
//!    from the film's earshot speech profile, not the stamps: ElevenLabs
//!    stretches a word's end stamp up to the next onset and fabricates
//!    stamps for words it inferred from context, so a boundary built on a
//!    stamp cuts off real speech or reports a gap that is not there.
//! 2. **The phoneme model.** The clip is cut and the model's per-frame
//!    distribution fetched ([`phoneme_verify::frame_matrix`]); the espeak
//!    rendering of the sentence is scored under CTC against the model's own
//!    free reading, and its first and last phonemes must be found in the
//!    audio under forced alignment ([`phoneme_verify::FrameMatrix::force_align`])
//!    — the direct test that the *whole* sentence is inside the cut. Given
//!    (1) the words are right, so what the ratio rejects beyond that is
//!    audio a listener can't actually make out — music beds, mumbling,
//!    heavy compression — which a transcription model is too robust to
//!    notice.
//!
//! Sentences are the ones course ingestion would produce from the same
//! subtitle (`movie_subtitles::segment`), keyed the same way, so a language
//! pack can look its sentences' clips up by text — independent of *which*
//! subtitle file a sentence came from.
//!
//! One `clips.jsonl` per film beside its transcript: a provenance line, then
//! one [`Clip`] per sentence that reached the phoneme gate, passing or not,
//! with every score kept so the gate can be re-tuned from the file alone.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use language_utils::Language;
use movie_subtitles::segment::SubtitleSegmenter;
use movie_subtitles::sentences::KeyedSentence;
use movie_subtitles::{cleanup_subtitle_text, SubtitleLine};
use phoneme_verify::{FrameMatrix, VerifyContext};
use serde::{Deserialize, Serialize};

use crate::cues::{
    agreement_tokens, align_sentence, load_transcript, parse_cues, slice_wav_padded,
    tokenization_for, AUDIO_PAD_MS, MATCH_SLOP_MS, MAX_CUE_MS, MIN_CUE_MS, MIN_TOKENS, POS_WER,
};
use crate::library::{course_dir, read_plan, Movie};
use crate::transcript::{Kind, Spoken};

/// Bump when the record format or the gating logic changes in a way that
/// makes existing `clips.jsonl` files not comparable.
const FORMAT_VERSION: u32 = 10;

/// How late earshot flags speech after it begins. Measured 2026-09-02 on
/// four films: with the profile allowed to trim inside the stamped words
/// at threshold 0.7, the start moved a median 60 ms later than the stamp
/// and the end 100 ms earlier, and the clips lost their first consonants
/// (edge and ratio rejects up, net yield down 7%). So the profile only
/// ever moves a boundary *outward*, and when it does — a squeezed or late
/// stamp — the onset is placed this much before the frame that first read
/// as speech.
const ONSET_LAG_MS: i64 = 80;

/// Where no neighbouring word bounds the search, how far from the stamp
/// the silence may be looked for.
const OPEN_SEARCH_MS: i64 = 2_000;

/// How many phonemes at each end of the target the edge check averages.
const EDGE_PHONEMES: usize = 3;

/// Target lead-in before the first word, clear margin permitting: a small
/// quiet lets the listener settle into the scene before the sentence
/// starts. The tail keeps the shorter [`AUDIO_PAD_MS`].
const LEAD_IN_MS: i64 = 300;

/// What a film's `clips.jsonl` was computed from. A film is redone when any
/// of it changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub format: u32,
    pub subtitle_digest: String,
    pub transcript_digest: String,
    /// The phoneme model's cache partition (model revision + decoder), or
    /// "none" for an [`audio_only`] language.
    pub model: String,
    /// Identity of the G2P backend that rendered `target_ipa` (today the
    /// espeak fork's binary digest for every gated language; other backends
    /// per lexide's PHONEME_BACKENDS.md stamp here when their languages get
    /// gates). A different phonemizer can never pose as current provenance.
    pub g2p: String,
    /// Which segmentation produced the sentences
    /// ([`movie_subtitles::segment::provenance`]): a new prompt or model on
    /// the model-segmented languages, or a bump of the rules, remaps.
    pub segmentation: String,
    pub language: String,
    /// The gate the verdicts were made under. Frame matrices are cached, so
    /// re-gating under a new cut is cheap — and must happen, or a loosened
    /// cut would leave old verdicts standing. `None` for an [`audio_only`]
    /// language: no phoneme gate at all.
    pub min_ratio: Option<f64>,
    pub min_clear_ms: i64,
    pub min_edge_logp: f64,
    pub max_pad_speech: f64,
    pub max_lead_rms: f64,
    pub min_voiced: f64,
    /// earshot score above which a 16 ms frame counts as speech when the
    /// clear margins are measured from the film's speech profile.
    pub speech_threshold: f64,
}

/// One transcript word inside a clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipWord {
    pub text: String,
    pub at_ms: i64,
    pub until_ms: i64,
}

/// A sentence and the audio span it was spoken in, with both witnesses'
/// verdicts. `passed` is the mapping's answer; everything else is why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub sentence: String,
    pub imdb_id: String,
    /// Clip bounds from the transcript's word stamps (unpadded).
    pub start_ms: i64,
    pub end_ms: i64,
    /// Padding the scored cut used on each side: up to [`LEAD_IN_MS`] before
    /// and [`AUDIO_PAD_MS`] after, but never more than half the silence to
    /// the neighbouring speech, so the cut carries no one else's onset or
    /// tail. Cut the same way to hear exactly what was scored.
    pub pad_before_ms: i64,
    pub pad_after_ms: i64,
    /// How far the profile moved each bound from the boundary word's stamp
    /// (positive = widened past the stamp, negative = trimmed inside it);
    /// `start_ms`/`end_ms` already include it.
    pub repaired_before_ms: i64,
    pub repaired_after_ms: i64,
    pub words: Vec<ClipWord>,
    /// Diarized speaker when every word in the span agrees on one.
    pub speaker: Option<String>,
    /// Token edit distance between sentence and span, over sentence tokens.
    pub transcript_wer: f64,
    pub audio_event_overlap: bool,
    /// Silence between the span and the nearest other speech, either side.
    pub clear_before_ms: i64,
    pub clear_after_ms: i64,
    /// espeak's rendering of the sentence — the CTC target.
    pub target_ipa: Vec<String>,
    /// Target tokens the model has no row for (the score is of the target
    /// without them).
    pub oov: Vec<String>,
    /// Log-odds per phoneme of the target against the model's free reading
    /// (0 = the target is what the model would have said).
    pub ratio: Option<f64>,
    pub logp_target_per_phoneme: Option<f64>,
    /// Forced-alignment mean log-prob of the first/last [`EDGE_PHONEMES`]
    /// target phonemes — very low when the clip is missing the sentence's
    /// start or end, which the whole-sentence ratio forgives on a long
    /// sentence.
    pub edge_logp_start: Option<f64>,
    pub edge_logp_end: Option<f64>,
    /// Fraction of the lead-in/tail pad the phoneme model hears as speech
    /// ([`phoneme_verify::FrameMatrix::speech_fraction`]) — voices the
    /// transcript never wrote down still show up here.
    pub lead_speech: Option<f64>,
    pub tail_speech: Option<f64>,
    /// RMS of the lead-in over RMS of the spoken span: how loud the clip
    /// opens relative to its own dialogue (< 1 = quieter).
    pub lead_rms: Option<f64>,
    /// Voiced fraction of the spoken span ([`voiced_fraction`]) — near zero
    /// for whispered delivery.
    pub voiced: Option<f64>,
    /// The model's free reading, for display.
    pub heard_ipa: Vec<String>,
    pub passed: bool,
    pub reject: Option<String>,
}

/// Per-film summary printed as films complete.
#[derive(Debug, Default, Clone, Copy)]
pub struct FilmSummary {
    pub sentences: usize,
    pub aligned: usize,
    pub scored: usize,
    pub passed: usize,
    /// Median phoneme ratio of the scored clips, when there is a phoneme
    /// gate. A film whose median sits far below the cut is not speaking
    /// the language the targets were rendered in — a Cantonese track under
    /// Mandarin subtitles, a dub — however well its subtitle placed.
    pub median_ratio: Option<f64>,
}

/// Below this median ratio a film is flagged as not sounding like its
/// course language. Mandarin films run −1 to −2; the Cantonese ones that
/// slipped through in 2026-09-03 sat at −3.3 to −3.8.
pub const FOREIGN_AUDIO_RATIO: f64 = -3.0;

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    Some(values[values.len() / 2])
}

/// Gate settings.
#[derive(Debug, Clone)]
pub struct Gate {
    /// Lowest CTC log-odds ratio a clip may have and still pass; `None`
    /// takes the per-language default from [`default_min_ratio`].
    pub min_ratio: Option<f64>,
    /// Silence required on each side of the span. The pad shrinks to fit
    /// whatever silence there is; below this there is no room for a clean
    /// cut at all — word stamps lag true onsets by tens of ms, so a cut
    /// into a smaller gap clips the first consonant or carries a
    /// neighbour's tail. Blind-ASR adjudication (2026-08-30) put margins of
    /// 100–200 ms at 90–94% clean — as clean as the pass pool — and only
    /// margins under 100 ms meaningfully worse, so 100 is the line.
    pub min_clear_ms: i64,
    /// Lowest forced-alignment mean log-prob the first/last
    /// [`EDGE_PHONEMES`] may have — the test that the sentence's start and
    /// end are actually inside the cut.
    pub min_edge_logp: f64,
    /// Most of the lead-in/tail pad the phoneme model may hear as speech —
    /// the voice-activity gate, run on the frame matrix itself rather than
    /// an energy VAD (earshot's levels proved film-mix-dependent: gaps sat
    /// at 0.3–0.4 against 0.7 during speech, with per-film baselines apart
    /// by 2×).
    pub max_pad_speech: f64,
    /// Loudest the lead-in may be relative to the spoken span (RMS ratio).
    /// 1.0 only rejects clips that open *louder* than their own dialogue.
    pub max_lead_rms: f64,
    /// Least voiced the spoken span may be — the whisper gate. The phoneme
    /// model, loudness, and ASR all comprehend whispers fine; only the
    /// missing glottal periodicity gives them away.
    pub min_voiced: f64,
    /// Least share of a film's eligible sentences its subtitle must place
    /// in the transcript ([`crate::verbatim`]) before the film is mapped at
    /// all. A rewrite or another-variety translation is not a source of
    /// sentences for this audio, however well synced.
    pub min_verbatim: Option<f64>,
    /// earshot score above which a 16 ms frame of the film's speech profile
    /// counts as speech. The clear margins are measured from the profile,
    /// not the transcript's word stamps: ElevenLabs stretches a word's end
    /// stamp up to the next onset, so stamped gaps cluster at 30–60 ms
    /// whatever the audio holds. On 120 stamp-rejected clips both earshot
    /// and the phoneme model heard a ≥100 ms pause in ~75% (2026-09-02);
    /// 0.7 matched the phoneme model on known-silent controls (92% vs 90%)
    /// where 0.5 called a quarter of them speech.
    pub speech_threshold: f64,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            min_verbatim: None,
            speech_threshold: 0.7,
            min_ratio: None,
            min_clear_ms: 100,
            min_edge_logp: -4.0,
            // An ear test (2026-08-30) found rejects at these thresholds
            // are often fine clips — but a bad clip in the deck costs far
            // more than a missed good one, so the strict settings stand.
            max_pad_speech: 0.25,
            max_lead_rms: 1.0,
            min_voiced: 0.25,
        }
    }
}

/// The CTC cut per language, from `phoneme-corpus-eval` on transcript-
/// verified vs transcript-rejected cues (model edcbbbf43a7f, 2026-08-29),
/// choosing the loosest cut that still keeps ≤ ~5–10% of rejected cues —
/// here the transcript has already thrown out the wrong-words cases, so
/// what the cut trades is yield against audio the model itself finds hard
/// to make out.
///
/// | lang | AUC  | cut  | verified kept | rejected kept |
/// |------|------|------|---------------|---------------|
/// | spa  | 0.95 | −1.0 | 65%           | 3%            |
/// | fra  | 0.93 | −1.0 | 62%           | 3%            |
/// | ita  | 0.92 | −1.0 | 56%           | 5%            |
/// | eng  | 0.82 | −1.0 | 50%           | 14%           |
/// | rus  | 0.93 | −1.0 | 58%           | 5%            |
/// | deu  | 0.89 | −1.5 | 72%           | 9%            |
/// | por  | 0.84 | −1.5 | 52%           | 8%            |
///
/// Those cuts were then loosened by 1.0 after a blind-ASR adjudication of a
/// stratified 180-clip sample (Whisper large-v3 + Gemini 3.1 Pro,
/// 2026-08-30): among *transcript-verified* clips the −1.5…−1 band was as
/// clean as the band that passed (~80–90% exact), while real defects —
/// mostly boundary words missing from the cut, now caught structurally by
/// the squeeze repair and the edge check — concentrated below −2. The
/// ratio's remaining job is audio nobody can make out (music beds,
/// mumbling, heavy compression), which is where it separates.
///
/// Russian first measured AUC 0.70 with verified cues at a median of −4.5:
/// the espeak parser was emitting palatalization `ʲ` as its own token
/// where the model's labels bind it to the consonant (`tʲ`), so every soft
/// consonant was a mismatch. With the parser fixed it behaves like French.
/// The g2p-backed languages were audited the same way once their label
/// chains were ported (2026-09-03, same model, `phoneme_verify::model_target`
/// targets; Hindi had measured 0.77 against espeak, the wrong reference):
///
/// | lang     | AUC  | cut  | verified kept | rejected kept |
/// |----------|------|------|---------------|---------------|
/// | hin      | 0.86 | −2.0 | 71%           | 13%           |
/// | jpn      | 0.87 | −1.5 | 68%           | 11%           |
/// | zho-hans | 0.81 | −2.0 | 61%           | 15%           |
/// | tha      | 0.88 | −2.5 | 70%           | 8%            |
///
/// These cuts keep rejected cues at or under the 14% English tolerates and
/// have had no ear-test adjudication. Thai's verified cues sit a full point
/// lower than the others (median −2.05): the model hears colloquial r as l
/// and never emits the glottal stops its labels carry, so its cut is looser
/// by design. Korean has no cut at all: see [`audio_only`]. Thai's backend
/// runs through `uv`, which must be on PATH or the preflight above fails
/// the film.
pub fn default_min_ratio(code: &str) -> Option<f64> {
    Some(match code {
        "spa" | "fra" | "ita" | "eng" | "rus" | "hin" | "zho-hans" => -2.0,
        "deu" | "por" | "tha" => -2.5,
        "jpn" => -1.5,
        _ => return None,
    })
}

/// Languages mapped without a phoneme gate: the clip passes on the audio
/// gates alone (a clean pause each side from the earshot profile, no
/// voices in the pads, a lead-in no louder than the dialogue, not
/// whispered). Korean's phoneme labels come from the g2p crate now, but the
/// deployed pronunciation model was never trained on Korean, so scoring
/// against it would be scoring against a model that never heard the
/// language. Andre's call (2026-09-03): ship Korean clips on the audio
/// gates and turn the phoneme gate on once a model trained on the g2p-kor
/// labels exists.
pub fn audio_only(code: &str) -> bool {
    code == "kor"
}

/// Whether `clips` maps this language at all — with a phoneme gate or
/// [`audio_only`].
pub fn maps(code: &str) -> bool {
    default_min_ratio(code).is_some() || audio_only(code)
}

/// The 16-bit mono samples of a RIFF wav as `slice_wav_padded` emits it.
fn wav_samples(wav: &[u8]) -> Option<Vec<i16>> {
    let mut at = 12; // past "RIFF<len>WAVE"
    while at + 8 <= wav.len() {
        let len = u32::from_le_bytes(wav.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        if &wav[at..at + 4] == b"data" {
            let data = wav.get(at + 8..at + 8 + len)?;
            return Some(
                data.chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect(),
            );
        }
        at += 8 + len + len % 2;
    }
    None
}

/// Fraction of speech-active frames with a periodic (voiced) signal:
/// a normalized autocorrelation peak in the 60–400 Hz pitch range. Whispered
/// speech has no glottal vibration, so it scores near zero however loud it
/// is — the signal loudness measures can't see. Calibrated on the ear test
/// (2026-08-30): whispered verdicts sat at a median 0.47 against 0.71 for
/// good clips, with only whispers and the softest speech below ~0.25.
fn voiced_fraction(samples: &[i16], sample_rate: usize) -> Option<f64> {
    let win = sample_rate * 30 / 1000;
    let hop = sample_rate * 10 / 1000;
    let (lag_lo, lag_hi) = (sample_rate / 400, sample_rate / 60);
    if samples.len() < win * 2 {
        return None;
    }
    let frames: Vec<&[i16]> = (0..)
        .map(|i| i * hop)
        .take_while(|&i| i + win <= samples.len())
        .map(|i| &samples[i..i + win])
        .collect();
    let energies: Vec<f64> = frames.iter().map(|f| rms(f)).collect();
    let mut sorted = energies.clone();
    sorted.sort_by(f64::total_cmp);
    // Only frames carrying speech count; the floor is relative to the
    // clip's own loud frames so quiet mixes are not all "inactive".
    let floor = sorted[sorted.len() * 8 / 10] * 0.25;
    let (mut voiced, mut active) = (0usize, 0usize);
    for (frame, energy) in frames.iter().zip(&energies) {
        if *energy < floor || *energy == 0.0 {
            continue;
        }
        active += 1;
        let mean = frame.iter().map(|&s| f64::from(s)).sum::<f64>() / frame.len() as f64;
        let x: Vec<f64> = frame.iter().map(|&s| f64::from(s) - mean).collect();
        let ac0: f64 = x.iter().map(|v| v * v).sum();
        if ac0 <= 0.0 {
            continue;
        }
        let peak = (lag_lo..lag_hi.min(x.len()))
            .map(|lag| {
                x[..x.len() - lag]
                    .iter()
                    .zip(&x[lag..])
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
            })
            .fold(f64::NEG_INFINITY, f64::max);
        if peak / ac0 > 0.45 {
            voiced += 1;
        }
    }
    (active > 0).then(|| voiced as f64 / active as f64)
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sq: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sq / samples.len() as f64).sqrt()
}

pub fn clips_path(dir: &Path) -> PathBuf {
    dir.join("clips.jsonl")
}

fn stored_provenance(path: &Path) -> Option<Provenance> {
    let file = std::fs::File::open(path).ok()?;
    let mut first = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(file), &mut first).ok()?;
    serde_json::from_str(&first).ok()
}

/// Every clip in a film's `clips.jsonl` (the provenance line is skipped).
pub fn read_clips(path: &Path) -> Result<Vec<Clip>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str::<Clip>(l).ok())
        .collect())
}

/// The sentences of a subtitle track, keyed exactly as course ingestion keys
/// them ([`movie_subtitles::sentences::keyed_sentences`] — one shared
/// implementation, so a pack sentence and its clip agree byte-for-byte).
/// All sentences are returned, course-worthy or not; the flag rides along.
pub async fn subtitle_sentences(
    srt: &str,
    language: Language,
    segmenter: &SubtitleSegmenter,
) -> Result<Vec<KeyedSentence>> {
    movie_subtitles::sentences::keyed_sentences(&subtitle_lines(srt), language, segmenter).await
}

/// A subtitle text as the cleaned cue lines segmentation starts from.
pub fn subtitle_lines(srt: &str) -> Vec<SubtitleLine> {
    parse_cues(srt)
        .into_iter()
        .filter_map(|cue| {
            let text = cleanup_subtitle_text(&cue.text);
            (!text.is_empty()).then_some(SubtitleLine {
                sentence: text,
                start_ms: cue.start_ms.max(0) as u32,
                end_ms: cue.end_ms.max(0) as u32,
            })
        })
        .collect()
}

/// A sentence's place in the transcript, or why it has none.
pub struct Placed {
    pub words: Vec<ClipWord>,
    pub speaker: Option<String>,
    pub wer: f64,
    pub audio_event_overlap: bool,
    /// Silence between the span and the nearest other word *by the stamps*
    /// — the transcript's opinion, kept for comparison; the cut is made
    /// from the speech profile.
    pub clear_before_ms: i64,
    pub clear_after_ms: i64,
    /// Where the neighbouring words begin and end: the silence search
    /// never reaches into them.
    pub prev_word_start_ms: Option<i64>,
    pub next_word_end_ms: Option<i64>,
}

pub fn place(
    sentence: &str,
    passage_start: i64,
    passage_end: i64,
    transcript: &[Spoken],
    code: &str,
) -> std::result::Result<Placed, &'static str> {
    let tokenization = tokenization_for(code);
    let tokens = agreement_tokens(sentence, tokenization);
    if tokens.len() < MIN_TOKENS {
        return Err("too short");
    }
    // Digits poison both witnesses: the transcript spells numbers out and
    // espeak expands them its own way.
    if sentence.chars().any(|c| c.is_ascii_digit()) {
        return Err("contains a digit");
    }
    let (lo, hi) = (passage_start - MATCH_SLOP_MS, passage_end + MATCH_SLOP_MS);
    let window: Vec<(usize, &Spoken)> = transcript
        .iter()
        .enumerate()
        .filter(|(_, w)| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
        .collect();
    let heard: Vec<String> = window
        .iter()
        .flat_map(|(_, w)| agreement_tokens(&w.text, tokenization))
        .collect();
    // Tokens and words are not 1:1 under `Chars` tokenization (or when a
    // transcript word holds punctuation-split pieces), so align on tokens and
    // map back to words through each word's token count.
    let word_of_token: Vec<usize> = window
        .iter()
        .enumerate()
        .flat_map(|(wi, (_, w))| {
            std::iter::repeat_n(wi, agreement_tokens(&w.text, tokenization).len())
        })
        .collect();
    let m = align_sentence(&tokens, &heard).ok_or("nothing heard")?;
    let wer = m.distance as f64 / tokens.len() as f64;
    if wer > POS_WER {
        return Err("transcript disagrees");
    }
    let (first_word, last_word) = (word_of_token[m.first], word_of_token[m.last]);
    let (first_idx, last_idx) = (window[first_word].0, window[last_word].0);
    let span = &transcript[first_idx..=last_idx];
    let start_ms = span[0].at_ms;
    let end_ms = span[span.len() - 1].until_ms;
    if !(MIN_CUE_MS..=MAX_CUE_MS).contains(&(end_ms - start_ms)) {
        return Err("span length out of bounds");
    }
    let audio_event_overlap = transcript
        .iter()
        .any(|w| w.kind == Kind::AudioEvent && w.at_ms < end_ms && w.until_ms > start_ms);
    // The nearest *spoken* neighbour on each side. For the character-
    // tokenized languages the transcript emits punctuation as its own
    // zero-length word sharing the previous word's stamps ("。" ending
    // exactly where the sentence ends), which as a neighbour would leave
    // the silence search no window at all — every Japanese sentence then
    // read as "too close" while the profile held a second of quiet.
    let spoken =
        |w: &&Spoken| w.kind == Kind::Word && !agreement_tokens(&w.text, tokenization).is_empty();
    let prev = transcript[..first_idx]
        .iter()
        .rev()
        .filter(spoken)
        .find(|w| w.at_ms < start_ms);
    let next = transcript[last_idx + 1..]
        .iter()
        .filter(spoken)
        .find(|w| w.until_ms > end_ms);
    let before = prev.map_or(i64::MAX, |w| start_ms - w.until_ms);
    let after = next.map_or(i64::MAX, |w| w.at_ms - end_ms);
    let mut speakers: Vec<&str> = span.iter().filter_map(|w| w.speaker.as_deref()).collect();
    speakers.dedup();
    Ok(Placed {
        words: span
            .iter()
            .filter(|w| w.kind == Kind::Word)
            .map(|w| ClipWord {
                text: w.text.clone(),
                at_ms: w.at_ms,
                until_ms: w.until_ms,
            })
            .collect(),
        speaker: match speakers.as_slice() {
            [only] => Some((*only).to_string()),
            _ => None,
        },
        wer,
        audio_event_overlap,
        clear_before_ms: before,
        clear_after_ms: after,
        prev_word_start_ms: prev.map(|w| w.at_ms),
        next_word_end_ms: next.map(|w| w.until_ms),
    })
}

/// The phoneme gate's verdict on a scored clip: the target must be in the
/// model's vocabulary, score above the language's cut, and have both its
/// edges inside the cut.
fn phoneme_reject(clip: &Clip, min_ratio: f64, gate: &Gate) -> Option<String> {
    match clip.ratio {
        _ if !clip.oov.is_empty() => Some(format!(
            "target phonemes outside the model vocabulary: {}",
            clip.oov.join(" ")
        )),
        None => Some("target could not be scored".into()),
        Some(r) if r < min_ratio => Some(format!("ratio {r:.2} below {min_ratio:.2}")),
        _ if clip.edge_logp_start.is_none_or(|e| e < gate.min_edge_logp) => {
            Some(match clip.edge_logp_start {
                Some(e) => format!("sentence start not in the cut (edge logp {e:.2})"),
                None => "target could not be aligned".into(),
            })
        }
        _ if clip.edge_logp_end.is_some_and(|e| e < gate.min_edge_logp) => Some(format!(
            "sentence end not in the cut (edge logp {:.2})",
            clip.edge_logp_end.unwrap()
        )),
        _ => None,
    }
}

/// The verdict of the gates that listen to the audio itself, phoneme model
/// or not: voices in the pads, a lead-in louder than the dialogue,
/// whispered delivery.
fn audio_reject(clip: &Clip, gate: &Gate) -> Option<String> {
    if let Some(v) = clip.lead_speech.filter(|v| *v > gate.max_pad_speech) {
        return Some(format!("voices in the lead-in (speech {v:.2})"));
    }
    if let Some(v) = clip.tail_speech.filter(|v| *v > gate.max_pad_speech) {
        return Some(format!("voices in the tail (speech {v:.2})"));
    }
    if let Some(v) = clip.lead_rms.filter(|v| *v > gate.max_lead_rms) {
        return Some(format!("lead-in louder than the dialogue (rms x{v:.2})"));
    }
    if let Some(v) = clip.voiced.filter(|v| *v < gate.min_voiced) {
        return Some(format!("whispered delivery (voiced {v:.2})"));
    }
    None
}

/// Fraction of the 16 ms frames in `[from_ms, to_ms)` the speech profile
/// scores at or above `threshold`; `None` for an empty stretch.
fn profile_speech_fraction(
    profile: &[f32],
    threshold: f32,
    from_ms: i64,
    to_ms: i64,
) -> Option<f64> {
    let frame_ms = (crate::vad::FRAME * 1000 / crate::vad::SAMPLE_RATE) as i64;
    let lo = (from_ms.max(0) / frame_ms) as usize;
    let hi = ((to_ms.max(0) + frame_ms - 1) / frame_ms) as usize;
    let hi = hi.min(profile.len());
    let lo = lo.min(hi);
    (hi > lo).then(|| {
        profile[lo..hi].iter().filter(|&&s| s >= threshold).count() as f64 / (hi - lo) as f64
    })
}

/// Stretches of `[from_ms, to_ms)` at least `min_ms` long in which every
/// 16 ms frame of the speech profile scores below `threshold`.
fn silences(
    profile: &[f32],
    threshold: f32,
    from_ms: i64,
    to_ms: i64,
    min_ms: i64,
) -> Vec<(i64, i64)> {
    let frame_ms = (crate::vad::FRAME * 1000 / crate::vad::SAMPLE_RATE) as i64;
    let lo = (from_ms.max(0) / frame_ms) as usize;
    let hi = ((to_ms.max(0) + frame_ms - 1) / frame_ms) as usize;
    let hi = hi.min(profile.len());
    let mut runs = Vec::new();
    let mut run_start: Option<usize> = None;
    // One frame past the end, never quiet, closes a run touching `hi`.
    let quiet_at = profile[lo.min(hi)..hi]
        .iter()
        .map(|&s| s < threshold)
        .chain(std::iter::once(false));
    for (i, quiet) in (lo..).zip(quiet_at) {
        match (quiet, run_start) {
            (true, None) => run_start = Some(i),
            (false, Some(s)) => {
                let (a, b) = (s as i64 * frame_ms, i as i64 * frame_ms);
                if b - a >= min_ms {
                    runs.push((a, b));
                }
                run_start = None;
            }
            _ => {}
        }
    }
    runs
}

/// Where the profile says the span really begins and ends, and how much
/// silence lies on each side.
struct Margins {
    start_ms: i64,
    end_ms: i64,
    clear_before_ms: i64,
    clear_after_ms: i64,
}

/// Find the pause on each side of a placed span in the film's speech
/// profile. The search runs between the neighbouring word's stamp and the
/// span's own stamp, never inside the span; the pause nearest the span
/// wins. A boundary only moves outward: when the pause ends before the
/// stamped onset, speech began earlier than the stamp says (a squeezed or
/// late stamp) and the start moves back to it, less [`ONSET_LAG_MS`]. A
/// stamp stretched over silence is left alone — dead air inside the span
/// is harmless, a clipped consonant is not. `None` when either side has no
/// pause of `min_clear_ms` next to the span, which is the transcript's
/// "too close" verdict made against the audio instead of its own stamps.
fn earshot_margins(
    profile: &[f32],
    threshold: f32,
    p: &Placed,
    min_clear_ms: i64,
) -> Option<Margins> {
    let first = &p.words[0];
    let last = &p.words[p.words.len() - 1];
    let head = silences(
        profile,
        threshold,
        p.prev_word_start_ms.unwrap_or(first.at_ms - OPEN_SEARCH_MS),
        first.at_ms,
        min_clear_ms,
    );
    let (gap_start, gap_end) = *head.last()?;
    // The window ends at the stamp, so a pause reaching it means the stamp
    // is where speech begins; a pause ending short of it means speech
    // began earlier, at (roughly) the frame that first read as speech.
    let start_ms = if gap_end < first.at_ms {
        (gap_end - ONSET_LAG_MS).max(gap_start)
    } else {
        first.at_ms
    };
    let clear_before_ms = start_ms - gap_start;
    let tail = silences(
        profile,
        threshold,
        last.until_ms,
        p.next_word_end_ms.unwrap_or(last.until_ms + OPEN_SEARCH_MS),
        min_clear_ms,
    );
    let (end_ms, tail_end) = *tail.first()?;
    let clear_after_ms = tail_end - end_ms;
    (clear_before_ms >= min_clear_ms && clear_after_ms >= min_clear_ms).then_some(Margins {
        start_ms,
        end_ms,
        clear_before_ms,
        clear_after_ms,
    })
}

/// Map one film. Returns the summary, or the reason nothing was done.
async fn clips_one(
    http: &reqwest::Client,
    store: &osmo::Store,
    movie: &Movie,
    dir: &Path,
    gate: &Gate,
    concurrency: usize,
) -> Result<FilmSummary> {
    let code = course_dir(&movie.original_language).context("unmapped language")?;
    let language = Language::from_code(code).context("unmapped course code")?;
    let subtitle = dir.join("subtitle.srt");
    let transcript_path = dir.join("transcript.jsonl");
    let audio = dir.join("audio.opus");
    for (what, p) in [
        ("subtitle", &subtitle),
        ("transcript", &transcript_path),
        ("audio", &audio),
    ] {
        if !p.exists() {
            bail!("no {what}");
        }
    }
    // A subtitle that does not say what is said yields nothing worth the
    // phoneme spend, and one on another clock places nothing at all.
    let min_verbatim = gate
        .min_verbatim
        .unwrap_or_else(|| crate::verbatim::min_fraction(code));
    let check = crate::verbatim::check(dir, language, code, min_verbatim).await?;
    if check.measure.verdict != crate::verbatim::Verdict::Verbatim {
        bail!(
            "subtitle not verbatim: {}",
            crate::verbatim::describe(&check.measure)
        );
    }

    let min_ratio = if audio_only(code) {
        None
    } else {
        let min_ratio = gate
            .min_ratio
            .or_else(|| default_min_ratio(code))
            .with_context(|| format!("no calibrated phoneme gate for {code}"))?;
        if language.g2p_lang().is_none() {
            bail!("{code}: the g2p crate does not produce this language's model labels");
        }
        // Fail fast if the G2P engine cannot run (its data unpacks into a
        // cache dir on first use): a run that cannot phonemize must not
        // write a clips.jsonl at all — a poisoned file with current
        // provenance would be trusted by every later resume. (Seen live
        // 2026-09-01, when espeak was still an external binary: absent from
        // a nohup env, it produced an all-reject fra file that a re-run
        // skipped as done.)
        let canary = match language {
            Language::Hindi => "नमस्ते",
            Language::ChineseSimplified => "你好",
            Language::Japanese => "こんにちは",
            Language::Thai => "สวัสดี",
            _ => "bon",
        };
        match phoneme_verify::model_target(canary, language) {
            Some(Ok(p)) if !p.phonemes.is_empty() => {}
            other => bail!("G2P preflight: g2p produced {other:?} for a canary word"),
        }
        Some(min_ratio)
    };
    // Which phonemizer, and for Hindi which label convention, the targets
    // came from; either changing must re-score every clip. An audio-only
    // language has neither a phonemizer nor a model in its provenance.
    let (model, g2p) = match (min_ratio, language) {
        (None, _) => ("none".to_string(), "none".to_string()),
        (Some(_), Language::Hindi) => (
            phoneme_verify::production_cache_version(),
            format!(
                "{} hindi={:?}",
                g2p::identity(),
                phoneme_verify::MODEL_HINDI_CANON
            ),
        ),
        (Some(_), _) => (phoneme_verify::production_cache_version(), g2p::identity()),
    };
    let provenance = Provenance {
        format: FORMAT_VERSION,
        subtitle_digest: crate::transcript::source_digest(&subtitle)?,
        transcript_digest: crate::transcript::source_digest(&transcript_path)?,
        model,
        g2p,
        segmentation: movie_subtitles::segment::provenance(language),
        language: code.to_string(),
        min_ratio,
        min_clear_ms: gate.min_clear_ms,
        min_edge_logp: gate.min_edge_logp,
        max_pad_speech: gate.max_pad_speech,
        max_lead_rms: gate.max_lead_rms,
        min_voiced: gate.min_voiced,
        speech_threshold: gate.speech_threshold,
    };
    let path = clips_path(dir);
    if stored_provenance(&path).as_ref() == Some(&provenance) {
        let clips = read_clips(&path)?;
        return Ok(FilmSummary {
            sentences: clips.len(),
            aligned: clips.len(),
            scored: clips.len(),
            passed: clips.iter().filter(|c| c.passed).count(),
            median_ratio: median(clips.iter().filter_map(|c| c.ratio).collect()),
        });
    }

    // The margins come from the profile; without one there is nothing to
    // cut against, and falling back to stamps would quietly change what a
    // "clear" margin means from film to film.
    let profile = crate::vad::read_profile(&dir.join("speech-profile-16ms.f32"))
        .context("no 16 ms speech profile (run speech-profiles)")?;
    let threshold = gate.speech_threshold as f32;

    let empty = std::collections::HashMap::new();
    let ctx = match min_ratio {
        Some(_) => Some(VerifyContext::new(http, store.clone(), &empty, language)?),
        None => None,
    };
    let segmenter = SubtitleSegmenter::for_language(language)?;
    let transcript = load_transcript(&transcript_path)?;
    let sentences =
        subtitle_sentences(&std::fs::read_to_string(&subtitle)?, language, &segmenter).await?;

    let mut summary = FilmSummary {
        sentences: sentences.len(),
        ..Default::default()
    };
    let placed: Vec<(String, Placed)> = sentences
        .iter()
        .filter_map(|k| {
            place(
                &k.sentence,
                k.start_ms.into(),
                k.end_ms.into(),
                &transcript,
                code,
            )
            .ok()
            .map(|p| (k.sentence.clone(), p))
        })
        .collect();
    summary.aligned = placed.len();

    use futures::StreamExt;
    let clips: Vec<Option<Clip>> = futures::stream::iter(placed)
        .map(|(sentence, p)| {
            let ctx = &ctx;
            let audio = audio.clone();
            let imdb_id = movie.imdb_id.clone();
            let profile = &profile;
            async move {
                let stamped = (p.words[0].at_ms, p.words[p.words.len() - 1].until_ms);
                let margins = earshot_margins(profile, threshold, &p, gate.min_clear_ms);
                // Without a pause on both sides the stamps stand in, so the
                // record still says where the transcript put the sentence.
                let (start_ms, end_ms, clear_before_ms, clear_after_ms) = match &margins {
                    Some(m) => (m.start_ms, m.end_ms, m.clear_before_ms, m.clear_after_ms),
                    None => (stamped.0, stamped.1, p.clear_before_ms, p.clear_after_ms),
                };
                let (repaired_before_ms, repaired_after_ms) =
                    (stamped.0 - start_ms, end_ms - stamped.1);
                let pad = |target: i64, clear: i64| target.min(clear / 2).max(0);
                let pad_before_ms = pad(LEAD_IN_MS, clear_before_ms);
                let pad_after_ms = pad(AUDIO_PAD_MS, clear_after_ms);
                let mut clip = Clip {
                    sentence: sentence.clone(),
                    imdb_id,
                    start_ms,
                    end_ms,
                    pad_before_ms,
                    pad_after_ms,
                    repaired_before_ms,
                    repaired_after_ms,
                    words: p.words,
                    speaker: p.speaker,
                    transcript_wer: p.wer,
                    audio_event_overlap: p.audio_event_overlap,
                    clear_before_ms,
                    clear_after_ms,
                    target_ipa: Vec::new(),
                    oov: Vec::new(),
                    ratio: None,
                    logp_target_per_phoneme: None,
                    edge_logp_start: None,
                    edge_logp_end: None,
                    lead_speech: None,
                    tail_speech: None,
                    lead_rms: None,
                    voiced: None,
                    heard_ipa: Vec::new(),
                    passed: false,
                    reject: None,
                };
                if clip.audio_event_overlap {
                    clip.reject = Some("audio event inside the span".into());
                    return Some(clip);
                }
                if margins.is_none() {
                    clip.reject = Some("neighbouring speech too close to cut clean".into());
                    return Some(clip);
                }
                let wav = match tokio::task::spawn_blocking(move || {
                    slice_wav_padded(&audio, start_ms, end_ms, pad_before_ms, pad_after_ms)
                })
                .await
                .expect("slice task panicked")
                {
                    Ok(w) => w,
                    Err(e) => {
                        clip.reject = Some(format!("cut: {e:#}"));
                        return Some(clip);
                    }
                };
                let padded_ms = (end_ms - start_ms + pad_before_ms + pad_after_ms) as f64;
                // The mix's loudness and voicing, straight from the samples.
                if let Some(samples) = wav_samples(&wav) {
                    let n = |ms: f64| (ms * samples.len() as f64 / padded_ms) as usize;
                    let (lead_n, span_to) =
                        (n(pad_before_ms as f64), n(padded_ms - pad_after_ms as f64));
                    let span_samples =
                        &samples[lead_n.min(samples.len())..span_to.min(samples.len())];
                    let span = rms(span_samples);
                    if lead_n > 0 && span > 0.0 {
                        clip.lead_rms = Some(rms(&samples[..lead_n.min(samples.len())]) / span);
                    }
                    clip.voiced = voiced_fraction(span_samples, 16_000);
                }
                if let Some(min_ratio) = min_ratio {
                    let ctx = ctx.as_ref().expect("gated languages have a verify context");
                    let target = match phoneme_verify::model_target(&sentence, language) {
                        Some(Ok(p)) if !p.phonemes.is_empty() => p.phonemes,
                        Some(Ok(_)) | None => {
                            clip.reject = Some("g2p produced no phonemes".into());
                            return Some(clip);
                        }
                        // Includes the Hindi chain refusing digits or Latin
                        // script: a target with a hole where the audio has
                        // speech would score wrong, so the clip is rejected.
                        Some(Err(e)) => {
                            clip.reject = Some(format!("g2p: {e:#}"));
                            return Some(clip);
                        }
                    };
                    clip.target_ipa = target.clone();
                    let frames: FrameMatrix = match phoneme_verify::frame_matrix(ctx, &wav).await {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("  {}: {e:#}", clip.sentence);
                            return None;
                        }
                    };
                    // The pads under the model's ear: frames spread evenly
                    // over the sliced audio, so the pad regions are the
                    // first and last stretches of the matrix.
                    let frame_ms = padded_ms / frames.frames as f64;
                    let lead_frames = (pad_before_ms as f64 / frame_ms) as usize;
                    let tail_frames = (pad_after_ms as f64 / frame_ms) as usize;
                    clip.lead_speech = frames.speech_fraction(0, lead_frames);
                    clip.tail_speech = frames
                        .speech_fraction(frames.frames.saturating_sub(tail_frames), frames.frames);
                    let score = frames.score_target(&target);
                    clip.heard_ipa = frames
                        .greedy_ids()
                        .into_iter()
                        .map(|id| frames.vocab[id].clone())
                        .collect();
                    clip.oov = score.oov;
                    clip.ratio = score.ratio;
                    clip.logp_target_per_phoneme = score.logp_target_per_phoneme;
                    let ids: Vec<usize> = target.iter().filter_map(|t| frames.id(t)).collect();
                    if let Some(spans) = frames.force_align(&ids) {
                        let k = EDGE_PHONEMES.min(spans.len());
                        let mean = |spans: &[phoneme_verify::AlignedPhoneme]| {
                            spans.iter().map(|s| s.logp_mean).sum::<f64>() / spans.len() as f64
                        };
                        clip.edge_logp_start = Some(mean(&spans[..k]));
                        clip.edge_logp_end = Some(mean(&spans[spans.len() - k..]));
                    }
                    if let Some(reason) = phoneme_reject(&clip, min_ratio, gate) {
                        clip.reject = Some(reason);
                        return Some(clip);
                    }
                } else {
                    // No model to listen to the pads: the earshot profile
                    // says whether anyone speaks in them.
                    clip.lead_speech = profile_speech_fraction(
                        profile,
                        threshold,
                        start_ms - pad_before_ms,
                        start_ms,
                    );
                    clip.tail_speech =
                        profile_speech_fraction(profile, threshold, end_ms, end_ms + pad_after_ms);
                }
                clip.reject = audio_reject(&clip, gate);
                clip.passed = clip.reject.is_none();
                Some(clip)
            }
        })
        .buffered(concurrency.max(1))
        .collect()
        .await;

    let clips: Vec<Clip> = clips.into_iter().flatten().collect();
    summary.scored = clips.len();
    summary.passed = clips.iter().filter(|c| c.passed).count();
    summary.median_ratio = median(clips.iter().filter_map(|c| c.ratio).collect());

    let mut text = serde_json::to_string(&provenance)?;
    text.push('\n');
    for clip in &clips {
        text.push_str(&serde_json::to_string(clip)?);
        text.push('\n');
    }
    let tmp = dir.join("clips.jsonl.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(summary)
}

/// Map every transcribed film (or the ones selected), skipping films whose
/// `clips.jsonl` is already current.
pub async fn clips_all(
    out: PathBuf,
    films_in_flight: usize,
    limit: usize,
    imdb: Option<String>,
    langs: Option<Vec<String>>,
    gate: Gate,
) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| imdb.as_deref().is_none_or(|id| m.imdb_id == id))
        .filter(|m| {
            langs.as_ref().is_none_or(|l| {
                course_dir(&m.original_language).is_some_and(|c| l.iter().any(|x| x == c))
            })
        })
        .filter(|m| {
            let dir = out.join(&m.imdb_id);
            dir.join("subtitle.srt").exists()
                && dir.join("transcript.jsonl").exists()
                && dir.join("audio.opus").exists()
        })
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!("{total} transcribed films to map");
    if total == 0 {
        return Ok(());
    }

    let store = Arc::new(osmo::Store::open("./.cache"));
    let http = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?,
    );
    let out = Arc::new(out);
    let gate = Arc::new(gate);
    let progress = AtomicUsize::new(0);

    use futures::StreamExt;
    let totals: Vec<Option<FilmSummary>> = futures::stream::iter(queue)
        .map(|movie| {
            let (http, store, out, gate) = (
                Arc::clone(&http),
                Arc::clone(&store),
                Arc::clone(&out),
                Arc::clone(&gate),
            );
            let progress = &progress;
            async move {
                let dir = out.join(&movie.imdb_id);
                let outcome = clips_one(&http, &store, &movie, &dir, &gate, 8).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                let title = crate::library::truncate(&movie.title, 34);
                match &outcome {
                    Ok(s) => {
                        println!(
                            "[{n}/{total}] {title} ✓ {} sentences → {} placed → {} scored → {} pass",
                            s.sentences, s.aligned, s.scored, s.passed
                        );
                        if let Some(m) = s.median_ratio.filter(|m| *m < FOREIGN_AUDIO_RATIO) {
                            println!(
                                "    ⚠ median phoneme ratio {m:.2}: the audio does not sound like \
                                 {} — another language or variety on this track?",
                                movie.original_language
                            );
                        }
                    }
                    Err(e) => println!("[{n}/{total}] {title} ✗ {e:#}"),
                }
                outcome.ok()
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    let done: Vec<FilmSummary> = totals.into_iter().flatten().collect();
    println!(
        "\n{} films mapped: {} sentences, {} placed by the transcript, {} passed both gates",
        done.len(),
        done.iter().map(|s| s.sentences).sum::<usize>(),
        done.iter().map(|s| s.aligned).sum::<usize>(),
        done.iter().map(|s| s.passed).sum::<usize>()
    );
    Ok(())
}
