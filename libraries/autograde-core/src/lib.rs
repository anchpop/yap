//! Core translation-autograding logic, shared between the AI backend handler
//! and offline evaluation tooling.
//!
//! This is the exact prompt-construction + LLM-call + response-mapping pipeline
//! that `yap-ai-backend` serves at `/autograde-translation`, lifted out of the
//! HTTP handler so it can be reused (e.g. for model evals) without spinning up
//! the server. The handler is now a thin wrapper: it selects a `ChatClient`
//! (based on auth / difficulty) and calls [`grade_translation`].

use language_utils::{
    Language,
    autograde::{AutoGradeTranslationRequest, AutoGradeTranslationResponse, Remembered},
};
use serde::Deserialize;
use tysm::chat_completions::ChatClient;

/// Shared assistant persona prepended to grading/feedback prompts.
pub const PERSONALITY: &str = r#"You are a helpful assistant that helps users learn languages. You are friendly and encouraging, and you always try to help the user learn from their mistakes. When correcting the user's mistakes, first congratulate them on the parts they did well on, and then explain the mistakes they made and how they can improve. But the main thing to do is to explain the mistakes in a helpful (but concise) way, and encourage the user. You speak conversationally, as if you were speaking to the user directly. You don't use bullet points or headings, but you do break concepts into individual lines as necessary."#;

#[derive(Debug, thiserror::Error)]
pub enum GradeError {
    #[error("translation grading is not implemented for target language {0:?}")]
    UnsupportedLanguage(Language),
    #[error("llm error: {0}")]
    Llm(String),
}

/// Best-effort `"{label} IPA: /…/\n"` prompt line for a sentence, in
/// espeak's readable IPA form (word boundaries kept — the model-label
/// tokenization exists for the pronunciation verifier, not for an LLM).
/// Pure enrichment: if espeak is unavailable, the language is
/// unsupported, or anything fails, returns an empty string and grading
/// proceeds without the line.
async fn ipa_line(sentence: &str, language: Language, label: &str) -> String {
    match espeak::phonemize_phrase_ipa(sentence, language).await {
        Ok(Some(ipa)) if !ipa.is_empty() => format!("{label} IPA: /{ipa}/\n"),
        _ => String::new(),
    }
}

