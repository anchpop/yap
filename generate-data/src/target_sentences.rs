use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Context;
use language_utils::{Course, Language, SentenceSource};
use movie_subtitles::SubtitleLine;
use movie_subtitles::segment::{RuleSegmenter, SubtitleSegmenter};
use movie_subtitles::sentences::KeyedSentence;
pub use movie_subtitles::sentences::{
    has_encoding_corruption, has_quote_apostrophe, is_proper_sentence, should_include_sentence,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PimsleurSentence {
    target_language: String,
    #[allow(dead_code)]
    native_language: String,
}

pub use language_utils::PimsleurLesson;

/// The result of collecting target sentences, split into app, book, and restricted sets.
pub struct TargetSentences {
    /// Sentences that can be used in the app (Anki, Tatoeba, manual, movies, songs)
    pub app_sentences: Vec<(String, Option<String>, SentenceSource)>,
    /// Sentences whose only source is book prose. Kept out of `app_sentences` (and thus
    /// the language packs) for now: the NLP models generate-data relies on were trained
    /// on the non-book distribution and mislabel book prose. clean-nlp-data DOES include
    /// these — the gold data it produces is what will retrain the models on book
    /// distribution. A sentence that also appears in another source stays in
    /// `app_sentences` (with its `book_ids` intact).
    pub book_sentences: Vec<(String, Option<String>, SentenceSource)>,
    /// Restricted sentences (e.g. Pimsleur) — used for frequency/tokenization only, not in the app.
    /// Each entry is (target_sentence, list of lessons it appears in).
    pub restricted_sentences: Vec<(String, Vec<PimsleurLesson>)>,
}

/// Default target maximum number of sentences to import from Tatoeba
const DEFAULT_TARGET_SENTENCE_COUNT: usize = 200_000;

/// macOS XProtect's `XProtect_MACOS_DAILYDUMPLING_UNST` yara rule SIGKILLs
/// (silently, with no log entry) any locally built Mach-O binary matching
/// ALL of {"That's strange", "brainpoolP224r1", "wonder"} AND one of
/// {"Welcome to H3LL", "Welcome to Paradise"}. Our subtitle corpus is
/// embedded into binaries via the language packs, so a single line of movie
/// dialogue can make every local `ai-backend` test binary unrunnable.
///
/// Hacky workaround: ban the second group from the corpus (sentences and
/// translations alike), which defeats the rule's AND condition. Only this
/// group is bannable — "wonder" and "That's strange" appear in thousands of
/// legitimate sentences, and "brainpoolP224r1" comes from crypto crates.
///
/// The yara rule is case-sensitive (capital W/P/H), so the lowercase
/// literals below don't themselves trip it — don't "fix" their casing.
pub fn contains_xprotect_tripwire(s: &str) -> bool {
    let lower = s.to_lowercase();
    ["welcome to paradise", "welcome to h3ll"]
        .iter()
        .any(|tripwire| lower.contains(tripwire))
}

/// Get target language sentences with optional translations and source information.
///
/// This function collects sentences from all available sources (Anki, Tatoeba, manual, songs, movies)
/// for a given course, split into app sentences and restricted sentences.
/// It does not perform Google Translate translations and does not write to cache files.
pub async fn get_target_sentences(course: Course) -> anyhow::Result<TargetSentences> {
    let source_data_path = PathBuf::from(format!(
        "./generate-data/data/{}",
        course.target_language.code()
    ));

    // Load banned sentences
    let banned_sentences = load_banned_sentences(&source_data_path, course.target_language)?;

    // Load manual sentences (should NEVER be filtered)
    let manual_sentences = load_manual_sentences(&source_data_path)?;

    // Get all data sources
    let all_cards = crate::read_anki::get_all_cards(&source_data_path);
    let tatoeba_pairs =
        crate::tatoeba::get_tatoeba_pairs(&source_data_path, course, DEFAULT_TARGET_SENTENCE_COUNT);

    // Extract target sentences from Anki cards with their native translations
    let use_native_card_side = course.native_language == language_utils::Language::English;
    let anki_sentences = all_cards
        .iter()
        .flat_map(|card| {
            card.target.iter().map(|target_language_sentence| {
                let native_sentence = if use_native_card_side {
                    let trimmed_native = card.english.trim();
                    if trimmed_native.is_empty() {
                        None
                    } else {
                        Some(trimmed_native.to_string())
                    }
                } else {
                    None
                };
                let mut source = SentenceSource::none();
                source.from_anki = true;
                (target_language_sentence.clone(), native_sentence, source)
            })
        })
        .collect::<Vec<_>>();

    // Extract target sentences from Tatoeba pairs with their translations
    let tatoeba_sentences = tatoeba_pairs.iter().map(|pair| {
        let native_sentence = if course.native_language == language_utils::Language::English {
            let trimmed_native = pair.native.trim();
            if trimmed_native.is_empty() {
                None
            } else {
                Some(trimmed_native.to_string())
            }
        } else {
            None
        };
        let mut source = SentenceSource::none();
        source.from_tatoeba = true;
        (pair.target.clone(), native_sentence, source)
    });

    // Load movie sentences
    let movie_sentences = load_movie_sentences(&source_data_path, course.target_language).await?;

    // Load book sentences (translated + segmented book prose; see crate::books)
    let book_sentences = crate::books::load_book_sentences(&source_data_path)?;

    println!(
        "  Loaded sentences: Anki: {}, Tatoeba: {}, Movies: {}, Books: {}, Manual: {}",
        anki_sentences.len(),
        tatoeba_sentences.len(),
        movie_sentences.len(),
        book_sentences.len(),
        manual_sentences.len(),
    );

    // Add manual sentences with source tracking
    let manual_sentences_iter = manual_sentences.into_iter().map(|sentence| {
        let mut source = SentenceSource::none();
        source.from_manual = true;
        (sentence, None, source)
    });

    // Combine all sentences
    // Apply cleanup BEFORE checking banned sentences to ensure proper matching
    let all_sentences: Vec<(String, Option<String>, SentenceSource)> = anki_sentences
        .into_iter()
        .chain(tatoeba_sentences)
        .chain(movie_sentences) // Add movie sentences
        .chain(
            book_sentences
                .into_iter()
                .map(|(sentence, source)| (sentence, None, source)),
        )
        .map(|(sentence, native, source)| {
            (
                language_utils::text_cleanup::cleanup_sentence(sentence, course.target_language),
                native,
                source,
            )
        })
        .filter(|(sentence, _, source)| {
            // Never filter manual sentences
            source.is_manual() || !banned_sentences.contains(&sentence.to_lowercase())
        })
        .filter(|(sentence, _, source)| source.is_manual() || !has_encoding_corruption(sentence))
        .chain(manual_sentences_iter)
        // Applied after the manual-sentence chain on purpose: unlike the
        // quality filters above, this one must hold even for manual sentences.
        .filter(|(sentence, native, _)| {
            !contains_xprotect_tripwire(sentence)
                && !native.as_deref().is_some_and(contains_xprotect_tripwire)
        })
        .collect();

    // Deduplicate by target sentence while preserving order
    // When there are duplicates, prefer entries with translations and merge sources
    let mut result: Vec<(String, Option<String>, SentenceSource)> = Vec::new();
    let mut target_to_index: HashMap<String, usize> = HashMap::new();

    for (target, native, source) in all_sentences {
        if let Some(&existing_index) = target_to_index.get(&target) {
            result[existing_index].2.merge(&source);
            if result[existing_index].1.is_none() && native.is_some() {
                result[existing_index].1 = native;
            }
        } else {
            let index = result.len();
            target_to_index.insert(target.clone(), index);
            result.push((target, native, source));
        }
    }

    // Manual sentences also need cleanup (they weren't cleaned up earlier).
    // Book-only sentences are split off so they stay out of the language packs
    // (see the field docs on `TargetSentences`).
    let (book_sentences, app_sentences): (Vec<_>, Vec<_>) = result
        .into_iter()
        .map(|(sentence, native, source)| {
            if source.is_manual() {
                (
                    language_utils::text_cleanup::cleanup_sentence(
                        sentence,
                        course.target_language,
                    ),
                    native,
                    source,
                )
            } else {
                // Already cleaned up
                (sentence, native, source)
            }
        })
        .partition(|(_, _, source)| source.is_book_only());

    // Load restricted (Pimsleur) sentences
    let restricted_sentences = load_pimsleur_sentences(&source_data_path, course)?;

    println!(
        "  Loaded restricted sentences: Pimsleur: {}",
        restricted_sentences.len(),
    );

    Ok(TargetSentences {
        app_sentences,
        book_sentences,
        restricted_sentences,
    })
}

/// Load banned sentences from both manual and AI-generated files.
///
/// Entries are normalized through `cleanup_sentence` so they match the form
/// that input sentences take after cleanup (e.g. thin NBSP before `?` / `!`
/// in French).
fn load_banned_sentences(
    source_data_path: &std::path::Path,
    language: Language,
) -> anyhow::Result<HashSet<String>> {
    let mut banned_sentences = HashSet::new();

    let normalize = |raw: &str| {
        language_utils::text_cleanup::cleanup_sentence(raw.trim().to_string(), language)
            .to_lowercase()
    };

    // Load manually created banned sentences
    let banned_sentences_file = source_data_path.join("banned_sentences.txt");
    if banned_sentences_file.exists() {
        let content = std::fs::read_to_string(&banned_sentences_file)
            .context("Failed to read banned sentences file")?;
        for line in content.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let normalized = normalize(line);
            if !normalized.is_empty() {
                banned_sentences.insert(normalized);
            }
        }
    }

    // Load AI-generated banned sentences
    let ai_banned_file = source_data_path.join("banned_sentences_ai.txt");
    if ai_banned_file.exists() {
        let content = std::fs::read_to_string(&ai_banned_file)
            .context("Failed to read AI banned sentences file")?;
        for line in content.lines() {
            // Parse JSON to extract just the sentence
            if let Ok(banned_entry) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(sentence) = banned_entry.get("sentence").and_then(|s| s.as_str())
            {
                banned_sentences.insert(normalize(sentence));
            }
        }
    }

    Ok(banned_sentences)
}

