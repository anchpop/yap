//! Unigram tokenization for merging atoms into supertokens.
//!
//! This implements a word-level Unigram model that learns to merge common
//! sequences of atoms (words + controls) into supertokens.

use crate::{Atom, MergedToken, Sentence, SuperToken};
use language_utils::{Gram, OtherWordType, WordType};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Check if an atom is a content word (dictionary word, not proper noun/punct/unknown)
/// Used for supertoken boundaries - only real words can start/end a merged token
fn is_content_word(atom: &Atom<String>) -> bool {
    match atom {
        Atom::<String>::Tok(word) => {
            // Only Heteronym (dictionary words) are content words
            // Excludes: proper nouns (Tom, Mary), punctuation, spaces, unknowns
            matches!(&word.word_type, WordType::Heteronym(_))
        }
        Atom::<String>::Control(_) => false,
    }
}

/// Check if an atom is a proper noun (Tom, Mary, Paris, etc.)
/// Proper nouns should never appear in supertokens at all
fn is_proper_noun(atom: &Atom<String>) -> bool {
    matches!(
        atom,
        Atom::<String>::Tok(word) if matches!(&word.word_type, WordType::Other(o) if o.other_tag == OtherWordType::Propn)
    )
}

/// Compute max supersequence counts for substring suppression.
/// For each sequence, finds the max count of any longer sequence containing it.
/// This is O(V × L²) where L is max_piece_length.
fn compute_max_superseq_counts(
    counts: &FxHashMap<Gram<String>, u32>,
) -> FxHashMap<Gram<String>, u32> {
    let mut max_superseq: FxHashMap<Gram<String>, u32> = FxHashMap::default();

    // Process longer sequences first
    let mut by_length: Vec<_> = counts.iter().collect();
    by_length.sort_by_key(|(seq, _)| std::cmp::Reverse(seq.len()));

    for (seq, &count) in by_length {
        if seq.len() <= 1 {
            continue; // Single atoms don't suppress anything
        }
        // For each contiguous subsequence of this sequence,
        // update its max_superseq if our count is higher
        for start in 0..seq.len() {
            for end in start + 1..seq.len() {
                // strictly shorter (end < seq.len())
                let subseq = Gram(seq.0[start..end].to_vec());
                max_superseq
                    .entry(subseq)
                    .and_modify(|c| *c = (*c).max(count))
                    .or_insert(count);
            }
        }
    }

    max_superseq
}

/// Convert a Gram to a SuperToken
pub fn gram_to_supertoken(gram: &Gram<String>) -> Option<SuperToken> {
    match gram.len() {
        0 => None,
        1 => Some(SuperToken::Base(gram.0[0].clone())),
        _ => {
            // Find first and last word tokens
            let first_word_idx = gram
                .0
                .iter()
                .position(|a| matches!(a, Atom::<String>::Tok(_)))?;
            let last_word_idx = gram
                .0
                .iter()
                .rposition(|a| matches!(a, Atom::<String>::Tok(_)))?;

            if first_word_idx == last_word_idx {
                // Only one word, return as base
                return Some(SuperToken::Base(gram.0[first_word_idx].clone()));
            }

            let first = match &gram.0[first_word_idx] {
                Atom::<String>::Tok(w) => w.clone(),
                _ => unreachable!(),
            };
            let last = match &gram.0[last_word_idx] {
                Atom::<String>::Tok(w) => w.clone(),
                _ => unreachable!(),
            };

            // Middle is everything between first and last word
            let middle: Vec<Atom<String>> = gram.0[first_word_idx + 1..last_word_idx].to_vec();

            Some(SuperToken::Merged(MergedToken {
                first,
                middle,
                last,
            }))
        }
    }
}

