//! Slot-loosened multiword-term matching.
//!
//! Dictionary citation forms often contain placeholder words that stand for
//! open argument slots — French "arriver à quelqu'un", English "give someone
//! the benefit of the doubt". These placeholders essentially never appear
//! literally in real sentences: the slot is filled by an actual noun phrase
//! ("c'est arrivé à Jean") or realized as a clitic pronoun ("ce qui *leur*
//! est arrivé"), so a literal pattern compiled from the citation form never
//! matches.
//!
//! This module fixes that with a split of responsibilities:
//! - An LLM makes the language-specific judgments, cached per call by tysm:
//!   which words are placeholders at all ([`placeholder_hints`]), which
//!   placeholder tokens in a given term are real slots vs. fixed parts of the
//!   idiom ([`analyze_slots`]), and which clitic pronoun lemmas can fill a
//!   slot.
//! - Deterministic code does the tree math in Universal Dependencies terms
//!   ([`compile_realizations`]), which holds across languages: a slot can be
//!   realized *filled* (any nominal, same case marker) or *cliticized*
//!   (a pronoun from the slot's clitic set attached to the head as iobj/obj,
//!   with the case marker gone).
//!
//! Loosened patterns overmatch by design ("arriver à" + place vs. "arriver à" +
//! person); [`grade_matches`] LLM-checks a deterministic sample of each (term,
//! realization)'s matches so the pipeline can drop realizations whose precision
//! is bad — one call per pattern, not per sentence.

use language_utils::Course;
use lexide::matching::{NodeMatcher, PatternNode};
use lexide::pos::PartOfSpeech;
use lexide::{DependencyRelation, Token};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

/// Listing a language's placeholder expressions — 2 calls per language, and
/// mostly recall of well-known vocabulary rather than judgment, so it runs on
/// the cheaper tier.
static HINTS_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-sol"));

/// The two judgment steps: deciding slot-vs-literal per term, and grading a
/// pattern's matches. Both are only a few hundred calls per language, and
/// both are where model quality shows — a missed slot is a silent recall
/// loss, and grading decides keep/drop from a 12-sentence sample.
static ANALYSIS_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-terra"));

/// How a slot pattern realizes its argument slot in a real sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotRealization {
    /// The slot is filled by an arbitrary nominal, keeping the citation
    /// form's case marker: "c'est arrivé à Jean". Precision varies a lot by
    /// term (the case marker is often ambiguous), so these matches are
    /// low-confidence and lean on the grading gate.
    Filled,
    /// The slot is realized as a clitic/weak pronoun on the head, with the
    /// case marker gone: "ce qui leur est arrivé". High precision.
    Clitic,
}

impl std::fmt::Display for SlotRealization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlotRealization::Filled => write!(f, "filled"),
            SlotRealization::Clitic => write!(f, "clitic"),
        }
    }
}

