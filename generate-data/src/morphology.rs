//! Morphological segmentation: align canonical morphemes to surface forms,
//! then train a character-level unigram model to segment any word.

use omnigram::unigram::{Seq, UnigramTrainer, UnigramTrainerConfig};
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A canonical morpheme entry: word + list of canonical morphemes
#[derive(Debug, Clone)]
pub struct CanonicalEntry {
    pub word: String,
    pub morphemes: Vec<String>,
}

/// A surface-aligned morpheme entry: word + list of surface substrings
#[derive(Debug, Clone)]
pub struct AlignedEntry {
    pub word: String,
    pub segments: Vec<String>,
}

// --- Parsing ---

/// Parse canonical_morphemes.tsv into entries
pub fn parse_canonical_morphemes(path: &Path) -> Vec<CanonicalEntry> {
    let file =
        File::open(path).unwrap_or_else(|e| panic!("Failed to open {}: {e}", path.display()));
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue; // Need word + at least 2 morphemes
        }
        let word = parts[0].to_string();
        let morphemes: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        entries.push(CanonicalEntry { word, morphemes });
    }

    entries
}

// --- Alignment ---

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            } else {
                dp[i][j] = 1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
            }
        }
    }
    dp[m][n]
}

/// Check if a string contains a vowel (Latin-script heuristic).
fn has_vowel(s: &str) -> bool {
    const VOWELS: &str = "aeiouyàâäéèêëïîôùûüœæáíóúñãõąęółśźżäöü";
    s.chars()
        .any(|c| VOWELS.contains(c.to_lowercase().next().unwrap_or(c)))
}

/// Align canonical morphemes to surface substrings of a word.
/// Returns None if alignment fails.
pub fn align_morphemes(word: &str, morphemes: &[String]) -> Option<Vec<String>> {
    let n = morphemes.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some(vec![word.to_string()]);
    }

    let chars: Vec<char> = word.chars().collect();
    let word_len = chars.len();

    if n > word_len {
        return None;
    }

    // For <= 6 morphemes, try all split points (combinatorial)
    if n <= 6 {
        brute_force_align(&chars, morphemes)
    } else {
        Some(greedy_align(&chars, morphemes))
    }
}

fn brute_force_align(chars: &[char], morphemes: &[String]) -> Option<Vec<String>> {
    let n = morphemes.len();
    let word_len = chars.len();

    if n > word_len {
        return None;
    }

    let mut best_score = usize::MAX;
    let mut best_split: Option<Vec<String>> = None;

    // Generate all combinations of (n-1) split points from (1..word_len)
    let positions: Vec<usize> = (1..word_len).collect();
    for combo in combinations(&positions, n - 1) {
        let mut parts = Vec::with_capacity(n);
        let mut prev = 0;
        for &split in &combo {
            parts.push(chars[prev..split].iter().collect::<String>());
            prev = split;
        }
        parts.push(chars[prev..].iter().collect::<String>());

        let mut score: usize = 0;
        for (part, morph) in parts.iter().zip(morphemes.iter()) {
            score += edit_distance(part, morph);
            if !has_vowel(part) {
                score += 2;
            }
        }

        if score < best_score {
            best_score = score;
            best_split = Some(parts);
        }
    }

    best_split
}

fn greedy_align(chars: &[char], morphemes: &[String]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut pos = 0;

    for (i, morph) in morphemes.iter().enumerate() {
        let remaining = morphemes.len() - i - 1;
        let chars_left = chars.len() - pos;
        let min_take = 1.max(chars_left.saturating_sub(remaining));
        let max_take = chars_left - remaining;

        let mut best_ed = usize::MAX;
        let mut best_len = min_take;
        for take in min_take..=max_take {
            let substr: String = chars[pos..pos + take].iter().collect();
            let ed = edit_distance(&substr, morph);
            let penalty = if has_vowel(&substr) { 0 } else { 2 };
            if ed + penalty < best_ed {
                best_ed = ed + penalty;
                best_len = take;
            }
        }

        parts.push(chars[pos..pos + best_len].iter().collect());
        pos += best_len;
    }

    parts
}

/// Simple combinations generator
fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    if k == 0 {
        return vec![vec![]];
    }
    if items.len() < k {
        return vec![];
    }
    let mut result = Vec::new();
    combinations_helper(items, k, 0, &mut Vec::new(), &mut result);
    result
}

fn combinations_helper(
    items: &[usize],
    k: usize,
    start: usize,
    current: &mut Vec<usize>,
    result: &mut Vec<Vec<usize>>,
) {
    if current.len() == k {
        result.push(current.clone());
        return;
    }
    let remaining = k - current.len();
    for i in start..=items.len() - remaining {
        current.push(items[i]);
        combinations_helper(items, k, i + 1, current, result);
        current.pop();
    }
}

// --- Batch alignment ---

/// Align all canonical entries to surface forms.
/// Returns successfully aligned entries.
pub fn align_all(entries: &[CanonicalEntry]) -> Vec<AlignedEntry> {
    entries
        .iter()
        .filter_map(|entry| {
            let segments = align_morphemes(&entry.word, &entry.morphemes)?;
            // Sanity: segments must concatenate to original word
            let reconstructed: String = segments.concat();
            if reconstructed != entry.word {
                return None;
            }
            // Skip if all segments are single characters (trivial)
            if segments.iter().all(|s| s.chars().count() <= 1) {
                return None;
            }
            Some(AlignedEntry {
                word: entry.word.clone(),
                segments,
            })
        })
        .collect()
}

