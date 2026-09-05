use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Pronunciations};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT: LazyLock<ChatClient> =
    LazyLock::new(|| crate::migrating_chat_client("gpt-5.6-luna"));

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct PronunciationResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    #[serde(rename = "2. selected_pronunciation")]
    selected_pronunciation: String,
}

/// Select the canonical pronunciation for each word. Unambiguous words are
/// handled locally; ambiguous words are resolved together through the Batch API.
pub async fn select_common_pronunciations(
    course: Course,
    words_with_pronunciations: HashMap<String, BTreeSet<String>>,
) -> anyhow::Result<Vec<(String, Pronunciations)>> {
    let target_language = course.target_language;
    let count = words_with_pronunciations.len();
    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} pronunciations ({per_sec}, ${msg}, {eta})")?
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Keep this prompt byte-for-byte compatible with the historical live-call
    // prompt so existing gpt-5.2/high cache entries remain usable.
    let system_prompt = format!(
        r#"You are analyzing {target_language} word pronunciations to select the most common one for beginner learners.

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
}}"#
    );

    let mut selected = words_with_pronunciations
        .iter()
        .filter(|(_, pronunciations)| pronunciations.len() == 1)
        .map(|(word, pronunciations)| {
            (
                word.clone(),
                Pronunciations {
                    main: pronunciations.first().expect("one pronunciation").clone(),
                    others: Vec::new(),
                },
            )
        })
        .collect::<Vec<_>>();
    let ambiguous = words_with_pronunciations
        .into_iter()
        .filter(|(_, pronunciations)| pronunciations.len() > 1)
        .collect::<Vec<_>>();

    let responses = CHAT_CLIENT
        .batch_chat_with_system_prompt_fn::<_, _, PronunciationResponse>(
            system_prompt,
            &ambiguous,
            |(word, pronunciations)| {
                format!(
                    "Word: {}\nPronunciations: {}",
                    word,
                    pronunciations
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            |batch| crate::report_batch_progress(&pb, 0, ambiguous.len(), batch),
        )
        .await?;
    for ((word, pronunciations), response) in responses {
        let main = response
            .ok()
            .map(|response| response.selected_pronunciation)
            .and_then(|candidate| {
                pronunciations
                    .iter()
                    .find(|pronunciation| {
                        pronunciation.replace(' ', "") == candidate.replace(' ', "")
                    })
                    .cloned()
            })
            .unwrap_or_else(|| {
                pronunciations
                    .first()
                    .expect("ambiguous pronunciations")
                    .clone()
            });
        let others = pronunciations
            .iter()
            .filter(|pronunciation| **pronunciation != main)
            .cloned()
            .collect();
        selected.push((word.clone(), Pronunciations { main, others }));
    }

    selected.sort_by(|(left, _), (right, _)| left.cmp(right));
    pb.set_position(count as u64);
    pb.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));
    Ok(selected)
}
