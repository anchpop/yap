pub mod features;
pub mod indexmap;
pub mod language_pack;
pub mod profile;
pub mod text_cleanup;

use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::hash::Hash;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::features::Morphology;

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
#[schemars(rename = "PartOfSpeech")]
pub enum PartOfSpeechTag {
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
    #[serde(rename = "SPACE")]
    Space, // space
    #[serde(rename = "X")]
    X, // other
}

impl std::fmt::Display for PartOfSpeechTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let word = match self {
            PartOfSpeechTag::Adj => "adjective",
            PartOfSpeechTag::Adp => "adposition",
            PartOfSpeechTag::Adv => "adverb",
            PartOfSpeechTag::Aux => "auxiliary",
            PartOfSpeechTag::Cconj => "coordinating conjunction",
            PartOfSpeechTag::Det => "determiner",
            PartOfSpeechTag::Intj => "interjection",
            PartOfSpeechTag::Noun => "noun",
            PartOfSpeechTag::Num => "numeral",
            PartOfSpeechTag::Part => "particle",
            PartOfSpeechTag::Pron => "pronoun",
            PartOfSpeechTag::Propn => "proper noun",
            PartOfSpeechTag::Punct => "punctuation",
            PartOfSpeechTag::Sconj => "subordinating conjunction",
            PartOfSpeechTag::Sym => "symbol",
            PartOfSpeechTag::Verb => "verb",
            PartOfSpeechTag::Space => "space",
            PartOfSpeechTag::X => "other",
        };
        write!(f, "{word}")
    }
}

