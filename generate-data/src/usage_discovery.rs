//! Usage & collocation discovery over cached token embeddings.
//!
//! A "usage" is a pedagogical unit, not a dictionary sense: two usages of a
//! gram are distinct exactly when a teacher would introduce them to a
//! learner one at a time rather than together. That deliberately includes
//! grammatical constructions (French "ils sont ..." vs the inverted
//! "sont-ils ...?") and conversational formulas (sentence-final "non ?" as
//! a confirmation tag) alongside genuine polysemy — each usage carries a
//! `kind` (meaning / construction / formula) so downstream consumers can
//! treat the categories differently.
//!
//! Mines every gram's occurrence cloud for cluster structure (recursive
//! 2-means plus HDBSCAN on large clouds — both purely geometric, tuned to
//! over-propose), then has an LLM adjudicate each gram in a single call.
//! Clusters organize the judge's reading material and pick which grams are
//! worth judging, but the unit of judgment is the *line*: the judge defines
//! the gram's usage inventory and labels every clearly-classifiable line
//! with the usage (or fixed expression) it shows. Line-level labels are
//! what make the artifacts trustworthy where cluster-level grouping was
//! not: clusters overlap (an HDBSCAN cluster can carve a subset out of a
//! 2-means leaf) and big background clusters are usually mixtures, so any
//! per-cluster verdict inherits their double counting and mislabeling.
//! The labeled lines ("gold") then drive a purely geometric extension:
//! per-usage centroids classify every occurrence in the cloud, yielding a
//! true partition with honest counts, a leave-one-out agreement score per
//! usage, and per-occurrence corpus labels. The same anchors + centroid
//! machinery classifies the gram's occurrences in *arbitrary* sentences
//! later (see [`usage_centroids`] and [`InventoryCentroids::classify`]) — anchors
//! are stored as (sentence, spans) rather than raw vectors, so they can be
//! re-embedded under any future embedding model.
//!
//! The expression lane is unchanged in spirit: contextual embeddings give a
//! word inside a fixed expression a tight, distinctive cluster, so geometry
//! alone cannot tell polysemy from collocation membership — the judge sees
//! both interpretations at once (e.g. "au clair" splitting into "au clair
//! de lune" vs "tirer au clair" has zero usages of its own, just two host
//! expressions). Writes files under `generate-data/data/{lang}/` that feed
//! the next generate-data run directly — there is no human review step, so
//! the judges are the quality bar:
//!
//! - `usage_inventories.jsonl`: grams with two or more usages, each usage
//!   with kind, gloss, gold + assigned counts, LOO agreement, and anchor
//!   sentences. Also writes per-occurrence labels for the whole corpus to
//!   `out/{lang}/usage_labels.jsonl` (inspection + downstream labeling).
//! - `discovered_multiword_terms.jsonl`: grounded, novel, frequent
//!   expressions (the LLM only cites lines and copies verbatim substrings;
//!   surface forms, gram sequences, and corpus counts are derived here)
//!   that a final per-expression opacity judgment deemed worth learning as
//!   a unit — the adjudicator reports even compositional patterns (to keep
//!   their lines out of the usage gold), and the opacity stage is what
//!   keeps "dire merci"-class free combinations out of the vocabulary.
//!
//! Grams are treated as opaque identities throughout: an occurrence is a
//! token of the encoder's gram stream — an exact instance of its gram, so
//! every point in a cloud is truly the key (multiword matches are
//! lemma-loosened and contribute no occurrences; they only claim their
//! words) — its vector is the mean of the cached vectors of the heteronym
//! words it covers, and extracted expressions are sequences of grams. No
//! linguistic re-analysis (lemmas, POS grouping) happens here — whatever
//! units the gram system defines are the units mined.
//!
//! Run by the standalone `usage_discovery` binary; nothing in the main
//! generate-data pipeline consumes these files except
//! `wiktionary_terms::extra_multiword_terms`, which folds the discovered
//! terms into the next run's multiword-term inventory as-is.

use anyhow::{Context, Result};
use language_utils::{Atom, Gram, Language, SentenceInfo, WordType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

use crate::pipeline::SegmentedCorpus;
use crate::token_embeddings;

/// A gram needs at least this many occurrences to be mined at all, and a
/// split side needs at least this many to be recursively re-split.
const MIN_OCC: usize = 8;
/// Absolute floor for the smaller side of a split (an absolute count, not a
/// balance fraction — a fraction tuned on small samples would kill real rare
/// structure like "à temps" at 7% of "temps").
const MIN_SIDE: usize = 5;
/// Maximum recursive 2-means split depth (up to 2^MAX_DEPTH leaves).
const MAX_DEPTH: usize = 3;
/// Every 2-means split (root included) must clear this cosine-silhouette
/// floor. Judge-confirmed root splits in French ranged 0.575-0.77, so this
/// floor discards only structure weaker than anything ever confirmed;
/// over-splitting past it is cheap because the adjudicator merges leaves.
const SPLIT_SILHOUETTE: f64 = 0.55;
/// Cap on clusters shown to the adjudicator for one gram (2-means leaves
/// first, then novel HDBSCAN clusters by silhouette).
const MAX_CLUSTERS: usize = 8;
/// How many top-silhouette grams (by root split) get adjudicated per
/// language. At 100+50, roughly a third of judged grams yielded artifacts —
/// nowhere near dry — so this probes deeper; adjudications are cached, so
/// raising it only ever costs the new grams.
const JUDGE_TOP: usize = 400;
/// Lines shown to the adjudicator per cluster: the most central (margin
/// filtered, they characterize what the cluster is about) plus a spread
/// sample strided across the whole core→edge spectrum (they reveal whether
/// the cluster is homogeneous — a background cluster's central exemplars
/// systematically misrepresent it, which once glossed the 3682-occurrence
/// mixed remainder of French "mais" as a single sense).
const EXEMPLARS_CENTRAL: usize = 4;
const EXEMPLARS_SPREAD: usize = 4;
/// Lines in the final "random sample" section: a deterministic stride over
/// the entire occurrence cloud, so the judge always sees the item's true
/// usage distribution no matter how unrepresentative the clusters are.
const RANDOM_SAMPLE_LINES: usize = 8;
/// A usage (or expression absorber class) needs this many resolvable gold
/// lines to get a centroid; below this the extension would be classifying
/// against noise.
const MIN_GOLD: usize = 3;
/// Anchor sentences stored per usage (gold lines first, then high-margin
/// assigned occurrences strided across the usage's similarity range).
const MAX_ANCHORS: usize = 16;
/// Silhouette is O(n²); score on a deterministic subsample past this size.
const SIL_SAMPLE: usize = 500;
/// HDBSCAN (the secondary proposer) only runs where density estimation is
/// in-regime; below this it demonstrably degenerates (see plan/experiments).
const HDBSCAN_MIN_N: usize = 150;
const HDBSCAN_MIN_CLUSTER: usize = 5;
/// HDBSCAN is O(n²)-ish in this dimensionality; larger clouds are
/// deterministically subsampled to this size before clustering.
const HDBSCAN_MAX_N: usize = 2000;
/// How many additional grams surfaced by HDBSCAN get adjudicated per
/// language, ranked by best novel-cluster silhouette (the full corpus has
/// thousands of n>=150 keys; unbounded, the secondary proposer would dwarf
/// the primary one it is meant to complement).
const HDBSCAN_TOP: usize = 50;
/// A grounded expression must recur this often in the corpus to be proposed.
const MIN_EXPRESSION_COUNT: usize = 3;
/// Examples shown per paradigm member in the opacity prompt. A paradigm can
/// have several members, so this is a per-member budget rather than a total.
const EXAMPLES_PER_PARADIGM: usize = 2;

/// Judgment-tier model, same reasoning as slot grading: few hundred calls
/// per language, and each one decides what enters the review files.
static JUDGE_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-terra"));

/// Client for the polysemy probe list — one call per language.
static PROBE_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-sol"));

/// Gram identity: the gram's index in the trainer vocabulary, which is also
/// its token id in the encoded sentences.
type GramId = u32;

struct GramArena {
    grams: Vec<Gram<String>>,
}

impl GramArena {
    fn from_vocabulary(vocab: &[language_utils::GramVocabEntry<String>]) -> Self {
        let grams: Vec<Gram<String>> = vocab.iter().map(|e| e.atoms.clone()).collect();
        GramArena { grams }
    }

    fn display(&self, id: GramId, lang: Language) -> String {
        self.grams[id as usize].to_display_string(lang)
    }
}

/// One occurrence of a gram: the sentence text (reconstructed from the
/// words, so char offsets agree with the embedding cache) and the char spans
/// of the heteronym words the gram covers.
#[derive(Clone)]
struct Occurrence {
    text: String,
    spans: Vec<(u32, u32)>,
}

/// A word of a sentence with both char span (embedding cache convention) and
/// byte span (for substring grounding).
struct WordSpan {
    char_span: (u32, u32),
    byte_span: (usize, usize),
    is_heteronym: bool,
}

/// Interned atom identity (for expression grounding/counting — a proposed
/// new gram is a sequence of atoms, independent of how the encoder chunked
/// the words into existing grams). `Atom<Spur>` is `Copy` with interned
/// strings, so it needs no arena of its own.
type SpurAtom = Atom<lasso::Spur>;

/// Per-sentence info shared by occurrence collection and expression
/// grounding: the word spans, the sentence's gram segmentation (each stream
/// entry is a gram id and the word range it covers), and the per-word atom
/// ids (the substrate expression proposals are made of).
struct SentenceIndex {
    words: Vec<WordSpan>,
    /// (gram id, first word index, one-past-last word index), in sentence
    /// order.
    gram_stream: Vec<(GramId, u16, u16)>,
    /// One `Tok` atom per word, in interned (Spur) form — the substrate
    /// expression proposals are made of.
    atom_seq: Vec<SpurAtom>,
}

/// Index a sentence for mining: decode its words (spans in both char and
/// byte terms) and take its gram segmentation and atoms directly from the
/// encoded form (aligned by construction).
fn index_sentence(
    info: &SentenceInfo,
    interners: &language_utils::GramInterners,
    language: Language,
) -> (String, SentenceIndex) {
    let decoded = info.decode_words(interners, language);
    let text = token_embeddings::sentence_text(&decoded);
    let mut words = Vec::with_capacity(decoded.len());
    let mut char_off = 0u32;
    let mut byte_off = 0usize;
    for literal in &decoded {
        let char_len = literal.word.text.chars().count() as u32;
        let byte_len = literal.word.text.len();
        words.push(WordSpan {
            char_span: (char_off, char_off + char_len),
            byte_span: (byte_off, byte_off + byte_len),
            is_heteronym: matches!(literal.word.word_type, WordType::Heteronym(_)),
        });
        char_off += char_len + literal.whitespace.chars().count() as u32;
        byte_off += byte_len + literal.whitespace.len();
    }
    use lasso::Key;
    let gram_stream: Vec<(GramId, u16, u16)> = info
        .gram_word_ranges(interners)
        .into_iter()
        .filter(|(_, range)| !range.is_empty())
        .map(|(key, range)| {
            (
                key.into_usize() as GramId,
                range.start as u16,
                range.end as u16,
            )
        })
        .collect();
    let atom_seq: Vec<SpurAtom> = info
        .sentence
        .tokens
        .iter()
        .flat_map(|&key| interners.atoms(key).iter().copied())
        .filter(|a| matches!(a, Atom::Tok(_)))
        .collect();
    debug_assert_eq!(atom_seq.len(), words.len());
    (
        text,
        SentenceIndex {
            words,
            gram_stream,
            atom_seq,
        },
    )
}

/// Collect per-gram occurrences from the encoder's token stream — and only
/// from it. A stream token IS its gram (exact surface, by construction), so
/// every point in a mined cloud is a true instance of the key; that
/// invariant is what lets cluster structure be read as polysemy. Multiword
/// matches contribute NO occurrences — matching is deliberately
/// lemma-loosened for coverage, so a match is evidence the term applies,
/// not that its gram appears (keying matches by their citation gram once
/// put "J'ai aimé" lines in the "j'aurais aimé" cloud, and the resulting
/// tense cluster judged as a sense at 0.98 confidence). High-confidence
/// matches only claim their member words, which keeps the interiors of
/// already-adopted collocations invisible to the miner — the convergence
/// property of the discover→adopt→re-segment loop.
fn collect_occurrences(
    corpus: &SegmentedCorpus,
    language: Language,
    index: &HashMap<String, SentenceIndex>,
    arena: &GramArena,
) -> BTreeMap<GramId, Vec<Occurrence>> {
    let mut occurrences: BTreeMap<GramId, Vec<Occurrence>> = BTreeMap::new();
    for info in corpus.nlp_sentences.values() {
        let decoded = info.decode_words(&corpus.interners, language);
        let text = token_embeddings::sentence_text(&decoded);
        let Some(sent) = index.get(&text) else {
            continue;
        };
        let consumed: HashSet<usize> = info
            .multiword_terms
            .high_confidence
            .iter()
            .flat_map(|m| m.matched_word_indices.iter().map(|&i| i as usize))
            .collect();
        for &(id, start, end) in &sent.gram_stream {
            if (start..end).any(|i| consumed.contains(&(i as usize))) {
                continue;
            }
            if !arena.grams[id as usize].is_learnable() {
                continue;
            }
            let spans: Vec<(u32, u32)> = (start as usize..end as usize)
                .filter_map(|i| sent.words.get(i))
                .filter(|w| w.is_heteronym && w.char_span.0 < w.char_span.1)
                .map(|w| w.char_span)
                .collect();
            if spans.is_empty() {
                continue;
            }
            occurrences.entry(id).or_default().push(Occurrence {
                text: text.clone(),
                spans,
            });
        }
    }
    occurrences.retain(|_, v| v.len() >= MIN_OCC);
    occurrences
}

// ---------------------------------------------------------------------------
// Clustering primitives
// ---------------------------------------------------------------------------

fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Dot product with eight independent accumulator lanes. A plain
/// `zip().map().sum()` fixes the summation order to a single serial chain,
/// which blocks SIMD (float addition is not associative) and runs at FP-add
/// latency per element; spelling out the lanes fixes a *parallel* summation
/// order the compiler can vectorize. Deterministic across runs and builds.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 8];
    for (ca, cb) in a.chunks_exact(8).zip(b.chunks_exact(8)) {
        for k in 0..8 {
            acc[k] += ca[k] * cb[k];
        }
    }
    let mut tail = 0.0f32;
    for (x, y) in a
        .chunks_exact(8)
        .remainder()
        .iter()
        .zip(b.chunks_exact(8).remainder())
    {
        tail += x * y;
    }
    (((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]))) + tail
}

