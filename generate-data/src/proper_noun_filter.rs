use futures::stream::StreamExt;
use language_utils::{Course, Lexeme};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-4o")
        .unwrap()
        .with_cache_directory("./.cache")
});

#[derive(serde::Serialize, serde::Deserialize, Debug, schemars::JsonSchema)]
struct ProperNounClassification {
    reasoning: String,
    is_proper_noun: bool,
}

pub async fn correct_proper_nouns(
    course: Course,
    proper_nouns: BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<BTreeMap<String, Lexeme<String>>> {
    let Course {
        target_language, ..
    } = course;

    let count = proper_nouns.len();

    let filtered_lexemes = futures::stream::iter(proper_nouns.iter()).enumerate().map(async |(i, (word, usages))| {
        let assume_not_proper_noun = usages.iter().all(|usage| usage.ends_with("-le.") || usage.ends_with("-la.") || usage.ends_with("-les.")) || ["même", "alors"].contains(&word.as_str());
        let is_proper_noun = if assume_not_proper_noun {false} else {
        let response: ProperNounClassification = CHAT_CLIENT.chat_with_system_prompt(
            format!(r#"You are analyzing {target_language} words that were automatically classified as proper nouns, but some may be misclassified. Your job is to determine if each word is actually a proper noun or should be reclassified.

For each word, first provide your reasoning, then indicate whether it's truly a proper noun. If it's not a proper noun, provide the correct classification as a Lexeme. For words that could be either a proper noun or a common noun, provide the classification based on the provided usage examples.

Examples:
- "bonjour" → not a proper noun
- "BMW" → proper noun
- "bébé" → not a proper noun
- "avant-hier" → not a proper noun

Output JSON format:
{{
    "reasoning": "Explain your analysis...",
    "is_proper_noun": true/false,
}}"#),
                format!("{target_language} word: `{word}`\n\nExample usages: {:?}", usages.iter().take(3).collect::<Vec<_>>()),
            ).await.ok()?;

            if i % 50 == 0 || !response.is_proper_noun {
                println!("{i} / {count}");
                println!("Analyzing word: {word}");
                println!("Result: {response:?}");
            }
            response.is_proper_noun
        };


        if !is_proper_noun {
            #[derive(serde::Serialize, serde::Deserialize, Debug, schemars::JsonSchema)]
            struct LexemeRequest {
                reasoning: String,
                lexeme: Lexeme<String>,
            }

            let request: Result<LexemeRequest, tysm::chat_completions::ChatError> = CHAT_CLIENT.chat_with_system_prompt(
                format!(r#"You are analyzing {target_language} words or fixed expressions. Your job is to determine what they are and provide the correct classification as a Lexeme. Lexemes are either Heteronyms or Multiwords. Multiwords are for fixed expressions that are multiple words, heteronyms are for single words. First write your reasoning, then provide the correct classification as a Lexeme.
    
Examples:
- "bonjour" → {{"reasoning": "This looks like an interjection", "lexeme": {{"Heteronym": {{"word": "bonjour", "lemma": "bonjour", "pos": "INTJ"}}}}}}
- "bébé" → {{"reasoning": "This seems to be a noun", "lexeme": {{"Heteronym": {{"word": "bébé", "lemma": "bébé", "pos": "NOUN"}}}}}}
- "avant-hier" →{{"reasoning": "This is a multiword", "lexeme": {{"Multiword": "avant-hier"}}}}"#),
                format!("{target_language} word: `{}`", word.to_lowercase()),
            ).await;

            if let Ok(lexeme) = request {
                println!("Lexeme for {word}: {:?}", lexeme.lexeme);

                return Some((word.to_lowercase(), lexeme.lexeme));
            }
            else {
                println!("Error: {request:?}");
            }
        }

        None
    })
    .buffered(40)
    .collect::<Vec<_>>()
    .await.into_iter().flatten().collect::<BTreeMap<_, _>>();

    Ok(filtered_lexemes)
}
