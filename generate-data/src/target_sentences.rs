use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Context;
use language_utils::{Course, Language, SentenceSource};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Subtitle {
    sentence: String,
    start_ms: u32,
    end_ms: u32,
}

#[derive(Debug, Deserialize)]
struct PimsleurSentence {
    target_language: String,
    #[allow(dead_code)]
    native_language: String,
}

pub use language_utils::PimsleurLesson;

/// The result of collecting target sentences, split into app and restricted sets.
pub struct TargetSentences {
    /// Sentences that can be used in the app (Anki, Tatoeba, manual, movies, songs)
    pub app_sentences: Vec<(String, Option<String>, SentenceSource)>,
    /// Restricted sentences (e.g. Pimsleur) — used for frequency/tokenization only, not in the app.
    /// Each entry is (target_sentence, list of lessons it appears in).
    pub restricted_sentences: Vec<(String, Vec<PimsleurLesson>)>,
}

/// Default target maximum number of sentences to import from Tatoeba
const DEFAULT_TARGET_SENTENCE_COUNT: usize = 200_000;

/// Get target language sentences with optional translations and source information.
///
/// This function collects sentences from all available sources (Anki, Tatoeba, manual, songs, movies)
/// for a given course, split into app sentences and restricted sentences.
/// It does not perform Google Translate translations and does not write to cache files.
pub fn get_target_sentences(course: Course) -> anyhow::Result<TargetSentences> {
    let source_data_path = PathBuf::from(format!(
        "./generate-data/data/{}",
        course.target_language.iso_639_3()
    ));

    // Load banned sentences
    let banned_sentences = load_banned_sentences(&source_data_path)?;

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
    let movie_sentences = load_movie_sentences(&source_data_path, course.target_language)?;

    println!(
        "  Loaded sentences: Anki: {}, Tatoeba: {}, Movies: {}, Manual: {}",
        anki_sentences.len(),
        tatoeba_sentences.len(),
        movie_sentences.len(),
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

    // Manual sentences also need cleanup (they weren't cleaned up earlier)
    let app_sentences = result
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
        .collect();

    // Load restricted (Pimsleur) sentences
    let restricted_sentences = load_pimsleur_sentences(&source_data_path, course)?;

    println!(
        "  Loaded restricted sentences: Pimsleur: {}",
        restricted_sentences.len(),
    );

    Ok(TargetSentences {
        app_sentences,
        restricted_sentences,
    })
}

/// Load banned sentences from both manual and AI-generated files
fn load_banned_sentences(source_data_path: &std::path::Path) -> anyhow::Result<HashSet<String>> {
    let mut banned_sentences = HashSet::new();

    // Load manually created banned sentences
    let banned_sentences_file = source_data_path.join("banned_sentences.txt");
    if banned_sentences_file.exists() {
        let content = std::fs::read_to_string(&banned_sentences_file)
            .context("Failed to read banned sentences file")?;
        for line in content.lines() {
            let line = line.trim().to_lowercase();
            if !line.is_empty() {
                banned_sentences.insert(line);
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
                banned_sentences.insert(sentence.to_lowercase());
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
fn load_movie_sentences(
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

    let mut all_movie_sentences = Vec::new();

    for line in metadata_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let movie: language_utils::MovieMetadataBasic =
            serde_json::from_str(line).context("Failed to parse movie metadata")?;

        // Load subtitle file for this movie
        let subtitle_file = movies_dir.join(format!("subtitles/{}.jsonl", movie.id));
        if !subtitle_file.exists() {
            continue;
        }

        let subtitle_content = std::fs::read_to_string(&subtitle_file).with_context(|| {
            format!("Failed to read subtitle file: {}", subtitle_file.display())
        })?;

        // Parse all subtitle lines into Subtitle objects
        let mut parsed_subtitles = Vec::new();
        for line in subtitle_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let subtitle: Subtitle =
                serde_json::from_str(line).context("Failed to parse subtitle line")?;

            // Split multi-dialogue subtitles (e.g., "- Speaker 1 - Speaker 2")
            let split_subtitles = split_multi_dialogue_subtitle(subtitle);
            parsed_subtitles.extend(split_subtitles);
        }

        // Sanity check: verify subtitles are actually in the target language
        if !passes_language_sanity_check(&parsed_subtitles, language, &movie.id) {
            eprintln!(
                "Warning: subtitles for movie {} failed language sanity check, skipping",
                movie.id
            );
            continue;
        }

        let movie_sentences = filter_subtitle_sentences(&parsed_subtitles, &movie.id, language);
        all_movie_sentences.extend(movie_sentences);
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
        course.native_language.iso_639_3()
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
    subtitles: &[Subtitle],
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

/// Split subtitles that contain multiple speakers' dialogue or multiple sentences.
///
/// Subtitles often combine multiple speakers in one line like:
/// "- Speaker 1 dialogue - Speaker 2 dialogue"
/// or multiple sentences like:
/// "What are you doing? I don't know!"
///
/// This function splits them into separate subtitle objects.
fn split_multi_dialogue_subtitle(subtitle: Subtitle) -> Vec<Subtitle> {
    let mut sentences = vec![subtitle.sentence.clone()];

    // First, split on " - " if it starts with "- "
    if subtitle.sentence.starts_with("- ") {
        let parts: Vec<&str> = subtitle.sentence.split(" - ").collect();
        if parts.len() > 1 {
            sentences = parts
                .into_iter()
                .map(|s| s.trim_start_matches("- ").trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    // Then split on ? and ! (but not . to avoid splitting abbreviations)
    let mut final_sentences = Vec::new();
    for sentence in sentences {
        let mut current = String::new();

        for ch in sentence.chars() {
            current.push(ch);

            // If we hit ? or !, that's the end of a sentence
            if ch == '?' || ch == '!' {
                let trimmed = current.trim().trim_start_matches('-').trim().to_string();
                if !trimmed.is_empty() {
                    final_sentences.push(trimmed);
                }
                current = String::new();
            }
        }

        // Don't forget the last part if it doesn't end with ? or !
        let trimmed = current.trim().trim_start_matches('-').trim().to_string();
        if !trimmed.is_empty() {
            final_sentences.push(trimmed);
        }
    }

    // Convert to Subtitle objects
    final_sentences
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(|sentence| Subtitle {
            sentence,
            start_ms: subtitle.start_ms,
            end_ms: subtitle.end_ms,
        })
        .collect()
}

/// Filter and transform parsed subtitle objects into sentences with source information.
/// This function is separated for testability.
///
/// Subtitles often break long sentences into multiple pieces for display. This function
/// merges them back together when:
/// - Writing system is Latin (where capitalization matters)
/// - Current subtitle doesn't end with sentence-terminating punctuation
/// - Next subtitle starts at the same time (or within 1ms) as current one ends
/// - Next subtitle starts with a lowercase letter
///
/// # Arguments
///
/// * `subtitles` - Array of parsed subtitle objects
/// * `movie_id` - The ID of the movie these subtitles belong to
/// * `language` - The language of the subtitles for filtering
///
/// # Returns
///
/// A vector of tuples: (sentence, None, source_info)
fn filter_subtitle_sentences(
    subtitles: &[Subtitle],
    movie_id: &str,
    language: Language,
) -> Vec<(String, Option<String>, SentenceSource)> {
    let mut sentences = Vec::new();
    let is_latin = language.writing_system() == language_utils::WritingSystem::Latin;

    let mut i = 0;
    while i < subtitles.len() {
        let mut merged_sentence = subtitles[i].sentence.clone();
        let mut j = i + 1;

        // Merge consecutive subtitle fragments if they belong to the same sentence
        while j < subtitles.len() && is_latin {
            let current = &subtitles[j - 1];
            let next = &subtitles[j];

            // Check if we should merge
            let no_terminating_punctuation = !current.sentence.ends_with('.')
                && !current.sentence.ends_with('!')
                && !current.sentence.ends_with('?');

            let timing_matches = next.start_ms <= current.end_ms + 1;

            let next_starts_lowercase = next
                .sentence
                .chars()
                .next()
                .map(|c| c.is_lowercase())
                .unwrap_or(false);

            if no_terminating_punctuation && timing_matches && next_starts_lowercase {
                // Merge with a space
                merged_sentence.push(' ');
                merged_sentence.push_str(&next.sentence);
                j += 1;
            } else {
                break;
            }
        }

        // Filter out bad sentences (music markers, numbers, too short/long, etc.)
        if should_include_sentence(&merged_sentence, language) {
            let mut source = SentenceSource::none();
            source.movie_ids.push(movie_id.to_string());

            sentences.push((merged_sentence, None, source));
        }

        i = j;
    }

    sentences
}

/// Check if a sentence pair should be included based on filtering criteria
pub fn should_include_pair(target_sentence: &str, native_sentence: &str, course: Course) -> bool {
    should_include_sentence(target_sentence, course.target_language)
        && should_include_sentence(native_sentence, course.native_language)
}

/// Check if a single sentence should be included (for sources without translations like movies)
pub fn should_include_sentence(sentence: &str, language: Language) -> bool {
    // 1. Skip sentences that are too short or too long
    if sentence.len() < 5 || sentence.len() > 80 {
        return false;
    }

    // 2. Skip sentences ending with ellipsis
    if sentence.ends_with("...") {
        return false;
    }

    // 3. Skip sentences containing ellipsis anywhere
    if sentence.contains("...") {
        return false;
    }

    // 4. Skip music markers (common in subtitles)
    if sentence.contains('♪') {
        return false;
    }

    // 5. Check if sentence is "proper" according to language rules
    if !is_proper_sentence(sentence, language) {
        return false;
    }

    // 6. Skip sentences with multiple punctuation marks
    let punct_count = sentence.matches('.').count()
        + sentence.matches('!').count()
        + sentence.matches('?').count();

    if punct_count > 1 {
        return false;
    }

    // 7. Skip sentences with numbers
    if sentence.chars().any(|c| c.is_numeric()) {
        return false;
    }

    // 8. Skip sentences with encoding corruption or garbage characters
    if has_encoding_corruption(sentence) {
        return false;
    }

    // 9. Skip ALL_CAPS sentences (shouting in subtitles)
    {
        let alpha_chars: Vec<char> = sentence.chars().filter(|c| c.is_alphabetic()).collect();
        if alpha_chars.len() >= 2 && alpha_chars.iter().all(|c| c.is_uppercase()) {
            return false;
        }
    }

    true
}

/// Check if a sentence has encoding corruption or garbage characters that make it unusable.
fn has_encoding_corruption(sentence: &str) -> bool {
    // BOM character in text
    if sentence.contains('\u{FEFF}') {
        return true;
    }
    // &nbsp; HTML entity (common in corrupted subtitle files)
    if sentence.contains("&nbsp;") {
        return true;
    }
    sentence.chars().any(|c| {
        matches!(
            c,
            // Literal backslash (from corrupted subtitle escapes like \n, \h)
            '\\' |
        // @ symbol (not a real word)
        '@' |
        // MacRoman encoding artifacts (ˆ instead of à, Ž instead of é)
        '\u{02C6}' | '\u{017D}' |
        // Backtick and acute accent used as apostrophe in corrupted subtitles
        '`' | '\u{00B4}' |
        // Unicode replacement character (indicates failed decoding)
        '\u{FFFD}' |
        // Soft hyphen used incorrectly (e.g., as ¡ in Spanish OCR)
        '\u{00AD}' |
        // Zero-width space (typically from copy-paste corruption)
        '\u{200B}'
        ) || matches!(c,
            // C1 control characters indicate mojibake (e.g., U+009C instead of œ)
            '\u{0080}'..='\u{009F}' |
            // Greek letter homoglyphs mixed into Latin text (subtitle copy-protection).
            // These look identical to Latin letters but are different Unicode codepoints.
            '\u{0370}'..='\u{03FF}'
        )
    })
}

/// Check if a sentence is "proper" - language-specific validation
fn is_proper_sentence(text: &str, language: Language) -> bool {
    if text.is_empty() {
        return false;
    }

    // Reject sentences starting with dash/hyphen
    if text.starts_with('-') || text.starts_with('—') || text.starts_with('–') {
        return false;
    }

    let first_char = text.chars().next().unwrap();
    let last_char = text.chars().last().unwrap();

    // Language-specific checks
    match language {
        Language::English
        | Language::French
        | Language::Spanish
        | Language::German
        | Language::Portuguese
        | Language::Italian => {
            // Must start with uppercase letter
            if !first_char.is_uppercase() || !first_char.is_alphabetic() {
                return false;
            }

            // Must end with period, exclamation mark, or question mark
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Russian => {
            // Russian sentences should not contain Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must start with uppercase Cyrillic letter
            if !first_char.is_uppercase() {
                return false;
            }

            // Must end with period, exclamation mark, or question mark
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Chinese => {
            // Chinese sentences should not contain Latin letters (except maybe proper nouns)
            // But we'll be strict and reject any with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with Chinese or Western punctuation
            if last_char != '。'
                && last_char != '！'
                && last_char != '？'
                && last_char != '.'
                && last_char != '!'
                && last_char != '?'
            {
                return false;
            }
        }
        Language::Japanese => {
            // Japanese sentences should not contain Latin letters (except maybe proper nouns)
            // But we'll be strict and reject any with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with Japanese or Western punctuation
            if last_char != '。'
                && last_char != '！'
                && last_char != '？'
                && last_char != '.'
                && last_char != '!'
                && last_char != '?'
            {
                return false;
            }
        }
        Language::Korean => {
            // Korean sentences should not contain Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with appropriate Korean punctuation or period/exclamation/question
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Hindi => {
            // Devanagari script — reject sentences with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }
            // Must end with Devanagari danda, or Western punctuation
            if last_char != '।' && last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
    }

    // Reject sentences with quotes (often dialogue or non-standard)
    if text.contains('"') || text.contains('\'') || text.contains('"') || text.contains('"') {
        return false;
    }

    // Reject sentences with special characters that indicate non-standard text
    if text.contains('~') || text.contains('*') || text.contains('_') {
        return false;
    }

    // Reject sentences with colons (often speaker attribution in subtitles)
    if text.contains(':') {
        return false;
    }

    true
}
