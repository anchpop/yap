//! Omnigram: Whitespace-normalized tokenization for supertoken training
//!
//! This module provides a tokenization layer that removes whitespace information from tokens,
//! enabling training a supertoken vocabulary (Unigram) that can merge common phrases
//! without whitespace interfering.

pub mod unigram;

use language_utils::{
    Atom, ControlToken, Language, Literal, OtherWordType, Whitespace, Word, WordType,
    literals_to_atoms, predict_whitespace,
};
use unigram::{Seq, UnigramToken};

/// Type alias for backwards compatibility (was named Control, now ControlToken)
pub type Control = ControlToken;

/// A merged token representing two or more words that have been combined.
/// Invariant: always has at least two words (first and last), with zero or more atoms in between.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergedToken {
    pub first: Word<String>,
    pub middle: Vec<Atom<String>>,
    pub last: Word<String>,
}

impl MergedToken {
    /// Create a new merged token from two words
    pub fn new(first: Word<String>, last: Word<String>) -> Self {
        Self {
            first,
            middle: Vec::new(),
            last,
        }
    }

    /// Create a merged token with middle atoms
    pub fn with_middle(first: Word<String>, middle: Vec<Atom<String>>, last: Word<String>) -> Self {
        Self {
            first,
            middle,
            last,
        }
    }

    /// Get all words in this merged token
    pub fn words(&self) -> impl Iterator<Item = &Word<String>> {
        std::iter::once(&self.first)
            .chain(self.middle.iter().filter_map(|a| match a {
                Atom::<String>::Tok(w) => Some(w),
                Atom::<String>::Control(_) => None,
            }))
            .chain(std::iter::once(&self.last))
    }
}

/// A supertoken is either a single atom or a merged token.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SuperToken {
    /// A merged token (two or more words)
    Merged(MergedToken),
    /// A single atom (word or control)
    Base(Atom<String>),
}

/// A sentence represented as a sequence of supertokens.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sentence {
    /// The tokens in this sentence
    pub tokens: Vec<SuperToken>,
    /// Whether the first letter was capitalized (and we lowercased it)
    pub capitalize_first_letter: bool,
}

impl Sentence {
    /// Create a new sentence from atoms (before any merging)
    pub fn from_atoms(atoms: Vec<Atom<String>>, capitalize_first_letter: bool) -> Self {
        Self {
            tokens: atoms.into_iter().map(SuperToken::Base).collect(),
            capitalize_first_letter,
        }
    }
}

// --- Plain char for character-level morphology ---

impl UnigramToken for char {
    fn can_be_sequence_boundary(&self) -> bool {
        true
    }
    fn is_content(&self) -> bool {
        true
    }
    fn is_excluded_from_sequences(&self) -> bool {
        false
    }
}

// --- Positioned character for character-level morphology ---

/// Position of a character within a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CharPosition {
    Start,
    Middle,
    End,
}

/// A unicode character tagged with its position in the word.
/// This lets the model distinguish e.g. prefix `re-` from suffix `-re`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PChar {
    pub pos: CharPosition,
    pub ch: char,
}

impl PChar {
    /// Convert a word into a sequence of positioned characters.
    pub fn from_word(word: &str) -> Vec<PChar> {
        let chars: Vec<char> = word.chars().collect();
        match chars.len() {
            0 => vec![],
            1 => vec![PChar {
                pos: CharPosition::Start,
                ch: chars[0],
            }],
            _ => {
                let last = chars.len() - 1;
                chars
                    .iter()
                    .enumerate()
                    .map(|(i, &ch)| PChar {
                        pos: if i == 0 {
                            CharPosition::Start
                        } else if i == last {
                            CharPosition::End
                        } else {
                            CharPosition::Middle
                        },
                        ch,
                    })
                    .collect()
            }
        }
    }

    /// Render a sequence of PChars back to a string (just the characters).
    pub fn seq_to_string(seq: &[PChar]) -> String {
        seq.iter().map(|pc| pc.ch).collect()
    }
}

impl std::fmt::Display for PChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let marker = match self.pos {
            CharPosition::Start => "^",
            CharPosition::Middle => "",
            CharPosition::End => "$",
        };
        write!(f, "{}{marker}", self.ch)
    }
}

impl UnigramToken for PChar {
    fn can_be_sequence_boundary(&self) -> bool {
        true
    }

    fn is_content(&self) -> bool {
        true
    }

    fn is_excluded_from_sequences(&self) -> bool {
        false
    }
}

// --- UnigramToken implementation for Atom<S> ---

impl<S: Clone + Eq + std::hash::Hash + std::fmt::Debug> UnigramToken for Atom<S> {
    fn can_be_sequence_boundary(&self) -> bool {
        matches!(self, Atom::Tok(_))
    }

    fn is_content(&self) -> bool {
        matches!(self, Atom::Tok(word) if matches!(&word.word_type, WordType::Heteronym(_)))
    }

    fn is_excluded_from_sequences(&self) -> bool {
        matches!(
            self,
            Atom::Tok(word) if matches!(&word.word_type, WordType::Other(o) if o.other_tag == OtherWordType::Propn)
        )
    }
}