fn mean_normalized(vectors: &[&Vec<f32>]) -> Vec<f32> {
    let dim = vectors[0].len();
    let mut out = vec![0.0f32; dim];
    for v in vectors {
        for (o, x) in out.iter_mut().zip(v.iter()) {
            *o += x;
        }
    }
    for o in out.iter_mut() {
        *o /= vectors.len() as f32;
    }
    normalize(&mut out);
    out
}

/// Deterministic 2-means on L2-normalized vectors (cosine geometry): best of
/// `n_init` seeded runs by inertia. Returns per-point labels and the two
/// normalized centroids, with cluster 0 always the larger side.
fn kmeans2(vectors: &[&Vec<f32>]) -> (Vec<u8>, [Vec<f32>; 2]) {
    let n = vectors.len();
    assert!(n >= 2);
    let mut best: Option<(f32, Vec<u8>, [Vec<f32>; 2])> = None;
    for seed in 0..10u64 {
        // Simple deterministic LCG for picking two distinct initial centers.
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = |bound: usize| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as usize) % bound
        };
        let i = next(n);
        let mut j = next(n);
        if j == i {
            j = (i + 1) % n;
        }
        let dim = vectors[0].len();
        let mut centroids = [vectors[i].clone(), vectors[j].clone()];
        let mut labels = vec![0u8; n];
        for _ in 0..100 {
            // Assign and accumulate the new centroids in one pass. The sums
            // run in index order, exactly like the old collect-then-mean
            // version, so the arithmetic is unchanged.
            let mut changed = false;
            let mut sums = [vec![0.0f32; dim], vec![0.0f32; dim]];
            let mut counts = [0usize; 2];
            for (k, v) in vectors.iter().enumerate() {
                let label = u8::from(dot(v, &centroids[1]) > dot(v, &centroids[0]));
                if labels[k] != label {
                    labels[k] = label;
                    changed = true;
                }
                let sum = &mut sums[label as usize];
                for (o, x) in sum.iter_mut().zip(v.iter()) {
                    *o += x;
                }
                counts[label as usize] += 1;
            }
            for c in 0..2 {
                if counts[c] > 0 {
                    let mut centroid = std::mem::take(&mut sums[c]);
                    for o in centroid.iter_mut() {
                        *o /= counts[c] as f32;
                    }
                    normalize(&mut centroid);
                    centroids[c] = centroid;
                }
            }
            if !changed {
                break;
            }
        }
        let inertia: f32 = vectors
            .iter()
            .zip(&labels)
            .map(|(v, &l)| 1.0 - dot(v, &centroids[l as usize]))
            .sum();
        if best.as_ref().is_none_or(|(b, _, _)| inertia < *b) {
            best = Some((inertia, labels, centroids));
        }
    }
    let (_, mut labels, mut centroids) = best.unwrap();
    // Canonicalize: cluster 0 is the larger side (stable across seeds).
    let zeros = labels.iter().filter(|&&l| l == 0).count();
    if zeros * 2 < n {
        for l in labels.iter_mut() {
            *l = 1 - *l;
        }
        centroids.swap(0, 1);
    }
    (labels, centroids)
}

/// Mean cosine silhouette of a binary labeling, on a deterministic subsample
/// of at most `SIL_SAMPLE` points (silhouette is the one O(n²·d) piece).
fn cosine_silhouette(vectors: &[&Vec<f32>], labels: &[u8]) -> f64 {
    let n = vectors.len();
    let stride = n.div_ceil(SIL_SAMPLE);
    let sample: Vec<usize> = (0..n).step_by(stride).collect();
    let mut total = 0.0f64;
    let mut counted = 0usize;
    for &i in &sample {
        let mut sum = [0.0f64; 2];
        let mut cnt = [0usize; 2];
        for &j in &sample {
            if i == j {
                continue;
            }
            let d = 1.0 - f64::from(dot(vectors[i], vectors[j]));
            sum[labels[j] as usize] += d;
            cnt[labels[j] as usize] += 1;
        }
        let own = labels[i] as usize;
        let other = 1 - own;
        if cnt[own] == 0 || cnt[other] == 0 {
            continue;
        }
        let a = sum[own] / cnt[own] as f64;
        let b = sum[other] / cnt[other] as f64;
        total += (b - a) / a.max(b).max(f64::EPSILON);
        counted += 1;
    }
    if counted == 0 {
        0.0
    } else {
        total / counted as f64
    }
}

// ---------------------------------------------------------------------------
// Cluster proposal (purely geometric — the adjudicator arbitrates)
// ---------------------------------------------------------------------------

/// One candidate cluster of a gram's occurrence cloud. Only the exemplars
/// survive into the prompt — the judge labels lines, so cluster membership
/// carries no downstream authority.
struct Cluster {
    /// Exemplar occurrence indices (see [`cluster_exemplars`]).
    exemplars: Vec<usize>,
}

/// Everything the adjudicator sees for one gram: its candidate clusters
/// (which may overlap — an HDBSCAN cluster can carve a subset out of a
/// 2-means leaf) plus the ranking scores used to pick which grams to judge.
struct KeyProposal {
    key: GramId,
    clusters: Vec<Cluster>,
    /// Root 2-means split silhouette (None if the cloud never split).
    root_silhouette: Option<f64>,
    /// Best novel HDBSCAN cluster-vs-rest silhouette (None without novel
    /// clusters).
    hdbscan_silhouette: Option<f64>,
}

