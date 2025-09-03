use std::{collections::BTreeMap, num::NonZeroUsize};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub fn strip_punctuation(text: &str) -> &str {
    text.trim_matches(|c| matches!(c, '.' | ',' | '!' | '?' | ':' | ';' | '-'))
}

/// Normalizes Spanish words by removing punctuation and converting to lowercase
pub fn expand_spanish_word(
    text: &str,
    pos: Option<PartOfSpeech>,
    _morph: &BTreeMap<String, String>,
) -> Option<(String, Option<String>, Option<PartOfSpeech>)> {
    Some({
        let text = strip_punctuation(text);

        if text.is_empty() {
            return None;
        }

        if ["'", "-", "—", "–", "'", "'"].contains(&text) {
            return None;
        }
        if text.chars().all(|c| c.is_numeric()) {
            return None;
        }

        let text = text.to_lowercase();

        if text == "lo" && pos == Some(PartOfSpeech::Pron) {
            return Some((
                "lo".to_string(),
                Some("lo".to_string()),
                Some(PartOfSpeech::Pron),
            ));
        }

        // expand Spanish contractions
        (text, None, None)
    })
}

/// Expands English contractions to their full forms and normalizes words
pub fn expand_english_word(
    text: &str,
    _pos: Option<PartOfSpeech>,
    _morph: &BTreeMap<String, String>,
) -> Option<(String, Option<String>, Option<PartOfSpeech>)> {
    Some({
        let text = strip_punctuation(text);

        if text.is_empty() {
            return None;
        }

        if ["'", "-", "—", "–", "'", "'"].contains(&text) {
            return None;
        }
        if text.chars().all(|c| c.is_numeric()) {
            return None;
        }

        let text = text.to_lowercase();

        (text, None, None)
    })
}