/// Convert a `Seq<Atom<String>>` to a `SuperToken`.
/// Returns `None` for empty sequences.
pub fn seq_to_supertoken(seq: &Seq<Atom<String>>) -> Option<SuperToken> {
    match seq.len() {
        0 => None,
        1 => Some(SuperToken::Base(seq.0[0].clone())),
        _ => {
            // Find first and last word tokens
            let first_word_idx = seq
                .0
                .iter()
                .position(|a| matches!(a, Atom::<String>::Tok(_)))?;
            let last_word_idx = seq
                .0
                .iter()
                .rposition(|a| matches!(a, Atom::<String>::Tok(_)))?;

            if first_word_idx == last_word_idx {
                // Only one word, return as base
                return Some(SuperToken::Base(seq.0[first_word_idx].clone()));
            }

            let first = match &seq.0[first_word_idx] {
                Atom::<String>::Tok(w) => w.clone(),
                _ => unreachable!(),
            };
            let last = match &seq.0[last_word_idx] {
                Atom::<String>::Tok(w) => w.clone(),
                _ => unreachable!(),
            };

            // Middle is everything between first and last word
            let middle: Vec<Atom<String>> = seq.0[first_word_idx + 1..last_word_idx].to_vec();

            Some(SuperToken::Merged(MergedToken {
                first,
                middle,
                last,
            }))
        }
    }
}

/// Create a Sentence from literals, performing the forward conversion
pub fn sentence_from_literals(literals: &[Literal<String>], language: Language) -> Sentence {
    let (atoms, capitalize_first) = literals_to_atoms(literals, language);
    Sentence::from_atoms(atoms, capitalize_first)
}

/// A single whitespace prediction error for diagnostics
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhitespacePredictionError {
    /// The left token text
    pub left_text: String,
    /// The right token text (if any)
    pub right_text: Option<String>,
    /// What was predicted
    pub predicted: Whitespace,
    /// What the actual whitespace was
    pub actual: Whitespace,
    /// Full sentence context
    pub sentence: String,
}

/// Analyze whitespace prediction accuracy for a set of literals.
/// Returns a list of prediction errors with context.
pub fn analyze_whitespace_predictions(
    literals: &[Literal<String>],
    sentence_text: &str,
    language: Language,
) -> Vec<WhitespacePredictionError> {
    let mut errors = Vec::new();

    for (i, literal) in literals.iter().enumerate() {
        let next_word = literals.get(i + 1).map(|l| &l.word);
        let predicted = predict_whitespace(&literal.word, next_word, language);
        let actual: Whitespace = literal.whitespace.parse().unwrap();

        if predicted != actual {
            errors.push(WhitespacePredictionError {
                left_text: literal.word.text.clone(),
                right_text: next_word.map(|w| w.text.clone()),
                predicted,
                actual,
                sentence: sentence_text.to_string(),
            });
        }
    }

    errors
}

/// Summary of whitespace prediction errors grouped by pattern
#[derive(Debug, Clone, Default)]
pub struct WhitespacePredictionSummary {
    /// Errors grouped by (left_text, right_text, predicted, actual)
    /// Using BTreeMap for stable ordering in output
    pub by_pattern:
        std::collections::BTreeMap<(String, Option<String>, Whitespace, Whitespace), Vec<String>>,
    /// Total errors
    pub total_errors: usize,
    /// Total predictions made
    pub total_predictions: usize,
}