/// Load manual sentences from the extra/manual.txt file
/// These sentences should NEVER be filtered out
fn load_manual_sentences(source_data_path: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let mut manual_sentences = Vec::new();

    let manual_file = source_data_path.join("sentence-sources/extra/manual.txt");
    if manual_file.exists() {
        let content = std::fs::read_to_string(&manual_file)
            .context("Failed to read manual sentences file")?;
        for line in content.lines() {
            let line = line.trim().to_string();
            if !line.is_empty() {
                manual_sentences.push(line);
            }
        }
    }

    Ok(manual_sentences)
}

/// Load movie sentences from OpenSubtitles data
async fn load_movie_sentences(
    source_data_path: &std::path::Path,
    language: Language,
) -> anyhow::Result<Vec<(String, Option<String>, SentenceSource)>> {
    let movies_dir = source_data_path.join("sentence-sources/movies");

    // If movies directory doesn't exist, return empty vec
    if !movies_dir.exists() {
        return Ok(vec![]);
    }

    let metadata_file = movies_dir.join("metadata.jsonl");
    if !metadata_file.exists() {
        return Ok(vec![]);
    }

    // Load movie metadata
    let metadata_content =
        std::fs::read_to_string(&metadata_file).context("Failed to read movie metadata file")?;

    let movies: Vec<language_utils::MovieMetadataBasic> = metadata_content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).context("Failed to parse movie metadata"))
        .collect::<anyhow::Result<_>>()?;
    let segmenter = SubtitleSegmenter::for_language(language)?;

    // Loading and the language check are per film and cheap; films are
    // processed in parallel and results keep metadata order.
    use rayon::prelude::*;
    let loaded: Vec<Option<(movie_subtitles::Source, Vec<SubtitleLine>)>> = movies
        .par_iter()
        .map(|movie| {
            // Prefer the raw SRT and clean it here, in memory, so improvements to
            // the cleaning rules reach every course on the next build. Movies whose
            // raw SRT was never kept fall back to the pre-cleaned JSONL.
            let Some((subtitles, source)) = movie_subtitles::load(&movies_dir, &movie.id)? else {
                return Ok(None);
            };

            // Sanity check: verify subtitles are actually in the target language
            if !passes_language_sanity_check(&subtitles, language, &movie.id) {
                eprintln!(
                    "Warning: subtitles for movie {} failed language sanity check, skipping",
                    movie.id
                );
                return Ok(None);
            }
            Ok(Some((source, subtitles)))
        })
        .collect::<anyhow::Result<_>>()?;
    let films: Vec<(
        &language_utils::MovieMetadataBasic,
        movie_subtitles::Source,
        Vec<SubtitleLine>,
    )> = movies
        .iter()
        .zip(loaded)
        .filter_map(|(movie, loaded)| loaded.map(|(source, subtitles)| (movie, source, subtitles)))
        .collect();

    type MovieSentences = Vec<(String, Option<String>, SentenceSource)>;
    let per_movie: Vec<(movie_subtitles::Source, MovieSentences)> = match &segmenter {
        // Segmentation is the expensive step (tens of seconds for a long
        // film), so films are segmented in parallel.
        SubtitleSegmenter::Rules(rules) => films
            .par_iter()
            .map(|(movie, source, subtitles)| {
                let keyed = movie_subtitles::sentences::keyed_sentences_by_rules(
                    subtitles, language, rules,
                );
                (*source, attributed(keyed, &movie.id))
            })
            .collect(),
        // One Batch API round trip for the whole course: every cue of every
        // film is one request, answered from the cache after the first run.
        SubtitleSegmenter::Llm(_) => {
            let client = crate::apply_cache_only(movie_subtitles::llm_segment::client()?);
            let prepared: Vec<Vec<SubtitleLine>> = films
                .iter()
                .map(|(_, _, subtitles)| movie_subtitles::sentences::prepared_lines(subtitles))
                .collect();
            let tracks: Vec<(&[SubtitleLine], Language)> =
                prepared.iter().map(|p| (p.as_slice(), language)).collect();
            let cues: usize = tracks.iter().map(|(t, _)| t.len()).sum();
            println!(
                "  segmenting {} films ({cues} cues) with {}",
                films.len(),
                movie_subtitles::llm_segment::MODEL
            );
            let (splits, report) = movie_subtitles::llm_segment::split_tracks(
                &client,
                &tracks,
                movie_subtitles::llm_segment::print_progress(),
            )
            .await?;
            if report.fallbacks > 0 {
                println!(
                    "  {} of the {} cues put to the model fell back to per-cue segmentation",
                    report.fallbacks, report.asked
                );
            }
            films
                .iter()
                .zip(&prepared)
                .zip(&splits)
                .map(|(((movie, source, _), lines), splits)| {
                    let keyed = movie_subtitles::sentences::keyed_sentences_from_splits(
                        lines, splits, language,
                    );
                    (*source, attributed(keyed, &movie.id))
                })
                .collect()
        }
    };

    let mut all_movie_sentences = Vec::new();
    let (mut from_raw, mut from_derived) = (0usize, 0usize);
    for (source, sentences) in per_movie {
        match source {
            movie_subtitles::Source::RawSrt => from_raw += 1,
            movie_subtitles::Source::DerivedJsonl => from_derived += 1,
        }
        all_movie_sentences.extend(sentences);
    }

    if from_derived > 0 {
        println!(
            "  movies: {from_raw} cleaned from raw SRT, {from_derived} still on the pre-cleaned \
             JSONL (run recover-subtitles to close the gap)"
        );
    } else if from_raw > 0 {
        println!("  movies: all {from_raw} cleaned from raw SRT");
    }

    Ok(all_movie_sentences)
}

