//! Is a film's subtitle what is actually said in it?
//!
//! A subtitle can be perfectly synced and still useless as a clip source: a
//! European Portuguese rewrite of a Brazilian film, a Latin American
//! translation of a Spanish one, an SDH track that condenses every line, a
//! fansub that paraphrases. Word-overlap audits pass all of these — a
//! paraphrase shares most of its words with the speech — so the measure here
//! is the one the clip mapper actually applies: the share of eligible
//! sentences (three or more tokens, no digits) that [`crate::clips::place`]
//! can find near-verbatim in the transcript at the time the subtitle claims.
//! Verbatim tracks sit at 40–90%; rewrites at 0–20%.
//!
//! A subtitle can also be verbatim but on another clock — a different cut,
//! or a download the syncers never placed. Word anchors against the full
//! transcript ([`sync::find_anchors`]) fit a shift and rate; when the
//! sentences place under that fit but not at identity, the verdict is
//! [`Verdict::Skewed`] and the fit is reported so the caller can re-time the
//! file. That repair is only right when the subtitle's clock is the suspect:
//! a disc track is authored to this file, so a disc track that reads as
//! skewed means the *audio* is off, and re-timing it would hide the defect.
//!
//! The verdict is written beside the film as `transcript-check.json`, keyed
//! by digests of both inputs, so a re-run is free until either changes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use language_utils::Language;
use movie_subtitles::segment::SubtitleSegmenter;
use serde::{Deserialize, Serialize};

use crate::clips::{place, subtitle_sentences};
use crate::cues::{load_transcript, parse_cues};
use crate::sync::{self, Cue, TimedWord};
use crate::transcript::{source_digest, Kind, Spoken};

/// Bump when the measure changes in a way that makes stored verdicts stale.
pub const FORMAT: u32 = 4;

/// Least share of eligible sentences that must place for a subtitle to
/// count as verbatim. Measured 2026-09-02 over 69 transcribed films: every
/// track known to be a rewrite or condensed SDH sat at or below 20%
/// (I'm Still Here 7%, Divorce Italian Style 11%, Spirit of the Beehive
/// 18%, Open Your Eyes 20%), the worst genuine track at 29% (Bacurau, heavy
/// dialect), and the bulk between 40% and 90%.
pub const MIN_FRACTION: f64 = 0.25;

/// The bar for a language. The g2p-backend languages sit lower across the
/// board (measured 2026-09-03: known-verbatim films 17–55%, known
/// rewrites and skews 0–8%) because the sentence segmenter merges three to
/// five cues into one "sentence" when a CJK or Hindi subtitle carries no
/// terminal punctuation, and the merged span then fails the length bound
/// or accumulates edits. That is a segmenter defect to fix at its root
/// (course ingestion keys sentences the same way); until then the bar
/// follows the measured gap.
pub fn min_fraction(code: &str) -> f64 {
    match code {
        "hin" | "jpn" | "zho-hans" | "tha" => 0.15,
        _ => MIN_FRACTION,
    }
}

/// Fewer eligible sentences than this and the fraction means nothing —
/// a near-silent film, a forced track, a file of credits.
pub const MIN_ELIGIBLE: usize = 30;

/// Shortest word `find_anchors` pairs — the value the syncers use.
const ANCHOR_MIN_LEN: usize = 4;
/// Anchors further than this from the consensus shift are discarded.
const FIT_TOLERANCE_MS: f64 = 1500.0;
/// A fit closer to the identity than this is not a skew worth applying.
const SKEW_MS: f64 = 1000.0;
/// Fewest anchors a fit may rest on (and they must be at least a quarter
/// of all anchors found) before it is reported at all.
const MIN_CONSENSUS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The subtitle says what is said, when it is said.
    Verbatim,
    /// Verbatim once re-timed by `aligned`; not at its own clock.
    Skewed,
    /// Not what is said, at any clock: a rewrite, a translation into
    /// another variety, a condensed track.
    Paraphrase,
    /// Too few eligible sentences to judge.
    Empty,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Verbatim => "verbatim",
            Verdict::Skewed => "skewed",
            Verdict::Paraphrase => "paraphrase",
            Verdict::Empty => "empty",
        }
    }
}

/// The best linear re-timing the transcript's anchors support, and how the
/// sentences place under it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fit {
    pub offset_ms: f64,
    pub rate: f64,
    pub anchors_used: usize,
    pub anchors_seen: usize,
    pub worst_residual_ms: f64,
    pub placed: usize,
    pub fraction: f64,
}

