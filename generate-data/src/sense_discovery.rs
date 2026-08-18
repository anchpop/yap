//! Sense & collocation discovery over cached token embeddings.
//!
//! Mines every gram's occurrence cloud for cluster structure, has an LLM
//! adjudicate each proposed split through two lanes — "are these genuinely
//! distinct senses?" and "is a cluster driven by a fixed multiword
//! expression?" — and writes reviewable proposal files under
//! `generate-data/data/{lang}/`:
//!
//! - `sense_candidates.jsonl`: confirmed sense splits in sense-inventory
//!   shape (gloss + anchor sentences per sense — leaves of a recursive,
//!   judge-gated 2-means split tree).
//! - `discovered_multiword_terms.jsonl`: grounded, novel, frequent fixed
//!   expressions (the LLM only cites lines and copies verbatim substrings;
//!   surface forms, gram sequences, and corpus counts are derived here).
//!
//! Grams are treated as opaque identities throughout: an occurrence is a
//! gram from the sentence's own segmentation (the encoder's gram stream plus
//! multiword-term matches), its vector is the mean of the cached vectors of
//! the heteronym words it covers, and extracted expressions are sequences of
//! grams. No linguistic re-analysis (lemmas, POS grouping) happens here —
//! whatever units the gram system defines are the units mined.
//!
//! Run by the standalone `sense_discovery` binary; nothing in the main
//! generate-data pipeline consumes these files except
//! `wiktionary_terms::extra_multiword_terms`, which folds adopted discovered
//! terms into the next run's multiword-term inventory.

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
/// Maximum recursive split depth (senses = leaves of the split tree).
const MAX_DEPTH: usize = 3;
/// How many top-silhouette root splits get judged per language.
const JUDGE_TOP: usize = 100;
/// Exemplars shown to the judge per cluster side.
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
/// How many top-silhouette HDBSCAN proposals get judged per language (the
/// full corpus has thousands of n>=150 keys; unbounded, the secondary lane
/// would dwarf the primary one it is meant to complement).
const HDBSCAN_TOP: usize = 50;
/// A grounded expression must recur this often in the corpus to be proposed.
const MIN_EXPRESSION_COUNT: usize = 3;

/// Judgment-tier model, same reasoning as slot grading: few hundred calls
/// per language, and each one decides what enters the review files.
static JUDGE_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-terra"));

/// Interned gram identity. Ids 0..vocab_len coincide with the encoded
/// sentences' token ids; multiword-match grams are appended after.
type GramId = u32;

struct GramArena {
    grams: Vec<Gram<String>>,
    ids: HashMap<Gram<String>, GramId>,
}

impl GramArena {
    fn from_vocabulary(vocab: &[language_utils::GramVocabEntry<String>]) -> Self {
        let grams: Vec<Gram<String>> = vocab.iter().map(|e| e.atoms.clone()).collect();
        let ids = grams
            .iter()
            .enumerate()
            .map(|(i, g)| (g.clone(), i as GramId))
            .collect();
        GramArena { grams, ids }
    }

