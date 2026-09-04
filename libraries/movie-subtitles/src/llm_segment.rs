//! Sentence segmentation by a language model, for scripts whose subtitles
//! carry no sentence-final punctuation.
//!
//! Japanese, Mandarin, Korean and Thai subtitles mark questions and
//! exclamations but leave plain statements bare, so the rule-based passage
//! builder ([`crate::segment`]) cannot tell where a statement ends: it glues
//! unpunctuated cues together until a mark turns up, and the course keeps only
//! the questions. Here every cue is instead put to a model, one request per
//! cue, with a few neighbouring cues for context. The model answers only
//! about the focus cue — its text cut into sentences, and whether the last one
//! runs on into the next cue — so each boundary has exactly one owner and no
//! two answers can disagree about it.
//!
//! The model never writes text. Each answer is checked letter for letter
//! against the cue it came from and thrown away if it differs, so the worst
//! a bad answer can do is leave a cue segmented the old way.
//!
//! Requests go through the Batch API and tysm's response cache, keyed by the
//! prompt: a cue's answer is reused across re-timed copies of the same
//! subtitle (pauses are only described when they are long, and only to the
//! second) and across the two consumers of this crate.

use language_utils::Language;
use serde::{Deserialize, Serialize};
use tysm::chat_completions::ChatClient;

use crate::segment::speaker_turns;
use crate::SubtitleLine;

/// The model that segments. Per-item workhorse tier, same as the rest of the
/// generate-data pipeline; a boundary decision does not need the judgment
/// tier.
pub const MODEL: &str = "gpt-5.6-luna";

/// Cues shown before and after the focus cue.
const CONTEXT_CUES: usize = 3;

/// Rejected answers printed per batch, to show what the model gets wrong
/// without flooding the log.
const SHOWN_FALLBACKS: usize = 40;

/// Requests per Batch API job. OpenAI caps a job at 50,000 requests; a whole
/// library of subtitles is several times that.
const BATCH_REQUESTS: usize = 20_000;

/// Batch jobs in flight at once. Enough to keep a library's worth moving,
/// few enough not to trip the enqueued-token quota.
const BATCHES_IN_FLIGHT: usize = 4;

/// A silence between cues is mentioned only from this length — a shorter
/// one is display timing, not evidence about sentence boundaries, and
/// leaving it out keeps the prompt identical across re-timed copies.
const NOTABLE_PAUSE_MS: u32 = 1_000;

/// Whether a language's subtitles are segmented by the model rather than by
/// punctuation rules.
pub fn uses_llm(language: Language) -> bool {
    matches!(
        language,
        Language::Japanese
            | Language::ChineseSimplified
            | Language::ChineseTraditional
            | Language::Korean
            | Language::Thai
    )
}

/// What goes between two halves of a sentence that a cue break split.
pub fn joiner(language: Language) -> &'static str {
    match language {
        Language::Japanese | Language::ChineseSimplified | Language::ChineseTraditional => "",
        _ => " ",
    }
}

/// Whether to route batches through ordinary chat completions instead of
/// OpenAI's Batch API (`YAP_NO_BATCH=1`).
///
/// The Batch API is otherwise a hard dependency with no fallback: when it
/// breaks account-side — as it did on 2026-08-28, when every batch started
/// failing validation with "Cannot find file <its own freshly uploaded
/// input>" — a run simply cannot finish, however healthy the rest of the
/// pipeline is. Sending live costs about double and runs the misses
/// sequentially, so this is an escape hatch to be switched off again once
/// batching recovers, not a default.
pub fn no_batch() -> bool {
    std::env::var("YAP_NO_BATCH").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The segmentation client: [`MODEL`], responses in the shared cache store.
pub fn client() -> anyhow::Result<ChatClient> {
    let client = ChatClient::from_env(MODEL)?
        .with_cache_directory("./.cache")
        .with_reasoning_effort("none");
    Ok(if no_batch() {
        client.with_small_batch_threshold(usize::MAX)
    } else {
        client
    })
}

/// Identity of this segmenter for provenance stamps: the model and a digest
/// of everything in the prompt, so a reworded prompt or a new model can
/// never pose as the segmentation a stored result was made under.
pub fn provenance() -> String {
    // FNV-1a: stable across Rust versions, unlike the std hasher.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{SYSTEM_PROMPT}\n{CONTEXT_CUES}\n{NOTABLE_PAUSE_MS}").bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("llm/{MODEL}/{h:016x}")
}

/// The model's answer for one focus cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CueSplit {
    /// The focus cue's text cut into sentences, in order, each copied
    /// exactly from the cue. A fragment that finishes a sentence begun in
    /// the previous cue, or that the next cue continues, is its own entry.
    pub sentences: Vec<String>,
    /// True when the last entry is not a complete sentence and the next cue
    /// carries on with it.
    pub unfinished: bool,
}