impl Fit {
    fn alignment(&self) -> sync::Alignment {
        sync::Alignment {
            rate: self.rate,
            offset_ms: self.offset_ms,
            anchors_used: self.anchors_used,
            anchors_seen: self.anchors_seen,
            worst_residual_ms: self.worst_residual_ms,
        }
    }
}

/// One subtitle text measured against one transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measure {
    /// Sentences with enough tokens and no digits — the ones the clip
    /// mapper would try.
    pub eligible: usize,
    /// Of those, placed near-verbatim at the subtitle's own clock.
    pub placed: usize,
    pub fraction: f64,
    /// Present when the anchors fit a clock that differs from identity.
    pub aligned: Option<Fit>,
    pub verdict: Verdict,
}

impl Measure {
    /// The best placement fraction at any clock: what the subtitle would
    /// yield once re-timed.
    pub fn best_fraction(&self) -> f64 {
        self.aligned
            .as_ref()
            .map_or(self.fraction, |a| a.fraction.max(self.fraction))
    }

    /// Sentences placed at the better clock — the yield a subtitle offers,
    /// which is what one candidate is judged against another by: a short
    /// clean track with a high fraction still gives fewer clips than a long
    /// one with a lower fraction.
    pub fn best_placed(&self) -> usize {
        self.aligned
            .as_ref()
            .map_or(self.placed, |a| a.placed.max(self.placed))
    }
}

/// [`Measure`] plus the provenance that makes it reusable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub format: u32,
    pub subtitle_digest: String,
    pub transcript_digest: String,
    pub min_fraction: f64,
    #[serde(flatten)]
    pub measure: Measure,
}

pub fn report_path(dir: &Path) -> PathBuf {
    dir.join("transcript-check.json")
}

pub fn stored(dir: &Path) -> Option<Report> {
    serde_json::from_slice(&std::fs::read(report_path(dir)).ok()?).ok()
}

fn heard_words(transcript: &[Spoken]) -> Vec<TimedWord> {
    transcript
        .iter()
        .filter(|w| w.kind == Kind::Word)
        .map(|w| TimedWord {
            text: w.text.clone(),
            at_ms: w.at_ms,
            until_ms: w.until_ms,
        })
        .collect()
}

/// `(eligible, placed)` for one subtitle text at its own clock.
async fn score(
    srt: &str,
    transcript: &[Spoken],
    language: Language,
    code: &str,
    segmenter: &SubtitleSegmenter,
) -> Result<(usize, usize)> {
    let (mut eligible, mut placed) = (0usize, 0usize);
    for k in subtitle_sentences(srt, language, segmenter).await? {
        match place(
            &k.sentence,
            k.start_ms.into(),
            k.end_ms.into(),
            transcript,
            code,
        ) {
            Ok(_) => {
                eligible += 1;
                placed += 1;
            }
            Err("too short" | "contains a digit") => {}
            Err(_) => eligible += 1,
        }
    }
    Ok((eligible, placed))
}

/// Frame-rate ratios a subtitle timed to another release commonly runs at
/// against this one: identity, PAL speed-up and its inverse, and the
/// 24/25 pair. [`sync::fit`] takes its consensus from a histogram of
/// per-anchor shifts, which a 4% rate smears across minutes on a feature
/// film — so the rate has to be guessed *before* the histogram, not fitted
/// after it.
const RATES: &[f64] = &[1.0, 25.0 / 23.976, 23.976 / 25.0, 25.0 / 24.0, 24.0 / 25.0];

/// The fit that the most anchors agree on, over [`RATES`].
fn best_fit(anchors: &[sync::Anchor]) -> Option<sync::Alignment> {
    RATES
        .iter()
        .filter_map(|&rate| {
            let scaled: Vec<sync::Anchor> = anchors
                .iter()
                .map(|a| sync::Anchor {
                    subtitle_ms: (a.subtitle_ms as f64 * rate).round() as i64,
                    spoken_ms: a.spoken_ms,
                })
                .collect();
            let fit = sync::fit(&scaled, FIT_TOLERANCE_MS)?;
            Some(sync::Alignment {
                rate: fit.rate * rate,
                ..fit
            })
        })
        .max_by_key(|a| a.anchors_used)
}

