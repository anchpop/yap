//! Diagnostic: where do a film's sentences fall out before the phoneme gate?
//! Runs the placement stage only (no inference) and tallies the reasons.
use std::collections::BTreeMap;
use std::path::PathBuf;

use language_utils::Language;
use movie_subtitles::segment::SubtitleSegmenter;
use subtitle_corpus::clips::{place, subtitle_sentences};
use subtitle_corpus::cues::{agreement_tokens, align_sentence, load_transcript, tokenization_for};
use subtitle_corpus::library::{course_dir, read_plan};
use subtitle_corpus::transcript::Kind;
use unicode_normalization::UnicodeNormalization;

fn fold(t: &str) -> String {
    t.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/data/andrep/subtitle-corpus");
    let ids: Vec<String> = std::env::args().skip(1).collect();
    let samples: usize = std::env::var("SAMPLES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let plan = read_plan(&out)?;
    for movie in plan.iter().filter(|m| ids.contains(&m.imdb_id)) {
        let dir = out.join(&movie.imdb_id);
        let code = course_dir(&movie.original_language).unwrap();
        let language = Language::from_code(code).unwrap();
        let segmenter = SubtitleSegmenter::for_language(language)?;
        let transcript = load_transcript(&dir.join("transcript.jsonl"))?;
        let srt = std::fs::read_to_string(dir.join("subtitle.srt"))?;
        let sentences = subtitle_sentences(&srt, language, &segmenter);
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        let mut shown: BTreeMap<&str, usize> = BTreeMap::new();
        let mut worthy = 0;
        println!(
            "== {} {} ({code}) {} sentences",
            movie.imdb_id,
            movie.title,
            sentences.len()
        );
        for k in &sentences {
            worthy += usize::from(k.course_worthy);
            let r = place(
                &k.sentence,
                k.start_ms.into(),
                k.end_ms.into(),
                &transcript,
                code,
            );
            let key = match &r {
                Ok(_) => "placed",
                Err(e) => e,
            };
            *tally.entry(key).or_default() += 1;
            let toks = agreement_tokens(&k.sentence, tokenization_for(code));
            if key == "too short" && k.course_worthy {
                *tally.entry("  too short & course_worthy").or_default() += 1;
            }
            if key == "placed" && k.course_worthy {
                *tally.entry("  placed & course_worthy").or_default() += 1;
            }
            if key == "transcript disagrees" {
                for slop in [1500i64, 3000] {
                    let (lo, hi) = (i64::from(k.start_ms) - slop, i64::from(k.end_ms) + slop);
                    let heard: Vec<String> = transcript
                        .iter()
                        .filter(|w| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
                        .flat_map(|w| agreement_tokens(&w.text, tokenization_for(code)))
                        .collect();
                    if let Some(m) = align_sentence(&toks, &heard) {
                        if (m.distance as f64) / (toks.len() as f64) <= 0.12 {
                            *tally
                                .entry(if slop == 1500 {
                                    "  disagrees: passes with slop 1500"
                                } else {
                                    "  disagrees: passes with slop 3000"
                                })
                                .or_default() += 1;
                        }
                    }
                }
            }
            if key == "too short" {
                *tally
                    .entry(if toks.len() == 1 {
                        "  too short: 1 token"
                    } else {
                        "  too short: 2 tokens"
                    })
                    .or_default() += 1;
            }
            if key == "transcript disagrees" {
                let (lo, hi) = (i64::from(k.start_ms) - 500, i64::from(k.end_ms) + 500);
                let heard: Vec<String> = transcript
                    .iter()
                    .filter(|w| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
                    .flat_map(|w| agreement_tokens(&w.text, tokenization_for(code)))
                    .collect();
                if let Some(m) = align_sentence(&toks, &heard) {
                    let d = m.distance;
                    if d == 1 && std::env::var("SHOW1").is_ok() {
                        println!(
                            "  1EDIT {:?}  ~  {:?}",
                            toks.join(" "),
                            heard[m.first..=m.last].join(" ")
                        );
                    }
                    let b = if d == 1 {
                        "  disagrees: 1 edit"
                    } else if d == 2 {
                        "  disagrees: 2 edits"
                    } else if (d as f64) / (toks.len() as f64) <= 0.34 {
                        "  disagrees: 3+ edits, wer<=.34"
                    } else {
                        "  disagrees: wer>.34 (different text/offset)"
                    };
                    *tally.entry(b).or_default() += 1;
                    if d == 1 && toks.len() <= 8 {
                        *tally
                            .entry("  disagrees: 1 edit on <=8 tokens")
                            .or_default() += 1;
                    }
                    if d >= 1 {
                        let ft: Vec<String> = toks.iter().map(|t| fold(t)).collect();
                        let fh: Vec<String> = heard.iter().map(|t| fold(t)).collect();
                        if let Some(m2) = align_sentence(&ft, &fh) {
                            if m2.distance == 0 {
                                *tally
                                    .entry("  disagrees: accent/diacritic-only")
                                    .or_default() += 1;
                            } else if (m2.distance as f64) / (toks.len() as f64) <= 0.12 {
                                *tally
                                    .entry("  disagrees: passes after diacritic fold")
                                    .or_default() += 1;
                            }
                        }
                    }
                }
            }
            let n = shown.entry(key).or_default();
            if *n < samples && key != "placed" && key != "too short" {
                *n += 1;
                let (lo, hi) = (i64::from(k.start_ms) - 500, i64::from(k.end_ms) + 500);
                let heard: Vec<String> = transcript
                    .iter()
                    .filter(|w| w.kind == Kind::Word && w.at_ms < hi && w.until_ms > lo)
                    .map(|w| w.text.clone())
                    .collect();
                let toks = agreement_tokens(&k.sentence, tokenization_for(code));
                println!(
                    "  [{key}] @{}-{} {:?}\n      tokens={} heard: {}",
                    k.start_ms,
                    k.end_ms,
                    k.sentence,
                    toks.len(),
                    heard.join(" ")
                );
            }
        }
        println!("  course_worthy: {worthy}/{}", sentences.len());
        for (k, v) in &tally {
            println!("  {v:6}  {k}");
        }
    }
    Ok(())
}
