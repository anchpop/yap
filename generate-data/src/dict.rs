use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{
    Atom, Course, DictionaryDefinition, Gram, GramFrequencyEntry, Heteronym,
    PhrasebookDefinitionEntry, PhrasebookDefinitionEntryV2, SentenceGram, SentenceGrams, WordType,
};
use rustc_hash::FxHashMap;
use sentence_sampler::sample_to_target;
use std::{collections::BTreeMap, sync::LazyLock};
use tysm::chat_completions::{ChatClient, ChatMessage};

static CHAT_CLIENT_LIGHT_DICTIONARY: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.2")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

static CHAT_CLIENT_LIGHTER: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4-mini")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

static CHAT_CLIENT_HEAVY: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("high")
            .with_service_tier("flex"),
    )
});

/// Internal helper that generates dictionary definitions for a map of heteronyms with frequencies.
/// Used by both `create_dictionary` and `create_gram_dictionary`.
async fn generate_dictionary_definitions(
    course: Course,
    target_language_heteronyms: BTreeMap<Heteronym<String>, u32>,
    progress_label: &str,
) -> anyhow::Result<BTreeMap<Heteronym<String>, DictionaryDefinition>> {
    let Course {
        native_language,
        target_language,
    } = course;

    let count = target_language_heteronyms.len();

    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!("{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {progress_label} ({{per_sec}}, ${{msg}}, {{eta}})"))
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let dictionary = futures::stream::iter(target_language_heteronyms.iter()).map(async |(heteronym, &freq)| {
        let cost = CHAT_CLIENT_HEAVY.cost().unwrap_or(0.0) + CHAT_CLIENT_LIGHT_DICTIONARY.cost().unwrap_or(0.0);
        pb.set_message(format!("{cost:.2} ({},{},{})", heteronym.word, heteronym.lemma, heteronym.pos));

        let chat_client = if freq > 500 { &*CHAT_CLIENT_HEAVY } else { &*CHAT_CLIENT_LIGHT_DICTIONARY };

        let dict_response = {
            let response: Result<DictionaryDefinition, _> = chat_client.chat_with_system_prompt(
            format!(r#"The input is a {target_language} word, along with its morphological information. Generate a dictionary entry for it, to be used in an app for beginner {target_language} learners (whose native language is {native_language}). First, the JSON schema gives an opportunity to write {target_language} word, then provide a list of one or more {native_language} translations/definitions. Each definition will be a JSON object with the following fields:

- "native" (string): The {native_language} translation(s) of the word. If a word has multiple very similar meanings (e.g. "this" and "that"), include them in the same string separated by commas. (If it's a verb, you don't have to include the infinitive form or information about conjugation - that will be displayed separately in the app.) The "native" field should just have the closest word (or short phrase) to the target language word. Don't capitalize the first letter unless it makes sense (e.g. english proper nouns, german nouns, etc).
- "note" (string, optional): Use only for extra info about usage that is *not already implied* by the other fields. (For example, you can note that "tu" is informal.) The "note" is a perfect place for this, so there's no need to include it in the "native" field. 
- "example_sentence_target_language" (string): A natural example sentence using the word in {target_language}. (Be sure that the word's usage in the example sentence has the same morphology as is provided.)
- "example_sentence_native_language" (string): A natural {native_language} translation of the example sentence.
- "cognate": (bool) whether the {target_language} word is a cognate in {native_language}. For our purposes, a word is a cognate to a definition if it looks similar to the {native_language} word. So "avocat" is a cognate for "avocado", but not "lawyer".
- "false_cognate": (bool) whether the {target_language} word is a false cognate / false friend in {native_language}. For our purposes, a word is a false cognate to a definition if it looks similar to a different {native_language} word and might be easily confused, a classic example being the french word "actuellement" not corresponding to the english word "actually".

You may return multiple definitions **only if the word has truly different meanings**. For example:
- ✅ in French, `avocat` can mean "lawyer" or "avocado" — include both definitions.
- ✅ in French, `fait` can mean "fact" (noun) or "done" (past participle of a verb) — include only the definition that makes sense given the morphological information provided.

However:
- ❌ Do NOT include rare or obscure meanings that are likely to confuse beginners.
- ❌ Do NOT include secondary meanings when one is overwhelmingly more common.

Each definition must correspond to exactly the word that is given. Do not define related forms or alternate spellings. If the word is ambiguous between forms (e.g. "avocat"), return all common meanings, but **do not speculate**.

Just like how the definition needs to respect the provided morphology, the example sentences should as well. For example, if the french word "est" were provided as an auxiliary, a good translation into english might be "has" and the example sentence should use "est" as an auxiliary (such that the english translation contains "has"). 

One last thing. The input may be conjugated. For example, it may be the italian word "è". Using an english translation for the sake of example, it should simply be "is". Not "he is". The UI layer will take care of ensuring the user sees the conjugation in the correct context. 

Do not add pronunciation, IPA, part of speech, gender, or conjugation info unless it's in the "note" field and truly necessary.

Output the result as a JSON object containing an array of one or more definition objects. Of course, their native language is {native_language}, so you should write the notes in {native_language}."#),
                format!("word: `{word}`\nlemma: `{lemma}`,\npos: {pos}", word=heteronym.word, lemma=heteronym.lemma, pos=heteronym.pos),
            ).await.inspect_err(|e| {
                println!("error: {e:#?}");
            });

            // If any example sentence doesn't contain the word, retry with feedback


            if let Ok(ref resp) = response {
                let bad_examples: Vec<_> = resp.definitions.iter()
                    .filter(|d| !d.example_sentence_target_language.to_lowercase().contains(&heteronym.word.to_lowercase()))
                    .map(|d| d.example_sentence_target_language.as_str())
                    .collect();
                if !bad_examples.is_empty() {
                    let previous_json = serde_json::to_string(resp).unwrap_or_default();
                    chat_client.chat_with_messages(vec![
                        ChatMessage::system(format!("You are a {target_language} dictionary entry generator for {native_language} speakers.")),
                        ChatMessage::user(format!(
                            "I asked you to generate a dictionary entry for the {target_language} word `{word}`, and you gave me this response:\n\n{previous_json}\n\nHowever, some of the example sentences don't contain the exact word `{word}`. The following sentences are missing it: {bad}\n\nPlease regenerate the entire response with the same format, making sure every example_sentence_target_language contains the exact word `{word}`.",
                            word = heteronym.word,
                            bad = bad_examples.join("; "),
                        )),
                    ]).await.inspect_err(|e| {
                        println!("retry error: {e:#?}");
                    })
                } else {
                    response
                }
            } else {
                response
            }
        };

        pb.inc(1);

        (heteronym, dict_response)
    })
    .buffer_unordered(15)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .filter_map(|(heteronym, dict_response)| {
        dict_response.ok().map(|entry| (heteronym.clone(), entry))
    })
    .collect::<BTreeMap<Heteronym<String>, DictionaryDefinition>>();

    pb.finish_with_message(format!(
        "{:.2}",
        CHAT_CLIENT_HEAVY.cost().unwrap_or(0.0)
            + CHAT_CLIENT_LIGHT_DICTIONARY.cost().unwrap_or(0.0)
    ));

    Ok(dictionary)
}

/// Extracts unique heteronyms from single-atom grams in the gram vocabulary.
///
/// This function filters gram_frequencies for grams that consist of a single atom
/// containing a heteronym, deduplicates them (taking the maximum frequency for
/// duplicates), and returns them in a format suitable for dictionary generation.
///
/// For grams with multiple atoms (multiword terms), further processing is needed
/// separately.
pub fn extract_single_atom_heteronyms(
    gram_frequencies: &[GramFrequencyEntry<String>],
) -> BTreeMap<Heteronym<String>, u32> {
    let mut heteronym_frequencies: BTreeMap<Heteronym<String>, u32> = BTreeMap::new();

    for entry in gram_frequencies {
        let gram = &entry.gram;

        // Only process single-atom grams
        if gram.len() != 1 {
            continue;
        }

        // Extract the heteronym if this single atom is a heteronym
        if let Some(Atom::Tok(word)) = gram.first()
            && let WordType::Heteronym(heteronym) = &word.word_type
        {
            // Keep the maximum frequency for duplicated heteronyms
            heteronym_frequencies
                .entry(heteronym.clone())
                .and_modify(|freq| *freq = (*freq).max(entry.count))
                .or_insert(entry.count);
        }
    }

    heteronym_frequencies
}

/// Creates dictionary definitions for heteronyms extracted from the gram vocabulary.
///
/// This function:
/// 1. Extracts heteronyms from single-atom grams in the gram vocabulary
/// 2. Generates dictionary definitions for each unique heteronym using LLM
///
/// For multi-atom grams (multiword terms), use a separate function.
pub async fn create_gram_dictionary(
    course: Course,
    gram_frequencies: &[GramFrequencyEntry<String>],
) -> anyhow::Result<BTreeMap<Heteronym<String>, DictionaryDefinition>> {
    // Extract unique heteronyms from single-atom grams
    let target_language_heteronyms = extract_single_atom_heteronyms(gram_frequencies);

    generate_dictionary_definitions(
        course,
        target_language_heteronyms,
        "gram dictionary entries",
    )
    .await
}

/// Returns a list of multi-atom grams (grams with more than one atom).
/// These are multiword terms that need separate definition generation.
pub fn extract_multi_atom_grams(
    gram_frequencies: &[GramFrequencyEntry<String>],
) -> Vec<&Gram<String>> {
    gram_frequencies
        .iter()
        .filter(|entry| entry.gram.len() > 1)
        .map(|entry| &entry.gram)
        .collect()
}

/// Builds an index from grams to sentences containing them.
/// This allows O(1) lookup of sentences for any gram.
fn build_gram_to_sentences_index<'a>(
    encoded_sentences: &'a [(String, SentenceGrams<Gram<String>>)],
) -> FxHashMap<&'a Gram<String>, Vec<&'a str>> {
    let mut index: FxHashMap<&'a Gram<String>, Vec<&'a str>> = FxHashMap::default();

    for (sentence_text, sentence_grams) in encoded_sentences {
        for gram in &sentence_grams.grams {
            let gram_ref = match gram {
                SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
            };
            index
                .entry(gram_ref)
                .or_default()
                .push(sentence_text.as_str());
        }
    }

    index
}