/// Expands French contractions to their full forms and normalizes words
pub fn expand_french_word(
    text: &str,
    pos: Option<PartOfSpeech>,
    morph: &BTreeMap<String, String>,
) -> Option<(String, Option<String>, Option<PartOfSpeech>)> {
    Some({
        let text = strip_punctuation(text);

        if text.is_empty() {
            return None;
        }

        if ["'", "-", "—", "–", "’", "‘"].contains(&text) {
            return None;
        }
        if text.chars().all(|c| c.is_numeric()) {
            return None;
        }

        let text = text.to_lowercase();

        // expand contractions
        match &text[..] {
            "j'" => (
                "je".to_string(),
                Some("je".to_string()),
                Some(PartOfSpeech::Pron),
            ),
            "m'" => (
                "me".to_string(),
                Some("me".to_string()),
                Some(PartOfSpeech::Pron),
            ),
            "t'" => (
                "te".to_string(),
                Some("te".to_string()),
                Some(PartOfSpeech::Pron),
            ),
            "t" => (
                "t".to_string(),
                Some("t".to_string()),
                Some(PartOfSpeech::Part),
            ),
            "s'" => {
                if pos == Some(PartOfSpeech::Sconj) {
                    (
                        "si".to_string(),
                        Some("si".to_string()),
                        Some(PartOfSpeech::Sconj),
                    )
                } else {
                    (
                        "se".to_string(),
                        Some("se".to_string()),
                        Some(PartOfSpeech::Pron),
                    )
                }
            }
            "c'" => ("ce".to_string(), Some("ce".to_string()), None), // either DET or PRON depending on context
            "n'" => (
                "ne".to_string(),
                Some("ne".to_string()),
                Some(PartOfSpeech::Adv),
            ),
            "l'" => {
                // if we know the gender, use that
                if morph
                    .get("Gender")
                    .map(String::as_str)
                    .unwrap_or("Masculin")
                    == "Feminin"
                {
                    ("la".to_string(), Some("la".to_string()), None) // either DET or PRON depending on context
                } else {
                    ("le".to_string(), Some("le".to_string()), None) // either DET or PRON depending on context
                }
            }
            "de" => (
                "de".to_string(),
                Some("de".to_string()),
                Some(PartOfSpeech::Adp),
            ),
            "d'" => (
                "de".to_string(),
                Some("de".to_string()),
                Some(PartOfSpeech::Adp),
            ),
            "qu'" => ("que".to_string(), Some("que".to_string()), None),
            "quelqu'" => ("quelque".to_string(), Some("quelque".to_string()), None),
            "jusqu'" => (
                "jusque".to_string(),
                Some("jusque".to_string()),
                Some(PartOfSpeech::Adp),
            ),
            "lorsqu'" => (
                "lorsque".to_string(),
                Some("lorsque".to_string()),
                Some(PartOfSpeech::Sconj),
            ),
            "puisqu'" => (
                "puisque".to_string(),
                Some("puisque".to_string()),
                Some(PartOfSpeech::Sconj),
            ),
            "quoiqu'" => (
                "quoique".to_string(),
                Some("quoique".to_string()),
                Some(PartOfSpeech::Sconj),
            ),
            "presqu'" => (
                "presque".to_string(),
                Some("presque".to_string()),
                Some(PartOfSpeech::Adv),
            ),
            _ => (text, None, None),
        }
    })
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Copy,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum PartOfSpeech {
    #[serde(rename = "ADJ")]
    Adj, // adjective
    #[serde(rename = "ADP")]
    Adp, // adposition
    #[serde(rename = "ADV")]
    Adv, // adverb
    #[serde(rename = "AUX")]
    Aux, // auxiliary
    #[serde(rename = "CCONJ")]
    Cconj, // coordinating conjunction
    #[serde(rename = "DET")]
    Det, // determiner
    #[serde(rename = "INTJ")]
    Intj, // interjection
    #[serde(rename = "NOUN")]
    Noun, // noun
    #[serde(rename = "NUM")]
    Num, // numeral
    #[serde(rename = "PART")]
    Part, // particle
    #[serde(rename = "PRON")]
    Pron, // pronoun
    #[serde(rename = "PROPN")]
    Propn, // proper noun
    #[serde(rename = "PUNCT")]
    Punct, // punctuation
    #[serde(rename = "SCONJ")]
    Sconj, // subordinating conjunction
    #[serde(rename = "SYM")]
    Sym, // symbol
    #[serde(rename = "VERB")]
    Verb, // verb
    #[serde(rename = "X")]
    X, // other
}

impl std::fmt::Display for PartOfSpeech {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let word = match self {
            PartOfSpeech::Adj => "adjective",
            PartOfSpeech::Adp => "adposition",
            PartOfSpeech::Adv => "adverb",
            PartOfSpeech::Aux => "auxiliary",
            PartOfSpeech::Cconj => "coordinating conjunction",
            PartOfSpeech::Det => "determiner",
            PartOfSpeech::Intj => "interjection",
            PartOfSpeech::Noun => "noun",
            PartOfSpeech::Num => "numeral",
            PartOfSpeech::Part => "particle",
            PartOfSpeech::Pron => "pronoun",
            PartOfSpeech::Propn => "proper noun",
            PartOfSpeech::Punct => "punctuation",
            PartOfSpeech::Sconj => "subordinating conjunction",
            PartOfSpeech::Sym => "symbol",
            PartOfSpeech::Verb => "verb",
            PartOfSpeech::X => "other",
        };
        write!(f, "{word}")
    }
}

#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    tsify::Tsify,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TargetToNativeWord {
    pub native: String,
    pub note: Option<String>,
    pub example_sentence_target_language: String,
    pub example_sentence_native_language: String,
}

#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    tsify::Tsify,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PhrasebookEntryThoughts {
    pub thoughts: String,
    pub target_language_multi_word_term: String,
    pub meaning: String,
    pub additional_notes: String,
    pub target_language_example: String,
    pub native_language_example: String,
}

