use axum::{
    Router,
    extract::Json,
    http::{StatusCode, header},
    response::Response,
    routing::{get, post},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use language_utils::{
    Course, Language, TtsRequest, autograde,
    profile::{
        FollowRequest, FollowResponse, FollowStatus, GetProfileQuery, Profile,
        UpdateLanguageStatsRequest, UpdateLanguageStatsResponse, UpdateProfileRequest,
        UpdateProfileResponse,
    },
    transcription_challenge,
};
use postgrest::Postgrest;
use resend_rs::{Resend, types::CreateEmailBaseOptions};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tysm::chat_completions::ChatClient;

static CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.1")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("medium")
        .with_max_concurrent_requests(3)
});

static LOW_REASONING_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.1")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("low")
        .with_max_concurrent_requests(3)
});

static UNAUTHENTICATED_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.1")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("low")
        .with_max_concurrent_requests(1)
});

const PERSONALITY: &str = r#"You are a helpful assistant that helps users learn languages. You are friendly and encouraging, and you always try to help the user learn from their mistakes. When correcting the user's mistakes, first congratulate them on the parts they did well on, and then explain the mistakes they made and how they can improve. But the main thing to do is to explain the mistakes in a helpful (but concise) way, and encourage the user. You speak conversationally, as if you were speaking to the user directly. You don't use bullet points or headings, but you do break concepts into individual lines as necessary."#;

fn language_data_for_course(course: &Course) -> Option<&'static [u8]> {
    LANGUAGE_DATA.get(course).copied()
}

// Include the language data rkyv file at compile time
static LANGUAGE_DATA: LazyLock<BTreeMap<Course, &'static [u8]>> = LazyLock::new(|| {
    let mut data = BTreeMap::new();
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::French,
        },
        include_bytes!("../../out/fra_for_eng/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::French,
            target_language: Language::English,
        },
        include_bytes!("../../out/eng_for_fra/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Spanish,
        },
        include_bytes!("../../out/spa_for_eng/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Korean,
        },
        include_bytes!("../../out/kor_for_eng/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::German,
        },
        include_bytes!("../../out/deu_for_eng/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Italian,
        },
        include_bytes!("../../out/ita_for_eng/language_data.rkyv") as &'static [u8],
    );
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Portuguese,
        },
        include_bytes!("../../out/por_for_eng/language_data.rkyv") as &'static [u8],
    );
    data
});

#[derive(Serialize)]
struct ElevenLabsRequest {
    text: String,
    model_id: String,
    voice_settings: VoiceSettings,
}

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
}

#[derive(Serialize)]
struct GoogleTtsRequest {
    input: GoogleTtsInput,
    voice: GoogleTtsVoice,
    #[serde(rename = "audioConfig")]
    audio_config: GoogleTtsAudioConfig,
}

#[derive(Serialize)]
struct GoogleTtsInput {
    text: String,
}

#[derive(Serialize)]
struct GoogleTtsVoice {
    #[serde(rename = "languageCode")]
    language_code: String,
    name: String,
}

#[derive(Serialize)]
struct GoogleTtsAudioConfig {
    #[serde(rename = "audioEncoding")]
    audio_encoding: String,
}

#[derive(Deserialize)]
struct GoogleTtsResponse {
    #[serde(rename = "audioContent")]
    audio_content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: uuid::Uuid, // subject (user id)
    exp: usize,      // expiry
}

#[allow(dead_code)]
async fn verify_jwt(token: &str) -> Result<Claims, StatusCode> {
    let jwt_secret =
        std::env::var("SUPABASE_JWT_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["authenticated"]);

    let decoding_key = DecodingKey::from_secret(jwt_secret.as_ref());

    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => Ok(token_data.claims),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    let client = reqwest::Client::new();

    let elevenlabs_request = ElevenLabsRequest {
        text: request.text,
        model_id: "eleven_multilingual_v2".to_string(),
        voice_settings: VoiceSettings {
            stability: 0.5,
            similarity_boost: 0.75,
        },
    };

    let elevenlabs_api_key =
        std::env::var("ELEVENLABS_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Select voice based on language
    let voice_id = match request.language {
        Language::French => "ohItIVrXTBI80RrUECOD", // Existing French voice
        Language::Spanish => "zl1Ut8dvwcVSuQSB9XkG", // Ninoska - Spanish voice
        Language::English => "ohItIVrXTBI80RrUECOD", // Default to French voice for now
        Language::Korean => "nbrxrAz3eYm9NgojrmFK", // Korean
        Language::German => "IWm8DnJ4NGjFI7QAM5lM", // Stephan - German voice
        Language::Italian => "sKbNSlHXq99bttvf8rRF", // Nicola Lorusso - Italian voice
        Language::Portuguese => "tS45q0QcrDHqHoaWdCDR", // Lax - Portuguese voice

        Language::Chinese | Language::Japanese | Language::Russian => todo!(),
    };
    let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}");

    let response = client
        .post(&url)
        .header("Accept", "audio/mpeg")
        .header("Content-Type", "application/json")
        .header("xi-api-key", elevenlabs_api_key)
        .json(&elevenlabs_request)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

    Ok(base64_audio)
}

async fn google_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    let client = reqwest::Client::new();

    let google_api_key =
        std::env::var("GOOGLE_CLOUD_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Select voice and language code based on language
    let (language_code, voice_name) = match request.language {
        Language::French => ("fr-FR", "fr-FR-Chirp3-HD-Achernar"),
        Language::Spanish => ("es-ES", "es-ES-Chirp3-HD-Achernar"),
        Language::English => ("en-US", "en-US-Chirp3-HD-Achernar"),
        Language::Korean => ("ko-KR", "ko-KR-Chirp3-HD-Achernar"),
        Language::German => ("de-DE", "de-DE-Chirp3-HD-Achernar"),
        Language::Italian => ("it-IT", "it-IT-Chirp3-HD-Achernar"),
        Language::Portuguese => ("pt-PT", "pt-PT-Chirp3-HD-Achernar"),

        Language::Chinese | Language::Japanese | Language::Russian => todo!(),
    };

    let google_request = GoogleTtsRequest {
        input: GoogleTtsInput { text: request.text },
        voice: GoogleTtsVoice {
            language_code: language_code.to_string(),
            name: voice_name.to_string(),
        },
        audio_config: GoogleTtsAudioConfig {
            audio_encoding: "MP3".to_string(),
        },
    };

    let url =
        format!("https://texttospeech.googleapis.com/v1/text:synthesize?key={google_api_key}");

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&google_request)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let response_json: GoogleTtsResponse = response
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Google TTS already returns base64-encoded audio
    Ok(response_json.audio_content)
}

