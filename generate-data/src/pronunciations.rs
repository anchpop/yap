use futures::StreamExt;
use language_utils::{Course, Pronunciation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::{collections::BTreeMap, sync::LazyLock};
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("o3")
        .unwrap()
        .with_cache_directory("./.cache")
});

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct PronunciationResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    #[serde(rename = "2. selected_pronunciation")]
    selected_pronunciation: String,
}

/// Takes a map of words to their multiple pronunciations and returns the most common one for each
pub async fn select_common_pronunciations(
    course: Course,
    words_with_pronunciations: HashMap<String, BTreeSet<String>>,
) -> anyhow::Result<Vec<(String, Pronunciation)>> {
    let Course {
        target_language, ..
    } = course;

    let count = words_with_pronunciations.len();

    let pronunciations = futures::stream::iter(&words_with_pronunciations)
        .enumerate()
        .map(async |(i, (word, pronunciations))| -> Result<_, tysm::chat_completions::ChatError>{
            // Skip if there's only one pronunciation
            if pronunciations.len() == 1 {
                return Ok((word.clone(), pronunciations.first().unwrap().clone()));
            }

            let response: Result<PronunciationResponse, _> = CHAT_CLIENT.chat_with_system_prompt(
                format!(r#"You are analyzing {target_language} word pronunciations to select the most common one for beginner learners.

Given a {target_language} word and its possible IPA pronunciations, select the pronunciation that:
1. Is most commonly used in standard metropolitan {target_language}
2. Would be most appropriate for beginners to learn
3. Represents the most frequent usage in everyday speech

If there are regional or contextual variations, prioritize the standard metropolitan pronunciation unless another variant is overwhelmingly more common. Return the selected pronunciation in the way it is given (retaining spaces as they're used to separate individual IPA characters, and without [] or / / surrounding it).
e
Output format:
{{
    "1. thoughts": "Brief analysis of the pronunciation options",
    "2. selected_pronunciation": "The chosen IPA pronunciation",
}}"#),
                format!(
                    "Word: {}\nPronunciations: {}",
                    word,
                    pronunciations.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            ).await;

            if i % 100 == 0 {
                println!("{i} / {count}");
                println!("Word: {word}");
                println!("Pronunciations: {pronunciations:?}");
                println!("Response: {response:#?}");
            }

            match response {
                Ok(resp) => {
                    // Validate that the selected pronunciation is one of the options
                    if pronunciations.contains(&resp.selected_pronunciation) {
                        Ok((word.clone(), resp.selected_pronunciation))
                    } else {
                        // Fallback to first pronunciation if AI response is invalid
                        // usually, the AI just messes up by adding spaces between characters. So let's see if the AI's response is the same as any of the pronunciations without spaces
                        let matching_pronunciation = pronunciations.iter().find(|p| p.replace(" ", "") == resp.selected_pronunciation.replace(" ", ""));
                        if let Some(matching_pronunciation) = matching_pronunciation {
                            Ok((word.clone(), matching_pronunciation.clone()))
                        } else {
                            let selected = resp.selected_pronunciation;
                            let first = pronunciations.first().unwrap().clone();
                            eprintln!("Warning: AI selected invalid pronunciation for '{word}' ({selected}), using first option: {first}");
                            Ok((word.clone(), first))
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error getting pronunciation for '{word}': {e}");
                    // Fallback to first pronunciation
                    Ok((word.clone(), pronunciations.first().unwrap().clone()))
                }
            }
        })
        .buffered(500)
        .collect::<Vec<_>>()
        .await;

    Ok(pronunciations.into_iter().filter_map(|r| r.ok()).collect())
}

/// Helper function to load scraped pronunciations from a file or other source
pub async fn load_scraped_pronunciations() -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    // TODO: Implement loading from your Wikipedia scrape
    // This is a placeholder - replace with your actual loading logic

    // Example format:
    // let mut pronunciations = BTreeMap::new();
    // pronunciations.insert("�tre".to_string(), vec!["/[t�/".to_string(), "/et�/".to_string()]);
    // pronunciations.insert("les".to_string(), vec!["/le/".to_string(), "/l[/".to_string()]);

    unimplemented!("Please implement loading of scraped pronunciations")
}