impl CueSplit {
    /// What a cue falls back to when the model's answer is unusable: the
    /// cue's speaker turns, taken as they are.
    fn per_cue(cue: &str) -> Self {
        Self {
            sentences: speaker_turns(cue),
            unfinished: false,
        }
    }

    /// The answer for a cue that needs no model: it ends in a sentence-final
    /// mark and carries none inside, so it is one closed sentence (or one
    /// per speaker turn). An ellipsis does not close a sentence — a line
    /// trailing off usually continues in the next cue — so it does not
    /// count. About a fifth of Japanese, Mandarin and Korean cues.
    fn self_evident(cue: &str) -> Option<Self> {
        const CLOSING: &[char] = &['.', '!', '?', '。', '！', '？'];
        let trimmed = cue.trim_end_matches(['"', '”', '」', '』', ')', ' ']);
        let body = trimmed.trim_end_matches(CLOSING);
        (body.len() < trimmed.len() && !body.contains(CLOSING)).then(|| Self::per_cue(cue))
    }
}

const SYSTEM_PROMPT: &str = "You segment film subtitles into sentences for a language-learning app that shows one sentence at a time. Subtitles split sentences across cues and pack several into one cue, and some languages write no sentence-final punctuation.

You get one focus cue with the cues before and after it as context. Return only the focus cue's text, cut into sentences in order, and whether the last one is unfinished, meaning the next cue completes it. Context text never belongs in the answer.

Copy the text exactly: same characters, same order, nothing added, corrected or dropped, not even punctuation at a cut. The app matches entries back letter for letter and discards any answer that differs. A dash marking a change of speaker may be dropped.

A sentence is a complete utterance: a statement, question, exclamation, one-word reply or interjection. A fragment that belongs with the previous or next cue is its own entry: first if it finishes the previous cue, last and unfinished if the next cue completes it. Long pauses between cues are noted; they usually, not always, mean a sentence ended.";

/// The prompt for `index` in `lines`: the focus cue on its own, the
/// neighbours before and after it, and any long pauses between them.
fn prompt_for(lines: &[SubtitleLine], index: usize, language: Language) -> String {
    let from = index.saturating_sub(CONTEXT_CUES);
    let to = (index + CONTEXT_CUES).min(lines.len() - 1);
    let pause_line = |i: usize| -> Option<String> {
        let pause = lines[i].start_ms.saturating_sub(lines[i - 1].end_ms);
        (pause >= NOTABLE_PAUSE_MS).then(|| {
            format!(
                "  (pause of {} s)\n",
                (f64::from(pause) / 1000.0).round() as u32
            )
        })
    };
    let mut out = format!("Language: {language}\n");
    if from < index {
        out.push_str("\nCues before the focus cue:\n");
        for (i, line) in lines.iter().enumerate().take(index).skip(from) {
            if i > from {
                out.extend(pause_line(i));
            }
            out.push_str(&format!("  {}\n", line.sentence));
        }
        out.extend(pause_line(index));
    }
    out.push_str(&format!("\nFocus cue:\n  {}\n", lines[index].sentence));
    if index < to {
        out.push_str("\nCues after the focus cue:\n");
        for (i, line) in lines.iter().enumerate().take(to + 1).skip(index + 1) {
            out.extend(pause_line(i));
            out.push_str(&format!("  {}\n", line.sentence));
        }
    }
    out
}

/// Text with everything a legitimate answer may differ in removed:
/// whitespace and speaker dashes.
fn comparable(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '–' | '—' | '‐' | '‑'))
        .collect()
}

