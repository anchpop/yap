use language_utils::{Heteronym, Literal, PartOfSpeech};
use std::collections::BTreeMap;

type ExpandWordFn =
    fn(&str, &str, Option<PartOfSpeech>, bool) -> Option<(String, String, Option<PartOfSpeech>)>;

pub(crate) fn strip_punctuation(text: &str) -> &str {
    text.trim_matches(|c| matches!(c, '.' | ',' | '!' | '?' | ':' | ';' | '-'))
}

/// Normalizes Spanish words by removing punctuation and converting to lowercase
pub(crate) fn expand_spanish_word(
    text: &str,
    lemma: &str,
    pos: Option<PartOfSpeech>,
    _is_first_word: bool,
) -> Option<(String, String, Option<PartOfSpeech>)> {
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
        let lemma = lemma.strip_prefix("-").unwrap_or(lemma);
        let lemma = lemma.strip_suffix(".").unwrap_or(lemma).to_lowercase();

        if text == "lo" && pos == Some(PartOfSpeech::Pron) {
            return Some(("lo".to_string(), "lo".to_string(), Some(PartOfSpeech::Pron)));
        }

        // expand Spanish contractions
        (text, lemma, pos)
    })
}

/// Normalizes english words
pub(crate) fn expand_english_word(
    text: &str,
    lemma: &str,
    pos: Option<PartOfSpeech>,
    _is_first_word: bool,
) -> Option<(String, String, Option<PartOfSpeech>)> {
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
        let lemma = lemma.strip_prefix("-").unwrap_or(lemma);
        let lemma = lemma.strip_suffix(".").unwrap_or(lemma).to_lowercase();

        (text, lemma, pos)
    })
}

/// Normalizes german words
pub(crate) fn expand_german_word(
    text: &str,
    lemma: &str,
    pos: Option<PartOfSpeech>,
    is_first_word: bool,
) -> Option<(String, String, Option<PartOfSpeech>)> {
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

        // Special handling for "Sie"
        // If it's not at the beginning of the sentence and it's capitalized,
        // we know it's the polite pronoun form
        // The Spacey Library should really do this for us, but they don't, so we have to.
        if !is_first_word && text == "Sie" {
            return Some((
                "Sie".to_string(),
                "Sie".to_string(),
                Some(PartOfSpeech::Pron),
            ));
        }

        // If it's at the beginning of the sentence, trust the NLP system
        let polite_forms = ["Sie"];
        if polite_forms.contains(&text) && polite_forms.contains(&lemma) {
            return Some((text.to_string(), lemma.to_string(), pos));
        }
        if pos == Some(PartOfSpeech::Noun) {
            return Some((text.to_string(), lemma.to_string(), pos));
        }

        let text = text.to_lowercase();
        let lemma = lemma.strip_prefix("-").unwrap_or(lemma);
        let lemma = lemma.strip_suffix(".").unwrap_or(lemma).to_lowercase();

        (text, lemma, pos)
    })
}

