use futures::StreamExt;
use language_utils::Lexeme;
use language_utils::{Course, Language, PatternPosition, PronunciationGuideThoughts};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;
use unicode_normalization::UnicodeNormalization;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("o3")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_service_tier("flex")
});

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SoundsListResponse {
    thoughts: String,
    sounds: Vec<String>,
}

/// Generate characteristic sounds/patterns for a language
/// Returns tuples of (clean_pattern, position)
pub async fn generate_language_sounds(
    language: Language,
) -> anyhow::Result<Vec<(String, PatternPosition)>> {
    let response: SoundsListResponse = CHAT_CLIENT.chat_with_system_prompt(
        format!(r#"You are creating a comprehensive list of characteristic sounds and letter patterns for {language:?}.

Generate a list of the most important letter patterns and sounds that learners need to know. Include:
- All individual letters
- For languages that use accents (or similar), all accented letter forms
- Letter combinations (digraphs, trigraphs)
- Position-dependent pronunciations (use $ for end of word, ^ for beginning)
- Silent letters and their patterns

Use standard notation:
- $ means end of word (e.g., "ent$" for French -ent ending)
- ^ means beginning of word (e.g., "^kn" for English kn- beginning)
- Otherwise just the letters/pattern itself.
- If there are multiple common patterns, include them all separately. (e.g. "ch", "sh", "th", "ph".) Don't write "[c,s,t,p]h".
- Don't use any other syntax besides $, ^, and the literal letters as they would appear in the word.

Focus on patterns that are:
1. Common and frequently encountered
2. Different from how they might be pronounced in other languages
3. Important for correct pronunciation

Return a JSON object with:
{{
  "thoughts": "Your analysis of {language:?} pronunciation patterns",
  "sounds": ["pattern1", "pattern2", "pattern3", ...]
}}

Examples of patterns:
- French: "é", "eau", "ent$", "^h", "ch", "oi", "eu"
- Spanish: "ñ", "ll", "rr", "g", "j", "v", "z"
- Korean: "ㄱ", "ㄴ", "ㄷ", "ㄹ", "ㅂ", "ㅅ", "ㅇ", "ㅎ", "ㅏ", "ㅓ", "ㅗ", "ㅜ"
- English: "gh$", "^kn", "tion$", "ch", "sh", "th", "ph"
"#),
        format!("Generate sounds for {language:?}"),
    ).await?;

    // Process patterns to extract position information
    let processed_sounds: Vec<(String, PatternPosition)> = response
        .sounds
        .into_iter()
        .map(|sound| {
            if let Some(stripped) = sound.strip_prefix('^') {
                (stripped.to_string(), PatternPosition::Beginning)
            } else if let Some(stripped) = sound.strip_suffix('$') {
                (stripped.to_string(), PatternPosition::End)
            } else {
                (sound, PatternPosition::Anywhere)
            }
        })
        .collect();

    Ok(processed_sounds)
}

/// Generate pronunciation guides for each sound in a course
pub async fn generate_pronunciation_guides(
    course: Course,
    sounds: &[(String, PatternPosition)],
) -> anyhow::Result<Vec<(String, PronunciationGuideThoughts)>> {
    let guides = futures::stream::iter(sounds)
        .map(|(clean_pattern, position)| {
            let clean_pattern = clean_pattern.clone();
            let position = *position;
            async move {
                let position_note = match position {
                    PatternPosition::Beginning => "This pattern appears at the beginning of words.",
                    PatternPosition::End => "This pattern appears at the end of words.",
                    PatternPosition::Anywhere => "This pattern can appear anywhere in words.",
                };

                let response: Result<PronunciationGuideThoughts, _> = CHAT_CLIENT.chat_with_system_prompt(
                    format!(r#"You are creating a pronunciation guide for {native:?} speakers learning {target:?}.

Analyze the {target:?} sound/pattern: "{clean_pattern}"
{position_note}

IMPORTANT: Write the description and notes in {native:?} (the learner's native language), not in {target:?}. The words you choose for the example words should be in {target:?}, using the {target_alphabet:?} writing system.

Create a guide that includes:
1. A clear description IN {native:?} of the ways this pattern is pronounced, maybe analogizing it to words in {native:?} or explaining the difference from similar {native:?} sounds. Keep this part brief. For tricky sounds, you can include some pronunciation advice.
2. How familiar a {native:?} speaker would be with this sound
3. How difficult it is for a {native:?} speaker to pronounce
4. Example words that demonstrate this sound

For the example words, choose 1-4 {target:?} words that:
- Are VERY likely to be familiar to {native:?} speakers (brand names, food items, place names, cultural references, loan words)
- Clearly demonstrate the pattern, in all the ways it can be pronounced
- Contain the actual pattern (e.g. for the pattern "yn", "sphinx" would not be a good example as it does not contain "yn". You may want to spell out candidate words letter by letter while you're thinking, to help make sure they contain the pattern.)

For each word, specify:
- position: Where the sound appears ("Beginning", "Middle", "End", or "Multiple" if it appears more than once)
- cultural_context: Write IN {native:?} - concisely explain why they know this word. This should just be a short hint that the word means what they user probably thinks it might mean. Keep this part brief.

Return a JSON object with this structure:
{{
  "thoughts": "Brief analysis of this sound for {native:?} speakers",
  "pattern": "{clean_pattern}",
  "description": "Clear description IN {native:?} of how to pronounce this",
  "familiarity": "LikelyAlreadyKnows" | "MaybeAlreadyKnows" | "ProbablyDoesNotKnow",
  "difficulty": "Easy" | "Medium" | "Hard",
  "example_words": [
    {{
      "target": "target_word", 
      "native": "native_translation",
      "position": "Beginning" | "Middle" | "End" | "Multiple",
      "cultural_context": "why they know this IN {native:?}"
    }}
  ]
}}

Good examples for Spanish "ñ" and English speakers:
[
  {{"target": "jalapeño", "native": "jalapeño", "position": "End", "cultural_context": "Popular Mexican pepper used in many dishes"}},
  {{"target": "piña colada", "native": "piña colada", "position": "Middle", "cultural_context": "Famous tropical cocktail"}},
  {{"target": "El Niño", "native": "El Niño", "position": "End", "cultural_context": "Weather pattern you hear about in the news"}}
]

Good examples for French "ch" and English speakers:
[
  {{"target": "champagne", "native": "champagne", "position": "Beginning", "cultural_context": "Sparkling wine used for celebrations"}},
  {{"target": "chef", "native": "chef", "position": "Beginning", "cultural_context": "Same word in English - head cook"}},
  {{"target": "cliché", "native": "cliché", "position": "Middle", "cultural_context": "Same word in English - overused phrase"}}
]
"#,
                        native = course.native_language,
                        target = course.target_language,
                        target_alphabet = course.target_language.writing_system(),
                        clean_pattern = clean_pattern,
                        position_note = position_note
                    ),
                    format!("Analyze sound: {clean_pattern}"),
                ).await;

                match response {
                    Ok(mut guide) => {
                        // Override the pattern with the clean version and set the position
                        guide.pattern = clean_pattern.clone();
                        guide.position = position;
                        Some((clean_pattern.clone(), guide))
                    },
                    Err(e) => {
                        eprintln!("Error generating guide for '{clean_pattern}': {e:?}");
                        None
                    }
                }
            }
        })
        .buffered(10)
        .filter_map(|result| async { result })
        .collect::<Vec<_>>()
        .await;

    Ok(guides)
}

/// Calculate the frequency of each pronunciation pattern based on word frequency data
/// Returns a HashMap mapping each pattern to its total frequency across all words containing it
pub fn calculate_pattern_frequencies<S>(
    sounds: &[(String, PatternPosition)],
    word_frequencies: &[language_utils::FrequencyEntry<S>],
) -> HashMap<(String, PatternPosition), u32>
where
    S: AsRef<str>,
    S: rkyv::Archive + PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
    <S as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash + std::fmt::Debug,
    <Lexeme<S> as rkyv::Archive>::Archived: PartialEq + PartialOrd + Eq + Ord + Hash,
{
    let mut frequencies = HashMap::new();

    // Initialize all patterns with 0
    for (pattern, position) in sounds {
        frequencies.insert((pattern.clone(), *position), 0);
    }

    // Check if we're dealing with Korean by looking at the patterns
    let is_korean = sounds.iter().any(|(pattern, _)| {
        pattern.chars().any(|c| {
            // Check if character is in Hangul Jamo ranges (including compatibility jamo)
            ('\u{1100}'..='\u{11FF}').contains(&c) || // Hangul Jamo
            ('\u{3130}'..='\u{318F}').contains(&c) || // Hangul Compatibility Jamo
            ('\u{A960}'..='\u{A97F}').contains(&c) || // Hangul Jamo Extended-A
            ('\u{D7B0}'..='\u{D7FF}').contains(&c) || // Hangul Jamo Extended-B
            ('\u{3131}'..='\u{314E}').contains(&c) || // Hangul compatibility consonants
            ('\u{314F}'..='\u{3163}').contains(&c) // Hangul compatibility vowels
        })
    });

    // Sum up frequencies for each pattern based on word occurrences
    for freq_entry in word_frequencies {
        // Get the actual word string from the lexeme
        let word = match &freq_entry.lexeme {
            language_utils::Lexeme::Heteronym(h) => h.word.as_ref(),
            language_utils::Lexeme::Multiword(s) => s.as_ref(),
        };

        // For Korean, use NFKD (compatibility decomposition) to handle jamo
        // For other languages, just use lowercase
        let word_normalized = if is_korean {
            // NFKD performs compatibility decomposition followed by canonical composition
            // This properly handles Korean text decomposition
            word.nfkd().collect::<String>()
        } else {
            word.to_lowercase()
        };

        for (pattern, position) in sounds {
            // For Korean patterns, use position-aware jamo conversion
            // For others, lowercase
            let pattern_normalized = if is_korean {
                // Korean compatibility jamo need position-based conversion
                // since they map to different combining jamo based on syllable position
                let mut normalized = String::new();
                for ch in pattern.chars() {
                    // Check if this is a compatibility jamo (U+3131..U+318E)
                    if ('\u{3131}'..='\u{318E}').contains(&ch) {
                        // Convert based on position in syllable
                        if *position == PatternPosition::End {
                            // Convert to final jamo for End position
                            let final_jamo = match ch {
                                'ㄱ' => 'ᆨ',
                                'ㄲ' => 'ᆩ',
                                'ㄳ' => 'ᆪ',
                                'ㄴ' => 'ᆫ',
                                'ㄵ' => 'ᆬ',
                                'ㄶ' => 'ᆭ',
                                'ㄷ' => 'ᆮ',
                                'ㄹ' => 'ᆯ',
                                'ㄺ' => 'ᆰ',
                                'ㄻ' => 'ᆱ',
                                'ㄼ' => 'ᆲ',
                                'ㄽ' => 'ᆳ',
                                'ㄾ' => 'ᆴ',
                                'ㄿ' => 'ᆵ',
                                'ㅀ' => 'ᆶ',
                                'ㅁ' => 'ᆷ',
                                'ㅂ' => 'ᆸ',
                                'ㅄ' => 'ᆹ',
                                'ㅅ' => 'ᆺ',
                                'ㅆ' => 'ᆻ',
                                'ㅇ' => 'ᆼ',
                                'ㅈ' => 'ᆽ',
                                'ㅊ' => 'ᆾ',
                                'ㅋ' => 'ᆿ',
                                'ㅌ' => 'ᇀ',
                                'ㅍ' => 'ᇁ',
                                'ㅎ' => 'ᇂ',
                                // Vowels (medial jamo) - same in all positions
                                'ㅏ' => 'ᅡ',
                                'ㅐ' => 'ᅢ',
                                'ㅑ' => 'ᅣ',
                                'ㅒ' => 'ᅤ',
                                'ㅓ' => 'ᅥ',
                                'ㅔ' => 'ᅦ',
                                'ㅕ' => 'ᅧ',
                                'ㅖ' => 'ᅨ',
                                'ㅗ' => 'ᅩ',
                                'ㅘ' => 'ᅪ',
                                'ㅙ' => 'ᅫ',
                                'ㅚ' => 'ᅬ',
                                'ㅛ' => 'ᅭ',
                                'ㅜ' => 'ᅮ',
                                'ㅝ' => 'ᅯ',
                                'ㅞ' => 'ᅰ',
                                'ㅟ' => 'ᅱ',
                                'ㅠ' => 'ᅲ',
                                'ㅡ' => 'ᅳ',
                                'ㅢ' => 'ᅴ',
                                'ㅣ' => 'ᅵ',
                                _ => ch,
                            };
                            normalized.push(final_jamo);
                        } else {
                            // Convert to initial jamo for Beginning/Anywhere
                            let initial_jamo = match ch {
                                'ㄱ' => 'ᄀ',
                                'ㄲ' => 'ᄁ',
                                'ㄴ' => 'ᄂ',
                                'ㄷ' => 'ᄃ',
                                'ㄸ' => 'ᄄ',
                                'ㄹ' => 'ᄅ',
                                'ㅁ' => 'ᄆ',
                                'ㅂ' => 'ᄇ',
                                'ㅃ' => 'ᄈ',
                                'ㅅ' => 'ᄉ',
                                'ㅆ' => 'ᄊ',
                                'ㅇ' => 'ᄋ',
                                'ㅈ' => 'ᄌ',
                                'ㅉ' => 'ᄍ',
                                'ㅊ' => 'ᄎ',
                                'ㅋ' => 'ᄏ',
                                'ㅌ' => 'ᄐ',
                                'ㅍ' => 'ᄑ',
                                'ㅎ' => 'ᄒ',
                                // Vowels (medial jamo) - same in all positions
                                'ㅏ' => 'ᅡ',
                                'ㅐ' => 'ᅢ',
                                'ㅑ' => 'ᅣ',
                                'ㅒ' => 'ᅤ',
                                'ㅓ' => 'ᅥ',
                                'ㅔ' => 'ᅦ',
                                'ㅕ' => 'ᅧ',
                                'ㅖ' => 'ᅨ',
                                'ㅗ' => 'ᅩ',
                                'ㅘ' => 'ᅪ',
                                'ㅙ' => 'ᅫ',
                                'ㅚ' => 'ᅬ',
                                'ㅛ' => 'ᅭ',
                                'ㅜ' => 'ᅮ',
                                'ㅝ' => 'ᅯ',
                                'ㅞ' => 'ᅰ',
                                'ㅟ' => 'ᅱ',
                                'ㅠ' => 'ᅲ',
                                'ㅡ' => 'ᅳ',
                                'ㅢ' => 'ᅴ',
                                'ㅣ' => 'ᅵ',
                                _ => ch,
                            };
                            normalized.push(initial_jamo);
                        }
                    } else {
                        // Not a compatibility jamo, keep as-is
                        normalized.push(ch);
                    }
                }
                normalized
            } else {
                pattern.to_lowercase()
            };

            let contains_pattern = match position {
                PatternPosition::Beginning => word_normalized.starts_with(&pattern_normalized),
                PatternPosition::End => word_normalized.ends_with(&pattern_normalized),
                PatternPosition::Anywhere => word_normalized.contains(&pattern_normalized),
            };

            if contains_pattern {
                // Add the word's frequency count to the pattern's total
                let current = frequencies.get_mut(&(pattern.clone(), *position)).unwrap();
                *current += freq_entry.count;
            }
        }
    }

    frequencies
}