    fn intern(&mut self, gram: &Gram<String>) -> GramId {
        if let Some(&id) = self.ids.get(gram) {
            return id;
        }
        let id = self.grams.len() as GramId;
        self.grams.push(gram.clone());
        self.ids.insert(gram.clone(), id);
        id
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

/// Collect per-gram occurrences from each sentence's own segmentation.
/// High-confidence multiword matches claim their member words: the multiword
/// gram gets the occurrence and stream grams overlapping those words are
/// excluded, so already-adopted collocations are invisible to the miner
/// (that's the convergence property of the discover→adopt→re-segment loop).
fn collect_occurrences(
    corpus: &SegmentedCorpus,
    language: Language,
    index: &HashMap<String, SentenceIndex>,
    arena: &mut GramArena,
) -> BTreeMap<GramId, Vec<Occurrence>> {
    let mut occurrences: BTreeMap<GramId, Vec<Occurrence>> = BTreeMap::new();
    for info in corpus.nlp_sentences.values() {
        let decoded = info.decode_words(&corpus.interners, language);
        let text = token_embeddings::sentence_text(&decoded);
        let Some(sent) = index.get(&text) else {
            continue;
        };
        let heteronym_spans = |range: std::ops::Range<usize>| -> Vec<(u32, u32)> {
            range
                .filter_map(|i| sent.words.get(i))
                .filter(|w| w.is_heteronym && w.char_span.0 < w.char_span.1)
                .map(|w| w.char_span)
                .collect()
        };
        let mut consumed: HashSet<usize> = HashSet::new();
        let mut match_ids: HashSet<GramId> = HashSet::new();
        for m in &info.multiword_terms.high_confidence {
            let id = arena.intern(&m.gram);
            match_ids.insert(id);
            let mut spans = Vec::new();
            for &i in &m.matched_word_indices {
                let i = i as usize;
                consumed.insert(i);
                if let Some(w) = sent.words.get(i)
                    && w.is_heteronym
                    && w.char_span.0 < w.char_span.1
                {
                    spans.push(w.char_span);
                }
            }
            if !spans.is_empty() {
                occurrences.entry(id).or_default().push(Occurrence {
                    text: text.clone(),
                    spans,
                });
            }
        }
        for &(id, start, end) in &sent.gram_stream {
            // Skip stream grams overlapping a multiword match's words (and
            // any stream gram identical to a match gram, so a match that
            // coincides with the encoder's segmentation isn't double
            // counted).
            if (start..end).any(|i| consumed.contains(&(i as usize))) || match_ids.contains(&id) {
                continue;
            }
            if !arena.grams[id as usize].is_learnable() {
                continue;
            }
            let spans = heteronym_spans(start as usize..end as usize);
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

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
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
        let mut centroids = [vectors[i].clone(), vectors[j].clone()];
        let mut labels = vec![0u8; n];
        for _ in 0..100 {
            let mut changed = false;
            for (k, v) in vectors.iter().enumerate() {
                let label = u8::from(dot(v, &centroids[1]) > dot(v, &centroids[0]));
                if labels[k] != label {
                    labels[k] = label;
                    changed = true;
                }
            }
            for c in 0..2u8 {
                let members: Vec<&Vec<f32>> = vectors
                    .iter()
                    .zip(&labels)
                    .filter(|&(_, &l)| l == c)
                    .map(|(v, _)| *v)
                    .collect();
                if !members.is_empty() {
                    centroids[c as usize] = mean_normalized(&members);
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
// Split proposal
// ---------------------------------------------------------------------------

/// One proposed binary split of a gram's occurrence subset, ready to judge.
struct Split {
    key: GramId,
    /// Indices into the key's occurrence list, per side. Side 0 is larger.
    sides: [Vec<usize>; 2],
    /// Exemplar occurrence indices per side: nearest own-centroid, margin
    /// filtered (an occurrence near-tied between centroids is exactly the
    /// noise HDBSCAN would have excluded — skip it as an exemplar).
    exemplars: [Vec<usize>; 2],
    silhouette: f64,
    depth: usize,
    source: &'static str,
    /// Gloss inherited from the parent judgment that created this subset
    /// (None at the root).
    parent_gloss: Option<String>,
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

/// Propose a 2-means split of `subset` (indices into the key's occurrences).
/// None if the subset is too small or the split leaves a side under the
/// absolute floor.
fn propose_kmeans_split(
    key: GramId,
    vectors: &[Vec<f32>],
    subset: &[usize],
    depth: usize,
    parent_gloss: Option<String>,
) -> Option<Split> {
    if subset.len() < MIN_OCC {
        return None;
    }
    let sub_vecs: Vec<&Vec<f32>> = subset.iter().map(|&i| &vectors[i]).collect();
    let (labels, centroids) = kmeans2(&sub_vecs);
    let mut sides: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for (k, &l) in labels.iter().enumerate() {
        sides[l as usize].push(subset[k]);
    }
    if sides[1].len() < MIN_SIDE {
        return None;
    }
    let silhouette = cosine_silhouette(&sub_vecs, &labels);
    let local: HashMap<usize, usize> = subset.iter().enumerate().map(|(k, &i)| (i, k)).collect();
    let exemplars = [0, 1].map(|c: usize| {
        let side_local: Vec<usize> = sides[c].iter().map(|i| local[i]).collect();
        margin_filtered_exemplars(&sub_vecs, &side_local, &centroids[c], &centroids[1 - c])
            .into_iter()
            .map(|k| subset[k])
            .collect()
    });
    Some(Split {
        key,
        sides,
        exemplars,
        silhouette,
        depth,
        source: "kmeans",
        parent_gloss,
    })
}

/// Secondary proposer: HDBSCAN on large occurrence clouds, where density
/// estimation is in-regime. Each found cluster becomes a cluster-vs-rest
/// binary split (deduped against the kmeans root split by side overlap), fed
/// to the same judges — a bad proposal costs one judge call, nothing more.
fn propose_hdbscan_splits(
    key: GramId,
    vectors: &[Vec<f32>],
    kmeans_root: Option<&Split>,
) -> Vec<Split> {
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
    let mut splits = Vec::new();
    for (_, members) in clusters.iter() {
        if members.len() < MIN_SIDE {
            continue;
        }
        let member_set: HashSet<usize> = members.iter().copied().collect();
        let rest: Vec<usize> = labels
            .iter()
            .enumerate()
            .filter(|&(i, &l)| l >= 0 && !member_set.contains(&sample[i]))
            .map(|(i, _)| sample[i])
            .collect();
        if rest.len() < MIN_SIDE {
            continue;
        }
        // Dedup against the kmeans root split: same structure, skip. Root
        // sides are restricted to the subsample so the Jaccard is comparable.
        if let Some(root) = kmeans_root {
            let sample_set: HashSet<usize> = sample.iter().copied().collect();
            let overlaps = |side: &Vec<usize>| {
                let side_set: HashSet<usize> = side
                    .iter()
                    .copied()
                    .filter(|i| sample_set.contains(i))
                    .collect();
                let inter = member_set.intersection(&side_set).count();
                let union = member_set.union(&side_set).count();
                union > 0 && inter as f64 / union as f64 > 0.5
            };
            if overlaps(&root.sides[0]) || overlaps(&root.sides[1]) {
                continue;
            }
        }
        let subset: Vec<usize> = rest.iter().chain(members.iter()).copied().collect();
        let sub_vecs: Vec<&Vec<f32>> = subset.iter().map(|&i| &vectors[i]).collect();
        let labels_bin: Vec<u8> = subset
            .iter()
            .map(|i| u8::from(member_set.contains(i)))
            .collect();
        let silhouette = cosine_silhouette(&sub_vecs, &labels_bin);
        let rest_refs: Vec<&Vec<f32>> = rest.iter().map(|&i| &vectors[i]).collect();
        let member_refs: Vec<&Vec<f32>> = members.iter().map(|&i| &vectors[i]).collect();
        let rest_centroid = mean_normalized(&rest_refs);
        let member_centroid = mean_normalized(&member_refs);
        let exemplars = [
            margin_filtered_exemplars(
                &vectors.iter().collect::<Vec<_>>(),
                &rest,
                &rest_centroid,
                &member_centroid,
            ),
            margin_filtered_exemplars(
                &vectors.iter().collect::<Vec<_>>(),
                members,
                &member_centroid,
                &rest_centroid,
            ),
        ];
        splits.push(Split {
            key,
            sides: [rest, members.clone()],
            exemplars,
            silhouette,
            depth: 1,
            source: "hdbscan",
            parent_gloss: None,
        });
    }
    splits
}

// ---------------------------------------------------------------------------
// LLM judging
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SenseJudgeResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// Whether the two clusters are genuinely distinct meanings.
    #[serde(rename = "2. distinct")]
    distinct: bool,
    /// Confidence in the verdict, 0.0-1.0.
    #[serde(rename = "3. confidence")]
    confidence: f64,
    /// 2-5 word gloss of cluster A's sense (or of the shared sense).
    #[serde(rename = "4. sense_a")]
    sense_a: String,
    /// 2-5 word gloss of cluster B's sense (or of the shared sense).
    #[serde(rename = "5. sense_b")]
    sense_b: String,
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
    /// The expression copied VERBATIM (a contiguous substring) from one of
    /// the cited lines.
    #[serde(rename = "2. verbatim")]
    verbatim: String,
    /// 2-6 word English gloss.
    #[serde(rename = "3. gloss")]
    gloss: String,
    // No doc comment: schemars would emit it as a `description` next to the
    // enum's `$ref`, which OpenAI's structured-output validator rejects. The
    // opacity guidance lives in the system prompt instead.
    #[serde(rename = "4. opacity")]
    opacity: Opacity,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct CollocationJudgeResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// Fixed multiword expressions accounting for several lines, if any.
    #[serde(rename = "2. expressions")]
    expressions: Vec<ExtractedExpression>,
}

const SENSE_SYSTEM_PROMPT: &str = "You are auditing a language-learning vocabulary. Each \
    request shows a word or expression from a sentence corpus whose occurrences an \
    embedding model split into two clusters. Decide whether the clusters correspond to \
    GENUINELY DISTINCT MEANINGS — distinct enough that a learner would treat them as \
    different vocabulary items and they would usually receive different translations in \
    another language (homonyms like bank=money/river, or strong polysemy like \
    paper=material/newspaper). Mere topic, register, or inflection variation of one \
    meaning does NOT count. Idioms count as distinct if the word's meaning inside the \
    idiom is opaque. The target is marked \u{ab}like this\u{bb}. If not distinct, \
    sense_a and sense_b should both gloss the single shared sense.";

const COLLOCATION_SYSTEM_PROMPT: &str = "You are mining a sentence corpus for FIXED \
    MULTIWORD EXPRESSIONS (collocations, idioms, compounds, fixed phrases). Each request \
    shows a word from the corpus whose occurrences an embedding model split into two \
    clusters; often one cluster is dominated by the word occurring inside one or more \
    fixed expressions. The target word is marked \u{ab}like this\u{bb} in numbered \
    lines. For each fixed multiword expression of the target that accounts for several \
    lines, report it: cite the line numbers, copy the expression VERBATIM as a \
    contiguous substring of one cited line (never invent or normalize a citation form), \
    give a short English gloss, and classify opacity: \"opaque\" = meaning not \
    derivable from the parts, must be learned as its own vocabulary item; \"semi\" = \
    conventionalized, a learner benefits from learning it as a unit; \"transparent\" = \
    fully compositional. Do NOT report free combinations, inflectional variants, or \
    expressions appearing only once. Report an empty list if neither cluster is driven \
    by fixed expressions.";

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

/// The per-split prompt context: marked exemplar lines, globally numbered so
/// the collocation lane can cite them.
struct SplitPrompt {
    display: String,
    /// (global line number, side, occurrence index, marked line)
    lines: Vec<(usize, usize, usize, String)>,
}

fn split_prompt(split: &Split, occs: &[Occurrence], display: String) -> SplitPrompt {
    let mut lines = Vec::new();
    let mut number = 0;
    for side in 0..2 {
        for &i in &split.exemplars[side] {
            lines.push((number, side, i, mark_occurrence(&occs[i])));
            number += 1;
        }
    }
    SplitPrompt { display, lines }
}

fn sense_user_prompt(p: &SplitPrompt) -> String {
    let mut out = format!("Target: \"{}\"\n\nCluster A:\n", p.display);
    for cluster in 0..2 {
        if cluster == 1 {
            out.push_str("\nCluster B:\n");
        }
        for (_, _, _, line) in p.lines.iter().filter(|(_, s, _, _)| *s == cluster) {
            let _ = writeln!(out, "- {line}");
        }
    }
    out
}

fn collocation_user_prompt(p: &SplitPrompt) -> String {
    let mut out = format!("Target: \"{}\"\n\nCluster A:\n", p.display);
    for cluster in 0..2 {
        if cluster == 1 {
            out.push_str("\nCluster B:\n");
        }
        for (n, _, _, line) in p.lines.iter().filter(|(_, s, _, _)| *s == cluster) {
            let _ = writeln!(out, "{n}. {line}");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Grounding
// ---------------------------------------------------------------------------

/// A grounded, corpus-verified expression proposal.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredTerm {
    /// Surface form: the covered words as they appear in the cited sentence.
    pub term: String,
    /// Display of the gram this proposal would become (its atom sequence).
    pub gram: String,
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

/// Count corpus sentences whose atom sequence contains `needle` contiguously.
fn count_atom_windows(index: &HashMap<String, SentenceIndex>, needle: &[SpurAtom]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    index
        .values()
        .filter(|s| s.atom_seq.windows(needle.len()).any(|w| w == needle))
        .count()
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
    pub key: String,
    pub n: usize,
    pub senses: Vec<SenseEntry>,
    pub silhouette: f64,
    pub confidence: f64,
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

/// Discover sense splits and collocation candidates for one language and
/// write the proposal files under `generate-data/data/{lang}/`.
pub async fn discover(
    language: Language,
    corpus: &SegmentedCorpus,
    store: &osmo::Store,
) -> Result<()> {
    let mut arena = GramArena::from_vocabulary(&corpus.gram_vocabulary);

    // Index every sentence: decoded word spans, the gram segmentation
    // (aligned by construction in the encoded form), and interned atoms.
    let index: HashMap<String, SentenceIndex> = corpus
        .nlp_sentences
        .values()
        .map(|info| index_sentence(info, &corpus.interners, language))
        .collect();

    let occurrences = collect_occurrences(corpus, language, &index, &mut arena);
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

    // Root proposals: 2-means on every key (ranked, top-K), plus HDBSCAN
    // cluster-vs-rest splits on large clouds (deduped against the root).
    // CPU-bound and per-key independent, so mine keys in parallel.
    use rayon::prelude::*;
    let mut root_splits: Vec<Split> = keys
        .par_iter()
        .flat_map_iter(|km| {
            let all: Vec<usize> = (0..km.occs.len()).collect();
            let kmeans_root = propose_kmeans_split(km.key, &km.vectors, &all, 1, None);
            let hdbscan = propose_hdbscan_splits(km.key, &km.vectors, kmeans_root.as_ref());
            kmeans_root.into_iter().chain(hdbscan)
        })
        .collect();
    // kmeans splits strictly before hdbscan ones (so a key's kmeans tree is
    // resolved before its hdbscan duplicate is considered), each group by
    // silhouette.
    root_splits.sort_by(|a, b| {
        (a.source == "hdbscan")
            .cmp(&(b.source == "hdbscan"))
            .then_with(|| b.silhouette.total_cmp(&a.silhouette))
            .then_with(|| a.key.cmp(&b.key))
    });
    let kept: Vec<Split> = {
        let mut kmeans_seen = 0usize;
        let mut hdbscan_seen = 0usize;
        root_splits
            .into_iter()
            .filter(|s| {
                if s.source == "kmeans" {
                    kmeans_seen += 1;
                    kmeans_seen <= JUDGE_TOP
                } else {
                    hdbscan_seen += 1;
                    hdbscan_seen <= HDBSCAN_TOP
                }
            })
            .collect()
    };
    println!(
        "sense-discovery[{}]: judging {} root splits",
        language.code(),
        kept.len()
    );

    let key_lookup: HashMap<GramId, usize> =
        keys.iter().enumerate().map(|(i, km)| (km.key, i)).collect();

    // Known multiword units (vocabulary grams and multiword-match grams
    // alike) as Tok-atom sequences, for the novelty check: a proposal whose
    // atom sequence is already a gram isn't a discovery. Match grams outside
    // the vocabulary carry `String` atoms; ones whose strings were never
    // interned can't match any corpus window, so they're safely skipped.
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

    // Judge round by round; confirmed splits recurse into their sides.
    let mut sense_rows: Vec<SenseCandidate> = Vec::new();
    let mut discovered: BTreeMap<Vec<SpurAtom>, DiscoveredTerm> = BTreeMap::new();
    let mut round = kept;
    let mut depth = 1;
    // Per key: the leaves of the split tree plus root stats, built as splits
    // resolve. A key enters the inventory only if its root split confirmed.
    struct Tree {
        leaves: Vec<(String, Vec<usize>, f64)>, // gloss, indices, confidence
        root_sil: f64,
        root_conf: f64,
        source: &'static str,
    }
    let mut trees: BTreeMap<GramId, Tree> = BTreeMap::new();
    while !round.is_empty() && depth <= MAX_DEPTH {
        let prompts: Vec<SplitPrompt> = round
            .iter()
            .map(|s| {
                split_prompt(
                    s,
                    &keys[key_lookup[&s.key]].occs,
                    arena.display(s.key, language),
                )
            })
            .collect();

        let progress = |label: &str, n: usize| {
            let pb = indicatif::ProgressBar::new(n as u64);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template(&format!(
                        "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {label} ({{per_sec}}, ${{msg}}, {{eta}})"
                    ))
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            pb
        };

        let pb = progress(
            &format!("{} sense judgments (depth {depth})", language.code()),
            prompts.len(),
        );
        let n_req = prompts.len();
        let sense_results = JUDGE_CLIENT
            .batch_chat_with_system_prompt_fn::<_, _, SenseJudgeResponse>(
                SENSE_SYSTEM_PROMPT,
                &prompts,
                sense_user_prompt,
                |batch| crate::report_batch_progress(&pb, 0, n_req, batch),
            )
            .await
            .context("sense judge batch failed")?;
        pb.finish_with_message(format!("{:.2}", JUDGE_CLIENT.cost().unwrap_or(0.0)));

        let pb = progress(
            &format!(
                "{} collocation extractions (depth {depth})",
                language.code()
            ),
            prompts.len(),
        );
        let colloc_results = JUDGE_CLIENT
            .batch_chat_with_system_prompt_fn::<_, _, CollocationJudgeResponse>(
                COLLOCATION_SYSTEM_PROMPT,
                &prompts,
                collocation_user_prompt,
                |batch| crate::report_batch_progress(&pb, 0, n_req, batch),
            )
            .await
            .context("collocation judge batch failed")?;
        pb.finish_with_message(format!("{:.2}", JUDGE_CLIENT.cost().unwrap_or(0.0)));

        let mut next_round: Vec<Split> = Vec::new();
        for (idx, split) in round.iter().enumerate() {
            let (_, sense) = &sense_results[idx];
            let (_, colloc) = &colloc_results[idx];
            let km = &keys[key_lookup[&split.key]];
            let prompt = &prompts[idx];

            // Collocation lane: ground every extraction against the cited
            // lines (word range -> atom sequence, the substrate a new gram
            // would be made of), then against the whole corpus.
            if let Ok(colloc) = colloc {
                for e in &colloc.expressions {
                    if e.opacity == Opacity::Transparent {
                        continue;
                    }
                    let Some((ids, surface)) = e.line_numbers.iter().find_map(|&n| {
                        let (_, _, occ_idx, _) =
                            prompt.lines.iter().find(|(ln, _, _, _)| *ln == n)?;
                        let occ = &km.occs[*occ_idx];
                        let sent = index.get(&occ.text)?;
                        let (first, last) = snap_to_words(sent, &occ.text, &e.verbatim)?;
                        let ids = sent.atom_seq.get(first..=last)?.to_vec();
                        let surface = occ.text
                            [sent.words[first].byte_span.0..sent.words[last].byte_span.1]
                            .to_string();
                        Some((ids, surface))
                    }) else {
                        log::info!(
                            "sense-discovery[{}]: ungroundable extraction {:?} for {}",
                            language.code(),
                            e.verbatim,
                            prompt.display,
                        );
                        continue;
                    };
                    if ids.len() < 2 {
                        continue;
                    }
                    let count = count_atom_windows(&index, &ids);
                    if count < MIN_EXPRESSION_COUNT {
                        continue;
                    }
                    discovered.entry(ids.clone()).or_insert_with(|| {
                        let gram = Gram::from(
                            ids.iter()
                                .map(|a| a.resolve(&corpus.interners.strings))
                                .collect::<Vec<Atom<String>>>(),
                        );
                        DiscoveredTerm {
                            term: surface,
                            gram: gram.to_display_string(language),
                            gloss: e.gloss.clone(),
                            opacity: format!("{:?}", e.opacity).to_lowercase(),
                            count,
                            source: prompt.display.clone(),
                            silhouette: split.silhouette,
                        }
                    });
                }
            }

            // Sense lane: confirmed splits recurse; refuted ones close their
            // subtree into a leaf.
            let Ok(sense) = sense else { continue };
            if sense.distinct {
                if split.source == "hdbscan" && trees.contains_key(&split.key) {
                    // The kmeans tree already covers this key (kmeans splits
                    // sort before hdbscan ones); skip the duplicate for
                    // inventory purposes. Its collocation extractions above
                    // still count.
                    continue;
                }
                trees.entry(split.key).or_insert(Tree {
                    leaves: Vec::new(),
                    root_sil: split.silhouette,
                    root_conf: sense.confidence,
                    source: split.source,
                });
                for (side, gloss) in [(0, &sense.sense_a), (1, &sense.sense_b)] {
                    let indices = split.sides[side].clone();
                    match propose_kmeans_split(
                        split.key,
                        &km.vectors,
                        &indices,
                        split.depth + 1,
                        Some(gloss.clone()),
                    ) {
                        Some(child) if split.depth < MAX_DEPTH && split.source == "kmeans" => {
                            next_round.push(child)
                        }
                        _ => trees.get_mut(&split.key).unwrap().leaves.push((
                            gloss.clone(),
                            indices,
                            sense.confidence,
                        )),
                    }
                }
            } else if let (Some(gloss), Some(tree)) =
                (&split.parent_gloss, trees.get_mut(&split.key))
            {
                // A refuted deeper split: the whole subset is one sense.
                let indices: Vec<usize> = split.sides.iter().flatten().copied().collect();
                tree.leaves.push((gloss.clone(), indices, 1.0));
            }
        }
        round = next_round;
        depth += 1;
    }
    // Any splits still pending when depth ran out: close them into leaves.
    for split in round {
        if let (Some(gloss), Some(tree)) = (&split.parent_gloss, trees.get_mut(&split.key)) {
            let indices: Vec<usize> = split.sides.iter().flatten().copied().collect();
            tree.leaves.push((gloss.clone(), indices, 1.0));
        }
    }

    for (key, tree) in trees {
        if tree.leaves.len() < 2 {
            continue;
        }
        let km = &keys[key_lookup[&key]];
        let senses: Vec<SenseEntry> = tree
            .leaves
            .iter()
            .map(|(gloss, indices, _)| {
                let anchors: Vec<SenseAnchor> = indices
                    .iter()
                    .take(EXEMPLARS)
                    .map(|&i| SenseAnchor {
                        sentence: km.occs[i].text.clone(),
                        spans: km.occs[i].spans.clone(),
                    })
                    .collect();
                SenseEntry {
                    gloss: gloss.clone(),
                    n: indices.len(),
                    anchors,
                }
            })
            .collect();
        sense_rows.push(SenseCandidate {
            key: arena.display(key, language),
            n: km.occs.len(),
            senses,
            silhouette: tree.root_sil,
            confidence: tree.root_conf,
            source: tree.source.to_string(),
        });
    }
    sense_rows.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
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
    let discovered: Vec<DiscoveredTerm> = discovered
        .into_iter()
        .filter(|(ids, d)| {
            !known_terms.contains(&normalize_term(&d.term)) && !known_multi.contains(ids)
        })
        .map(|(_, d)| d)
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

    let terms_path = Path::new(&data_dir).join("discovered_multiword_terms.jsonl");
    let mut f =
        File::create(&terms_path).context("Failed to create discovered_multiword_terms.jsonl")?;
    for row in &discovered {
        writeln!(f, "{}", serde_json::to_string(row)?)?;
    }
    println!(
        "sense-discovery[{}]: wrote {} discovered multiword terms to {}",
        language.code(),
        discovered.len(),
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
    fn silhouette_low_for_one_blob() {
        let data: Vec<Vec<f32>> = (0..12).map(|i| vec2(1.0, 0.001 * i as f32)).collect();
        let refs: Vec<&Vec<f32>> = data.iter().collect();
        let (labels, _) = kmeans2(&refs);
        let sil = cosine_silhouette(&refs, &labels);
        assert!(sil < 0.9, "one blob should not look cleanly separable");
    }
}