/// Part-of-speech for heteronyms (excludes proper nouns, punctuation, spaces, and unknown)
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
    #[serde(rename = "SCONJ")]
    Sconj, // subordinating conjunction
    #[serde(rename = "SYM")]
    Sym, // symbol
    #[serde(rename = "VERB")]
    Verb, // verb
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
            PartOfSpeech::Sconj => "subordinating conjunction",
            PartOfSpeech::Sym => "symbol",
            PartOfSpeech::Verb => "verb",
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TargetToNativeWord {
    pub native: String,
    pub note: Option<String>,
    pub example_sentence_target_language: String,
    pub example_sentence_native_language: String,
    pub cognate: bool,
    pub false_cognate: bool,
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PhrasebookDefinitionEntry {
    pub target_language_multi_word_term: String,
    pub meaning: String,
    pub additional_notes: String,
    pub target_language_example: String,
    pub native_language_example: String,
    pub informal: bool,
    pub compositional: bool,
    pub cognate: bool,
    pub false_cognate: bool,
    pub can_be_translated_literally: bool,
}

#[derive(Clone, Debug, serde::Deserialize, schemars::JsonSchema)]
#[schemars(rename = "PhrasebookDefinitionEntry")]
pub struct PhrasebookDefinitionEntryV2 {
    pub target_language_multi_word_term: String,
    pub meanings: Vec<String>,
    pub additional_notes: String,
    pub target_language_example: String,
    pub native_language_example: String,
    pub informal: bool,
    pub compositional: bool,
    pub cognate: bool,
    pub false_cognate: bool,
    pub can_be_translated_literally: bool,
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
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug), derive(Hash))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ProperNounDefinition {
    pub is_person_name: bool,
    pub is_place_name: bool,
    pub is_organization_name: bool,
    pub is_other: bool,
    pub learner_native_language_translation: String,
    pub description: Option<String>,
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DictionaryDefinition {
    pub target_language_word: String,
    pub definitions: Vec<TargetToNativeWord>,
}

/// A gram definition - either a dictionary entry (single word) or phrasebook entry (multi-word)
#[derive(
    Clone,
    Debug,
    serde::Deserialize,
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum GramDefinition {
    Dictionary(DictionaryEntry),
    Phrasebook(PhrasebookDefinitionEntry),
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DictionaryEntry {
    pub target_language_word: String,
    pub definitions: Vec<TargetToNativeWord>,
    pub morphology: Vec<Morphology>,
}

impl From<(DictionaryDefinition, Vec<Morphology>)> for DictionaryEntry {
    fn from(entry: (DictionaryDefinition, Vec<Morphology>)) -> Self {
        let (entry, morphology) = entry;
        Self {
            target_language_word: entry.target_language_word,
            definitions: entry.definitions,
            morphology,
        }
    }
}

/// Tracks the source(s) of a sentence. Since a sentence can appear in multiple sources,
/// we use boolean fields for each source type.
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
pub struct SentenceSource {
    /// Sentence came from an Anki deck
    pub from_anki: bool,
    /// Sentence came from Tatoeba
    pub from_tatoeba: bool,
    /// Sentence was manually added to extra/manual.txt
    pub from_manual: bool,
    /// Sentence came from a song in sentence-sources/songs/
    pub from_song: bool,
    /// Movie IDs if this sentence appears in movies (e.g., ["tt0211915", "tt0241527"])
    pub movie_ids: Vec<String>,
}

impl SentenceSource {
    /// Create a new source with all fields set to false
    pub fn none() -> Self {
        Self {
            from_anki: false,
            from_tatoeba: false,
            from_manual: false,
            from_song: false,
            movie_ids: Vec::new(),
        }
    }

    /// Returns true if the sentence came from a manual source (should never be filtered)
    pub fn is_manual(&self) -> bool {
        self.from_manual
    }

    /// Merge two sources together (OR operation on all fields)
    pub fn merge(&mut self, other: &Self) {
        self.from_anki |= other.from_anki;
        self.from_tatoeba |= other.from_tatoeba;
        self.from_manual |= other.from_manual;
        self.from_song |= other.from_song;
        // Merge movie IDs, avoiding duplicates
        for movie_id in &other.movie_ids {
            if !self.movie_ids.contains(movie_id) {
                self.movie_ids.push(movie_id.clone());
            }
        }
    }
}

/// A Pimsleur lesson identifier (level + lesson number)
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
pub struct PimsleurLesson {
    pub level: u32,
    #[serde(alias = "unit")]
    pub lesson: u32,
}

/// Identifies a source for per-source gram frequency data
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
pub enum FrequencySourceId {
    Movie(String),
    PimsleurLesson(PimsleurLesson),
}

/// Basic movie metadata without poster bytes, for serialization to files
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct MovieMetadataBasic {
    /// Unique identifier (IMDb ID, e.g., "tt0211915")
    pub id: String,
    /// Movie title
    pub title: String,
    /// Release year
    pub year: Option<u16>,
    /// Original language of the movie (ISO 639-1 code, e.g., "en", "fr")
    #[serde(default)]
    pub original_language: Option<String>,
    /// Rotten Tomatoes score (0-100)
    #[serde(default)]
    pub rotten_tomatoes_score: Option<u8>,
}

/// Full movie metadata including poster bytes, for runtime use
#[derive(
    Clone,
    Debug,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
pub struct MovieMetadata {
    /// Unique identifier (IMDb ID, e.g., "tt0211915")
    pub id: String,
    /// Movie title
    pub title: String,
    /// Release year
    pub year: Option<u16>,
    /// Original language of the movie (ISO 639-1 code, e.g., "en", "fr")
    pub original_language: Option<String>,
    /// Rotten Tomatoes score (0-100)
    pub rotten_tomatoes_score: Option<u8>,
    /// Poster image bytes (JPEG format)
    pub poster_bytes: Option<Vec<u8>>,
}

impl From<MovieMetadataBasic> for MovieMetadata {
    fn from(basic: MovieMetadataBasic) -> Self {
        MovieMetadata {
            id: basic.id,
            title: basic.title,
            year: basic.year,
            original_language: basic.original_language,
            rotten_tomatoes_score: basic.rotten_tomatoes_score,
            poster_bytes: None,
        }
    }
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(derive(Debug))]
pub struct SubtitleLine<S>
where
    S: rkyv::Archive + Hash + std::fmt::Debug + Eq + PartialEq + Ord + PartialOrd,
    <S as rkyv::Archive>::Archived: std::fmt::Debug,
{
    /// The sentence text (Spur reference to sentence)
    pub sentence: S,
    /// Start timestamp in milliseconds
    pub start_ms: u32,
    /// End timestamp in milliseconds
    pub end_ms: u32,
}

/// An encoded sentence with grams (atoms with learnability info) and display metadata
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SentenceGrams<G> {
    /// The grams that make up this sentence, each marked as learnable or obvious
    pub grams: Vec<SentenceGram<G>>,
    /// Whether the first letter should be capitalized when displaying
    pub capitalize_first: bool,
    /// High-confidence multiword terms found in this sentence
    pub multiword_terms: Vec<G>,
    /// Low-confidence multiword terms found in this sentence
    pub low_confidence_multiword_terms: Vec<G>,
}

impl SentenceGrams<SpurGram> {
    pub fn to_literals(
        self,
        string_rodeo: &lasso::RodeoReader<String>,
        gram_rodeo: &lasso::RodeoReader<Gram<lasso::Spur>>,
        language: Language,
    ) -> Vec<SentenceGram<(SpurGram, Vec<Literal<String>>)>> {
        // First pass: collect all atoms with their gram index, preserving
        // Control tokens so whitespace corrections are honored.
        let mut all_atoms: Vec<(usize, Atom<String>)> = Vec::new();
        for (gram_idx, gram) in self.grams.iter().enumerate() {
            let gram_spur = match gram {
                SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
            };
            let gram_resolved = gram_rodeo.resolve(gram_spur).resolve(string_rodeo);
            for atom in gram_resolved.iter() {
                all_atoms.push((gram_idx, atom.clone()));
            }
        }

        // Collect just the words for capitalization
        if self.capitalize_first {
            if let Some((_, Atom::Tok(first_word))) = all_atoms
                .iter_mut()
                .find(|(_, a)| matches!(a, Atom::Tok(_)))
            {
                first_word.text = capitalize_first_letter(&first_word.text);
            }
        }

        // Build literals using control tokens for whitespace correction.
        // We need to look ahead across gram boundaries for correct whitespace,
        // so we iterate over the flat atom list, then group by gram index.
        let mut literals_with_gram_idx: Vec<(usize, Literal<String>)> = Vec::new();
        let mut i = 0;
        while i < all_atoms.len() {
            let (gram_idx, atom) = &all_atoms[i];
            match atom {
                Atom::Tok(word) => {
                    // Determine whitespace: check if next atom is a Control token
                    let whitespace = if i + 1 < all_atoms.len() {
                        match &all_atoms[i + 1].1 {
                            Atom::Control(ctrl) => {
                                i += 1; // consume the control token
                                ctrl.0
                            }
                            Atom::Tok(next_word) => {
                                predict_whitespace(word, Some(next_word), language)
                            }
                        }
                    } else {
                        predict_whitespace(word, None, language)
                    };

                    literals_with_gram_idx.push((
                        *gram_idx,
                        Literal {
                            word: word.clone(),
                            whitespace: whitespace.to_str().to_string(),
                        },
                    ));
                }
                Atom::Control(_) => {
                    // Standalone control token (not preceded by Tok) — skip
                }
            }
            i += 1;
        }

        // Group literals back into their grams
        self.grams
            .into_iter()
            .enumerate()
            .map(|(gram_idx, gram)| {
                gram.map(|gram_spur| {
                    let literals: Vec<Literal<String>> = literals_with_gram_idx
                        .iter()
                        .filter(|(idx, _)| *idx == gram_idx)
                        .map(|(_, lit)| lit.clone())
                        .collect();
                    (gram_spur, literals)
                })
            })
            .collect()
    }
}

impl SentenceGrams<SpurGram> {
    pub fn resolve(
        &self,
        rodeo: &lasso::RodeoReader<Gram<lasso::Spur>>,
    ) -> SentenceGrams<Gram<lasso::Spur>> {
        SentenceGrams {
            grams: self.grams.iter().map(|gram| gram.resolve(rodeo)).collect(),
            capitalize_first: self.capitalize_first,
            multiword_terms: self
                .multiword_terms
                .iter()
                .map(|g| rodeo.resolve(g).to_gram())
                .collect(),
            low_confidence_multiword_terms: self
                .low_confidence_multiword_terms
                .iter()
                .map(|g| rodeo.resolve(g).to_gram())
                .collect(),
        }
    }
}

impl SentenceGrams<Gram<lasso::Spur>> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> SentenceGrams<Gram<String>> {
        SentenceGrams {
            grams: self.grams.iter().map(|gram| gram.resolve(rodeo)).collect(),
            capitalize_first: self.capitalize_first,
            multiword_terms: self
                .multiword_terms
                .iter()
                .map(|gram| gram.resolve(rodeo))
                .collect(),
            low_confidence_multiword_terms: self
                .low_confidence_multiword_terms
                .iter()
                .map(|gram| gram.resolve(rodeo))
                .collect(),
        }
    }
}

impl<S> SentenceGram<S> {
    pub fn map<T, F>(self, f: F) -> SentenceGram<T>
    where
        F: Fn(S) -> T,
    {
        match self {
            SentenceGram::Learnable(s) => SentenceGram::Learnable(f(s)),
            SentenceGram::Obvious(s) => SentenceGram::Obvious(f(s)),
        }
    }
}

impl SentenceGram<SpurGram> {
    pub fn resolve(
        &self,
        rodeo: &lasso::RodeoReader<Gram<lasso::Spur>>,
    ) -> SentenceGram<Gram<lasso::Spur>> {
        match self {
            SentenceGram::Learnable(g) => SentenceGram::Learnable(rodeo.resolve(g).to_gram()),
            SentenceGram::Obvious(g) => SentenceGram::Obvious(rodeo.resolve(g).to_gram()),
        }
    }
}

impl SentenceGram<Gram<lasso::Spur>> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> SentenceGram<Gram<String>> {
        match self {
            SentenceGram::Learnable(g) => SentenceGram::Learnable(g.resolve(rodeo)),
            SentenceGram::Obvious(g) => SentenceGram::Obvious(g.resolve(rodeo)),
        }
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
pub struct MultiwordTerms<T> {
    pub high_confidence: Vec<T>,
    pub low_confidence: Vec<T>,
}

/// The raw output from the Spacy python script
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
pub struct NlpAnalyzedSentence {
    pub sentence: String,
    pub multiword_terms: MultiwordTerms<String>,
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
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SentenceInfo {
    pub words: Vec<Literal<String>>,
    pub multiword_terms: MultiwordTerms<Gram<String>>,
}

impl SentenceInfo {
    /// The fraction of words that are proper nouns
    pub fn proper_noun_fraction(&self) -> f32 {
        let total_words = self
            .words
            .iter()
            .filter(|token| {
                !matches!(
                    token.word.word_type,
                    WordType::Other(OtherWord {
                        other_tag: OtherWordType::Punct | OtherWordType::Space | OtherWordType::X
                    })
                )
            })
            .count() as f32;

        if total_words == 0.0 {
            return 0.0;
        }

        let proper_nouns = self
            .words
            .iter()
            .filter(|token| {
                matches!(
                    token.word.word_type,
                    WordType::Other(OtherWord {
                        other_tag: OtherWordType::Propn
                    })
                )
            })
            .count() as f32;

        proper_nouns / total_words
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
    schemars::JsonSchema,
)]
pub struct DocToken {
    pub text: String,
    pub whitespace: String,
    pub pos: PartOfSpeechTag,
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Heteronym<S> {
    pub word: S,
    pub lemma: S,
    pub pos: PartOfSpeech,
}

/// Type of non-heteronym word
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum OtherWordType {
    #[serde(rename = "PROPN")]
    Propn, // proper noun
    #[serde(rename = "PUNCT")]
    Punct, // punctuation
    #[serde(rename = "SPACE")]
    Space, // space
    #[serde(rename = "X")]
    X, // other/unknown
}

/// Information about a non-heteronym word
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct OtherWord {
    pub other_tag: OtherWordType,
}

/// The type of word in a literal - heteronym or other
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
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum WordType<S> {
    /// A word with dictionary/grammatical information
    Heteronym(Heteronym<S>),
    /// Other (proper noun, punctuation, space, unknown)
    Other(OtherWord),
}

/// A word with its text and grammatical type
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Word<S> {
    pub text: S,
    pub word_type: WordType<S>,
}

/// A literal token in a sentence (word + trailing whitespace)
#[derive(
    Copy,
    Clone,
    Debug,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Literal<S> {
    pub word: Word<S>,
    pub whitespace: S,
}

impl<S> Word<S> {
    /// Get the heteronym if this word represents a heteronym, otherwise None
    pub fn heteronym(&self) -> Option<&Heteronym<S>> {
        match &self.word_type {
            WordType::Heteronym(h) => Some(h),
            _ => None,
        }
    }
}

impl<S> Literal<S> {
    /// Get the heteronym if this literal represents a heteronym, otherwise None
    pub fn heteronym(&self) -> Option<&Heteronym<S>> {
        self.word.heteronym()
    }
}

impl Word<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Word<lasso::Spur> {
        Word {
            text: rodeo.get_or_intern(&self.text),
            word_type: match &self.word_type {
                WordType::Heteronym(h) => WordType::Heteronym(h.get_or_intern(rodeo)),
                WordType::Other(other) => WordType::Other(*other),
            },
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Word<lasso::Spur>> {
        let word_type = match &self.word_type {
            WordType::Heteronym(h) => WordType::Heteronym(h.get_interned(rodeo)?),
            WordType::Other(other) => WordType::Other(*other),
        };
        Some(Word {
            text: rodeo.get(&self.text)?,
            word_type,
        })
    }
}

impl Literal<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Literal<lasso::Spur> {
        Literal {
            word: self.word.get_or_intern(rodeo),
            whitespace: rodeo.get_or_intern(&self.whitespace),
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Literal<lasso::Spur>> {
        Some(Literal {
            word: self.word.get_interned(rodeo)?,
            whitespace: rodeo.get(&self.whitespace)?,
        })
    }
}

impl Word<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Word<String> {
        Word {
            text: rodeo.resolve(&self.text).to_string(),
            word_type: match &self.word_type {
                WordType::Heteronym(h) => WordType::Heteronym(h.resolve(rodeo)),
                WordType::Other(other) => WordType::Other(*other),
            },
        }
    }
}

impl Literal<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Literal<String> {
        Literal {
            word: self.word.resolve(rodeo),
            whitespace: rodeo.resolve(&self.whitespace).to_string(),
        }
    }
}

impl Heteronym<String> {
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
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum Lexeme<S> {
    Heteronym { heteronym: Heteronym<S> },
    Multiword { phrase: S },
}

impl<S> Lexeme<S> {
    pub fn heteronym(&self) -> Option<&Heteronym<S>> {
        match self {
            Lexeme::Heteronym { heteronym } => Some(heteronym),
            _ => None,
        }
    }

    pub fn multiword(&self) -> Option<&S> {
        match self {
            Lexeme::Multiword { phrase } => Some(phrase),
            _ => None,
        }
    }
}

impl Lexeme<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Lexeme<lasso::Spur> {
        match self {
            Lexeme::Heteronym { heteronym } => Lexeme::Heteronym {
                heteronym: heteronym.get_or_intern(rodeo),
            },
            Lexeme::Multiword { phrase } => Lexeme::Multiword {
                phrase: rodeo.get_or_intern(phrase),
            },
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Lexeme<lasso::Spur>> {
        match self {
            Lexeme::Heteronym { heteronym } => Some(Lexeme::Heteronym {
                heteronym: heteronym.get_interned(rodeo)?,
            }),
            Lexeme::Multiword { phrase } => Some(Lexeme::Multiword {
                phrase: rodeo.get(phrase)?,
            }),
        }
    }

    pub fn get_disambiguation_key(&self) -> u32 {
        match self {
            Lexeme::Heteronym { heteronym: h } => {
                let combined = format!("{}\0{}\0{:?}", h.word, h.lemma, h.pos);
                xxhash_rust::xxh3::xxh3_64(combined.as_bytes()) as u32
            }
            Lexeme::Multiword { phrase } => xxhash_rust::xxh3::xxh3_64(phrase.as_bytes()) as u32,
        }
    }
}

impl Lexeme<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Lexeme<String> {
        match self {
            Lexeme::Heteronym { heteronym } => Lexeme::Heteronym {
                heteronym: heteronym.resolve(rodeo),
            },
            Lexeme::Multiword { phrase } => Lexeme::Multiword {
                phrase: rodeo.resolve(phrase).to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct GramFrequencyEntry<S> {
    pub count: u32,

    /// Count of occurrences in actual sentences only (not including multi-word term occurrences).
    pub direct_count: u32,

    /// This key is different for each gram, which allows a consistent ordering with the same frequency.
    pub disambiguation_key: u32,

    pub gram: Gram<S>,
}

/// A frequency list with its total count (for percentage calculations).
/// Used in ConsolidatedLanguageData (pre-interning).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GramFrequencyList {
    pub entries: Vec<GramFrequencyEntry<String>>,
    /// Total gram count from unfiltered data (for accurate percentage calculations)
    pub total_count: u64,
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Frequency {
    pub count: u32,
    /// Count of occurrences in actual sentences only (not including multi-word term occurrences).
    pub direct_count: u32,
    /// Whether this gram is considered "easy" (cognate with single-word definition).
    /// Easy grams are excluded from the isotonic regression and preferred during onboarding.
    pub easy: bool,
    /// Whether this is a compositional/literally-translatable multi-word gram.
    /// These are excluded from the isotonic regression (their difficulty depends on
    /// component words, violating the regression's independence assumption).
    pub compositional: bool,
    /// Estimated ease of this gram (higher = easier to already know).
    /// For single-atom grams: ln(count), with a cognate bonus.
    /// For multi-atom grams: depends on compositionality and component word ease.
    /// Used as the x-axis in the isotonic regression (instead of raw ln_frequency).
    pub ease: f32,
}

impl Frequency {
    pub fn ln_frequency(&self) -> f32 {
        (self.count as f32).ln()
    }

    pub fn ln_direct_frequency(&self) -> f32 {
        (self.direct_count as f32).ln()
    }

    pub fn exclude_from_regression(&self) -> bool {
        self.easy || self.compositional
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
        pub course: Course,
        pub challenge_sentence: String,
        pub user_sentence: String,
        pub literals: Vec<Literal<String>>,
        pub phrases: Vec<Gram<String>>,
    }

    /// Response from autograde.
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct AutoGradeTranslationResponse {
        pub encouragement: Option<String>,
        pub explanation: Option<String>,
        /// One entry per literal in order. None = ungradable (Other word type) or indeterminate.
        /// Covers single-word grams (heteronyms); multi-word grams use phrases_remembered/phrases_forgot.
        pub literal_grades: Vec<Option<Remembered>>,
        pub phrases_remembered: Vec<Gram<String>>,
        pub phrases_forgot: Vec<Gram<String>>,
        /// Set when heuristic grading was used instead of the LLM.
        pub autograding_error: Option<String>,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct AutoGradeTranscriptionRequest {
        pub course: Course,
        pub submission: Vec<transcription_challenge::PartSubmitted>,
    }

    /// Wrapper for passing gram grades across the WASM boundary.
    /// One entry per literal in order. None = ungradable (Other word type) or indeterminate.
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct LiteralGrades(pub Vec<Option<Remembered>>);

    /// Wrapper for passing gram definitions across the WASM boundary.
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
    #[tsify(into_wasm_abi, from_wasm_abi)]
    pub struct GramDefinitions(pub Vec<Option<GramDefinition>>);
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
    #[serde(tag = "type")]
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
    #[serde(tag = "type")]
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
    #[serde(tag = "type")]
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
    #[serde(tag = "type")]
    pub enum WordGrade {
        Perfect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        CorrectWithTypo {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        PhoneticallyIdenticalButContextuallyIncorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        PhoneticallySimilarButContextuallyIncorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        Incorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
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
        pub encouragement: Option<String>,
        pub explanation: Option<String>,
        pub results: Vec<PartGraded>,
        pub compare: Vec<String>,
        pub autograding_error: Option<String>,
    }
}

/// Represents whitespace between tokens.
/// We use an explicit enum rather than storing the actual string to normalize
/// the representation and make it more compact.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Whitespace {
    /// Regular space (U+0020)
    Space,
    /// Narrow non-breaking space (U+202F) - used in French before high punctuation
    NarrowNbsp,
    /// Regular non-breaking space (U+00A0)
    Nbsp,
    /// No whitespace
    None,
}

impl Whitespace {
    /// Convert whitespace enum to actual string
    pub fn to_str(&self) -> &'static str {
        match self {
            Whitespace::Space => " ",
            Whitespace::NarrowNbsp => "\u{202F}",
            Whitespace::Nbsp => "\u{00A0}",
            Whitespace::None => "",
        }
    }
}

impl std::str::FromStr for Whitespace {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => Whitespace::None,
            " " => Whitespace::Space,
            "\u{202F}" => Whitespace::NarrowNbsp,
            "\u{00A0}" => Whitespace::Nbsp,
            // For multiple spaces or other whitespace, normalize to single space
            s if s.chars().all(|c| c.is_whitespace()) => {
                if s.contains('\u{202F}') {
                    Whitespace::NarrowNbsp
                } else if s.contains('\u{00A0}') {
                    Whitespace::Nbsp
                } else {
                    Whitespace::Space
                }
            }
            // Default to space for anything else
            _ => Whitespace::Space,
        })
    }
}

/// A control token that corrects wrong whitespace predictions.
/// When the predicted whitespace doesn't match the actual whitespace,
/// we emit a Control token to record the correct value.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ControlToken(pub Whitespace);

/// An atom is either a word token or a control token.
/// This is the basic unit after whitespace normalization.
#[derive(
    Copy,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Atom<S> {
    /// A regular word token
    Tok(Word<S>),
    /// A control token for whitespace correction
    Control(ControlToken),
}

impl Atom<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Atom<lasso::Spur> {
        match self {
            Atom::Tok(word) => Atom::Tok(word.get_or_intern(rodeo)),
            Atom::Control(ctrl) => Atom::Control(*ctrl),
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Atom<lasso::Spur>> {
        match self {
            Atom::Tok(word) => Some(Atom::Tok(word.get_interned(rodeo)?)),
            Atom::Control(ctrl) => Some(Atom::Control(*ctrl)),
        }
    }
}

impl Atom<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Atom<String> {
        match self {
            Atom::Tok(word) => Atom::Tok(word.resolve(rodeo)),
            Atom::Control(ctrl) => Atom::Control(*ctrl),
        }
    }
}

/// An encoded sentence with token IDs from the gram vocabulary
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq), derive(Debug, PartialEq, Eq))]
pub struct EncodedSentence {
    /// Token IDs from the gram vocabulary
    pub tokens: Vec<u32>,
    /// Whether the first letter should be capitalized when displaying
    pub capitalize_first: bool,
}

/// A gram is a sequence of atoms representing a learnable unit in a sentence.
/// This is a newtype wrapper around `Vec<Atom<S>>` that provides methods for
/// working with grams and implements `Internable` for use with lasso.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Gram<S>(pub Vec<Atom<S>>);
pub type SpurGram = lasso::Spur<Gram<lasso::Spur>>;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
pub struct grm<S> {
    atoms: [Atom<S>],
}
impl<S> AsRef<grm<S>> for grm<S> {
    fn as_ref(&self) -> &grm<S> {
        self
    }
}

impl<S> grm<S> {
    pub fn from_slice(slice: &[Atom<S>]) -> &Self {
        // SAFETY: grm is repr(transparent) over [Atom<S>]
        unsafe { &*(slice as *const [Atom<S>] as *const grm<S>) }
    }

    pub fn atoms(&self) -> &[Atom<S>] {
        &self.atoms
    }

    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Atom<S>> {
        self.atoms.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }
}

impl<S: Clone> grm<S> {
    pub fn to_gram(&self) -> Gram<S> {
        Gram(self.atoms.to_vec())
    }
}

impl<S> std::ops::Deref for Gram<S> {
    type Target = grm<S>;

    fn deref(&self) -> &grm<S> {
        grm::from_slice(&self.0)
    }
}
impl<S> AsRef<grm<S>> for Gram<S> {
    fn as_ref(&self) -> &grm<S> {
        self
    }
}

impl<S> Gram<S> {
    /// Creates a new gram from a vector of atoms.
    pub fn new(atoms: Vec<Atom<S>>) -> Self {
        Gram(atoms)
    }

    /// Returns true if this gram contains at least one learnable atom (a heteronym).
    pub fn is_learnable(&self) -> bool {
        self.0.iter().any(|atom| match atom {
            Atom::Tok(word) => matches!(word.word_type, WordType::Heteronym(_)),
            Atom::Control(_) => false,
        })
    }

    /// Returns the number of atoms in this gram.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if this gram has no atoms.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the atoms in this gram.
    pub fn iter(&self) -> impl Iterator<Item = &Atom<S>> {
        self.0.iter()
    }

    /// Returns a reference to the first atom, if any.
    pub fn first(&self) -> Option<&Atom<S>> {
        self.0.first()
    }

    /// If this is a single-atom heteronym gram, return a reference to the heteronym.
    pub fn heteronym(&self) -> Option<&Heteronym<S>> {
        if self.0.len() != 1 {
            return None;
        }
        match self.0.first()? {
            Atom::Tok(word) => word.heteronym(),
            Atom::Control(_) => None,
        }
    }
}

impl grm<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Gram<String> {
        Gram(self.atoms.iter().map(|a| a.resolve(rodeo)).collect())
    }
}

impl Gram<String> {
    /// Returns a disambiguation key for this gram, used to maintain consistent ordering
    /// of grams with the same frequency.
    pub fn disambiguation_key(&self) -> u32 {
        use std::fmt::Write;
        let mut combined = String::new();
        for atom in &self.0 {
            match atom {
                Atom::Tok(word) => {
                    let _ = write!(combined, "{}\0", word.text);
                }
                Atom::Control(ctrl) => {
                    let _ = write!(combined, "{ctrl:?}\0");
                }
            }
        }
        xxhash_rust::xxh3::xxh3_64(combined.as_bytes()) as u32
    }

    /// Converts this gram to a display string using whitespace prediction.
    /// This properly reconstructs the text with correct spacing for the given language.
    pub fn to_display_string(&self, language: Language) -> String {
        let literals = atoms_to_literals(&self.0, language);
        literals_to_text(&literals)
    }

    /// Interns all strings in this gram using the given rodeo.
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> Gram<lasso::Spur> {
        Gram(self.0.iter().map(|a| a.get_or_intern(rodeo)).collect())
    }

    /// Looks up all strings in this gram in the given rodeo reader.
    /// Returns None if any string is not found.
    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<Gram<lasso::Spur>> {
        Some(Gram(
            self.0
                .iter()
                .map(|a| a.get_interned(rodeo))
                .collect::<Option<Vec<_>>>()?,
        ))
    }
}

impl Gram<lasso::Spur> {
    pub fn get_interned(&self, rodeo: &lasso::RodeoReader<Gram<lasso::Spur>>) -> Option<SpurGram> {
        rodeo.get(self)
    }
}

impl Gram<lasso::Spur> {
    /// Resolves all interned strings in this gram using the given rodeo reader.
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> Gram<String> {
        Gram(self.0.iter().map(|a| a.resolve(rodeo)).collect())
    }
}

impl<S> AsRef<[Atom<S>]> for Gram<S> {
    fn as_ref(&self) -> &[Atom<S>] {
        &self.0
    }
}

impl<S> From<Vec<Atom<S>>> for Gram<S> {
    fn from(atoms: Vec<Atom<S>>) -> Self {
        Gram(atoms)
    }
}

impl<S> FromIterator<Atom<S>> for Gram<S> {
    fn from_iter<I: IntoIterator<Item = Atom<S>>>(iter: I) -> Self {
        Gram(iter.into_iter().collect())
    }
}

impl<S> IntoIterator for Gram<S> {
    type Item = Atom<S>;
    type IntoIter = std::vec::IntoIter<Atom<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, S> IntoIterator for &'a Gram<S> {
    type Item = &'a Atom<S>;
    type IntoIter = std::slice::Iter<'a, Atom<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// Implement Internable for Gram so it can be used with lasso::Rodeo<Gram<S>>
impl<S: Copy + Eq + std::hash::Hash + 'static> lasso::Internable for Gram<S> {
    type Ref = grm<S>;

    fn from_ref(r: &Self::Ref) -> Self {
        Gram(r.atoms().to_vec())
    }
}

impl<S: Copy + Eq + std::hash::Hash + 'static> lasso::InternableRef for grm<S> {
    const ALIGNMENT: usize = std::mem::align_of::<Atom<S>>();
    fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }
    fn as_bytes(&self) -> &[u8] {
        self.atoms.as_bytes()
    }
    fn len(&self) -> usize {
        self.atoms.len()
    }
    fn empty() -> &'static Self {
        grm::<S>::from_slice(<[Atom<S>] as lasso::InternableRef>::empty())
    }

    unsafe fn from_raw_parts<'a>(ptr: *const u8, count: usize) -> &'a Self {
        let slice: &[Atom<S>] =
            unsafe { <[Atom<S>] as lasso::InternableRef>::from_raw_parts(ptr, count) };
        unsafe { &*(slice as *const [Atom<S>] as *const grm<S>) }
    }
}