/// Load Pimsleur sentences (restricted/copyrighted).
///
/// Directory structure: `sentence-sources/pimsleur/for_{native_iso}/level_{N}/unit_{NN}/sentences.jsonl`
///
/// Each sentence may appear in multiple lessons, so we deduplicate and collect all lessons per sentence.
fn load_pimsleur_sentences(
    source_data_path: &std::path::Path,
    course: Course,
) -> anyhow::Result<Vec<(String, Vec<PimsleurLesson>)>> {
    let pimsleur_dir = source_data_path.join(format!(
        "sentence-sources/pimsleur/for_{}",
        course.native_language.code()
    ));

    if !pimsleur_dir.exists() {
        return Ok(vec![]);
    }

    // Collect all (sentence, lesson) pairs, then deduplicate
    let mut sentence_to_lessons: HashMap<String, Vec<PimsleurLesson>> = HashMap::new();

    // Iterate over level directories
    let mut level_dirs: Vec<_> = std::fs::read_dir(&pimsleur_dir)
        .context("Failed to read pimsleur directory")?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    level_dirs.sort_by_key(|e| e.file_name());

    for level_entry in level_dirs {
        let level_name = level_entry.file_name();
        let level_str = level_name.to_string_lossy();
        let Some(level_num) = level_str
            .strip_prefix("level_")
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };

        // Iterate over unit directories within this level
        let mut unit_dirs: Vec<_> = std::fs::read_dir(level_entry.path())
            .context("Failed to read pimsleur level directory")?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();
        unit_dirs.sort_by_key(|e| e.file_name());

        for unit_entry in unit_dirs {
            let unit_name = unit_entry.file_name();
            let unit_str = unit_name.to_string_lossy();
            let Some(unit_num) = unit_str
                .strip_prefix("unit_")
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };

            let sentences_file = unit_entry.path().join("sentences.jsonl");
            if !sentences_file.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&sentences_file).with_context(|| {
                format!(
                    "Failed to read pimsleur sentences: {}",
                    sentences_file.display()
                )
            })?;

            let lesson = PimsleurLesson {
                level: level_num,
                lesson: unit_num,
            };

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let entry: PimsleurSentence = serde_json::from_str(line).with_context(|| {
                    format!(
                        "Failed to parse pimsleur sentence in {}",
                        sentences_file.display()
                    )
                })?;

                let cleaned = language_utils::text_cleanup::cleanup_sentence(
                    entry.target_language,
                    course.target_language,
                );

                if !should_include_sentence(&cleaned, course.target_language) {
                    continue;
                }

                sentence_to_lessons
                    .entry(cleaned)
                    .or_default()
                    .push(lesson.clone());
            }
        }
    }

    Ok(sentence_to_lessons.into_iter().collect())
}

