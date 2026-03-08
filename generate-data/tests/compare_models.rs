//! Comparison test: generate dictionary and phrasebook entries across GPT-5.4, Claude Opus 4.6, and Gemini 3.1 Pro.
//!
//! Run with: cargo test -p generate-data --test compare_models -- --nocapture
//!
//! Required env vars:
//!   OPENAI_API_KEY     - for GPT-5.4
//!   ANTHROPIC_API_KEY  - for Claude Opus 4.6
//!   GEMINI_API_KEY     - for Gemini 3.1 Pro (from AI Studio)

use language_utils::{DictionaryDefinition, PhrasebookDefinitionEntry};
use tysm::chat_completions::ChatClient;

fn make_clients() -> Vec<(&'static str, ChatClient)> {
    let mut clients = vec![];

    if let Ok(client) = ChatClient::from_env("gpt-5.4") {
        clients.push(("GPT-5.4", client.with_cache_directory("../.cache")));
    } else {
        eprintln!("⚠ Skipping GPT-5.4 (no OPENAI_API_KEY)");
    }

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        clients.push((
            "Claude Opus 4.6",
            ChatClient::new(key, "claude-opus-4-6")
                .with_url("https://api.anthropic.com/v1/")
                .with_cache_directory("../.cache"),
        ));
    } else {
        eprintln!("⚠ Skipping Claude Opus 4.6 (no ANTHROPIC_API_KEY)");
    }

    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        clients.push((
            "Gemini 3.1 Pro",
            ChatClient::new(key, "gemini-3.1-pro-preview")
                .with_url("https://generativelanguage.googleapis.com/v1beta/openai/")
                .with_cache_directory("../.cache"),
        ));
    } else {
        eprintln!("⚠ Skipping Gemini 3.1 Pro (no GEMINI_API_KEY)");
    }

    clients
}

const TARGET_LANGUAGE: &str = "French";
const NATIVE_LANGUAGE: &str = "English";