/// Creates phrasebook entries for multi-atom grams (multiword terms) from the gram vocabulary.
///
/// This function:
/// 1. Extracts multi-atom grams from the gram vocabulary
/// 2. Finds example sentences containing each gram (using cached sentences when available)
/// 3. Generates phrasebook entries using LLM with example sentences in the prompt
///
/// `gram_sentences` is a persistent cache of gram -> example sentences. Missing grams
/// will have sentences selected from the corpus and added to this map.
pub async fn create_gram_phrasebook(
    course: Course,
    gram_frequencies: &[GramFrequencyEntry<String>],
    encoded_sentences: &[(String, SentenceGrams<Gram<String>>)],
    gram_sentences: &mut BTreeMap<Gram<String>, Vec<String>>,
) -> anyhow::Result<Vec<(Gram<String>, PhrasebookDefinitionEntry)>> {
    let Course {
        native_language,
        target_language,
        ..
    } = course;

    // Build index from gram -> sentences (O(n) where n = total grams across all sentences)
    let gram_to_sentences = build_gram_to_sentences_index(encoded_sentences);

    // Extract multi-atom grams with their frequencies
    let mut multi_atom_grams: BTreeMap<Gram<String>, u32> = BTreeMap::new();
    for entry in gram_frequencies {
        let gram = &entry.gram;
        if gram.len() > 1 {
            multi_atom_grams.entry(gram.clone()).or_insert(entry.count);
        }
    }

    // Populate gram_sentences for any grams not already cached
    for gram in multi_atom_grams.keys() {
        gram_sentences.entry(gram.clone()).or_insert_with(|| {
            let sentences: Vec<&str> = gram_to_sentences.get(gram).cloned().unwrap_or_default();
            let selected = sample_to_target(sentences, 5, |s: &&str| *s);
            selected.into_iter().map(|s| s.to_owned()).collect()
        });
    }

    let count = multi_atom_grams.len();

    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} gram phrasebook entries ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut phrasebook = futures::stream::iter(multi_atom_grams.iter())
        .map(|(gram, &freq)| {
            let pb = pb.clone();
            let gram_text = gram.to_display_string(target_language);
            let cost =
                CHAT_CLIENT_HEAVY.cost().unwrap_or(0.0) + CHAT_CLIENT_LIGHTER.cost().unwrap_or(0.0) + CHAT_CLIENT.cost().unwrap_or(0.0);
            pb.set_message(format!("{cost:.2} ({gram_text})"));

            let example_sentences = gram_sentences
                .get(gram)
                .cloned()
                .unwrap_or_default();

            async move {
                let examples_text = if example_sentences.is_empty() {
                    String::from("(No example sentences available)")
                } else {
                    example_sentences
                        .iter()
                        .enumerate()
                        .map(|(i, s)| format!("{}. {}", i + 1, s))
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                let chat_client = if freq > 250 {
                    &*CHAT_CLIENT_HEAVY
                } else if freq > 150 {
                    &*CHAT_CLIENT
                } else {
                    &*CHAT_CLIENT_LIGHTER
                };

                // kind of ugly, but the old system prompt is bad, but it's too expensive to regenerate all of them so i'll just do it for the most important words
                let system_prompt= format!(r#"The input is a {target_language} multi-word term along with example sentences showing its usage. Generate a phrasebook entry for it, to be used in an app for beginner {target_language} learners (whose native language is {native_language}).

Think about the word and its meaning based on how it's used in the example sentences, and what is likely to be relevant to a beginner learner. Your thoughts will not be shown to the user. Then, write the word, then provide the meaning as the closest {native_language} equivalent word or short phrase — just like a dictionary translation (e.g. "just did", "what" or "that which", "as soon as"). Skip any preamble like "the {target_language} term [term] is often used to indicate that...", or "a question phrase equivalent to..." and just give the {native_language} equivalent. And don't include any parentheticals or other notes in the "meanings" field. Any grammatical notes, parentheticals, or other notes belong in the "additional_notes" field, not the "meanings". You can always provide additional context about things like context and gender and other notes about how the term is used in the "additional_notes" field. But what belongs in the "meanings" field is just the raw textual translation / meaning.

Next, provide your own example of the term's usage in a natural sentence.

If the term is informal/slang, you can also set the "informal" field to true. For example, the english phrase "kick the bucket" is informal. If the term makes sense from the component words, you can also set the "compositional" field to true. (E.g. "Être sur son 31" makes no sense from its individual words, nor does "Altes Haus" in german. But "C'est" does make sense from its individual words) More examples: "pass away" is non-compositional (it's a phrasal verb whose meaning isn't obvious from "pass" + "away") but not informal. And "it is what it is" is informal but compositional.

"Cognate" and "false_cognate" are boolean fields that indicate whether the {target_language} word is a cognate or false cognate in {native_language}.

For our purposes, a phrase is a cognate to a definition if it looks similar to the {native_language} phrase. So "carte de crédit" is a cognate for "credit card", and "en général" is a cognate for "in general".
And a phrase is a false cognate to a definition if it looks similar to a different {native_language} phrase and might be easily confused, a classic example being the spanish phrase "en absoluto" which looks like "absolutely" but means "not at all". (Another example is French "passer un examen" which looks like "pass an exam" but actually means "take an exam" with no implication of success).

Lastly, "can_be_translated_literally" should be set to true if the phrase can be translated literally into the {native_language} phrase. For example, "carte de crédit" can be translated literally into "credit card", and "en général" can be translated literally into "in general".

More french/english examples:
"ce que" → not a cognate, but can be translated literally ("that which")
"avoir lieu" → not a cognate, cannot be translated literally ("to have place" ≠ "to take place")

Example:
Input: multiword term: `ce que`
Example sentences:
1. Dis-moi ce que tu veux.
2. Je ne sais pas ce que tu veux dire.

(Here, both sentences use `ce que` to mean `what` in English, so you can just say ["what"] for the `meanings` field.)

Output: {{
    "target_language_multi_word_term":"ce que",
    "meanings": ["what"], // this field should be super concise - ideally include 1 string, maybe 2 strings of the most common meanings, no extra notes or context. If there are multiple meanings, include them in order of frequency. You might be able to get a hint from the example sentences. 
    "additional_notes": "Refers to something previously mentioned or understood from context.",
    "target_language_example":"C'est ce que je pensais.",
    "native_language_example":"That's what I thought.",
    "informal": false,
    "compositional": true,
    "cognate": false,
    "false_cognate": false,
    "can_be_translated_literally": true
}}

Don't capitalize the first letter of the meaning unless it makes sense (e.g. english proper nouns, german nouns, etc). Do not add pronunciation, IPA, part of speech, gender, or conjugation info unless it's in the "additional_notes" field and truly necessary.

Of course, their native language is {native_language}, so you should write the meaning and additional notes in {native_language}."#);

                let response: Result<PhrasebookDefinitionEntry, _> = if freq > 100  {
                    let response: Result<PhrasebookDefinitionEntryV2, _> = chat_client
                        .chat_with_system_prompt(
                            system_prompt,
                            format!(
                                "multiword term: `{gram_text}`\n\nExample sentences:\n{examples_text}"
                            ),
                        )
                        .await
                        .inspect_err(|e| {
                            println!("error: {e:#?}");
                        });
                    response.map(|entry| PhrasebookDefinitionEntry {
                        target_language_multi_word_term: entry.target_language_multi_word_term,
                        meaning: entry.meanings.first().cloned().unwrap_or_default(),
                        additional_notes: entry.additional_notes,
                        target_language_example: entry.target_language_example,
                        native_language_example: entry.native_language_example,
                        informal: entry.informal,
                        compositional: entry.compositional,
                        cognate: entry.cognate,
                        false_cognate: entry.false_cognate,
                        can_be_translated_literally: entry.can_be_translated_literally,
                    })
                } else {
                    chat_client
                        .chat_with_system_prompt(
                            system_prompt,
                            format!(
                                "multiword term: `{gram_text}`\n\nExample sentences:\n{examples_text}"
                            ),
                        )
                        .await
                        .inspect_err(|e| {
                            println!("error: {e:#?}");
                        })
                };

                // If the example doesn't contain the term, retry with feedback
                let response = if let Ok(ref resp) = response {
                    if !resp.target_language_example.to_lowercase().contains(&gram_text.to_lowercase()) {
                        let previous_json = serde_json::to_string(resp).unwrap_or_default();
                        chat_client.chat_with_messages(vec![
                            ChatMessage::system(format!("You are a {target_language} phrasebook entry generator for {native_language} speakers.")),
                            ChatMessage::user(format!(
                                "I asked you to generate a phrasebook entry for the {target_language} multi-word term `{gram_text}`, and you gave me this response:\n\n{previous_json}\n\nHowever, your target_language_example `{example}` does not contain the exact term `{gram_text}`. Please regenerate the entire response with the same format, making sure target_language_example contains the exact term `{gram_text}`. Here are some example sentences for inspiration that use the term:\n{examples_text}",
                                example = resp.target_language_example,
                            )),
                        ]).await.inspect_err(|e| {
                            println!("retry error: {e:#?}");
                        })
                    } else {
                        response
                    }
                } else {
                    response
                };

                pb.inc(1);

                (response, gram)
            }
        })
        .buffer_unordered(15)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|(response, gram)| response.ok().map(|entry| (gram.clone(), entry)))
        .collect::<Vec<_>>();

    phrasebook.sort_by(|(a, _), (b, _)| a.cmp(b));

    pb.finish_with_message(format!(
        "{:.2}",
        CHAT_CLIENT_HEAVY.cost().unwrap_or(0.0)
            + CHAT_CLIENT_LIGHTER.cost().unwrap_or(0.0)
            + CHAT_CLIENT.cost().unwrap_or(0.0)
    ));

    Ok(phrasebook)
}
