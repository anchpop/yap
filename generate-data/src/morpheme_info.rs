//! Post-etymology pass: classify every morpheme as Free / Bound / Derivational /
//! Inflectional, then resolve each one — Free morphemes get mapped back to a
//! dictionary entry (word + lemma + POS), while Bound / Derivational /
//! Inflectional morphemes get a short learner-facing gloss.
//!
//! Input: the surface segmentations produced by the etymology pass (word →
//! morphemes). We invert that into morpheme → words-it-appears-in, show the
//! model a stable sample of those words, and ask it to classify the morpheme.

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Heteronym, Language, MorphemeInfo, MorphemeSegment, PartOfSpeech};
use sentence_sampler::sample_to_target;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

static CLASSIFY_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4-mini")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

static DEFINE_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum MorphemeCategory {
    /// A morpheme that can stand alone as a word (e.g. English `stable`,
    /// Korean `먹다` as a canonical form).
    Free,
    /// A content morpheme that can't stand alone and isn't a standard affix
    /// (e.g. English `-cide`, `-ology`, Korean `-학` in 생물학).
    Bound,
    /// An affix that changes meaning or part of speech (e.g. English `un-`,
    /// `-ize`; Korean `-하다`, `-화`).
    Derivational,
    /// An affix that marks grammatical function (e.g. English plural `-s`,
    /// past `-ed`; Korean `-아`, `-요`).
    Inflectional,
}