#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    tsify::Tsify,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PhrasebookEntry {
    pub target_language_multi_word_term: String,
    pub meaning: String,
    pub additional_notes: String,
    pub target_language_example: String,
    pub native_language_example: String,
}

impl From<PhrasebookEntryThoughts> for PhrasebookEntry {
    fn from(entry: PhrasebookEntryThoughts) -> Self {
        Self {
            target_language_multi_word_term: entry.target_language_multi_word_term,
            meaning: entry.meaning,
            additional_notes: entry.additional_notes,
            target_language_example: entry.target_language_example,
            native_language_example: entry.native_language_example,
        }
    }
}

#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    tsify::Tsify,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DictionaryEntryThoughts {
    pub thoughts: String,
    pub target_language_word: String,
    pub definitions: Vec<TargetToNativeWord>,
}

#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    tsify::Tsify,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DictionaryEntry {
    pub target_language_word: String,
    pub definitions: Vec<TargetToNativeWord>,
}

impl From<DictionaryEntryThoughts> for DictionaryEntry {
    fn from(entry: DictionaryEntryThoughts) -> Self {
        Self {
            target_language_word: entry.target_language_word,
            definitions: entry.definitions,
        }
    }
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct Sentence {
    pub french: Vec<String>,
    pub english: String,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct MultiwordTerms {
    pub high_confidence: Vec<String>,
    pub low_confidence: Vec<String>,
}

/// The raw output from the Spacy python script
#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct NlpAnalyzedSentence {
    pub sentence: String,
    pub multiword_terms: MultiwordTerms,
    pub doc: Vec<DocToken>,
}

/// A more condensed version of NlpAnalyzedSentence
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SentenceInfo {
    pub words: Vec<Literal<String>>,
    pub multiword_terms: MultiwordTerms,
}

impl SentenceInfo {
    pub fn from_nlp_analyzed_sentence(
        analysis: NlpAnalyzedSentence,
        proper_nouns: &BTreeMap<String, Heteronym<String>>,
        language: Language,
    ) -> Self {
        Self {
            words: analysis
                .doc
                .into_iter()
                .map(|doc_token| Literal::from_doc_token(doc_token, proper_nouns, language))
                .collect(),
            multiword_terms: analysis.multiword_terms,
        }
    }

    /// The words and the high-confidence multiword terms
    pub fn lexemes(&self) -> impl Iterator<Item = Lexeme<String>> {
        self.words
            .iter()
            .filter_map(|token| {
                token
                    .heteronym
                    .as_ref()
                    .map(|heteronym| Lexeme::Heteronym(heteronym.clone()))
            })
            .chain(
                self.multiword_terms
                    .high_confidence
                    .iter()
                    .map(|term| Lexeme::Multiword(term.clone())),
            )
    }