/// Check if subtitle lines pass the language sanity check.
fn passes_language_sanity_check(
    subtitles: &[SubtitleLine],
    language: Language,
    movie_id: &str,
) -> bool {
    let skip_markers = sanity_check_skip_markers(language, movie_id);
    let skip_refs: Vec<&str> = skip_markers.to_vec();
    match language.check_subtitle_sanity(subtitles.iter().map(|s| s.sentence.as_str()), &skip_refs)
    {
        Ok(()) => true,
        Err(reason) => {
            eprintln!("  Sanity check failed: {reason}");
            false
        }
    }
}

/// Returns corruption markers to skip for a specific (language, movie) pair.
/// This allows whitelisting files where markers produce false positives
/// (e.g., a character named "Rourke" triggering the French "rour" marker).
fn sanity_check_skip_markers(language: Language, movie_id: &str) -> Vec<&'static str> {
    match (language, movie_id) {
        // Atlantis: character named "Rourke"
        (Language::French, "tt0230011") => vec!["rour"],
        _ => vec![],
    }
}

/// Course-worthy sentences of a subtitle track, spelled as the pack keys
/// them. The segmentation, filtering and keying all live in
/// [`movie_subtitles::sentences::keyed_sentences`] — the same code the
/// subtitle corpus's clip mapping runs, so a pack sentence and its clip
/// agree byte-for-byte.
pub async fn subtitle_sentences(
    subtitles: &[SubtitleLine],
    language: Language,
    segmenter: &SubtitleSegmenter,
) -> anyhow::Result<Vec<String>> {
    Ok(course_sentences(
        movie_subtitles::sentences::keyed_sentences(subtitles, language, segmenter).await?,
    ))
}