impl WhitespacePredictionSummary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add errors from analyzing a sentence
    pub fn add_sentence(
        &mut self,
        literals: &[Literal<String>],
        sentence_text: &str,
        language: Language,
    ) {
        self.total_predictions += literals.len();

        let errors = analyze_whitespace_predictions(literals, sentence_text, language);
        self.total_errors += errors.len();

        for error in errors {
            let key = (
                error.left_text,
                error.right_text,
                error.predicted,
                error.actual,
            );
            self.by_pattern.entry(key).or_default().push(error.sentence);
        }
    }

    /// Generate a diagnostic report as a string
    pub fn generate_report(&self) -> String {
        let mut report = String::new();

        let accuracy = if self.total_predictions > 0 {
            100.0 * (1.0 - self.total_errors as f64 / self.total_predictions as f64)
        } else {
            100.0
        };

        report.push_str(&format!(
            "# Whitespace Prediction Diagnostics\n\n\
             Total predictions: {}\n\
             Total errors: {}\n\
             Accuracy: {:.2}%\n\n\
             ## Error Patterns (sorted by frequency)\n\n",
            self.total_predictions, self.total_errors, accuracy
        ));

        // Sort patterns by frequency (most common first)
        let mut patterns: Vec<_> = self.by_pattern.iter().collect();
        patterns.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for ((left, right, predicted, actual), sentences) in patterns {
            let right_str = right.as_deref().unwrap_or("<END>");
            report.push_str(&format!(
                "### \"{}\" + \"{}\" ({} occurrences)\n",
                left,
                right_str,
                sentences.len()
            ));
            report.push_str(&format!(
                "- Predicted: {predicted:?}\n- Actual: {actual:?}\n"
            ));
            report.push_str("- Examples:\n");
            for sentence in sentences.iter().take(3) {
                report.push_str(&format!("  - {sentence}\n"));
            }
            report.push('\n');
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::{
        Heteronym, OtherWord, OtherWordType, PartOfSpeech, WordType, atoms_to_literals,
        literals_to_atoms, literals_to_text, predict_whitespace,
    };

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

    fn make_punct(text: &str) -> Word<String> {
        Word {
            text: text.to_string(),
            word_type: WordType::Other(OtherWord {
                other_tag: OtherWordType::Punct,
            }),
        }
    }

    fn make_literal(text: &str, whitespace: &str) -> Literal<String> {
        Literal {
            word: make_word(text),
            whitespace: whitespace.to_string(),
        }
    }

    fn make_punct_literal(text: &str, whitespace: &str) -> Literal<String> {
        Literal {
            word: make_punct(text),
            whitespace: whitespace.to_string(),
        }
    }

    #[test]
    fn test_whitespace_from_str() {
        assert_eq!("".parse::<Whitespace>().unwrap(), Whitespace::None);
        assert_eq!(" ".parse::<Whitespace>().unwrap(), Whitespace::Space);
        assert_eq!(
            "\u{202F}".parse::<Whitespace>().unwrap(),
            Whitespace::NarrowNbsp
        );
        assert_eq!("\u{00A0}".parse::<Whitespace>().unwrap(), Whitespace::Nbsp);
    }

    #[test]
    fn test_predict_whitespace_apostrophe() {
        let l = make_word("l'");
        let amour = make_word("amour");
        assert_eq!(
            predict_whitespace(&l, Some(&amour), Language::French),
            Whitespace::None
        );
    }

    #[test]
    fn test_predict_whitespace_french_punct() {
        let word = make_word("bonjour");
        let exclaim = make_punct("!");
        assert_eq!(
            predict_whitespace(&word, Some(&exclaim), Language::French),
            Whitespace::NarrowNbsp
        );
    }

    #[test]
    fn test_predict_whitespace_english_punct() {
        let word = make_word("hello");
        let exclaim = make_punct("!");
        // English should NOT have narrow nbsp before punctuation
        assert_eq!(
            predict_whitespace(&word, Some(&exclaim), Language::English),
            Whitespace::None
        );
    }

    #[test]
    fn test_predict_whitespace_period() {
        let word = make_word("fin");
        let period = make_punct(".");
        assert_eq!(
            predict_whitespace(&word, Some(&period), Language::French),
            Whitespace::None
        );
    }

    #[test]
    fn test_predict_whitespace_default() {
        let hello = make_word("hello");
        let world = make_word("world");
        assert_eq!(
            predict_whitespace(&hello, Some(&world), Language::English),
            Whitespace::Space
        );
    }

    #[test]
    fn test_round_trip_simple() {
        let literals = vec![make_literal("Hello", " "), make_literal("world", "")];

        let (atoms, capitalize) = literals_to_atoms(&literals, Language::English);
        let mut reconstructed = atoms_to_literals(&atoms, Language::English);
        // Manually apply capitalization for round-trip
        if capitalize && !reconstructed.is_empty() {
            reconstructed[0].word.text =
                language_utils::capitalize_first_letter(&reconstructed[0].word.text);
        }
        let original_text = literals_to_text(&literals);
        let reconstructed_text = literals_to_text(&reconstructed);

        assert_eq!(original_text, reconstructed_text);
    }

    #[test]
    fn test_round_trip_french_apostrophe() {
        let literals = vec![
            make_literal("L'", ""),
            make_literal("amour", " "),
            make_literal("est", " "),
            make_literal("beau", ""),
        ];

        let (atoms, capitalize) = literals_to_atoms(&literals, Language::French);
        let mut reconstructed = atoms_to_literals(&atoms, Language::French);
        if capitalize && !reconstructed.is_empty() {
            reconstructed[0].word.text =
                language_utils::capitalize_first_letter(&reconstructed[0].word.text);
        }
        let original_text = literals_to_text(&literals);
        let reconstructed_text = literals_to_text(&reconstructed);

        assert_eq!(original_text, reconstructed_text);
    }

    #[test]
    fn test_round_trip_french_punct() {
        let literals = vec![
            make_literal("Bonjour", "\u{202F}"),
            make_punct_literal("!", " "),
            make_literal("Comment", " "),
            make_literal("ça", " "),
            make_literal("va", "\u{202F}"),
            make_punct_literal("?", ""),
        ];

        let (atoms, capitalize) = literals_to_atoms(&literals, Language::French);
        let mut reconstructed = atoms_to_literals(&atoms, Language::French);
        if capitalize && !reconstructed.is_empty() {
            reconstructed[0].word.text =
                language_utils::capitalize_first_letter(&reconstructed[0].word.text);
        }
        let original_text = literals_to_text(&literals);
        let reconstructed_text = literals_to_text(&reconstructed);

        assert_eq!(original_text, reconstructed_text);
    }
}