/// A trained Unigram model for supertoken segmentation
#[derive(Debug, Clone)]
pub struct UnigramModel {
    /// Vocabulary: atom sequence -> (id, log probability)
    vocab: FxHashMap<Gram<String>, (u32, f64)>,
    /// Reverse lookup: id -> atom sequence (for serialization/inspection)
    id_to_seq: Vec<Gram<String>>,
    /// Counts for each sequence (for reporting)
    counts: Vec<u32>,
    /// Unknown token log probability (for single atoms not in vocab)
    unk_log_prob: f64,
}

impl UnigramModel {
    /// Get vocabulary items in order
    pub fn get_vocab(&self) -> &[Gram<String>] {
        &self.id_to_seq
    }

    /// Get vocabulary items with their counts in ID order (index = token ID)
    pub fn get_vocab_in_id_order(&self) -> impl Iterator<Item = (&Gram<String>, u32)> {
        self.id_to_seq.iter().zip(self.counts.iter().copied())
    }

    /// Get vocabulary items with their counts, sorted by count descending
    pub fn get_vocab_with_counts(&self) -> Vec<(&Gram<String>, u32)> {
        let mut items: Vec<_> = self
            .id_to_seq
            .iter()
            .zip(self.counts.iter())
            .map(|(seq, &count)| (seq, count))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        items
    }

    /// Create a new model from vocabulary with scores and counts
    pub fn new(vocab_with_scores: Vec<(Gram<String>, f64, u32)>) -> Self {
        let mut vocab = FxHashMap::default();
        let mut id_to_seq = Vec::with_capacity(vocab_with_scores.len());
        let mut counts = Vec::with_capacity(vocab_with_scores.len());

        for (id, (seq, log_prob, count)) in vocab_with_scores.into_iter().enumerate() {
            vocab.insert(seq.clone(), (id as u32, log_prob));
            id_to_seq.push(seq);
            counts.push(count);
        }

        // UNK log prob is lower than any vocab item
        let min_log_prob = vocab
            .values()
            .map(|(_, lp)| *lp)
            .fold(f64::INFINITY, f64::min);
        let unk_log_prob = min_log_prob - 10.0;

        Self {
            vocab,
            id_to_seq,
            counts,
            unk_log_prob,
        }
    }

    /// Get the log probability of an atom sequence
    pub fn get_log_prob(&self, seq: &Gram<String>) -> f64 {
        self.vocab.get(seq).map(|(_, lp)| *lp).unwrap_or_else(|| {
            // Unknown sequences: penalize by length AND add extra penalty per merge
            // This ensures that merging unknown atoms is WORSE than keeping them separate
            // Without the -1.0 penalty, 9 unknowns = 1 unknown of length 9 (same score!)
            let length = seq.len() as f64;
            self.unk_log_prob * length - 1.0 * (length - 1.0)
        })
    }

    /// Segment a sequence of atoms into supertokens using Viterbi algorithm
    pub fn segment(&self, atoms: &[Atom<String>]) -> Vec<SuperToken> {
        if atoms.is_empty() {
            return Vec::new();
        }

        let n = atoms.len();

        // best_score[i] = best log probability to reach position i
        let mut best_score = vec![f64::NEG_INFINITY; n + 1];
        // best_prev[i] = (previous position, sequence that got us here)
        let mut best_prev: Vec<Option<(usize, Gram<String>)>> = vec![None; n + 1];

        best_score[0] = 0.0;

        for i in 0..n {
            if best_score[i] == f64::NEG_INFINITY {
                continue;
            }

            // Try all possible next sequences starting at position i
            for end in i + 1..=n.min(i + 16) {
                // Max sequence length 16
                let seq = Gram(atoms[i..end].to_vec());
                let log_prob = self.get_log_prob(&seq);
                let new_score = best_score[i] + log_prob;

                if new_score > best_score[end] {
                    best_score[end] = new_score;
                    best_prev[end] = Some((i, seq));
                }
            }
        }

        // Backtrack to find the best segmentation
        let mut result = Vec::new();
        let mut pos = n;

        while pos > 0 {
            if let Some((prev_pos, seq)) = best_prev[pos].take() {
                if let Some(st) = gram_to_supertoken(&seq) {
                    result.push(st);
                }
                pos = prev_pos;
            } else {
                // Fallback: emit single atom
                pos -= 1;
                result.push(SuperToken::Base(atoms[pos].clone()));
            }
        }

        result.reverse();
        result
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }

