//! Sense & collocation discovery over cached token embeddings.
//!
//! Mines every gram's occurrence cloud for cluster structure (recursive
//! 2-means plus HDBSCAN on large clouds — both purely geometric, tuned to
//! over-propose), then has an LLM adjudicate each gram's full set of
//! candidate clusters in a single call: group them into genuinely distinct
//! senses (merging over-split ones), or explain them as the gram occurring
//! inside a larger fixed multiword expression. The arbitration matters:
//! contextual embeddings give a word inside a fixed expression a tight,
//! distinctive cluster, so geometry alone cannot tell polysemy from
//! collocation membership — only a judge that sees both interpretations at
//! once can (e.g. "au clair" splitting into "au clair de lune" vs "tirer au
//! clair" has zero senses of its own, just two host expressions). Writes
//! files under `generate-data/data/{lang}/` that feed the next
//! generate-data run directly — there is no human review step, so the
//! judges are the quality bar:
//!
//! - `sense_candidates.jsonl`: grams whose clusters group into two or more
//!   distinct senses, in sense-inventory shape (gloss + anchor sentences
//!   per sense).
//! - `discovered_multiword_terms.jsonl`: grounded, novel, frequent
//!   expressions (the LLM only cites lines and copies verbatim substrings;
//!   surface forms, gram sequences, and corpus counts are derived here)
//!   that a final per-expression opacity judgment deemed worth learning as
//!   a unit — the adjudicator reports even compositional cluster-dominating
//!   patterns (to keep them out of the sense inventory), and the opacity
//!   stage is what keeps "dire merci"-class free combinations out of the
//!   vocabulary.
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
//! Run by the standalone `sense_discovery` binary; nothing in the main
//! generate-data pipeline consumes these files except
//! `wiktionary_terms::extra_multiword_terms`, which folds the discovered
//! terms into the next run's multiword-term inventory as-is.

