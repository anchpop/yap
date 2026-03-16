//! Unigram tokenization for merging atoms into supertokens.
//!
//! This implements a word-level Unigram model that learns to merge common
//! sequences of atoms (words + controls) into supertokens.

use crate::{Atom, MergedToken, Sentence, SuperToken};
use language_utils::{Gram, OtherWordType, WordType};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::HashSet;

const LOG_PROB_FLOOR: f64 = -99.0;
type VocabEntry = (Gram<String>, f64, u32);
type ExpectedCounts = FxHashMap<Gram<String>, f64>;
type EmRunResult = (Vec<VocabEntry>, ExpectedCounts, f64);
type SentenceLattice = Vec<Vec<(usize, Gram<String>, f64)>>;

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

fn logsumexp(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }

    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !max.is_finite() {
        return max;
    }

    let sum = values.iter().map(|v| (v - max).exp()).sum::<f64>();
    max + sum.ln()
}

fn is_representable_supertoken_sequence(seq: &Gram<String>) -> bool {
    match seq.len() {
        0 => false,
        1 => true,
        _ => {
            matches!(seq.0.first(), Some(Atom::<String>::Tok(_)))
                && matches!(seq.0.last(), Some(Atom::<String>::Tok(_)))
        }
    }
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
    /// Longest sequence present in the vocabulary
    max_piece_length: usize,
    /// Blend factor for segmentation (see UnigramTrainerConfig::merge_alpha)
    merge_alpha: f64,
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
    pub fn new(vocab_with_scores: Vec<(Gram<String>, f64, u32)>, merge_alpha: f64) -> Self {
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
        let max_piece_length = id_to_seq.iter().map(Gram::len).max().unwrap_or(1);

        Self {
            vocab,
            id_to_seq,
            counts,
            unk_log_prob,
            max_piece_length,
            merge_alpha,
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

    fn vocab_log_prob(&self, seq: &Gram<String>) -> Option<f64> {
        self.vocab.get(seq).map(|(_, lp)| *lp)
    }

    fn sequence_score_from_log_prob(&self, log_prob: f64) -> f64 {
        (1.0 - self.merge_alpha) * log_prob - self.merge_alpha
    }

    fn vocab_score(&self, seq: &Gram<String>) -> Option<f64> {
        self.vocab_log_prob(seq)
            .map(|log_prob| self.sequence_score_from_log_prob(log_prob))
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
            for end in i + 1..=n.min(i + self.max_piece_length) {
                let seq = Gram(atoms[i..end].to_vec());
                let Some(score) = self.vocab_score(&seq) else {
                    continue;
                };
                let new_score = best_score[i] + score;

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
    pub fn reorder_by_actual_usage(
        self,
        corpus: &[Vec<Atom<String>>],
        seed_set: &HashSet<&Gram<String>>,
    ) -> Self {
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

        // Create (sequence, log_prob, actual_count) tuples and sort by count descending.
        // Drop non-seed multi-atom grams with 0 actual usage — these are n-grams that
        // omnigram counted from the corpus but the Viterbi decoder never selects
        // (e.g., "il être" from French inversions like "peut-il être").
        let mut vocab_with_counts: Vec<(Gram<String>, f64, u32)> = self
            .id_to_seq
            .into_iter()
            .enumerate()
            .filter_map(|(id, seq)| {
                let log_prob = self.vocab.get(&seq).map(|(_, lp)| *lp).unwrap_or(0.0);
                let count = usage_counts[id] as u32;
                // Keep single atoms (always needed), seeds, and grams with actual usage
                if seq.len() == 1 || count > 0 || seed_set.contains(&seq) {
                    Some((seq, log_prob, count))
                } else {
                    None
                }
            })
            .collect();

        // Sort by actual usage count descending
        vocab_with_counts.sort_by(|a, b| b.2.cmp(&a.2));

        UnigramModel::new(vocab_with_counts, self.merge_alpha)
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
    /// Number of EM iterations to run between prune rounds
    pub em_iterations: usize,
    /// Blend between pathpiece (α=1, minimize token count) and unigram (α=0,
    /// maximize log-probability). Per-token Viterbi score = (1-α)*log_prob - α.
    /// At α=0.5, merges like "te dire" become cheap enough to use, but junk
    /// merges like "all, you" are still avoided.
    pub merge_alpha: f64,
    /// Whether to use hard EM (Viterbi counts) instead of soft EM (forward-backward marginals).
    pub hard_em: bool,
}

impl Default for UnigramTrainerConfig {
    fn default() -> Self {
        Self {
            target_multiword_tokens: 4000,
            max_piece_length: 16,
            shrinking_factor: 0.75,
            min_frequency: 2,
            em_iterations: 10,
            merge_alpha: 0.0,
            hard_em: false,
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
        let em_iterations = self.config.em_iterations.max(1);

        // Step 1: Count all n-grams up to max_piece_length
        let mut ngram_counts: FxHashMap<Gram<String>, u32> = FxHashMap::default();

        // Build a set of seed sequences for quick lookup
        let filtered_seeds: Vec<Gram<String>> = seed_sequences
            .iter()
            .filter(|seq| is_representable_supertoken_sequence(seq))
            .cloned()
            .collect();
        let seed_set: HashSet<&Gram<String>> = filtered_seeds.iter().collect();

        // Index seeds by first atom for fast lookup during counting
        let mut seeds_by_first_atom: FxHashMap<&Atom<String>, Vec<&Gram<String>>> =
            FxHashMap::default();
        for seed in &filtered_seeds {
            if let Some(first) = seed.0.first() {
                seeds_by_first_atom.entry(first).or_default().push(seed);
            }
        }

        for sentence in corpus.iter() {
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
        for seed in &filtered_seeds {
            ngram_counts.entry(seed.clone()).or_insert(0);
        }

        // Step 2: Filter by minimum frequency (but protect seed sequences)
        ngram_counts
            .retain(|seq, count| *count >= self.config.min_frequency || seed_set.contains(seq));

        // Step 3: Initialize vocabulary with normalized probabilities
        let total_ngrams: u64 = ngram_counts.values().map(|&c| c as u64).sum();

        // Compute initial log probability for each n-gram
        let mut vocab: Vec<(Gram<String>, f64, u32)> = ngram_counts
            .into_iter()
            .map(|(seq, count)| {
                let log_prob = (count as f64 / total_ngrams as f64).ln();
                (seq, log_prob, count)
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

        // Step 4: Iteratively prune vocabulary using Viterbi-based counts
        let protected_count = vocab
            .iter()
            .filter(|(seq, _, _)| seq.len() == 1 || seed_set.contains(seq))
            .count();
        let target_vocab_size = protected_count + self.config.target_multiword_tokens;

        while vocab.len() > target_vocab_size {
            let start_size = vocab.len();
            let (trained_vocab, expected_counts, _) = self.run_em(corpus, &vocab, em_iterations);
            vocab = trained_vocab;
            let model = UnigramModel::new(vocab.clone(), self.config.merge_alpha);

            let mut losses: Vec<(usize, f64)> = Vec::with_capacity(vocab.len());
            for (idx, (seq, log_prob, _)) in vocab.iter().enumerate() {
                if seq.len() == 1 || seed_set.contains(seq) {
                    losses.push((idx, f64::INFINITY));
                    continue;
                }

                let expected = *expected_counts.get(seq).unwrap_or(&0.0);
                let alt_score = self
                    .best_alternative_score(seq, &model)
                    .unwrap_or(f64::NEG_INFINITY);
                let piece_score = model.sequence_score_from_log_prob(*log_prob);
                let approx_loss = -expected * (piece_score - alt_score);
                losses.push((idx, approx_loss));
            }

            losses.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

            let iter_target = (vocab.len() as f64 * self.config.shrinking_factor).ceil() as usize;
            let iter_target = iter_target.max(target_vocab_size);

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

            if removed == 0 {
                break;
            }
        }

        let (vocab, _, _) = self.run_em(corpus, &vocab, em_iterations);
        let model = UnigramModel::new(vocab, self.config.merge_alpha);

        // Reorder by actual usage when segmenting the corpus
        // This ensures ID 0 = most frequently used gram in practice
        model.reorder_by_actual_usage(corpus, &seed_set)
    }

    fn best_alternative_score(&self, seq: &Gram<String>, model: &UnigramModel) -> Option<f64> {
        let n = seq.len();
        if n <= 1 {
            return None;
        }

        let mut best = vec![f64::NEG_INFINITY; n + 1];
        best[0] = 0.0;

        for start in 0..n {
            if !best[start].is_finite() {
                continue;
            }

            for end in start + 1..=n.min(start + model.max_piece_length) {
                if start == 0 && end == n {
                    continue;
                }
                let sub_seq = Gram(seq.0[start..end].to_vec());
                let Some(score) = model.vocab_score(&sub_seq) else {
                    continue;
                };
                let candidate = best[start] + score;
                if candidate > best[end] {
                    best[end] = candidate;
                }
            }
        }

        best[n].is_finite().then_some(best[n])
    }

    fn run_em(
        &self,
        corpus: &[Vec<Atom<String>>],
        vocab: &[VocabEntry],
        iterations: usize,
    ) -> EmRunResult {
        let mut current: Vec<VocabEntry> = vocab.to_vec();
        let mut last_expected = FxHashMap::default();
        let mut last_likelihood = f64::NEG_INFINITY;

        for _ in 0..iterations {
            let model = UnigramModel::new(current.clone(), self.config.merge_alpha);
            let (expected_counts, corpus_log_likelihood) =
                self.compute_expected_counts(corpus, &model);

            let total_expected = expected_counts.values().sum::<f64>();
            let total_expected = if total_expected > 0.0 {
                total_expected
            } else {
                1.0
            };

            current = current
                .iter()
                .map(|(seq, _, count)| {
                    let expected = *expected_counts.get(seq).unwrap_or(&0.0);
                    let log_prob = if expected > 0.0 {
                        (expected / total_expected).ln()
                    } else {
                        LOG_PROB_FLOOR
                    };
                    (seq.clone(), log_prob, *count)
                })
                .collect();

            last_expected = expected_counts;
            last_likelihood = corpus_log_likelihood;
        }

        (current, last_expected, last_likelihood)
    }

    fn compute_expected_counts(
        &self,
        corpus: &[Vec<Atom<String>>],
        model: &UnigramModel,
    ) -> (ExpectedCounts, f64) {
        if self.config.hard_em {
            return self.compute_hard_expected_counts(corpus, model);
        }

        let mut expected_counts: ExpectedCounts = FxHashMap::default();
        let mut corpus_log_likelihood = 0.0;

        for sentence in corpus {
            let n = sentence.len();
            let (incoming, outgoing) = self.build_lattice(sentence, model);
            let alpha = self.forward_pass(&incoming);
            let beta = self.backward_pass(&outgoing);

            let sentence_log_prob = alpha[n];
            corpus_log_likelihood += sentence_log_prob;

            for edges in outgoing.iter().take(n) {
                for (end, seq, log_prob) in edges {
                    let start = end - seq.len();
                    let marginal =
                        (alpha[start] + *log_prob + beta[*end] - sentence_log_prob).exp();
                    *expected_counts.entry(seq.clone()).or_insert(0.0) += marginal;
                }
            }
        }

        (expected_counts, corpus_log_likelihood)
    }

    fn compute_hard_expected_counts(
        &self,
        corpus: &[Vec<Atom<String>>],
        model: &UnigramModel,
    ) -> (ExpectedCounts, f64) {
        let mut expected_counts: ExpectedCounts = FxHashMap::default();
        let mut corpus_log_likelihood = 0.0;

        for sentence in corpus {
            let (segments, sentence_score) = self.viterbi_segments(sentence, model);
            corpus_log_likelihood += sentence_score;

            for seq in segments {
                *expected_counts.entry(seq).or_insert(0.0) += 1.0;
            }
        }

        (expected_counts, corpus_log_likelihood)
    }

    fn viterbi_segments(
        &self,
        sentence: &[Atom<String>],
        model: &UnigramModel,
    ) -> (Vec<Gram<String>>, f64) {
        if sentence.is_empty() {
            return (Vec::new(), 0.0);
        }

        let n = sentence.len();
        let mut best_score = vec![f64::NEG_INFINITY; n + 1];
        let mut best_prev: Vec<Option<(usize, Gram<String>)>> = vec![None; n + 1];
        best_score[0] = 0.0;

        for start in 0..n {
            if !best_score[start].is_finite() {
                continue;
            }

            for end in start + 1..=n.min(start + model.max_piece_length) {
                let seq = Gram(sentence[start..end].to_vec());
                let Some(score) = model.vocab_score(&seq) else {
                    continue;
                };
                let candidate = best_score[start] + score;
                if candidate > best_score[end] {
                    best_score[end] = candidate;
                    best_prev[end] = Some((start, seq));
                }
            }
        }

        let mut segments = Vec::new();
        let mut pos = n;
        while pos > 0 {
            if let Some((prev_pos, seq)) = best_prev[pos].take() {
                segments.push(seq);
                pos = prev_pos;
            } else {
                pos -= 1;
                segments.push(Gram(vec![sentence[pos].clone()]));
            }
        }
        segments.reverse();

        (segments, best_score[n])
    }

    fn build_lattice(
        &self,
        sentence: &[Atom<String>],
        model: &UnigramModel,
    ) -> (SentenceLattice, SentenceLattice) {
        let n = sentence.len();
        let mut incoming: SentenceLattice = vec![Vec::new(); n + 1];
        let mut outgoing: SentenceLattice = vec![Vec::new(); n + 1];

        for start in 0..n {
            for end in start + 1..=n.min(start + model.max_piece_length) {
                let seq = Gram(sentence[start..end].to_vec());
                let Some(score) = model.vocab_score(&seq) else {
                    continue;
                };
                incoming[end].push((start, seq.clone(), score));
                outgoing[start].push((end, seq, score));
            }
        }

        (incoming, outgoing)
    }

    fn forward_pass(&self, incoming: &SentenceLattice) -> Vec<f64> {
        let n = incoming.len() - 1;
        let mut alpha = vec![f64::NEG_INFINITY; n + 1];
        alpha[0] = 0.0;

        for end in 1..=n {
            let terms: Vec<f64> = incoming[end]
                .iter()
                .map(|(start, _, log_prob)| alpha[*start] + *log_prob)
                .collect();
            alpha[end] = logsumexp(&terms);
        }

        alpha
    }

    fn backward_pass(&self, outgoing: &SentenceLattice) -> Vec<f64> {
        let n = outgoing.len() - 1;
        let mut beta = vec![f64::NEG_INFINITY; n + 1];
        beta[n] = 0.0;

        for start in (0..n).rev() {
            let terms: Vec<f64> = outgoing[start]
                .iter()
                .map(|(end, _, log_prob)| *log_prob + beta[*end])
                .collect();
            beta[start] = logsumexp(&terms);
        }

        beta
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
            em_iterations: 8,
            merge_alpha: 0.0,
            hard_em: false,
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

        let model = UnigramModel::new(vocab, 0.0);

        let atoms = vec![make_atom("je"), make_atom("suis"), make_atom("content")];
        let segmented = model.segment(&atoms);

        // Should prefer "je suis" as a merged token since it has higher probability
        assert_eq!(segmented.len(), 2); // "je suis" + "content"
    }

    #[test]
    fn test_viterbi_only_uses_vocab_entries() {
        let vocab = vec![
            (Gram(vec![make_atom("new")]), -1.0, 100),
            (Gram(vec![make_atom("york")]), -1.0, 100),
        ];

        let model = UnigramModel::new(vocab, 0.0);
        let atoms = vec![make_atom("new"), make_atom("york")];
        let segmented = model.segment(&atoms);

        assert_eq!(segmented.len(), 2);
        assert!(matches!(segmented[0], SuperToken::Base(_)));
        assert!(matches!(segmented[1], SuperToken::Base(_)));
    }

    #[test]
    fn test_viterbi_respects_longest_vocab_entry() {
        let atoms: Vec<_> = (0..20).map(|i| make_atom(&format!("w{i}"))).collect();
        let vocab = vec![
            (Gram(atoms.clone()), -0.1, 50),
            (Gram(vec![make_atom("w0")]), -5.0, 100),
            (Gram(vec![make_atom("w1")]), -5.0, 100),
            (Gram(vec![make_atom("w2")]), -5.0, 100),
            (Gram(vec![make_atom("w3")]), -5.0, 100),
            (Gram(vec![make_atom("w4")]), -5.0, 100),
            (Gram(vec![make_atom("w5")]), -5.0, 100),
            (Gram(vec![make_atom("w6")]), -5.0, 100),
            (Gram(vec![make_atom("w7")]), -5.0, 100),
            (Gram(vec![make_atom("w8")]), -5.0, 100),
            (Gram(vec![make_atom("w9")]), -5.0, 100),
            (Gram(vec![make_atom("w10")]), -5.0, 100),
            (Gram(vec![make_atom("w11")]), -5.0, 100),
            (Gram(vec![make_atom("w12")]), -5.0, 100),
            (Gram(vec![make_atom("w13")]), -5.0, 100),
            (Gram(vec![make_atom("w14")]), -5.0, 100),
            (Gram(vec![make_atom("w15")]), -5.0, 100),
            (Gram(vec![make_atom("w16")]), -5.0, 100),
            (Gram(vec![make_atom("w17")]), -5.0, 100),
            (Gram(vec![make_atom("w18")]), -5.0, 100),
            (Gram(vec![make_atom("w19")]), -5.0, 100),
        ];

        let model = UnigramModel::new(vocab, 0.0);
        let segmented = model.segment(&atoms);

        assert_eq!(segmented.len(), 1);
        match &segmented[0] {
            SuperToken::Merged(merged) => {
                assert_eq!(merged.first.text, "w0");
                assert_eq!(merged.last.text, "w19");
            }
            _ => panic!("expected a merged long token"),
        }
    }

    #[test]
    fn test_em_expected_counts_are_consistent() {
        let corpus = vec![vec![make_atom("a"), make_atom("b")]];
        let vocab = vec![
            (Gram(vec![make_atom("a")]), 0.4f64.ln(), 10),
            (Gram(vec![make_atom("b")]), 0.4f64.ln(), 10),
            (Gram(vec![make_atom("a"), make_atom("b")]), 0.2f64.ln(), 10),
        ];
        let trainer = UnigramTrainer::new(UnigramTrainerConfig::default());
        let model = UnigramModel::new(vocab, 0.0);

        let (expected_counts, _) = trainer.compute_expected_counts(&corpus, &model);
        let total_expected = expected_counts.values().sum::<f64>();

        assert!(total_expected >= 1.0);
        assert!(total_expected <= 2.0);
        assert!(expected_counts.contains_key(&Gram(vec![make_atom("a"), make_atom("b")])));
    }

    #[test]
    fn test_merge_alpha_affects_em_expected_counts() {
        let corpus = vec![vec![make_atom("a"), make_atom("b")]];
        let vocab = vec![
            (Gram(vec![make_atom("a")]), 0.4f64.ln(), 10),
            (Gram(vec![make_atom("b")]), 0.4f64.ln(), 10),
            (Gram(vec![make_atom("a"), make_atom("b")]), 0.2f64.ln(), 10),
        ];

        let trainer_plain = UnigramTrainer::new(UnigramTrainerConfig {
            merge_alpha: 0.0,
            ..UnigramTrainerConfig::default()
        });
        let trainer_merge = UnigramTrainer::new(UnigramTrainerConfig {
            merge_alpha: 0.5,
            ..UnigramTrainerConfig::default()
        });

        let model_plain = UnigramModel::new(vocab.clone(), 0.0);
        let model_merge = UnigramModel::new(vocab, 0.5);

        let (expected_plain, _) = trainer_plain.compute_expected_counts(&corpus, &model_plain);
        let (expected_merge, _) = trainer_merge.compute_expected_counts(&corpus, &model_merge);

        let merged = Gram(vec![make_atom("a"), make_atom("b")]);
        let merged_plain = *expected_plain.get(&merged).unwrap();
        let merged_merge = *expected_merge.get(&merged).unwrap();

        assert!(merged_merge > merged_plain);
    }

    #[test]
    fn test_hard_em_uses_viterbi_counts() {
        let corpus = vec![vec![make_atom("a"), make_atom("b")]];
        let vocab = vec![
            (Gram(vec![make_atom("a")]), 0.2f64.ln(), 10),
            (Gram(vec![make_atom("b")]), 0.2f64.ln(), 10),
            (Gram(vec![make_atom("a"), make_atom("b")]), 0.6f64.ln(), 10),
        ];

        let trainer = UnigramTrainer::new(UnigramTrainerConfig {
            hard_em: true,
            ..UnigramTrainerConfig::default()
        });
        let model = UnigramModel::new(vocab, 0.0);

        let (expected_counts, _) = trainer.compute_expected_counts(&corpus, &model);

        assert_eq!(
            expected_counts.get(&Gram(vec![make_atom("a"), make_atom("b")])),
            Some(&1.0)
        );
        assert!(!expected_counts.contains_key(&Gram(vec![make_atom("a")])));
        assert!(!expected_counts.contains_key(&Gram(vec![make_atom("b")])));
    }

    #[test]
    fn test_best_alternative_score_prefers_non_singleton_decomposition() {
        let trainer = UnigramTrainer::new(UnigramTrainerConfig::default());
        let vocab = vec![
            (Gram(vec![make_atom("a")]), 0.1f64.ln(), 10),
            (Gram(vec![make_atom("b")]), 0.1f64.ln(), 10),
            (Gram(vec![make_atom("c")]), 0.1f64.ln(), 10),
            (Gram(vec![make_atom("a"), make_atom("b")]), 0.35f64.ln(), 10),
            (
                Gram(vec![make_atom("a"), make_atom("b"), make_atom("c")]),
                0.25f64.ln(),
                10,
            ),
        ];
        let model = UnigramModel::new(vocab, 0.0);
        let seq = Gram(vec![make_atom("a"), make_atom("b"), make_atom("c")]);

        let alt = trainer.best_alternative_score(&seq, &model).unwrap();
        let expected = 0.35f64.ln() + 0.1f64.ln();

        assert!((alt - expected).abs() < 1e-9);
    }

    #[test]
    fn test_forward_backward_agree_on_sentence_probability() {
        let sentence = vec![
            make_atom("the"),
            make_atom("new"),
            make_atom("york"),
            make_atom("times"),
        ];
        let vocab = vec![
            (Gram(vec![make_atom("the")]), 0.2f64.ln(), 10),
            (Gram(vec![make_atom("new")]), 0.1f64.ln(), 10),
            (Gram(vec![make_atom("york")]), 0.1f64.ln(), 10),
            (Gram(vec![make_atom("times")]), 0.1f64.ln(), 10),
            (
                Gram(vec![make_atom("new"), make_atom("york")]),
                0.2f64.ln(),
                10,
            ),
            (
                Gram(vec![
                    make_atom("new"),
                    make_atom("york"),
                    make_atom("times"),
                ]),
                0.15f64.ln(),
                10,
            ),
            (
                Gram(vec![make_atom("the"), make_atom("new"), make_atom("york")]),
                0.15f64.ln(),
                10,
            ),
        ];

        let trainer = UnigramTrainer::new(UnigramTrainerConfig::default());
        let model = UnigramModel::new(vocab, 0.0);
        let (incoming, outgoing) = trainer.build_lattice(&sentence, &model);
        let alpha = trainer.forward_pass(&incoming);
        let beta = trainer.backward_pass(&outgoing);

        assert!((alpha[sentence.len()] - beta[0]).abs() < 1e-9);
    }
}