    /// Get the token ID for a supertoken, if it exists in the vocabulary
    pub fn get_token_id(&self, st: &SuperToken) -> Option<u32> {
        let seq = match st {
            SuperToken::Base(atom) => Gram(vec![atom.clone()]),
            SuperToken::Merged(merged) => {
                let mut atoms = vec![Atom::<String>::Tok(merged.first.clone())];
                atoms.extend(merged.middle.iter().cloned());
                atoms.push(Atom::<String>::Tok(merged.last.clone()));
                Gram(atoms)
            }
        };
        self.vocab.get(&seq).map(|(id, _)| *id)
    }

    /// Compute actual usage counts by segmenting the corpus and counting gram occurrences.
    /// Returns a new model with IDs reassigned so that ID 0 = most frequently used gram.
    pub fn reorder_by_actual_usage(self, corpus: &[Vec<Atom<String>>]) -> Self {
        // Count actual usage of each gram when segmenting
        let mut usage_counts: Vec<u64> = vec![0; self.id_to_seq.len()];

        for sentence in corpus {
            let supertokens = self.segment(sentence);
            for st in &supertokens {
                if let Some(id) = self.get_token_id(st) {
                    usage_counts[id as usize] += 1;
                }
            }
        }

        // Create (sequence, log_prob, actual_count) tuples and sort by count descending
        let mut vocab_with_counts: Vec<(Gram<String>, f64, u32)> = self
            .id_to_seq
            .into_iter()
            .enumerate()
            .map(|(id, seq)| {
                let log_prob = self.vocab.get(&seq).map(|(_, lp)| *lp).unwrap_or(0.0);
                let count = usage_counts[id] as u32;
                (seq, log_prob, count)
            })
            .collect();

        // Sort by actual usage count descending
        vocab_with_counts.sort_by(|a, b| b.2.cmp(&a.2));

        UnigramModel::new(vocab_with_counts)
    }
}

/// Configuration for Unigram training
#[derive(Debug, Clone)]
pub struct UnigramTrainerConfig {
    /// Target number of multi-word supertokens to learn
    /// (actual vocab size = num_single_atoms + this value)
    pub target_multiword_tokens: usize,
    /// Maximum length of atom sequences to consider
    pub max_piece_length: usize,
    /// Shrinking factor for vocabulary pruning
    pub shrinking_factor: f64,
    /// Minimum frequency for a sequence to be considered
    pub min_frequency: u32,
}

impl Default for UnigramTrainerConfig {
    fn default() -> Self {
        Self {
            target_multiword_tokens: 4000,
            max_piece_length: 16,
            shrinking_factor: 0.75,
            min_frequency: 2,
        }
    }
}

/// Trainer for building a Unigram model from a corpus of atom sequences
pub struct UnigramTrainer {
    config: UnigramTrainerConfig,
}

impl UnigramTrainer {
    pub fn new(config: UnigramTrainerConfig) -> Self {
        Self { config }
    }