/// Intermediate result of the morpheme-analysis pass — pairs the final
/// `MorphemeInfo` enum (what gets stored in the language pack) with the sampled
/// example words that drove the classification (for JSONL debug output).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MorphemeAnalysis {
    pub segment: MorphemeSegment<String>,
    pub kind: MorphemeInfo<String>,
    pub example_words: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct ClassifyResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    #[serde(rename = "2. category")]
    category: MorphemeCategory,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct DefineResponse {
    /// Short learner-facing gloss, ideally a single word or short phrase.
    /// Return null when the morpheme has no useful synchronic meaning (e.g.
    /// English `cran-` in `cranberry`, a pure morphological relic).
    definition: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct LookupEntry {
    /// The dictionary word — the form a learner would actually look up.
    #[serde(rename = "1. word")]
    word: String,
    /// The lemma (usually identical to `word` for root morphemes).
    #[serde(rename = "2. lemma")]
    lemma: String,
    #[serde(rename = "3. pos")]
    pos: PartOfSpeech,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct LookupResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// The dictionary entry this morpheme corresponds to. Return null if the
    /// morpheme doesn't cleanly map to a single dictionary word.
    #[serde(rename = "2. entry")]
    entry: Option<LookupEntry>,
}

/// Per-language reference examples for each category. The match is exhaustive
/// so the compiler will flag any `Language` variant that doesn't have coverage.
fn language_examples(language: Language) -> &'static str {
    match language {
        Language::English => {
            r#"- Free: "stable", "book", "happy", "go"
- Bound: "-cide" (killer; genocide, pesticide), "-ology" (study of; biology), "-graph" (writing; photograph), "-scope" (view; telescope)
- Derivational: "un-" (unhappy), "-ize" (verb-forming; stabilize), "-able" (drinkable), "-ness" (happiness)
- Inflectional: plural "-s", past "-ed", progressive "-ing", comparative "-er""#
        }
        Language::French => {
            r#"- Free: "chat", "rouge", "aller"
- Bound: "-logie" (science; biologie), "-cide" (génocide), "-phobie" (claustrophobie)
- Derivational: "re-" (again), "-ation" (N-forming), "-able", "-ment" (adv-forming), "-té"
- Inflectional: plural "-s", feminine "-e", verb endings "-ons", "-ez", "-ent", "-ais", "-era""#
        }
        Language::Spanish => {
            r#"- Free: "casa", "rojo", "comer"
- Bound: "-logía", "-cidio", "-fobia"
- Derivational: "re-", "des-", "-ción", "-mente", "-dor", "-ito" (diminutive)
- Inflectional: plural "-s", feminine "-a", verb endings "-o", "-amos", "-aste", "-ado""#
        }
        Language::Italian => {
            r#"- Free: "casa", "rosso", "mangiare"
- Bound: "-logia", "-cidio", "-fobia"
- Derivational: "ri-" (again), "-zione", "-mente", "-ino" (diminutive), "-ista"
- Inflectional: plural "-i"/"-e", feminine "-a", verb endings "-o", "-iamo", "-ato", "-ava""#
        }
        Language::Portuguese => {
            r#"- Free: "casa", "vermelho", "comer"
- Bound: "-logia", "-cídio", "-fobia"
- Derivational: "re-", "des-", "-ção", "-mente", "-inho" (diminutive)
- Inflectional: plural "-s", feminine "-a", verb endings "-o", "-amos", "-ado""#
        }
        Language::German => {
            r#"- Free: "Buch", "rot", "gehen"
- Bound: "-logie", "-phobie", "-graph"
- Derivational: "un-", "-ung" (N-forming), "-keit", "-lich", "-bar"
- Inflectional: case/number endings "-e", "-en", "-er"; verb endings "-st", "-te""#
        }
        Language::Russian => {
            r#"- Free: "дом", "красный", "идти"
- Bound: "-логия", "-фобия", "-граф"
- Derivational: prefixes "про-", "пере-", "под-"; suffixes "-ник", "-ский", "-ость"
- Inflectional: case/gender endings "-ый", "-ая", "-ое", "-ов", "-ам"; verb endings "-ю", "-ешь", "-ли""#
        }
        Language::Korean => {
            r#"- Free: "먹다", "있다", "학교", "사랑" (dictionary forms and standalone nouns)
- Bound: "-학" (study of, 學; 생물학, 철학), "-론" (theory; 진화론), "-炎" (-itis)
- Derivational: "-하다" (verbalizer; 공부하다), "-화" (-ization; 현대화), "-적" (-ic; 사회적), "-자" (agent; 학자)
- Inflectional: "-아", "-어" (connective ending), "-았", "-었" (past), "-요" (politeness), "-ㄴ"/"-은" (attributive), "-다" (declarative)"#
        }
        Language::ChineseSimplified | Language::ChineseTraditional => {
            r#"- Free: "吃", "好", "学生", "学校"
- Bound: "-炎" (-itis; 肺炎), "-症" (condition; 癌症)
- Derivational: "-化" (-ization; 现代化), "-者" (-er; 学者), "-性" (-ness; 可能性), "-的" (attributive)
- Inflectional: "了" (perfective), "着" (continuous), "过" (experiential), "们" (plural)"#
        }
        Language::Japanese => {
            r#"- Free: "食べる", "学校", "赤"
- Bound: "-学" (study of; 生物学), "-症" (condition)
- Derivational: "-化" (-ization; 近代化), "-的" (-ic; 社会的), "-者" (agent; 学者), "-さ" (N-forming; 大きさ)
- Inflectional: "-た" (past), "-ます" (polite), "-ない" (negative), "-て" (connective), "-る" (non-past)"#
        }
        Language::Hindi => {
            r#"- Free: "घर" (home), "लाल" (red), "खाना" (food/eat)
- Bound: "-विज्ञान" (science), "-शास्त्र" (study of)
- Derivational: "-वाला" (doer/possessor), "-पन" (N-forming), "-ी" (diminutive/feminine)
- Inflectional: "-ना" (infinitive), "-ता" (habitual masc), "-ती" (habitual fem), "-ए" (perfective pl)"#
        }
    }
}