/// A cluster's exemplar lines: `EXEMPLARS_CENTRAL` most-central members
/// (margin filtered against the nearest other cluster — an occurrence
/// near-tied between centroids is exactly the noise HDBSCAN would have
/// excluded) plus `EXEMPLARS_SPREAD` members strided across the remaining
/// core→edge similarity range, so a heterogeneous cluster shows its
/// heterogeneity instead of only its center.
fn cluster_exemplars(
    vectors: &[&Vec<f32>],
    side: &[usize],
    own: &[f32],
    other: &[f32],
) -> Vec<usize> {
    let mut ranked: Vec<(usize, f32, f32)> = side
        .iter()
        .map(|&i| (i, dot(vectors[i], own), dot(vectors[i], other)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let confident: Vec<usize> = ranked
        .iter()
        .filter(|(_, o, x)| o > x)
        .map(|(i, _, _)| *i)
        .take(EXEMPLARS_CENTRAL)
        .collect();
    let mut picked: Vec<usize> = if confident.len() >= EXEMPLARS_CENTRAL.min(side.len()).min(3) {
        confident
    } else {
        // Degenerate margins (tiny side): fall back to nearest-to-centroid.
        ranked
            .iter()
            .map(|(i, _, _)| *i)
            .take(EXEMPLARS_CENTRAL)
            .collect()
    };
    let chosen: HashSet<usize> = picked.iter().copied().collect();
    let rest: Vec<usize> = ranked
        .iter()
        .map(|(i, _, _)| *i)
        .filter(|i| !chosen.contains(i))
        .collect();
    if !rest.is_empty() {
        let stride = rest.len().div_ceil(EXEMPLARS_SPREAD);
        picked.extend(rest.iter().step_by(stride.max(1)).take(EXEMPLARS_SPREAD));
    }
    picked
}

/// Recursively 2-means-split `subset` into leaves while the geometry
/// supports it: silhouette over the floor, both sides over the absolute
/// minimum, depth and leaf count in bounds. No judgment gates recursion —
/// over-splitting is safe because the judge labels lines, not clusters.
/// Returns the silhouette of the split performed at this level, if any.
fn kmeans_leaves(
    vectors: &[Vec<f32>],
    subset: Vec<usize>,
    depth: usize,
    leaves: &mut Vec<Vec<usize>>,
) -> Option<f64> {
    if depth > MAX_DEPTH || subset.len() < MIN_OCC || leaves.len() + 2 > MAX_CLUSTERS {
        leaves.push(subset);
        return None;
    }
    let sub_vecs: Vec<&Vec<f32>> = subset.iter().map(|&i| &vectors[i]).collect();
    let (labels, _) = kmeans2(&sub_vecs);
    let mut sides: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for (k, &l) in labels.iter().enumerate() {
        sides[l as usize].push(subset[k]);
    }
    if sides[1].len() < MIN_SIDE {
        leaves.push(subset);
        return None;
    }
    let silhouette = cosine_silhouette(&sub_vecs, &labels);
    if silhouette < SPLIT_SILHOUETTE {
        leaves.push(subset);
        return None;
    }
    let [a, b] = sides;
    kmeans_leaves(vectors, a, depth + 1, leaves);
    kmeans_leaves(vectors, b, depth + 1, leaves);
    Some(silhouette)
}

/// Secondary proposer: HDBSCAN on large occurrence clouds, where density
/// estimation is in-regime. Clusters that essentially duplicate a 2-means
/// leaf are dropped; the rest are returned with a cluster-vs-rest
/// silhouette for ranking. A bad proposal costs the adjudicator one extra
/// cluster to look at, nothing more.
fn hdbscan_clusters(vectors: &[Vec<f32>], leaves: &[Vec<usize>]) -> Vec<(Vec<usize>, f64)> {
    if vectors.len() < HDBSCAN_MIN_N {
        return Vec::new();
    }
    // Deterministic stride subsample: HDBSCAN is quadratic-ish here, and a
    // 2000-point sample keeps density structure while bounding cost.
    let stride = vectors.len().div_ceil(HDBSCAN_MAX_N);
    let sample: Vec<usize> = (0..vectors.len()).step_by(stride).collect();
    let sampled: Vec<Vec<f32>> = sample.iter().map(|&i| vectors[i].clone()).collect();
    let params = hdbscan::HdbscanHyperParams::builder()
        .min_cluster_size(HDBSCAN_MIN_CLUSTER)
        .build();
    let clusterer = hdbscan::Hdbscan::new(&sampled, params);
    let Ok(labels) = clusterer.cluster() else {
        return Vec::new();
    };
    let mut clusters: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, &l) in labels.iter().enumerate() {
        if l >= 0 {
            clusters.entry(l).or_default().push(sample[i]);
        }
    }
    if clusters.len() < 2 {
        return Vec::new();
    }
    let sample_set: HashSet<usize> = sample.iter().copied().collect();
    let mut out = Vec::new();
    for members in clusters.into_values() {
        if members.len() < MIN_SIDE {
            continue;
        }
        let member_set: HashSet<usize> = members.iter().copied().collect();
        // Dedup against the 2-means leaves: same structure, skip. Leaves
        // are restricted to the subsample so the Jaccard is comparable.
        let duplicate = leaves.iter().any(|leaf| {
            let leaf_set: HashSet<usize> = leaf
                .iter()
                .copied()
                .filter(|i| sample_set.contains(i))
                .collect();
            let inter = member_set.intersection(&leaf_set).count();
            let union = member_set.union(&leaf_set).count();
            union > 0 && inter as f64 / union as f64 > 0.5
        });
        if duplicate {
            continue;
        }
        let rest: Vec<usize> = labels
            .iter()
            .enumerate()
            .filter(|&(i, &l)| l >= 0 && !member_set.contains(&sample[i]))
            .map(|(i, _)| sample[i])
            .collect();
        if rest.len() < MIN_SIDE {
            continue;
        }
        let subset: Vec<usize> = rest.iter().chain(members.iter()).copied().collect();
        let sub_vecs: Vec<&Vec<f32>> = subset.iter().map(|&i| &vectors[i]).collect();
        let labels_bin: Vec<u8> = subset
            .iter()
            .map(|i| u8::from(member_set.contains(i)))
            .collect();
        let silhouette = cosine_silhouette(&sub_vecs, &labels_bin);
        out.push((members, silhouette));
    }
    out
}

/// Build a gram's full cluster proposal: geometric 2-means leaves plus
/// novel HDBSCAN clusters, exemplars margin-filtered against each cluster's
/// nearest neighbor cluster. None if no structure was found (fewer than two
/// clusters).
fn build_key_proposal(key: GramId, vectors: &[Vec<f32>]) -> Option<KeyProposal> {
    let all: Vec<usize> = (0..vectors.len()).collect();
    let mut leaves: Vec<Vec<usize>> = Vec::new();
    let root_silhouette = kmeans_leaves(vectors, all, 1, &mut leaves);
    let mut hdb = hdbscan_clusters(vectors, &leaves);
    hdb.sort_by(|a, b| b.1.total_cmp(&a.1));
    let hdbscan_silhouette = hdb.first().map(|&(_, s)| s);
    let mut clusters: Vec<(Vec<usize>, &'static str)> = Vec::new();
    if leaves.len() >= 2 {
        clusters.extend(leaves.into_iter().map(|l| (l, "kmeans")));
    } else if !hdb.is_empty() {
        // No 2-means structure: show the HDBSCAN clusters against the
        // complement of their union as background.
        let in_cluster: HashSet<usize> = hdb.iter().flat_map(|(m, _)| m.iter().copied()).collect();
        let rest: Vec<usize> = (0..vectors.len())
            .filter(|i| !in_cluster.contains(i))
            .collect();
        if rest.len() >= MIN_SIDE {
            clusters.push((rest, "hdbscan"));
        }
    }
    for (members, _) in hdb {
        if clusters.len() >= MAX_CLUSTERS {
            break;
        }
        clusters.push((members, "hdbscan"));
    }
    finish_proposal(key, vectors, clusters, root_silhouette, hdbscan_silhouette)
}

/// Turn member-index clusters into a [`KeyProposal`] with exemplars picked
/// against each cluster's nearest competitor. `None` with fewer than two
/// clusters — the exemplar picks are built around contrast.
fn finish_proposal(
    key: GramId,
    vectors: &[Vec<f32>],
    clusters: Vec<(Vec<usize>, &'static str)>,
    root_silhouette: Option<f64>,
    hdbscan_silhouette: Option<f64>,
) -> Option<KeyProposal> {
    if clusters.len() < 2 {
        return None;
    }
    let refs: Vec<&Vec<f32>> = vectors.iter().collect();
    let centroids: Vec<Vec<f32>> = clusters
        .iter()
        .map(|(indices, _)| {
            let members: Vec<&Vec<f32>> = indices.iter().map(|&i| &vectors[i]).collect();
            mean_normalized(&members)
        })
        .collect();
    let clusters = clusters
        .into_iter()
        .enumerate()
        .map(|(ci, (indices, _))| {
            let own = &centroids[ci];
            let other = centroids
                .iter()
                .enumerate()
                .filter(|&(cj, _)| cj != ci)
                .max_by(|a, b| dot(own, a.1).total_cmp(&dot(own, b.1)))
                .map(|(_, c)| c)
                .expect("at least two clusters");
            let exemplars = cluster_exemplars(&refs, &indices, own, other);
            Cluster { exemplars }
        })
        .collect();
    Some(KeyProposal {
        key,
        clusters,
        root_silhouette,
        hdbscan_silhouette,
    })
}

/// Proposal for a probe gram the geometry never surfaced: one unconditional
/// 2-means split, silhouette floor ignored. Even a meaningless split is a
/// fine sampler — the clusters only choose which lines the judge sees, and
/// the random-sample section always shows the item's true distribution.
fn build_forced_proposal(key: GramId, vectors: &[Vec<f32>]) -> Option<KeyProposal> {
    let refs: Vec<&Vec<f32>> = vectors.iter().collect();
    let (labels, _) = kmeans2(&refs);
    let mut sides: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for (i, &l) in labels.iter().enumerate() {
        sides[l as usize].push(i);
    }
    let [a, b] = sides;
    if a.is_empty() || b.is_empty() {
        return None;
    }
    finish_proposal(key, vectors, vec![(a, "forced"), (b, "forced")], None, None)
}

// ---------------------------------------------------------------------------
// LLM adjudication
// ---------------------------------------------------------------------------

/// What kind of pedagogical split a usage is. The definitions live in the
/// system prompt (a doc comment here would be emitted by schemars as a
/// `description` next to the enum's `$ref`, which OpenAI's structured-output
/// validator rejects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
enum UsageKind {
    Meaning,
    Construction,
    Formula,
}

impl UsageKind {
    fn as_str(self) -> &'static str {
        match self {
            UsageKind::Meaning => "meaning",
            UsageKind::Construction => "construction",
            UsageKind::Formula => "formula",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct UsageGroup {
    #[serde(rename = "1. kind")]
    kind: UsageKind,
    /// 2-5 word monolingual gloss of the usage, in the audited language.
    #[serde(rename = "2. gloss")]
    gloss: String,
    /// Numbers of every listed line that clearly shows this usage.
    #[serde(rename = "3. line_numbers")]
    line_numbers: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Opacity {
    Opaque,
    Semi,
    Transparent,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ExtractedExpression {
    /// Numbers of the listed lines that contain this expression.
    #[serde(rename = "1. line_numbers")]
    line_numbers: Vec<usize>,
    /// The expression copied verbatim (a contiguous substring) from one of
    /// the cited lines.
    #[serde(rename = "2. verbatim")]
    verbatim: String,
    /// 2-6 word gloss, in the language being mined.
    #[serde(rename = "3. gloss")]
    gloss: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AdjudicationResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// The item's usage inventory, with per-line labels.
    #[serde(rename = "2. usages")]
    usages: Vec<UsageGroup>,
    /// Fixed multiword expressions found in the lines, if any.
    #[serde(rename = "3. expressions")]
    expressions: Vec<ExtractedExpression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ProbeWord {
    /// The word exactly as it appears in text.
    #[serde(rename = "1. word")]
    word: String,
    /// One short line per usage: a few-word gloss plus a tiny example phrase.
    #[serde(rename = "2. usages")]
    usages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct PolysemyProbeResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// The polysemous words worth probing.
    #[serde(rename = "2. words")]
    words: Vec<ProbeWord>,
}

fn probe_system_prompt(language: Language) -> String {
    format!(
        "We are building a course for {language} learners, and we split each \
        vocabulary word into distinct \"usages\" so we can introduce them to the \
        learner one at a time. The test is pedagogical rather than \
        lexicographic: would a learner benefit from having these uses introduced \
        separately, or are they similar enough to teach as one thing?\n\
        \n\
        We find candidate words by clustering contextual embeddings of every \
        occurrence in our corpus, but that only surfaces words whose usages are \
        balanced enough to form visible clusters. A word whose second usage is \
        real but rarer slips right through — French \u{ab}quartier\u{bb} is \
        almost always 'neighborhood' in everyday speech, so the clustering never \
        notices \u{ab}pas de quartier !\u{bb}. Those are the words we would like \
        your help finding, since you can simply know them rather than having to \
        see them. Every word you name that occurs in our corpus gets its \
        occurrences looked at properly, so a suggestion that turns out to be \
        monosemous in practice costs little — err on the side of naming it.\n\
        \n\
        Please list {language} words with genuinely distinct usages a learner \
        would want introduced separately: a literal meaning plus an idiomatic or \
        figurative one, a noun that doubles as an interjection or form of \
        address, a grammar-conditioned use (politeness formula, discourse \
        marker, tag question), a second sense that dictionaries list but a \
        course would never think to teach.\n\
        \n\
        For each word give the surface form as it actually appears in text \
        (inflected is fine if that is the form the second usage lives in), and \
        one short line per usage: a few-word gloss plus a tiny example phrase in \
        {language}. Two or three usages is typical; only list usages you are \
        confident about. Aim for breadth — a hundred or more words would be \
        wonderful."
    )
}

/// Ask the probe model for polysemous words the geometric gate would miss.
/// One cached call per language.
async fn probe_words(language: Language) -> Result<Vec<ProbeWord>> {
    let response: PolysemyProbeResponse = PROBE_CLIENT
        .chat_with_system_prompt(
            probe_system_prompt(language),
            "Please list the words when ready!",
        )
        .await
        .context("polysemy probe failed")?;
    Ok(response.words)
}

fn adjudication_system_prompt(language: Language) -> String {
    format!(
        "You are auditing a language-learning vocabulary built from a {language} \
        sentence corpus. What we have done is taken all the known occurrences of a \
        certain vocabulary item (a word or phrase), embedded them with an embedding \
        model, then clustered them. The goal is to split the vocabulary item into \
        distinct usages, so that we can introduce each usage to learners \
        separately. Unfortunately, the embedding models are not perfect at this, \
        which is where you come in. We would like you to work out the item's \
        distinct usages, label the example lines with them, and report any fixed \
        expressions you notice along the way.\n\
        \n\
        In each request the target item is marked \u{ab}like this\u{bb} in its \
        sentence, and the lines are numbered across all clusters so you can cite \
        them. The clustering is imperfect in shape as well: one usage may be \
        split across several clusters, clusters may overlap — a small cluster \
        can carve a subset out of a larger one — and a large cluster is often \
        really a mixture of several usages, so please judge each line on its \
        own rather than assuming a cluster is uniform. The embedding features, \
        and therefore the resulting input clusters, often reflect superficial \
        features like sentence length, punctuation, or surrounding formality \
        rather than a difference in usage, which is one reason why merges can \
        be necessary. After the clusters there is a random sample drawn from \
        all occurrences; it shows the item's overall distribution, which the \
        clusters by themselves can misrepresent.\n\
        \n\
        Usages: Definitely report separate usages for distinct meanings — distinct \
        enough that a learner would treat them as different vocabulary items and \
        they would usually receive different translations in another language \
        (homonyms like bank=money/river, or strong polysemy like \
        paper=material/newspaper). Some of the input will show splits that only \
        vary slightly in topic, register, or inflection, but where the usage is \
        the same. Those lines should be labeled as one usage. And there is a \
        final category, where the meaning really is slightly different, but might \
        not ordinarily be seen as different. For example, in French it is common \
        to say \"non\" to literally mean \"no\", but at the end of a sentence \
        \"non ?\" can mean something like \"don't you think?\". Even though it's \
        natural, learners would probably still benefit from learning it and seeing \
        examples of it, because it's not totally obvious to a language learner \
        that \"non\" would be used this way. So that case also deserves its own \
        usage. On the other hand, you can merge if the usage seems so similar that \
        there's no real reason to teach them separately. For example, the word \
        \"ending\" in \"the road is ending\" vs \"the story is ending\" means \
        basically the same thing — we are reaching the end. These can be grouped, \
        as it is very unlikely that a learner would even benefit from learning \
        them separately.\n\
        \n\
        The overall test is pedagogical rather than lexicographic. Ask yourself: \
        if I were going to teach this word to someone learning the language, \
        would I find it helpful to introduce these ways of using it one at a \
        time, or are they similar enough that it makes more sense to teach them \
        together? A usage does not have to be a distinct dictionary sense — \
        grammatical patterns count too. French \"ils sont ...\" vs the inverted \
        question \"sont-ils ... ?\" is worth introducing separately even though \
        \"ils\" refers to the same people in both: a learner who has only met \
        one pattern will stumble on the other. The same goes for a word negating \
        a verb vs the same word standing alone as an answer. Your judgement in \
        deciding when to split into different usages and when to group \
        occurrences into one usage is appreciated.\n\
        \n\
        To help us organize the results, please tag each usage with a kind: \
        \"meaning\" when a different thing is meant (the bank and paper cases), \
        \"construction\" when it is the same core meaning appearing in a \
        distinct grammatical pattern (the sont-ils inversion, or verb negation \
        vs a standalone answer), and \"formula\" when the item is serving as a \
        conversational routine — a greeting, a term of address, a discourse \
        marker that moves the conversation along, an exclamation, or a \
        confirmation tag like the sentence-final \"non ?\". Don't agonize over \
        a borderline kind; the usages themselves matter more than their tags.\n\
        \n\
        For each usage, cite the numbers of all the lines that show it, \
        including lines from the random sample. We use your line labels as \
        anchors to automatically classify every other occurrence in the corpus, \
        so it matters that every cited line really does show its usage — when a \
        line is ambiguous between usages, garbled, or doesn't fit any usage, \
        the right thing to do is leave it uncited. Uncited lines are simply set \
        aside, so an incoherent cluster needs no special treatment: just don't \
        cite its lines. And if the whole item is really one usage, reporting a \
        single usage covering its lines is a perfectly good outcome — that \
        tells us not to split it.\n\
        \n\
        Expressions: The above directives refer to cases where a word individually \
        has multiple usages, but sometimes a word takes on a new meaning when \
        it's a component of a fixed expression. This can range from the opaque, \
        like the \"high\" in \"high school\", to the relatively transparent, like \
        \"god\" in \"god willing\". In these cases, it is true that \"high\" and \
        \"god\" mean something slightly different from usual, but the best way to \
        explain this to users is not to say that \"high\" has some sense relating \
        to schools or something, but to just say that \"high\" has usages like \
        \"vertically elevated\", \"inebriated\", and also participates in fixed \
        expressions like \"high school\". Even when a pattern is very \
        compositional and transparent, such as \"god willing\", it's still \
        beneficial to report it, because fluency requires that a language learner \
        already be familiar with such patterns, and we must be aware of them if \
        we are to explicitly teach them. One caveat: an expression needs at \
        least two actual words — a single word with a characteristic position \
        or punctuation, like the sentence-final \"non ?\", should be reported \
        as a usage of that word instead. A line where the item only appears as \
        part of a fixed expression should be cited on that expression rather \
        than on a usage, since it isn't really evidence of a separate usage of \
        the word.\n\
        \n\
        One practical note about reporting expressions. We can only use an \
        expression if we can find it in the corpus ourselves: we take exactly \
        what you write and look for it in the lines you cited, so the safest \
        thing is to copy the expression directly out of one of the numbered \
        lines, without tidying it up or converting it to a dictionary form — if \
        what you write doesn't appear in the cited line word-for-word, we won't \
        be able to find it, and the expression will unfortunately be lost. Along \
        with the expression itself, cite the numbers of the lines you saw it on. \
        If the same pattern appears in several inflected forms, one entry \
        covering the pattern is enough. And a pattern that only ever appears on \
        one line is too little evidence for us to do anything with, so it isn't \
        worth reporting.\n\
        \n\
        For the usages, please write the gloss in {language}. Feel free to put \
        some thoughts in the thoughts field \
        (in English) — that will help us understand your reasoning for your \
        decisions."
    )
}

/// «»-mark every member span of an occurrence in its sentence text.
fn mark_occurrence(occ: &Occurrence) -> String {
    let mut byte_spans: Vec<(usize, usize)> = Vec::new();
    let char_to_byte: Vec<usize> = occ
        .text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(occ.text.len()))
        .collect();
    for &(a, b) in &occ.spans {
        if let (Some(&ba), Some(&bb)) = (char_to_byte.get(a as usize), char_to_byte.get(b as usize))
        {
            byte_spans.push((ba, bb));
        }
    }
    byte_spans.sort();
    let mut out = String::new();
    let mut cursor = 0;
    for (a, b) in byte_spans {
        if a < cursor {
            continue;
        }
        out.push_str(&occ.text[cursor..a]);
        out.push('\u{ab}');
        out.push_str(&occ.text[a..b]);
        out.push('\u{bb}');
        cursor = b;
    }
    out.push_str(&occ.text[cursor..]);
    out
}

/// Sentinel "cluster" index for the random-sample section of the prompt.
const RANDOM_SECTION: usize = usize::MAX;

/// The per-gram prompt context: marked exemplar lines grouped by cluster,
/// then a deterministic stride over the entire cloud (the random-sample
/// section — cluster index [`RANDOM_SECTION`]), globally numbered so usage
/// labels and expression extractions can cite them.
struct KeyPrompt {
    display: String,
    /// (global line number, cluster index, occurrence index, marked line)
    lines: Vec<(usize, usize, usize, String)>,
}

fn key_prompt(prop: &KeyProposal, occs: &[Occurrence], display: String) -> KeyPrompt {
    let mut lines = Vec::new();
    let mut number = 0;
    for (cluster, c) in prop.clusters.iter().enumerate() {
        for &i in &c.exemplars {
            lines.push((number, cluster, i, mark_occurrence(&occs[i])));
            number += 1;
        }
    }
    let shown: HashSet<usize> = lines.iter().map(|(_, _, i, _)| *i).collect();
    let unshown: Vec<usize> = (0..occs.len()).filter(|i| !shown.contains(i)).collect();
    if !unshown.is_empty() {
        let stride = unshown.len().div_ceil(RANDOM_SAMPLE_LINES).max(1);
        for &i in unshown.iter().step_by(stride).take(RANDOM_SAMPLE_LINES) {
            lines.push((number, RANDOM_SECTION, i, mark_occurrence(&occs[i])));
            number += 1;
        }
    }
    KeyPrompt { display, lines }
}

fn adjudication_user_prompt(p: &KeyPrompt) -> String {
    let mut out = format!("Target: \"{}\"\n", p.display);
    let mut current = usize::MAX - 1;
    for (n, cluster, _, line) in &p.lines {
        if *cluster != current {
            current = *cluster;
            if current == RANDOM_SECTION {
                out.push_str("\nRandom sample (from all occurrences):\n");
            } else {
                let _ = write!(out, "\nCluster {}:\n", current + 1);
            }
        }
        let _ = writeln!(out, "{n}. {line}");
    }
    out
}

// ---------------------------------------------------------------------------
// Opacity judging (the adoption gate)
// ---------------------------------------------------------------------------
//
// A separate, per-expression call rather than a field on the adjudication
// response, deliberately: the adjudicator must report even compositional
// cluster-dominating patterns (that's what keeps them out of the usage
// inventory), so it can't also be the adoption bar — and opacity is the
// judgment we iterate on most, so it gets its own small prompt whose
// calibration can change without refilling the expensive per-gram cache.

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct OpacityJudgeResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    // No doc comment: schemars would emit it as a `description` next to the
    // enum's `$ref`, which OpenAI's structured-output validator rejects. The
    // opacity guidance lives in the system prompt instead.
    #[serde(rename = "2. opacity")]
    opacity: Opacity,
}

fn opacity_system_prompt(language: Language) -> String {
    format!(
        "You are curating multiword entries for a {language} vocabulary-learning \
        app. Each request shows a candidate multiword expression mined from a \
        sentence corpus, with a gloss and example sentences. Decide whether a \
        learner needs it as its own vocabulary item:\n\
        \n\
        - \"opaque\": the meaning is not derivable from the parts; it must be \
        learned as a unit (idioms — French \"il était une fois\", English \"once \
        upon a time\").\n\
        \n\
        - \"semi\": compositional in hindsight, but conventionalized — of all the \
        ways the idea could have been phrased, the language settled on this one, so \
        a learner benefits from learning it as a unit (French \"fait preuve de\", \
        \"adresse électronique\", \"me suis occupé de\"; English \"make a \
        decision\").\n\
        \n\
        - \"transparent\": the learner gets it for free from its parts, or it is \
        not a genuine unit at all. This includes: ordinary verb+object or \
        verb+reply combinations (French \"dire merci\", \"dit non\"; English \"say \
        thank you\" — knowing the words is knowing the phrase); free combinations \
        that merely recur; and truncated fragments that stop mid-phrase instead of \
        at a natural unit boundary (French \"reconnaissant pour votre\", \
        \"enchanté de vous\").\n\
        \n\
        These entries are adopted automatically — nobody reviews them — so when \
        unsure, choose \"transparent\": a wrongly admitted entry pollutes the \
        vocabulary; a wrongly dropped one costs nothing. Write your thoughts in \
        English."
    )
}

// ---------------------------------------------------------------------------
// Paradigm expansion
// ---------------------------------------------------------------------------
//
// Grounded extractions are copied verbatim out of a cited line, so they arrive
// frozen in whatever person, tense, or clitic that line happened to use. The
// frozen form is a poor citation ("me suis occupé de" where a dictionary says
// "s'occuper de") and a poor pattern: French clitics lemmatize to themselves,
// so a lemma pattern built from `me` can never match `te` or `se`, and the
// rest of the paradigm is simply invisible to the matcher.
//
// A third batched stage, separate from adjudication and opacity for the same
// reason those two are separate: it is the judgment most likely to be
// recalibrated, and its prompt must be able to change without refilling the
// expensive per-gram adjudication cache.

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ParadigmResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// Dictionary citation form of the phrase, or empty if the expression is
    /// already general.
    #[serde(rename = "2. citation")]
    citation: String,
    /// Complete surface forms of the phrase's other instantiations.
    #[serde(rename = "3. variants")]
    variants: Vec<String>,
}

fn paradigm_system_prompt(language: Language) -> String {
    format!(
        "You are curating multiword entries for a {language} vocabulary-learning \
        app. Each request shows one expression mined from a sentence corpus, with \
        its gloss and example sentences.\n\
        \n\
        The mining copies an expression verbatim out of whatever sentence it was \
        found in, so it often arrives frozen in one person, tense, or pronoun even \
        though the phrase itself is more general: French \"me suis occupé de\" for \
        what a dictionary would list as \"s'occuper de\", English \"God help me\" \
        for \"God help someone\". Taught as-is, the learner meets one arbitrary \
        instantiation and never sees that the others are the same phrase.\n\
        \n\
        Decide whether the expression you are shown is one instantiation of a more \
        general phrase.\n\
        \n\
        If it is, give the citation form a {language} dictionary would use as the \
        headword, and list the instantiations a learner would actually meet — the \
        other persons, the other clitics, the ordinary tenses. Write each one as a \
        complete surface form that could appear in a sentence rather than a \
        template with a placeholder in it: the forms are matched mechanically \
        against the corpus and any that does not occur there is dropped, so a \
        placeholder can never match anything. Include the expression you were \
        shown when it is itself one of the instantiations.\n\
        \n\
        If the expression is already general — nothing in it stands in for an open \
        slot or agrees with a subject — leave the citation empty and the variant \
        list empty. Most fixed expressions genuinely admit no variation, so an \
        empty answer is the common and expected one; inventing variation where \
        there is none fills the vocabulary with near-duplicates that each have to \
        be learned separately. Write your thoughts in English."
    )
}

fn paradigm_user_prompt(term: &DiscoveredTerm, examples: &[String]) -> String {
    let mut p = format!(
        "Expression: \"{}\"\nGloss: {}\n\nExamples:\n",
        term.term, term.gloss
    );
    for e in examples {
        let _ = writeln!(p, "- {e}");
    }
    p
}

// ---------------------------------------------------------------------------
// Grounding
// ---------------------------------------------------------------------------

/// A grounded, corpus-verified expression proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTerm {
    /// Surface form: the covered words as they appear in the cited sentence.
    pub term: String,
    /// Human-readable display of the gram this proposal would become.
    pub display: String,
    /// The proposed gram itself (its atom sequence — the canonical identity).
    pub gram: Gram<String>,
    /// The citation form this variant belongs to.
    ///
    /// The citation is an ordinary multiword term: it enters the inventory
    /// alongside the variant surfaces (`wiktionary_terms::extra_multiword_terms`),
    /// so it is tokenized, encodable, matchable, and taught like anything
    /// else — which is what makes it a legitimate vocabulary member for a
    /// match to point at. The variants are its paradigm, but they are related
    /// only loosely: each is its own vocabulary item, and any deeper
    /// relation (propagating a review across a paradigm, merging frequency)
    /// is deliberately deferred until practice shows it's needed.
    ///
    /// The rule the pipeline does enforce: when a variant matches in a
    /// sentence, the sentence's phrases list shows the citation form. Every
    /// variant is attempted separately — necessary, since clitics lemmatize
    /// to themselves, so a lemma pattern built from French "me" can never
    /// match "te" or "se" — and whichever fires, the match is rewritten to
    /// record the citation (see [`crate::pipeline::apply_citations`]).
    ///
    /// The citation's atoms are minted at discovery time and stored here;
    /// the terms-inventory copy is derived from this gram's display string.
    /// `None` for terms discovered before citations existed and for
    /// expressions the judge deemed already general.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<Gram<String>>,
    pub gloss: String,
    pub opacity: String,
    /// How many corpus sentences contain the atom sequence contiguously.
    pub count: usize,
    /// The mined gram whose cluster split surfaced it.
    pub source: String,
    pub silhouette: f64,
}

fn normalize_term(s: &str) -> String {
    s.to_lowercase().replace(['\u{2019}', '\u{02BC}'], "'")
}

/// Path of a language's committed discovery record.
fn discovered_terms_path(language: Language) -> std::path::PathBuf {
    Path::new("./generate-data/data")
        .join(language.code())
        .join("discovered_multiword_terms.jsonl")
}

/// Read a language's committed discovery record. Absent file → no
/// discoveries; a malformed line is an error rather than a skip, because this
/// file is meant to be reviewed and corrected by hand and a silently dropped
/// entry would un-adopt a term without saying so.
pub fn load_discovered_terms(language: Language) -> Result<Vec<DiscoveredTerm>> {
    let path = discovered_terms_path(language);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str::<DiscoveredTerm>(line)
                .with_context(|| format!("{}:{}: malformed discovered term", path.display(), i + 1))
        })
        .collect()
}

/// Snap a verbatim substring of `text` to word boundaries; returns the
/// (inclusive) word index range only if both ends align exactly with word
/// edges — a citation that starts or ends mid-word is rejected rather than
/// guessed at.
/// Strip the «» target markers the model may copy along with the words
/// (anywhere in the citation, not just its edges).
fn clean_verbatim(verbatim: &str) -> String {
    verbatim
        .replace(['\u{ab}', '\u{bb}'], "")
        .trim()
        .to_string()
}

fn snap_to_words(sent: &SentenceIndex, text: &str, verbatim: &str) -> Option<(usize, usize)> {
    let needle = clean_verbatim(verbatim);
    let needle = needle.as_str();
    if needle.is_empty() {
        return None;
    }
    let start = text.find(needle)?;
    let end = start + needle.len();
    let first = sent.words.iter().position(|w| w.byte_span.0 == start)?;
    let last = sent.words.iter().position(|w| w.byte_span.1 == end)?;
    (last >= first).then_some((first, last))
}

/// The structural constraints a proposed sequence must satisfy to become a
/// gram, matching what the unigram trainer requires of learned sequences:
/// more than one atom, real word tokens at both ends, no proper nouns
/// anywhere. Shared by the adjudicator's extractions and by paradigm variants
/// so the two can't drift apart.
fn admissible_sequence(ids: &[SpurAtom]) -> bool {
    use omnigram::unigram::UnigramToken;
    ids.len() >= 2
        && ids.first().is_some_and(|a| a.is_content())
        && ids.last().is_some_and(|a| a.is_content())
        && !ids.iter().any(|a| a.is_excluded_from_sequences())
}

/// Ground a free-standing surface form against the corpus: find a sentence
/// containing it on word boundaries and take the atom sequence it covers.
///
/// The adjudicator's extractions ground against the line they cite, but a
/// paradigm variant cites nothing — it is a form the judge believes occurs, so
/// the corpus is the only witness, and a form no sentence realizes is
/// ungroundable by definition. Sentences are visited in sorted order so the
/// witness (and therefore the recorded surface) is deterministic.
fn ground_surface(
    index: &HashMap<String, SentenceIndex>,
    surface: &str,
) -> Option<(Vec<SpurAtom>, String)> {
    let needle = surface.trim();
    if needle.is_empty() {
        return None;
    }
    let mut texts: Vec<&String> = index.keys().filter(|t| t.contains(needle)).collect();
    texts.sort_unstable();
    texts.into_iter().find_map(|text| {
        let sent = index.get(text)?;
        let (first, last) = snap_to_words(sent, text, needle)?;
        let ids = sent.atom_seq.get(first..=last)?.to_vec();
        let surface = text[sent.words[first].byte_span.0..sent.words[last].byte_span.1].to_string();
        Some((ids, surface))
    })
}

/// Corpus sentences whose atom sequence contains `needle` contiguously,
/// sorted for determinism (their count is the expression's corpus count;
/// the first few double as examples for the opacity judge).
fn atom_window_matches<'a>(
    index: &'a HashMap<String, SentenceIndex>,
    needle: &[SpurAtom],
) -> Vec<&'a str> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut texts: Vec<&str> = index
        .iter()
        .filter(|(_, s)| s.atom_seq.windows(needle.len()).any(|w| w == needle))
        .map(|(text, _)| text.as_str())
        .collect();
    texts.sort_unstable();
    texts
}