/// Expands French contractions to their full forms and normalizes words
/// Note: Without morphology data, we can't determine gender for l' -> le/la
/// We default to masculine (le) when ambiguous
pub(crate) fn expand_french_word(
    text: &str,
    lemma: &str,
    pos: Option<PartOfSpeech>,
    _is_first_word: bool,
) -> Option<(String, String, Option<PartOfSpeech>)> {
    Some({
        // Handle common French abbreviations before stripping punctuation
        let text = match text {
            "M." | "m." => {
                return Some((
                    "monsieur".to_string(),
                    "monsieur".to_string(),
                    Some(PartOfSpeech::Noun),
                ));
            }
            "Mme" | "Mme." | "mme" | "mme." => {
                return Some((
                    "madame".to_string(),
                    "madame".to_string(),
                    Some(PartOfSpeech::Noun),
                ));
            }
            "Mlle" | "Mlle." | "mlle" | "mlle." => {
                return Some((
                    "mademoiselle".to_string(),
                    "mademoiselle".to_string(),
                    Some(PartOfSpeech::Noun),
                ));
            }
            _ => text,
        };

        let text = text.replace("’", "'");
        let text = strip_punctuation(&text);

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
        let lemma = lemma.strip_prefix("-").unwrap_or(lemma);
        let lemma = lemma.strip_suffix(".").unwrap_or(lemma).to_lowercase();

        if text == "lui" && pos == Some(PartOfSpeech::Pron) {
            return Some((text.to_string(), text.to_string(), pos));
        }

        match &text[..] {
            "elle" => (
                "elle".to_string(),
                "elle".to_string(),
                Some(PartOfSpeech::Pron),
            ),
            // expand contractions
            "j'" => ("je".to_string(), "je".to_string(), Some(PartOfSpeech::Pron)),
            "m'" => ("me".to_string(), "me".to_string(), Some(PartOfSpeech::Pron)),
            "t'" => ("te".to_string(), "te".to_string(), Some(PartOfSpeech::Pron)),
            "t" => ("t".to_string(), "t".to_string(), Some(PartOfSpeech::Part)),
            "s'" => {
                if pos == Some(PartOfSpeech::Sconj) {
                    (
                        "si".to_string(),
                        "si".to_string(),
                        Some(PartOfSpeech::Sconj),
                    )
                } else {
                    ("se".to_string(), "se".to_string(), Some(PartOfSpeech::Pron))
                }
            }
            "c'" => ("ce".to_string(), "ce".to_string(), None), // either DET or PRON depending on context
            "n'" => ("ne".to_string(), "ne".to_string(), Some(PartOfSpeech::Adv)),
            "l'" => {
                // Without morphology, we default to masculine (le)
                // This is a limitation when switching from spaCy to lexide
                ("le".to_string(), "le".to_string(), None) // either DET or PRON depending on context
            }
            "de" => ("de".to_string(), "de".to_string(), Some(PartOfSpeech::Adp)),
            "d'" => ("de".to_string(), "de".to_string(), Some(PartOfSpeech::Adp)),
            "qu'" => ("que".to_string(), "que".to_string(), None),
            "quelqu'" => ("quelque".to_string(), "quelque".to_string(), None),
            "jusqu'" => (
                "jusque".to_string(),
                "jusque".to_string(),
                Some(PartOfSpeech::Adp),
            ),
            "lorsqu'" => (
                "lorsque".to_string(),
                "lorsque".to_string(),
                Some(PartOfSpeech::Sconj),
            ),
            "puisqu'" => (
                "puisque".to_string(),
                "puisque".to_string(),
                Some(PartOfSpeech::Sconj),
            ),
            "quoiqu'" => (
                "quoique".to_string(),
                "quoique".to_string(),
                Some(PartOfSpeech::Sconj),
            ),
            "presqu'" => (
                "presque".to_string(),
                "presque".to_string(),
                Some(PartOfSpeech::Adv),
            ),
            _ => (text, lemma, None),
        }
    })
}

/// Expands Korean contractions to their full forms and normalizes words
pub(crate) fn expand_korean_word(
    text: &str,
    lemma: &str,
    _pos: Option<PartOfSpeech>,
    _is_first_word: bool,
) -> Option<(String, String, Option<PartOfSpeech>)> {
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
    let lemma = lemma.strip_prefix("-").unwrap_or(lemma);
    let lemma = lemma.strip_suffix(".").unwrap_or(lemma).to_lowercase();

    Some((text, lemma, None))
}

/// Convert a lexide::Token to a Literal<String>
/// This is the main entry point for token conversion from lexide
pub(crate) fn lexide_token_to_literal(
    token: &lexide::Token,
    proper_nouns: &BTreeMap<String, Heteronym<String>>,
    language: language_utils::Language,
    is_first_word: bool,
) -> Literal<String> {
    use language_utils::PartOfSpeechTag;

    // Convert lexide POS to language_utils PartOfSpeechTag
    let convert_pos_tag = |pos: lexide::pos::PartOfSpeech| -> PartOfSpeechTag {
        // Both enums have identical variants with the same serde renames,
        // so we can convert by serializing and deserializing
        let json = serde_json::to_string(&pos).unwrap();
        serde_json::from_str(&json).unwrap()
    };

    let pos_tag = convert_pos_tag(token.pos);

    // Handle space tokens specially
    if pos_tag == PartOfSpeechTag::Space {
        let whitespace = if token.text.text.is_empty() && token.whitespace.is_empty() {
            " ".to_string()
        } else if token.text.text.is_empty() {
            token.whitespace.clone()
        } else if token.whitespace.is_empty() {
            token.text.text.clone()
        } else {
            format!("{}{}", token.text.text, token.whitespace)
        };
        return Literal {
            text: "".to_string(),
            whitespace,
            word_type: language_utils::WordType::Other(language_utils::OtherWord {
                other_tag: language_utils::OtherWordType::Space,
            }),
        };
    }

    // Determine the word type based on POS tag
    let word_type = match pos_tag {
        PartOfSpeechTag::Propn => language_utils::WordType::Other(language_utils::OtherWord {
            other_tag: language_utils::OtherWordType::Propn,
        }),
        PartOfSpeechTag::Punct => language_utils::WordType::Other(language_utils::OtherWord {
            other_tag: language_utils::OtherWordType::Punct,
        }),
        PartOfSpeechTag::X => language_utils::WordType::Other(language_utils::OtherWord {
            other_tag: language_utils::OtherWordType::X,
        }),
        _ => {
            // Try to create heteronym for other word types
            match heteronym_from_lexide_token(
                &token.text.text,
                &token.lemma.lemma,
                pos_tag,
                proper_nouns,
                language,
                is_first_word,
            ) {
                Some(h) => language_utils::WordType::Heteronym(h),
                // If heteronym creation failed for a non-special POS tag, mark as unknown
                None => language_utils::WordType::Other(language_utils::OtherWord {
                    other_tag: language_utils::OtherWordType::X,
                }),
            }
        }
    };

    Literal {
        text: token.text.text.clone(),
        whitespace: token.whitespace.clone(),
        word_type,
    }
}