async fn autograde_translation(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<autograde::AutoGradeTranslationRequest>,
) -> Result<Json<autograde::AutoGradeTranslationResponse>, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;
    let logged_in = verify_jwt(auth.token()).await.is_ok();

    let autograde::AutoGradeTranslationRequest {
        challenge_sentence,
        user_sentence,
        primary_expression,
        lexemes,
        course,
    } = request;

    let target_language = course.target_language;
    let native_language = course.native_language;

    let (target_language_name, example) = match target_language {
        Language::French => (
            "French",
            r#"Example
Input: "Challenge sentence: Ça se passe bien.
User response: It passes itself well.
Primary expression: se passer
Expressions: {{word: 'ça', lemma: 'ce', pos: 'PRON'}}, {{word: 'se', lemma: 'se', pos: 'PRON'}}, {{word: 'passe', lemma: 'passer', pos: 'VERB'}}, {{word: 'bien', lemma: 'bien', pos: 'ADV'}}, {{word: 'se passer', lemma: 'se passer', pos: 'VERB'}}"

Output: {{
"explanation": "The French expression 'se passer' means 'to happen.' You translated it literally as 'pass itself.' A correct translation is: 'It's going well.'",
"primary_expression_status": "Forgot",
"expressions_remembered": [{{"Heteronym": {{ "word": "se", "lemma": "se", "pos": "Pron" }}}}, {{"Heteronym": {{ "word": "passe", "lemma": "passer", "pos": "Verb" }}}}, {{"Heteronym": {{ "word": "bien", "lemma": "bien", "pos": "Adv" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "se passer", "lemma": "se passer", "pos": "Verb" }}}}]
}}
"#,
        ),
        Language::Spanish => (
            "Spanish",
            r#"Example
Input: "Challenge sentence: Me di cuenta del error.
User response: I gave myself account of the error.
Primary expression: darse cuenta
Expressions: {{word: 'me', lemma: 'yo', pos: 'PRON'}}, {{word: 'di', lemma: 'dar', pos: 'VERB'}}, {{word: 'cuenta', lemma: 'cuenta', pos: 'NOUN'}}, {{word: 'del', lemma: 'de+el', pos: 'ADP'}}, {{word: 'error', lemma: 'error', pos: 'NOUN'}}, {{word: 'darse cuenta', lemma: 'darse cuenta', pos: 'VERB'}}"

Output: {{
"explanation": "In Spanish, 'darse cuenta' means 'to realize.' You translated it literally. A correct translation is: 'I realized the mistake.'",
"primary_expression_status": "Forgot",
"expressions_remembered": [{{"Heteronym": {{ "word": "me", "lemma": "yo", "pos": "Pron" }}}}, {{"Heteronym": {{ "word": "di", "lemma": "dar", "pos": "Verb" }}}}, {{"Heteronym": {{ "word": "cuenta", "lemma": "cuenta", "pos": "Noun" }}}}, {{"Heteronym": {{ "word": "error", "lemma": "error", "pos": "Noun" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "darse cuenta", "lemma": "darse cuenta", "pos": "Verb" }}}}]
}}
"#,
        ),
        Language::English => (
            "English",
            r#"Example
Input: "Challenge sentence: He gave up.
User response: He quit.
Primary expression: give up
Expressions: {{word: 'he', lemma: 'he', pos: 'PRON'}}, {{word: 'gave', lemma: 'give', pos: 'VERB'}}, {{word: 'up', lemma: 'up', pos: 'PART'}}, {{word: 'give up', lemma: 'give up', pos: 'VERB'}}"

Output: {{
"explanation": "Great work! 'Give up' means 'to quit,' which you translated correctly.",
"primary_expression_status": "Remembered",
"expressions_remembered": [{{"Heteronym": {{ "word": "he", "lemma": "he", "pos": "Pron" }}}}, {{"Heteronym": {{ "word": "give up", "lemma": "give up", "pos": "Verb" }}}}],
"expressions_forgot": []
}}
"#,
        ),
        Language::Korean => (
            "Korean",
            r#"Example
Input: "Challenge sentence: 책을 읽고 있어요.
User response: I read and am existing a book.
Primary expression: 읽고 있다
Expressions: {{word: '책을', lemma: '책', pos: 'NOUN'}}, {{word: '읽고', lemma: '읽다', pos: 'VERB'}}, {{word: '있어요', lemma: '있다', pos: 'VERB'}}, {{word: '읽고 있다', lemma: '읽고 있다', pos: 'VERB'}}"

Output: {{
"explanation": "The Korean expression '읽고 있다' means 'to be reading.' You treated it as separate verbs 'read' and 'exist.' A correct translation is: 'I am reading a book.'",
"primary_expression_status": "Forgot",
"expressions_remembered": [{{"Heteronym": {{ "word": "책을", "lemma": "책", "pos": "Noun" }}}}, {{"Heteronym": {{ "word": "읽다", "lemma": "읽다", "pos": "Verb" }}}}, {{"Heteronym": {{ "word": "있다", "lemma": "있다", "pos": "Verb" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "읽고 있다", "lemma": "읽고 있다", "pos": "Verb" }}}}]
}}
"#,
        ),
        Language::German => (
            "German",
            r#"Example
Input: "Challenge sentence: Ich gehe zur Schule.
User response: I go to the school.
Primary expression: zur
Expressions: {{word: 'ich', lemma: 'ich', pos: 'PRON'}}, {{word: 'gehe', lemma: 'gehen', pos: 'VERB'}}, {{word: 'zur', lemma: 'zu', pos: 'ADP'}}, {{word: 'Schule', lemma: 'Schule', pos: 'NOUN'}}"

Output: {{
"explanation": "Your translation is correct! 'Ich gehe zur Schule' means 'I go to school.'",
"primary_expression_status": "Remembered",
"expressions_remembered": [{{"Heteronym": {{ "word": "ich", "lemma": "ich", "pos": "Pron" }}}}, {{"Heteronym": {{ "word": "gehe", "lemma": "gehen", "pos": "Verb" }}}}, {{"Heteronym": {{ "word": "zur", "lemma": "zu", "pos": "Adp" }}}}, {{"Heteronym": {{ "word": "Schule", "lemma": "Schule", "pos": "Noun" }}}}],
"expressions_forgot": []
}}
"#,
        ),
        Language::Italian => (
            "Italian",
            r#"Example
Input: "Challenge sentence: Mi sono divertito.
User response: I am enjoyed myself.
Primary expression: divertirsi
Expressions: {{word: 'mi', lemma: 'io', pos: 'PRON'}}, {{word: 'sono', lemma: 'essere', pos: 'AUX'}}, {{word: 'divertito', lemma: 'divertire', pos: 'VERB'}}, {{word: 'divertirsi', lemma: 'divertirsi', pos: 'VERB'}}"

Output: {{
"explanation": "The Italian reflexive verb 'divertirsi' means 'to have fun' or 'to enjoy oneself.' You confused the tense and structure. A correct translation is: 'I had fun.'",
"primary_expression_status": "Forgot",
"expressions_remembered": [{{"Heteronym": {{ "word": "mi", "lemma": "io", "pos": "Pron" }}}}, {{"Heteronym": {{ "word": "sono", "lemma": "essere", "pos": "Aux" }}}}, {{"Heteronym": {{ "word": "divertito", "lemma": "divertire", "pos": "Verb" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "divertirsi", "lemma": "divertirsi", "pos": "Verb" }}}}]
}}
"#,
        ),
        Language::Portuguese => (
            "Portuguese",
            r#"Example
Input: "Challenge sentence: Vou pôr a mesa.
User response: I go for to put the table.
Primary expression: pôr
Expressions: {{word: 'vou', lemma: 'ir', pos: 'VERB'}}, {{word: 'pôr', lemma: 'pôr', pos: 'VERB'}}, {{word: 'a', lemma: 'o', pos: 'DET'}}, {{word: 'mesa', lemma: 'mesa', pos: 'NOUN'}}"

Output: {{
"explanation": "The verb 'pôr' means 'to put' or 'to place.' The expression 'pôr a mesa' means 'to set the table.' You translated 'vou' as 'go' instead of future tense, and mistook 'por' (for) for 'pôr' (to put). A correct translation is: 'I'm going to set the table.'",
"primary_expression_status": "Forgot",
"expressions_remembered": [{{"Heteronym": {{ "word": "vou", "lemma": "ir", "pos": "Verb" }}}}, {{"Heteronym": {{ "word": "mesa", "lemma": "mesa", "pos": "Noun" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "pôr", "lemma": "pôr", "pos": "Verb" }}}}]
}}
"#,
        ),
        Language::Chinese | Language::Japanese | Language::Russian => {
            return Err(StatusCode::NOT_IMPLEMENTED);
        }
    };

    let native_language_name = native_language.to_string();

    let system_prompt = format!(
        r#"{PERSONALITY}The user is learning {target_language_name}. They were challenged to translate a {target_language_name} sentence to {native_language_name}. Your goal is to identify which {target_language_name} words or phrases they remembered, and which ones they forgot. If they translated the sentence correctly, that means they remembered everything! But if they translated the sentence incorrectly, we need to figure out what words and phrases they seemed to have remembered correctly, and which ones they seem to have remembered incorrectly. This will be used as part of a spaced-repetition system, which will help users study the words they need to. The system can only incorporate this for the words that it knows are in the sentence, which will be provided to you. Words are provided with additional context about their part of speech and lemmatised form, to allow you to distinguish between different usages of the same word. The 'primary word' is also provided, which is the word that the sentence most needed to test. You should always provide encouragement highlighting what the user got right and acknowledging their progress. If there are any errors, also provide a brief explanation focusing on where they made mistakes and how they can improve.

Many sentences will be "partial sentences," such as "Ne pas." meaning "Do not." These partial sentences are still useful as test sentences for the user, so you should still grade them.

Do not grade individual words when they are part of a multi-word expression that can be definitively graded as remembered or forgotten. Grade the multi-word expression as a whole. Only grade the individual words separately if the user's translation clearly shows they specifically understood or failed to understand those words independently. For example, if the challenge sentence includes "se passer" and the user writes "pass itself," mark "se passer" as forgotten but mark "se" and "passe" as remembered. On the other hand, do not punish leaners for not translating a sentence or phrase or word literally, if the meaning has been fully preserved (including past/present/future tense, tone, etc).

Respond with JSON.

{example}
If there are lexemes (particularly multiword terms) that are not in the challenge sentence, do not include them in the expressions_remembered or expressions_forgot arrays. (Since the user did not have a chance to try to translate them.) However, if the user forgot a word that is in the challenge sentence, include it in the expressions_forgot array. The conjugations used in the multiword terms might be different than how they appear in the challenge sentence.

The encouragement should always be provided, be a short positive message (1-2 sentences), focus on what they got right, and be written as if speaking directly to the user. The explanation should only be provided if there are errors, focus on their mistakes and how to improve, and be written as if speaking directly to the user. Markdown formatting is allowed for both (just no bullet points or numbered lists). Try to keep both short and concise. The user is still learning {target_language_name}, so respond in {native_language_name}!
"#,
    );

    // Use low reasoning effort for simple challenges with few lexemes
    let client = if !logged_in {
        &UNAUTHENTICATED_CLIENT
    } else if lexemes.len() <= 4 {
        &LOW_REASONING_CLIENT
    } else {
        &CLIENT
    };

    let autograde_response: autograde::AutoGradeTranslationResponse = client.chat_with_system_prompt(
        system_prompt,
        &{
            format!(
            "{target_language_name} challenge sentence: {challenge_sentence}\nUser response: {user_sentence}\nPrimary expression: {primary_expression}\nExpressions: {expressions}",
            challenge_sentence = challenge_sentence,
            user_sentence = user_sentence,
            primary_expression = serde_json::to_value(&primary_expression).unwrap(),
            expressions = serde_json::to_value(&lexemes).unwrap()
        )}
    )
    .await
    .inspect_err(|e| eprintln!("Error: {e:?}"))
    .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;
    eprintln!("Response: {autograde_response:?}");

    Ok(Json(autograde_response))
}