// ---------------------------------------------------------------------------
// Artifacts
// ---------------------------------------------------------------------------

/// One anchor occurrence of a usage: a corpus sentence and the char spans
/// of the gram's words in it. Anchors are stored as (sentence, spans), not
/// vectors, so centroids can be recomputed under any embedding version.
#[derive(Debug, Serialize, Deserialize)]
pub struct UsageAnchor {
    pub sentence: String,
    pub spans: Vec<(u32, u32)>,
    /// Labeled by the judge (true) or assigned by the centroid extension.
    pub gold: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageEntry {
    /// "meaning" | "construction" | "formula".
    pub kind: String,
    pub gloss: String,
    /// Judge-labeled lines that grounded this usage.
    pub n_gold: usize,
    /// Occurrences assigned to this usage by the centroid extension over
    /// the whole cloud (a true partition; includes the gold lines).
    pub n_assigned: usize,
    /// Leave-one-out validation: how many of the gold lines are assigned
    /// back to this usage when classified against centroids built without
    /// them. `loo_correct == n_gold` means the split is cleanly separable
    /// in embedding space.
    pub loo_correct: usize,
    pub anchors: Vec<UsageAnchor>,
}

/// A gram's usage inventory: the pedagogical units it splits into, with
/// enough labeled data to classify any occurrence of the gram.
#[derive(Debug, Serialize, Deserialize)]
pub struct UsageInventory {
    /// Human-readable display of the mined gram.
    pub key: String,
    /// The mined gram itself (canonical identity — display strings can
    /// collide, e.g. est@AUX vs est@VERB grams both display "est").
    pub gram: Gram<String>,
    /// Total occurrences in the mined cloud.
    pub n: usize,
    /// Occurrences absorbed by extracted-expression prototypes (the gram
    /// appearing inside a fixed expression) rather than any usage — the sum
    /// of the absorbers' `n_assigned`.
    pub n_expression: usize,
    pub usages: Vec<UsageEntry>,
    /// Expression-absorber classes (kind "expression"), persisted so that
    /// arbitrary-sentence classification can reproduce the corpus-time
    /// behavior: without them, an occurrence inside a fixed expression
    /// would be force-assigned to whichever usage is nearest.
    pub absorbers: Vec<UsageEntry>,
    pub silhouette: f64,
    pub source: String,
}

/// One per-occurrence corpus label, as written to
/// `out/{lang}/usage_labels.jsonl`.
#[derive(Debug, Serialize)]
struct UsageLabelRow<'a> {
    key: &'a str,
    /// 0-based line number of this gram's row in usage_inventories.jsonl —
    /// the unambiguous join key (display strings can collide across grams).
    inventory: usize,
    /// The assigned usage's gloss ("expression: {gloss}" for occurrences
    /// absorbed by an extracted-expression prototype).
    usage: &'a str,
    kind: &'a str,
    sentence: &'a str,
    spans: &'a [(u32, u32)],
    /// Cosine similarity to the assigned class centroid.
    sim: f32,
    /// Similarity margin over the strongest competing class (confidence
    /// proxy — downstream consumers can threshold on it; negative for a
    /// gold line the geometry disagrees with).
    margin: f32,
    /// Whether the judge labeled this occurrence directly.
    gold: bool,
}