/// Grade a user's translation attempt, identifying which words/phrases they
/// remembered vs. forgot. Pure logic: pass in whichever [`ChatClient`] you want
/// to use (reasoning effort, model, endpoint are all configured on the client).
pub async fn grade_translation(
    client: &ChatClient,
    request: &AutoGradeTranslationRequest,
) -> Result<AutoGradeTranslationResponse, GradeError> {
    let AutoGradeTranslationRequest {
        challenge_sentence,
        user_sentence,
        literals,
        phrases,
        course,
        primary_expression,
    } = request;

    let target_language = course.target_language;
    let native_language = course.native_language;

    // Dedup phrases
    let phrases: Vec<language_utils::Gram<String>> = phrases
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // Build display string → gram map for converting LLM output back to grams
    let phrase_display_strings: Vec<String> = phrases
        .iter()
        .map(|g| g.to_display_string(target_language))
        .collect();
    let display_to_gram: std::collections::HashMap<&str, &language_utils::Gram<String>> =
        phrase_display_strings
            .iter()
            .zip(phrases.iter())
            .map(|(s, g)| (s.as_str(), g))
            .collect();

    // Check whether the primary expression is a phrase or a literal-level gram
    let primary_is_phrase = phrases.contains(primary_expression);

    // Count gradable literals early for threshold checks
    let gradable_count = literals
        .iter()
        .filter(|l| l.word.heteronym().is_some())
        .count();

    // Early return if nothing to grade. `literal_grades` keeps its contract of
    // one entry per literal (all None — nothing was gradable).
    if gradable_count == 0 && phrases.is_empty() {
        return Ok(AutoGradeTranslationResponse {
            encouragement: Some("Good effort!".to_string()),
            explanation: None,
            literal_grades: vec![None; literals.len()],
            phrases_remembered: vec![],
            phrases_forgot: vec![],
            autograding_error: None,
        });
    }

    if target_language == Language::Chinese {
        return Err(GradeError::UnsupportedLanguage(target_language));
    }
    let target_language_name = target_language.to_string();
    let native_language_name = native_language.to_string();

    // Build the literals list with indices for gradable words, _ for ungradable
    // Track which literal positions have gradable words (for mapping indices back)
    let mut literals_display = String::new();
    let mut gradable_index = 1u32;
    let mut index_to_position: Vec<usize> = Vec::new(); // Maps 1-based index to literal position
    for (position, literal) in literals.iter().enumerate() {
        let is_gradable = literal.word.heteronym().is_some();
        if is_gradable {
            index_to_position.push(position);
            literals_display.push_str(&format!(
                "{}. \"{}\" (lemma: {}, pos: {})\n",
                gradable_index,
                literal.word.text,
                literal
                    .word
                    .heteronym()
                    .map(|h| h.lemma.as_str())
                    .unwrap_or(&literal.word.text),
                literal
                    .word
                    .heteronym()
                    .map(|h| format!("{:?}", h.pos))
                    .unwrap_or_else(|| "OTHER".to_string())
            ));
            gradable_index += 1;
        } else {
            literals_display.push_str(&format!(
                "_. \"{}\" (does not need to be graded)\n",
                literal.word.text
            ));
        }
    }

    // Build phrases list
    let phrases_display = if phrases.is_empty() {
        "(none)".to_string()
    } else {
        phrase_display_strings
            .iter()
            .map(|p| format!("- \"{p}\""))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let primary_expression_system_instruction = if primary_is_phrase {
        let display = primary_expression.to_display_string(target_language);
        format!(
            "The phrase \"{display}\" motivated this challenge, so please always include it in either phrases_remembered or phrases_forgot."
        )
    } else {
        let words: Vec<&str> = primary_expression
            .0
            .iter()
            .filter_map(|atom| match atom {
                language_utils::Atom::Tok(word) => Some(word.text.as_str()),
                _ => None,
            })
            .collect();
        if words.len() == 1 {
            format!(
                "The word \"{}\" motivated this challenge, so please always grade it as Remembered or Forgot (not null) in literal_grades.",
                words[0]
            )
        } else {
            format!(
                "The words {words} motivated this challenge, so please always grade at least one of them as Remembered or Forgot (not null) in literal_grades. If you mark at least one of them as \"forgot\", the user will be shown more words with the words \"{display}\".",
                words = words
                    .iter()
                    .map(|w| format!("\"{w}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                display = primary_expression.to_display_string(target_language)
            )
        }
    };

    let system_prompt = format!(
        r#"{PERSONALITY}The user is learning {target_language_name}. They were challenged to translate a {target_language_name} sentence to {native_language_name}. Your goal is to identify which {target_language_name} words or phrases they remembered, and which ones they forgot. If they translated the sentence correctly, that means they remembered everything! But if they translated the sentence incorrectly, we need to figure out what words and phrases they seemed to have remembered correctly, and which ones they seem to have remembered incorrectly. This will be used as part of a spaced-repetition system, which will help users study the words they need to.

{primary_expression_system_instruction}

You will be given:
1. The challenge sentence and the user's response, each sometimes followed by its IPA pronunciation (connected-speech phonemes). Use the IPA to spot pronunciation- or homophone-driven mistakes. The IPA is generated by espeak and may be incorrect — especially the user response's IPA if their submission contains typos. Either IPA line may be absent.
2. Literals: Individual words in order, each with an index number. Words marked with "_" do not need grading (proper nouns, punctuation, etc.).
3. Phrases: Multi-word expressions that should be graded as units.

For each indexed literal, decide if the user remembered it ("Remembered"), forgot it ("Forgot"), or if it's indeterminate (null). Grade each literal individually based on the user's translation.

For phrases, list which ones were remembered and which were forgotten. If one was netiher remembered nor forgotten (e.g. it was not in the sentence), just don't mention it at all. There might be a lot of phrases in the provided list that are not actually in the sentence - that's just to give you a large block of marble to carve from, but our phrase detection is very liberal and expansive so it often picks up false positives that you should basically ignore.

Do not punish learners for non-literal translations if the meaning is preserved (including tense, tone, etc).

Many sentences will be "partial sentences," such as "Ne pas." meaning "Do not." These are still valid test sentences.

Respond with JSON in this format:
{{
  "encouragement": "Always provide: short positive message (1-2 sentences) highlighting what they got right",
  "explanation": "Only if errors: brief explanation of mistakes and how to improve",
  "literal_grades": [{{"index": 1, "result": "Remembered"}}, {{"index": 2, "result": "Forgot"}}, {{"index": 3, "result": null}}],
  "phrases_remembered": ["phrase1"],
  "phrases_forgot": ["phrase2"]
}}

Example:
Input:
Challenge sentence: Ça se passe bien.
User response: It passes itself well.

Literals:
1. "Ça" (lemma: ce, pos: Pron)
2. "se" (lemma: se, pos: Pron)
3. "passe" (lemma: passer, pos: Verb)
4. "bien" (lemma: bien, pos: Adv)
_. "." (does not need to be graded)

Phrases:
- "se passer"

Output:
{{
  "encouragement": "Good effort tackling this sentence!",
  "explanation": "The French expression '<word>se passer</word>' means 'to happen.' You translated it literally as 'pass itself.' A correct translation is: 'It's going well.'",
  "literal_grades": [{{"index": 1, "result": "Remembered"}}, {{"index": 2, "result": "Remembered"}}, {{"index": 3, "result": "Remembered"}}, {{"index": 4, "result": "Remembered"}}],
  "phrases_remembered": [],
  "phrases_forgot": ["se passer"]
}}

Note: Even though "se passer" was forgotten, the individual words "se" and "passe" were understood (the user knew they mean "itself" and "pass"), so they are marked as remembered.

The encouragement should always be provided, focus on what they got right, and be written as if speaking directly to the user. The explanation should only be provided if there are errors. Markdown formatting is allowed (no bullet points or numbered lists). Keep both short and concise. Respond in {native_language_name}!

When you mention a {target_language_name} word or phrase inside the encouragement or explanation, wrap it in a <word>...</word> tag (e.g. <word>word</word>). This lets the UI style and pronounce it correctly. Do not wrap {native_language_name} text.
"#,
    );

    // Phonemize both sentences so the LLM can reason about pronunciation-
    // and homophone-driven mistakes (espeak applies language-level
    // liaison/elision the per-word data can't). The challenge sentence is
    // in the target language; the user's response is their
    // native-language translation, so it gets the native voice.
    let challenge_ipa_line =
        ipa_line(challenge_sentence, target_language, "Challenge sentence").await;
    let user_ipa_line = ipa_line(user_sentence, native_language, "User response").await;

    let user_prompt = format!(
        r#"Challenge sentence: {challenge_sentence}
{challenge_ipa_line}User response: {user_sentence}
{user_ipa_line}
Literals:
{literals_display}
Phrases:
{phrases_display}"#
    );

    // LLM response format uses indexed grades for easier model tracking
    #[derive(Deserialize, schemars::JsonSchema)]
    struct LiteralGrade {
        index: u32,
        result: Option<Remembered>,
    }

    #[derive(Deserialize, schemars::JsonSchema)]
    struct LlmResponse {
        encouragement: Option<String>,
        explanation: Option<String>,
        literal_grades: Vec<LiteralGrade>,
        phrases_remembered: Vec<String>,
        phrases_forgot: Vec<String>,
    }

    let llm_response: LlmResponse = client
        .chat_with_system_prompt(system_prompt, &user_prompt)
        .await
        .map_err(|e| GradeError::Llm(format!("{e:?}")))?;

    // Map indexed grades back to positional array (one entry per literal)
    // Ungradable literals (Other word types) remain None
    let mut positional_grades: Vec<Option<Remembered>> = vec![None; literals.len()];
    for grade in llm_response.literal_grades {
        if grade.index >= 1 && (grade.index as usize) <= index_to_position.len() {
            let position = index_to_position[(grade.index - 1) as usize];
            positional_grades[position] = grade.result;
        }
    }

    // Sanitize phrase outputs:
    // 1. Map LLM display strings back to Gram<String> using the display_to_gram map (filters unknown phrases)
    // 2. Resolve contradictions: if same phrase in both, keep in forgot (forgot takes precedence)
    let mut phrases_forgot: Vec<language_utils::Gram<String>> = llm_response
        .phrases_forgot
        .into_iter()
        .filter_map(|p| display_to_gram.get(p.as_str()).map(|g| (*g).clone()))
        .collect();
    phrases_forgot.sort();
    phrases_forgot.dedup();

    let forgot_set: std::collections::BTreeSet<&language_utils::Gram<String>> =
        phrases_forgot.iter().collect();
    let mut phrases_remembered: Vec<language_utils::Gram<String>> = llm_response
        .phrases_remembered
        .into_iter()
        .filter_map(|p| display_to_gram.get(p.as_str()).map(|g| (*g).clone()))
        .filter(|p| !forgot_set.contains(p))
        .collect();
    phrases_remembered.sort();
    phrases_remembered.dedup();

    Ok(AutoGradeTranslationResponse {
        encouragement: llm_response.encouragement,
        explanation: llm_response.explanation,
        literal_grades: positional_grades,
        phrases_remembered,
        phrases_forgot,
        autograding_error: None,
    })
}