/// Grammatical role of a slot in its citation form: `direct_object` is a bare
/// object of the verb ("dire quelque chose"), `case_marked_argument` is the
/// object of a preposition/case marker ("arriver à quelqu'un"), `possessive`
/// is "de quelqu'un" / "someone's" (realized as a possessive determiner).
//
// NOTE: no doc comments on the variants or on fields of this type — OpenAI
// structured outputs rejects schemas where a description sits next to a $ref.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    DirectObject,
    CaseMarkedArgument,
    Possessive,
    Other,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SlotSpec {
    /// 0-based index of the slot's head token (the pronoun/noun itself, not
    /// its case marker or determiner).
    #[serde(rename = "1. token_index")]
    pub token_index: usize,
    /// true if this placeholder gets replaced by a real argument in actual
    /// usage; false if it is a literal, fixed part of the idiom.
    #[serde(rename = "2. is_slot")]
    pub is_slot: bool,
    #[serde(rename = "3. role")]
    pub role: SlotRole,
    /// Lemmas of clitic/weak pronouns (or possessive determiners for
    /// possessive slots) that can realize this slot in real sentences, as the
    /// NLP lemmatizer would lemmatize them. Empty if the slot cannot be
    /// pronominalized.
    #[serde(rename = "4. clitic_pronoun_lemmas")]
    pub clitic_pronoun_lemmas: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct SlotAnalysisResponse {
    /// Brief reasoning about which tokens are placeholder slots.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// One entry per candidate placeholder token.
    #[serde(rename = "2. slots")]
    slots: Vec<SlotSpec>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct PlaceholderHintsResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// Placeholder expressions, each as the space-separated lemma sequence a
    /// UD lemmatizer would produce.
    #[serde(rename = "2. placeholder_phrases")]
    placeholder_phrases: Vec<String>,
}

/// Ask the LLM which placeholder expressions this language uses in dictionary
/// citation forms. This only gates which terms get a per-term analysis call
/// (a cost optimization); the per-term call makes the actual slot-vs-literal
/// decision.
///
/// Two complementary calls, unioned: an abstract one (list the language's
/// placeholder words from knowledge of the language) and one grounded in a
/// sample of the actual term list (spot placeholders as they really appear,
/// in the lemma forms our tokenizer actually produced). The union protects
/// against either call's blind spots — a placeholder the abstract call
/// forgot is usually visible in the sample, and vice versa.
async fn placeholder_hints(
    course: &Course,
    multiword_terms_tokenizations: &BTreeMap<String, Vec<Token>>,
) -> anyhow::Result<Vec<Vec<String>>> {
    let language = course.target_language;
    let shared_instructions = "You are helping a language-learning app detect which \
        dictionary citation forms of idioms/multiword terms contain PLACEHOLDER \
        expressions standing for open argument slots. Examples: French \"quelqu'un\", \
        \"quelque chose\"; English \"someone\", \"something\", \"somebody\", \"one's\", \
        \"oneself\"; German \"jemand\", \"etwas\". Return every such placeholder \
        expression, each written as the space-separated LEMMA sequence a Universal \
        Dependencies lemmatizer would produce for it (e.g. \"quelque chose\" stays two \
        lemmas). Include indefinite person and thing placeholders and possessive \
        placeholders. Do not include ordinary pronouns (je, tu, il...) or reflexive \
        markers.";

    let abstract_response: PlaceholderHintsResponse = HINTS_CLIENT
        .chat_with_system_prompt(
            shared_instructions,
            format!("Language: {language}\nList the placeholder expressions."),
        )
        .await?;

    // Grounded call: show a deterministic sample of real terms (with the
    // lemma sequences our tokenizer produced) and ask which placeholder
    // expressions appear in them.
    const GROUNDING_SAMPLE_SIZE: usize = 200;
    let all_terms: Vec<(&String, &Vec<Token>)> = multiword_terms_tokenizations.iter().collect();
    let sampled: Vec<String> = if all_terms.is_empty() {
        vec![]
    } else {
        (0..GROUNDING_SAMPLE_SIZE.min(all_terms.len()))
            .map(|i| {
                let (term, tokens) =
                    &all_terms[i * all_terms.len() / GROUNDING_SAMPLE_SIZE.min(all_terms.len())];
                let lemmas = tokens
                    .iter()
                    .map(|t| t.lemma.lemma.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{term} (lemmas: {lemmas})")
            })
            .collect()
    };
    let grounded_response: Option<PlaceholderHintsResponse> = if sampled.is_empty() {
        None
    } else {
        Some(
            HINTS_CLIENT
                .chat_with_system_prompt(
                    format!(
                        "{shared_instructions}\n\nYou are given a sample of real {language} \
                         multiword terms from the dictionary, each with the lemma sequence \
                         our tokenizer produced. Report every placeholder expression that \
                         appears in any of them, quoting the lemma forms exactly as they \
                         appear in the sample. Also report placeholder expressions you know \
                         the language uses even if absent from this sample."
                    ),
                    format!("Terms:\n{}", sampled.join("\n")),
                )
                .await?,
        )
    };

    let mut hints: BTreeSet<Vec<String>> = BTreeSet::new();
    for phrase in abstract_response.placeholder_phrases.iter().chain(
        grounded_response
            .iter()
            .flat_map(|r| r.placeholder_phrases.iter()),
    ) {
        let lemmas: Vec<String> = phrase.split_whitespace().map(str::to_string).collect();
        if !lemmas.is_empty() {
            hints.insert(lemmas);
        }
    }
    Ok(hints.into_iter().collect())
}

fn contains_phrase(lemmas: &[&str], phrase: &[String]) -> bool {
    !phrase.is_empty()
        && lemmas
            .windows(phrase.len())
            .any(|w| w.iter().zip(phrase).all(|(a, b)| *a == b))
}

fn format_parse(tokens: &[Token]) -> String {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "  {i}: {} / {} / {:?} / {} -> {}",
                t.text.text, t.lemma.lemma, t.pos, t.dep, t.head
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// LLM slot analysis for every candidate term (one cached call per term).
/// Returns, for each term that has at least one real slot, the slot specs.
pub async fn analyze_slots(
    course: &Course,
    multiword_terms_tokenizations: &BTreeMap<String, Vec<Token>>,
) -> anyhow::Result<BTreeMap<String, Vec<SlotSpec>>> {
    let language = course.target_language;
    let hints = placeholder_hints(course, multiword_terms_tokenizations).await?;
    if hints.is_empty() {
        return Ok(BTreeMap::new());
    }

    let candidates: Vec<(&String, &Vec<Token>)> = multiword_terms_tokenizations
        .iter()
        .filter(|(_, tokens)| {
            let lemmas: Vec<&str> = tokens.iter().map(|t| t.lemma.lemma.as_str()).collect();
            hints.iter().any(|phrase| contains_phrase(&lemmas, phrase))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(BTreeMap::new());
    }
    println!(
        "Slot analysis: {} candidate slot-bearing terms (of {})",
        candidates.len(),
        multiword_terms_tokenizations.len()
    );

    let system_prompt = format!(
        r#"You are helping a language-learning app detect idioms/multiword terms in real {language} sentences.

You are given a dictionary citation form and its dependency parse (index: text / lemma / POS / dep -> head index, 1-based heads, 0 = root).

Some citation forms contain PLACEHOLDER words (like French "quelqu'un"/"quelque chose", English "someone"/"something"/"one's") that stand for an open argument slot — in real sentences they are replaced by an actual noun phrase or a clitic pronoun. Others use these words LITERALLY as a fixed part of the idiom (e.g. "il y a quelque chose de pourri au royaume du Danemark" is a fixed quote).

For each candidate placeholder token in the citation form, decide:
1. is_slot: is it an open argument slot (true) or a literal fixed part (false)?
2. role: direct_object (bare object of the verb), case_marked_argument (object of a preposition/case marker), possessive ("of someone" / "someone's"), or other.
3. clitic_pronoun_lemmas: which clitic/weak pronoun lemmas can fill this slot in real sentences. For French: dative slots ("à quelqu'un") -> ["me","te","lui","nous","vous","leur","se"]; direct-object slots -> ["le","la","les","me","te","nous","vous","se"]; inanimate "de X" -> ["en"]; inanimate "à X" -> ["y"]; possessive "de quelqu'un" -> possessive determiner lemmas ["mon","ton","son","notre","votre","leur"]. Other languages: use that language's clitic/weak pronoun system, or an empty list if the language has no such pronouns for this slot. Use the lemma forms a UD lemmatizer would output. Empty list if pronominalization is impossible or would destroy the idiom.

Only report tokens that are placeholder candidates (indefinite pronouns / "quelque chose"-type phrases). Report the index of the HEAD token of the placeholder phrase (e.g. "chose" in "quelque chose")."#
    );

    let progress = indicatif::ProgressBar::new(candidates.len() as u64);
    progress.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} slot analyses ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let n_candidates = candidates.len();
    let results = ANALYSIS_CLIENT
        .batch_chat_with_system_prompt_fn::<_, _, SlotAnalysisResponse>(
            system_prompt,
            &candidates,
            |(term, tokens)| {
                format!(
                    "Citation form: \"{term}\"\n\nParse:\n{}",
                    format_parse(tokens)
                )
            },
            |batch| crate::report_batch_progress(&progress, 0, n_candidates, batch),
        )
        .await?;
    progress.finish_with_message(format!("{:.2}", ANALYSIS_CLIENT.cost().unwrap_or(0.0)));

    let mut out = BTreeMap::new();
    for ((term, tokens), response) in results {
        let Ok(response) = response else { continue };
        let slots: Vec<SlotSpec> = response
            .slots
            .into_iter()
            .filter(|s| s.is_slot && s.token_index < tokens.len())
            .collect();
        if !slots.is_empty() {
            out.insert((*term).clone(), slots);
        }
    }
    Ok(out)
}

/// Relations that can express possession.
///
/// UD splits possession across determiner possessives ("her prayers") and
/// nominal ones ("John's prayers"), and parsers label the first as either the
/// plain `det` or the subtyped `det:poss` — both are common in our corpora
/// (in English, 28k `det` vs 3k `det:poss`; in French, 22k vs 5k). A slot
/// edge therefore has to accept the whole family, because the label the
/// citation form happened to receive says nothing about the label a real
/// sentence's possessor will get.
fn possessive_relations() -> BTreeSet<DependencyRelation> {
    BTreeSet::from([
        DependencyRelation::Det,
        DependencyRelation::DetPoss,
        DependencyRelation::Nmod,
        DependencyRelation::NmodPoss,
    ])
}

/// The relations a slot's edge should accept when the slot is *filled* by a
/// real phrase. Possessives move between the det/nmod families depending on
/// how the possessor is expressed; every other role keeps the citation
/// form's own relation.
fn filled_slot_relations(
    role: SlotRole,
    citation_dep: DependencyRelation,
) -> BTreeSet<DependencyRelation> {
    match role {
        SlotRole::Possessive => possessive_relations(),
        _ => BTreeSet::from([citation_dep]),
    }
}

/// Which POS a wildcard slot filler may have.
fn nominal_pos() -> BTreeSet<PartOfSpeech> {
    BTreeSet::from([
        PartOfSpeech::Noun,
        PartOfSpeech::Propn,
        PartOfSpeech::Pron,
        PartOfSpeech::Num,
    ])
}

fn children_of(tokens: &[Token], idx: usize) -> impl Iterator<Item = usize> + '_ {
    // heads are 1-based, like lexide's TreeNode construction
    tokens
        .iter()
        .enumerate()
        .filter(move |(j, t)| t.head as usize == idx + 1 && *j != idx)
        .map(|(j, _)| j)
}

/// Compile the loosened realizations of a slotted citation form.
///
/// The tree math is stated purely in UD vocabulary, so it holds for every
/// language the pipeline parses:
/// - `Filled`: the slot node becomes a nominal wildcard; only its case-marker
///   children are kept as requirements (its determiners — "quelque" — vanish
///   with the placeholder).
/// - `Clitic`: the slot subtree is removed and the head instead requires a
///   pronoun from the slot's clitic set as `iobj`/`obj` (case-marked and
///   direct slots), or a possessive determiner (possessive slots). UD
///   annotates clitic datives as `iobj` uniformly across languages, which is
///   what makes this a single rule.
///
/// A term with several slots yields only all-filled and all-cliticized
/// patterns, not the mixed realizations in between; almost every slotted
/// citation form has a single slot, so the combinatorial version isn't worth
/// its cost yet.
pub fn compile_realizations(
    tokens: &[Token],
    slots: &[SlotSpec],
) -> Vec<(SlotRealization, PatternNode)> {
    // Reuse lexide's own validation: it rejects parses with orphaned tokens
    // or no root, which would otherwise compile to a partial pattern that
    // matches far more than the term does.
    if lexide::matching::TreeNode::try_from(lexide::Tokenization {
        tokens: tokens.to_vec(),
    })
    .is_err()
    {
        return vec![];
    }

    let slot_by_idx: BTreeMap<usize, &SlotSpec> =
        slots.iter().map(|s| (s.token_index, s)).collect();
    let Some(root_idx) = tokens
        .iter()
        .position(|t| t.dep == DependencyRelation::Root || t.head as usize == 0)
    else {
        return vec![];
    };
    // A slot at the root would compile to an unanchored wildcard pattern that
    // matches half the corpus; such "terms" are not real multiword slots.
    if slot_by_idx.contains_key(&root_idx) {
        return vec![];
    }

    fn build(
        tokens: &[Token],
        idx: usize,
        realization: SlotRealization,
        slot_by_idx: &BTreeMap<usize, &SlotSpec>,
    ) -> Option<PatternNode> {
        let token = &tokens[idx];
        if slot_by_idx.contains_key(&idx) {
            // Only reachable in Filled mode (Clitic replaces the slot at the
            // parent): wildcard nominal keeping just the case marker.
            let children = children_of(tokens, idx)
                .filter(|&c| tokens[c].dep == DependencyRelation::Case)
                .map(|c| {
                    (
                        BTreeSet::from([tokens[c].dep]),
                        PatternNode {
                            matcher: NodeMatcher::Lemma(tokens[c].lemma.lemma.clone()),
                            children: vec![],
                        },
                    )
                })
                .collect();
            return Some(PatternNode {
                matcher: NodeMatcher::AnyPos(nominal_pos()),
                children,
            });
        }

        let mut children = Vec::new();
        for c in children_of(tokens, idx) {
            if tokens[c].dep == DependencyRelation::Punct {
                continue;
            }
            if realization == SlotRealization::Clitic
                && let Some(slot) = slot_by_idx.get(&c)
            {
                if slot.clitic_pronoun_lemmas.is_empty() {
                    return None; // this slot can't cliticize
                }
                let lemmas: BTreeSet<String> = slot.clitic_pronoun_lemmas.iter().cloned().collect();
                let (deps, pos) = match slot.role {
                    SlotRole::Possessive => (
                        possessive_relations(),
                        BTreeSet::from([PartOfSpeech::Det, PartOfSpeech::Pron]),
                    ),
                    SlotRole::CaseMarkedArgument => (
                        BTreeSet::from([DependencyRelation::Iobj, DependencyRelation::Obj]),
                        BTreeSet::from([PartOfSpeech::Pron]),
                    ),
                    SlotRole::DirectObject | SlotRole::Other => (
                        BTreeSet::from([DependencyRelation::Obj]),
                        BTreeSet::from([PartOfSpeech::Pron]),
                    ),
                };
                children.push((
                    deps,
                    PatternNode {
                        matcher: NodeMatcher::LemmaSet { lemmas, pos },
                        children: vec![],
                    },
                ));
                continue;
            }
            let child = build(tokens, c, realization, slot_by_idx)?;
            let deps = match slot_by_idx.get(&c) {
                // A filled slot's edge follows the filler, not the citation
                // form's placeholder.
                Some(slot) => filled_slot_relations(slot.role, tokens[c].dep),
                None => BTreeSet::from([tokens[c].dep]),
            };
            children.push((deps, child));
        }
        Some(PatternNode {
            matcher: NodeMatcher::Lemma(token.lemma.lemma.clone()),
            children,
        })
    }

    [SlotRealization::Filled, SlotRealization::Clitic]
        .into_iter()
        .filter_map(|realization| {
            build(tokens, root_idx, realization, &slot_by_idx).map(|p| (realization, p))
        })
        .collect()
}

/// One sentence matched by a slot pattern, with the tokens the pattern bound.
/// The indices let downstream consumers (grading, and potentially the app)
/// point at the exact words that realized the term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotMatch {
    pub sentence: String,
    /// Indices into the sentence's tokenization of the tokens the pattern's
    /// nodes bound, sorted.
    pub matched_token_indices: Vec<usize>,
    /// The surface text of those tokens, in sentence order.
    pub matched_words: Vec<String>,
}

/// Match every slot pattern against every sentence's dependency tree.
/// Returns, for each pattern (parallel to `patterns`), the sentences it
/// matched.
pub fn find_slot_matches(
    sentence_tokenizations: &BTreeMap<String, Vec<Token>>,
    patterns: &[PatternNode],
) -> Vec<Vec<SlotMatch>> {
    use lexide::matching::{DependencyMatcher, TreeNode};

    let labeled: Vec<(usize, PatternNode)> = patterns.iter().cloned().enumerate().collect();
    let matcher = DependencyMatcher::new(&labeled);

    let mut matches: Vec<Vec<SlotMatch>> = vec![Vec::new(); patterns.len()];
    for (sentence, tokens) in sentence_tokenizations {
        let tokenization = lexide::Tokenization {
            tokens: tokens.clone(),
        };
        let Ok(tree) = TreeNode::try_from(tokenization) else {
            continue;
        };
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for m in matcher.find_all(&tree) {
            if seen.insert(m.matched_label) {
                let matched_words = m
                    .matched_token_indices
                    .iter()
                    .filter_map(|&i| tokens.get(i))
                    .map(|t| t.text.text.clone())
                    .collect();
                matches[m.matched_label].push(SlotMatch {
                    sentence: sentence.clone(),
                    matched_token_indices: m.matched_token_indices,
                    matched_words,
                });
            }
        }
    }
    matches
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct GradeVerdict {
    /// Index of the sentence in the numbered list.
    #[serde(rename = "1. index")]
    index: usize,
    /// Whether the sentence actually uses the term in its idiom/construction
    /// sense.
    #[serde(rename = "2. uses_idiom")]
    uses_idiom: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct GradeResponse {
    /// Brief reasoning.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// One verdict per numbered sentence.
    #[serde(rename = "2. verdicts")]
    verdicts: Vec<GradeVerdict>,
}

/// Deterministically sample up to `n` matches, spread across the (sorted)
/// match list so the sample stays stable across runs and tysm's cache hits.
pub fn sample_matches(matches: &[SlotMatch], n: usize) -> Vec<SlotMatch> {
    let mut sorted: Vec<&SlotMatch> = matches.iter().collect();
    sorted.sort_by(|a, b| a.sentence.cmp(&b.sentence));
    sorted.dedup_by(|a, b| a.sentence == b.sentence);
    if sorted.len() <= n {
        return sorted.into_iter().cloned().collect();
    }
    (0..n)
        .map(|i| sorted[i * sorted.len() / n].clone())
        .collect()
}

/// How many of a pattern's matches get graded.
pub const GRADE_SAMPLE_SIZE: usize = 12;

/// Sampled precision a realization must reach to be kept.
pub const PRECISION_THRESHOLD: f64 = 0.6;

/// One pattern queued for grading, with its sample already drawn (so callers
/// never hand over — or clone — the full match list, which runs to thousands
/// of sentences for common terms).
#[derive(Debug, Clone)]
pub struct GradeRequest {
    pub term: String,
    pub realization: SlotRealization,
    pub match_count: usize,
    pub sample: Vec<SlotMatch>,
}

impl GradeRequest {
    pub fn new(term: String, realization: SlotRealization, matches: &[SlotMatch]) -> Self {
        Self {
            term,
            realization,
            match_count: matches.len(),
            sample: sample_matches(matches, GRADE_SAMPLE_SIZE),
        }
    }
}

/// The graded verdict for one (term, realization).
#[derive(Debug, Clone)]
pub struct GradedPattern {
    pub term: String,
    pub realization: SlotRealization,
    pub match_count: usize,
    /// Per-sampled-sentence verdicts: (sentence, bound words, uses the idiom).
    pub verdicts: Vec<(String, Vec<String>, bool)>,
}

impl GradedPattern {
    pub fn good(&self) -> usize {
        self.verdicts.iter().filter(|(_, _, ok)| *ok).count()
    }

    pub fn total(&self) -> usize {
        self.verdicts.len()
    }

    pub fn precision(&self) -> f64 {
        if self.verdicts.is_empty() {
            0.0
        } else {
            self.good() as f64 / self.total() as f64
        }
    }

    /// Whether the pipeline should keep this realization's matches.
    pub fn kept(&self) -> bool {
        self.total() > 0 && self.precision() >= PRECISION_THRESHOLD
    }

    /// A sampled sentence the grader rejected, for eyeballing why a pattern
    /// was dropped.
    pub fn first_failure(&self) -> Option<&(String, Vec<String>, bool)> {
        self.verdicts.iter().find(|(_, _, ok)| !ok)
    }
}

fn grade_user_prompt(request: &GradeRequest) -> String {
    let listing = request
        .sample
        .iter()
        .enumerate()
        .map(|(i, m)| {
            format!(
                "{i}. {}\n   (the term matched these words: {})",
                m.sentence,
                m.matched_words
                    .iter()
                    .map(|w| format!("\"{w}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Term: \"{}\" (checking its \"{}\" realization)\n\nSentences:\n{listing}",
        request.term, request.realization
    )
}

/// LLM-grade every pattern's sample in one batch — this is the precision gate
/// that lets loosened patterns overmatch structurally without polluting the
/// deck. Patterns whose grading call fails are returned with no verdicts,
/// which reads as "not kept".
pub async fn grade_patterns(requests: &[GradeRequest]) -> anyhow::Result<Vec<GradedPattern>> {
    let requests: Vec<&GradeRequest> = requests.iter().filter(|r| !r.sample.is_empty()).collect();
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let system_prompt = "A language-learning app tags sentences that use a dictionary \
        term (in any inflected or pronominalized form — e.g. with the argument slot \
        filled by a noun phrase or a clitic pronoun). Each numbered sentence below shows \
        which words of the sentence the matcher bound to the term. For each one, answer: \
        do those matched words actually realize the term in its idiom/construction \
        sense? Answer false if the matcher seems to have misunderstood the sentence — \
        the matched words belong to a different sense or a different construction (e.g. \
        for \"arriver à quelqu'un\" = happen to someone, matching \"arriver ... à \
        Paris\" = arrive at a place is wrong). Judge the construction, not the \
        placeholder's animacy: a person filling a \"something\" slot (or vice versa) \
        still counts when the construction is the same.";

    let progress = indicatif::ProgressBar::new(requests.len() as u64);
    progress.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} slot pattern gradings ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let n_requests = requests.len();
    let results = ANALYSIS_CLIENT
        .batch_chat_with_system_prompt_fn::<_, _, GradeResponse>(
            system_prompt,
            &requests,
            |request| grade_user_prompt(request),
            |batch| crate::report_batch_progress(&progress, 0, n_requests, batch),
        )
        .await?;
    progress.finish_with_message(format!("{:.2}", ANALYSIS_CLIENT.cost().unwrap_or(0.0)));

    Ok(results
        .into_iter()
        .map(|(request, response)| {
            let verdicts = response
                .map(|response| {
                    response
                        .verdicts
                        .into_iter()
                        .filter_map(|v| {
                            let m = request.sample.get(v.index)?;
                            Some((m.sentence.clone(), m.matched_words.clone(), v.uses_idiom))
                        })
                        .collect()
                })
                .unwrap_or_default();
            GradedPattern {
                term: request.term.clone(),
                realization: request.realization,
                match_count: request.match_count,
                verdicts,
            }
        })
        .collect())
}

/// Write the per-pattern grading results to a TSV, so a run's slot decisions
/// can be reviewed after the fact instead of scrolling thousands of log lines.
pub fn write_summary(path: &std::path::Path, graded: &[GradedPattern]) -> anyhow::Result<()> {
    let mut sorted: Vec<&GradedPattern> = graded.iter().collect();
    sorted.sort_by(|a, b| {
        b.match_count
            .cmp(&a.match_count)
            .then_with(|| a.term.cmp(&b.term))
    });

    let mut out = String::from(
        "term\trealization\tmatches\tgood\ttotal\tprecision\tkept\texample_failure\texample_failure_bound_words\n",
    );
    for g in sorted {
        let (failure, bound) = match g.first_failure() {
            Some((sentence, words, _)) => (sentence.replace('\t', " "), words.join(" … ")),
            None => (String::new(), String::new()),
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{}\n",
            g.term.replace('\t', " "),
            g.realization,
            g.match_count,
            g.good(),
            g.total(),
            g.precision(),
            if g.kept() { "yes" } else { "no" },
            failure,
            bound,
        ));
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexide::{Lemma, Text};

    fn tok(
        text: &str,
        lemma: &str,
        pos: PartOfSpeech,
        dep: DependencyRelation,
        head: i32,
    ) -> Token {
        Token {
            text: Text {
                text: text.to_string(),
            },
            whitespace: " ".to_string(),
            pos,
            lemma: Lemma {
                lemma: lemma.to_string(),
            },
            dep,
            head,
        }
    }

    /// "arriver à quelqu'un": arriver(root) <- quelqu'un(obl) <- à(case)
    fn arriver_a_quelquun() -> Vec<Token> {
        vec![
            tok(
                "arriver",
                "arriver",
                PartOfSpeech::Verb,
                DependencyRelation::Root,
                0,
            ),
            tok("à", "à", PartOfSpeech::Adp, DependencyRelation::Case, 3),
            tok(
                "quelqu'un",
                "quelqu'un",
                PartOfSpeech::Pron,
                DependencyRelation::Obl,
                1,
            ),
        ]
    }

    fn dative_slot() -> SlotSpec {
        SlotSpec {
            token_index: 2,
            is_slot: true,
            role: SlotRole::CaseMarkedArgument,
            clitic_pronoun_lemmas: ["me", "te", "lui", "nous", "vous", "leur", "se"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    #[test]
    fn compiles_filled_and_clitic_realizations() {
        let tokens = arriver_a_quelquun();
        let realizations = compile_realizations(&tokens, &[dative_slot()]);
        let kinds: Vec<SlotRealization> = realizations.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![SlotRealization::Filled, SlotRealization::Clitic]
        );

        let (_, filled) = &realizations[0];
        // root anchor is the literal verb
        assert_eq!(filled.matcher, NodeMatcher::Lemma("arriver".to_string()));
        // obl child is a nominal wildcard that keeps the case marker
        let (deps, slot_node) = &filled.children[0];
        assert!(deps.contains(&DependencyRelation::Obl));
        assert!(matches!(slot_node.matcher, NodeMatcher::AnyPos(_)));
        assert_eq!(
            slot_node.children[0].1.matcher,
            NodeMatcher::Lemma("à".to_string())
        );

        let (_, clitic) = &realizations[1];
        let (deps, pron_node) = &clitic.children[0];
        assert!(deps.contains(&DependencyRelation::Iobj));
        assert!(deps.contains(&DependencyRelation::Obj));
        match &pron_node.matcher {
            NodeMatcher::LemmaSet { lemmas, pos } => {
                assert!(lemmas.contains("leur"));
                assert!(pos.contains(&PartOfSpeech::Pron));
            }
            other => panic!("expected LemmaSet, got {other:?}"),
        }
        // the case marker is gone in the clitic realization
        assert_eq!(clitic.children.len(), 1);
    }

    #[test]
    fn clitic_realization_matches_leur_est_arrive() {
        use lexide::Tokenization;
        use lexide::matching::{DependencyMatcher, TreeNode};

        let tokens = arriver_a_quelquun();
        let realizations = compile_realizations(&tokens, &[dative_slot()]);
        let (_, clitic) = realizations
            .iter()
            .find(|(k, _)| *k == SlotRealization::Clitic)
            .unwrap();

        // "ce qui leur est arrivé" (simplified): qui(nsubj) leur(iobj) est(aux) arrivé(root)
        let sentence = Tokenization {
            tokens: vec![
                tok(
                    "qui",
                    "qui",
                    PartOfSpeech::Pron,
                    DependencyRelation::Nsubj,
                    4,
                ),
                tok(
                    "leur",
                    "leur",
                    PartOfSpeech::Pron,
                    DependencyRelation::Iobj,
                    4,
                ),
                tok("est", "être", PartOfSpeech::Aux, DependencyRelation::Aux, 4),
                tok(
                    "arrivé",
                    "arriver",
                    PartOfSpeech::Verb,
                    DependencyRelation::Root,
                    0,
                ),
            ],
        };
        let tree = TreeNode::try_from(sentence).unwrap();
        let matcher =
            DependencyMatcher::new(&[("arriver à quelqu'un".to_string(), clitic.clone())]);
        assert_eq!(matcher.find_all(&tree).len(), 1);
    }

    #[test]
    fn slot_at_root_compiles_to_nothing() {
        // A bare placeholder like "quelqu'un" must not become a wildcard that
        // matches every nominal in the corpus.
        let tokens = vec![tok(
            "quelqu'un",
            "quelqu'un",
            PartOfSpeech::Pron,
            DependencyRelation::Root,
            0,
        )];
        let slot = SlotSpec {
            token_index: 0,
            is_slot: true,
            role: SlotRole::Other,
            clitic_pronoun_lemmas: vec![],
        };
        assert!(compile_realizations(&tokens, &[slot]).is_empty());
    }

    #[test]
    fn uncliticizable_slot_skips_clitic_realization() {
        let tokens = arriver_a_quelquun();
        let slot = SlotSpec {
            clitic_pronoun_lemmas: vec![],
            ..dative_slot()
        };
        let realizations = compile_realizations(&tokens, &[slot]);
        assert_eq!(realizations.len(), 1);
        assert_eq!(realizations[0].0, SlotRealization::Filled);
    }

    /// "answer someone's prayers": answer(root) <- prayers(obj) <- someone's(nmod:poss)
    fn answer_someones_prayers() -> Vec<Token> {
        vec![
            tok(
                "answer",
                "answer",
                PartOfSpeech::Verb,
                DependencyRelation::Root,
                0,
            ),
            tok(
                "someone",
                "someone",
                PartOfSpeech::Pron,
                DependencyRelation::NmodPoss,
                3,
            ),
            tok(
                "prayers",
                "prayer",
                PartOfSpeech::Noun,
                DependencyRelation::Obj,
                1,
            ),
        ]
    }

    fn possessive_slot() -> SlotSpec {
        SlotSpec {
            token_index: 1,
            is_slot: true,
            role: SlotRole::Possessive,
            clitic_pronoun_lemmas: ["my", "your", "his", "her", "its", "our", "their"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    #[test]
    fn possessive_slot_accepts_the_whole_det_nmod_family() {
        // Parsers label determiner possessives as either `det` or `det:poss`,
        // and nominal possessives as `nmod:poss`. The citation form's own
        // label must not decide which of those a real sentence may use.
        let realizations = compile_realizations(&answer_someones_prayers(), &[possessive_slot()]);
        for (realization, pattern) in &realizations {
            let slot_edge = pattern
                .children
                .iter()
                .flat_map(|(_, child)| child.children.iter())
                .map(|(deps, _)| deps)
                .next()
                .unwrap_or_else(|| panic!("{realization} pattern has no slot edge"));
            for dep in [
                DependencyRelation::Det,
                DependencyRelation::DetPoss,
                DependencyRelation::NmodPoss,
            ] {
                assert!(
                    slot_edge.contains(&dep),
                    "{realization} slot edge is missing {dep:?}: {slot_edge:?}"
                );
            }
        }
    }

    #[test]
    fn possessive_clitic_matches_a_det_poss_possessor() {
        use lexide::matching::{DependencyMatcher, TreeNode};

        let realizations = compile_realizations(&answer_someones_prayers(), &[possessive_slot()]);
        let (_, clitic) = realizations
            .iter()
            .find(|(k, _)| *k == SlotRealization::Clitic)
            .expect("possessive slots cliticize");

        // "answered her prayers", with `her` labelled det:poss — the label
        // that the pre-fix pattern (which only accepted `det`) missed.
        let sentence = lexide::Tokenization {
            tokens: vec![
                tok(
                    "answered",
                    "answer",
                    PartOfSpeech::Verb,
                    DependencyRelation::Root,
                    0,
                ),
                tok(
                    "her",
                    "her",
                    PartOfSpeech::Det,
                    DependencyRelation::DetPoss,
                    3,
                ),
                tok(
                    "prayers",
                    "prayer",
                    PartOfSpeech::Noun,
                    DependencyRelation::Obj,
                    1,
                ),
            ],
        };
        let tree = TreeNode::try_from(sentence).unwrap();
        let matcher =
            DependencyMatcher::new(&[("answer someone's prayers".to_string(), clitic.clone())]);
        assert_eq!(matcher.find_all(&tree).len(), 1);
    }

    #[test]
    fn malformed_citation_parse_compiles_to_nothing() {
        // An orphaned token (head points past the end) means the parse is not
        // a tree; compiling it would yield a partial, overly broad pattern.
        let mut tokens = arriver_a_quelquun();
        tokens[1].head = 99;
        assert!(compile_realizations(&tokens, &[dative_slot()]).is_empty());
    }

    #[test]
    fn sample_matches_is_deterministic_and_spread() {
        let matches: Vec<SlotMatch> = (0..100)
            .map(|i| SlotMatch {
                sentence: format!("sentence {i:03}"),
                matched_token_indices: vec![i],
                matched_words: vec![format!("word{i}")],
            })
            .collect();
        let a = sample_matches(&matches, 12);
        let b = sample_matches(&matches, 12);
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
        let few = sample_matches(&matches[..5], 12);
        assert_eq!(few.len(), 5);
    }
}