/// A label computed during the judged loop, held until the inventory rows
/// are sorted so each label row can cite its inventory line number.
struct PendingLabel {
    usage: String,
    kind: &'static str,
    occ: usize,
    sim: f32,
    margin: f32,
    gold: bool,
}

/// Classify one occurrence vector against class centroids: returns
/// (best class index, similarity, margin over the runner-up). `None` if
/// fewer than two centroids (nothing to discriminate).
pub fn assign_to_centroids(v: &[f32], centroids: &[Vec<f32>]) -> Option<(usize, f32, f32)> {
    if centroids.len() < 2 {
        return None;
    }
    let mut sims: Vec<(usize, f32)> = centroids.iter().map(|c| dot(v, c)).enumerate().collect();
    sims.sort_by(|a, b| b.1.total_cmp(&a.1));
    Some((sims[0].0, sims[0].1, sims[0].1 - sims[1].1))
}

/// Where an occurrence lands when classified against an inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageAssignment {
    /// Index into [`UsageInventory::usages`].
    Usage(usize),
    /// Index into [`UsageInventory::absorbers`] — the occurrence is inside
    /// a fixed expression, not evidence of any usage.
    ExpressionMember(usize),
}

/// A usage inventory's classifier state: one centroid per usage and one per
/// expression absorber, recomputed from anchors under whatever embedding
/// version is current.
pub struct InventoryCentroids {
    pub usages: Vec<Vec<f32>>,
    pub absorbers: Vec<Vec<f32>>,
}

impl InventoryCentroids {
    /// Classify one occurrence vector: (assignment, similarity, margin over
    /// the strongest competitor). `None` if the inventory has fewer than
    /// two classes.
    pub fn classify(&self, v: &[f32]) -> Option<(UsageAssignment, f32, f32)> {
        let sims: Vec<f32> = self
            .usages
            .iter()
            .chain(self.absorbers.iter())
            .map(|c| dot(v, c))
            .collect();
        if sims.len() < 2 {
            return None;
        }
        let best = (0..sims.len())
            .max_by(|&a, &b| sims[a].total_cmp(&sims[b]))
            .expect("non-empty");
        let runner_up = (0..sims.len())
            .filter(|&i| i != best)
            .map(|i| sims[i])
            .fold(f32::NEG_INFINITY, f32::max);
        let assignment = if best < self.usages.len() {
            UsageAssignment::Usage(best)
        } else {
            UsageAssignment::ExpressionMember(best - self.usages.len())
        };
        Some((assignment, sims[best], sims[best] - runner_up))
    }
}

/// One class's centroid from its anchors' cached embeddings. Each anchor's
/// vector is the mean of its span vectors, matching how the mining built
/// occurrence vectors.
async fn entry_centroid(
    store: &osmo::Store,
    language: Language,
    key: &str,
    entry: &UsageEntry,
) -> Result<Vec<f32>> {
    let mut anchor_vectors: Vec<Vec<f32>> = Vec::new();
    for anchor in &entry.anchors {
        let Some(vecs) =
            token_embeddings::read_word_vectors(store, language, &anchor.sentence, &anchor.spans)
                .await
        else {
            continue;
        };
        let refs: Vec<&Vec<f32>> = vecs.iter().collect();
        anchor_vectors.push(mean_normalized(&refs));
    }
    anyhow::ensure!(
        !anchor_vectors.is_empty(),
        "no cached embeddings for any anchor of {key} usage {:?}",
        entry.gloss
    );
    if anchor_vectors.len() < entry.anchors.len() {
        // Tolerated (an embedding-version migration re-embeds lazily), but
        // worth a trace: a centroid from a subset of the serialized anchors
        // is not exactly the classifier the mining run validated.
        log::warn!(
            "usage centroid for {key} {:?} built from {}/{} anchors (missing embeddings)",
            entry.gloss,
            anchor_vectors.len(),
            entry.anchors.len()
        );
    }
    let refs: Vec<&Vec<f32>> = anchor_vectors.iter().collect();
    Ok(mean_normalized(&refs))
}

/// Recompute a usage inventory's classifier centroids from its anchors'
/// cached embeddings — the entry point for labeling the gram in arbitrary
/// sentences: fetch the target occurrence's vector (embedding the sentence
/// via the token-embeddings endpoint if it isn't cached), then
/// [`InventoryCentroids::classify`] against these.
pub async fn usage_centroids(
    store: &osmo::Store,
    language: Language,
    inventory: &UsageInventory,
) -> Result<InventoryCentroids> {
    let mut usages = Vec::with_capacity(inventory.usages.len());
    for entry in &inventory.usages {
        usages.push(entry_centroid(store, language, &inventory.key, entry).await?);
    }
    let mut absorbers = Vec::with_capacity(inventory.absorbers.len());
    for entry in &inventory.absorbers {
        absorbers.push(entry_centroid(store, language, &inventory.key, entry).await?);
    }
    Ok(InventoryCentroids { usages, absorbers })
}

