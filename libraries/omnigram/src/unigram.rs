//! Generic unigram tokenization for merging tokens into supertokens.
//!
//! This implements a word-level Unigram model that learns to merge common
//! sequences of tokens into multi-token units. The algorithm is generic over
//! the token type via the [`UnigramToken`] trait.

use rustc_hash::FxHashMap;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

const LOG_PROB_FLOOR: f64 = -99.0;
type VocabEntry<T> = (Seq<T>, f64, u32);
type ExpectedCounts<T> = FxHashMap<Seq<T>, f64>;
type EmRunResult<T> = (Vec<VocabEntry<T>>, ExpectedCounts<T>, f64);
type SentenceLattice = Vec<Vec<(usize, u32, f64)>>;

/// Trait for tokens that can be used with the unigram model.
///
/// Implementations control which token sequences are valid candidates for merging:
/// - `can_be_sequence_boundary`: structural constraint on what can appear at the
///   start/end of a multi-token sequence (e.g., control tokens cannot)
/// - `is_content`: semantic constraint on what should start/end a *learned* sequence
///   (e.g., only dictionary words, not punctuation)
/// - `is_excluded_from_sequences`: tokens that must never appear anywhere in a
///   multi-token sequence (e.g., proper nouns)
pub trait UnigramToken: Clone + Eq + Hash + Debug {
    /// Whether this token can structurally appear at the boundary (first/last position)
    /// of a multi-token sequence.
    fn can_be_sequence_boundary(&self) -> bool;

    /// Whether this token is a "content" token suitable for starting/ending learned
    /// multi-token sequences. This is a semantic constraint, typically stricter than
    /// `can_be_sequence_boundary`.
    fn is_content(&self) -> bool;

    /// Whether this token must never appear anywhere in a multi-token sequence.
    fn is_excluded_from_sequences(&self) -> bool;
}

/// A sequence of tokens used in the unigram model.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Seq<T>(pub Vec<T>);

impl<T> Seq<T> {
    pub fn new(tokens: Vec<T>) -> Self {
        Seq(tokens)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }

    pub fn first(&self) -> Option<&T> {
        self.0.first()
    }
}

impl<T> Borrow<[T]> for Seq<T> {
    fn borrow(&self) -> &[T] {
        &self.0
    }
}

impl<T: Hash> Seq<T> {
    /// Returns a disambiguation key for this sequence, used to maintain consistent
    /// ordering of sequences with the same frequency.
    pub fn disambiguation_key(&self) -> u32 {
        use std::hash::{DefaultHasher, Hasher};
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish() as u32
    }
}

fn logsumexp_by<T>(values: &[T], value: impl Fn(&T) -> f64) -> f64 {
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }

    let max = values.iter().map(&value).fold(f64::NEG_INFINITY, f64::max);
    if !max.is_finite() {
        return max;
    }

    let sum = values
        .iter()
        .map(|item| (value(item) - max).exp())
        .sum::<f64>();
    max + sum.ln()
}

fn is_representable_supertoken_sequence<T: UnigramToken>(seq: &Seq<T>) -> bool {
    match seq.len() {
        0 => false,
        1 => true,
        _ => {
            seq.0.first().is_some_and(|t| t.can_be_sequence_boundary())
                && seq.0.last().is_some_and(|t| t.can_be_sequence_boundary())
        }
    }
}

/// A trained Unigram model for segmentation
#[derive(Debug, Clone)]
pub struct UnigramModel<T> {
    /// Vocabulary: token sequence -> (id, log probability)
    vocab: FxHashMap<Seq<T>, (u32, f64)>,
    /// Reverse lookup: id -> token sequence (for serialization/inspection)
    id_to_seq: Vec<Seq<T>>,
    /// Counts for each sequence (for reporting)
    counts: Vec<u32>,
    /// Unknown token log probability (for single tokens not in vocab)
    unk_log_prob: f64,
    /// Longest sequence present in the vocabulary
    max_piece_length: usize,
    /// Blend factor for segmentation (see UnigramTrainerConfig::merge_alpha)
    merge_alpha: f64,
}

impl<T: UnigramToken> UnigramModel<T> {
    /// Get vocabulary items in order
    pub fn get_vocab(&self) -> &[Seq<T>] {
        &self.id_to_seq
    }

    /// Get vocabulary items with their counts in ID order (index = token ID)
    pub fn get_vocab_in_id_order(&self) -> impl Iterator<Item = (&Seq<T>, u32)> {
        self.id_to_seq.iter().zip(self.counts.iter().copied())
    }

    /// Get vocabulary items with their counts, sorted by count descending
    pub fn get_vocab_with_counts(&self) -> Vec<(&Seq<T>, u32)> {
        let mut items: Vec<_> = self
            .id_to_seq
            .iter()
            .zip(self.counts.iter())
            .map(|(seq, &count)| (seq, count))
            .collect();
        items.sort_by_key(|b| std::cmp::Reverse(b.1));
        items
    }