// Legacy function for backwards compatibility - delegates to the method
#[deprecated(note = "Use gram.is_learnable() instead")]
pub fn is_gram_learnable<S>(gram: &Gram<S>) -> bool {
    gram.is_learnable()
}

// Legacy function for backwards compatibility - delegates to the method
#[deprecated(note = "Use gram.disambiguation_key() instead")]
pub fn get_gram_disambiguation_key(gram: &Gram<String>) -> u32 {
    gram.disambiguation_key()
}

// By doing it like this instead of using a Boolean, we allow the Rust compiler to use the learnability in the niche left by the Gram type argument.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SentenceGram<G> {
    Learnable(G),
    Obvious(G),
}

impl<S> From<Gram<S>> for SentenceGram<Gram<S>> {
    fn from(gram: Gram<S>) -> Self {
        if gram.is_learnable() {
            SentenceGram::Learnable(gram)
        } else {
            SentenceGram::Obvious(gram)
        }
    }
}

impl<G> SentenceGram<G> {
    /// Get a reference to the inner gram
    pub fn learnable(&self) -> Option<&G> {
        match self {
            SentenceGram::Learnable(g) => Some(g),
            SentenceGram::Obvious(_) => None,
        }
    }

    /// Transform the inner gram type with a fallible function
    pub fn try_map<H, F: FnOnce(G) -> Option<H>>(self, f: F) -> Option<SentenceGram<H>> {
        match self {
            SentenceGram::Learnable(g) => Some(SentenceGram::Learnable(f(g)?)),
            SentenceGram::Obvious(g) => Some(SentenceGram::Obvious(f(g)?)),
        }
    }
}