/// Expand each grounded expression into its paradigm: ask the judge for the
/// citation form and the other instantiations, ground every proposed variant
/// against the corpus, append the ones that survive, and attach the citation
/// gram to every member of the phrase. Returns the paradigm membership —
/// member index to citation string — which stays valid even for members
/// whose citation string failed to tokenize into a gram, so the opacity
/// stage can still judge the family as one unit.
///
/// Variants go through exactly the gates an adjudicator extraction goes
/// through — admissible sequence, corpus count, novelty — because a variant is
/// a vocabulary proposal like any other; the judge's belief that a form exists
/// counts for nothing until the corpus shows it. A variant that is already a
/// discovered term is not duplicated; it just joins the paradigm.
///
/// Citation grams are minted here, through the same tokenizer that turns a
/// Wiktionary citation form into a gram, and this is their only derivation —
/// the citation never enters the multiword-terms list, so nothing else will
/// ever tokenize it (see [`DiscoveredTerm::citation`]).
async fn expand_paradigms(
    language: Language,
    strings: &lasso::RodeoReader,
    index: &HashMap<String, SentenceIndex>,
    known_terms: &HashSet<String>,
    known_multi: &HashSet<Vec<SpurAtom>>,
    discovered: &mut Vec<(DiscoveredTerm, Vec<String>)>,
) -> Result<BTreeMap<usize, String>> {
    if discovered.is_empty() {
        return Ok(BTreeMap::new());
    }
    let pb = indicatif::ProgressBar::new(discovered.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template(&format!(
                "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {} paradigm expansions ({{per_sec}}, ${{msg}}, {{eta}})",
                language.code()
            ))
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    let items: Vec<(usize, String)> = discovered
        .iter()
        .enumerate()
        .map(|(i, (d, examples))| (i, paradigm_user_prompt(d, examples)))
        .collect();
    let n_req = items.len();
    let results = JUDGE_CLIENT
        .batch_chat_with_system_prompt_fn::<_, _, ParadigmResponse>(
            paradigm_system_prompt(language),
            &items,
            |(_, p)| p.clone(),
            |batch| crate::report_batch_progress(&pb, 0, n_req, batch),
        )
        .await
        .context("paradigm batch failed")?;
    pb.finish_with_message(format!("{:.2}", JUDGE_CLIENT.cost().unwrap_or(0.0)));

    // Surface → index, so a proposed variant that is already a discovered term
    // joins its paradigm instead of being appended a second time.
    let mut by_surface: HashMap<String, usize> = discovered
        .iter()
        .enumerate()
        .map(|(i, (d, _))| (normalize_term(&d.term), i))
        .collect();
    // Member index → citation string, resolved to grams in one batch below.
    let mut membership: BTreeMap<usize, String> = BTreeMap::new();
    let (mut n_new, mut n_ungroundable, mut n_rejected) = (0usize, 0usize, 0usize);

    for ((parent, _), resp) in &results {
        let Ok(resp) = resp else { continue };
        let citation = resp.citation.trim().to_string();
        if citation.is_empty() {
            continue;
        }
        membership.insert(*parent, citation.clone());
        let (gloss, source, silhouette) = {
            let d = &discovered[*parent].0;
            (d.gloss.clone(), d.source.clone(), d.silhouette)
        };
        for variant in &resp.variants {
            if let Some(&i) = by_surface.get(&normalize_term(variant.trim())) {
                membership.insert(i, citation.clone());
                continue;
            }
            let Some((ids, surface)) = ground_surface(index, variant) else {
                n_ungroundable += 1;
                continue;
            };
            // Re-check after grounding: the corpus surface can differ from the
            // judge's spelling (case, elision) and land on a term we have.
            let key = normalize_term(&surface);
            if let Some(&i) = by_surface.get(&key) {
                membership.insert(i, citation.clone());
                continue;
            }
            if !admissible_sequence(&ids)
                || known_multi.contains(&ids)
                || known_terms.contains(&key)
            {
                n_rejected += 1;
                continue;
            }
            let matches = atom_window_matches(index, &ids);
            if matches.len() < MIN_EXPRESSION_COUNT {
                n_rejected += 1;
                continue;
            }
            let gram = Gram::from(
                ids.iter()
                    .map(|a| a.resolve(strings))
                    .collect::<Vec<Atom<String>>>(),
            );
            discovered.push((
                DiscoveredTerm {
                    term: surface,
                    display: gram.to_display_string(language),
                    gram,
                    citation: None,
                    gloss: gloss.clone(),
                    opacity: String::new(),
                    count: matches.len(),
                    source: source.clone(),
                    silhouette,
                },
                matches.iter().take(5).map(|s| s.to_string()).collect(),
            ));
            let i = discovered.len() - 1;
            by_surface.insert(key, i);
            membership.insert(i, citation.clone());
            n_new += 1;
        }
    }

    let citation_strings: BTreeSet<String> = membership.values().cloned().collect();
    println!(
        "usage-discovery[{}]: paradigms: {} citation forms, {n_new} new variants grounded, \
         {n_ungroundable} not attested in the corpus, {n_rejected} rejected by the term gates",
        language.code(),
        citation_strings.len(),
    );
    if citation_strings.is_empty() {
        return Ok(membership);
    }

    let tokenized = crate::nlp::process_sentences(
        citation_strings.into_iter().collect(),
        &Path::new("./out")
            .join(language.code())
            .join("discovered_citations_tokenization.jsonl"),
        language,
    )
    .await
    .context("Failed to tokenize discovered citation forms")?;
    let citation_grams: BTreeMap<String, Gram<String>> =
        crate::nlp::convert_tokens_to_literals(&tokenized, language)
            .iter()
            .filter_map(|(text, literals)| {
                let (atoms, _) = language_utils::literals_to_atoms(literals, language);
                // A citation that tokenizes to one atom is a word, not a
                // phrase — the same filter the multiword-terms list applies.
                (atoms.len() > 1).then(|| (text.clone(), Gram::from(atoms)))
            })
            .collect();
    let mut unresolved = 0usize;
    for (&i, citation) in &membership {
        match citation_grams.get(citation) {
            Some(gram) => discovered[i].0.citation = Some(gram.clone()),
            None => unresolved += 1,
        }
    }
    if unresolved > 0 {
        log::warn!(
            "usage-discovery[{}]: {unresolved} members left uncited (their citation form did not \
             tokenize to a phrase)",
            language.code()
        );
    }
    Ok(membership)
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

struct KeyMining {
    key: GramId,
    occs: Vec<Occurrence>,
    vectors: Vec<Vec<f32>>,
}

/// What `discover` does once the adjudication prompts are built.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiscoverMode {
    /// Run the judges and write the proposal files.
    Full,
    /// Print every adjudication prompt to stdout and stop — for iterating
    /// on the prompt against the judge's real inputs, without LLM spend.
    DumpPrompts,
    /// Run the adjudication batch (a pure tysm cache hit after a Full run)
    /// and print every raw response to stdout, then stop — for inspecting
    /// the judge's thoughts and line labels behind the artifacts.
    DumpResponses,
}

/// Discover usage splits and collocation candidates for one language and
/// write the proposal files under `generate-data/data/{lang}/`.
pub async fn discover(
    language: Language,
    corpus: &SegmentedCorpus,
    store: &osmo::Store,
    mode: DiscoverMode,
) -> Result<()> {
    let mut timer = crate::StageTimer::new();
    let arena = GramArena::from_vocabulary(&corpus.gram_vocabulary);

    // Index every sentence: decoded word spans, the gram segmentation
    // (aligned by construction in the encoded form), and interned atoms.
    let index: HashMap<String, SentenceIndex> = corpus
        .nlp_sentences
        .values()
        .map(|info| index_sentence(info, &corpus.interners, language))
        .collect();
    timer.lap("sentence indexing");

    let occurrences = collect_occurrences(corpus, language, &index, &arena);
    timer.lap("occurrence collection");
    println!(
        "usage-discovery[{}]: {} grams with >= {MIN_OCC} occurrences",
        language.code(),
        occurrences.len()
    );

    // Fetch vectors for every occurrence from the embedding cache; each
    // occurrence's vector is the mean of its covered heteronym words'
    // vectors. Occurrences without cached embeddings are dropped with a
    // count.
    let mut keys: Vec<KeyMining> = Vec::new();
    let mut missing = 0usize;
    {
        // One cache read per unique sentence, shared across keys.
        let mut needed: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
        for occ in occurrences.values().flatten() {
            needed
                .entry(occ.text.clone())
                .or_default()
                .extend(&occ.spans);
        }
        for spans in needed.values_mut() {
            spans.sort_unstable();
            spans.dedup();
        }
        let needed: Vec<(String, Vec<(u32, u32)>)> = needed.into_iter().collect();
        use futures::StreamExt;
        type SpanVectors = HashMap<(u32, u32), Vec<f32>>;
        let fetched: Vec<Option<SpanVectors>> =
            futures::stream::iter(needed.iter().map(|(text, spans)| async {
                let vecs =
                    token_embeddings::read_word_vectors(store, language, text, spans).await?;
                Some(spans.iter().copied().zip(vecs).collect())
            }))
            .buffered(256)
            .collect()
            .await;
        let by_text: HashMap<String, HashMap<(u32, u32), Vec<f32>>> = needed
            .iter()
            .zip(fetched)
            .filter_map(|((text, _), m)| m.map(|m| (text.clone(), m)))
            .collect();
        for (key, occs) in occurrences {
            let mut kept_occs = Vec::new();
            let mut vectors = Vec::new();
            for occ in occs {
                let Some(spans) = by_text.get(occ.text.as_str()) else {
                    missing += 1;
                    continue;
                };
                let member: Vec<&Vec<f32>> =
                    occ.spans.iter().filter_map(|s| spans.get(s)).collect();
                if member.len() != occ.spans.len() {
                    missing += 1;
                    continue;
                }
                vectors.push(mean_normalized(&member));
                kept_occs.push(occ);
            }
            if kept_occs.len() >= MIN_OCC {
                keys.push(KeyMining {
                    key,
                    occs: kept_occs,
                    vectors,
                });
            }
        }
    }
    if missing > 0 {
        log::warn!(
            "usage-discovery[{}]: {missing} occurrences lacked cached embeddings and were skipped",
            language.code()
        );
    }
    timer.lap("vector fetch");

    // Cluster every key's occurrence cloud: recursive 2-means leaves plus
    // novel HDBSCAN clusters on large clouds — purely geometric, tuned to
    // over-propose (the adjudicator merges and arbitrates). CPU-bound and
    // per-key independent, so mine keys in parallel.
    use rayon::prelude::*;
    let mut proposals: Vec<KeyProposal> = keys
        .par_iter()
        .filter_map(|km| build_key_proposal(km.key, &km.vectors))
        .collect();
    timer.lap("mining (2-means + hdbscan)");

    // The silhouette gate only surfaces grams whose usages are balanced
    // enough to cluster; a common word with a rare second usage (French
    // "quartier": overwhelmingly 'neighborhood', occasionally 'mercy')
    // never shows up geometrically. The probe list attacks that blind spot
    // from the other side: a model that simply knows the language names
    // polysemous words worth checking, and every probe word present in the
    // corpus gets judged regardless of geometry. The judge labels lines
    // either way, so a dud probe just comes back as a single usage.
    let probe_ids: HashSet<GramId> = {
        let mut by_display: HashMap<String, Vec<GramId>> = HashMap::new();
        for km in &keys {
            by_display
                .entry(arena.display(km.key, language).to_lowercase())
                .or_default()
                .push(km.key);
        }
        let words = probe_words(language).await?;
        let mut ids = HashSet::new();
        let mut matched = 0usize;
        for w in &words {
            if let Some(list) = by_display.get(&w.word.to_lowercase()) {
                matched += 1;
                ids.extend(list.iter().copied());
            }
        }
        println!(
            "usage-discovery[{}]: polysemy probe: {} words suggested, {matched} present in the corpus",
            language.code(),
            words.len(),
        );
        ids
    };
    {
        let have: HashSet<GramId> = proposals.iter().map(|p| p.key).collect();
        let by_key: HashMap<GramId, &KeyMining> = keys.iter().map(|km| (km.key, km)).collect();
        proposals.extend(
            probe_ids
                .iter()
                .filter(|id| !have.contains(id))
                .filter_map(|id| build_forced_proposal(*id, &by_key[id].vectors)),
        );
    }

    // Pick which grams to adjudicate: top keys by root 2-means silhouette,
    // top keys surfaced by HDBSCAN, plus every probe word.
    let judged: Vec<&KeyProposal> = {
        let top = |score: fn(&KeyProposal) -> Option<f64>, limit: usize| -> HashSet<GramId> {
            let mut scored: Vec<(GramId, f64)> = proposals
                .iter()
                .filter_map(|p| score(p).map(|s| (p.key, s)))
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            scored.into_iter().take(limit).map(|(key, _)| key).collect()
        };
        let kmeans_top = top(|p| p.root_silhouette, JUDGE_TOP);
        let hdbscan_top = top(|p| p.hdbscan_silhouette, HDBSCAN_TOP);
        proposals
            .iter()
            .filter(|p| {
                kmeans_top.contains(&p.key)
                    || hdbscan_top.contains(&p.key)
                    || probe_ids.contains(&p.key)
            })
            .collect()
    };
    println!(
        "usage-discovery[{}]: adjudicating {} grams",
        language.code(),
        judged.len()
    );

    let key_lookup: HashMap<GramId, usize> =
        keys.iter().enumerate().map(|(i, km)| (km.key, i)).collect();

    // Known multiword vocabulary grams as Tok-atom sequences, for the
    // novelty check: a proposal whose atom sequence is already a gram isn't
    // a discovery. (Adopted terms the trainer pruned from the vocabulary
    // are still caught by the surface check against the terms txt.)
    let known_multi: HashSet<Vec<SpurAtom>> = arena
        .grams
        .iter()
        .filter_map(|g| {
            g.iter()
                .filter(|a| matches!(a, Atom::Tok(_)))
                .map(|a| a.get_interned(&corpus.interners.strings))
                .collect::<Option<Vec<SpurAtom>>>()
        })
        .filter(|seq| seq.len() > 1)
        .collect();

    // One adjudication call per gram: the judge sees every candidate
    // cluster's exemplars plus a random sample, defines the usage
    // inventory, and labels each line with its usage or host expression.
    let prompts: Vec<KeyPrompt> = judged
        .iter()
        .map(|p| {
            key_prompt(
                p,
                &keys[key_lookup[&p.key]].occs,
                arena.display(p.key, language),
            )
        })
        .collect();

    timer.lap("prompt building");
    if mode == DiscoverMode::DumpPrompts {
        println!("──── system prompt ────");
        println!("{}", adjudication_system_prompt(language));
        for prompt in &prompts {
            println!("──── {} ────", prompt.display);
            println!("{}", adjudication_user_prompt(prompt));
        }
        return Ok(());
    }

    let pb = indicatif::ProgressBar::new(prompts.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template(&format!(
                "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {} adjudications ({{per_sec}}, ${{msg}}, {{eta}})",
                language.code()
            ))
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    let n_req = prompts.len();
    let results = JUDGE_CLIENT
        .batch_chat_with_system_prompt_fn::<_, _, AdjudicationResponse>(
            adjudication_system_prompt(language),
            &prompts,
            adjudication_user_prompt,
            |batch| crate::report_batch_progress(&pb, 0, n_req, batch),
        )
        .await
        .context("adjudication batch failed")?;
    pb.finish_with_message(format!("{:.2}", JUDGE_CLIENT.cost().unwrap_or(0.0)));

    if mode == DiscoverMode::DumpResponses {
        for (idx, prompt) in prompts.iter().enumerate() {
            let (_, resp) = &results[idx];
            println!("──── {} ────", prompt.display);
            match resp {
                Ok(resp) => println!("{}", serde_json::to_string_pretty(resp)?),
                Err(e) => println!("(failed: {e})"),
            }
        }
        return Ok(());
    }

    // One entry per inventoried gram: the inventory row, the gram's index
    // into `keys` (to reach occurrence texts at write time), and the
    // per-occurrence labels — held until the rows are sorted so each label
    // can cite its final inventory line number.
    let mut inventory_rows: Vec<(UsageInventory, usize, Vec<PendingLabel>)> = Vec::new();
    // Aggregate leave-one-out and judge-conflict tallies for the summary.
    let (mut loo_total, mut loo_ok, mut n_conflict) = (0usize, 0usize, 0usize);
    // Grounded expressions, each with a few example sentences for the
    // opacity judge. Opacity itself is filled by that later stage.
    let mut discovered: BTreeMap<Vec<SpurAtom>, (DiscoveredTerm, Vec<String>)> = BTreeMap::new();
    // Per-reason rejection tallies for the grounding gate, so a language
    // where extractions die wholesale (Korean's near-empty term file) shows
    // where instead of failing silently.
    let (mut n_ungroundable, mut n_single, mut n_boundary, mut n_infrequent, mut n_grounded) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for (idx, prop) in judged.iter().enumerate() {
        let (_, resp) = &results[idx];
        let Ok(resp) = resp else { continue };
        let km = &keys[key_lookup[&prop.key]];
        let prompt = &prompts[idx];
        let silhouette = prop
            .root_silhouette
            .or(prop.hdbscan_silhouette)
            .unwrap_or(0.0);

        // Expression lane: ground every extraction against the cited lines
        // (word range -> atom sequence, the substrate a new gram would be
        // made of), then against the whole corpus.
        for e in &resp.expressions {
            let Some((ids, surface)) = e.line_numbers.iter().find_map(|&n| {
                let (_, _, occ_idx, _) = prompt.lines.iter().find(|(ln, _, _, _)| *ln == n)?;
                let occ = &km.occs[*occ_idx];
                let sent = index.get(&occ.text)?;
                let (first, last) = snap_to_words(sent, &occ.text, &e.verbatim)?;
                let ids = sent.atom_seq.get(first..=last)?.to_vec();
                let surface = occ.text[sent.words[first].byte_span.0..sent.words[last].byte_span.1]
                    .to_string();
                Some((ids, surface))
            }) else {
                n_ungroundable += 1;
                log::info!(
                    "usage-discovery[{}]: ungroundable extraction {:?} for {}",
                    language.code(),
                    e.verbatim,
                    prompt.display,
                );
                continue;
            };
            if ids.len() < 2 {
                n_single += 1;
                continue;
            }
            if !admissible_sequence(&ids) {
                n_boundary += 1;
                continue;
            }
            let matches = atom_window_matches(&index, &ids);
            let count = matches.len();
            if count < MIN_EXPRESSION_COUNT {
                n_infrequent += 1;
                continue;
            }
            n_grounded += 1;
            discovered.entry(ids.clone()).or_insert_with(|| {
                let gram = Gram::from(
                    ids.iter()
                        .map(|a| a.resolve(&corpus.interners.strings))
                        .collect::<Vec<Atom<String>>>(),
                );
                let examples = matches.iter().take(5).map(|s| s.to_string()).collect();
                (
                    DiscoveredTerm {
                        term: surface,
                        display: gram.to_display_string(language),
                        gram,
                        // Minted by the paradigm-expansion stage, which does
                        // not exist yet; the format and the map that consumes
                        // it land first.
                        citation: None,
                        gloss: e.gloss.clone(),
                        opacity: String::new(),
                        count,
                        source: prompt.display.clone(),
                        silhouette,
                    },
                    examples,
                )
            });
        }

        // Usage lane: resolve the judge's per-line labels to occurrence
        // indices and build one prototype class per usage, plus one
        // absorber class per extracted expression (so occurrences of e.g.
        // "au clair de lune" are claimed by the expression rather than
        // force-assigned to a usage of "clair"). A line cited by more than
        // one class is a judge conflict: drop it from all of them — it
        // would poison both centroids.
        let resolve = |n: usize| {
            prompt
                .lines
                .iter()
                .find(|(ln, _, _, _)| *ln == n)
                .map(|(_, _, occ, _)| *occ)
        };
        struct ProtoClass<'r> {
            /// `None` marks an extracted-expression absorber.
            kind: Option<UsageKind>,
            gloss: &'r str,
            gold: Vec<usize>,
        }
        let gold_lines = |numbers: &[usize]| {
            let mut gold: Vec<usize> = numbers.iter().filter_map(|&n| resolve(n)).collect();
            gold.sort_unstable();
            gold.dedup();
            gold
        };
        let mut classes: Vec<ProtoClass> = resp
            .usages
            .iter()
            .map(|u| ProtoClass {
                kind: Some(u.kind),
                gloss: &u.gloss,
                gold: gold_lines(&u.line_numbers),
            })
            .chain(resp.expressions.iter().map(|e| {
                // An absorber's citations are verified individually: the
                // expression must ground on the cited line as a two-plus
                // word window on word boundaries that contains the target
                // occurrence — a mistakenly cited line (or an expression
                // matching elsewhere in the sentence) would poison the
                // absorber centroid and siphon unrelated occurrences.
                let gold: Vec<usize> = gold_lines(&e.line_numbers)
                    .into_iter()
                    .filter(|&i| {
                        let occ = &km.occs[i];
                        let Some(sent) = index.get(&occ.text) else {
                            return false;
                        };
                        let Some((first, last)) = snap_to_words(sent, &occ.text, &e.verbatim)
                        else {
                            return false;
                        };
                        if last <= first {
                            return false;
                        }
                        let lo = sent.words[first].char_span.0;
                        let hi = sent.words[last].char_span.1;
                        occ.spans.iter().all(|&(a, b)| lo <= a && b <= hi)
                    })
                    .collect();
                ProtoClass {
                    kind: None,
                    gloss: &e.gloss,
                    gold,
                }
            }))
            .collect();
        let mut claims: HashMap<usize, usize> = HashMap::new();
        for c in &classes {
            for &i in &c.gold {
                *claims.entry(i).or_default() += 1;
            }
        }
        let contested: HashSet<usize> = claims
            .iter()
            .filter(|&(_, &n)| n > 1)
            .map(|(&i, _)| i)
            .collect();
        n_conflict += contested.len();
        for c in &mut classes {
            c.gold.retain(|i| !contested.contains(i));
        }
        classes.retain(|c| c.gold.len() >= MIN_GOLD);
        if classes.iter().filter(|c| c.kind.is_some()).count() < 2 {
            continue;
        }

        // Two-pass centroid extension. Pass 1: gold-only centroids
        // provisionally classify the cloud, which is how each class's
        // spread anchors get picked. Pass 2: final centroids are computed
        // from exactly the anchor sets that get persisted, and those
        // produce the labels and counts — so the classifier we ship
        // (anchors → [`usage_centroids`] → [`InventoryCentroids::classify`])
        // is the classifier that produced them, not a near miss. Gold
        // lines keep the judge's label in both passes.
        let gold_class: HashMap<usize, usize> = classes
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| c.gold.iter().map(move |&i| (i, ci)))
            .collect();
        let classify_all = |centroids: &[Vec<f32>]| -> Vec<(usize, f32, f32)> {
            km.vectors
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let (best, sim, margin) =
                        assign_to_centroids(v, centroids).expect("two or more classes");
                    match gold_class.get(&i) {
                        Some(&ci) => {
                            // The judge's label wins; the margin is measured
                            // from the assigned class to its strongest
                            // competitor, so a gold line the geometry
                            // disagrees with correctly shows a negative
                            // margin.
                            let own = dot(v, &centroids[ci]);
                            let best_other = centroids
                                .iter()
                                .enumerate()
                                .filter(|&(cj, _)| cj != ci)
                                .map(|(_, c)| dot(v, c))
                                .fold(f32::NEG_INFINITY, f32::max);
                            (ci, own, own - best_other)
                        }
                        None => (best, sim, margin),
                    }
                })
                .collect()
        };
        let gold_centroids: Vec<Vec<f32>> = classes
            .iter()
            .map(|c| {
                let members: Vec<&Vec<f32>> = c.gold.iter().map(|&i| &km.vectors[i]).collect();
                mean_normalized(&members)
            })
            .collect();
        let provisional = classify_all(&gold_centroids);
        let mut prov_members: Vec<Vec<usize>> = vec![Vec::new(); classes.len()];
        for (i, &(ci, _, _)) in provisional.iter().enumerate() {
            prov_members[ci].push(i);
        }

        // Anchor selection: every gold line first, then provisionally
        // assigned occurrences strided across the class's similarity range
        // (not just the most central), so the persisted anchors span the
        // class.
        let anchor_sets: Vec<Vec<(usize, bool)>> = classes
            .iter()
            .enumerate()
            .map(|(ci, class)| {
                let gold_set: HashSet<usize> = class.gold.iter().copied().collect();
                let mut anchor_idx: Vec<(usize, bool)> =
                    class.gold.iter().map(|&i| (i, true)).collect();
                let mut rest: Vec<usize> = prov_members[ci]
                    .iter()
                    .copied()
                    .filter(|i| !gold_set.contains(i))
                    .collect();
                rest.sort_by(|&a, &b| provisional[b].1.total_cmp(&provisional[a].1));
                if anchor_idx.len() < MAX_ANCHORS && !rest.is_empty() {
                    let want = MAX_ANCHORS - anchor_idx.len();
                    let stride = rest.len().div_ceil(want).max(1);
                    anchor_idx.extend(rest.iter().step_by(stride).take(want).map(|&i| (i, false)));
                }
                anchor_idx.truncate(MAX_ANCHORS);
                anchor_idx
            })
            .collect();
        let centroids: Vec<Vec<f32>> = anchor_sets
            .iter()
            .map(|set| {
                let vecs: Vec<&Vec<f32>> = set.iter().map(|&(i, _)| &km.vectors[i]).collect();
                mean_normalized(&vecs)
            })
            .collect();
        let assignments = classify_all(&centroids);
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); classes.len()];
        for (i, &(ci, _, _)) in assignments.iter().enumerate() {
            members[ci].push(i);
        }

        // Leave-one-out over the gold lines, entirely in gold-only
        // geometry: withhold each gold line from its own gold centroid and
        // check it still classifies home against the other classes' gold
        // centroids. This is the honesty meter for the judge's labels
        // themselves — gold that can't be recovered geometrically means
        // the usage distinction is real to the judge but invisible to the
        // embedding, so assignment can't be trusted.
        let loo: Vec<usize> = classes
            .iter()
            .enumerate()
            .map(|(ci, c)| {
                c.gold
                    .iter()
                    .filter(|&&g| {
                        let rest: Vec<&Vec<f32>> = c
                            .gold
                            .iter()
                            .filter(|&&i| i != g)
                            .map(|&i| &km.vectors[i])
                            .collect();
                        let held = mean_normalized(&rest);
                        let own = dot(&km.vectors[g], &held);
                        gold_centroids
                            .iter()
                            .enumerate()
                            .filter(|&(cj, _)| cj != ci)
                            .all(|(_, other)| own >= dot(&km.vectors[g], other))
                    })
                    .count()
            })
            .collect();

        let mut usages: Vec<UsageEntry> = Vec::new();
        let mut absorbers: Vec<UsageEntry> = Vec::new();
        let mut n_expression = 0usize;
        for (ci, class) in classes.iter().enumerate() {
            let anchors: Vec<UsageAnchor> = anchor_sets[ci]
                .iter()
                .map(|&(i, gold)| UsageAnchor {
                    sentence: km.occs[i].text.clone(),
                    spans: km.occs[i].spans.clone(),
                    gold,
                })
                .collect();
            let entry = |kind: &str| UsageEntry {
                kind: kind.to_string(),
                gloss: class.gloss.to_string(),
                n_gold: class.gold.len(),
                n_assigned: members[ci].len(),
                loo_correct: loo[ci],
                anchors,
            };
            match class.kind {
                Some(kind) => {
                    loo_total += class.gold.len();
                    loo_ok += loo[ci];
                    usages.push(entry(kind.as_str()));
                }
                None => {
                    n_expression += members[ci].len();
                    absorbers.push(entry("expression"));
                }
            }
        }

        let pending: Vec<PendingLabel> = assignments
            .iter()
            .enumerate()
            .map(|(i, &(ci, sim, margin))| {
                let class = &classes[ci];
                let (kind, usage) = match class.kind {
                    Some(k) => (k.as_str(), class.gloss.to_string()),
                    None => ("expression", format!("expression: {}", class.gloss)),
                };
                PendingLabel {
                    usage,
                    kind,
                    occ: i,
                    sim,
                    margin,
                    gold: gold_class.contains_key(&i),
                }
            })
            .collect();
        inventory_rows.push((
            UsageInventory {
                key: prompt.display.clone(),
                gram: arena.grams[prop.key as usize].clone(),
                n: km.occs.len(),
                n_expression,
                usages,
                absorbers,
                silhouette,
                source: match (
                    prop.root_silhouette.is_some(),
                    prop.hdbscan_silhouette.is_some(),
                ) {
                    (true, true) => "kmeans+hdbscan",
                    (true, false) => "kmeans",
                    _ => "hdbscan",
                }
                .to_string(),
            },
            key_lookup[&prop.key],
            pending,
        ));
    }
    println!(
        "usage-discovery[{}]: expression grounding: {n_grounded} grounded, \
         {n_ungroundable} ungroundable, {n_single} single-word, \
         {n_boundary} boundary-rejected, {n_infrequent} below count {MIN_EXPRESSION_COUNT}",
        language.code(),
    );
    inventory_rows.sort_by(|a, b| {
        b.0.silhouette
            .total_cmp(&a.0.silhouette)
            .then(a.0.key.cmp(&b.0.key))
    });
    if loo_total > 0 {
        println!(
            "usage-discovery[{}]: gold labels: {loo_ok}/{loo_total} recovered by leave-one-out \
             centroid assignment, {n_conflict} lines dropped as multi-class conflicts",
            language.code(),
        );
    }

    // Novelty filter for the multiword proposals: drop expressions the
    // pipeline already knows — by normalized surface against the
    // multiword-terms inventory, or because the concatenated gram sequence
    // is itself already a gram.
    let data_dir = format!("./generate-data/data/{}", language.code());
    let known_terms: HashSet<String> = {
        let path = Path::new("./out")
            .join(language.code())
            .join("target_language_multiword_terms.txt");
        std::fs::read_to_string(&path)
            .map(|s| s.lines().map(normalize_term).collect())
            .unwrap_or_default()
    };
    let mut discovered: Vec<(DiscoveredTerm, Vec<String>)> = discovered
        .into_iter()
        .filter(|(ids, (d, _))| {
            !known_terms.contains(&normalize_term(&d.term)) && !known_multi.contains(ids)
        })
        .map(|(_, entry)| entry)
        .collect();

    // Paradigm expansion, after the novelty filter so it only runs on terms
    // that are actually going to be proposed, and before opacity so the
    // adoption gate judges whole phrases rather than individual surfaces.
    let paradigm_membership = expand_paradigms(
        language,
        &corpus.interners.strings,
        &index,
        &known_terms,
        &known_multi,
        &mut discovered,
    )
    .await?;

    // Opacity judging: the adoption gate. These files feed the next
    // generate-data run with no human review, so only expressions judged
    // worth learning as a unit (opaque or semi) are written; transparent
    // ones — and ones whose judgment failed — are dropped.
    //
    // One judgment per paradigm, not per surface form: the instantiations of
    // a phrase are one vocabulary decision, so judging them separately costs
    // N times as much and can leave a paradigm half-adopted, which is worse
    // than either adopting or dropping it whole.
    // Grouped by the judge's citation *string* rather than the resolved
    // citation gram: the string exists even when the citation form failed to
    // tokenize into a gram (a transient lexide failure), and grouping on it
    // keeps such a family from fragmenting into per-surface judgments with
    // potentially inconsistent verdicts. Citation-less terms fall back to
    // their own gram — not the display string, so two phrases that render
    // identically (est@AUX vs est@VERB) don't share a verdict.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum ParadigmKey {
        Citation(String),
        Solo(Gram<String>),
    }
    let paradigms: Vec<(String, Vec<usize>)> = {
        let mut groups: BTreeMap<ParadigmKey, Vec<usize>> = BTreeMap::new();
        for (i, (d, _)) in discovered.iter().enumerate() {
            let key = match paradigm_membership.get(&i) {
                Some(citation) => ParadigmKey::Citation(citation.clone()),
                None => ParadigmKey::Solo(d.gram.clone()),
            };
            groups.entry(key).or_default().push(i);
        }
        groups
            .into_iter()
            .map(|(key, members)| {
                // Prompt heading: the resolved citation gram's display when it
                // exists (byte-identical to the old prompt, preserving the
                // judgment cache), else the citation string itself.
                let display = match &key {
                    ParadigmKey::Citation(c) => discovered[members[0]]
                        .0
                        .citation
                        .as_ref()
                        .map_or_else(|| c.clone(), |g| g.to_display_string(language)),
                    ParadigmKey::Solo(g) => g.to_display_string(language),
                };
                (display, members)
            })
            .collect()
    };
    if !discovered.is_empty() {
        let pb = indicatif::ProgressBar::new(paradigms.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template(&format!(
                    "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {} opacity judgments ({{per_sec}}, ${{msg}}, {{eta}})",
                    language.code()
                ))
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        let items: Vec<(usize, String)> = paradigms
            .iter()
            .enumerate()
            .map(|(g, (display, members))| {
                let head = &discovered[members[0]].0;
                let mut p = format!("Expression: \"{display}\"\nGloss: {}\n", head.gloss);
                if members.len() > 1 {
                    let forms: Vec<&str> = members
                        .iter()
                        .map(|&i| discovered[i].0.term.as_str())
                        .collect();
                    let _ = writeln!(p, "Attested forms: {}", forms.join(", "));
                }
                p.push_str("\nExamples:\n");
                // Round-robin one example per member before taking a second
                // from any, so a paradigm is illustrated by its whole range
                // rather than by whichever form happens to be first.
                for round in 0..EXAMPLES_PER_PARADIGM {
                    for &i in members {
                        if let Some(e) = discovered[i].1.get(round) {
                            let _ = writeln!(p, "- {e}");
                        }
                    }
                }
                (g, p)
            })
            .collect();
        let n_req = items.len();
        let opacity_results = JUDGE_CLIENT
            .batch_chat_with_system_prompt_fn::<_, _, OpacityJudgeResponse>(
                opacity_system_prompt(language),
                &items,
                |(_, p)| p.clone(),
                |batch| crate::report_batch_progress(&pb, 0, n_req, batch),
            )
            .await
            .context("opacity batch failed")?;
        pb.finish_with_message(format!("{:.2}", JUDGE_CLIENT.cost().unwrap_or(0.0)));
        for ((g, _), resp) in opacity_results {
            if let Ok(r) = resp {
                let verdict = format!("{:?}", r.opacity).to_lowercase();
                for &i in &paradigms[*g].1 {
                    discovered[i].0.opacity = verdict.clone();
                }
            }
        }
    }
    let discovered: Vec<DiscoveredTerm> = discovered
        .into_iter()
        .map(|(d, _)| d)
        .filter(|d| matches!(d.opacity.as_str(), "opaque" | "semi"))
        .collect();

    std::fs::create_dir_all(&data_dir).context("Failed to create data dir")?;
    let inventory_path = Path::new(&data_dir).join("usage_inventories.jsonl");
    let mut f =
        File::create(&inventory_path).context("Failed to create usage_inventories.jsonl")?;
    for (row, _, _) in &inventory_rows {
        writeln!(f, "{}", serde_json::to_string(row)?)?;
    }
    // The usage inventory replaces the old cluster-grouped sense file
    // wholesale (nothing ever consumed it); leaving the stale file behind
    // would look like data.
    let legacy = Path::new(&data_dir).join("sense_candidates.jsonl");
    if legacy.exists() {
        std::fs::remove_file(&legacy).context("Failed to remove legacy sense_candidates.jsonl")?;
    }
    println!(
        "usage-discovery[{}]: wrote {} usage inventories to {}",
        language.code(),
        inventory_rows.len(),
        inventory_path.display()
    );

    // Per-occurrence labels for the whole mined corpus: one row per
    // occurrence of every inventoried gram — the demonstration (and
    // inspection surface) of the assignment capability.
    let labels_path = Path::new("./out")
        .join(language.code())
        .join("usage_labels.jsonl");
    if let Some(parent) = labels_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create out dir")?;
    }
    let mut f = File::create(&labels_path).context("Failed to create usage_labels.jsonl")?;
    let mut n_labels = 0usize;
    for (inv_idx, (inv, key_idx, pending)) in inventory_rows.iter().enumerate() {
        let km = &keys[*key_idx];
        for l in pending {
            writeln!(
                f,
                "{}",
                serde_json::to_string(&UsageLabelRow {
                    key: &inv.key,
                    inventory: inv_idx,
                    usage: &l.usage,
                    kind: l.kind,
                    sentence: &km.occs[l.occ].text,
                    spans: &km.occs[l.occ].spans,
                    sim: l.sim,
                    margin: l.margin,
                    gold: l.gold,
                })?
            )?;
            n_labels += 1;
        }
    }
    println!(
        "usage-discovery[{}]: wrote {n_labels} per-occurrence labels to {}",
        language.code(),
        labels_path.display()
    );

    // The jsonl is the durable adoption record: entries from previous runs
    // are preserved — once adopted, a term is in the merged multiword-terms
    // list, so the novelty filter (correctly) refuses to re-discover it, and
    // dropping a line here would silently un-adopt it — and only genuinely
    // new discoveries are appended. Metadata, though, is healed: when this
    // run re-derived an already-adopted term (the caches replay extractions
    // until the term enters the inventory), a citation the original run
    // failed to mint is backfilled, and opacity is re-aligned with the fresh
    // per-paradigm verdict so a family can't stay half opaque, half semi.
    let terms_path = Path::new(&data_dir).join("discovered_multiword_terms.jsonl");
    let fresh_by_term: HashMap<String, &DiscoveredTerm> = discovered
        .iter()
        .map(|d| (normalize_term(&d.term), d))
        .collect();
    let mut lines: Vec<String> = Vec::new();
    let mut adopted: HashSet<String> = HashSet::new();
    let mut n_backfilled = 0usize;
    if let Ok(content) = std::fs::read_to_string(&terms_path) {
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let mut line = line.to_string();
            if let Ok(mut row) = serde_json::from_str::<DiscoveredTerm>(&line) {
                let key = normalize_term(&row.term);
                if let Some(fresh) = fresh_by_term.get(&key) {
                    let heal_citation = row.citation.is_none() && fresh.citation.is_some();
                    let heal_opacity = !fresh.opacity.is_empty() && row.opacity != fresh.opacity;
                    if heal_citation {
                        row.citation = fresh.citation.clone();
                    }
                    if heal_opacity {
                        row.opacity = fresh.opacity.clone();
                    }
                    if heal_citation || heal_opacity {
                        line = serde_json::to_string(&row)?;
                        n_backfilled += 1;
                    }
                }
                adopted.insert(key);
            } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                && let Some(term) = v["term"].as_str()
            {
                adopted.insert(normalize_term(term));
            }
            lines.push(line);
        }
    }
    if n_backfilled > 0 {
        println!(
            "usage-discovery[{}]: backfilled citation/opacity on {n_backfilled} previously \
             adopted terms",
            language.code(),
        );
    }
    let mut new_terms = 0usize;
    for row in &discovered {
        if !adopted.insert(normalize_term(&row.term)) {
            continue;
        }
        lines.push(serde_json::to_string(row)?);
        new_terms += 1;
    }
    let mut f =
        File::create(&terms_path).context("Failed to create discovered_multiword_terms.jsonl")?;
    for line in &lines {
        writeln!(f, "{line}")?;
    }
    println!(
        "usage-discovery[{}]: wrote {} discovered multiword terms ({new_terms} new) to {}",
        language.code(),
        lines.len(),
        terms_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec2(x: f32, y: f32) -> Vec<f32> {
        let mut v = vec![x, y];
        normalize(&mut v);
        v
    }

    #[test]
    fn kmeans_separates_two_blobs() {
        let data: Vec<Vec<f32>> = (0..10)
            .map(|i| {
                if i < 6 {
                    vec2(1.0, 0.01 * i as f32)
                } else {
                    vec2(0.01 * i as f32, 1.0)
                }
            })
            .collect();
        let refs: Vec<&Vec<f32>> = data.iter().collect();
        let (labels, _) = kmeans2(&refs);
        assert_eq!(labels[..6], [0, 0, 0, 0, 0, 0]);
        assert_eq!(labels[6..], [1, 1, 1, 1]);
        let sil = cosine_silhouette(&refs, &labels);
        assert!(sil > 0.5, "silhouette {sil} too low for clean blobs");
    }

    #[test]
    fn leaves_split_two_blobs_but_not_one() {
        let two: Vec<Vec<f32>> = (0..12)
            .map(|i| {
                if i < 6 {
                    vec2(1.0, 0.01 * i as f32)
                } else {
                    vec2(0.01 * i as f32, 1.0)
                }
            })
            .collect();
        let mut leaves = Vec::new();
        let sil = kmeans_leaves(&two, (0..12).collect(), 1, &mut leaves);
        assert!(sil.is_some());
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0], (0..6).collect::<Vec<_>>());
        assert_eq!(leaves[1], (6..12).collect::<Vec<_>>());

        // Identical points can't be split (every point lands on one side).
        // A *near*-identical 1-D line is no good as the negative case here:
        // cosine distance grows with angle², which inflates silhouette on
        // degenerate synthetic data past the floor.
        let one: Vec<Vec<f32>> = (0..12).map(|_| vec2(1.0, 0.5)).collect();
        let mut leaves = Vec::new();
        let sil = kmeans_leaves(&one, (0..12).collect(), 1, &mut leaves);
        assert!(sil.is_none(), "identical points must not split");
        assert_eq!(leaves.len(), 1);
    }

    /// A record as committed before citations existed, verbatim from
    /// `data/fra/discovered_multiword_terms.jsonl`. The adoption log is
    /// append-only, so every future read of it has to keep parsing lines in
    /// this shape.
    const LEGACY_RECORD: &str = r#"{"term":"dit vrai","display":"dit vrai","gram":[{"Tok":{"text":"dit","word_type":{"type":"Heteronym","word":"dit","lemma":"dire","pos":"VERB"}}},{"Tok":{"text":"vrai","word_type":{"type":"Heteronym","word":"vrai","lemma":"vrai","pos":"ADV"}}}],"gloss":"dire la vérité","opacity":"semi","count":6,"source":"vrai","silhouette":0.5770227295363336}"#;

    #[test]
    fn citationless_record_round_trips() {
        let parsed: DiscoveredTerm = serde_json::from_str(LEGACY_RECORD).unwrap();
        assert_eq!(parsed.term, "dit vrai");
        assert_eq!(parsed.gram.iter().count(), 2);
        assert!(parsed.citation.is_none());
        // `skip_serializing_if` keeps a citationless entry byte-identical, so
        // adding the field doesn't churn every committed line on the next
        // write.
        assert_eq!(serde_json::to_string(&parsed).unwrap(), LEGACY_RECORD);
    }

    #[test]
    fn citation_round_trips_and_is_the_only_added_field() {
        let mut parsed: DiscoveredTerm = serde_json::from_str(LEGACY_RECORD).unwrap();
        parsed.citation = Some(parsed.gram.clone());
        let json = serde_json::to_string(&parsed).unwrap();
        let back: DiscoveredTerm = serde_json::from_str(&json).unwrap();
        assert_eq!(back.citation.as_ref(), Some(&parsed.gram));
        // The citation is a full gram, not a display string, so two phrases
        // that render alike stay distinct in the file as well as in the map.
        assert!(json.contains("\"citation\":[{\"Tok\""));
    }

    #[test]
    fn admissible_sequence_rejects_lone_and_unbounded_atoms() {
        let parsed: DiscoveredTerm = serde_json::from_str(LEGACY_RECORD).unwrap();
        let atoms: Vec<Atom<String>> = parsed.gram.iter().cloned().collect();
        let mut interner = lasso::Rodeo::default();
        let interned: Vec<SpurAtom> = atoms
            .iter()
            .map(|a| a.get_or_intern(&mut interner))
            .collect();
        assert!(admissible_sequence(&interned));
        assert!(!admissible_sequence(&interned[..1]));
        assert!(!admissible_sequence(&[]));
    }

    #[test]
    fn silhouette_low_for_one_blob() {
        let data: Vec<Vec<f32>> = (0..12).map(|i| vec2(1.0, 0.001 * i as f32)).collect();
        let refs: Vec<&Vec<f32>> = data.iter().collect();
        let (labels, _) = kmeans2(&refs);
        let sil = cosine_silhouette(&refs, &labels);
        assert!(sil < 0.9, "one blob should not look cleanly separable");
    }
}
