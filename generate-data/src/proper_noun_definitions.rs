use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, OtherWordType, ProperNounDefinition, SentenceInfo, WordType};
use std::{collections::BTreeMap, sync::LazyLock};
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5-mini")
        .unwrap()
        .with_cache_directory("./.new-cache")
        .with_backup_cache_directory("./.cache")
});

pub async fn generate_proper_noun_definitions(
    course: Course,
    nlp_sentences: &BTreeMap<String, SentenceInfo>,
) -> anyhow::Result<BTreeMap<String, ProperNounDefinition>> {
    let Course {
        native_language,
        target_language,
        ..
    } = course;
    // Collect proper nouns and their example sentences
    let mut proper_noun_to_sentences: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (sentence, info) in nlp_sentences {
        for literal in &info.words {
            if let WordType::Other(other) = &literal.word.word_type
                && other.other_tag == OtherWordType::Propn
            {
                proper_noun_to_sentences
                    .entry(literal.word.text.clone())
                    .or_default()
                    .push(sentence.clone());
            }
        }
    }

    let count = proper_noun_to_sentences.len();

    if count == 0 {
        return Ok(BTreeMap::new());
    }

    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} proper nouns ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let definitions = futures::stream::iter(proper_noun_to_sentences.iter())
        .map(|(proper_noun, example_sentences)| {
            let pb = pb.clone();
            async move {
                // Take up to 3 example sentences
                let examples = example_sentences
                    .iter()
                    .take(3)
                    .map(|s| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                let response: Result<ProperNounDefinition, _> = CHAT_CLIENT
                    .chat_with_system_prompt(
                        format!(
                            r#"The learner is a native speaker of {native_language} and is learning {target_language}.

You are analyzing a proper noun from {target_language} text. Your task is to:
1. Determine what type of proper noun it is (person name, place name, organization name, or other)
2. Provide a very concise translation or transliteration to {native_language}
3. Optionally provide a brief description if the learner might not be familiar with this proper noun

For the translation:
- For person names: Use the common {native_language} equivalent if one exists, otherwise keep the original
- For place names: Use the standard {native_language} name if different from {target_language}
- For organizations: Use the common {native_language} name or abbreviation if it exists
- If the proper noun is the same in both languages, just repeat it

For the description (optional, set to null if not needed):
- Include a brief explanation if the proper noun is an acronym, abbreviation, or organization that learners might not recognize
- Include context if it's a historical figure, famous place, or cultural reference that might need explanation
- Leave as null for common names, well-known places, or self-explanatory proper nouns

Be very concise - this is for a language learning app.

Examples:

Input: "Marie" (in French text, learner is English speaker)
Output: {{
    "is_person_name": true,
    "is_place_name": false,
    "is_organization_name": false,
    "is_other": false,
    "learner_native_language_translation": "Marie",
    "description": null
}}

Input: "la Tour Eiffel" (in French text, learner is English speaker)
Output: {{
    "is_person_name": false,
    "is_place_name": true,
    "is_organization_name": false,
    "is_other": false,
    "learner_native_language_translation": "the Eiffel Tower",
    "description": null
}}

Input: "DGSE" (French intelligence agency, in French text, learner is English speaker)
Output: {{
    "is_person_name": false,
    "is_place_name": false,
    "is_organization_name": true,
    "is_other": false,
    "learner_native_language_translation": "DGSE",
    "description": "France's foreign intelligence service"
}}

Output JSON format:
{{
    "is_person_name": true/false,
    "is_place_name": true/false,
    "is_organization_name": true/false,
    "is_other": true/false,
    "learner_native_language_translation": "concise translation here",
    "description": null or "brief explanation if needed"
}}"#
                        ),
                        format!(
                            "Proper noun: `{proper_noun}`\n\nExample sentences containing this proper noun:\n{examples}"
                        ),
                    )
                    .await;

                pb.set_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));
                pb.inc(1);

                (response, proper_noun.clone())
            }
        })
        .buffer_unordered(200)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|(response, proper_noun)| {
            response.ok().map(|definition| (proper_noun, definition))
        })
        .collect::<BTreeMap<_, _>>();

    pb.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

    Ok(definitions)
}