impl SentenceGram<Gram<String>> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> SentenceGram<Gram<lasso::Spur>> {
        match self {
            SentenceGram::Learnable(gram) => SentenceGram::Learnable(gram.get_or_intern(rodeo)),
            SentenceGram::Obvious(gram) => SentenceGram::Obvious(gram.get_or_intern(rodeo)),
        }
    }

    pub fn get_interned(
        &self,
        rodeo: &lasso::RodeoReader,
    ) -> Option<SentenceGram<Gram<lasso::Spur>>> {
        match self {
            SentenceGram::Learnable(gram) => {
                Some(SentenceGram::Learnable(gram.get_interned(rodeo)?))
            }
            SentenceGram::Obvious(gram) => Some(SentenceGram::Obvious(gram.get_interned(rodeo)?)),
        }
    }
}

/// A vocabulary entry for a gram (supertoken)
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct GramVocabEntry<S> {
    /// The atoms that make up this gram
    pub atoms: Gram<S>,
    /// Frequency count in the corpus
    pub frequency: u32,
}

impl GramVocabEntry<String> {
    pub fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> GramVocabEntry<lasso::Spur> {
        GramVocabEntry {
            atoms: self.atoms.get_or_intern(rodeo),
            frequency: self.frequency,
        }
    }

    pub fn get_interned(&self, rodeo: &lasso::RodeoReader) -> Option<GramVocabEntry<lasso::Spur>> {
        Some(GramVocabEntry {
            atoms: self.atoms.get_interned(rodeo)?,
            frequency: self.frequency,
        })
    }
}

impl GramVocabEntry<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> GramVocabEntry<String> {
        GramVocabEntry {
            atoms: self.atoms.resolve(rodeo),
            frequency: self.frequency,
        }
    }
}