fn dict_system_prompt() -> String {
    format!(
        r#"The input is a {TARGET_LANGUAGE} word, along with its morphological information. Generate a dictionary entry for it, to be used in an app for beginner {TARGET_LANGUAGE} learners (whose native language is {NATIVE_LANGUAGE}). First, the JSON schema gives an opportunity to write {TARGET_LANGUAGE} word, then provide a list of one or more {NATIVE_LANGUAGE} translations/definitions. Each definition will be a JSON object with the following fields:

- "native" (string): The {NATIVE_LANGUAGE} translation(s) of the word. If a word has multiple very similar meanings (e.g. "this" and "that"), include them in the same string separated by commas. (If it's a verb, you don't have to include the infinitive form or information about conjugation - that will be displayed separately in the app.) The "native" field should just have the closest word (or short phrase) to the target language word. Don't capitalize the first letter unless it makes sense (e.g. english proper nouns, german nouns, etc).
- "note" (string, optional): Use only for extra info about usage that is *not already implied* by the other fields. (For example, you can note that "tu" is informal.) The "note" is a perfect place for this, so there's no need to include it in the "native" field.
- "example_sentence_target_language" (string): A natural example sentence using the word in {TARGET_LANGUAGE}. (Be sure that the word's usage in the example sentence has the same morphology as is provided.)
- "example_sentence_native_language" (string): A natural {NATIVE_LANGUAGE} translation of the example sentence.
- "cognate": (bool) whether the {TARGET_LANGUAGE} word is a cognate in {NATIVE_LANGUAGE}. For our purposes, a word is a cognate to a definition if it looks similar to the {NATIVE_LANGUAGE} word. So "avocat" is a cognate for "avocado", but not "lawyer".
- "false_cognate": (bool) whether the {TARGET_LANGUAGE} word is a false cognate / false friend in {NATIVE_LANGUAGE}. For our purposes, a word is a false cognate to a definition if it looks similar to a different {NATIVE_LANGUAGE} word and might be easily confused, a classic example being the french word "actuellement" not corresponding to the english word "actually".

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

Output the result as a JSON object containing an array of one or more definition objects. Of course, their native language is {NATIVE_LANGUAGE}, so you should write the notes in {NATIVE_LANGUAGE}."#
    )
}

fn phrasebook_system_prompt() -> String {
    format!(
        r#"The input is a {TARGET_LANGUAGE} multi-word term along with example sentences showing its usage. Generate a phrasebook entry for it, to be used in an app for beginner {TARGET_LANGUAGE} learners (whose native language is {NATIVE_LANGUAGE}).

Think about the word and its meaning based on how it's used in the example sentences, and what is likely to be relevant to a beginner learner. Your thoughts will not be shown to the user. Then, write the word, then provide the meaning as the closest {NATIVE_LANGUAGE} equivalent word or short phrase — just like a dictionary translation (e.g. "just did (something)", "what" or "that which", "as soon as"). Skip any preamble like "the {TARGET_LANGUAGE} term [term] is often used to indicate that...", or "a question phrase equivalent to..." and just give the {NATIVE_LANGUAGE} equivalent. Any grammatical notes belong in the "additional_notes" field, not the meaning. Then, provide additional context for how the term is used in the "additional_notes" field. Finally, provide your own example of the term's usage in a natural sentence.

If the term is informal/slang, you can also set the "informal" field to true. For example, the english phrase "kick the bucket" is informal. If the term makes sense from the component words, you can also set the "compositional" field to true. (E.g. "Être sur son 31" makes no sense from its individual words, nor does "Altes Haus" in german. But "C'est" does make sense from its individual words) More examples: "pass away" is non-compositional (it's a phrasal verb whose meaning isn't obvious from "pass" + "away") but not informal. And "it is what it is" is informal but compositional.

"Cognate" and "false_cognate" are boolean fields that indicate whether the {TARGET_LANGUAGE} word is a cognate or false cognate in {NATIVE_LANGUAGE}.

For our purposes, a phrase is a cognate to a definition if it looks similar to the {NATIVE_LANGUAGE} phrase. So "carte de crédit" is a cognate for "credit card", and "en général" is a cognate for "in general".
And a phrase is a false cognate to a definition if it looks similar to a different {NATIVE_LANGUAGE} phrase and might be easily confused, a classic example being the spanish phrase "en absoluto" which looks like "absolutely" but means "not at all". (Another example is French "passer un examen" which looks like "pass an exam" but actually means "take an exam" with no implication of success).

Lastly, "can_be_translated_literally" should be set to true if the phrase can be translated literally into the {NATIVE_LANGUAGE} phrase. For example, "carte de crédit" can be translated literally into "credit card", and "en général" can be translated literally into "in general".

More french/english examples:
"ce que" → not a cognate, but can be translated literally ("that which")
"avoir lieu" → not a cognate, cannot be translated literally ("to have place" ≠ "to take place")

Example:
Input: multiword term: `ce que`
Example sentences:
1. Dis-moi ce que tu veux.
2. Je ne sais pas ce que c'est.

Output: {{{{
    "target_language_multi_word_term":"ce que",
    "meaning":"'what' or 'that which'.",
    "additional_notes": "Refers to something previously mentioned or understood from context.",
    "target_language_example":"C'est ce que je pensais.",
    "native_language_example":"That's what I thought.",
    "informal": false,
    "compositional": true,
    "cognate": false,
    "false_cognate": false,
    "can_be_translated_literally": true
}}}}

Don't capitalize the first letter of the meaning unless it makes sense (e.g. english proper nouns, german nouns, etc). Do not add pronunciation, IPA, part of speech, gender, or conjugation info unless it's in the "additional_notes" field and truly necessary.

Of course, their native language is {NATIVE_LANGUAGE}, so you should write the meaning and additional notes in {NATIVE_LANGUAGE}."#
    )
}

struct DictTestCase {
    word: &'static str,
    lemma: &'static str,
    pos: &'static str,
}

struct PhrasebookTestCase {
    term: &'static str,
    examples: &'static [&'static str],
}

