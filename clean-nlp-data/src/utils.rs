use language_utils::Language;

use crate::classify::SimplifiedTokenPrime;

#[derive(Debug)]
pub enum ValidationResult {
    /// The response matches the original text exactly
    Valid,
    /// The response had a single-space mismatch that was auto-fixed
    AutoFixed,
    /// The response has mismatches that cannot be auto-fixed
    Invalid {
        original: String,
        reconstructed: String,
    },
}

/// Validate that an LLM correction response matches the original text.
/// If there's a single-space difference, automatically fix it.
pub fn validate_and_fix_whitespace(
    original: &str,
    corrected_tokens: &mut [SimplifiedTokenPrime],
    language: Language,
) -> ValidationResult {
    // remove `se ` and `s'` prefix from french lemmas if present
    if language == Language::French {
        corrected_tokens.iter_mut().for_each(|token| {
            if let Some(word) = token.lemma.strip_prefix("se ") {
                token.lemma = word.to_string();
            } else if let Some(word) = token.lemma.strip_prefix("s'") {
                token.lemma = word.to_string();
            }
        });
    }

    let reconstructed: String = corrected_tokens
        .iter()
        .map(|token| format!("{}{}", token.text, token.whitespace))
        .collect();

    if reconstructed == original {
        return ValidationResult::Valid;
    }

    // Normalize whitespace: replace all whitespace chars with regular space
    let normalize_whitespace = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_whitespace() { ' ' } else { c })
            .collect()
    };

    let orig_normalized = normalize_whitespace(original);
    let recon_normalized = normalize_whitespace(&reconstructed);

    // Check if the only difference is whitespace character types (e.g., nbsp vs space)
    if orig_normalized == recon_normalized {
        // Fix whitespace characters to match the original
        let orig_chars: Vec<char> = original.chars().collect();

        // Build a mapping of positions where whitespace differs
        let mut pos = 0;
        for token in corrected_tokens.iter_mut() {
            // Update whitespace characters to match original
            let whitespace_start = pos + token.text.chars().count();
            let mut new_whitespace = String::new();

            // Iterate over each CHARACTER in the whitespace, not each byte
            let whitespace_char_count = token.whitespace.chars().count();
            for i in 0..whitespace_char_count {
                let char_pos = whitespace_start + i;
                if char_pos < orig_chars.len() && orig_chars[char_pos].is_whitespace() {
                    new_whitespace.push(orig_chars[char_pos]);
                } else {
                    new_whitespace.push(token.whitespace.chars().nth(i).unwrap_or(' '));
                }
            }

            token.whitespace = new_whitespace;
            pos = whitespace_start + whitespace_char_count;
        }

        return ValidationResult::AutoFixed;
    }

    // Check if the difference is exactly one missing whitespace character
    let orig_no_spaces: String = original.chars().filter(|c| !c.is_whitespace()).collect();
    let recon_no_spaces: String = reconstructed
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    // Use character count instead of byte length (important for UTF-8)
    if orig_no_spaces == recon_no_spaces
        && original.chars().count() == reconstructed.chars().count() + 1
    {
        // Find where the whitespace is missing and what character it is
        let orig_chars: Vec<char> = original.chars().collect();
        let recon_chars: Vec<char> = reconstructed.chars().collect();

        let mut missing_space_pos = 0;
        let mut missing_space_char = ' ';
        for i in 0..orig_chars.len() {
            if i >= recon_chars.len() || orig_chars[i] != recon_chars[i] {
                missing_space_pos = i;
                missing_space_char = orig_chars[i];
                break;
            }
        }

        // Find which token this position falls into and add the whitespace
        let mut pos = 0;
        for token in corrected_tokens.iter_mut() {
            let token_end = pos + token.text.len();

            if missing_space_pos >= pos && missing_space_pos <= token_end {
                // Whitespace should be added to this token's whitespace
                token.whitespace.push(missing_space_char);
                return ValidationResult::AutoFixed;
            }

            pos = token_end + token.whitespace.len();
        }

        // Couldn't find where to add the whitespace
        ValidationResult::Invalid {
            original: original.to_string(),
            reconstructed,
        }
    } else {
        ValidationResult::Invalid {
            original: original.to_string(),
            reconstructed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::PartOfSpeechTag;

    fn make_token(text: &str, whitespace: &str) -> SimplifiedTokenPrime {
        SimplifiedTokenPrime {
            text: text.to_string(),
            whitespace: whitespace.to_string(),
            pos: PartOfSpeechTag::Noun,
            lemma: text.to_string(),
        }
    }

    #[test]
    fn test_narrow_nbsp_replaced_with_regular_space() {
        // Original: "Hello" + narrow non-breaking space + "world"
        let original = "Hello\u{202F}world";

        // LLM output: "Hello" + regular space + "world"
        let mut tokens = vec![make_token("Hello", " "), make_token("world", "")];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        // Should auto-fix to use narrow non-breaking space
        assert!(matches!(result, ValidationResult::AutoFixed));
        assert_eq!(tokens[0].whitespace, "\u{202F}");

        // Verify reconstruction matches original
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn test_missing_narrow_nbsp() {
        // Original: "Hello" + narrow non-breaking space + "world"
        let original = "Hello\u{202F}world";

        // LLM output: "Hello" + no space + "world"
        let mut tokens = vec![make_token("Hello", ""), make_token("world", "")];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        // Should auto-fix by adding the narrow non-breaking space
        assert!(matches!(result, ValidationResult::AutoFixed));
        assert_eq!(tokens[0].whitespace, "\u{202F}");

        // Verify reconstruction matches original
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn test_multiple_tokens_with_mixed_whitespace() {
        // Original: "A" + nbsp + "B" + regular space + "C"
        let original = "A\u{00A0}B C";

        // LLM output: "A" + regular space + "B" + regular space + "C"
        let mut tokens = vec![
            make_token("A", " "),
            make_token("B", " "),
            make_token("C", ""),
        ];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        // Should auto-fix
        assert!(matches!(result, ValidationResult::AutoFixed));
        assert_eq!(tokens[0].whitespace, "\u{00A0}");
        assert_eq!(tokens[1].whitespace, " ");

        // Verify reconstruction matches original
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn test_already_valid() {
        let original = "Hello world";

        let mut tokens = vec![make_token("Hello", " "), make_token("world", "")];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        assert!(matches!(result, ValidationResult::Valid));
    }

    #[test]
    fn test_narrow_nbsp_bug_reproduction() {
        // Original: three tokens with narrow nbsp between first two
        // "A" + narrow nbsp + "B" + regular space + "C"
        let original = "A\u{202F}B C";

        // LLM output: missing the narrow nbsp
        let mut tokens = vec![
            make_token("A", ""),
            make_token("B", " "),
            make_token("C", ""),
        ];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        // Should auto-fix
        assert!(matches!(result, ValidationResult::AutoFixed));

        // Check that we don't get narrow nbsp FOLLOWED by regular space
        assert_eq!(tokens[0].whitespace, "\u{202F}");
        assert_eq!(tokens[1].whitespace, " ");

        // Verify reconstruction matches original exactly
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(
            reconstructed,
            original,
            "Reconstructed should match original. Got: {:?}, Expected: {:?}",
            reconstructed.chars().collect::<Vec<_>>(),
            original.chars().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_real_world_bug_nbsp_becomes_narrow_nbsp_plus_space() {
        // Real bug: Original has regular nbsp (\u{a0}), LLM returns regular space
        // Original: "faire" + nbsp + "?"
        let original = "faire\u{a0}?";

        // LLM output: "faire" + regular space + "?"
        let mut tokens = vec![make_token("faire", " "), make_token("?", "")];

        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);

        // Should auto-fix
        assert!(matches!(result, ValidationResult::AutoFixed));

        // Should have nbsp, NOT narrow nbsp + space
        assert_eq!(
            tokens[0].whitespace,
            "\u{a0}",
            "Expected regular nbsp, got: {:?}",
            tokens[0].whitespace.chars().collect::<Vec<_>>()
        );

        // Verify reconstruction matches original exactly
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(
            reconstructed,
            original,
            "Reconstructed should match original. Got: {:?}, Expected: {:?}",
            reconstructed.chars().collect::<Vec<_>>(),
            original.chars().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_exact_bug_reproduction() {
        // Exact bug from the error message
        // Original sentence has narrow nbsp (\u{202f})
        let original = "C'est pour quoi faire\u{202f}?";

        // LLM returns WITH regular nbsp (\u{a0}) instead of narrow nbsp
        let mut tokens = vec![
            make_token("C'", ""),
            make_token("est", " "),
            make_token("pour", " "),
            make_token("quoi", " "),
            make_token("faire", "\u{a0}"), // Regular nbsp instead of narrow nbsp!
            make_token("?", ""),
        ];

        println!(
            "Before: {:?}",
            tokens
                .iter()
                .map(|t| format!("{:?}", t.whitespace.chars().collect::<Vec<_>>()))
                .collect::<Vec<_>>()
        );
        let result = validate_and_fix_whitespace(original, &mut tokens, Language::French);
        println!(
            "After: {:?}",
            tokens
                .iter()
                .map(|t| format!("{:?}", t.whitespace.chars().collect::<Vec<_>>()))
                .collect::<Vec<_>>()
        );

        // Should auto-fix
        assert!(matches!(result, ValidationResult::AutoFixed));

        // Should have narrow nbsp, NOT narrow nbsp + space
        assert_eq!(
            tokens[4].whitespace,
            "\u{202f}",
            "Expected narrow nbsp only, got: {:?}",
            tokens[4].whitespace.chars().collect::<Vec<_>>()
        );

        // Verify reconstruction matches original exactly
        let reconstructed: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        assert_eq!(
            reconstructed,
            original,
            "Reconstructed should match original. Got: {:?}, Expected: {:?}",
            reconstructed.chars().collect::<Vec<_>>(),
            original.chars().collect::<Vec<_>>()
        );
    }
}