/// Consolidated data structure containing all generated language data
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConsolidatedLanguageData {
    /// All target language sentences from Anki cards
    pub target_language_sentences: Vec<String>,
    /// Mapping from target language sentences to all native translations
    pub translations: Vec<(String, Vec<String>)>,
    /// NLP-analyzed sentences with multiword terms and heteronyms
    pub nlp_sentences: Vec<(String, SentenceInfo)>,
    /// Unified phrasebook: maps gram to definition entry (both MWE phrases and learned multi-atom grams)
    pub phrasebook: BTreeMap<Gram<String>, PhrasebookDefinitionEntry>,
    /// Proper noun definitions for names, places, organizations
    pub proper_noun_definitions: BTreeMap<String, ProperNounDefinition>,
    /// Per-source gram frequencies (movies, Pimsleur lessons, etc.)
    pub source_gram_frequencies: FxHashMap<FrequencySourceId, GramFrequencyList>,
    /// Mapping from words to their IPA pronunciations
    pub word_to_pronunciation: Vec<(String, Pronunciation)>,
    /// Mapping from IPA pronunciations to lists of words
    pub pronunciation_to_words: Vec<(Pronunciation, Vec<String>)>,
    /// Pronunciation patterns and guides for the course
    pub pronunciation_data: PronunciationData,
    /// Homophone disambiguation practice sentences
    pub homophone_practice: BTreeMap<HomophoneWordPair<String>, HomophonePractice<String>>,
    /// Movie metadata indexed by movie ID
    pub movies: FxHashMap<String, MovieMetadata>,
    /// Sentence source provenance tracking (including movie_ids)
    pub sentence_sources: Vec<(String, SentenceSource)>,
    /// Gram vocabulary: maps gram ID to display info (index = gram ID)
    pub gram_vocabulary: Vec<GramVocabEntry<String>>,
    /// Gram frequencies for learnable grams
    pub gram_frequencies: GramFrequencyList,
    /// Encoded sentences: sentence text -> grams with learnability and capitalize_first
    pub encoded_sentences: Vec<(String, SentenceGrams<Gram<String>>)>,
    /// Gram dictionary: definitions for grams (keyed by Gram for correct surface-form matching)
    pub gram_dictionary: BTreeMap<Gram<String>, DictionaryEntry>,
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

        for gram in self.gram_dictionary.keys() {
            for atom in gram.iter() {
                if let Atom::Tok(word) = atom {
                    rodeo.get_or_intern(&word.text);
                    if let WordType::Heteronym(h) = &word.word_type {
                        rodeo.get_or_intern(&h.word);
                        rodeo.get_or_intern(&h.lemma);
                    }
                }
            }
        }
        for entry in self.phrasebook.values() {
            rodeo.get_or_intern(&entry.target_language_multi_word_term);
        }

        // Intern words used in sentences (includes proper nouns, plus capitalization might differ)
        for (_, sentence_info) in &self.nlp_sentences {
            for literal in &sentence_info.words {
                rodeo.get_or_intern(&literal.word.text);
                rodeo.get_or_intern(&literal.whitespace);
                // Also intern word and lemma if it's a heteronym
                if let WordType::Heteronym(h) = &literal.word.word_type {
                    rodeo.get_or_intern(&h.word);
                    rodeo.get_or_intern(&h.lemma);
                }
            }
        }

        // intern pronunciations
        for (_word, pronunciation) in &self.word_to_pronunciation {
            rodeo.get_or_intern(pronunciation);
        }

        // intern pronunciation data
        for (sound, _) in &self.pronunciation_data.sounds {
            rodeo.get_or_intern(sound);
        }
        for guide in &self.pronunciation_data.guides {
            rodeo.get_or_intern(&guide.pattern);
            rodeo.get_or_intern(&guide.description);
            for word_pair in &guide.example_words {
                rodeo.get_or_intern(&word_pair.target);
                rodeo.get_or_intern(&word_pair.native);
                rodeo.get_or_intern(&word_pair.cultural_context);
            }
        }

        // intern homophone practice data
        for (word_pair, practice) in &self.homophone_practice {
            word_pair.get_or_intern(rodeo);
            practice.get_or_intern(rodeo);
        }

        // intern movie data
        for movie in self.movies.values() {
            rodeo.get_or_intern(&movie.id);
            rodeo.get_or_intern(&movie.title);
        }

        // intern sentence sources (sentences already interned, just need movie_ids)
        for (sentence, source) in &self.sentence_sources {
            rodeo.get_or_intern(sentence);
            for movie_id in &source.movie_ids {
                rodeo.get_or_intern(movie_id);
            }
        }

        // intern proper noun definitions keys
        for proper_noun in self.proper_noun_definitions.keys() {
            rodeo.get_or_intern(proper_noun);
        }

        // intern gram vocabulary atom texts
        for entry in &self.gram_vocabulary {
            for atom in &entry.atoms {
                if let Atom::<String>::Tok(word) = atom {
                    rodeo.get_or_intern(&word.text);
                    // Also intern word and lemma if it's a heteronym
                    if let WordType::Heteronym(h) = &word.word_type {
                        rodeo.get_or_intern(&h.word);
                        rodeo.get_or_intern(&h.lemma);
                    }
                }
            }
        }

        // intern encoded sentence keys, atoms, and multiword term grams
        for (sentence, encoded) in &self.encoded_sentences {
            rodeo.get_or_intern(sentence);
            for gram in &encoded.grams {
                match gram {
                    SentenceGram::Learnable(atoms) | SentenceGram::Obvious(atoms) => {
                        for atom in atoms {
                            atom.get_or_intern(rodeo);
                        }
                    }
                }
            }
            for gram in &encoded.multiword_terms {
                for atom in gram.iter() {
                    atom.get_or_intern(rodeo);
                }
            }
            for gram in &encoded.low_confidence_multiword_terms {
                for atom in gram.iter() {
                    atom.get_or_intern(rodeo);
                }
            }
        }

        // intern source gram frequencies
        for freq_list in self.source_gram_frequencies.values() {
            for entry in &freq_list.entries {
                for atom in &entry.gram {
                    if let Atom::<String>::Tok(word) = atom {
                        rodeo.get_or_intern(&word.text);
                        if let WordType::Heteronym(h) = &word.word_type {
                            rodeo.get_or_intern(&h.word);
                            rodeo.get_or_intern(&h.lemma);
                        }
                    }
                }
            }
        }

        // intern master gram frequencies
        for entry in &self.gram_frequencies.entries {
            for atom in &entry.gram {
                if let Atom::<String>::Tok(word) = atom {
                    rodeo.get_or_intern(&word.text);
                    if let WordType::Heteronym(h) = &word.word_type {
                        rodeo.get_or_intern(&h.word);
                        rodeo.get_or_intern(&h.lemma);
                    }
                }
            }
        }
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum TtsProvider {
    ElevenLabs,
    Google,
}

pub type Pronunciation = String;

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
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum PronunciationDifficulty {
    Easy,
    Medium,
    Hard,
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum PronunciationFamiliarity {
    LikelyAlreadyKnows,
    MaybeAlreadyKnows,
    ProbablyDoesNotKnow,
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LanguageSoundPattern {
    pub pattern: String, // e.g. "ch", "ent$", "^h"
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum SoundPosition {
    Beginning,
    Middle,
    End,
    Multiple,
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
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum PatternPosition {
    Beginning,
    End,
    Anywhere,
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WordPair {
    pub target: String,
    pub native: String,
    pub position: SoundPosition,  // Where the sound appears in the word
    pub cultural_context: String, // Cultural reference or familiarity note (in native language)
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PronunciationGuideThoughts {
    pub thoughts: String,
    pub pattern: String,
    pub position: PatternPosition,
    pub description: String,
    pub familiarity: PronunciationFamiliarity,
    pub difficulty: PronunciationDifficulty,
    pub example_words: Vec<WordPair>,
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PronunciationGuide {
    pub pattern: String,
    pub position: PatternPosition,
    pub description: String,
    pub familiarity: PronunciationFamiliarity,
    pub difficulty: PronunciationDifficulty,
    pub example_words: Vec<WordPair>,
}

impl From<PronunciationGuideThoughts> for PronunciationGuide {
    fn from(thoughts: PronunciationGuideThoughts) -> Self {
        Self {
            pattern: thoughts.pattern,
            position: thoughts.position,
            description: thoughts.description,
            familiarity: thoughts.familiarity,
            difficulty: thoughts.difficulty,
            example_words: thoughts.example_words,
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
    tsify::Tsify,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PronunciationData {
    pub sounds: Vec<(String, PatternPosition)>, // List of characteristic sounds/patterns for the language
    pub guides: Vec<PronunciationGuide>,        // Detailed guides for each sound
    pub pattern_frequencies: Vec<((String, PatternPosition), u32)>, // Pattern frequencies sorted by frequency (descending)
}

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
    Korean,
    German,
    Chinese,
    Japanese,
    Russian,
    Portuguese,
    Italian,
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    tsify::Tsify,
    schemars::JsonSchema,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum WritingSystem {
    /// Latin alphabet (Romance languages, Germanic languages, etc.)
    Latin,
    /// Korean Hangul script
    Hangul,
    /// Cyrillic alphabet (Russian, etc.)
    Cyrillic,
    /// Chinese Han characters (simplified and traditional)
    Han,
    /// Japanese writing system (combines Kanji, Hiragana, and Katakana)
    Japanese,
}

impl Language {
    pub fn iso_639_3(&self) -> &str {
        match self {
            Language::French => "fra",
            Language::English => "eng",
            Language::Spanish => "spa",
            Language::Korean => "kor",
            Language::German => "deu",
            Language::Chinese => "zho",
            Language::Japanese => "jpn",
            Language::Russian => "rus",
            Language::Portuguese => "por",
            Language::Italian => "ita",
        }
    }

    pub fn from_iso_639_3(code: &str) -> Option<Self> {
        Some(match code {
            "fra" => Language::French,
            "eng" => Language::English,
            "spa" => Language::Spanish,
            "kor" => Language::Korean,
            "deu" => Language::German,
            "zho" => Language::Chinese,
            "jpn" => Language::Japanese,
            "rus" => Language::Russian,
            "por" => Language::Portuguese,
            "ita" => Language::Italian,
            _ => return None,
        })
    }

    pub fn iso_639_1(&self) -> &'static str {
        match self {
            Language::French => "fr",
            Language::English => "en",
            Language::Spanish => "es",
            Language::Korean => "ko",
            Language::German => "de",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Russian => "ru",
            Language::Portuguese => "pt",
            Language::Italian => "it",
        }
    }

    pub fn writing_system(&self) -> WritingSystem {
        match self {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Portuguese
            | Language::Italian => WritingSystem::Latin,
            Language::Korean => WritingSystem::Hangul,
            Language::Russian => WritingSystem::Cyrillic,
            Language::Chinese => WritingSystem::Han,
            Language::Japanese => WritingSystem::Japanese,
        }
    }

    pub fn tv_politeness(&self) -> bool {
        matches!(
            self,
            Language::French
                | Language::Spanish
                | Language::German
                | Language::Portuguese
                | Language::Italian
        )
    }

    /// OpenSubtitles API language code (usually ISO 639-1, but pt-br for Portuguese)
    pub fn opensubtitles_language_code(&self) -> &'static str {
        match self {
            Language::Portuguese => "pt-br",
            other => other.iso_639_1(),
        }
    }

    /// TMDB API language code (language-REGION format)
    pub fn tmdb_language_code(&self) -> &'static str {
        match self {
            Language::French => "fr-FR",
            Language::English => "en-US",
            Language::Spanish => "es-ES",
            Language::German => "de-DE",
            Language::Korean => "ko-KR",
            Language::Chinese => "zh-CN",
            Language::Japanese => "ja-JP",
            Language::Russian => "ru-RU",
            Language::Portuguese => "pt-BR",
            Language::Italian => "it-IT",
        }
    }

    /// Words that should appear in virtually any movie's subtitles for this language.
    /// Used as a sanity check to detect wrong-language subtitle files.
    /// Returns a list of words where ALL must appear at least once (as whole words) in the subtitle text.
    pub fn subtitle_sanity_words(&self) -> &'static [&'static str] {
        match self {
            Language::French => &["le", "de", "pas", "je"],
            Language::English => &["the", "to", "you", "is"],
            Language::Spanish => &["el", "de", "no", "que"],
            Language::German => &["ich", "das", "nicht", "du"],
            Language::Korean => &["이", "는", "을", "에"],
            Language::Chinese => &["的", "了", "是", "不"],
            Language::Japanese => &["の", "は", "を", "に"],
            Language::Russian => &["не", "что", "на", "это"],
            Language::Portuguese => &["que", "de", "não", "eu"],
            Language::Italian => &["che", "di", "non", "il"],
        }
    }

    /// Substrings that should NEVER appear in properly-encoded subtitles for this language.
    /// Used to detect systematic character corruption (e.g. t→r substitution from bad OCR/encoding).
    /// If ANY of these substrings appear frequently, the subtitle file is corrupt.
    /// Returns (forbidden_substring, what_it_should_be) pairs.
    pub fn subtitle_corruption_markers(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            // t→r corruption: "est"→"esr", "être"→"êrre", "tout"→"rour", "cette"→"cerre"
            // OCR corruption: "Il"→"II" (two capital I's), "Il"→"ll", "l'"→"I'" (capital I for l)
            Language::French => &[
                ("c'esr", "c'est"),
                ("qu'esr", "qu'est"),
                ("êrre", "être"),
                ("rour", "tout"),
                ("cerre", "cette"),
                ("conrre", "contre"),
                ("enrre", "entre"),
                ("aurre", "autre"),
                ("perir", "petit"),
                ("norre", "notre"),
                ("vorre", "votre"),
                // OCR: two capital I's instead of "Il"
                ("II ", "Il "),
                // OCR: two lowercase l's instead of "Il"
                (" ll ", " Il "),
                // OCR: capital I instead of lowercase l before apostrophe
                // Detected separately via ocr_i_apostrophe check in check_subtitle_sanity
            ],
            // t→r corruption: "the"→"rhe", "that"→"rhar", "this"→"rhis", "it"→"ir"
            // OCR: lowercase l for uppercase I: "l'm" for "I'm", "lt" for "It"
            Language::English => &[
                (" rhe ", " the "),
                ("rhar", "that"),
                ("rhis", "this"),
                ("rhere", "there"),
                ("rhey", "they"),
                ("whar", "what"),
                ("abour", "about"),
                ("jusr", "just"),
                ("righr", "right"),
                ("don'r", "don't"),
                // OCR: lowercase l for uppercase I
                (" l'm", " I'm"),
                (" l'd", " I'd"),
                (" l'll", " I'll"),
                (" lt ", " It "),
                (" ln ", " In "),
                (" lf ", " If "),
            ],
            // t→r corruption: "está"→"esrá", "todo"→"rodo", "tiene"→"riene"
            Language::Spanish => &[
                ("esrá", "está"),
                ("esro", "esto"),
                ("riene", "tiene"),
                ("riempo", "tiempo"),
                ("rambién", "también"),
                ("conrra", "contra"),
                ("nuesrro", "nuestro"),
                // NOTE: "orro" and "parr" omitted — too many false positives from
                // legitimate Spanish words (socorro, horror, zorro, parrilla, parroquia, etc.)
            ],
            // t→r corruption: "nicht"→"nichr", "ist"→"isr", "mit"→"mir" (ambiguous)
            // OCR: lowercase l for uppercase I: "lch" for "Ich", "lhr" for "Ihr"
            Language::German => &[
                ("nichr", "nicht"),
                ("nichrs", "nichts"),
                ("jerzt", "jetzt"),
                ("harre", "hatte"),
                ("birer", "bitte"),
                ("lerzr", "letzt"),
                ("mussr", "musst"),
                ("kannsr", "kannst"),
                // OCR: lowercase l for uppercase I
                (" lch ", " Ich "),
                (" lhr", " Ihr"),
                (" lhm", " Ihm"),
                (" lhn", " Ihn"),
                (" lst ", " Ist "),
                (" ln ", " In "),
            ],
            // t→r corruption: "tutto"→"rurro", "questo"→"quesr-", "fatto"→"farro"
            // OCR: "Il"→"II" or "ll", "In"→"ln", zz→e'e'
            Language::Italian => &[
                ("rurro", "tutto"),
                ("rurra", "tutta"),
                ("quesr", "quest"),
                ("farro", "fatto"),
                ("derro", "detto"),
                ("sraro", "stato"),
                ("conrro", "contro"),
                ("nienre", "niente"),
                // OCR: two capital I's or two lowercase l's instead of "Il"
                ("II ", "Il "),
                (" ll ", " Il "),
                // OCR: "In" → "ln"
                (" ln ", " In "),
                // OCR: zz→e'e' (bizarre but systematic): "soluzione"→"solue'ione"
                ("e'e'", "zz"),
                // OCR: í (accented i) replacing normal i in common words
                ("Grazíe", "Grazie"),
                ("prímo", "primo"),
                ("díre", "dire"),
            ],
            // t→r corruption: "está"→"esrá", "tudo"→"rudo", "tem"→"rem" (ambiguous)
            // OCR: uppercase I for lowercase l: "paIavra" for "palavra"
            // OCR: "-Io" for "-lo" (clitic), "Ihe" for "lhe"
            Language::Portuguese => &[
                ("esrá", "está"),
                ("esre", "este"),
                ("rudo", "tudo"),
                ("conrra", "contra"),
                ("ourro", "outro"),
                ("denrro", "dentro"),
                ("enrão", "então"),
                // OCR: uppercase I substituted for lowercase l
                ("eIe", "ele"),
                ("paIavra", "palavra"),
                ("fIor", "flor"),
                ("Iugar", "lugar"),
                ("Iindo", "lindo"),
                ("reaImente", "realmente"),
                // OCR: I for l in clitics and common words
                ("-Io ", "-lo "),
                ("á-Io", "á-lo"),
                ("ê-Io", "ê-lo"),
                ("Ihe ", "lhe "),
                ("Ihes ", "lhes "),
                ("úItim", "últim"),
            ],
            // Encoding corruption: Windows-1251 decoded as Latin-1/ISO-8859-1 produces
            // garbled sequences like "Ð" prefixes. Also Latin homoglyphs mixed into Cyrillic:
            // Latin "a", "e", "o", "c", "p", "x" look identical to Cyrillic "а", "е", "о", "с", "р", "х"
            // but break text processing. We detect systematic Latin-for-Cyrillic substitution
            // by looking for Latin letters surrounded by Cyrillic context.
            Language::Russian => &[
                // Windows-1251 → Latin-1 mojibake: Cyrillic capital letters become Ð+something
                ("Ð\u{00B0}", "а"), // а
                ("Ð\u{00B5}", "е"), // е
                ("Ð¾", "о"),        // о (common mojibake pattern)
                ("Ñ\u{0082}", "т"), // т
                ("Ñ\u{0080}", "р"), // р
                // OCR: Ь (soft sign) misread as b
                ("6ы", "бы"),
                // Digit 3 for З (Ze)
                ("3а", "За"),
                ("3де", "Зде"),
            ],
            // No Latin script subtitles for these
            Language::Korean | Language::Chinese | Language::Japanese => &[],
        }
    }

    /// Count Latin homoglyph characters that appear inside words that are otherwise Cyrillic.
    /// This detects subtitle corruption where visually identical Latin letters
    /// (a, e, o, c, p, x, y, A, B, C, E, H, K, M, O, P, T, X) replace their Cyrillic
    /// counterparts (а, е, о, с, р, х, у, А, В, С, Е, Н, К, М, О, Р, Т, Х).
    /// Non-homoglyph Latin chars (b, d, f, g, etc.) are ignored since they appear
    /// legitimately in foreign words with Russian case endings (e.g., "Mercedes'ом").
    fn count_latin_in_cyrillic_words(text: &str) -> usize {
        // Latin chars that are visual homoglyphs of Cyrillic chars
        const LATIN_HOMOGLYPHS: &[char] = &[
            'a', 'e', 'o', 'c', 'p', 'x', 'y', // lowercase
            'A', 'B', 'C', 'E', 'H', 'K', 'M', 'O', 'P', 'T', 'X', // uppercase
        ];
        let mut count = 0;
        for word in text.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()) {
            if word.is_empty() {
                continue;
            }
            let mut cyrillic_chars = 0usize;
            let mut homoglyph_chars = 0usize;
            for c in word.chars() {
                if ('\u{0400}'..='\u{04FF}').contains(&c) {
                    cyrillic_chars += 1;
                } else if LATIN_HOMOGLYPHS.contains(&c) {
                    homoglyph_chars += 1;
                }
            }
            // Only flag words that are mostly Cyrillic but have Latin homoglyphs mixed in
            if cyrillic_chars >= 2 && homoglyph_chars >= 1 {
                count += homoglyph_chars;
            }
        }
        count
    }

    /// Unified subtitle sanity check. Takes an iterator of subtitle sentence strings.
    /// Returns `Ok(())` if the subtitles pass, or `Err(reason)` describing why they failed.
    /// `skip_markers` allows whitelisting specific corruption markers for files with known
    /// false positives (e.g., a character named "Rourke" triggering the "rour" marker).
    pub fn check_subtitle_sanity<'a>(
        &self,
        sentences: impl Iterator<Item = &'a str>,
        skip_markers: &[&str],
    ) -> Result<(), String> {
        let lines: Vec<&str> = sentences.collect();
        let total_lines = lines.len();

        // 0. Minimum line count — reject fragments
        if total_lines < 75 {
            return Err(format!("too few lines ({total_lines}), likely a fragment"));
        }

        let all_text: String = lines.join(" ");
        let all_text_lower = all_text.to_lowercase();

        // 1. Check that all required sanity words appear
        let sanity_words = self.subtitle_sanity_words();
        for &word in sanity_words {
            let found = match self.writing_system() {
                WritingSystem::Latin | WritingSystem::Cyrillic => all_text_lower
                    .split(|c: char| !c.is_alphanumeric() && c != '\'')
                    .any(|w| w == word),
                _ => all_text_lower.contains(word),
            };
            if !found {
                return Err(format!("missing required word \"{word}\""));
            }
        }

        // 2. Check for systematic character corruption (e.g. t→r)
        let corruption_markers = self.subtitle_corruption_markers();
        for &(marker, expected) in corruption_markers {
            if skip_markers.contains(&marker) {
                continue;
            }
            // Some markers are case-sensitive (OCR patterns like "II"), check against original
            let count = if marker.chars().any(|c| c.is_uppercase()) {
                all_text.matches(marker).count()
            } else {
                all_text_lower.matches(marker).count()
            };
            if count >= 3 {
                return Err(format!(
                    "corruption: found \"{marker}\" {count} times (should be \"{expected}\")"
                ));
            }
        }

        // 3. OCR "I'" where "l'" should be (I followed by ' then lowercase) — French and Italian
        if matches!(self, Language::French | Language::Italian) {
            let mut i_apos_count = 0usize;
            let chars: Vec<char> = all_text.chars().collect();
            for i in 0..chars.len().saturating_sub(2) {
                if chars[i] == 'I'
                    && (chars[i + 1] == '\'' || chars[i + 1] == '\u{2019}')
                    && chars[i + 2].is_lowercase()
                {
                    i_apos_count += 1;
                }
            }
            if i_apos_count >= 5 {
                return Err(format!(
                    "OCR corruption: found \"I'\" followed by lowercase {i_apos_count} times (should be \"l'\")"
                ));
            }
        }

        // 4. SSA/ASS subtitle formatting codes that should have been stripped
        let ssa_count = all_text.matches("{\\an").count()
            + all_text.matches("{\\i1}").count()
            + all_text.matches("{\\i0}").count()
            + all_text.matches("{\\fs").count()
            + all_text.matches("{\\pos").count()
            + all_text.matches("{\\c&").count()
            + all_text.matches("{\\frz").count()
            + all_text.matches("\\h").count();
        if ssa_count >= 10 {
            return Err(format!(
                "SSA/ASS formatting codes found {ssa_count} times (should be stripped)"
            ));
        }

        // 5. BOM characters in text
        let bom_count = all_text.matches('\u{FEFF}').count();
        if bom_count >= 5 {
            return Err(format!("BOM characters (U+FEFF) found {bom_count} times"));
        }

        // 6. C1 control characters (U+0080-U+009F) indicate Windows-1252 mojibake
        let c1_count = all_text
            .chars()
            .filter(|&c| ('\u{0080}'..='\u{009F}').contains(&c))
            .count();
        if c1_count >= 5 {
            return Err(format!(
                "C1 control characters found {c1_count} times (Windows-1252 encoding corruption)"
            ));
        }

        // 7. Greek homoglyphs mixed into Latin text (subtitle copy-protection)
        if matches!(
            self.writing_system(),
            WritingSystem::Latin | WritingSystem::Cyrillic
        ) {
            let greek_count = all_text
                .chars()
                .filter(|&c| ('\u{0370}'..='\u{03FF}').contains(&c))
                .count();
            if greek_count >= 5 {
                return Err(format!(
                    "Greek homoglyph characters found {greek_count} times (copy-protection corruption)"
                ));
            }
        }

        // 8. Backtick or acute accent used as apostrophe
        let bad_apostrophe_count = all_text
            .chars()
            .filter(|&c| c == '`' || c == '\u{00B4}')
            .count();
        if bad_apostrophe_count >= 10 {
            return Err(format!(
                "Backtick/acute accent used as apostrophe {bad_apostrophe_count} times"
            ));
        }

        // 9. CJK characters in non-CJK subtitle files
        if !matches!(
            self,
            Language::Chinese | Language::Japanese | Language::Korean
        ) {
            let cjk_count = all_text
                .chars()
                .filter(|&c| {
                    ('\u{4E00}'..='\u{9FFF}').contains(&c)
                        || ('\u{3400}'..='\u{4DBF}').contains(&c)
                        || ('\u{F900}'..='\u{FAFF}').contains(&c)
                })
                .count();
            if cjk_count >= 20 {
                return Err(format!(
                    "CJK characters found {cjk_count} times in {self} subtitles (wrong language content)"
                ));
            }
        }

        // 10. Invisible directional Unicode markers (LRM U+200E, RLE U+202B, etc.)
        let invisible_dir_count = all_text
            .chars()
            .filter(|&c| {
                c == '\u{200E}'
                    || c == '\u{200F}'
                    || c == '\u{202A}'
                    || c == '\u{202B}'
                    || c == '\u{202C}'
                    || c == '\u{202D}'
                    || c == '\u{202E}'
                    || c == '\u{2066}'
                    || c == '\u{2067}'
                    || c == '\u{2068}'
                    || c == '\u{2069}'
                    || c == '\u{3164}'
            })
            .count();
        if invisible_dir_count >= 20 {
            return Err(format!(
                "Invisible directional/filler Unicode characters found {invisible_dir_count} times"
            ));
        }

        // 11. Spanish: missing inverted punctuation ¿ and ¡
        if matches!(self, Language::Spanish) && total_lines >= 100 {
            let questions = all_text.matches('?').count();
            let inv_questions = all_text.matches('¿').count();
            // If there are many questions but zero or near-zero inverted marks
            if questions >= 20 && inv_questions * 5 < questions {
                return Err(format!(
                    "Spanish missing ¿: {questions} questions but only {inv_questions} inverted marks"
                ));
            }
        }

        // 14. OCR: "fii" for "fi" (ligature mangling)
        if matches!(
            self,
            Language::English | Language::French | Language::Italian
        ) {
            let fii_count = all_text_lower.matches("fii").count();
            if fii_count >= 5 {
                return Err(format!(
                    "OCR ligature corruption: \"fii\" found {fii_count} times (should be \"fi\")"
                ));
            }
        }

        // 15. OCR: "0" for "O" at word boundaries (0lá, 0brigado, etc.)
        if matches!(self.writing_system(), WritingSystem::Latin) {
            let zero_for_o: usize = lines
                .iter()
                .map(|line| {
                    line.split_whitespace()
                        .filter(|w| {
                            w.starts_with('0')
                                && w.len() > 1
                                && w.chars().nth(1).is_some_and(|c| c.is_alphabetic())
                        })
                        .count()
                })
                .sum();
            if zero_for_o >= 5 {
                return Err(format!(
                    "OCR: digit 0 used for letter O found {zero_for_o} times"
                ));
            }
        }

        // 16. SRT timecodes leaked into text (e.g. "00:12:34,567 --> 00:12:36,789")
        let timecode_count = lines.iter().filter(|line| line.contains("-->")).count();
        if timecode_count >= 3 {
            return Err(format!(
                "SRT timecodes found in {timecode_count} lines (format conversion error)"
            ));
        }

        // 17. Russian: Latin homoglyphs mixed into Cyrillic text
        // Latin a/e/o/c/p/x/y/A/B/C/E/H/K/M/O/P/T/X look identical to
        // Cyrillic а/е/о/с/р/х/у/А/В/С/Е/Н/К/М/О/Р/Т/Х but break text processing.
        // Count Latin letters that appear inside otherwise-Cyrillic words.
        if matches!(self, Language::Russian) {
            let latin_in_cyrillic = Self::count_latin_in_cyrillic_words(&all_text);
            if latin_in_cyrillic >= 20 {
                return Err(format!(
                    "Latin homoglyphs mixed into Cyrillic text: {latin_in_cyrillic} Latin characters found inside Cyrillic words"
                ));
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::French => write!(f, "French"),
            Language::English => write!(f, "English"),
            Language::Spanish => write!(f, "Spanish"),
            Language::Korean => write!(f, "Korean"),
            Language::German => write!(f, "German"),
            Language::Chinese => write!(f, "Chinese"),
            Language::Japanese => write!(f, "Japanese"),
            Language::Russian => write!(f, "Russian"),
            Language::Portuguese => write!(f, "Portuguese"),
            Language::Italian => write!(f, "Italian"),
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub native_language: Language,
    pub target_language: Language,
}

impl Course {
    pub fn teaches_new_writing_system(&self) -> bool {
        self.native_language.writing_system() != self.target_language.writing_system()
    }
}

pub const COURSES: &[Course] = &[
    Course {
        native_language: Language::English,
        target_language: Language::French,
    },
    Course {
        native_language: Language::French,
        target_language: Language::English,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Spanish,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Korean,
    },
    Course {
        native_language: Language::English,
        target_language: Language::German,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Italian,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Portuguese,
    },
    Course {
        native_language: Language::English,
        target_language: Language::Russian,
    },
];

pub const LANGUAGES: &[Language] = &[
    Language::French,
    Language::Spanish,
    Language::English,
    Language::Korean,
    Language::German,
    Language::Chinese,
    Language::Japanese,
    Language::Russian,
    Language::Portuguese,
    Language::Italian,
];

/// A sentence example for the landing page showcase.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
pub struct ShowcaseExampleSentence {
    pub target: String,
    pub native: String,
}

/// A phrase entry for the landing page showcase.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ShowcasePhrase {
    pub display_text: String,
    pub definition: String,
    pub examples: Vec<ShowcaseExampleSentence>,
}

/// Landing page showcase data for a single course.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct CourseShowcase {
    pub target_language: Language,
    pub native_language: Language,
    pub sentence_count: usize,
    pub phrases: Vec<ShowcasePhrase>,
}

/// A pair of homophone words, lexicographically sorted to ensure consistency
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
    schemars::JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Hash), derive(PartialEq), derive(Eq))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HomophoneWordPair<S>
where
    S: rkyv::Archive + Hash + std::fmt::Debug + Eq + PartialEq + Ord + PartialOrd,
    <S as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
{
    pub word1: S,
    pub word2: S,
}

impl HomophoneWordPair<String> {
    /// Create a new word pair, ensuring lexicographic ordering.
    /// Returns None if the words are the same.
    pub fn new(word_a: String, word_b: String) -> Option<Self> {
        if word_a == word_b {
            return None;
        }

        let (word1, word2) = if word_a < word_b {
            (word_a, word_b)
        } else {
            (word_b, word_a)
        };

        Some(Self { word1, word2 })
    }

    pub fn get_interned(
        &self,
        rodeo: &lasso::RodeoReader,
    ) -> Option<HomophoneWordPair<lasso::Spur>> {
        Some(HomophoneWordPair {
            word1: rodeo.get(&self.word1)?,
            word2: rodeo.get(&self.word2)?,
        })
    }

    fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> HomophoneWordPair<lasso::Spur> {
        HomophoneWordPair {
            word1: rodeo.get_or_intern(&self.word1),
            word2: rodeo.get_or_intern(&self.word2),
        }
    }
}

impl HomophoneWordPair<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> HomophoneWordPair<String> {
        HomophoneWordPair {
            word1: rodeo.resolve(&self.word1).to_string(),
            word2: rodeo.resolve(&self.word2).to_string(),
        }
    }
}

/// A pair of practice sentences for disambiguating two homophones
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HomophoneSentencePair<S>
where
    S: rkyv::Archive + Hash + std::fmt::Debug + Eq + PartialEq + Ord + PartialOrd,
    <S as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
{
    /// Sentence using the first word (lexicographically)
    pub sentence1: S,
    /// Sentence using the second word (lexicographically)
    pub sentence2: S,
}

impl HomophoneSentencePair<String> {
    pub fn get_interned(
        &self,
        rodeo: &lasso::RodeoReader,
    ) -> Option<HomophoneSentencePair<lasso::Spur>> {
        Some(HomophoneSentencePair {
            sentence1: rodeo.get(&self.sentence1)?,
            sentence2: rodeo.get(&self.sentence2)?,
        })
    }

    fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> HomophoneSentencePair<lasso::Spur> {
        HomophoneSentencePair {
            sentence1: rodeo.get_or_intern(&self.sentence1),
            sentence2: rodeo.get_or_intern(&self.sentence2),
        }
    }
}

impl HomophoneSentencePair<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> HomophoneSentencePair<String> {
        HomophoneSentencePair {
            sentence1: rodeo.resolve(&self.sentence1).to_string(),
            sentence2: rodeo.resolve(&self.sentence2).to_string(),
        }
    }
}
/// Complete disambiguation practice data for a pair of homophones
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HomophonePractice<S>
where
    S: rkyv::Archive + Hash + std::fmt::Debug + Eq + PartialEq + Ord + PartialOrd,
    <S as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
{
    pub sentence_pairs: Vec<HomophoneSentencePair<S>>,
}

impl HomophonePractice<String> {
    pub fn get_interned(
        &self,
        rodeo: &lasso::RodeoReader,
    ) -> Option<HomophonePractice<lasso::Spur>> {
        Some(HomophonePractice {
            sentence_pairs: self
                .sentence_pairs
                .iter()
                .map(|s| s.get_interned(rodeo).unwrap())
                .collect(),
        })
    }

    fn get_or_intern(&self, rodeo: &mut lasso::Rodeo) -> HomophonePractice<lasso::Spur> {
        HomophonePractice {
            sentence_pairs: self
                .sentence_pairs
                .iter()
                .map(|s| s.get_or_intern(rodeo))
                .collect(),
        }
    }
}

impl HomophonePractice<lasso::Spur> {
    pub fn resolve(&self, rodeo: &lasso::RodeoReader) -> HomophonePractice<String> {
        HomophonePractice {
            sentence_pairs: self
                .sentence_pairs
                .iter()
                .map(|s| s.resolve(rodeo))
                .collect(),
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TtsRequest {
    pub text: String,
    pub language: Language,
    #[serde(default)]
    pub is_ssml: bool,
}

// ============================================================================
// Whitespace prediction for reconstructing text from atoms
// ============================================================================

/// French punctuation that requires narrow non-breaking space before it
const FRENCH_HIGH_PUNCT: &[char] = &['?', '!', ';', '»'];

/// Common Korean particles that attach directly to the preceding word
const KOREAN_PARTICLES: &[&str] = &[
    "이", "가", "을", "를", "은", "는", "에", "에서", "으로", "로", "와", "과", "하고", "의", "도",
    "만", "까지", "부터", "처럼", "같이", "보다", "마다", "이나", "나",
];

/// Punctuation that typically has no space after it
/// Includes Spanish inverted punctuation (¿ ¡) which attach to the following word
const NO_SPACE_AFTER: &[char] = &[
    '(', '[', '{', '«', '\'', '\u{2018}', '"', '\u{201C}', '-', '¿', '¡',
];

/// Punctuation that typically has no space before it
/// Note: apostrophe/quote chars are handled specially (opening vs closing)
/// Note: French » (closing guillemet) has space BEFORE it, so not in this list
const NO_SPACE_BEFORE: &[char] = &[
    ')', ']', '}', ',', '.', '?', '!', ';', '\u{2019}', '"', '\u{201D}', '-', '…',
];

/// Characters that end words and attach directly (apostrophes, hyphens in compounds)
const ATTACHING_SUFFIXES: &[char] = &['\'', '\u{2019}', '-'];

/// Capitalize the first letter of a string, leaving the rest unchanged.
pub fn capitalize_first_letter(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// Returns true if the first letter of this word is always capitalized regardless of position.
///
/// This is used to avoid lowercasing words that carry meaning through their capitalization:
/// - Proper nouns in any language (e.g. "Paris", "Marie")
/// - All nouns in German (German capitalizes every noun)
/// - The pronoun "I" in English
pub fn first_letter_always_capitalized<S: AsRef<str>>(word: &Word<S>, language: Language) -> bool {
    // English "I" is always capitalized
    if language == Language::English && word.text.as_ref() == "I" {
        return true;
    }
    match &word.word_type {
        WordType::Other(other) => matches!(other.other_tag, OtherWordType::Propn),
        WordType::Heteronym(h) => language == Language::German && h.pos == PartOfSpeech::Noun,
    }
}

/// Lowercase the first letter of a word to match encoded gram form,
/// but only if the word's capitalization isn't intrinsic (e.g. proper nouns,
/// German nouns). Returns whether a change was made.
pub fn normalize_word_capitalization_for_gram_matching(
    word: &mut Word<String>,
    language: Language,
) -> bool {
    if !first_letter_always_capitalized(word, language) {
        let (lowercased, changed) = lowercase_first_letter(&word.text);
        if changed {
            word.text = lowercased;
            return true;
        }
    }
    false
}

/// Lowercase the first letter of a string, leaving the rest unchanged.
/// Returns the modified string and whether a change was made.
pub fn lowercase_first_letter(s: &str) -> (String, bool) {
    let mut chars = s.chars();
    match chars.next() {
        None => (String::new(), false),
        Some(first) => {
            if first.is_uppercase() {
                let lowercased: String = first.to_lowercase().chain(chars).collect();
                let changed = lowercased != s;
                (lowercased, changed)
            } else {
                (s.to_string(), false)
            }
        }
    }
}

/// Predicts the whitespace between two tokens based on deterministic rules.
///
/// This is the core function that enables whitespace normalization. By predicting
/// whitespace from token properties, we can omit explicit whitespace storage
/// and only emit Control tokens when the prediction is wrong.
///
/// Rules:
/// - After apostrophes/elisions: no space (l'amour, j'ai)
/// - Before French high punctuation (?, !, ;, ») in French: narrow nbsp
/// - After opening brackets/quotes: no space
/// - Before closing brackets/quotes/punctuation: no space
/// - After hyphen in compounds: no space
/// - Default: space
pub fn predict_whitespace(
    left: &Word<String>,
    right: Option<&Word<String>>,
    language: Language,
) -> Whitespace {
    let left_text = &left.text;

    // If there's no right token, no whitespace needed
    let right = match right {
        Some(r) => r,
        None => return Whitespace::None,
    };

    let right_text = &right.text;

    // Get the last char of left and first char of right
    let left_last = left_text.chars().last();
    let right_first = right_text.chars().next();

    // Check if left ends with an attaching suffix (apostrophe, hyphen)
    if let Some(c) = left_last
        && ATTACHING_SUFFIXES.contains(&c)
    {
        return Whitespace::None;
    }

    // Check if right starts with certain punctuation
    if let Some(c) = right_first {
        // No space before closing punct, commas, periods
        if NO_SPACE_BEFORE.contains(&c) {
            // Special case: French high punctuation needs narrow nbsp (only for French)
            if language == Language::French && FRENCH_HIGH_PUNCT.contains(&c) && c != '»' {
                return Whitespace::NarrowNbsp;
            }
            return Whitespace::None;
        }
    }

    // Check if left ends with opening bracket/quote (no space after)
    if let Some(c) = left_last
        && NO_SPACE_AFTER.contains(&c)
    {
        return Whitespace::None;
    }

    // Check if left is punctuation and right is punctuation (no space between)
    // Exception: after colon before quotes, there's a space (e.g., "dit : 'hello'")
    // Exception: Spanish inverted punctuation (¿ ¡) has space before it after comma
    let left_is_punct =
        matches!(&left.word_type, WordType::Other(o) if o.other_tag == OtherWordType::Punct);
    let right_is_punct =
        matches!(&right.word_type, WordType::Other(o) if o.other_tag == OtherWordType::Punct);

    if left_is_punct && right_is_punct {
        // After colon, there's typically a space (before quotes, etc.)
        if left_last == Some(':') {
            return Whitespace::Space;
        }
        // Spanish: space before inverted punctuation (¿ ¡) after comma/period
        if right_first == Some('¿') || right_first == Some('¡') {
            return Whitespace::Space;
        }
        return Whitespace::None;
    }

    // Korean: particles attach directly to the preceding word, but only when
    // the POS actually indicates a particle/adposition (not a verb or other POS
    // that happens to share the same text as a particle).
    if language == Language::Korean && KOREAN_PARTICLES.contains(&right_text.as_str()) {
        let is_particle_pos = match &right.word_type {
            WordType::Heteronym(h) => matches!(h.pos, PartOfSpeech::Part | PartOfSpeech::Adp),
            WordType::Other(_) => true, // Non-heteronym tokens that match particle text are likely particles
        };
        if is_particle_pos {
            return Whitespace::None;
        }
    }

    // Default: regular space
    Whitespace::Space
}

/// Convert a sequence of Literals into a sequence of Atoms.
///
/// This is the forward conversion that removes explicit whitespace and
/// replaces it with Control tokens where the prediction is wrong.
pub fn literals_to_atoms(
    literals: &[Literal<String>],
    language: Language,
) -> (Vec<Atom<String>>, bool) {
    if literals.is_empty() {
        return (Vec::new(), false);
    }

    let mut atoms = Vec::new();
    let mut capitalize_first = false;

    for (i, literal) in literals.iter().enumerate() {
        let mut word = literal.word.clone();

        if i == 0 && normalize_word_capitalization_for_gram_matching(&mut word, language) {
            capitalize_first = true;
        }

        // Emit the word token
        atoms.push(Atom::Tok(word.clone()));

        // Check if we need a control token for whitespace
        let next_word = literals.get(i + 1).map(|l| &l.word);
        let predicted = predict_whitespace(&word, next_word, language);
        let actual: Whitespace = literal.whitespace.parse().unwrap();

        // If prediction is wrong, emit a control token
        if predicted != actual {
            atoms.push(Atom::Control(ControlToken(actual)));
        }
    }

    (atoms, capitalize_first)
}

/// Convert a sequence of Atoms back into Literals.
///
/// This is the reverse conversion that reconstructs the original
/// whitespace from predictions and control tokens.
pub fn atoms_to_literals(atoms: &[Atom<String>], language: Language) -> Vec<Literal<String>> {
    let mut literals = Vec::new();
    let mut i = 0;

    while i < atoms.len() {
        let atom = &atoms[i];

        match atom {
            Atom::<String>::Tok(word) => {
                let word = word.clone();

                // Look ahead to determine whitespace
                let whitespace = if i + 1 < atoms.len() {
                    match &atoms[i + 1] {
                        // If next is a control token, use its whitespace
                        Atom::<String>::Control(ctrl) => {
                            i += 1; // consume the control token
                            ctrl.0
                        }
                        // Otherwise predict based on next word
                        Atom::<String>::Tok(next_word) => {
                            predict_whitespace(&word, Some(next_word), language)
                        }
                    }
                } else {
                    // Last token - predict with no lookahead
                    predict_whitespace(&word, None, language)
                };

                literals.push(Literal {
                    word,
                    whitespace: whitespace.to_str().to_string(),
                });
            }
            Atom::<String>::Control(_) => {
                // Standalone control tokens shouldn't happen in well-formed input,
                // but if they do, skip them
            }
        }

        i += 1;
    }

    literals
}

/// Reconstruct the original sentence text from literals
pub fn literals_to_text(literals: &[Literal<String>]) -> String {
    literals
        .iter()
        .map(|lit| format!("{}{}", lit.word.text, lit.whitespace))
        .collect()
}