    /// Train a Unigram model from a corpus of atom sequences.
    /// `seed_sequences` are atom sequences that will be injected into the vocabulary
    /// with their actual corpus PMI, and protected from pruning.
    pub fn train(
        &self,
        corpus: &[Vec<Atom<String>>],
        seed_sequences: &[Gram<String>],
    ) -> UnigramModel {
        // Step 1: Count all n-grams up to max_piece_length
        let mut ngram_counts: FxHashMap<Gram<String>, u32> = FxHashMap::default();
        let mut unigram_counts: FxHashMap<Atom<String>, u64> = FxHashMap::default();
        let mut total_unigrams: u64 = 0;

        // Build a set of seed sequences for quick lookup
        let seed_set: HashSet<&Gram<String>> = seed_sequences.iter().collect();

        // Index seeds by first atom for fast lookup during counting
        let mut seeds_by_first_atom: FxHashMap<&Atom<String>, Vec<&Gram<String>>> =
            FxHashMap::default();
        for seed in seed_sequences {
            if let Some(first) = seed.0.first() {
                seeds_by_first_atom.entry(first).or_default().push(seed);
            }
        }

        for sentence in corpus.iter() {
            // Count unigrams for PMI calculation
            for atom in sentence {
                *unigram_counts.entry(atom.clone()).or_insert(0) += 1;
                total_unigrams += 1;
            }

            for start in 0..sentence.len() {
                for end in start + 1..=sentence.len().min(start + self.config.max_piece_length) {
                    let slice = &sentence[start..end];

                    // For multi-atom sequences:
                    // 1. Boundaries must be content words (no punct/proper nouns at start/end)
                    // 2. No proper nouns anywhere (Tom, Mary, etc. shouldn't be in supertokens)
                    if slice.len() >= 2 {
                        let first_ok = is_content_word(&slice[0]);
                        let last_ok = is_content_word(&slice[slice.len() - 1]);
                        if !first_ok || !last_ok {
                            continue;
                        }
                        if slice.iter().any(is_proper_noun) {
                            continue;
                        }
                    }

                    let seq = Gram(slice.to_vec());
                    if !seed_set.contains(&seq) {
                        *ngram_counts.entry(seq).or_insert(0) += 1;
                    }
                }

                // Count seed sequences separately (bypasses boundary filters
                // and handles sequences longer than max_piece_length).
                // Uses first-atom index to avoid iterating all seeds at every position.
                if let Some(candidates) = seeds_by_first_atom.get(&sentence[start]) {
                    for seed in candidates {
                        let end = start + seed.len();
                        if end <= sentence.len() && sentence[start..end] == seed.0[..] {
                            *ngram_counts.entry((*seed).clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        // Ensure all seed sequences have an entry (even if they never appear in the corpus)
        for seed in seed_sequences {
            ngram_counts.entry(seed.clone()).or_insert(0);
        }

        // Step 2: Filter by minimum frequency (but protect seed sequences)
        ngram_counts
            .retain(|seq, count| *count >= self.config.min_frequency || seed_set.contains(seq));

        // Step 3: Initialize vocabulary with PMI scores
        let total_ngrams: u64 = ngram_counts.values().map(|&c| c as u64).sum();

        // Keep counts and PMI for loss estimation later
        let counts_for_loss: FxHashMap<Gram<String>, u32> = ngram_counts.clone();

        // Compute PMI for each n-gram: log(P(ngram) / product(P(atom_i)))
        // PMI measures how much more likely the sequence is than if atoms were independent
        let mut vocab: Vec<(Gram<String>, f64, u32)> = ngram_counts
            .into_iter()
            .map(|(seq, count)| {
                let log_prob_ngram = (count as f64 / total_ngrams as f64).ln();

                if seq.len() == 1 {
                    // Single atoms just use log probability
                    (seq, log_prob_ngram, count)
                } else {
                    // Multi-word: compute PMI
                    // PMI = log(P(ngram)) - sum(log(P(atom_i)))
                    let log_prob_independent: f64 = seq
                        .0
                        .iter()
                        .map(|atom| {
                            let atom_count = unigram_counts.get(atom).copied().unwrap_or(1);
                            (atom_count as f64 / total_unigrams as f64).ln()
                        })
                        .sum();

                    let pmi = log_prob_ngram - log_prob_independent;
                    // Use PMI as score (higher = more associated)
                    (seq, pmi, count)
                }
            })
            .collect();

        // Ensure all single atoms are in vocabulary (required for fallback)
        let mut single_atoms: FxHashMap<Gram<String>, f64> = FxHashMap::default();
        for sentence in corpus {
            for atom in sentence {
                let seq = Gram(vec![atom.clone()]);
                single_atoms.entry(seq).or_insert(-10.0);
            }
        }
        for (seq, score) in single_atoms {
            if !vocab.iter().any(|(s, _, _)| s == &seq) {
                vocab.push((seq, score, 1)); // count=1 for rare single atoms
            }
        }

        // Step 4: Compute substring suppression (independence ratios)
        // For each sequence, find max count of any supersequence containing it
        let max_superseq_counts = compute_max_superseq_counts(&counts_for_loss);

        // Step 5: Iteratively prune vocabulary
        // Seed sequences don't count towards the multiword quota
        let protected_count = vocab
            .iter()
            .filter(|(seq, _, _)| seq.len() == 1 || seed_set.contains(seq))
            .count();
        let target_vocab_size = protected_count + self.config.target_multiword_tokens;

        while vocab.len() > target_vocab_size {
            let start_size = vocab.len();

            // Calculate loss for each item
            // Single atoms and seed sequences get INFINITY (never removed)
            // Multi-word: use PMI * count * independence as value (higher = keep, lower = remove)
            // Independence ratio = (count - max_superseq_count) / count
            // This suppresses phrases that mostly appear as part of longer phrases
            let mut losses: Vec<(usize, f64)> = Vec::with_capacity(vocab.len());
            for (idx, (seq, pmi, _)) in vocab.iter().enumerate() {
                if seq.len() == 1 || seed_set.contains(seq) {
                    losses.push((idx, f64::INFINITY));
                    continue;
                }
                // Value = PMI * count * independence
                // - PMI: how associated the words are (higher = more meaningful phrase)
                // - count: how frequent (higher = more useful)
                // - independence: what fraction appears independently vs as substring
                //   (higher = more often stands alone, not just part of longer phrase)
                let count = counts_for_loss.get(seq).copied().unwrap_or(0);
                let max_super = max_superseq_counts.get(seq).copied().unwrap_or(0);
                let independence = if max_super == 0 {
                    1.0 // No supersequence found, fully independent
                } else {
                    // Fraction that appears independently (not as part of longer phrase)
                    (count.saturating_sub(max_super) as f64) / (count as f64)
                };
                let value = pmi * (count as f64) * independence;
                losses.push((idx, value));
            }

            // Sort by loss (ascending) and remove lowest-impact items
            losses.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

            let iter_target = (vocab.len() as f64 * self.config.shrinking_factor).ceil() as usize;
            let iter_target = iter_target.max(target_vocab_size);

            // Only remove items with finite loss (not INFINITY)
            // This ensures single atoms are never removed
            let to_remove: std::collections::HashSet<usize> = losses
                .iter()
                .filter(|(_, loss)| *loss != f64::INFINITY)
                .take(vocab.len() - iter_target)
                .map(|(idx, _)| *idx)
                .collect();

            vocab = vocab
                .into_iter()
                .enumerate()
                .filter(|(idx, _)| !to_remove.contains(idx))
                .map(|(_, item)| item)
                .collect();

            let removed = start_size - vocab.len();

            // If we couldn't remove anything, we've hit the floor (all remaining items are protected)
            if removed == 0 {
                break;
            }
        }

        // Create the initial model
        let model = UnigramModel::new(vocab);

        // Reorder by actual usage when segmenting the corpus
        // This ensures ID 0 = most frequently used gram in practice
        model.reorder_by_actual_usage(corpus)
    }
}

/// Apply a trained model to convert sentences to supertoken form
pub fn apply_model(model: &UnigramModel, sentences: &[Sentence]) -> Vec<Sentence> {
    sentences
        .iter()
        .map(|sent| {
            // Extract atoms from the sentence
            let atoms: Vec<Atom<String>> = sent
                .tokens
                .iter()
                .flat_map(|st| match st {
                    SuperToken::Base(atom) => vec![atom.clone()],
                    SuperToken::Merged(merged) => {
                        let mut atoms = vec![Atom::<String>::Tok(merged.first.clone())];
                        atoms.extend(merged.middle.iter().cloned());
                        atoms.push(Atom::<String>::Tok(merged.last.clone()));
                        atoms
                    }
                })
                .collect();

            // Segment with the model
            let new_tokens = model.segment(&atoms);

            Sentence {
                tokens: new_tokens,
                capitalize_first_letter: sent.capitalize_first_letter,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::{Heteronym, PartOfSpeech, Word, WordType};

    fn make_word(text: &str) -> Word<String> {
        Word {
            text: text.to_string(),
            word_type: WordType::Heteronym(Heteronym {
                word: text.to_string(),
                lemma: text.to_string(),
                pos: PartOfSpeech::Noun,
            }),
        }
    }

    fn make_atom(text: &str) -> Atom<String> {
        Atom::<String>::Tok(make_word(text))
    }

    #[test]
    fn test_atom_sequence_to_supertoken_single() {
        let seq = Gram(vec![make_atom("hello")]);
        let st = gram_to_supertoken(&seq).unwrap();
        assert!(matches!(st, SuperToken::Base(Atom::<String>::Tok(_))));
    }

    #[test]
    fn test_atom_sequence_to_supertoken_merged() {
        let seq = Gram(vec![make_atom("qu'"), make_atom("est"), make_atom("ce")]);
        let st = gram_to_supertoken(&seq).unwrap();
        match st {
            SuperToken::Merged(merged) => {
                assert_eq!(merged.first.text, "qu'");
                assert_eq!(merged.last.text, "ce");
                assert_eq!(merged.middle.len(), 1);
            }
            _ => panic!("Expected merged token"),
        }
    }

    #[test]
    fn test_unigram_training_basic() {
        // Create a simple corpus with repeated patterns
        let corpus: Vec<Vec<Atom<String>>> = vec![
            vec![make_atom("je"), make_atom("suis")],
            vec![make_atom("je"), make_atom("suis"), make_atom("content")],
            vec![make_atom("tu"), make_atom("es")],
            vec![make_atom("je"), make_atom("suis"), make_atom("là")],
            vec![make_atom("il"), make_atom("est")],
            vec![make_atom("je"), make_atom("suis"), make_atom("ici")],
        ];

        let config = UnigramTrainerConfig {
            target_multiword_tokens: 5,
            max_piece_length: 4,
            shrinking_factor: 0.8,
            min_frequency: 2,
        };

        let trainer = UnigramTrainer::new(config);
        let model = trainer.train(&corpus, &[]);

        // The model should have learned that "je suis" is common
        let atoms = vec![make_atom("je"), make_atom("suis"), make_atom("content")];
        let segmented = model.segment(&atoms);

        // Should have fewer segments than atoms due to merging
        assert!(segmented.len() <= atoms.len());
    }

    #[test]
    fn test_viterbi_segmentation() {
        // Create a model with known vocabulary (seq, score, count)
        let vocab = vec![
            (Gram(vec![make_atom("je")]), -1.0, 100),
            (Gram(vec![make_atom("suis")]), -1.0, 100),
            (Gram(vec![make_atom("content")]), -1.0, 50),
            (Gram(vec![make_atom("je"), make_atom("suis")]), -0.5, 80), // More likely as pair
        ];

        let model = UnigramModel::new(vocab);

        let atoms = vec![make_atom("je"), make_atom("suis"), make_atom("content")];
        let segmented = model.segment(&atoms);

        // Should prefer "je suis" as a merged token since it has higher probability
        assert_eq!(segmented.len(), 2); // "je suis" + "content"
    }
}