use anyhow::{Context, Result};
use language_utils::{Atom, Gram, Language, SentenceInfo, WordType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
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
/// Exemplars shown to the adjudicator per cluster.
const EXEMPLARS: usize = 8;
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

/// Judgment-tier model, same reasoning as slot grading: few hundred calls
/// per language, and each one decides what enters the review files.
static JUDGE_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-terra"));

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

/// One candidate cluster of a gram's occurrence cloud.
struct Cluster {
    /// Indices into the key's occurrence list.
    indices: Vec<usize>,
    /// Exemplar occurrence indices: nearest own-centroid, margin filtered
    /// against the nearest other cluster (an occurrence near-tied between
    /// centroids is exactly the noise HDBSCAN would have excluded — skip it
    /// as an exemplar).
    exemplars: Vec<usize>,
    source: &'static str,
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

fn margin_filtered_exemplars(
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
        .take(EXEMPLARS)
        .collect();
    if confident.len() >= EXEMPLARS.min(side.len()).min(3) {
        confident
    } else {
        // Degenerate margins (tiny side): fall back to nearest-to-centroid.
        ranked.iter().map(|(i, _, _)| *i).take(EXEMPLARS).collect()
    }
}

/// Recursively 2-means-split `subset` into leaves while the geometry
/// supports it: silhouette over the floor, both sides over the absolute
/// minimum, depth and leaf count in bounds. No judgment gates recursion —
/// over-splitting is safe because the adjudicator merges same-sense leaves.
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
        .map(|(ci, (indices, source))| {
            let own = &centroids[ci];
            let other = centroids
                .iter()
                .enumerate()
                .filter(|&(cj, _)| cj != ci)
                .max_by(|a, b| dot(own, a.1).total_cmp(&dot(own, b.1)))
                .map(|(_, c)| c)
                .expect("at least two clusters");
            let exemplars = margin_filtered_exemplars(&refs, &indices, own, other);
            Cluster {
                indices,
                exemplars,
                source,
            }
        })
        .collect();
    Some(KeyProposal {
        key,
        clusters,
        root_silhouette,
        hdbscan_silhouette,
    })
}

// ---------------------------------------------------------------------------
// LLM adjudication
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SenseGroup {
    /// Numbers of the clusters that share this sense.
    #[serde(rename = "1. cluster_numbers")]
    cluster_numbers: Vec<usize>,
    /// 2-5 word monolingual gloss of the sense, in the audited language.
    #[serde(rename = "2. gloss")]
    gloss: String,
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
    /// Numbers of the clusters dominated by this expression (empty if it
    /// merely appears on some lines without explaining a whole cluster).
    #[serde(rename = "1. cluster_numbers")]
    cluster_numbers: Vec<usize>,
    /// Numbers of the listed lines that contain this expression.
    #[serde(rename = "2. line_numbers")]
    line_numbers: Vec<usize>,
    /// The expression copied verbatim (a contiguous substring) from one of
    /// the cited lines.
    #[serde(rename = "3. verbatim")]
    verbatim: String,
    /// 2-6 word gloss, in the language being mined.
    #[serde(rename = "4. gloss")]
    gloss: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AdjudicationResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// The clusters grouped into genuinely distinct senses.
    #[serde(rename = "2. senses")]
    senses: Vec<SenseGroup>,
    /// Fixed multiword expressions found in the clusters, if any.
    #[serde(rename = "3. expressions")]
    expressions: Vec<ExtractedExpression>,
    /// Numbers of the clusters that are incoherent mixtures or noise.
    #[serde(rename = "4. noise")]
    noise: Vec<usize>,
}

fn adjudication_system_prompt(language: Language) -> String {
    format!(
        "You are auditing a language-learning vocabulary built from a {language} \
        sentence corpus. What we have done is taken all the known occurrences of a \
        certain vocabulary item (a word or phrase), embedded them with an embedding \
        model, then clustered them. The goal is to split the vocabulary item into \
        distinct meanings, so that we can teach each meaning separately. \
        Unfortunately, the embedding models are not perfect at this, which is where \
        you come in. We would like you to sort the clusters into \"senses\", \
        \"expressions\", and \"noise\".\n\
        \n\
        In each request the target item is marked \u{ab}like this\u{bb} in its \
        sentence, and the lines are numbered across all clusters so you can cite \
        them. The clustering is imperfect in shape as well: one meaning may be \
        split across several clusters, and clusters may overlap — a small cluster \
        can carve a subset out of a larger one. The embedding features, and \
        therefore the resulting input clusters, often reflect superficial \
        features like sentence length, punctuation, or surrounding formality \
        rather than a difference in meaning, which is one reason why merges can \
        be necessary.\n\
        \n\
        Senses: Definitely report separate groups for distinct meanings — distinct \
        enough that a learner would treat them as different vocabulary items and \
        they would usually receive different translations in another language \
        (homonyms like bank=money/river, or strong polysemy like \
        paper=material/newspaper). Some of the input will show splits that only \
        vary slightly in topic, register, or inflection, but where the meaning is \
        the same. Such clusters should be merged into one group. And there is a \
        final category, where the meaning really is slightly different, but might \
        not ordinarily be seen as different. For example, in French it is common \
        to say \"non\" to literally mean \"no\", but at the end of a sentence \
        \"non ?\" can mean something like \"don't you think?\". Even though it's \
        natural, learners would probably still benefit from learning it and seeing \
        examples of it, because it's not totally obvious to a language learner \
        that \"non\" would be used this way. So that case also deserves its own \
        sense. On the other hand, you can merge if the usage seems so similar that \
        there's no real reason to teach them separately. For example, the word \
        \"ending\" in \"the road is ending\" vs \"the story is ending\" means \
        basically the same thing — we are reaching the end. These can be grouped, \
        as it is very unlikely that a learner would even benefit from learning \
        them separately. Your judgement in deciding when to split into different \
        senses and when to group occurrences into one sense is appreciated.\n\
        \n\
        Expressions: The above directives refer to cases where a word individually \
        has multiple meanings, but sometimes a word takes on a new meaning when \
        it's a component of a fixed expression. This can range from the opaque, \
        like the \"high\" in \"high school\", to the relatively transparent, like \
        \"god\" in \"god willing\". In these cases, it is true that \"high\" and \
        \"god\" mean something slightly different from usual, but the best way to \
        explain this to users is not to say that \"high\" has some sense relating \
        to schools or something, but to just say that \"high\" has senses like \
        \"vertically elevated\", \"inebriated\", and also participates in fixed \
        expressions like \"high school\". Even when a pattern is very \
        compositional and transparent, such as \"god willing\", it's still \
        beneficial to report it, because fluency requires that a language learner \
        already be familiar with such patterns, and we must be aware of them if \
        we are to explicitly teach them. One caveat: an expression needs at \
        least two actual words — a single word with a characteristic position \
        or punctuation, like the sentence-final \"non ?\", should be reported \
        as a sense of that word instead.\n\
        \n\
        Noise: Some clusters are unfortunately incoherent. They might be a \
        mixture of many different senses, or just noise. Please put those in the \
        noise category so that the cluster is not taught to the user as a \
        coherent concept.\n\
        \n\
        One practical note about reporting expressions. We can only use an \
        expression if we can find it in the corpus ourselves: we take exactly \
        what you write and look for it in the lines you cited, so the safest \
        thing is to copy the expression directly out of one of the numbered \
        lines, without tidying it up or converting it to a dictionary form — if \
        what you write doesn't appear in the cited line word-for-word, we won't \
        be able to find it, and the expression will unfortunately be lost. Along \
        with the expression itself, cite the numbers of the lines you saw it on. \
        Sometimes an expression is the real explanation for an entire cluster; \
        when that happens, put that cluster's number in the expression's \
        cluster_numbers and leave the cluster out of the senses, since it isn't \
        really evidence of a separate sense of the word. Other times an \
        expression just shows up on a few lines here and there — report it too, \
        with empty cluster_numbers. If the same pattern appears in several \
        inflected forms, one entry covering the pattern is enough. And a pattern \
        that only ever appears on one line is too little evidence for us to do \
        anything with, so it isn't worth reporting.\n\
        \n\
        For the senses, please write the gloss in {language}. Feel free to put \
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

/// The per-gram prompt context: marked exemplar lines grouped by cluster,
/// globally numbered so expression extractions can cite them.
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
    KeyPrompt { display, lines }
}

fn adjudication_user_prompt(p: &KeyPrompt) -> String {
    let mut out = format!("Target: \"{}\"\n", p.display);
    let mut current = usize::MAX;
    for (n, cluster, _, line) in &p.lines {
        if *cluster != current {
            current = *cluster;
            let _ = write!(out, "\nCluster {}:\n", current + 1);
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
// cluster-dominating patterns (that's what keeps them out of the sense
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
// Grounding
// ---------------------------------------------------------------------------

/// A grounded, corpus-verified expression proposal.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredTerm {
    /// Surface form: the covered words as they appear in the cited sentence.
    pub term: String,
    /// Human-readable display of the gram this proposal would become.
    pub display: String,
    /// The proposed gram itself (its atom sequence — the canonical identity).
    pub gram: Gram<String>,
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

/// Snap a verbatim substring of `text` to word boundaries; returns the
/// (inclusive) word index range only if both ends align exactly with word
/// edges — a citation that starts or ends mid-word is rejected rather than
/// guessed at.
fn snap_to_words(sent: &SentenceIndex, text: &str, verbatim: &str) -> Option<(usize, usize)> {
    // The model may copy our «» markers along with the words (anywhere in
    // the citation, not just its edges); strip them.
    let needle = verbatim.replace(['\u{ab}', '\u{bb}'], "");
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let start = text.find(needle)?;
    let end = start + needle.len();
    let first = sent.words.iter().position(|w| w.byte_span.0 == start)?;
    let last = sent.words.iter().position(|w| w.byte_span.1 == end)?;
    (last >= first).then_some((first, last))
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

#[derive(Debug, Serialize)]
pub struct SenseAnchor {
    pub sentence: String,
    pub spans: Vec<(u32, u32)>,
}

#[derive(Debug, Serialize)]
pub struct SenseEntry {
    pub gloss: String,
    pub n: usize,
    pub anchors: Vec<SenseAnchor>,
}

#[derive(Debug, Serialize)]
pub struct SenseCandidate {
    /// Human-readable display of the mined gram.
    pub key: String,
    /// The mined gram itself (canonical identity — display strings can
    /// collide, e.g. est@AUX vs est@VERB grams both display "est").
    pub gram: Gram<String>,
    pub n: usize,
    pub senses: Vec<SenseEntry>,
    pub silhouette: f64,
    pub source: String,
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
    /// the judge's thoughts and noise assignments behind the artifacts.
    DumpResponses,
}

/// Discover sense splits and collocation candidates for one language and
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
        "sense-discovery[{}]: {} grams with >= {MIN_OCC} occurrences",
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
            "sense-discovery[{}]: {missing} occurrences lacked cached embeddings and were skipped",
            language.code()
        );
    }
    timer.lap("vector fetch");

    // Cluster every key's occurrence cloud: recursive 2-means leaves plus
    // novel HDBSCAN clusters on large clouds — purely geometric, tuned to
    // over-propose (the adjudicator merges and arbitrates). CPU-bound and
    // per-key independent, so mine keys in parallel.
    use rayon::prelude::*;
    let proposals: Vec<KeyProposal> = keys
        .par_iter()
        .filter_map(|km| build_key_proposal(km.key, &km.vectors))
        .collect();
    timer.lap("mining (2-means + hdbscan)");

    // Pick which grams to adjudicate: top keys by root 2-means silhouette,
    // plus top keys surfaced by HDBSCAN.
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
            .filter(|p| kmeans_top.contains(&p.key) || hdbscan_top.contains(&p.key))
            .collect()
    };
    println!(
        "sense-discovery[{}]: adjudicating {} grams",
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
    // cluster at once and sorts them into sense groups (merging over-split
    // ones) and fixed-expression fragments.
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

    let mut sense_rows: Vec<SenseCandidate> = Vec::new();
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
                    "sense-discovery[{}]: ungroundable extraction {:?} for {}",
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
            // A proposal must satisfy the same constraints the unigram
            // trainer puts on learned sequences: content words at both
            // ends, no proper nouns anywhere.
            {
                use omnigram::unigram::UnigramToken;
                let boundary_ok = ids.first().is_some_and(|a| a.is_content())
                    && ids.last().is_some_and(|a| a.is_content());
                if !boundary_ok || ids.iter().any(|a| a.is_excluded_from_sequences()) {
                    n_boundary += 1;
                    continue;
                }
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

        // Sense lane: resolve each group's clusters (1-based numbers from
        // the prompt); a gram with two or more surviving groups becomes a
        // sense candidate. Clusters explained by a host expression or
        // judged noise simply appear in no group.
        let groups: Vec<(&SenseGroup, Vec<&Cluster>)> = resp
            .senses
            .iter()
            .filter_map(|g| {
                let members: Vec<&Cluster> = g
                    .cluster_numbers
                    .iter()
                    .filter_map(|&n| n.checked_sub(1).and_then(|i| prop.clusters.get(i)))
                    .collect();
                (!members.is_empty()).then_some((g, members))
            })
            .collect();
        if groups.len() < 2 {
            continue;
        }
        let mut sources: Vec<&str> = groups
            .iter()
            .flat_map(|(_, cs)| cs.iter().map(|c| c.source))
            .collect();
        sources.sort_unstable();
        sources.dedup();
        let senses: Vec<SenseEntry> = groups
            .iter()
            .map(|(g, members)| {
                let mut indices: Vec<usize> = members
                    .iter()
                    .flat_map(|c| c.indices.iter().copied())
                    .collect();
                indices.sort_unstable();
                indices.dedup();
                // Round-robin the anchors across member clusters so a
                // merged group is represented by all of its clusters, not
                // just the first (anchors feed the future nearest-centroid
                // assignment pass).
                let mut picked: Vec<usize> = Vec::new();
                let mut seen: HashSet<usize> = HashSet::new();
                let mut cluster_exemplars: Vec<_> = members
                    .iter()
                    .map(|c| c.exemplars.iter().copied())
                    .collect();
                'fill: loop {
                    let mut progressed = false;
                    for exemplars in &mut cluster_exemplars {
                        for i in exemplars.by_ref() {
                            if seen.insert(i) {
                                picked.push(i);
                                progressed = true;
                                if picked.len() == EXEMPLARS {
                                    break 'fill;
                                }
                                break;
                            }
                        }
                    }
                    if !progressed {
                        break;
                    }
                }
                let anchors: Vec<SenseAnchor> = picked
                    .into_iter()
                    .map(|i| SenseAnchor {
                        sentence: km.occs[i].text.clone(),
                        spans: km.occs[i].spans.clone(),
                    })
                    .collect();
                SenseEntry {
                    gloss: g.gloss.clone(),
                    n: indices.len(),
                    anchors,
                }
            })
            .collect();
        sense_rows.push(SenseCandidate {
            key: prompt.display.clone(),
            gram: arena.grams[prop.key as usize].clone(),
            n: km.occs.len(),
            senses,
            silhouette,
            source: sources.join("+"),
        });
    }
    println!(
        "sense-discovery[{}]: expression grounding: {n_grounded} grounded, \
         {n_ungroundable} ungroundable, {n_single} single-word, \
         {n_boundary} boundary-rejected, {n_infrequent} below count {MIN_EXPRESSION_COUNT}",
        language.code(),
    );
    sense_rows.sort_by(|a, b| {
        b.silhouette
            .total_cmp(&a.silhouette)
            .then(a.key.cmp(&b.key))
    });

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

    // Opacity judging: the adoption gate. These files feed the next
    // generate-data run with no human review, so only expressions judged
    // worth learning as a unit (opaque or semi) are written; transparent
    // ones — and ones whose judgment failed — are dropped.
    if !discovered.is_empty() {
        let pb = indicatif::ProgressBar::new(discovered.len() as u64);
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
        let items: Vec<(usize, String)> = discovered
            .iter()
            .enumerate()
            .map(|(i, (d, examples))| {
                let mut p = format!(
                    "Expression: \"{}\"\nGloss: {}\n\nExamples:\n",
                    d.term, d.gloss
                );
                for e in examples {
                    let _ = writeln!(p, "- {e}");
                }
                (i, p)
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
        for ((i, _), resp) in opacity_results {
            if let Ok(r) = resp {
                discovered[*i].0.opacity = format!("{:?}", r.opacity).to_lowercase();
            }
        }
    }
    let discovered: Vec<DiscoveredTerm> = discovered
        .into_iter()
        .map(|(d, _)| d)
        .filter(|d| matches!(d.opacity.as_str(), "opaque" | "semi"))
        .collect();

    std::fs::create_dir_all(&data_dir).context("Failed to create data dir")?;
    let sense_path = Path::new(&data_dir).join("sense_candidates.jsonl");
    let mut f = File::create(&sense_path).context("Failed to create sense_candidates.jsonl")?;
    for row in &sense_rows {
        writeln!(f, "{}", serde_json::to_string(row)?)?;
    }
    println!(
        "sense-discovery[{}]: wrote {} sense candidates to {}",
        language.code(),
        sense_rows.len(),
        sense_path.display()
    );

    // The jsonl is the durable adoption record: entries from previous runs
    // are preserved verbatim — once adopted, a term is in the merged
    // multiword-terms list, so the novelty filter (correctly) refuses to
    // re-discover it, and a plain rewrite here would silently un-adopt it —
    // and only genuinely new discoveries are appended.
    let terms_path = Path::new(&data_dir).join("discovered_multiword_terms.jsonl");
    let mut lines: Vec<String> = Vec::new();
    let mut adopted: HashSet<String> = HashSet::new();
    if let Ok(content) = std::fs::read_to_string(&terms_path) {
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(term) = v["term"].as_str()
            {
                adopted.insert(normalize_term(term));
            }
            lines.push(line.to_string());
        }
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
        "sense-discovery[{}]: wrote {} discovered multiword terms ({new_terms} new) to {}",
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

    #[test]
    fn silhouette_low_for_one_blob() {
        let data: Vec<Vec<f32>> = (0..12).map(|i| vec2(1.0, 0.001 * i as f32)).collect();
        let refs: Vec<&Vec<f32>> = data.iter().collect();
        let (labels, _) = kmeans2(&refs);
        let sil = cosine_silhouette(&refs, &labels);
        assert!(sil < 0.9, "one blob should not look cleanly separable");
    }
}