async fn autograde_transcription(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<autograde::AutoGradeTranscriptionRequest>,
) -> Result<Json<transcription_challenge::Grade>, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    let target_language = request.course.target_language;
    let native_language = request.course.native_language;
    let target_language_name = target_language.to_string();
    let native_language_name = native_language.to_string();

    let system_prompt = format!(
        r#"{PERSONALITY}The user is learning {target_language_name} through transcription exercises. They listened to {target_language_name} audio and were asked to transcribe certain parts of the sentence while other parts were provided to them. Your job is to grade their transcription by comparing what they heard with what they wrote.

For each word they were asked to transcribe, assign one of these grades:
- Perfect: They transcribed the word in a way that makes sense semantically and is consistent with what they heard. Essentially, whether the transcription was correct. (This is relevant because some {target_language_name} sentences are ambiguous when spoken - if the user wrote a homophone that is contextually valid, they should not be penalized.)
- CorrectWithTypo: They wrote a word that is correct, but with a typo or accent error. If they typoed it into a different word entirely, you should not mark it as CorrectWithTypo.
- PhoneticallyIdenticalButContextuallyIncorrect: They wrote a word that sounds the same but is contextually wrong. Especially in the case where the user wrote the wrong conjugation of a word, you should mark it as PhoneticallyIdenticalButContextuallyIncorrect and explain to the user what other words in the sentence would have tipped them off as to what conjugation to use. However, remember that the user only hears the audio, and so if there are multiple possible words that sound the same and are all contextually valid interpretations, you should mark it as Perfect. For example, if the user wrote "Faut pas" when the expected phrase was "faux pas", you should still mark it as Perfect because there was no grammatical or phonetic way for them to distinguish between the two.
- PhoneticallySimilarButContextuallyIncorrect: They wrote a word that sounds similar but is contextually wrong
- Incorrect: They wrote something incorrect that doesn't sound like the target word
- Missed: They didn't write this word at all

Consider common {target_language_name} homophones and near-homophones when grading. Be understanding of minor spelling mistakes if the phonetics are correct.

You should always provide encouragement highlighting what the user did right and acknowledging their progress. If there are any errors, also provide a brief explanation focusing on where they made mistakes and how they can improve.

Respond with JSON in this format:
{{
  "encouragement": "Always provide this: warm, encouraging message highlighting what the user got right and their progress.",
  "explanation": "Only if there are errors: brief explanation of where they made mistakes and how they can improve.",
  "grades": [{{"Perfect": {{"wrote": "the word the user wrote"}}}}, {{"PhoneticallyIdenticalButContextuallyIncorrect": {{"wrote": "the word the user wrote"}}}}, {{"Missed": {{}}}}, ...]
}}

The grades array should have one grade for each word the user was asked to transcribe, in the order they appear.

The encouragement should always be provided, be in {native_language_name}, be a short positive message (1-2 sentences), and focus on what they got right. The explanation should only be provided if there are errors, be in {native_language_name}, focus on their mistakes and how to improve, and help the user learn from their errors. Markdown formatting is allowed, and encouraged for emphasis (just no bullet points or numbered lists). If the user appeared to confuse some words, you can include those words in the compare array, and a TTS example for each word will be generated for the user to hear. {}

P.S. Don't bother giving the user IPA-style phonetic transcriptions as they may not understand them. But you can still try to explain the phonetic differences in terms that the user might understand."#,
        match target_language {
            Language::French =>
                r#"For example, if the user confused "de" and "des", you could generate ["de", "des"] in the compare array."#,
            Language::Spanish =>
                r#"For example, if the user confused "esta" and "está", you could generate ["esta", "está"] in the compare array."#,
            Language::English =>
                r#"For example, if the user confused "then" and "than", you could generate ["then", "than"] in the compare array."#,
            Language::Korean =>
                r#"For example, if the user confused "어떻게" and "어떡해", you could generate ["어떻게", "어떡해"] in the compare array."#,
            Language::German =>
                r#"For example, if the user confused "der" and "die", you could generate ["der", "die"] in the compare array."#,
            Language::Italian =>
                r#"For example, if the user confused "anno" and "hanno", or "pena" and "penna", you could generate ["anno", "hanno"] or ["pena", "penna"] in the compare array."#,
            Language::Portuguese =>
                r#"For example, if the user confused "avô" and "avó", or "coser" and "cozer", you could generate ["avô", "avó"] or ["coser", "cozer"] in the compare array."#,

            Language::Chinese | Language::Japanese | Language::Russian =>
                return Err(StatusCode::NOT_IMPLEMENTED),
        }
    );

    // Collect all words to be graded and their context
    let mut all_words_to_grade = Vec::new();
    let mut word_to_part_mapping = Vec::new(); // Track which part each word belongs to

    for (part_idx, part) in request.submission.iter().enumerate() {
        match part {
            transcription_challenge::PartSubmitted::AskedToTranscribe {
                parts,
                submission: _,
            } => {
                for literal in parts {
                    all_words_to_grade.push(literal.text.clone());
                    word_to_part_mapping.push((part_idx, all_words_to_grade.len() - 1));
                }
            }
            transcription_challenge::PartSubmitted::Provided { .. } => {
                // Skip provided parts - they don't need grading
            }
        }
    }

    // Reconstruct the full sentence to show what the user heard
    let mut full_sentence_parts = Vec::new();
    let mut sentence_with_blanks = Vec::new();
    let mut user_submission_parts = Vec::new();

    for part in &request.submission {
        match part {
            transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
                // For the full sentence
                for literal in parts {
                    full_sentence_parts.push(literal.text.clone());
                }

                // For the sentence with blanks
                sentence_with_blanks.push("____".to_string());

                // For user's submission
                user_submission_parts.push(submission.clone());
            }
            transcription_challenge::PartSubmitted::Provided { part } => {
                // Add provided parts to all versions
                full_sentence_parts.push(part.text.clone());
                sentence_with_blanks.push(part.text.clone());
                user_submission_parts.push(part.text.clone());
            }
        }
    }

    // Build the full context
    let full_sentence = full_sentence_parts.join(" ");
    let sentence_shown = sentence_with_blanks.join(" ");
    let user_sentence = user_submission_parts.join(" ");

    // Create list of words to grade with their positions
    let mut words_to_grade_list = Vec::new();
    for (i, word) in all_words_to_grade.iter().enumerate() {
        words_to_grade_list.push(format!("{}. {}", i + 1, word));
    }

    let prompt = format!(
        r#"User heard: "{}"
User saw: {}
User wrote: {}

Words that need grading:
{}"#,
        full_sentence,
        sentence_shown,
        user_sentence,
        words_to_grade_list.join("\n")
    );

    #[derive(
        Clone,
        Debug,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
    )]
    #[serde(tag = "type")]
    pub enum WordGradeResponse {
        Perfect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        CorrectWithTypo {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        PhoneticallyIdenticalButContextuallyIncorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        PhoneticallySimilarButContextuallyIncorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        Incorrect {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wrote: Option<String>,
        },
        Missed,
    }

    impl From<WordGradeResponse> for transcription_challenge::WordGrade {
        fn from(response: WordGradeResponse) -> Self {
            match response {
                WordGradeResponse::Perfect { wrote } => transcription_challenge::WordGrade::Perfect { wrote },
                WordGradeResponse::CorrectWithTypo { wrote } => transcription_challenge::WordGrade::CorrectWithTypo { wrote },
                WordGradeResponse::PhoneticallyIdenticalButContextuallyIncorrect { wrote } => transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect { wrote },
                WordGradeResponse::PhoneticallySimilarButContextuallyIncorrect { wrote } => transcription_challenge::WordGrade::PhoneticallySimilarButContextuallyIncorrect { wrote },
                WordGradeResponse::Incorrect { wrote } => transcription_challenge::WordGrade::Incorrect { wrote },
                WordGradeResponse::Missed => transcription_challenge::WordGrade::Missed {},
            }
        }
    }

    // Get response from LLM
    #[derive(Deserialize, schemars::JsonSchema)]
    struct LlmResponse {
        encouragement: Option<String>,
        explanation: Option<String>,
        grades: Vec<WordGradeResponse>,
        compare: Vec<String>,
    }

    let llm_response: LlmResponse = CLIENT
        .chat_with_system_prompt(system_prompt, &prompt)
        .await
        .inspect_err(|e| eprintln!("Error: {e:?}"))
        .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Convert LLM response to Grade structure
    let mut results = Vec::new();
    let mut grade_idx = 0;

    for part in request.submission {
        match part {
            transcription_challenge::PartSubmitted::AskedToTranscribe { parts, submission } => {
                let mut graded_words = Vec::new();

                for literal in parts {
                    let grade: transcription_challenge::WordGrade =
                        if let Some(grade) = llm_response.grades.get(grade_idx) {
                            grade.clone().into()
                        } else {
                            transcription_challenge::WordGrade::Missed {}
                        };

                    graded_words.push(transcription_challenge::PartGradedPart {
                        heard: literal,
                        grade,
                    });

                    grade_idx += 1;
                }

                results.push(transcription_challenge::PartGraded::AskedToTranscribe {
                    parts: graded_words,
                    submission,
                });
            }
            transcription_challenge::PartSubmitted::Provided { part } => {
                results.push(transcription_challenge::PartGraded::Provided { part });
            }
        }
    }

    let grade = transcription_challenge::Grade {
        encouragement: llm_response.encouragement,
        explanation: llm_response.explanation,
        compare: llm_response.compare,
        results,
        autograding_error: None,
    };

    Ok(Json(grade))
}

fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                // Remove other characters
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

const NEW_FOLLOWER_EMAIL_TEMPLATE_TEXT: &str = include_str!("email_templates/new_follower.txt");
const NEW_FOLLOWER_EMAIL_TEMPLATE_HTML: &str = include_str!("email_templates/new_follower.html");

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

async fn send_follow_notification(
    follower_id: uuid::Uuid,
    following_id: &str,
    supabase_url: &str,
    service_role_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key)
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Get the following user's profile to check if email notifications are enabled
    let following_profile_response = client
        .from("profiles")
        .select("id,display_name,email_notifications_enabled")
        .eq("id", following_id)
        .single()
        .execute()
        .await?;

    if !following_profile_response.status().is_success() {
        return Err("Failed to fetch following user's profile".into());
    }

    let following_profile: serde_json::Value = following_profile_response.json().await?;

    // Check if email notifications are enabled
    let email_notifications_enabled = following_profile["email_notifications_enabled"]
        .as_bool()
        .unwrap_or(true); // Default to true if field is missing

    if !email_notifications_enabled {
        // User has disabled email notifications, don't send email
        return Ok(());
    }

    let following_display_name = following_profile["display_name"]
        .as_str()
        .unwrap_or("there");

    // Get follower's display name
    let follower_profile_response = client
        .from("profiles")
        .select("display_name,display_name_slug")
        .eq("id", follower_id.to_string())
        .single()
        .execute()
        .await?;

    if !follower_profile_response.status().is_success() {
        return Err("Failed to fetch follower's profile".into());
    }

    let follower_profile: serde_json::Value = follower_profile_response.json().await?;
    let follower_display_name = follower_profile["display_name"]
        .as_str()
        .unwrap_or("Someone");

    // Get the email from auth.users table using Supabase REST API
    let auth_client = reqwest::Client::new();
    let auth_response = auth_client
        .get(format!("{supabase_url}/auth/v1/admin/users/{following_id}"))
        .header("apikey", service_role_key)
        .header("Authorization", format!("Bearer {service_role_key}"))
        .send()
        .await?;

    if !auth_response.status().is_success() {
        return Err("Failed to fetch user email from auth".into());
    }

    let auth_user: serde_json::Value = auth_response.json().await?;
    let email = auth_user["email"]
        .as_str()
        .ok_or("No email found for user")?;

    // Build the profile link with the correct format
    let profile_link = format!("https://yap.town/user/id/{follower_id}");

    // Escape user-provided content for HTML
    // following_display_name = person receiving the email (the one being followed)
    // follower_display_name = person who clicked follow
    let recipient_name_escaped = html_escape(following_display_name);
    let follower_name_escaped = html_escape(follower_display_name);

    // Replace template variables in HTML version
    let email_body_html = NEW_FOLLOWER_EMAIL_TEMPLATE_HTML
        .replace("{{recipient_name}}", &recipient_name_escaped)
        .replace("{{follower_name}}", &follower_name_escaped)
        .replace("{{profile_link}}", &profile_link);

    // Replace template variables in text version (no HTML escaping needed for plain text)
    let email_body_text = NEW_FOLLOWER_EMAIL_TEMPLATE_TEXT
        .replace("{{recipient_name}}", following_display_name)
        .replace("{{follower_name}}", follower_display_name);

    // Send email using Resend
    let resend_api_key = std::env::var("RESEND_API_KEY")?;
    let resend = Resend::new(&resend_api_key);

    // Use plain text for the subject (escape for safety)
    let subject = format!("{follower_display_name} just followed you on Yap Town!");

    let email_request =
        CreateEmailBaseOptions::new("Yap Town <noreply@yap.town>", [email], subject)
            .with_html(&email_body_html)
            .with_text(&email_body_text);

    resend.emails.send(email_request).await?;

    Ok(())
}