    /// Create a new model from vocabulary with scores and counts
    pub fn new(vocab_with_scores: Vec<(Seq<T>, f64, u32)>, merge_alpha: f64) -> Self {
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
        let max_piece_length = id_to_seq.iter().map(Seq::len).max().unwrap_or(1);

        Self {
            vocab,
            id_to_seq,
            counts,
            unk_log_prob,
            max_piece_length,
            merge_alpha,
        }
    }

    /// Get the log probability of a token sequence
    pub fn get_log_prob(&self, seq: &Seq<T>) -> f64 {
        self.vocab.get(seq).map(|(_, lp)| *lp).unwrap_or_else(|| {
            // Unknown sequences: penalize by length AND add extra penalty per merge
            // This ensures that merging unknown tokens is WORSE than keeping them separate
            // Without the -1.0 penalty, 9 unknowns = 1 unknown of length 9 (same score!)
            let length = seq.len() as f64;
            self.unk_log_prob * length - 1.0 * (length - 1.0)
        })
    }

    fn vocab_log_prob(&self, seq: &Seq<T>) -> Option<f64> {
        self.vocab.get(seq).map(|(_, lp)| *lp)
    }

    fn sequence_score_from_log_prob(&self, log_prob: f64) -> f64 {
        (1.0 - self.merge_alpha) * log_prob - self.merge_alpha
    }

    fn vocab_score(&self, seq: &Seq<T>) -> Option<f64> {
        self.vocab_log_prob(seq)
            .map(|log_prob| self.sequence_score_from_log_prob(log_prob))
    }

    fn vocab_entry(&self, tokens: &[T]) -> Option<(u32, f64)> {
        self.vocab
            .get(tokens)
            .map(|&(id, log_prob)| (id, self.sequence_score_from_log_prob(log_prob)))
    }

    /// Segment a sequence of tokens using Viterbi algorithm.
    /// Returns a list of sequences, where each sequence is a segment.
    pub fn segment(&self, tokens: &[T]) -> Vec<Seq<T>> {
        if tokens.is_empty() {
            return Vec::new();
        }

        let n = tokens.len();

        // best_score[i] = best log probability to reach position i
        let mut best_score = vec![f64::NEG_INFINITY; n + 1];
        // best_prev[i] = previous position on the best path to i
        let mut best_prev: Vec<Option<usize>> = vec![None; n + 1];

        best_score[0] = 0.0;

        for i in 0..n {
            if best_score[i] == f64::NEG_INFINITY {
                continue;
            }

            // Try all possible next sequences starting at position i
            for end in i + 1..=n.min(i + self.max_piece_length) {
                let Some((_, score)) = self.vocab_entry(&tokens[i..end]) else {
                    continue;
                };
                let new_score = best_score[i] + score;

                if new_score > best_score[end] {
                    best_score[end] = new_score;
                    best_prev[end] = Some(i);
                }
            }
        }

        // Backtrack to find the best segmentation
        let mut result = Vec::new();
        let mut pos = n;

        assert!(
            best_score[n].is_finite(),
            "UnigramModel::segment found no path through the lattice.\n\
             sentence_len={n}\n\
             max_piece_length={}\n\
             sentence_tokens={:#?}",
            self.max_piece_length,
            tokens
        );

        while pos > 0 {
            let prev_pos = best_prev[pos].take().unwrap_or_else(|| {
                panic!(
                    "UnigramModel::segment lost a backpointer while reconstructing the best path.\n\
                     position={pos}\n\
                     sentence_len={n}\n\
                     max_piece_length={}\n\
                     sentence_tokens={:#?}\n\
                     best_score={:#?}\n\
                     best_prev={:#?}",
                    self.max_piece_length, tokens, best_score, best_prev
                )
            });
            result.push(Seq(tokens[prev_pos..pos].to_vec()));
            pos = prev_pos;
        }

        result.reverse();
        result
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }

    /// Get the token ID for a sequence, if it exists in the vocabulary
    pub fn get_token_id(&self, seq: &Seq<T>) -> Option<u32> {
        self.vocab.get(seq).map(|(id, _)| *id)
    }

    /// Compute actual usage counts by segmenting the corpus and counting sequence occurrences.
    /// Returns a new model with IDs reassigned so that ID 0 = most frequently used sequence.
    pub fn reorder_by_actual_usage(self, corpus: &[Vec<T>], seed_set: &HashSet<Seq<T>>) -> Self {
        // Count actual usage of each sequence when segmenting
        let mut usage_counts: Vec<u64> = vec![0; self.id_to_seq.len()];

        for sentence in corpus {
            let segments = self.segment(sentence);
            for seq in &segments {
                if let Some(&(id, _)) = self.vocab.get(seq) {
                    usage_counts[id as usize] += 1;
                }
            }
        }

        // Create (sequence, log_prob, actual_count) tuples and sort by count descending.
        // Drop non-seed multi-token sequences with 0 actual usage — these are n-grams that
        // the Viterbi decoder never selects.
        let mut vocab_with_counts: Vec<(Seq<T>, f64, u32)> = self
            .id_to_seq
            .into_iter()
            .enumerate()
            .filter_map(|(id, seq)| {
                let log_prob = self.vocab.get(&seq).map(|(_, lp)| *lp).unwrap_or(0.0);
                let count = usage_counts[id] as u32;
                // Keep single tokens (always needed), seeds, and sequences with actual usage
                if seq.len() == 1 || count > 0 || seed_set.contains(&seq) {
                    Some((seq, log_prob, count))
                } else {
                    None
                }
            })
            .collect();

        // Sort by actual usage count descending
        vocab_with_counts.sort_by_key(|b| std::cmp::Reverse(b.2));

        UnigramModel::new(vocab_with_counts, self.merge_alpha)
    }
}

