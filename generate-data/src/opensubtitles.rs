use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use language_utils::{Course, Language};

pub struct SubtitlePair {
    pub target: String,
    pub native: String,
}

/// Read OpenSubtitles parallel corpus files and filter for proper sentences
pub fn get_subtitle_pairs(data_path: &Path, course: Course) -> Vec<SubtitlePair> {
    let mut pairs = Vec::new();

    // Look for OpenSubtitles files in the data directory
    let opensubtitles_dir = data_path.join("opensubtitles");
    if !opensubtitles_dir.exists() {
        eprintln!(
            "OpenSubtitles directory not found at: {}",
            opensubtitles_dir.display()
        );
        return pairs;
    }

    // Find all subdirectories (e.g., en-ko)
    let entries = match std::fs::read_dir(&opensubtitles_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read OpenSubtitles directory: {e}");
            return pairs;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        // Parse language pair (e.g., "en-ko")
        let parts: Vec<&str> = dir_name.split('-').collect();
        if parts.len() != 2 {
            continue;
        }

        // Determine which is English and which is the target language
        let (en_suffix, target_suffix) = if parts[0] == "en" {
            ("en", parts[1])
        } else if parts[1] == "en" {
            ("en", parts[0])
        } else {
            continue; // Skip if neither language is English
        };

        // Look for the parallel corpus files
        let en_file = path.join(format!(
            "OpenSubtitles.{}-{}.{}",
            parts[0], parts[1], en_suffix
        ));
        let target_file = path.join(format!(
            "OpenSubtitles.{}-{}.{}",
            parts[0], parts[1], target_suffix
        ));

        if !en_file.exists() || !target_file.exists() {
            eprintln!("Missing parallel files in {}", path.display());
            continue;
        }

        println!("Reading OpenSubtitles files from {}", path.display());

        // Read both files
        let en_reader = match File::open(&en_file) {
            Ok(f) => BufReader::new(f),
            Err(e) => {
                eprintln!("Failed to open English file: {e}");
                continue;
            }
        };

        let target_reader = match File::open(&target_file) {
            Ok(f) => BufReader::new(f),
            Err(e) => {
                eprintln!("Failed to open target language file: {e}");
                continue;
            }
        };

        // Read lines from both files in parallel
        let en_lines: Vec<String> = en_reader.lines().map_while(Result::ok).collect();
        let target_lines: Vec<String> = target_reader.lines().map_while(Result::ok).collect();

        if en_lines.len() != target_lines.len() {
            eprintln!(
                "Warning: Line count mismatch in {}: {} English lines vs {} target lines",
                path.display(),
                en_lines.len(),
                target_lines.len()
            );
        }

        let min_len = en_lines.len().min(target_lines.len());

        // Process pairs and filter
        for i in 0..min_len {
            let en_line = en_lines[i].trim();
            let target_line = target_lines[i].trim();

            // Filter: English must start with capital letter and end with . or !
            if !is_proper_sentence(en_line, course.native_language)
                || !is_proper_sentence(target_line, course.target_language)
            {
                continue;
            }

            // Skip empty lines or lines that are too short
            if target_line.len() < 2 || en_line.len() < 2 {
                continue;
            }

            // Skip sentences that are too long (more than 70 characters)
            if target_line.len() > 70 || en_line.len() > 70 {
                continue;
            }

            // Skip sentences containing ellipsis
            if target_line.contains("...") || en_line.contains("...") {
                continue;
            }

            pairs.push(SubtitlePair {
                target: target_line.to_string(),
                native: en_line.to_string(),
            });
        }

        println!(
            "Loaded {} filtered sentence pairs from OpenSubtitles",
            pairs.len()
        );
    }

    // Deduplicate based on target sentences
    let mut seen_targets = std::collections::HashSet::new();
    let unique_pairs: Vec<SubtitlePair> = pairs
        .into_iter()
        .filter(|pair| seen_targets.insert(pair.target.clone()))
        .collect();

    println!(
        "After deduplication: {} unique sentence pairs",
        unique_pairs.len()
    );

    unique_pairs
}

/// Check if a sentence is "proper" - starts with capital letter and ends with . or !
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

    // Must start with uppercase letter (and be alphabetic)
    if !first_char.is_uppercase() && [Language::English, Language::French].contains(&language) {
        return false;
    }

    // Must end with period or exclamation mark
    if last_char != '.' && last_char != '!' {
        return false;
    }

    // Must only contain one period or exclamation mark
    if text.matches('.').count() > 1 || text.matches('!').count() > 1 {
        return false;
    }

    // must not contain quotation marks
    if text.contains('"') || text.contains('\'') {
        return false;
    }

    // must not contain `~`
    if text.contains('~') {
        return false;
    }

    if language == Language::Korean {
        // the sentence should not contain any letters a-z or A-Z,
        if text
            .chars()
            .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
        {
            return false;
        }
    }

    // No numbers
    if text.chars().any(|c| c.is_numeric()) {
        return false;
    }

    true
}
