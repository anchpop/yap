//! Dry run of slot-loosened multiword-term matching against an existing
//! `out/<lang>/` directory, without running the rest of the pipeline.
//!
//! Prints, per (term, realization): match count, sampled precision, and
//! whether the pipeline's grading gate would keep it.
//!
//! Usage (from the repo root, after a normal generate-data run has produced
//! the tokenization files). The optional second argument prints every match
//! containing that substring, for spot-checking a specific sentence:
//!
//! ```sh
//! cargo run --release --bin slot_dry_run -- fra
//! cargo run --release --bin slot_dry_run -- fra "leur est arrivé"
//! ```

use anyhow::Context;
use generate_data::slot_analysis;
use language_utils::COURSES;
use std::collections::BTreeMap;
use std::io::BufRead;

fn load_tokenizations(
    path: &std::path::Path,
) -> anyhow::Result<BTreeMap<String, Vec<lexide::Token>>> {
    #[derive(serde::Deserialize)]
    struct Line {
        sentence: String,
        tokens: Vec<lexide::Token>,
    }
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut out = BTreeMap::new();
    for line in std::io::BufReader::new(file).lines() {
        let line: Line = serde_json::from_str(&line?)?;
        out.insert(line.sentence, line.tokens);
    }
    Ok(out)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();
    let lang_code = std::env::args()
        .nth(1)
        .context("usage: slot_dry_run <lang code, e.g. fra> [match substring to print]")?;
    let show_matches_containing = std::env::args().nth(2);
    let course = *COURSES
        .iter()
        .find(|c| c.target_language.code() == lang_code)
        .with_context(|| format!("no course with target language {lang_code:?}"))?;

    let out_dir = std::path::Path::new("out").join(&lang_code);
    let terms =
        load_tokenizations(&out_dir.join("target_language_multiword_terms_tokenization.jsonl"))?;
    let sentences =
        load_tokenizations(&out_dir.join("target_language_sentences_tokenization.jsonl"))?;
    println!("{} terms, {} sentences", terms.len(), sentences.len());

    let slot_specs = slot_analysis::analyze_slots(&course, &terms).await?;
    println!("{} terms with argument slots", slot_specs.len());

    let mut patterns = Vec::new();
    for (term, specs) in &slot_specs {
        for (realization, pattern) in slot_analysis::compile_realizations(&terms[term], specs) {
            patterns.push((term.clone(), realization, pattern));
        }
    }
    println!("{} loosened patterns", patterns.len());

    let matches = slot_analysis::find_slot_matches(
        &sentences,
        &patterns
            .iter()
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>(),
    );

    // Grade everything in one batch, same as the pipeline does.
    let requests: Vec<slot_analysis::GradeRequest> = patterns
        .iter()
        .zip(&matches)
        .filter(|(_, matched)| !matched.is_empty())
        .map(|((term, realization, _), matched)| {
            slot_analysis::GradeRequest::new(term.clone(), *realization, matched)
        })
        .collect();
    let graded = slot_analysis::grade_patterns(&requests).await?;

    let summary_path = out_dir.join("slot_patterns.tsv");
    slot_analysis::write_summary(&summary_path, &graded)?;

    let kept = graded.iter().filter(|g| g.kept()).count();
    let kept_matches: usize = graded
        .iter()
        .filter(|g| g.kept())
        .map(|g| g.match_count)
        .sum();
    println!(
        "\n{} patterns graded, {kept} kept, {kept_matches} sentence matches (details: {})",
        graded.len(),
        summary_path.display(),
    );

    // Top patterns by match count, as a quick eyeball.
    let mut rows: Vec<&slot_analysis::GradedPattern> = graded.iter().collect();
    rows.sort_by_key(|g| std::cmp::Reverse(g.match_count));
    println!(
        "\n{:<50} {:<8} {:>8} {:>10} {:>6}",
        "term", "realiz.", "matches", "precision", "kept"
    );
    for g in rows.iter().take(25) {
        println!(
            "{:<50} {:<8} {:>8} {:>7}/{:<2} {:>6}",
            g.term,
            g.realization.to_string(),
            g.match_count,
            g.good(),
            g.total(),
            if g.kept() { "yes" } else { "NO" },
        );
    }

    if let Some(needle) = &show_matches_containing {
        println!("\nmatches containing {needle:?}:");
        for ((term, realization, _), matched) in patterns.iter().zip(&matches) {
            for m in matched
                .iter()
                .filter(|m| m.sentence.contains(needle.as_str()))
            {
                println!(
                    "  {term:?} [{realization}]: {} (bound: {})",
                    m.sentence,
                    m.matched_words.join(" \u{2026} ")
                );
            }
        }
    }

    Ok(())
}
