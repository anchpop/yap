use unicode_normalization::UnicodeNormalization;

use crate::Language;

/// Remove accents/diacritics from a string by NFD-decomposing and stripping combining characters.
/// Also lowercases the result.
pub fn remove_accents_lowercase(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Normalize quotes, hyphens, case and punctuation; expand English contractions.
pub fn normalize_for_grading(text: &str, language: Language) -> String {
    let normalized_chars = text
        .chars()
        .map(|c| match c {
            // Single quote variants: ' (U+2018), ' (U+2019), ‚ (U+201A), ‛ (U+201B),
            // ′ (U+2032), ‵ (U+2035), ❛ (U+275B), ❜ (U+275C), ＇ (U+FF07),
            // ʻ (U+02BB), ʼ (U+02BC), ʽ (U+02BD), ʹ (U+02B9), `, ´ (U+00B4)
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{2035}'
            | '\u{275B}' | '\u{275C}' | '\u{FF07}' | '\u{02BB}' | '\u{02BC}' | '\u{02BD}'
            | '\u{02B9}' | '`' | '\u{00B4}' => '\'',

            // Double quote variants: " (U+201C), " (U+201D), „ (U+201E), ‟ (U+201F),
            // ″ (U+2033), ‶ (U+2036), ❝ (U+275D), ❞ (U+275E), ＂ (U+FF02)
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{2036}'
            | '\u{275D}' | '\u{275E}' | '\u{FF02}' => '"',

            // Hyphen/dash variants: ‐ (U+2010), ‑ (U+2011), ‒ (U+2012), – (U+2013),
            // — (U+2014), ― (U+2015), − (U+2212), ﹘ (U+FE58), ﹣ (U+FE63), － (U+FF0D)
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            | '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => '-',

            _ => c,
        })
        .collect::<String>();

    let mut result = normalized_chars.to_lowercase();

    if language == Language::English {
        result = expand_english_contractions(&result);
    }

    result = result
        .chars()
        .map(|c| {
            if c.is_ascii_punctuation() && c != '\'' && c != '-' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    result
}

fn expand_english_contractions(text: &str) -> String {
    let contractions = [
        ("won't", "will not"),
        ("can't", "cannot"),
        ("i'm", "i am"),
        ("you're", "you are"),
        ("we're", "we are"),
        ("they're", "they are"),
        ("it's", "it is"),
        ("that's", "that is"),
        ("what's", "what is"),
        ("where's", "where is"),
        ("who's", "who is"),
        ("there's", "there is"),
        ("here's", "here is"),
        ("he's", "he is"),
        ("she's", "she is"),
        ("i've", "i have"),
        ("you've", "you have"),
        ("we've", "we have"),
        ("they've", "they have"),
        ("i'd", "i would"),
        ("you'd", "you would"),
        ("he'd", "he would"),
        ("she'd", "she would"),
        ("we'd", "we would"),
        ("they'd", "they would"),
        ("i'll", "i will"),
        ("you'll", "you will"),
        ("he'll", "he will"),
        ("she'll", "she will"),
        ("we'll", "we will"),
        ("they'll", "they will"),
        ("wouldn't", "would not"),
        ("shouldn't", "should not"),
        ("couldn't", "could not"),
        ("don't", "do not"),
        ("doesn't", "does not"),
        ("didn't", "did not"),
        ("isn't", "is not"),
        ("aren't", "are not"),
        ("wasn't", "was not"),
        ("weren't", "were not"),
        ("hasn't", "has not"),
        ("haven't", "have not"),
        ("hadn't", "had not"),
    ];

    let mut result = text.to_string();
    for (contraction, expansion) in &contractions {
        result = result.replace(contraction, expansion);
    }
    result
}

/// Find the closest matching string from a list of candidates using Levenshtein distance
///
/// Compares the normalized forms of the strings
pub fn find_closest_match(
    input: &str,
    candidates: &[String],
    language: Language,
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let normalized_input = normalize_for_grading(input, language);

    candidates
        .iter()
        .min_by_key(|candidate| {
            levenshtein_distance(
                &normalize_for_grading(candidate, language),
                &normalized_input,
            )
        })
        .cloned()
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Normalize whitespace, then apply language-specific punctuation spacing.
pub fn cleanup_sentence(sentence: String, language: Language) -> String {
    let normalized = sentence.split_whitespace().collect::<Vec<_>>().join(" ");

    match language {
        Language::French => cleanup_french_sentence(normalized),
        _ => normalized,
    }
}

/// French ! and ? take a thin non-breaking space (U+202F).
pub fn cleanup_french_sentence(sentence: String) -> String {
    const THIN_NBSP: char = '\u{202F}';
    const NBSP: char = '\u{00A0}';
    const HIGH_PUNCTUATION: &[char] = &['!', '?'];

    let mut result = String::with_capacity(sentence.len() + 10);
    let chars: Vec<char> = sentence.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if HIGH_PUNCTUATION.contains(&ch) && i > 0 {
            let prev_char = chars[i - 1];
            if prev_char == ' ' || prev_char == NBSP {
                result.pop();
                result.push(THIN_NBSP);
            } else if prev_char != THIN_NBSP {
                result.push(THIN_NBSP);
            }
        }
        result.push(ch);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_punctuation_spacing() {
        for (input, expected) in [
            ("Bonjour !", "Bonjour\u{202F}!"),
            ("Bonjour!", "Bonjour\u{202F}!"),
            ("Bonjour\u{202F}!", "Bonjour\u{202F}!"),
            ("Bonjour\u{00A0}!", "Bonjour\u{202F}!"),
            (
                "Question ? Exclamation ! Colon : Semicolon ;",
                "Question\u{202F}? Exclamation\u{202F}! Colon : Semicolon ;",
            ),
            ("What ?! Really !", "What\u{202F}?\u{202F}! Really\u{202F}!"),
            (
                "Bonjour, comment allez-vous.",
                "Bonjour, comment allez-vous.",
            ),
            ("!Wow", "!Wow"),
        ] {
            assert_eq!(cleanup_french_sentence(input.into()), expected, "{input:?}");
        }
    }

    #[test]
    fn sentence_whitespace_and_punctuation() {
        for (language, input, expected) in [
            (Language::French, "Bonjour !", "Bonjour\u{202F}!"),
            (Language::English, "Hello!", "Hello!"),
            (Language::French, " \u{00A0}Bonjour. ", "Bonjour."),
            (
                Language::French,
                "Bonjour\u{00A0}le\u{00A0}monde !",
                "Bonjour le monde\u{202F}!",
            ),
            (Language::French, "Ça\u{202F}va ?", "Ça va\u{202F}?"),
            (
                Language::French,
                "Tu as dit \u{ab}\u{a0}bonjour\u{a0}\u{bb} ?",
                "Tu as dit \u{ab} bonjour \u{bb}\u{202F}?",
            ),
            (
                Language::French,
                "Voulez-vous coucher avec moi\u{a0}\u{202f}?",
                "Voulez-vous coucher avec moi\u{202F}?",
            ),
            (
                Language::French,
                "Est-ce que tu fumes ici\u{2009}?",
                "Est-ce que tu fumes ici\u{202F}?",
            ),
            (
                Language::French,
                "Bonjour  le   monde.",
                "Bonjour le monde.",
            ),
            (
                Language::French,
                "Vraiment\u{2009}\u{a0} ?",
                "Vraiment\u{202F}?",
            ),
        ] {
            assert_eq!(
                cleanup_sentence(input.into(), language),
                expected,
                "{language:?}: {input:?}"
            );
        }
    }

    #[test]
    fn test_normalize_for_grading_french() {
        let input = "\u{2018}Bonjour\u{2019}, c\u{2019}est bien!";
        let result = normalize_for_grading(input, Language::French);
        assert!(result.contains("bonjour"));
        assert!(result.contains("est bien"));
    }

    #[test]
    fn test_normalize_for_grading_english_contractions() {
        assert_eq!(
            normalize_for_grading("It's a test", Language::English),
            "it is a test"
        );
        assert_eq!(
            normalize_for_grading("I'm happy", Language::English),
            "i am happy"
        );
        assert_eq!(
            normalize_for_grading("won't do it", Language::English),
            "will not do it"
        );
    }

    #[test]
    fn test_normalize_for_grading_punctuation() {
        assert_eq!(
            normalize_for_grading("Hello, world!", Language::English),
            "hello world"
        );
        assert_eq!(
            normalize_for_grading("What's up?", Language::English),
            "what is up"
        );
    }
}