// --- Unigram training with fixed morphology counts ---

/// Compute fixed expected counts from aligned entries.
/// For each aligned word, each segment contributes 1.0 to its char sequence count.
/// This is what EM *would* compute for these words if their segmentation were forced.
fn compute_fixed_counts(aligned: &[AlignedEntry]) -> FxHashMap<Seq<char>, f64> {
    let mut counts: FxHashMap<Seq<char>, f64> = FxHashMap::default();

    for entry in aligned {
        let full_pchars = entry.word.chars().collect::<Vec<_>>();
        let mut pos = 0;
        for seg in &entry.segments {
            let seg_len = seg.chars().count();
            if pos + seg_len <= full_pchars.len() {
                let seq = Seq::new(full_pchars[pos..pos + seg_len].to_vec());
                *counts.entry(seq).or_insert(0.0) += 1.0;
            }
            pos += seg_len;
        }
    }

    counts
}

/// Train a morphological unigram model from canonical morphemes + word list.
///
/// The key insight: words with known canonical breakdowns don't need to go through EM.
/// We precompute their segment counts and inject them as fixed counts into the EM loop.
/// EM only runs on unknown words, but is biased by the known morpheme frequencies.
/// Build morphological segmentations for a set of words.
///
/// Returns a map from word → surface segments (e.g. "destabilization" → ["de", "stabil", "iz", "ation"]).
///
/// For words that appear in canonical_morphemes.tsv, the segmentation comes from
/// aligning the canonical morphemes to the surface form. For all other words,
/// a unigram model (trained with fixed counts from the known segmentations) is used.
pub fn build_morphology_segmentations(
    canonical_path: &Path,
    words: &[String],
    target_pieces: usize,
) -> std::collections::BTreeMap<String, Vec<String>> {
    use std::collections::BTreeMap;

    // 1. Parse canonical morphemes
    let entries = parse_canonical_morphemes(canonical_path);
    println!(
        "  Parsed {} canonical morpheme entries from {}",
        entries.len(),
        canonical_path.display()
    );

    // 2. Align to surface forms
    let aligned = align_all(&entries);
    println!("  Aligned {} entries to surface forms", aligned.len());

    // 3. Build result map with known segmentations (only for requested words)
    let requested: std::collections::HashSet<&str> = words.iter().map(|w| w.as_str()).collect();
    let aligned_lookup: std::collections::HashMap<&str, &AlignedEntry> = aligned
        .iter()
        .filter(|e| requested.contains(e.word.as_str()))
        .map(|e| (e.word.as_str(), e))
        .collect();

    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (word, entry) in &aligned_lookup {
        result.insert(word.to_string(), entry.segments.clone());
    }

    // 4. Find requested words that need unigram segmentation
    let unknown_words: Vec<&String> = words
        .iter()
        .filter(|w| !result.contains_key(w.as_str()))
        .collect();
    println!(
        "  {} words already known, {} need unigram segmentation",
        words.len() - unknown_words.len(),
        unknown_words.len()
    );

    if unknown_words.is_empty() {
        return result;
    }

    // 5. Compute fixed counts from known segmentations
    let fixed_counts = compute_fixed_counts(&aligned);
    println!(
        "  Fixed counts: {} unique segments from known words",
        fixed_counts.len()
    );

    // 6. Build EM corpus: unknown words + a sample of known words
    let unknown_corpus: Vec<Vec<char>> = unknown_words
        .iter()
        .map(|w| w.chars().collect::<Vec<_>>())
        .collect();

    let max_known_for_em = 50_000;
    let step = (aligned.len() / max_known_for_em).max(1);
    let known_sample: Vec<Vec<char>> = aligned
        .iter()
        .step_by(step)
        .take(max_known_for_em)
        .map(|e| e.word.chars().collect::<Vec<_>>())
        .collect();

    let mut corpus = unknown_corpus;
    let unknown_count = corpus.len();
    corpus.extend(known_sample);
    println!(
        "  EM corpus: {} unknown + {} sampled known = {} total",
        unknown_count,
        corpus.len() - unknown_count,
        corpus.len()
    );

    // 7. Train unigram model
    let config = UnigramTrainerConfig {
        target_multiword_tokens: target_pieces,
        max_piece_length: 12,
        shrinking_factor: 0.75,
        min_frequency: 2,
        em_iterations: 10,
        initial_candidate_multiplier: 4,
        merge_alpha: 0.0,
    };

    println!("  Training unigram model (target_pieces={target_pieces})...");
    let trainer = UnigramTrainer::new(config);
    let model = trainer.train_with_fixed_counts(&corpus, &[], &fixed_counts);
    println!("  Trained: vocab_size={}", model.vocab_size());

    // 8. Segment unknown words with the trained model
    for word in &unknown_words {
        let chars: Vec<char> = word.chars().collect();
        let segments = model.segment(&chars);
        let surface_segments: Vec<String> = segments
            .iter()
            .map(|seq| seq.0.iter().collect::<String>())
            .collect();
        result.insert(word.to_string(), surface_segments);
    }

    println!(
        "  Total segmentations: {} ({} known + {} unigram)",
        result.len(),
        result.len() - unknown_words.len(),
        unknown_words.len()
    );

    result
}
