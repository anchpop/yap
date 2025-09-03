use futures::StreamExt;
use language_utils::{
    Course, DictionaryEntryThoughts, Heteronym, PhrasebookEntryThoughts, SentenceInfo,
};
use std::{collections::BTreeMap, sync::LazyLock};
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-4o")
        .unwrap()
        .with_cache_directory("./.cache")
});

pub async fn create_phrasebook(
    course: Course,
    nlp_analyzed_sentences: &Vec<(String, SentenceInfo)>,
) -> anyhow::Result<Vec<(String, PhrasebookEntryThoughts)>> {
    let Course {
        native_language,
        target_language,
        ..
    } = course;

    let mut target_language_multi_word_terms: BTreeMap<String, String> = BTreeMap::new();
    for (sentence, analysis) in nlp_analyzed_sentences {
        for multiword_term in analysis
            .multiword_terms
            .high_confidence
            .iter()
            .chain(analysis.multiword_terms.low_confidence.iter())
        {
            target_language_multi_word_terms.insert(multiword_term.clone(), sentence.clone());
        }
    }

    let count = target_language_multi_word_terms.len();

    let phrasebook = futures::stream::iter(&target_language_multi_word_terms).enumerate().map(async |(i, (multiword_term, sentence))| {
        let response: Result<PhrasebookEntryThoughts, _> = CHAT_CLIENT.chat_with_system_prompt(
            format!(r#"The input is a {target_language} multi-word term. Generate a phrasebook entry for it, to be used in an app for beginner  {target_language} learners (whose native language is {native_language}). First, think about the word and its meaning, and what is likely to be relevant to a beginner learner. Your thoughts will not be shown to the user. Then, write the word, then provide the meaning in a concise way. (Skip any preamble like "the {target_language} term [term] is often used to indicate that...", or "a question phrase equivalent to..." and just get straight to the meaning.) Then, provide additional context for how the term is used in the "additional_notes" field. Finally, provide an example of the term's usage in a natural sentence.

Example:
Input: multiword term: `ce que`
Output: {{
    "thoughts":"'Ce que' is a common French phrase often used to introduce indirect questions or relative clauses.",
    "target_language_multi_word_term":"ce que",
    "meaning":"'what' or 'that which'.", // this field should be super concise
    "additional_notes": "Refers to something previously mentioned or understood from context.",
    "target_language_example":"Dis-moi ce que tu veux.",
    "native_language_example":"Tell me what you want."
}}
            "#),
            format!("multiword term: `{multiword_term}`"),
        ).await.inspect_err(|e| {
            println!("error: {e:#?}");
        });

        if i % 200 == 0 {
            println!("{i} / {count}");
            println!("multiword_term: {multiword_term:?}");
            println!("example sentence: {sentence}");
            println!("{response:#?}");
        }
        response
    })
    .buffered(50)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .zip(target_language_multi_word_terms)
    .filter_map(|(response, (multi_word_term, _))| {
        response.ok().map(|entry| (multi_word_term, entry))
    })
    .collect::<Vec<_>>();

    Ok(phrasebook)
}

pub async fn create_dictionary(
    course: Course,
    nlp_analyzed_sentences: &Vec<(String, SentenceInfo)>,
) -> anyhow::Result<Vec<(Heteronym<String>, DictionaryEntryThoughts)>> {
    let Course {
        native_language,
        target_language,
    } = course;
    // Process sentences to get unique words and track occurrences
    let mut target_language_heteronyms = BTreeMap::new();
    for (sentence, analysis) in nlp_analyzed_sentences {
        for literal in &analysis.words {
            if let Some(heteronym) = &literal.heteronym {
                target_language_heteronyms.insert(heteronym.clone(), sentence.clone());
            }
        }
    }

    let count = target_language_heteronyms.len();

    let dictionary = futures::stream::iter(&target_language_heteronyms).enumerate().map(async |(i, (heteronym, sentence))| {
        if heteronym.word == "t" && heteronym.lemma == "tu" {
            panic!("heteronym: {heteronym:?} | sentence: {sentence}");
        }
        let response: Result<DictionaryEntryThoughts, _> = CHAT_CLIENT.chat_with_system_prompt(
            format!(r#"The input is a {target_language} word, along with its morphological information. Generate a dictionary entry for it, to be used in an app for beginner {target_language} learners (whose native language is {native_language}). (First, think about the word and its meaning, and what is likely to be relevant to a beginner learner.) First, write the word, then provide a list of one or more definitions. Each definition should be a JSON object with the following fields:

- "native" (string): The {native_language} translation(s) of the word. If a word has multiple very similar meanings (e.g. “this” and “that”), include them in the same string separated by commas. (If it's a verb, you don't have to include the infinitive form or information about conjugation - that will be displayed separately in the app.)
- "note" (string, optional): Use only for extra info about usage that is *not already implied* by the other fields. (For example, you can note that "tu" is informal, or that "on" often means “we” in speech.)  
- "example_sentence_target_language" (string): A natural example sentence using the word in {target_language}. (Be sure that the word's usage in the example sentence has the same morphology as is provided.)
- "example_sentence_native_language" (string): A natural {native_language} translation of the example sentence.  

You may return multiple definitions **only if the word has truly different meanings**. For example:
- ✅ `avocat` can mean “lawyer” or “avocado” — include both definitions.
- ✅ `fait` can mean “fact” (noun) or “done” (past participle of a verb) — include only the definition that makes sense given the morphological information provided.

However:
- ❌ Do NOT include rare or obscure meanings that are likely to confuse beginners.
- ❌ Do NOT include secondary meanings when one is overwhelmingly more common.

Each definition must correspond to exactly the word that is given. Do not define related forms or alternate spellings. If the word is ambiguous between forms (e.g. "avocat"), return all common meanings, but **do not speculate**.

Output the result as a JSON object containing an array of one or more definition objects."#),
            format!("word: `{word}`\nlemma: `{lemma}`,\npos: {pos}", word=heteronym.word, lemma=heteronym.lemma, pos=heteronym.pos),
        ).await.inspect_err(|e| {
            println!("error: {e:#?}");
        });
        if i % 200 == 0 {
            println!("{i} / {count}");
            println!("Heteronym: {heteronym:?}");
            println!("example sentence: {sentence}");
            println!("{response:#?}");
        }
        response
    })
    .buffered(50)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .zip(target_language_heteronyms)
    .filter_map(|(response, (heteronym, _))| {
        response.ok().map(|entry| (heteronym.clone(), entry))
    })
    .collect::<Vec<_>>();

    Ok(dictionary)
}
