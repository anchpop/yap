//! How much of each movie's dialogue reaches the language pack.
//!
//! For every movie in a course's `sentence-sources/movies/metadata.jsonl`,
//! runs the ingestion path (`target_sentences::subtitle_sentences`: passage
//! joining → parsley segmentation → `should_include_sentence`) over the
//! subtitle track and compares the result against the sentences the current
//! language pack attributes to that film. Sentences are compared after
//! `cleanup_sentence`, exactly as the pack keys them.
//!
//! Prints one line per film — cues, sentences the pack has, sentences the
//! ingestion path produces now, and how many of those are new — plus totals,
//! so a change to ingestion can be judged on the whole course rather than on
//! a hand-picked film.
//!
//! Usage (from the yap repo root):
//!     cargo run --release --bin movie-coverage -- --course fra [--native eng] [--movie tt0108500]

use anyhow::{Context, Result};
use clap::Parser;
use generate_data::target_sentences::{should_include_sentence, subtitle_sentences};
use language_utils::language_pack::LanguagePack;
use language_utils::{Course, Language, text_cleanup::cleanup_sentence};
use movie_subtitles::segment::{SubtitleSegmenter, subtitle_passages};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about = "Per-movie subtitle → language-pack coverage")]
struct Args {
    /// Target language code (fra, spa, ...).
    #[arg(long)]
    course: String,
    /// Native language code of the pack to compare against.
    #[arg(long, default_value = "eng")]
    native: String,
    /// Root of the yap language data.
    #[arg(long, default_value = "./generate-data/data")]
    data_root: PathBuf,
    /// Directory holding `<target>_for_<native>/language_data.rkyv`.
    #[arg(long, default_value = "./out")]
    out: PathBuf,
    /// Restrict to these IMDb ids (repeatable).
    #[arg(long)]
    movie: Vec<String>,
    /// Print the sentences that are new (not in the pack) for each film.
    #[arg(long)]
    show_new: bool,
    /// Print every passage containing this text, with its segmentation and
    /// which sentences the filter keeps.
    #[arg(long)]
    trace: Option<String>,
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let target = Language::from_code(&args.course)
        .with_context(|| format!("unknown language code {}", args.course))?;
    let native = Language::from_code(&args.native)
        .with_context(|| format!("unknown language code {}", args.native))?;
    let course = Course {
        target_language: target,
        native_language: native,
    };

    // Sentences the pack attributes to each movie, keyed by IMDb id.
    let pack = load_pack(&args.out, &course)?;
    let mut in_pack: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for (sentence, source) in &pack.sentence_sources {
        for id in &source.movie_ids {
            in_pack
                .entry(id.clone())
                .or_default()
                .insert(pack.string_rodeo.resolve(sentence).to_string());
        }
    }

    let movies_dir = args
        .data_root
        .join(target.code())
        .join("sentence-sources/movies");
    let metadata = std::fs::read_to_string(movies_dir.join("metadata.jsonl"))
        .with_context(|| format!("reading {}", movies_dir.join("metadata.jsonl").display()))?;
    let segmenter = SubtitleSegmenter::for_language(target)?;
    let wanted: HashSet<&str> = args.movie.iter().map(String::as_str).collect();

    println!(
        "{:<11} {:>5} {:>6} {:>6} {:>6} {:>6}  title",
        "imdb", "cues", "pack", "now", "kept", "new"
    );
    let (mut t_cues, mut t_pack, mut t_now, mut t_kept, mut t_new, mut films) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let movies: Vec<language_utils::MovieMetadataBasic> = metadata
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let movies: Vec<_> = movies
        .into_iter()
        .filter(|m| wanted.is_empty() || wanted.contains(m.id.as_str()))
        .collect();

    // Segment every film in parallel (the slow step), then report in order.
    use rayon::prelude::*;
    let segmented: Vec<Option<(Vec<movie_subtitles::SubtitleLine>, HashSet<String>)>> = movies
        .par_iter()
        .map(|movie| {
            let Some((subtitles, _)) = movie_subtitles::load(&movies_dir, &movie.id)? else {
                return Ok(None);
            };
            let now: HashSet<String> = subtitle_sentences(&subtitles, target, &segmenter)
                .into_iter()
                .map(|s| cleanup_sentence(s, target))
                .collect();
            anyhow::Ok(Some((subtitles, now)))
        })
        .collect::<Result<_>>()?;

    for (movie, film) in movies.iter().zip(segmented) {
        let Some((subtitles, now)) = film else {
            continue;
        };
        if let Some(needle) = &args.trace {
            for passage in subtitle_passages(&subtitles) {
                if !passage.contains(needle.as_str()) {
                    continue;
                }
                println!(
                    "--- passage ({}):\n{}",
                    movie.id,
                    passage.replace('\n', "⏎\n")
                );
                for s in segmenter.segment(&passage) {
                    let mark = if should_include_sentence(s.trim(), target) {
                        "keep"
                    } else {
                        "drop"
                    };
                    println!("    [{mark}] {s}");
                }
            }
        }
        let had = in_pack.remove(&movie.id).unwrap_or_default();
        let kept = now.intersection(&had).count();
        let new = now.len() - kept;
        println!(
            "{:<11} {:>5} {:>6} {:>6} {:>6} {:>6}  {} ({})",
            movie.id,
            subtitles.len(),
            had.len(),
            now.len(),
            kept,
            new,
            movie.title,
            movie.year.map(|y| y.to_string()).unwrap_or_default()
        );
        if args.show_new {
            let mut fresh: Vec<&String> = now.difference(&had).collect();
            fresh.sort();
            for s in fresh {
                println!("    + {s}");
            }
            let mut lost: Vec<&String> = had.difference(&now).collect();
            lost.sort();
            for s in lost {
                println!("    - {s}");
            }
        }
        films += 1;
        t_cues += subtitles.len();
        t_pack += had.len();
        t_now += now.len();
        t_kept += kept;
        t_new += new;
    }
    println!(
        "\n{films} films: {t_cues} cues → pack has {t_pack}, ingestion now yields {t_now} \
         ({t_kept} already in pack, {t_new} new, {} of the pack's dropped)",
        t_pack - t_kept
    );
    Ok(())
}

fn load_pack(out_dir: &std::path::Path, course: &Course) -> Result<LanguagePack> {
    let dir = out_dir.join(format!(
        "{}_for_{}",
        course.target_language.code(),
        course.native_language.code()
    ));
    language_utils::language_pack::load_split_dir(&dir)
        .map_err(|e| anyhow::anyhow!("loading language pack in {}: {e}", dir.display()))
}