/// Invert `word → segments` into `morpheme → words containing it`, keeping
/// only morphemes that appear in at least one multi-segment word. A morpheme
/// that *only* shows up as a whole word on its own doesn't need separate
/// morpheme-level info — the regular dictionary already covers it.
pub fn invert_segmentations(
    segmentations: &BTreeMap<String, Vec<MorphemeSegment<String>>>,
) -> BTreeMap<MorphemeSegment<String>, Vec<String>> {
    // Morphemes that participate in at least one compound word.
    let in_compound: std::collections::HashSet<&MorphemeSegment<String>> = segmentations
        .iter()
        .filter(|(_, segs)| segs.len() >= 2)
        .flat_map(|(_, segs)| segs.iter())
        .collect();

    let mut out: BTreeMap<MorphemeSegment<String>, Vec<String>> = BTreeMap::new();
    for (word, segments) in segmentations {
        for seg in segments {
            if in_compound.contains(seg) {
                out.entry(seg.clone()).or_default().push(word.clone());
            }
        }
    }
    out
}

/// Pick a stable sample of up to `n` words from `words`.
fn pick_example_words(words: &[String], n: usize) -> Vec<String> {
    if words.len() <= n {
        let mut out = words.to_vec();
        out.sort();
        return out;
    }
    // Over-sample to reduce the variance of `sample_to_target`, then sort and
    // truncate so the prompt (and thus the tysm cache key) is stable.
    let mut sampled = sample_to_target(words.to_vec(), n * 2, |w| w.clone());
    sampled.sort();
    sampled.truncate(n);
    sampled
}

async fn classify_one(
    system_prompt: &str,
    segment: &MorphemeSegment<String>,
    example_words: &[String],
) -> Option<MorphemeCategory> {
    let tag_line = segment
        .tag
        .as_ref()
        .map(|t| {
            format!(
                "\ngrammatical tag: {t}  (UniMorph-style descriptor identifying this morpheme instance's specific grammatical function — use it to disambiguate from other uses of the same surface/canonical)"
            )
        })
        .unwrap_or_default();
    let user = format!(
        "morpheme surface: {surface}\ncanonical / dictionary form: {canonical}{tag_line}\nexample words: {examples}",
        surface = segment.surface,
        canonical = segment.canonical,
        examples = example_words.join(", "),
    );
    let response: Result<ClassifyResponse, _> = CLASSIFY_CLIENT
        .chat_with_system_prompt(system_prompt, user)
        .await;
    response.ok().map(|r| r.category)
}