/// Create a Heteronym from lexide token components
fn heteronym_from_lexide_token(
    text: &str,
    lemma: &str,
    pos_tag: language_utils::PartOfSpeechTag,
    proper_nouns: &BTreeMap<String, Heteronym<String>>,
    language: language_utils::Language,
    is_first_word: bool,
) -> Option<Heteronym<String>> {
    use language_utils::{PartOfSpeech, PartOfSpeechTag};

    // Convert PartOfSpeechTag to PartOfSpeech, filtering out invalid types
    let pos = match pos_tag {
        PartOfSpeechTag::Adj => PartOfSpeech::Adj,
        PartOfSpeechTag::Adp => PartOfSpeech::Adp,
        PartOfSpeechTag::Adv => PartOfSpeech::Adv,
        PartOfSpeechTag::Aux => PartOfSpeech::Aux,
        PartOfSpeechTag::Cconj => PartOfSpeech::Cconj,
        PartOfSpeechTag::Det => PartOfSpeech::Det,
        PartOfSpeechTag::Intj => PartOfSpeech::Intj,
        PartOfSpeechTag::Noun => PartOfSpeech::Noun,
        PartOfSpeechTag::Num => PartOfSpeech::Num,
        PartOfSpeechTag::Part => PartOfSpeech::Part,
        PartOfSpeechTag::Pron => PartOfSpeech::Pron,
        PartOfSpeechTag::Sconj => PartOfSpeech::Sconj,
        PartOfSpeechTag::Sym => PartOfSpeech::Sym,
        PartOfSpeechTag::Verb => PartOfSpeech::Verb,
        // Filter out proper nouns, punctuation, spaces, and unknown
        PartOfSpeechTag::Propn
        | PartOfSpeechTag::Punct
        | PartOfSpeechTag::Space
        | PartOfSpeechTag::X => {
            return None;
        }
    };

    let expand_word: ExpandWordFn = match language {
        language_utils::Language::French => expand_french_word,
        language_utils::Language::Spanish => expand_spanish_word,
        language_utils::Language::English => expand_english_word,
        language_utils::Language::Korean => expand_korean_word,
        language_utils::Language::German => expand_german_word,
        // For unsupported languages, use a simple normalizer
        language_utils::Language::Italian
        | language_utils::Language::Portuguese
        | language_utils::Language::Russian
        | language_utils::Language::Chinese
        | language_utils::Language::Japanese => {
            // Simple normalization for unsupported languages
            let text = strip_punctuation(text);
            if text.is_empty() {
                return None;
            }
            let text = text.to_lowercase();
            let lemma = lemma.to_lowercase();
            return Some(Heteronym {
                word: text,
                lemma,
                pos,
            });
        }
    };

    let heteronym = if let Some(heteronym) = proper_nouns.get(&text.to_lowercase()) {
        let (word, lemma, expanded_pos) =
            expand_word(&heteronym.word, &heteronym.lemma, None, is_first_word)?;
        let pos = expanded_pos.unwrap_or(heteronym.pos);
        Heteronym { word, lemma, pos }
    } else {
        let (word, lemma, expanded_pos) = expand_word(text, lemma, Some(pos), is_first_word)?;
        let pos = expanded_pos.unwrap_or(pos);
        Heteronym { word, lemma, pos }
    };

    // Punctuation and proper nouns are already filtered out during PartOfSpeechTag -> PartOfSpeech conversion

    Some(heteronym)
}