/// Configuration for Unigram training
#[derive(Debug, Clone)]
pub struct UnigramTrainerConfig {
    /// Target number of multi-token sequences to learn
    /// (actual vocab size = num_single_tokens + this value)
    pub target_multiword_tokens: usize,
    /// Maximum length of token sequences to consider
    pub max_piece_length: usize,
    /// Shrinking factor for vocabulary pruning
    pub shrinking_factor: f64,
    /// Minimum frequency for a sequence to be considered
    pub min_frequency: u32,
    /// Number of EM iterations to run between prune rounds
    pub em_iterations: usize,
    /// Initial multiword candidate budget as a multiple of the target budget.
    /// Keeping every qualifying n-gram makes the initial vocabulary enormous on
    /// real corpora, which both slows training and drowns useful candidates in junk.
    pub initial_candidate_multiplier: usize,
    /// Blend between pathpiece (α=1, minimize token count) and unigram (α=0,
    /// maximize log-probability). Per-token Viterbi score = (1-α)*log_prob - α.
    /// At α=0.5, merges become cheap enough to use, but junk merges are still avoided.
    pub merge_alpha: f64,
}

impl Default for UnigramTrainerConfig {
    fn default() -> Self {
        Self {
            target_multiword_tokens: 4000,
            max_piece_length: 16,
            shrinking_factor: 0.75,
            min_frequency: 2,
            em_iterations: 10,
            initial_candidate_multiplier: 4,
            merge_alpha: 0.0,
        }
    }
}

/// Trainer for building a Unigram model from a corpus of token sequences
pub struct UnigramTrainer {
    config: UnigramTrainerConfig,
}

impl UnigramTrainer {
    pub fn new(config: UnigramTrainerConfig) -> Self {
        Self { config }
    }

    /// Train a Unigram model from a corpus of token sequences.
    /// `seed_sequences` are token sequences that will be injected into the vocabulary
    /// with their actual corpus frequency, and protected from pruning.
    /// `fixed_counts` are pre-computed expected counts from known segmentations
    /// (e.g. aligned morphological data). These are added to the EM expected counts
    /// at each iteration, biasing the model toward known morphemes without needing
    /// to run EM on those words.
    pub fn train<T: UnigramToken>(
        &self,
        corpus: &[Vec<T>],
        seed_sequences: &[Seq<T>],
    ) -> UnigramModel<T> {
        self.train_with_fixed_counts(corpus, seed_sequences, &FxHashMap::default())
    }