fn pos_str(pos: PartOfSpeech) -> String {
    serde_json::to_value(pos)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// Per-language worked examples of the dictionary-lookup task. Exhaustive
/// match so the compiler flags any missing `Language` variant.
fn lookup_examples(language: Language) -> &'static str {
    match language {
        Language::English => {
            r#"- morpheme "stabl" with candidates "stable (lemma: stable, pos: ADJ)", "stabilize (lemma: stabilize, pos: VERB)", "stables" → {"word": "stable", "lemma": "stable", "pos": "ADJ"}
- morpheme "book" with candidates "book (lemma: book, pos: NOUN)", "books", "booking" → {"word": "book", "lemma": "book", "pos": "NOUN"}"#
        }
        Language::French => {
            r#"- morpheme "mang" with candidates "manger (lemma: manger, pos: VERB)", "mange", "manges" → {"word": "manger", "lemma": "manger", "pos": "VERB"}
- morpheme "heureu" with candidates "heureux (lemma: heureux, pos: ADJ)", "heureuse" → {"word": "heureux", "lemma": "heureux", "pos": "ADJ"}
- morpheme "chat" with candidates "chat (lemma: chat, pos: NOUN)", "chats" → {"word": "chat", "lemma": "chat", "pos": "NOUN"}"#
        }
        Language::Spanish => {
            r#"- morpheme "habl" with candidates "hablar (lemma: hablar, pos: VERB)", "habla", "hablamos" → {"word": "hablar", "lemma": "hablar", "pos": "VERB"}
- morpheme "cas" with candidates "casa (lemma: casa, pos: NOUN)", "casas" → {"word": "casa", "lemma": "casa", "pos": "NOUN"}"#
        }
        Language::Italian => {
            r#"- morpheme "parl" with candidates "parlare (lemma: parlare, pos: VERB)", "parla", "parlano" → {"word": "parlare", "lemma": "parlare", "pos": "VERB"}
- morpheme "cas" with candidates "casa (lemma: casa, pos: NOUN)", "case" → {"word": "casa", "lemma": "casa", "pos": "NOUN"}"#
        }
        Language::Portuguese => {
            r#"- morpheme "fal" with candidates "falar (lemma: falar, pos: VERB)", "fala", "falamos" → {"word": "falar", "lemma": "falar", "pos": "VERB"}
- morpheme "cas" with candidates "casa (lemma: casa, pos: NOUN)" → {"word": "casa", "lemma": "casa", "pos": "NOUN"}"#
        }
        Language::German => {
            r#"- morpheme "geh" with candidates "gehen (lemma: gehen, pos: VERB)", "gehe", "gehst" → {"word": "gehen", "lemma": "gehen", "pos": "VERB"}
- morpheme "Haus" with candidates "Haus (lemma: Haus, pos: NOUN)", "Hauses" → {"word": "Haus", "lemma": "Haus", "pos": "NOUN"}"#
        }
        Language::Russian => {
            r#"- morpheme "говор" with candidates "говорить (lemma: говорить, pos: VERB)", "говорю", "говоришь" → {"word": "говорить", "lemma": "говорить", "pos": "VERB"}
- morpheme "дом" with candidates "дом (lemma: дом, pos: NOUN)", "дома" → {"word": "дом", "lemma": "дом", "pos": "NOUN"}"#
        }
        Language::Korean => {
            r#"- morpheme "괜찮" with candidates "괜찮아요", "괜찮은", "괜찮음" (note: the canonical dictionary form "괜찮다" isn't among the candidates) → {"word": "괜찮다", "lemma": "괜찮다", "pos": "ADJ"}
- morpheme "먹" with candidates "먹다 (lemma: 먹다, pos: VERB)", "먹어요" → {"word": "먹다", "lemma": "먹다", "pos": "VERB"}"#
        }
        Language::ChineseSimplified | Language::ChineseTraditional => {
            r#"- morpheme "吃" with candidates "吃 (lemma: 吃, pos: VERB)", "吃饭" → {"word": "吃", "lemma": "吃", "pos": "VERB"}
- morpheme "学生" with candidates "学生 (lemma: 学生, pos: NOUN)" → {"word": "学生", "lemma": "学生", "pos": "NOUN"}"#
        }
        Language::Japanese => {
            r#"- morpheme "食べ" with candidates "食べる (lemma: 食べる, pos: VERB)", "食べた", "食べて" → {"word": "食べる", "lemma": "食べる", "pos": "VERB"}
- morpheme "赤" with candidates "赤い (lemma: 赤い, pos: ADJ)" → {"word": "赤い", "lemma": "赤い", "pos": "ADJ"}"#
        }
        Language::Hindi => {
            r#"- morpheme "खा" with candidates "खाना (lemma: खाना, pos: VERB)", "खाता" → {"word": "खाना", "lemma": "खाना", "pos": "VERB"}
- morpheme "लाल" with candidates "लाल (lemma: लाल, pos: ADJ)" → {"word": "लाल", "lemma": "लाल", "pos": "ADJ"}"#
        }
    }
}

/// Trie-style prefix lookup: find up to `limit` dictionary words that start
/// with `morpheme`, each paired with lemma + POS. This lets the LLM see real
/// dictionary candidates — e.g. for `stabl` it will see `stable (lemma: stable,
/// pos: ADJ)` and `stabilize (lemma: stabilize, pos: VERB)`.
fn prefix_candidates(
    word_info: &BTreeMap<String, (String, PartOfSpeech)>,
    morpheme: &str,
    limit: usize,
) -> Vec<(String, String, PartOfSpeech)> {
    let mut out = Vec::new();
    for (word, (lemma, pos)) in word_info.range(morpheme.to_string()..) {
        if !word.starts_with(morpheme) {
            break;
        }
        out.push((word.clone(), lemma.clone(), *pos));
        if out.len() >= limit {
            break;
        }
    }
    out
}

