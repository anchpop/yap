use axum::{
    Router,
    extract::{Json, Path},
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
use language_utils::{Language, TtsRequest, autograde, transcription_challenge};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tysm::chat_completions::ChatClient;

static CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("o3").unwrap().with_url(my_api)
});

// Include the language data rkyv file at compile time
static FRENCH_LANGUAGE_DATA: &[u8] = include_bytes!("../../out/fra/language_data.rkyv");
static SPANISH_LANGUAGE_DATA: &[u8] = include_bytes!("../../out/spa/language_data.rkyv");

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
    let _claims = verify_jwt(auth.token()).await?;

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
    let _claims = verify_jwt(auth.token()).await?;

    let client = reqwest::Client::new();

    let google_api_key =
        std::env::var("GOOGLE_CLOUD_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Select voice and language code based on language
    let (language_code, voice_name) = match request.language {
        Language::French => ("fr-FR", "fr-FR-Chirp3-HD-Achernar"),
        Language::Spanish => ("es-ES", "es-ES-Chirp3-HD-Achernar"),
        Language::English => ("en-US", "en-US-Chirp3-HD-Achernar"),
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
    let _claims = verify_jwt(auth.token()).await?;

    let autograde::AutoGradeTranslationRequest {
        challenge_sentence,
        user_sentence,
        primary_expression,
        lexemes,
        language,
    } = request;

    let language = match language {
        Language::French => "French",
        Language::Spanish => "Spanish",
        Language::English => "English",
    };

    let system_prompt = format!(
        r#"The user is learning {language}. They were challenged to translate a {language} sentence to English. Your goal is to identify which {language} words or phrases they remembered, and which ones they forgot. If they translated the sentence correctly, that means they remembered everything! But if they translated the sentence incorrectly, we need to figure out what words and phrases they seemed to have remembered correctly, and which ones they seem to have remembered incorrectly. This will be used as part of a spaced-repetition system, which will help users study the words they need to. The system can only incorporate this for the words that it knows are in the sentence, which will be provided to you. Words are provided with additional context about their part of speech and lemmatised form, to allow you to distinguish between different usages of the same word. The 'primary word' is also provided, which is the word that the sentence most needed to test. You will also have the opportunity to provide an explanation, which you should make use of to provide the user with additional info if their translation is incorrect.

Many sentences will be "partial sentences," such as "Ne pas." meaning "Do not." These partial sentences are still useful as test sentences for the user, so you should still grade them. Respond with JSON.

Example Input: "Challenge sentence: J'ai à faire.
User response: I have to go.
Primary expression: faire
Expressions: {{word: 'je', lemma: 'je', pos: 'PRON'}}, {{word: 'ai', lemma: 'avoir', pos: 'VERB'}}, {{word: 'à', lemma: 'à', pos: 'ADP'}}, {{word: 'faire', lemma: 'faire', pos: 'VERB'}}"

Example output: {{
"explanation": "The {language} expression 'avoir à faire' means 'to have (something) to do,' implying you are busy. You rendered 'faire' as 'go,' but it should be 'do.'  A correct translation is: 'I have things to do.'"
"primary_expression_status": "Forgot",  // The primary expression was "faire", and it looks like the user incorrectly translated it to "go". Therefore, we'll mark it as "forgot"
"expressions_remembered": [{{"Heteronym": {{ "word": "je", "lemma": "je", "pos": "Pron" }}, {{"Heteronym": {{ "word": "ai", "lemma": "avoir", "pos": "Verb" }}, {{"Heteronym": {{ "word": "à", "lemma": "à", "pos": "Adp" }}}}],
"expressions_forgot": [{{"Heteronym": {{ "word": "faire", "lemma": "faire", "pos": "Verb" }}}}],
}}

If there are lexemes (particularly multiword terms) that are not in the challenge sentence, do not include them in the expressions_remembered or expressions_forgot arrays. (Since the user did not have a chance to try to translate them.) However, if the user forgot a word that is in the challenge sentence, include it in the expressions_forgot array. The conjugations used in the multiword terms might be different than how they appear in the challenge sentence.

The explanation should be written as if speaking directly to the user. Markdown formatting is allowed. Try to keep the explanations short and concise. The user is still learning {language}, so respond in English!
"#
    );

    let autograde_response: autograde::AutoGradeTranslationResponse = CLIENT.chat_with_system_prompt(
        system_prompt,
        &{
            format!(
            "{language} challenge sentence: {challenge_sentence}\nUser response: {user_sentence}\nPrimary expression: {primary_expression}\nExpressions: {expressions}",
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
    let _claims = verify_jwt(auth.token()).await?;

    let language_name = match request.language {
        Language::French => "French",
        Language::Spanish => "Spanish",
        Language::English => "English",
    };

    let system_prompt = format!(
        r#"The user is learning {language_name} through transcription exercises. They listened to {language_name} audio and were asked to transcribe certain parts of the sentence while other parts were provided to them. Your job is to grade their transcription by comparing what they heard with what they wrote.

For each word they were asked to transcribe, assign one of these grades:
- Perfect: They transcribed the word in a way that makes sense semantically and is consistent with what they heard. Essentially, whether the transcription was correct. (This is relevant because some {language_name} sentences are ambiguous when spoken - if the user wrote a homophone that is contextually valid, they should not be penalized.)
- CorrectWithTypo: They wrote a word that is correct, but with a typo or accent error. If they typoed it into a different word entirely, you should not mark it as CorrectWithTypo.
- PhoneticallyIdenticalButContextuallyIncorrect: They wrote a word that sounds the same but is contextually wrong. Especially in the case where the user wrote the wrong conjugation of a word, you should mark it as PhoneticallyIdenticalButContextuallyIncorrect and explain to the user what other words in the sentence would have tipped them off as to what conjugation to use. However, remember that the user only hears the audio, and so if there are multiple possible words that sound the same and are all contextually valid interpretations, you should mark it as Perfect.
- PhoneticallySimilarButContextuallyIncorrect: They wrote a word that sounds similar but is contextually wrong
- Incorrect: They wrote something incorrect that doesn't sound like the target word
- Missed: They didn't write this word at all

Consider common {language_name} homophones and near-homophones when grading. Be understanding of minor spelling mistakes if the phonetics are correct.

You should also provide a brief explanation if there are any errors, helping the user understand what they missed or confused.

Respond with JSON in this format:
{{
  "explanation": "Brief explanation of any errors, and how the user can improve.",
  "grades": ["Perfect", "PhoneticallyIdenticalButContextuallyIncorrect", "Missed", ...]
}}

The grades array should have one grade for each word the user was asked to transcribe, in the order they appear.

The explanation should be in English and help the user learn from their mistakes. Markdown formatting is allowed, and encouraged for emphasis. If the user appeared to confuse some words, you can include those words in the compare array, and a TTS example for each word will be generated for the user to hear. {}"#,
        match request.language {
            Language::French =>
                r#"For example, if the user confused "de" and "des", you could generate ["de", "des"] in the compare array."#,
            Language::Spanish =>
                r#"For example, if the user confused "esta" and "está", you could generate ["esta", "está"] in the compare array."#,
            Language::English =>
                r#"For example, if the user confused "then" and "than", you could generate ["then", "than"] in the compare array."#,
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

    // Get response from LLM
    #[derive(Deserialize, schemars::JsonSchema)]
    struct LlmResponse {
        explanation: Option<String>,
        grades: Vec<String>,
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
                    let grade = if let Some(grade_str) = llm_response.grades.get(grade_idx) {
                        match grade_str.as_str() {
                            "Perfect" => transcription_challenge::WordGrade::Perfect {},
                            "CorrectWithTypo" => {
                                transcription_challenge::WordGrade::CorrectWithTypo {}
                            },
                            "PhoneticallyIdenticalButContextuallyIncorrect" => {
                                transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect {}
                            }
                            "PhoneticallySimilarButContextuallyIncorrect" => {
                                transcription_challenge::WordGrade::PhoneticallySimilarButContextuallyIncorrect {}
                            }
                            "Missed" => transcription_challenge::WordGrade::Missed {},
                            _ => transcription_challenge::WordGrade::Incorrect {},
                        }
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
        explanation: llm_response.explanation,
        compare: llm_response.compare,
        results,
        autograding_error: None,
    };

    Ok(Json(grade))
}

async fn serve_language_data(Path(language): Path<String>) -> Response {
    if language == "fra" {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, FRENCH_LANGUAGE_DATA.len())
            .body(axum::body::Body::from(FRENCH_LANGUAGE_DATA))
            .unwrap()
    } else if language == "spa" {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, SPANISH_LANGUAGE_DATA.len())
            .body(axum::body::Body::from(SPANISH_LANGUAGE_DATA))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Not found"))
            .unwrap()
    }
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
        .route("/language-data/{language}", post(serve_language_data))
        .layer(CompressionLayer::new())
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Listening on port 8080");
    axum::serve(listener, app).await.unwrap();
}