#[ignore]
#[tokio::test]
async fn compare_dictionary_definitions() {
    let clients = make_clients();
    if clients.is_empty() {
        panic!(
            "No API keys configured. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, and/or GEMINI_API_KEY."
        );
    }

    let test_cases = [
        DictTestCase {
            word: "actuellement",
            lemma: "actuellement",
            pos: "ADV",
        },
        DictTestCase {
            word: "veux",
            lemma: "vouloir",
            pos: "VERB",
        },
        DictTestCase {
            word: "côté",
            lemma: "côté",
            pos: "NOUN",
        },
        DictTestCase {
            word: "tellement",
            lemma: "tellement",
            pos: "ADV",
        },
    ];

    for case in &test_cases {
        println!("\n============================================================");
        println!(
            "DICTIONARY: word=`{}` lemma=`{}` pos={}",
            case.word, case.lemma, case.pos
        );
        println!("============================================================");

        let user_msg = format!(
            "word: `{}`\nlemma: `{}`,\npos: {}",
            case.word, case.lemma, case.pos
        );

        for (name, client) in &clients {
            let result: Result<DictionaryDefinition, _> = client
                .chat_with_system_prompt(dict_system_prompt(), user_msg.clone())
                .await;

            println!("\n--- {name} ---");
            match result {
                Ok(def) => {
                    println!("  target_language_word: {}", def.target_language_word);
                    for (i, d) in def.definitions.iter().enumerate() {
                        println!("  definition {}:", i + 1);
                        println!("    native: {}", d.native);
                        if let Some(note) = &d.note {
                            println!("    note: {note}");
                        }
                        println!(
                            "    example: {} → {}",
                            d.example_sentence_target_language, d.example_sentence_native_language
                        );
                        println!(
                            "    cognate: {}, false_cognate: {}",
                            d.cognate, d.false_cognate
                        );
                    }
                }
                Err(e) => println!("  ERROR: {e:#?}"),
            }
        }
    }
}

#[ignore]
#[tokio::test]
async fn compare_phrasebook_definitions() {
    let clients = make_clients();
    if clients.is_empty() {
        panic!(
            "No API keys configured. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, and/or GEMINI_API_KEY."
        );
    }

    let test_cases = [
        PhrasebookTestCase {
            term: "venir de",
            examples: &[
                "Je viens de manger.",
                "Elle vient de partir.",
                "Nous venons de finir le projet.",
            ],
        },
        PhrasebookTestCase {
            term: "avoir beau",
            examples: &[
                "J'ai beau essayer, je n'y arrive pas.",
                "Il a beau être riche, il n'est pas heureux.",
            ],
        },
        PhrasebookTestCase {
            term: "en train de",
            examples: &[
                "Je suis en train de lire un livre.",
                "Elle est en train de cuisiner.",
            ],
        },
        PhrasebookTestCase {
            term: "n'importe quoi",
            examples: &["Il dit n'importe quoi.", "Tu peux acheter n'importe quoi."],
        },
    ];

    for case in &test_cases {
        println!("\n============================================================");
        println!("PHRASEBOOK: `{}`", case.term);
        println!("============================================================");

        let examples_text: String = case
            .examples
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {s}", i + 1))
            .collect::<Vec<_>>()
            .join("\n");

        let user_msg = format!(
            "multiword term: `{}`\n\nExample sentences:\n{examples_text}",
            case.term
        );

        for (name, client) in &clients {
            let result: Result<PhrasebookDefinitionEntry, _> = client
                .chat_with_system_prompt(phrasebook_system_prompt(), user_msg.clone())
                .await;

            println!("\n--- {name} ---");
            match result {
                Ok(entry) => {
                    println!("  meaning: {}", entry.meaning);
                    println!("  additional_notes: {}", entry.additional_notes);
                    println!(
                        "  example: {} → {}",
                        entry.target_language_example, entry.native_language_example
                    );
                    println!(
                        "  informal: {}, compositional: {}",
                        entry.informal, entry.compositional
                    );
                    println!(
                        "  cognate: {}, false_cognate: {}, literal: {}",
                        entry.cognate, entry.false_cognate, entry.can_be_translated_literally
                    );
                }
                Err(e) => println!("  ERROR: {e:#?}"),
            }
        }
    }
}