    /// The words and the high-confidence and low-confidence multiword terms
    pub fn all_lexemes(&self) -> impl Iterator<Item = Lexeme<String>> {
        self.lexemes().chain(
            self.multiword_terms
                .low_confidence
                .iter()
                .map(|term| Lexeme::Multiword(term.clone())),
        )
    }
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct DocToken {
    pub text: String,
    pub whitespace: String,
    pub pos: PartOfSpeech,
    pub lemma: String,
    pub morph: BTreeMap<String, String>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[rkyv(compare(PartialEq))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Heteronym<S> {
    pub word: S,
    pub lemma: S,
    pub pos: PartOfSpeech,
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Literal<S> {
    pub text: S,
    pub whitespace: S,
    pub heteronym: Option<Heteronym<S>>,
}

impl Literal<String> {
    fn from_doc_token(
        doc_token: DocToken,
        proper_nouns: &BTreeMap<String, Heteronym<String>>,
        language: Language,
    ) -> Self {
        Self {
            text: doc_token.text.clone(),
            whitespace: doc_token.whitespace.clone(),
            heteronym: Heteronym::from_doc_token(doc_token, proper_nouns, language),
        }
    }

    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Literal<lasso::Spur> {
        Literal {
            text: rodeo.get_or_intern(&self.text),
            whitespace: rodeo.get_or_intern(&self.whitespace),
            heteronym: self.heteronym.as_ref().map(|h| h.get_or_intern(rodeo)),
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Literal<lasso::Spur>> {
        let heteronym = match self.heteronym.as_ref() {
            Some(h) => Some(h.get_interned(rodeo)?),
            None => None,
        };
        Some(Literal {
            text: rodeo.get(&self.text)?,
            whitespace: rodeo.get(&self.whitespace)?,
            heteronym,
        })
    }
}

impl Literal<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Literal<String> {
        Literal {
            text: rodeo.resolve(&self.text).to_string(),
            whitespace: rodeo.resolve(&self.whitespace).to_string(),
            heteronym: self.heteronym.as_ref().map(|h| h.resolve(rodeo)),
        }
    }
}

impl Heteronym<String> {
    fn from_doc_token(
        doc_token: DocToken,
        proper_nouns: &BTreeMap<String, Heteronym<String>>,
        language: Language,
    ) -> Option<Self> {
        let expand_word = match language {
            Language::French => expand_french_word,
            Language::Spanish => expand_spanish_word,
            Language::English => expand_english_word,
        };

        let heteronym = if let Some(heteronym) = proper_nouns.get(&doc_token.text.to_lowercase()) {
            let (word, lemma, pos) = expand_word(&heteronym.word, None, &BTreeMap::new())?;
            let lemma = lemma.unwrap_or(heteronym.lemma.clone());
            let pos = pos.unwrap_or(heteronym.pos);
            Self { word, lemma, pos }
        } else {
            let (word, lemma, pos) =
                expand_word(&doc_token.text, Some(doc_token.pos), &doc_token.morph)?;
            let lemma = lemma.unwrap_or(doc_token.lemma.clone());
            let lemma = lemma.strip_prefix("-").unwrap_or(&lemma).to_string();
            let lemma = lemma.strip_suffix(".").unwrap_or(&lemma).to_string();
            let pos = pos.unwrap_or(doc_token.pos);
            Self { word, lemma, pos }
        };

        if heteronym.pos == PartOfSpeech::Punct {
            return None;
        }
        if heteronym.pos == PartOfSpeech::Propn {
            return None;
        }

        Some(heteronym)
    }

    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Heteronym<lasso::Spur> {
        let word = rodeo.get_or_intern(&self.word);
        let lemma = rodeo.get_or_intern(&self.lemma);
        Heteronym {
            word,
            lemma,
            pos: self.pos,
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Heteronym<lasso::Spur>> {
        let word = rodeo.get(&self.word)?;
        let lemma = rodeo.get(&self.lemma)?;
        Some(Heteronym {
            word,
            lemma,
            pos: self.pos,
        })
    }
}

impl Heteronym<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Heteronym<String> {
        Heteronym {
            word: rodeo.resolve(&self.word).to_string(),
            lemma: rodeo.resolve(&self.lemma).to_string(),
            pos: self.pos,
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[rkyv(compare(PartialEq))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Lexeme<S> {
    Heteronym(Heteronym<S>),
    Multiword(S),
}

impl<S> Lexeme<S> {
    pub fn heteronym(&self) -> Option<&Heteronym<S>> {
        match self {
            Lexeme::Heteronym(heteronym) => Some(heteronym),
            _ => None,
        }
    }

    pub fn multiword(&self) -> Option<&S> {
        match self {
            Lexeme::Multiword(multiword) => Some(multiword),
            _ => None,
        }
    }
}

impl Lexeme<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Lexeme<lasso::Spur> {
        match self {
            Lexeme::Heteronym(heteronym) => Lexeme::Heteronym(heteronym.get_or_intern(rodeo)),
            Lexeme::Multiword(multiword) => Lexeme::Multiword(rodeo.get_or_intern(multiword)),
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Lexeme<lasso::Spur>> {
        match self {
            Lexeme::Heteronym(heteronym) => Some(Lexeme::Heteronym(heteronym.get_interned(rodeo)?)),
            Lexeme::Multiword(multiword) => Some(Lexeme::Multiword(rodeo.get(multiword)?)),
        }
    }
}

impl Lexeme<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Lexeme<String> {
        match self {
            Lexeme::Heteronym(heteronym) => Lexeme::Heteronym(heteronym.resolve(rodeo)),
            Lexeme::Multiword(multiword) => Lexeme::Multiword(rodeo.resolve(multiword).to_string()),
        }
    }
}

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
pub struct FrequencyEntry<S> {
    pub lexeme: Lexeme<S>,
    pub count: u32,
}

impl FrequencyEntry<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> FrequencyEntry<lasso::Spur> {
        FrequencyEntry {
            lexeme: self.lexeme.get_or_intern(rodeo),
            count: self.count,
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<FrequencyEntry<lasso::Spur>> {
        Some(FrequencyEntry {
            lexeme: self.lexeme.get_interned(rodeo)?,
            count: self.count,
        })
    }
}

pub mod autograde {
    use super::*;

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub enum Remembered {
        Remembered,
        Forgot,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct AutoGradeTranslationRequest {
        pub language: Language,
        pub challenge_sentence: String,
        pub user_sentence: String,
        pub primary_expression: Lexeme<String>,
        pub lexemes: Vec<Lexeme<String>>,
    }
    #[derive(
        Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema, tsify::Tsify,
    )]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct AutoGradeTranslationResponse {
        pub explanation: Option<String>,
        pub primary_expression_status: Remembered,
        pub expressions_remembered: Vec<Lexeme<String>>,
        pub expressions_forgot: Vec<Lexeme<String>>,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct AutoGradeTranscriptionRequest {
        pub language: Language,
        pub submission: Vec<transcription_challenge::PartSubmitted>,
    }
}

pub mod transcription_challenge {
    use super::*;

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(namespace, into_wasm_abi, from_wasm_abi)]
    pub enum Part {
        AskedToTranscribe { parts: Vec<Literal<String>> },
        Provided { part: Literal<String> },
    }

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(namespace, into_wasm_abi, from_wasm_abi)]
    pub enum PartSubmitted {
        AskedToTranscribe {
            parts: Vec<Literal<String>>,
            submission: String,
        },
        Provided {
            part: Literal<String>,
        },
    }

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(namespace, into_wasm_abi, from_wasm_abi)]
    pub enum PartGraded {
        AskedToTranscribe {
            parts: Vec<PartGradedPart>,
            submission: String,
        },
        Provided {
            part: Literal<String>,
        },
    }

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct PartGradedPart {
        pub heard: Literal<String>,
        pub grade: WordGrade,
    }

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(namespace, into_wasm_abi, from_wasm_abi)]
    pub enum WordGrade {
        Perfect {},
        CorrectWithTypo {},
        PhoneticallyIdenticalButContextuallyIncorrect {},
        PhoneticallySimilarButContextuallyIncorrect {},
        Incorrect {},
        Missed {},
    }

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        tsify::Tsify,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct Grade {
        pub explanation: Option<String>,
        pub results: Vec<PartGraded>,
        pub compare: Vec<String>,
        pub autograding_error: Option<String>,
    }
}

/// Consolidated data structure containing all generated language data
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
pub struct ConsolidatedLanguageData {
    /// All target language sentences from Anki cards
    pub target_language_sentences: Vec<String>,
    /// Mapping from target language sentences to all native translations
    pub translations: Vec<(String, Vec<String>)>,
    /// NLP-analyzed sentences with multiword terms and heteronyms
    pub nlp_sentences: Vec<(String, SentenceInfo)>,
    /// Dictionary entries for individual words
    pub dictionary: Vec<(Heteronym<String>, DictionaryEntry)>,
    /// Phrasebook entries for multiword terms
    pub phrasebook: Vec<(String, PhrasebookEntry)>,
    /// Frequency data for words and phrases
    pub frequencies: Vec<FrequencyEntry<String>>,
    /// Mapping from words to their IPA pronunciations
    pub word_to_pronunciation: Vec<(String, Pronunciation)>,
    /// Mapping from IPA pronunciations to lists of words
    pub pronunciation_to_words: Vec<(Pronunciation, Vec<String>)>,
}

#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
pub struct ConsolidatedLanguageDataWithCapacity {
    pub consolidated_language_data: ConsolidatedLanguageData,
    pub num_strings: u32,
    pub num_string_bytes: u32,
}

impl ConsolidatedLanguageData {
    pub fn intern(&self, rodeo: &mut lasso::Rodeo) {
        // Intern empty string and space, just to make sure it's in there
        let _ = rodeo.get_or_intern("");
        let _ = rodeo.get_or_intern(" ");

        // Intern sentences
        for sentence in &self.target_language_sentences {
            rodeo.get_or_intern(sentence);
        }

        // Intern translations
        for (french, englishes) in &self.translations {
            rodeo.get_or_intern(french);
            for english in englishes {
                rodeo.get_or_intern(english);
            }
        }

        // Intern words from frequency list
        for freq in &self.frequencies {
            match &freq.lexeme {
                Lexeme::Heteronym(heteronym) => {
                    rodeo.get_or_intern(&heteronym.word);
                    rodeo.get_or_intern(&heteronym.lemma);
                }
                Lexeme::Multiword(multiword) => {
                    rodeo.get_or_intern(multiword);
                }
            }
        }

        // Intern words used in sentences (includes proper nouns, plus capitalization might differ)
        for (_, sentence_info) in &self.nlp_sentences {
            for word in &sentence_info.words {
                rodeo.get_or_intern(&word.text);
            }
        }

        // intern pronunciations
        for (_word, pronunciation) in &self.word_to_pronunciation {
            rodeo.get_or_intern(pronunciation);
        }
    }
}

impl ConsolidatedLanguageDataWithCapacity {
    pub fn intern(&self) -> lasso::Rodeo {
        let mut rodeo = lasso::Rodeo::with_capacity(lasso::Capacity::new(
            self.num_strings as usize,
            NonZeroUsize::new(self.num_string_bytes as usize).unwrap(),
        ));

        self.consolidated_language_data.intern(&mut rodeo);
        rodeo
    }
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum TtsProvider {
    ElevenLabs,
    Google,
}

pub type Pronunciation = String;

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Language {
    French,
    English,
    Spanish,
}

impl Language {
    pub fn iso_639_3(&self) -> &str {
        match self {
            Language::French => "fra",
            Language::English => "eng",
            Language::Spanish => "spa",
        }
    }

    pub fn iso_639_1(&self) -> &'static str {
        match self {
            Language::French => "fr",
            Language::English => "en",
            Language::Spanish => "es",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::French => write!(f, "French"),
            Language::English => write!(f, "English"),
            Language::Spanish => write!(f, "Spanish"),
        }
    }
}

#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Course {
    pub native_language: Language,
    pub target_language: Language,
}

pub const COURSES: &[Course] = &[
    Course {
        native_language: Language::English,
        target_language: Language::French,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Spanish,
    },
];

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TtsRequest {
    pub text: String,
    pub language: Language,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_punctuation_removes_surrounding_marks() {
        assert_eq!(strip_punctuation("hello!?"), "hello");
        assert_eq!(strip_punctuation("--hi--"), "hi");
        assert_eq!(strip_punctuation("?!hi??"), "hi");
    }
}