/// A dialogue dash the model kept at the head of an entry, dropped like the
/// rule-based path drops it.
fn strip_speaker_dash(entry: &str) -> String {
    entry
        .trim()
        .trim_start_matches(['-', '–', '—', '‐', '‑'])
        .trim()
        .to_string()
}

/// The answer if it reproduces the cue exactly, else `None`.
fn validated(cue: &str, answer: CueSplit) -> Option<CueSplit> {
    let sentences: Vec<String> = answer
        .sentences
        .iter()
        .map(|s| strip_speaker_dash(s))
        .collect();
    if sentences.is_empty() || sentences.iter().any(String::is_empty) {
        return None;
    }
    let joined: String = sentences.iter().map(|s| comparable(s)).collect();
    (joined == comparable(cue)).then_some(CueSplit {
        sentences,
        unfinished: answer.unfinished,
    })
}

/// How a batch of tracks fared.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SplitReport {
    pub cues: usize,
    /// Cues put to the model; the rest were closed by their own punctuation.
    pub asked: usize,
    /// Answers that failed the letter-for-letter check (or the request
    /// itself failed) and fell back to the cue's speaker turns.
    pub fallbacks: usize,
}

/// Segment several tracks in one Batch API round trip.
///
/// One request per cue, all tracks together — a course of fifty films is one
/// batch, not fifty consecutive ones. Results come back per track, aligned
/// with the cues passed in. `on_progress` sees the Batch API's status polls.
pub async fn split_tracks(
    client: &ChatClient,
    tracks: &[(&[SubtitleLine], Language)],
    on_progress: impl FnMut(&tysm::batch::Batch),
) -> anyhow::Result<(Vec<Vec<CueSplit>>, SplitReport)> {
    let mut prompts: Vec<(usize, usize, String)> = Vec::new();
    let mut evident: Vec<Vec<Option<CueSplit>>> = Vec::with_capacity(tracks.len());
    for (t, (lines, language)) in tracks.iter().enumerate() {
        let mut track = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            let split = CueSplit::self_evident(&line.sentence);
            if split.is_none() {
                prompts.push((t, i, prompt_for(lines, i, *language)));
            }
            track.push(split);
        }
        evident.push(track);
    }
    // Chunked into Batch API jobs, a few in flight at a time; answers are
    // reassembled in prompt order.
    let on_progress = std::sync::Mutex::new(on_progress);
    type Answer<'a> = (
        &'a (usize, usize, String),
        Result<CueSplit, tysm::chat_completions::IndividualChatError>,
    );
    let mut answers: Vec<Answer> = Vec::with_capacity(prompts.len());
    for wave in prompts.chunks(BATCH_REQUESTS * BATCHES_IN_FLIGHT) {
        let jobs = wave.chunks(BATCH_REQUESTS).map(|chunk| {
            client.batch_chat_with_system_prompt_fn::<_, _, CueSplit>(
                SYSTEM_PROMPT,
                chunk,
                |(_, _, p)| p.clone(),
                |batch| (on_progress.lock().unwrap())(batch),
            )
        });
        for job in futures::future::join_all(jobs).await {
            answers.extend(job?);
        }
    }

    // Model answers land in their cue's slot; the self-evident cues are
    // already there.
    let mut out: Vec<Vec<Option<CueSplit>>> = evident;
    let mut report = SplitReport {
        cues: tracks.iter().map(|(lines, _)| lines.len()).sum(),
        asked: prompts.len(),
        fallbacks: 0,
    };
    for ((t, i, _), answer) in answers {
        let cue = &tracks[*t].0[*i].sentence;
        let split = match answer {
            Ok(answer) => {
                let split = validated(cue, answer.clone());
                if split.is_none() && report.fallbacks < SHOWN_FALLBACKS {
                    eprintln!(
                        "segmentation answer rejected for {cue:?}: {:?}",
                        answer.sentences
                    );
                }
                split
            }
            Err(e) => {
                eprintln!("segmentation request failed for {cue:?}: {e:#}");
                None
            }
        };
        out[*t][*i] = Some(split.unwrap_or_else(|| {
            report.fallbacks += 1;
            CueSplit::per_cue(cue)
        }));
    }
    let out = out
        .into_iter()
        .map(|track| {
            track
                .into_iter()
                .map(|s| s.expect("every cue is self-evident or answered"))
                .collect()
        })
        .collect();
    Ok((out, report))
}