/// [`subtitle_sentences`] for a language segmented by rules.
pub fn subtitle_sentences_by_rules(
    subtitles: &[SubtitleLine],
    language: Language,
    segmenter: &RuleSegmenter,
) -> Vec<String> {
    course_sentences(movie_subtitles::sentences::keyed_sentences_by_rules(
        subtitles, language, segmenter,
    ))
}

fn course_sentences(keyed: Vec<KeyedSentence>) -> Vec<String> {
    keyed
        .into_iter()
        .filter(|s| s.course_worthy)
        .map(|s| s.sentence)
        .collect()
}

/// A track's course-worthy sentences, tagged with the movie as source.
fn attributed(
    keyed: Vec<KeyedSentence>,
    movie_id: &str,
) -> Vec<(String, Option<String>, SentenceSource)> {
    course_sentences(keyed)
        .into_iter()
        .map(|sentence| {
            let mut source = SentenceSource::none();
            source.movie_ids.push(movie_id.to_string());
            (sentence, None, source)
        })
        .collect()
}

/// Check if a sentence pair should be included based on filtering criteria
pub fn should_include_pair(target_sentence: &str, native_sentence: &str, course: Course) -> bool {
    should_include_sentence(target_sentence, course.target_language)
        && should_include_sentence(native_sentence, course.native_language)
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
    fn elision_apostrophes_are_not_quotes() {
        for ok in [
            "C'est bien de l'épaule.",
            "M'aimez-vous, ma douce ?",
            "Don't stop.",
        ] {
            assert!(!has_quote_apostrophe(ok), "{ok}");
            assert!(is_proper_sentence(ok, Language::French), "{ok}");
        }
        for quote in ["'Bonjour', dit-il.", "Il a dit 'non'.", "Rock 'n roll."] {
            assert!(has_quote_apostrophe(quote), "{quote}");
        }
    }

    /// End-to-end through the real segmenter. Skipped when the weights aren't
    /// on this machine (`LEXIDE_MODEL_DIR`, as lexide's own tests use).
    #[test]
    fn multi_sentence_cues_are_split_by_the_segmenter() {
        let Ok(dir) = std::env::var("LEXIDE_MODEL_DIR") else {
            eprintln!("LEXIDE_MODEL_DIR unset; skipping");
            return;
        };
        if !std::path::Path::new(&dir)
            .join("sentence_segmenter.safetensors")
            .exists()
        {
            eprintln!("no segmenter weights in LEXIDE_MODEL_DIR; skipping");
            return;
        }
        let SubtitleSegmenter::Rules(segmenter) =
            SubtitleSegmenter::for_language(Language::French).unwrap()
        else {
            panic!("French is segmented by rules")
        };
        let cues = [
            cue("On va la dépecer vive ! Lui arracher la langue !", 0, 2_000),
            cue("M. Godefroy ! Vous voilà.", 2_100, 3_000),
            cue("- Où est mon Daniel ? - Il est là.", 3_100, 4_000),
        ];
        assert_eq!(
            subtitle_sentences_by_rules(&cues, Language::French, &segmenter),
            vec![
                "On va la dépecer vive !",
                "Lui arracher la langue !",
                "M. Godefroy !",
                "Vous voilà.",
                "Où est mon Daniel ?",
                "Il est là.",
            ]
        );
    }
}