    /// Like [`train`], but with pre-computed fixed counts merged into each EM iteration.
    pub fn train_with_fixed_counts<T: UnigramToken>(
        &self,
        corpus: &[Vec<T>],
        seed_sequences: &[Seq<T>],
        fixed_counts: &FxHashMap<Seq<T>, f64>,
    ) -> UnigramModel<T> {
        let em_iterations = self.config.em_iterations.max(1);

        // Step 1: Count all n-grams up to max_piece_length
        let mut ngram_counts: FxHashMap<Seq<T>, u32> = FxHashMap::default();

        // Build a set of seed sequences for quick lookup
        let filtered_seeds: Vec<Seq<T>> = seed_sequences
            .iter()
            .filter(|seq| is_representable_supertoken_sequence(seq))
            .cloned()
            .collect();
        let seed_set: HashSet<Seq<T>> = filtered_seeds.iter().cloned().collect();

        // Index seeds by first token for fast lookup during counting
        let mut seeds_by_first_token: FxHashMap<&T, Vec<&Seq<T>>> = FxHashMap::default();
        for seed in &filtered_seeds {
            if let Some(first) = seed.0.first() {
                seeds_by_first_token.entry(first).or_default().push(seed);
            }
        }

        for sentence in corpus.iter() {
            for start in 0..sentence.len() {
                for end in start + 1..=sentence.len().min(start + self.config.max_piece_length) {
                    let slice = &sentence[start..end];

                    // For multi-token sequences:
                    // 1. Boundaries must be content tokens (no punct/proper nouns at start/end)
                    // 2. No excluded tokens anywhere (e.g., proper nouns shouldn't be in supertokens)
                    if slice.len() >= 2 {
                        let first_ok = slice[0].is_content();
                        let last_ok = slice[slice.len() - 1].is_content();
                        if !first_ok || !last_ok {
                            continue;
                        }
                        if slice.iter().any(|t| t.is_excluded_from_sequences()) {
                            continue;
                        }
                    }

                    if !seed_set.contains(slice) {
                        if let Some(count) = ngram_counts.get_mut(slice) {
                            *count += 1;
                        } else {
                            ngram_counts.insert(Seq(slice.to_vec()), 1);
                        }
                    }
                }

                // Count seed sequences separately (bypasses boundary filters
                // and handles sequences longer than max_piece_length).
                // Uses first-token index to avoid iterating all seeds at every position.
                if let Some(candidates) = seeds_by_first_token.get(&sentence[start]) {
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

        // Step 2.5: Cap the initial multiword candidate set by raw frequency.
        let initial_multiword_budget = self
            .config
            .target_multiword_tokens
            .saturating_mul(self.config.initial_candidate_multiplier.max(1));
        let multiword_count = ngram_counts.keys().filter(|seq| seq.len() >= 2).count();
        if multiword_count > initial_multiword_budget {
            let mut multiwords: Vec<(Seq<T>, u32)> = ngram_counts
                .iter()
                .filter(|(seq, _)| seq.len() >= 2 && !seed_set.contains(*seq))
                .map(|(seq, count)| (seq.clone(), *count))
                .collect();
            multiwords.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.disambiguation_key().cmp(&b.0.disambiguation_key()))
            });

            let keep_multi: HashSet<Seq<T>> = multiwords
                .into_iter()
                .take(initial_multiword_budget)
                .map(|(seq, _)| seq)
                .collect();

            ngram_counts.retain(|seq, _| {
                seq.len() == 1 || seed_set.contains(seq) || keep_multi.contains(seq)
            });
        }

        // Step 3: Initialize vocabulary with normalized probabilities
        let total_ngrams: u64 = ngram_counts.values().map(|&c| c as u64).sum();

        // Compute initial log probability for each n-gram
        let mut vocab: Vec<(Seq<T>, f64, u32)> = ngram_counts
            .into_iter()
            .map(|(seq, count)| {
                let log_prob = (count as f64 / total_ngrams as f64).ln();
                (seq, log_prob, count)
            })
            .collect();

        // Ensure all single tokens are in vocabulary (required for fallback)
        let mut single_tokens: FxHashMap<Seq<T>, f64> = FxHashMap::default();
        for sentence in corpus {
            for token in sentence {
                let seq = Seq(vec![token.clone()]);
                single_tokens.entry(seq).or_insert(-10.0);
            }
        }
        for (seq, score) in single_tokens {
            if !vocab.iter().any(|(s, _, _)| s == &seq) {
                vocab.push((seq, score, 1)); // count=1 for rare single tokens
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
            let (trained_vocab, _, _) =
                self.run_em(corpus, &vocab, em_iterations, &seed_set, fixed_counts);
            vocab = trained_vocab;
            let model = UnigramModel::new(vocab.clone(), self.config.merge_alpha);
            let mut losses = self.compute_prune_losses(corpus, &vocab, &model, &seed_set);
            losses.sort_by(|a, b| a.1.total_cmp(&b.1));

            let iter_target = (vocab.len() as f64 * self.config.shrinking_factor).ceil() as usize;
            let iter_target = iter_target.max(target_vocab_size);

            let to_remove: std::collections::HashSet<usize> = losses
                .iter()
                .filter(|(_, loss)| loss.is_finite())
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

        let (vocab, _, _) = self.run_em(corpus, &vocab, em_iterations, &seed_set, fixed_counts);
        let model = UnigramModel::new(vocab, self.config.merge_alpha);

        // Reorder by actual usage when segmenting the corpus
        // This ensures ID 0 = most frequently used sequence in practice
        model.reorder_by_actual_usage(corpus, &seed_set)
    }

    fn run_em<T: UnigramToken>(
        &self,
        corpus: &[Vec<T>],
        vocab: &[VocabEntry<T>],
        iterations: usize,
        _seed_set: &HashSet<Seq<T>>,
        fixed_counts: &FxHashMap<Seq<T>, f64>,
    ) -> EmRunResult<T> {
        let mut current: Vec<VocabEntry<T>> = vocab.to_vec();
        let mut last_expected = FxHashMap::default();
        let mut last_likelihood = f64::NEG_INFINITY;

        for _ in 0..iterations {
            let model = UnigramModel::new(current.clone(), self.config.merge_alpha);
            let (mut expected_counts, corpus_log_likelihood) =
                self.compute_expected_counts(corpus, &model);

            // Merge in fixed counts from known segmentations.
            // These act as a prior: known morphemes get a count boost every iteration,
            // anchoring the model toward real morphological segments.
            for (seq, &count) in fixed_counts {
                *expected_counts.entry(seq.clone()).or_insert(0.0) += count;
            }

            let total_expected = expected_counts.values().sum::<f64>();
            assert!(
                total_expected.is_finite() && total_expected > 0.0,
                "EM M-step received a non-finite total expected count.\n\
                 total_expected={total_expected}\n\
                 corpus_log_likelihood={corpus_log_likelihood}\n\
                 nonfinite_expected_entries={:#?}",
                expected_counts
                    .iter()
                    .filter(|(_, value)| !value.is_finite())
                    .take(20)
                    .collect::<Vec<_>>()
            );

            current = current
                .iter()
                .map(|(seq, _, count)| {
                    let expected = *expected_counts.get(seq).unwrap_or(&0.0);
                    let log_prob = if expected.is_finite() && expected > 0.0 {
                        let ratio = expected / total_expected;
                        if ratio.is_finite() && ratio > 0.0 {
                            ratio.ln()
                        } else {
                            LOG_PROB_FLOOR
                        }
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

    fn compute_expected_counts<T: UnigramToken>(
        &self,
        corpus: &[Vec<T>],
        model: &UnigramModel<T>,
    ) -> (ExpectedCounts<T>, f64) {
        let mut expected_counts: ExpectedCounts<T> = FxHashMap::default();
        let mut corpus_log_likelihood = 0.0;

        for sentence in corpus {
            let n = sentence.len();
            let (incoming, outgoing) = self.build_lattice(sentence, model);
            let alpha = self.forward_pass(&incoming);
            let beta = self.backward_pass(&outgoing);

            let sentence_log_prob = alpha[n];
            assert!(
                sentence_log_prob.is_finite(),
                "EM E-step found a sentence with no finite lattice path.\n\
                 sentence_tokens={:#?}\n\
                 max_piece_length={}\n\
                 alpha={:#?}",
                sentence,
                model.max_piece_length,
                alpha
            );
            corpus_log_likelihood += sentence_log_prob;

            for edges in outgoing.iter().take(n) {
                for (end, id, log_prob) in edges {
                    let seq = &model.id_to_seq[*id as usize];
                    let start = end - seq.len();
                    let marginal =
                        (alpha[start] + *log_prob + beta[*end] - sentence_log_prob).exp();
                    *expected_counts.entry(seq.clone()).or_insert(0.0) += marginal;
                }
            }
        }

        (expected_counts, corpus_log_likelihood)
    }

    fn compute_prune_losses<T: UnigramToken>(
        &self,
        corpus: &[Vec<T>],
        vocab: &[VocabEntry<T>],
        model: &UnigramModel<T>,
        seed_set: &HashSet<Seq<T>>,
    ) -> Vec<(usize, f64)> {
        let mut sentence_scores = Vec::with_capacity(corpus.len());
        let mut token_to_sentences = vec![Vec::new(); model.id_to_seq.len()];

        for (sentence_idx, sentence) in corpus.iter().enumerate() {
            let (segmentation, score) = self
                .viterbi_segments_and_score(sentence, model, None)
                .expect("single-token fallback should always permit segmentation");
            sentence_scores.push(score);

            let unique_tokens: HashSet<u32> = segmentation.into_iter().collect();
            for id in unique_tokens {
                token_to_sentences[id as usize].push(sentence_idx);
            }
        }

        let mut losses = Vec::with_capacity(vocab.len());
        for (idx, (seq, _, _)) in vocab.iter().enumerate() {
            if seq.len() == 1 || seed_set.contains(seq) {
                losses.push((idx, f64::INFINITY));
                continue;
            }

            let sentence_indices = &token_to_sentences[idx];
            if sentence_indices.is_empty() {
                losses.push((idx, 0.0));
                continue;
            }

            let mut loss = 0.0;
            for &sentence_idx in sentence_indices {
                let original_score = sentence_scores[sentence_idx];
                let rescored = self
                    .viterbi_score_with_forbidden(&corpus[sentence_idx], model, seq)
                    .unwrap_or_else(|| {
                        let missing_singletons: Vec<u32> = corpus[sentence_idx]
                            .iter()
                            .filter_map(|token| {
                                let singleton = Seq(vec![token.clone()]);
                                (!model.vocab.contains_key(&singleton))
                                    .then_some(singleton.disambiguation_key())
                            })
                            .collect();
                        panic!(
                            "Pruning invariant violated: removing a token made a sentence non-segmentable.\n\
                             sentence_idx={sentence_idx}\n\
                             token_len={}\n\
                             token_disambiguation_key={}\n\
                             token={:#?}\n\
                             sentence_tokens={:#?}\n\
                             missing_singleton_keys={:#?}\n\
                             model_max_piece_length={}\n\
                             original_score={}",
                            seq.len(),
                            seq.disambiguation_key(),
                            seq,
                            &corpus[sentence_idx],
                            missing_singletons,
                            model.max_piece_length,
                            original_score
                        )
                    });
                loss += (original_score - rescored).max(0.0);
            }

            losses.push((idx, loss));
        }

        losses
    }

    fn viterbi_segments_and_score<T: UnigramToken>(
        &self,
        sentence: &[T],
        model: &UnigramModel<T>,
        forbidden: Option<&Seq<T>>,
    ) -> Option<(Vec<u32>, f64)> {
        if sentence.is_empty() {
            return Some((Vec::new(), 0.0));
        }

        let n = sentence.len();
        let mut best_score = vec![f64::NEG_INFINITY; n + 1];
        let mut best_prev: Vec<Option<(usize, u32)>> = vec![None; n + 1];
        best_score[0] = 0.0;

        for start in 0..n {
            if !best_score[start].is_finite() {
                continue;
            }

            for end in start + 1..=n.min(start + model.max_piece_length) {
                let slice = &sentence[start..end];
                if forbidden.is_some_and(|forbidden| forbidden.0 == slice) {
                    continue;
                }
                let Some((id, score)) = model.vocab_entry(slice) else {
                    continue;
                };
                let candidate = best_score[start] + score;
                if candidate > best_score[end] {
                    best_score[end] = candidate;
                    best_prev[end] = Some((start, id));
                }
            }
        }

        let final_score = best_score[n];
        if !final_score.is_finite() {
            return None;
        }

        let mut pos = n;
        let mut segments = Vec::new();
        while pos > 0 {
            let (start, id) = best_prev[pos].take()?;
            segments.push(id);
            pos = start;
        }
        segments.reverse();

        Some((segments, final_score))
    }

    fn viterbi_score_with_forbidden<T: UnigramToken>(
        &self,
        sentence: &[T],
        model: &UnigramModel<T>,
        forbidden: &Seq<T>,
    ) -> Option<f64> {
        if sentence.is_empty() {
            return Some(0.0);
        }

        let n = sentence.len();
        let mut best_score = vec![f64::NEG_INFINITY; n + 1];
        best_score[0] = 0.0;

        for start in 0..n {
            if !best_score[start].is_finite() {
                continue;
            }

            for end in start + 1..=n.min(start + model.max_piece_length) {
                let slice = &sentence[start..end];
                if forbidden.0 == slice {
                    continue;
                }
                let Some((_, score)) = model.vocab_entry(slice) else {
                    continue;
                };
                let candidate = best_score[start] + score;
                if candidate > best_score[end] {
                    best_score[end] = candidate;
                }
            }
        }

        if best_score[n].is_finite() {
            Some(best_score[n])
        } else {
            panic!(
                "Viterbi with forbidden token found no path.\n\
                 forbidden_token={:#?}\n\
                 sentence_tokens={:#?}\n\
                 reachable_positions={:#?}\n\
                 edge_dump={}",
                forbidden,
                sentence,
                best_score
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, score)| score.is_finite().then_some(idx))
                    .collect::<Vec<_>>(),
                self.debug_edge_dump(sentence, model, forbidden, &best_score)
            );
        }
    }

    fn debug_edge_dump<T: UnigramToken>(
        &self,
        sentence: &[T],
        model: &UnigramModel<T>,
        forbidden: &Seq<T>,
        best_score: &[f64],
    ) -> String {
        let n = sentence.len();
        let mut out = String::new();

        for start in 0..n {
            if !best_score[start].is_finite() {
                continue;
            }

            out.push_str(&format!("\nstart={start} token={:#?}\n", sentence[start]));
            for end in start + 1..=n.min(start + model.max_piece_length) {
                let seq = Seq(sentence[start..end].to_vec());
                let is_forbidden = &seq == forbidden;
                let score = model.vocab_score(&seq);
                out.push_str(&format!(
                    "  end={end} forbidden={is_forbidden} score={score:?} seq={seq:#?}\n"
                ));
            }
        }

        out
    }

    fn build_lattice<T: UnigramToken>(
        &self,
        sentence: &[T],
        model: &UnigramModel<T>,
    ) -> (SentenceLattice, SentenceLattice) {
        let n = sentence.len();
        let mut incoming: SentenceLattice = vec![Vec::new(); n + 1];
        let mut outgoing: SentenceLattice = vec![Vec::new(); n + 1];

        for start in 0..n {
            for end in start + 1..=n.min(start + model.max_piece_length) {
                let Some((id, score)) = model.vocab_entry(&sentence[start..end]) else {
                    continue;
                };
                incoming[end].push((start, id, score));
                outgoing[start].push((end, id, score));
            }
        }

        (incoming, outgoing)
    }

    fn forward_pass(&self, incoming: &SentenceLattice) -> Vec<f64> {
        let n = incoming.len() - 1;
        let mut alpha = vec![f64::NEG_INFINITY; n + 1];
        alpha[0] = 0.0;

        for end in 1..=n {
            alpha[end] = logsumexp_by(&incoming[end], |(start, _, log_prob)| {
                alpha[*start] + *log_prob
            });
        }

        alpha
    }

    fn backward_pass(&self, outgoing: &SentenceLattice) -> Vec<f64> {
        let n = outgoing.len() - 1;
        let mut beta = vec![f64::NEG_INFINITY; n + 1];
        beta[n] = 0.0;

        for start in (0..n).rev() {
            beta[start] = logsumexp_by(&outgoing[start], |(end, _, log_prob)| {
                *log_prob + beta[*end]
            });
        }

        beta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple test token type to test the generic unigram algorithm
    /// without depending on language-utils.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    #[allow(dead_code)]
    enum TestToken {
        Word(String),
        Control,
        ProperNoun(String),
        Punct(String),
    }

    impl UnigramToken for TestToken {
        fn can_be_sequence_boundary(&self) -> bool {
            !matches!(self, TestToken::Control)
        }

        fn is_content(&self) -> bool {
            matches!(self, TestToken::Word(_))
        }

        fn is_excluded_from_sequences(&self) -> bool {
            matches!(self, TestToken::ProperNoun(_))
        }
    }

    fn make_token(text: &str) -> TestToken {
        TestToken::Word(text.to_string())
    }

    #[test]
    fn test_segment_single_tokens() {
        let seq = Seq(vec![make_token("hello")]);
        assert_eq!(seq.len(), 1);
    }

    #[test]
    fn test_segment_multi_tokens() {
        let seq = Seq(vec![make_token("qu'"), make_token("est"), make_token("ce")]);
        assert_eq!(seq.len(), 3);
        assert_eq!(seq.0[0], make_token("qu'"));
        assert_eq!(seq.0[2], make_token("ce"));
    }

    #[test]
    fn test_unigram_training_basic() {
        // Create a simple corpus with repeated patterns
        let corpus: Vec<Vec<TestToken>> = vec![
            vec![make_token("je"), make_token("suis")],
            vec![make_token("je"), make_token("suis"), make_token("content")],
            vec![make_token("tu"), make_token("es")],
            vec![make_token("je"), make_token("suis"), make_token("là")],
            vec![make_token("il"), make_token("est")],
            vec![make_token("je"), make_token("suis"), make_token("ici")],
        ];

        let config = UnigramTrainerConfig {
            target_multiword_tokens: 5,
            max_piece_length: 4,
            shrinking_factor: 0.8,
            min_frequency: 2,
            em_iterations: 8,
            initial_candidate_multiplier: 4,
            merge_alpha: 0.0,
        };

        let trainer = UnigramTrainer::new(config);
        let model = trainer.train(&corpus, &[]);

        // The model should have learned that "je suis" is common
        let tokens = vec![make_token("je"), make_token("suis"), make_token("content")];
        let segmented = model.segment(&tokens);

        // Should have fewer segments than tokens due to merging
        assert!(segmented.len() <= tokens.len());
    }

    #[test]
    fn test_viterbi_segmentation() {
        // Create a model with known vocabulary (seq, score, count)
        let vocab = vec![
            (Seq(vec![make_token("je")]), -1.0, 100),
            (Seq(vec![make_token("suis")]), -1.0, 100),
            (Seq(vec![make_token("content")]), -1.0, 50),
            (Seq(vec![make_token("je"), make_token("suis")]), -0.5, 80), // More likely as pair
        ];

        let model = UnigramModel::new(vocab, 0.0);

        let tokens = vec![make_token("je"), make_token("suis"), make_token("content")];
        let segmented = model.segment(&tokens);

        // Should prefer "je suis" as a merged token since it has higher probability
        assert_eq!(segmented.len(), 2); // "je suis" + "content"
    }

    #[test]
    fn test_viterbi_only_uses_vocab_entries() {
        let vocab = vec![
            (Seq(vec![make_token("new")]), -1.0, 100),
            (Seq(vec![make_token("york")]), -1.0, 100),
        ];

        let model = UnigramModel::new(vocab, 0.0);
        let tokens = vec![make_token("new"), make_token("york")];
        let segmented = model.segment(&tokens);

        assert_eq!(segmented.len(), 2);
        assert_eq!(segmented[0].len(), 1);
        assert_eq!(segmented[1].len(), 1);
    }

    #[test]
    fn test_viterbi_respects_longest_vocab_entry() {
        let tokens: Vec<_> = (0..20).map(|i| make_token(&format!("w{i}"))).collect();
        let vocab = vec![
            (Seq(tokens.clone()), -0.1, 50),
            (Seq(vec![make_token("w0")]), -5.0, 100),
            (Seq(vec![make_token("w1")]), -5.0, 100),
            (Seq(vec![make_token("w2")]), -5.0, 100),
            (Seq(vec![make_token("w3")]), -5.0, 100),
            (Seq(vec![make_token("w4")]), -5.0, 100),
            (Seq(vec![make_token("w5")]), -5.0, 100),
            (Seq(vec![make_token("w6")]), -5.0, 100),
            (Seq(vec![make_token("w7")]), -5.0, 100),
            (Seq(vec![make_token("w8")]), -5.0, 100),
            (Seq(vec![make_token("w9")]), -5.0, 100),
            (Seq(vec![make_token("w10")]), -5.0, 100),
            (Seq(vec![make_token("w11")]), -5.0, 100),
            (Seq(vec![make_token("w12")]), -5.0, 100),
            (Seq(vec![make_token("w13")]), -5.0, 100),
            (Seq(vec![make_token("w14")]), -5.0, 100),
            (Seq(vec![make_token("w15")]), -5.0, 100),
            (Seq(vec![make_token("w16")]), -5.0, 100),
            (Seq(vec![make_token("w17")]), -5.0, 100),
            (Seq(vec![make_token("w18")]), -5.0, 100),
            (Seq(vec![make_token("w19")]), -5.0, 100),
        ];

        let model = UnigramModel::new(vocab, 0.0);
        let segmented = model.segment(&tokens);

        assert_eq!(segmented.len(), 1);
        assert_eq!(segmented[0].len(), 20);
        assert_eq!(segmented[0].0[0], make_token("w0"));
        assert_eq!(segmented[0].0[19], make_token("w19"));
    }

    #[test]
    fn test_em_expected_counts_are_consistent() {
        let corpus = vec![vec![make_token("a"), make_token("b")]];
        let vocab = vec![
            (Seq(vec![make_token("a")]), 0.4f64.ln(), 10),
            (Seq(vec![make_token("b")]), 0.4f64.ln(), 10),
            (Seq(vec![make_token("a"), make_token("b")]), 0.2f64.ln(), 10),
        ];
        let trainer = UnigramTrainer::new(UnigramTrainerConfig::default());
        let model = UnigramModel::new(vocab, 0.0);

        let (expected_counts, _) = trainer.compute_expected_counts(&corpus, &model);
        let total_expected = expected_counts.values().sum::<f64>();

        assert!(total_expected >= 1.0);
        assert!(total_expected <= 2.0);
        assert!(expected_counts.contains_key(&Seq(vec![make_token("a"), make_token("b")])));
    }

    #[test]
    fn test_merge_alpha_affects_em_expected_counts() {
        let corpus = vec![vec![make_token("a"), make_token("b")]];
        let vocab = vec![
            (Seq(vec![make_token("a")]), 0.4f64.ln(), 10),
            (Seq(vec![make_token("b")]), 0.4f64.ln(), 10),
            (Seq(vec![make_token("a"), make_token("b")]), 0.2f64.ln(), 10),
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

        let merged = Seq(vec![make_token("a"), make_token("b")]);
        let merged_plain = *expected_plain.get(&merged).unwrap();
        let merged_merge = *expected_merge.get(&merged).unwrap();

        assert!(merged_merge > merged_plain);
    }

    #[test]
    fn test_viterbi_score_with_forbidden_matches_resegmentation_loss() {
        let trainer = UnigramTrainer::new(UnigramTrainerConfig::default());
        let vocab = vec![
            (Seq(vec![make_token("a")]), 0.1f64.ln(), 10),
            (Seq(vec![make_token("b")]), 0.1f64.ln(), 10),
            (Seq(vec![make_token("c")]), 0.1f64.ln(), 10),
            (
                Seq(vec![make_token("a"), make_token("b")]),
                0.35f64.ln(),
                10,
            ),
            (
                Seq(vec![make_token("a"), make_token("b"), make_token("c")]),
                0.25f64.ln(),
                10,
            ),
        ];
        let model = UnigramModel::new(vocab, 0.0);
        let sentence = vec![make_token("a"), make_token("b"), make_token("c")];
        let removed = Seq(vec![make_token("a"), make_token("b"), make_token("c")]);

        let (_, original_score) = trainer
            .viterbi_segments_and_score(&sentence, &model, None)
            .unwrap();
        let rescored = trainer
            .viterbi_score_with_forbidden(&sentence, &model, &removed)
            .unwrap();
        let expected_rescored = 0.35f64.ln() + 0.1f64.ln();

        assert!((rescored - expected_rescored).abs() < 1e-9);
        assert!((original_score - 0.25f64.ln()).abs() < 1e-9);
        assert!(original_score > rescored);
    }

    #[test]
    fn test_forward_backward_agree_on_sentence_probability() {
        let sentence = vec![
            make_token("the"),
            make_token("new"),
            make_token("york"),
            make_token("times"),
        ];
        let vocab = vec![
            (Seq(vec![make_token("the")]), 0.2f64.ln(), 10),
            (Seq(vec![make_token("new")]), 0.1f64.ln(), 10),
            (Seq(vec![make_token("york")]), 0.1f64.ln(), 10),
            (Seq(vec![make_token("times")]), 0.1f64.ln(), 10),
            (
                Seq(vec![make_token("new"), make_token("york")]),
                0.2f64.ln(),
                10,
            ),
            (
                Seq(vec![
                    make_token("new"),
                    make_token("york"),
                    make_token("times"),
                ]),
                0.15f64.ln(),
                10,
            ),
            (
                Seq(vec![
                    make_token("the"),
                    make_token("new"),
                    make_token("york"),
                ]),
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

    #[test]
    fn test_proper_nouns_excluded_from_sequences() {
        let corpus: Vec<Vec<TestToken>> = vec![
            vec![
                TestToken::ProperNoun("Tom".to_string()),
                make_token("est"),
                make_token("content"),
            ],
            vec![
                TestToken::ProperNoun("Tom".to_string()),
                make_token("est"),
                make_token("là"),
            ],
            vec![make_token("il"), make_token("est"), make_token("content")],
            vec![make_token("il"), make_token("est"), make_token("là")],
        ];

        let config = UnigramTrainerConfig {
            target_multiword_tokens: 5,
            max_piece_length: 4,
            shrinking_factor: 0.8,
            min_frequency: 2,
            em_iterations: 8,
            initial_candidate_multiplier: 4,
            merge_alpha: 0.0,
        };

        let trainer = UnigramTrainer::new(config);
        let model = trainer.train(&corpus, &[]);

        // Check that no multi-token sequence contains a proper noun
        for seq in model.get_vocab() {
            if seq.len() > 1 {
                for token in seq.iter() {
                    assert!(
                        !token.is_excluded_from_sequences(),
                        "Proper noun found in multi-token sequence: {seq:?}"
                    );
                }
            }
        }
    }
}