use axum::extract::Query;

async fn get_profile(Query(params): Query<GetProfileQuery>) -> Result<Json<Profile>, StatusCode> {
    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Build query based on provided parameter
    let mut query = client.from("profiles").select("*");

    if let Some(id) = params.id {
        query = query.eq("id", id);
    } else if let Some(slug) = params.slug {
        query = query.eq("display_name_slug", slug);
    } else {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Fetch the profile
    let response = query.single().execute().await.map_err(|e| {
        eprintln!("Error fetching profile: {e:?}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if response.status().is_success() {
        let profile: Profile = response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(profile))
    } else if response.status() == 406 {
        // 406 is what Supabase returns when no rows match
        Err(StatusCode::NOT_FOUND)
    } else {
        eprintln!("Failed to fetch profile: {:?}", response.text().await);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn get_language_stats(
    Query(params): Query<GetProfileQuery>,
) -> Result<Json<Vec<language_utils::profile::UserLanguageStats>>, StatusCode> {
    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Build query based on provided parameter - we need to get user_id first
    let user_id = if let Some(id) = params.id {
        id
    } else if let Some(slug) = params.slug {
        // First get the user_id from the profile
        let profile_response = client
            .from("profiles")
            .select("id")
            .eq("display_name_slug", slug)
            .single()
            .execute()
            .await
            .map_err(|e| {
                eprintln!("Error fetching profile for slug: {e:?}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if !profile_response.status().is_success() {
            return Err(StatusCode::NOT_FOUND);
        }

        let profile: serde_json::Value = profile_response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        profile["id"]
            .as_str()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .to_string()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // Fetch language stats for this user
    let response = client
        .from("user_language_stats")
        .select("*")
        .eq("user_id", user_id)
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error fetching language stats: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if response.status().is_success() {
        let stats: Vec<language_utils::profile::UserLanguageStats> = response
            .json()
            .await
            .inspect_err(|e| eprintln!("Error fetching language stats: {e:?}"))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(stats))
    } else {
        eprintln!(
            "Failed to fetch language stats: {:?}",
            response.text().await
        );
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn update_profile(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<UpdateProfileResponse>, StatusCode> {
    // Verify JWT token to get the user's ID
    let claims = verify_jwt(auth.token()).await?;
    let user_id = claims.sub;

    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Build the update payload
    let mut update_data = serde_json::Map::new();

    if let Some(display_name) = request.display_name {
        // Generate slug from display name
        let slug = slugify(&display_name);
        update_data.insert(
            "display_name".to_string(),
            serde_json::Value::String(display_name),
        );
        update_data.insert(
            "display_name_slug".to_string(),
            serde_json::Value::String(slug),
        );
    }

    if let Some(bio) = request.bio {
        update_data.insert("bio".to_string(), serde_json::Value::String(bio));
    }

    // If no fields to update, return early
    if update_data.is_empty() {
        return Ok(Json(UpdateProfileResponse { success: true }));
    }

    // Update the profile in Supabase
    let response = client
        .from("profiles")
        .eq("id", user_id.to_string())
        .update(serde_json::Value::Object(update_data).to_string())
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error updating profile: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if response.status().is_success() {
        Ok(Json(UpdateProfileResponse { success: true }))
    } else {
        eprintln!("Failed to update profile: {:?}", response.text().await);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn update_language_stats(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<UpdateLanguageStatsRequest>,
) -> Result<Json<UpdateLanguageStatsResponse>, StatusCode> {
    // Verify JWT token to get the user's ID
    let claims = verify_jwt(auth.token()).await?;
    let user_id = claims.sub;

    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Serialize the language to a string for the database
    let language_str = request.language.to_string();

    // Build the upsert payload
    let mut upsert_data = serde_json::Map::new();
    upsert_data.insert(
        "user_id".to_string(),
        serde_json::Value::String(user_id.to_string()),
    );
    upsert_data.insert(
        "language".to_string(),
        serde_json::Value::String(language_str),
    );
    upsert_data.insert(
        "total_count".to_string(),
        serde_json::Value::Number(request.total_count.into()),
    );
    upsert_data.insert(
        "daily_streak".to_string(),
        serde_json::Value::Number(request.daily_streak.into()),
    );
    upsert_data.insert("xp".to_string(), serde_json::json!(request.xp));
    upsert_data.insert(
        "percent_known".to_string(),
        serde_json::json!(request.percent_known),
    );

    if let Some(expiry) = request.daily_streak_expiry {
        upsert_data.insert(
            "daily_streak_expiry".to_string(),
            serde_json::Value::String(expiry),
        );
    }

    if let Some(start_time) = request.start_time {
        upsert_data.insert("started".to_string(), serde_json::Value::String(start_time));
    }

    upsert_data.insert(
        "last_updated".to_string(),
        serde_json::Value::String("now()".to_string()),
    );

    // Upsert the language stats
    let response = client
        .from("user_language_stats")
        .upsert(serde_json::Value::Object(upsert_data).to_string())
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error upserting language stats: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if response.status().is_success() {
        Ok(Json(UpdateLanguageStatsResponse { success: true }))
    } else {
        eprintln!(
            "Failed to upsert language stats: {:?}",
            response.text().await
        );
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn serve_language_data(Json(course): Json<Course>) -> Response {
    if let Some(language_data) = language_data_for_course(&course) {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, language_data.len())
            .body(axum::body::Body::from(language_data))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Not found"))
            .unwrap()
    }
}

async fn follow_user(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<FollowRequest>,
) -> Result<Json<FollowResponse>, StatusCode> {
    // Verify JWT token to get the user's ID
    let claims = verify_jwt(auth.token()).await?;
    let follower_id = claims.sub;

    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Prevent users from following themselves
    if follower_id.to_string() == request.user_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Clone user_id for email notification before it's moved
    let following_user_id = request.user_id.clone();

    // Insert the follow relationship
    let mut insert_data = serde_json::Map::new();
    insert_data.insert(
        "follower_id".to_string(),
        serde_json::Value::String(follower_id.to_string()),
    );
    insert_data.insert(
        "following_id".to_string(),
        serde_json::Value::String(request.user_id),
    );

    let response = client
        .from("follows")
        .insert(serde_json::Value::Object(insert_data).to_string())
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error inserting follow: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if response.status().is_success() {
        // Send email notification (non-blocking, errors are logged but don't fail the request)
        let supabase_url_clone = supabase_url.clone();
        let service_role_key_clone = service_role_key.clone();
        tokio::spawn(async move {
            if let Err(e) = send_follow_notification(
                follower_id,
                &following_user_id,
                &supabase_url_clone,
                &service_role_key_clone,
            )
            .await
            {
                eprintln!("Failed to send follow notification email: {e:?}");
            }
        });

        Ok(Json(FollowResponse { success: true }))
    } else {
        eprintln!("Failed to insert follow: {:?}", response.text().await);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn unfollow_user(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<FollowRequest>,
) -> Result<Json<FollowResponse>, StatusCode> {
    // Verify JWT token to get the user's ID
    let claims = verify_jwt(auth.token()).await?;
    let follower_id = claims.sub;

    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Delete the follow relationship
    let response = client
        .from("follows")
        .eq("follower_id", follower_id.to_string())
        .eq("following_id", request.user_id)
        .delete()
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error deleting follow: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if response.status().is_success() {
        Ok(Json(FollowResponse { success: true }))
    } else {
        eprintln!("Failed to delete follow: {:?}", response.text().await);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn get_follow_status(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Query(params): Query<GetProfileQuery>,
) -> Result<Json<FollowStatus>, StatusCode> {
    // Verify JWT token to get the current user's ID
    let claims = verify_jwt(auth.token()).await?;
    let current_user_id = claims.sub;

    // Get Supabase credentials from environment
    let supabase_url =
        std::env::var("SUPABASE_URL").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create Supabase client
    let client = Postgrest::new(format!("{supabase_url}/rest/v1"))
        .insert_header("apikey", service_role_key.clone())
        .insert_header("Authorization", format!("Bearer {service_role_key}"));

    // Get the target user's ID
    let target_user_id = if let Some(id) = params.id {
        id
    } else if let Some(slug) = params.slug {
        // First get the user_id from the profile
        let profile_response = client
            .from("profiles")
            .select("id")
            .eq("display_name_slug", slug)
            .single()
            .execute()
            .await
            .map_err(|e| {
                eprintln!("Error fetching profile for slug: {e:?}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if !profile_response.status().is_success() {
            return Err(StatusCode::NOT_FOUND);
        }

        let profile: serde_json::Value = profile_response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        profile["id"]
            .as_str()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .to_string()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // Check if current user follows target user
    let is_following_response = client
        .from("follows")
        .select("*")
        .eq("follower_id", current_user_id.to_string())
        .eq("following_id", &target_user_id)
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error checking follow status: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let is_following = if is_following_response.status().is_success() {
        let data: Vec<serde_json::Value> = is_following_response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        !data.is_empty()
    } else {
        false
    };

    // Get follower count (how many people follow the target user)
    let follower_count_response = client
        .from("follows")
        .select("*")
        .eq("following_id", &target_user_id)
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error fetching follower count: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let follower_count = if follower_count_response.status().is_success() {
        let data: Vec<serde_json::Value> = follower_count_response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        data.len() as i64
    } else {
        0
    };

    // Get following count (how many people the target user follows)
    let following_count_response = client
        .from("follows")
        .select("*")
        .eq("follower_id", &target_user_id)
        .execute()
        .await
        .map_err(|e| {
            eprintln!("Error fetching following count: {e:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let following_count = if following_count_response.status().is_success() {
        let data: Vec<serde_json::Value> = following_count_response
            .json()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        data.len() as i64
    } else {
        0
    };

    Ok(Json(FollowStatus {
        is_following,
        follower_count,
        following_count,
    }))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    let app = Router::new()
        .route("/", get(|| async { "Hello from fly.io!" }))
        .route("/tts", post(text_to_speech))
        .route("/tts/google", post(google_text_to_speech))
        .route("/autograde-translation", post(autograde_translation))
        .route("/autograde-transcription", post(autograde_transcription))
        .route("/language-data", post(serve_language_data))
        .route("/profile", get(get_profile).patch(update_profile))
        .route("/language-stats", post(update_language_stats))
        .route("/user-language-stats", get(get_language_stats))
        .route("/follow", post(follow_user))
        .route("/unfollow", post(unfollow_user))
        .route("/follow-status", get(get_follow_status))
        .layer(CompressionLayer::new())
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Listening on port 8080");
    axum::serve(listener, app).await.unwrap();
}