async fn lookup_one(
    language: Language,
    segment: &MorphemeSegment<String>,
    prefix_candidates: &[(String, String, PartOfSpeech)],
    example_words: &[String],
) -> Option<Heteronym<String>> {
    // Prefer trie-derived prefix candidates since they carry lemma + POS. Fall
    // back to the raw example words if the prefix search came up empty.
    let candidates_block = if !prefix_candidates.is_empty() {
        prefix_candidates
            .iter()
            .map(|(w, l, p)| format!("  - {w} (lemma: {l}, pos: {})", pos_str(*p)))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        example_words
            .iter()
            .map(|w| format!("  - {w}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let examples = lookup_examples(language);
    let system_prompt = format!(
        r#"You are an expert {language} morphologist. You'll be given a {language} morpheme (surface form plus its canonical / lemma hint) and a list of candidate dictionary words (words that start with this morpheme, each with their lemma + POS). Return the dictionary entry this morpheme corresponds to.

The morpheme is often the bound surface form of a word — return the free-standing dictionary form. The canonical hint tells you what dictionary form the segmentation intended, so let it disambiguate cases where the surface alone could map to several lemmas.

Prefer a `word` that appears in the candidate list when one is the right lemma. Otherwise return the lemma that those surface forms inflect from, even if it doesn't appear in the candidates.

Return null for `entry` if the morpheme doesn't cleanly correspond to a single dictionary word.

Worked examples for {language}:
{examples}"#
    );
    let tag_line = segment
        .tag
        .as_ref()
        .map(|t| {
            format!(
                "\ngrammatical tag: {t}  (UniMorph-style descriptor identifying this morpheme instance's specific grammatical function — use it to disambiguate from other uses of the same surface/canonical)"
            )
        })
        .unwrap_or_default();
    let user = format!(
        "morpheme surface: {surface}\ncanonical / dictionary form: {canonical}{tag_line}\ncandidate words:\n{candidates_block}",
        surface = segment.surface,
        canonical = segment.canonical,
    );
    let response: Result<LookupResponse, _> = DEFINE_CLIENT
        .chat_with_system_prompt(system_prompt, user)
        .await;
    response.ok().and_then(|r| r.entry).map(|e| Heteronym {
        word: e.word,
        lemma: e.lemma,
        pos: e.pos,
    })
}

/// The learner's native-language word for "conjugation". Threaded into the
/// tag-aware guidance so the gloss is phrased in a term the learner actually
/// knows, and the model doesn't confuse it with grammar terminology from some
/// other language.
fn conjugation_in(native_language: Language) -> &'static str {
    match native_language {
        Language::English => "conjugation",
        Language::French => "conjugaison",
        Language::Spanish => "conjugación",
        Language::Italian => "coniugazione",
        Language::Portuguese => "conjugação",
        Language::German => "Konjugation",
        Language::Russian => "спряжение",
        Language::Korean => "활용",
        Language::ChineseSimplified => "变位",
        Language::ChineseTraditional => "變位",
        Language::Japanese => "活用",
        Language::Hindi => "संयुग्मन",
    }
}

/// When a morpheme arrives with a grammatical tag, the naive translation of the
/// Leipzig descriptor into native-language text is usually terrible for
/// learners ("1st/2nd singular present indicative..."). Tell the model to
/// re-cast the tag using concrete target-language surface markers (pronouns,
/// affixes) and learner-friendly terms.
fn tag_define_guidance(native_language: Language) -> String {
    let conjugation = conjugation_in(native_language);
    format!(
        r#"If the tag marks a {conjugation} or a derivation, please don't output the Leipzig-style tag directly — learners can't parse labels like "1SG;2SG.PRS.IND". Translate it into concrete, learner-friendly terms. Please don't quote the affix itself in the label (e.g. no "-요", "-ed", "-ment") — the learner already sees the affix in the UI, so the label just needs to describe its function. Pronouns like "je/tu" are fine when they make the description clearer. Examples:
- (french example) "je/tu {conjugation}" rather than "1st/2nd person singular present"
- "polite ending" rather than "POLITE"
- "past participle" rather than "PTCP.PST"
- "adverb marker" rather than "ADV derivation"
Prefer a terse (but still informative) label over an exhaustive list of every syncretic reading. These labels are for students studying a language, not linguists. If a word can be used for 1st person and 2nd person, that's important, but (for example) if a word can be used for the present tense and also the prophetic future tense, the present tense is the only one that really needs to be mentioned.

Note that you can trust the tags to be accurate. If a tag says the suffix is used for the present tense and the future tense, please trust it, even if that's not universally true. The app will only show your response to the user in cases where the tag applies."#
    )
}

async fn define_one(
    course: Course,
    segment: &MorphemeSegment<String>,
    category: MorphemeCategory,
    example_words: &[String],
) -> Option<String> {
    let Course {
        target_language: language,
        native_language,
    } = course;
    let category_guidance = match category {
        MorphemeCategory::Bound => {
            "This is a bound root — a content morpheme that can't stand alone as a word (like English \"-cide\" or \"-ology\"). Return a short meaning gloss — ideally a single word or short phrase."
        }
        MorphemeCategory::Derivational => {
            "This is a derivational affix — one that changes meaning or part of speech (like English \"-ize\" or \"un-\"). Return a short gloss summarizing what it does — a word or short phrase."
        }
        MorphemeCategory::Inflectional => {
            "This is an inflectional affix — a grammatical marker (like plural, past tense, politeness). Return a terse grammatical label — the equivalent of \"Plural\", \"Past tense\", \"1sg present\", \"Polite\"."
        }
        MorphemeCategory::Free => return None,
    };
    // Only append tag-aware guidance when the segment actually carries a tag.
    // This splits the system-prompt cache into two variants per language pair
    // (tagged vs. untagged) but keeps the untagged variant identical to the
    // pre-existing prompt so its cache entries stay warm.
    let tag_guidance_block = if segment.tag.is_some() {
        format!("\n\n{}", tag_define_guidance(native_language))
    } else {
        String::new()
    };
    let system_prompt = format!(
        r#"You are an expert {language} morphologist. You'll be given a {language} morpheme (surface form plus its canonical / lemma hint) and a few example words it appears in. The canonical hint disambiguates the morpheme when multiple underlying morphemes share the same surface.

{category_guidance}{tag_guidance_block}

Write the gloss in {native_language}.

Return null for the `definition` field if the morpheme has no useful synchronic meaning (e.g. English "cran-" in "cranberry" is a historical relic with no independent meaning today)."#
    );
    let tag_line = segment
        .tag
        .as_ref()
        .map(|t| {
            format!(
                "\ngrammatical tag: {t}  (UniMorph-style descriptor identifying this morpheme instance's specific grammatical function — use it to disambiguate from other uses of the same surface/canonical)"
            )
        })
        .unwrap_or_default();
    let user = format!(
        "morpheme surface: {surface}\ncanonical / dictionary form: {canonical}{tag_line}\nexample words: {examples}",
        surface = segment.surface,
        canonical = segment.canonical,
        examples = example_words.join(", "),
    );
    let response: Result<DefineResponse, _> = DEFINE_CLIENT
        .chat_with_system_prompt(system_prompt, user)
        .await;
    response.ok().and_then(|r| r.definition)
}

/// Classify every morpheme, then resolve it: Free morphemes get mapped to a
/// dictionary entry (word/lemma/POS), others get a short gloss. Returns a
/// sorted list (by morpheme string).
///
/// `word_info` is a dictionary from surface word → (lemma, POS) and is used
/// as a "trie" to find real dictionary words starting with each morpheme, so
/// the lookup LLM call can see candidate entries with their POS.
pub async fn analyze_morphemes(
    course: Course,
    morpheme_to_words: &BTreeMap<MorphemeSegment<String>, Vec<String>>,
    word_info: &BTreeMap<String, (String, PartOfSpeech)>,
) -> Vec<MorphemeAnalysis> {
    let language = course.target_language;
    let examples_block = language_examples(language);
    let classify_prompt = format!(
        r#"You are an expert {language} morphologist. Classify the given {language} morpheme as one of:
- "free": can stand alone as a word (for languages like Korean, dictionary/canonical forms like "먹다" count as free even when the surface stem "먹" is bound)
- "bound": content morpheme that can't stand alone (like English "-cide", "-ology")
- "derivational": affix that changes meaning or part of speech (like English "un-", "-ize")
- "inflectional": affix marking grammatical function (like English plural "-s", past "-ed")

Reference classifications for {language}:
{examples_block}

You'll receive a morpheme plus a small sample of words it appears in. Pick the single best category."#
    );

    // Phase 1: classify every morpheme.
    let count = morpheme_to_words.len();
    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                 {pos}/{len} classifying morphemes ({per_sec}, ${msg}, {eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let classified: Vec<(MorphemeSegment<String>, MorphemeCategory, Vec<String>)> =
        futures::stream::iter(morpheme_to_words.iter())
            .map(|(segment, words)| {
                let pb = pb.clone();
                let classify_prompt = classify_prompt.clone();
                async move {
                    let examples = pick_example_words(words, 10);
                    let category = classify_one(&classify_prompt, segment, &examples).await?;
                    pb.set_message(format!("{:.2}", CLASSIFY_CLIENT.cost().unwrap_or(0.0)));
                    pb.inc(1);
                    Some((segment.clone(), category, examples))
                }
            })
            .buffer_unordered(50)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flatten()
            .collect();

    pb.finish_with_message(format!("{:.2}", CLASSIFY_CLIENT.cost().unwrap_or(0.0)));

    // Phase 2: resolve each morpheme — Free morphemes get a dictionary lookup,
    // Bound / Derivational / Inflectional get a short gloss.
    let pb = ProgressBar::new(classified.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                 {pos}/{len} resolving morphemes ({per_sec}, ${msg}, {eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut results: Vec<MorphemeAnalysis> = futures::stream::iter(classified)
        .map(|(segment, category, example_words)| {
            let pb = pb.clone();
            async move {
                let kind = match category {
                    MorphemeCategory::Free => {
                        let candidates = prefix_candidates(word_info, &segment.surface, 15);
                        match lookup_one(language, &segment, &candidates, &example_words).await {
                            Some(heteronym) => MorphemeInfo::Root { heteronym },
                            // Fallback: the LLM couldn't resolve a dictionary entry for this
                            // Free-classified morpheme. Store it as a bound morpheme with no
                            // gloss rather than dropping it entirely.
                            None => MorphemeInfo::Bound { meaning: None },
                        }
                    }
                    MorphemeCategory::Bound => {
                        let def = define_one(course, &segment, category, &example_words).await;
                        MorphemeInfo::Bound { meaning: def }
                    }
                    MorphemeCategory::Derivational => {
                        let def = define_one(course, &segment, category, &example_words).await;
                        MorphemeInfo::Derivation { meaning: def }
                    }
                    MorphemeCategory::Inflectional => {
                        let def = define_one(course, &segment, category, &example_words).await;
                        MorphemeInfo::Inflection { meaning: def }
                    }
                };
                pb.set_message(format!("{:.2}", DEFINE_CLIENT.cost().unwrap_or(0.0)));
                pb.inc(1);
                MorphemeAnalysis {
                    segment,
                    kind,
                    example_words,
                }
            }
        })
        .buffer_unordered(30)
        .collect::<Vec<_>>()
        .await;

    pb.finish_with_message(format!("{:.2}", DEFINE_CLIENT.cost().unwrap_or(0.0)));

    results.sort_by(|a, b| a.segment.cmp(&b.segment));
    results
}