/// Segment one track.
pub async fn split(
    client: &ChatClient,
    lines: &[SubtitleLine],
    language: Language,
) -> anyhow::Result<(Vec<CueSplit>, SplitReport)> {
    let (mut tracks, report) = split_tracks(client, &[(lines, language)], print_progress()).await?;
    Ok((tracks.remove(0), report))
}

/// A progress callback that prints a line to stderr whenever a batch's
/// status or completed count changes.
pub fn print_progress() -> impl FnMut(&tysm::batch::Batch) {
    let mut last: Option<(String, u32)> = None;
    move |batch: &tysm::batch::Batch| {
        let now = (
            format!("{:?}", batch.status),
            batch.request_counts.completed + batch.request_counts.failed,
        );
        if last.as_ref() != Some(&now) {
            eprintln!(
                "  segmentation batch {}: {} {}/{}",
                batch.id, now.0, now.1, batch.request_counts.total
            );
            last = Some(now);
        }
    }
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
    fn answers_must_reproduce_the_cue() {
        let ok = CueSplit {
            sentences: vec!["安静！".into(), "别敲了！".into()],
            unfinished: false,
        };
        assert_eq!(validated("安静！别敲了！", ok.clone()), Some(ok));

        // Whitespace and speaker dashes are the model's to drop.
        let dashed = CueSplit {
            sentences: vec!["- あそこはダメだ".into(), "何で？".into()],
            unfinished: false,
        };
        assert_eq!(
            validated("‐ あそこはダメだ ‐ 何で？", dashed)
                .unwrap()
                .sentences,
            vec!["あそこはダメだ", "何で？"]
        );

        // Anything added, corrected or dropped is refused.
        let edited = CueSplit {
            sentences: vec!["安静。".into(), "别敲了！".into()],
            unfinished: false,
        };
        assert_eq!(validated("安静！别敲了！", edited), None);
        let short = CueSplit {
            sentences: vec!["安静！".into()],
            unfinished: false,
        };
        assert_eq!(validated("安静！别敲了！", short), None);
        let empty = CueSplit {
            sentences: vec![],
            unfinished: false,
        };
        assert_eq!(validated("安静！", empty), None);
    }

    #[test]
    fn a_closed_cue_needs_no_model() {
        assert!(CueSplit::self_evident("嫁给什么人能由得了我吗？").is_some());
        assert!(CueSplit::self_evident("정말 고마워요.").is_some());
        assert_eq!(
            CueSplit::self_evident("- 어이! - 여 봐!").map(|s| s.sentences),
            None,
            "an internal mark may hide a boundary"
        );
        assert!(CueSplit::self_evident("嫁人就嫁人吧").is_none());
        assert!(CueSplit::self_evident("それは…").is_none());
        assert_eq!(
            CueSplit::self_evident("「行きます！」").map(|s| s.sentences),
            Some(vec!["「行きます！」".to_string()])
        );
    }

    #[test]
    fn prompt_shows_neighbours_and_long_pauses_only() {
        let lines = [
            cue("娘你不要再说了", 51_194, 55_987),
            cue("你已经跟我说了三天了", 55_987, 58_985),
            cue("我也想明白了", 59_029, 61_233),
            cue("嫁人就嫁人吧", 61_279, 63_317),
            cue("小姐你找谁呀", 250_594, 255_750),
        ];
        let prompt = prompt_for(&lines, 3, Language::ChineseSimplified);
        assert_eq!(
            prompt,
            "Language: Chinese (Simplified)\n\nCues before the focus cue:\n  娘你不要再说了\n  你已经跟我说了三天了\n  我也想明白了\n\nFocus cue:\n  嫁人就嫁人吧\n\nCues after the focus cue:\n  (pause of 187 s)\n  小姐你找谁呀\n"
        );
        // A 44 ms gap is display timing, not a pause; the first cue has
        // nothing before it and the last nothing after.
        assert!(!prompt_for(&lines, 0, Language::ChineseSimplified).contains("before"));
        assert!(!prompt_for(&lines, 4, Language::ChineseSimplified).contains("after"));
    }
}