fn retimed(cues: &[Cue], alignment: &sync::Alignment) -> Vec<Cue> {
    cues.iter()
        .map(|c| Cue {
            start_ms: alignment.apply(c.start_ms),
            end_ms: alignment.apply(c.end_ms),
            text: c.text.clone(),
        })
        .collect()
}

/// The subtitle re-timed under a fit, as SRT text.
pub fn retime(srt: &str, fit: &Fit) -> String {
    sync::write_cues(&retimed(&parse_cues(srt), &fit.alignment()))
}

/// Measure one subtitle text against a transcript.
pub async fn measure(
    srt: &str,
    transcript: &[Spoken],
    language: Language,
    code: &str,
    min_fraction: f64,
) -> Result<Measure> {
    let segmenter = SubtitleSegmenter::for_language(language)?;
    let (eligible, placed) = score(srt, transcript, language, code, &segmenter).await?;
    let fraction = if eligible == 0 {
        0.0
    } else {
        placed as f64 / eligible as f64
    };

    let cues = parse_cues(srt);
    let anchors = sync::find_anchors(&cues, &heard_words(transcript), ANCHOR_MIN_LEN);
    // A fit only means something when the anchors agree on it: a different
    // cut yields hundreds of anchors of which a dozen happen to share a
    // shift, and re-timing on those is noise dressed as evidence.
    let candidate = best_fit(&anchors)
        .filter(|a| a.anchors_used >= MIN_CONSENSUS && a.anchors_used * 4 >= a.anchors_seen)
        .filter(|a| a.offset_ms.abs() >= SKEW_MS || (a.rate - 1.0).abs() > 1e-4);
    let mut aligned = None;
    if let Some(a) = candidate {
        let srt = sync::write_cues(&retimed(&cues, &a));
        let (n, placed) = score(&srt, transcript, language, code, &segmenter).await?;
        aligned = Some(Fit {
            offset_ms: a.offset_ms,
            rate: a.rate,
            anchors_used: a.anchors_used,
            anchors_seen: a.anchors_seen,
            worst_residual_ms: a.worst_residual_ms,
            placed,
            fraction: if n == 0 {
                0.0
            } else {
                placed as f64 / n as f64
            },
        });
    }

    let verdict = if eligible < MIN_ELIGIBLE {
        Verdict::Empty
    } else if fraction >= min_fraction {
        Verdict::Verbatim
    } else if aligned.as_ref().is_some_and(|a| a.fraction >= min_fraction) {
        Verdict::Skewed
    } else {
        Verdict::Paraphrase
    };
    Ok(Measure {
        eligible,
        placed,
        fraction,
        aligned,
        verdict,
    })
}

/// The film's verdict, from `transcript-check.json` when it was computed
/// from the same inputs under the same threshold, else measured and written.
pub async fn check(
    dir: &Path,
    language: Language,
    code: &str,
    min_fraction: f64,
) -> Result<Report> {
    let subtitle = dir.join("subtitle.srt");
    let transcript_path = dir.join("transcript.jsonl");
    let subtitle_digest = source_digest(&subtitle).context("subtitle digest")?;
    let transcript_digest = source_digest(&transcript_path).context("transcript digest")?;
    if let Some(r) = stored(dir) {
        if r.format == FORMAT
            && r.subtitle_digest == subtitle_digest
            && r.transcript_digest == transcript_digest
            && r.min_fraction == min_fraction
        {
            return Ok(r);
        }
    }
    let transcript = load_transcript(&transcript_path)?;
    let srt = std::fs::read_to_string(&subtitle)?;
    let measure = measure(&srt, &transcript, language, code, min_fraction).await?;
    let report = Report {
        format: FORMAT,
        subtitle_digest,
        transcript_digest,
        min_fraction,
        measure,
    };
    std::fs::write(report_path(dir), serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

/// One line of evidence for a table.
pub fn describe(m: &Measure) -> String {
    let mut s = format!(
        "{:10} {:5.1}% of {:4} placed",
        m.verdict.label(),
        m.fraction * 100.0,
        m.eligible
    );
    if let Some(a) = &m.aligned {
        s.push_str(&format!(
            "; re-timed {:+.1}s ×{:.4} ({}/{} anchors) → {:.1}%",
            a.offset_ms / 1000.0,
            a.rate,
            a.anchors_used,
            a.anchors_seen,
            a.fraction * 100.0
        ));
    }
    s
}
