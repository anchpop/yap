use axum::{
    Router,
    body::Bytes,
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
    ChatClient::from_env("gpt-5.4")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("medium")
        .with_service_tier("priority")
        .with_max_concurrent_requests(3)
});

static LOW_REASONING_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.4")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("low")
        .with_service_tier("priority")
        .with_max_concurrent_requests(3)
});

static UNAUTHENTICATED_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.4-mini")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("low")
        .with_max_concurrent_requests(1)
});

const PERSONALITY: &str = r#"You are a helpful assistant that helps users learn languages. You are friendly and encouraging, and you always try to help the user learn from their mistakes. When correcting the user's mistakes, first congratulate them on the parts they did well on, and then explain the mistakes they made and how they can improve. But the main thing to do is to explain the mistakes in a helpful (but concise) way, and encourage the user. You speak conversationally, as if you were speaking to the user directly. You don't use bullet points or headings, but you do break concepts into individual lines as necessary."#;

fn language_data_for_course(course: &Course) -> Option<&'static [u8]> {
    LANGUAGE_DATA.get(course).copied()
}

#[derive(Debug, Deserialize)]
struct LanguageDataRequest {
    course: Course,
    chunk_index: Option<usize>,
    chunk_size: Option<usize>,
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
    data.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Russian,
        },
        include_bytes!("../../out/rus_for_eng/language_data.rkyv") as &'static [u8],
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
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssml: Option<String>,
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
    #[serde(rename = "speakingRate")]
    speaking_rate: f64,
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
        Language::Russian => "hLjwV7lYzk15SWLUmhEH", // Russian voice
        Language::Japanese => "GxhGYQesaQaYKePCZDEC", // Japanese voice
        Language::Hindi => "K24eC7JpUgk8zMtQYrpV",  // Hindi voice

        Language::Chinese => todo!(),
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
        eprintln!("ElevenLabs TTS Error: {response:?}");
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
        Language::Portuguese => ("pt-BR", "pt-BR-Chirp3-HD-Achernar"),
        Language::Russian => ("ru-RU", "ru-RU-Chirp3-HD-Aoede"),
        Language::Japanese => ("ja-JP", "ja-JP-Chirp3-HD-Achernar"),
        Language::Hindi => ("hi-IN", "hi-IN-Chirp3-HD-Achernar"),

        Language::Chinese => todo!(),
    };

    let input = if request.is_ssml {
        GoogleTtsInput {
            text: None,
            ssml: Some(request.text),
        }
    } else {
        GoogleTtsInput {
            text: Some(request.text),
            ssml: None,
        }
    };

    let google_request = GoogleTtsRequest {
        input,
        voice: GoogleTtsVoice {
            language_code: language_code.to_string(),
            name: voice_name.to_string(),
        },
        audio_config: GoogleTtsAudioConfig {
            audio_encoding: "MP3".to_string(),
            speaking_rate: request.speed,
        },
    };

    let url =
        format!("https://texttospeech.googleapis.com/v1beta1/text:synthesize?key={google_api_key}");

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&google_request)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("Google TTS Error ({status}): {body}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let response_json: GoogleTtsResponse = response
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Google TTS already returns base64-encoded audio
    Ok(response_json.audio_content)
}

async fn openai_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    let _claims = verify_jwt(auth.token()).await;

    let client = reqwest::Client::new();

    let openai_api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut openai_request = serde_json::json!({
        "model": "gpt-4o-mini-tts",
        "input": request.text,
        "voice": "coral",
        "response_format": "mp3",
    });

    if let Some(instructions) = &request.instructions {
        openai_request["instructions"] = serde_json::Value::String(instructions.clone());
    }

    let response = client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {openai_api_key}"))
        .header("Content-Type", "application/json")
        .json(&openai_request)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("OpenAI TTS Error ({status}): {body}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

    Ok(base64_audio)
}

fn wrap_pcm_in_wav(
    pcm_data: &[u8],
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
) -> Vec<u8> {
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm_data.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);
    wav
}

async fn gemini_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    let _claims = verify_jwt(auth.token()).await;

    let client = reqwest::Client::new();

    let gemini_api_key =
        std::env::var("GEMINI_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let default_instructions = format!(
        "A fluent {lang} speaker is teaching the listener how to pronounce different {lang} words. Each word is enunciated clearly, with a small gap in between.",
        lang = request.language,
    );
    let instructions = request
        .instructions
        .as_deref()
        .unwrap_or(&default_instructions);

    let prompt = format!("{instructions}\n{}", request.text);

    let gemini_request = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseModalities": ["audio"],
            "temperature": 1,
            "speech_config": {
                "voice_config": {
                    "prebuilt_voice_config": {
                        "voice_name": "Zephyr"
                    }
                }
            }
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-preview-tts:streamGenerateContent?key={gemini_api_key}"
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&gemini_request)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("Gemini TTS Error ({status}): {body}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    // Gemini streamGenerateContent returns an array of response chunks
    let response_body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Collect all audio data chunks from the streaming response
    let mut audio_data = Vec::new();
    if let Some(chunks) = response_body.as_array() {
        for chunk in chunks {
            if let Some(data) = chunk
                .pointer("/candidates/0/content/parts/0/inlineData/data")
                .and_then(|v| v.as_str())
            {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                audio_data.extend_from_slice(&bytes);
            }
        }
    }

    if audio_data.is_empty() {
        eprintln!("Gemini TTS Error: no audio data in response");
        return Err(StatusCode::BAD_GATEWAY);
    }

    // Gemini returns raw linear16 PCM at 24kHz mono - wrap in a WAV header
    let wav_data = wrap_pcm_in_wav(&audio_data, 24000, 1, 16);
    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&wav_data);

    Ok(base64_audio)
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
        literals,
        phrases,
        course,
        primary_expression,
    } = request;

    let target_language = course.target_language;
    let native_language = course.native_language;

    // Dedup phrases
    let phrases: Vec<language_utils::Gram<String>> = phrases
        .into_iter()
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
    let primary_is_phrase = phrases.contains(&primary_expression);

    // Count gradable literals early for threshold checks
    let gradable_count = literals
        .iter()
        .filter(|l| l.word.heteronym().is_some())
        .count();

    // Early return if nothing to grade
    if gradable_count == 0 && phrases.is_empty() {
        return Ok(Json(autograde::AutoGradeTranslationResponse {
            encouragement: Some("Good effort!".to_string()),
            explanation: None,
            literal_grades: vec![],
            phrases_remembered: vec![],
            phrases_forgot: vec![],
            autograding_error: None,
        }));
    }

    if target_language == Language::Chinese {
        return Err(StatusCode::NOT_IMPLEMENTED);
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
1. Literals: Individual words in order, each with an index number. Words marked with "_" do not need grading (proper nouns, punctuation, etc.).
2. Phrases: Multi-word expressions that should be graded as units.

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
  "explanation": "The French expression 'se passer' means 'to happen.' You translated it literally as 'pass itself.' A correct translation is: 'It's going well.'",
  "literal_grades": [{{"index": 1, "result": "Remembered"}}, {{"index": 2, "result": "Remembered"}}, {{"index": 3, "result": "Remembered"}}, {{"index": 4, "result": "Remembered"}}],
  "phrases_remembered": [],
  "phrases_forgot": ["se passer"]
}}

Note: Even though "se passer" was forgotten, the individual words "se" and "passe" were understood (the user knew they mean "itself" and "pass"), so they are marked as remembered.

The encouragement should always be provided, focus on what they got right, and be written as if speaking directly to the user. The explanation should only be provided if there are errors. Markdown formatting is allowed (no bullet points or numbered lists). Keep both short and concise. Respond in {native_language_name}!
"#,
    );

    // Use low reasoning effort for simple challenges
    let client = if !logged_in {
        &UNAUTHENTICATED_CLIENT
    } else if gradable_count + phrases.len() <= 4 {
        &LOW_REASONING_CLIENT
    } else {
        &CLIENT
    };

    let user_prompt = format!(
        r#"Challenge sentence: {challenge_sentence}
User response: {user_sentence}

Literals:
{literals_display}
Phrases:
{phrases_display}"#
    );

    // LLM response format uses indexed grades for easier model tracking
    #[derive(Deserialize, schemars::JsonSchema)]
    struct LiteralGrade {
        index: u32,
        result: Option<autograde::Remembered>,
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
        .inspect_err(|e| eprintln!("Error: {e:?}"))
        .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Map indexed grades back to positional array (one entry per literal)
    // Ungradable literals (Other word types) remain None
    let mut positional_grades: Vec<Option<autograde::Remembered>> = vec![None; literals.len()];
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

    let autograde_response = autograde::AutoGradeTranslationResponse {
        encouragement: llm_response.encouragement,
        explanation: llm_response.explanation,
        literal_grades: positional_grades,
        phrases_remembered,
        phrases_forgot,
        autograding_error: None,
    };

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
            Language::Russian =>
                r#"For example, if the user confused "компания" and "кампания", or "предать" and "придать", you could generate ["компания", "кампания"] or ["предать", "придать"] in the compare array."#,
            Language::Japanese =>
                r#"For example, if the user confused "行って" and "言って", or "聞く" and "効く", you could generate ["行って", "言って"] or ["聞く", "効く"] in the compare array."#,
            Language::Hindi =>
                r#"For example, if the user confused "सुनना" and "सुनाना", or "बोलना" and "बुलाना", you could generate ["सुनना", "सुनाना"] or ["बोलना", "बुलाना"] in the compare array."#,

            Language::Chinese => {
                return Err(StatusCode::NOT_IMPLEMENTED);
            }
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
                    all_words_to_grade.push(literal.word.text.clone());
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
                    full_sentence_parts.push(literal.word.text.clone());
                }

                // For the sentence with blanks
                sentence_with_blanks.push("____".to_string());

                // For user's submission
                user_submission_parts.push(submission.clone());
            }
            transcription_challenge::PartSubmitted::Provided { part } => {
                // Add provided parts to all versions
                full_sentence_parts.push(part.word.text.clone());
                sentence_with_blanks.push(part.word.text.clone());
                user_submission_parts.push(part.word.text.clone());
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

// --- Pronunciation Feedback ---

#[derive(Deserialize)]
struct PronunciationFeedbackRequest {
    /// The sentence being practiced
    sentence: String,
    /// Target language
    language: Language,
    /// Base64-encoded user audio (mp3)
    user_audio: String,
    /// Base64-encoded reference audio (mp3)
    reference_audio: String,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct PronunciationFeedbackResponse {
    /// A brief encouraging remark about what the user did well
    encouragement: String,
    /// Detailed chunk-by-chunk feedback on pronunciation errors
    feedback: String,
}

#[derive(Deserialize)]
struct ModalPhonemeResponse {
    phonemes: Vec<ModalPhoneme>,
}

#[derive(Deserialize)]
struct ModalPhoneme {
    phoneme: String,
    confidence: f64,
    top_k: Vec<ModalPhonemeAlt>,
}

#[derive(Deserialize)]
struct ModalPhonemeAlt {
    phoneme: String,
    probability: f64,
}

fn format_phoneme_analysis(phonemes: &[ModalPhoneme]) -> String {
    phonemes
        .iter()
        .map(|p| {
            let alts: String = p
                .top_k
                .iter()
                .map(|a| format!("{}:{:.0}%", a.phoneme, a.probability * 100.0))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "  {} (conf={:.0}%) [{}]",
                p.phoneme,
                p.confidence * 100.0,
                alts
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

static GEMINI_PRO_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");
    ChatClient::new(&api_key, "gemini-3.1-pro-preview")
        .with_url("https://generativelanguage.googleapis.com/v1beta/openai/")
        .with_reasoning_effort("high")
});

async fn get_phonemes_from_modal(
    http: &reqwest::Client,
    audio_bytes: &[u8],
) -> Result<Vec<ModalPhoneme>, StatusCode> {
    // Convert mp3 to f32 samples at 16kHz using symphonia or just send raw and let Modal resample
    // For now, we send the audio as f32 samples. We need to decode the mp3 first.
    // Actually, the Modal endpoint expects raw float samples. Let's add an mp3 endpoint to Modal instead.
    // For now, let's base64-encode and send to a new Modal endpoint that accepts mp3 directly.

    let modal_url = std::env::var("WAV2VEC2_ENDPOINT_URL").unwrap_or_else(|_| {
        "https://anchpop--wav2vec2-phoneme-wav2vec2phoneme-predict.modal.run".to_string()
    });

    // We need to send raw audio samples. Let's decode the mp3 to PCM f32 here.
    // Use a subprocess call to ffmpeg to decode, or add a Rust mp3 decoder.
    // For simplicity in a server context, let's use the `rodio` or `minimp3` crate.
    // Actually, let's just update the Modal endpoint to accept base64 mp3 directly.
    // For now, let's do the conversion here with symphonia.

    // Decode mp3 to f32 samples at whatever sample rate, send with sample_rate
    let (samples, sample_rate) = decode_mp3_to_f32(audio_bytes).map_err(|e| {
        eprintln!("Failed to decode mp3: {e}");
        StatusCode::BAD_REQUEST
    })?;

    let payload = serde_json::json!({
        "audio": samples,
        "sample_rate": sample_rate,
        "top_k": 5,
    });

    let response = http
        .post(&modal_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            eprintln!("Modal request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("Modal error ({status}): {body}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let result: ModalPhonemeResponse = response.json().await.map_err(|e| {
        eprintln!("Failed to parse Modal response: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    Ok(result.phonemes)
}

fn decode_mp3_to_f32(mp3_bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    use std::io::Cursor;

    let cursor = Cursor::new(mp3_bytes);
    let mut decoder = minimp3::Decoder::new(cursor);

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;

    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                sample_rate = frame.sample_rate as u32;
                let channels = frame.channels;
                // Convert to mono f32
                for chunk in frame.data.chunks(channels) {
                    let mono = chunk.iter().map(|&s| s as f32).sum::<f32>() / channels as f32;
                    samples.push(mono / 32768.0);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(e) => return Err(format!("mp3 decode error: {e:?}")),
        }
    }

    if samples.is_empty() {
        return Err("No audio data decoded".to_string());
    }

    Ok((samples, sample_rate))
}

async fn generate_pronunciation_feedback(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<PronunciationFeedbackRequest>,
) -> Result<Json<PronunciationFeedbackResponse>, StatusCode> {
    let _claims = verify_jwt(auth.token()).await;
    let http = reqwest::Client::new();

    let user_audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.user_audio)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let reference_audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.reference_audio)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // Get phonemes from Modal for both audio clips in parallel
    let (user_phonemes, ref_phonemes) = tokio::try_join!(
        get_phonemes_from_modal(&http, &user_audio_bytes),
        get_phonemes_from_modal(&http, &reference_audio_bytes),
    )?;

    let user_detailed = format_phoneme_analysis(&user_phonemes);
    let ref_detailed = format_phoneme_analysis(&ref_phonemes);

    use tysm::chat_completions::{ChatMessage, ChatMessageContent, InputAudio, Role};

    let language_name = &request.language;

    let prompt = format!(
        "The user is practicing their {language_name} pronunciation.\n\
         They are practicing the sentence: \"{sentence}\".\n\
         The FIRST audio is the user's attempt.\n\
         The SECOND audio is a native {language_name} reference pronunciation.\n\n\
         A phoneme recognition model (wav2vec2) analyzed both recordings.\n\
         Each phoneme has a confidence score and the top-5 alternative\n\
         phonemes the model considered, with probabilities.\n\n\
         USER'S PRONUNCIATION:\n{user_detailed}\n\n\
         NATIVE REFERENCE:\n{ref_detailed}\n\n\
         Your analysis should be primarily based on what you hear\n\
         in the actual audio recordings. The phoneme analysis above is\n\
         provided only as a jumping-off point and food for thought —\n\
         use it to guide your attention, but trust your own ears.\n\n\
         Give detailed chunk-by-chunk feedback.\n\
         Be strict and honest. Do not give credit for sounds the user\n\
         did not produce correctly.",
        sentence = request.sentence,
    );

    let messages = vec![ChatMessage::new(
        Role::User,
        vec![
            ChatMessageContent::InputAudio {
                input_audio: InputAudio::mp3(user_audio_bytes),
            },
            ChatMessageContent::InputAudio {
                input_audio: InputAudio::mp3(reference_audio_bytes),
            },
            ChatMessageContent::Text { text: prompt },
        ],
    )];

    let response: PronunciationFeedbackResponse = GEMINI_PRO_CLIENT
        .chat_with_messages(messages)
        .await
        .map_err(|e| {
            eprintln!("Gemini pronunciation feedback failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    Ok(Json(response))
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

async fn serve_language_data(Json(request): Json<LanguageDataRequest>) -> Response {
    if let Some(language_data) = language_data_for_course(&request.course) {
        let body = match (request.chunk_index, request.chunk_size) {
            (Some(chunk_index), Some(chunk_size)) => {
                if chunk_size == 0 {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(axum::body::Body::from("chunk_size must be positive"))
                        .unwrap();
                }

                let start = chunk_index.saturating_mul(chunk_size);
                if start >= language_data.len() {
                    return Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .body(axum::body::Body::from("chunk_index out of range"))
                        .unwrap();
                }

                let end = (start + chunk_size).min(language_data.len());
                &language_data[start..end]
            }
            (None, None) => language_data,
            _ => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(axum::body::Body::from(
                        "chunk_index and chunk_size must be provided together",
                    ))
                    .unwrap();
            }
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, body.len())
            .body(axum::body::Body::from(body))
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

const SENTRY_HOST: &str = "o4511102905090048.ingest.us.sentry.io";
const SENTRY_PROJECT_ID: &str = "4511102907056128";

async fn sentry_tunnel(body: Bytes) -> StatusCode {
    // Parse the envelope header (first line) to verify the DSN matches our project.
    // Only the first line needs to be valid UTF-8; the rest may be binary (e.g. replay data).
    let newline_pos = body.iter().position(|&b| b == b'\n');
    let header_bytes = match newline_pos {
        Some(pos) => &body[..pos],
        None => &body[..],
    };
    let header_line = match std::str::from_utf8(header_bytes) {
        Ok(s) => s,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    // Verify the DSN in the envelope header points to our project
    if !header_line.contains(SENTRY_HOST) {
        return StatusCode::UNAUTHORIZED;
    }

    let url = format!("https://{SENTRY_HOST}/api/{SENTRY_PROJECT_ID}/envelope/");
    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("Content-Type", "application/x-sentry-envelope")
        .body(body.to_vec())
        .send()
        .await
    {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::BAD_GATEWAY,
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
        .route("/tts/openai", post(openai_text_to_speech))
        .route("/tts/gemini", post(gemini_text_to_speech))
        .route("/autograde-translation", post(autograde_translation))
        .route("/autograde-transcription", post(autograde_transcription))
        .route(
            "/pronunciation-feedback",
            post(generate_pronunciation_feedback),
        )
        .route("/language-data", post(serve_language_data))
        .route("/profile", get(get_profile).patch(update_profile))
        .route("/language-stats", post(update_language_stats))
        .route("/user-language-stats", get(get_language_stats))
        .route("/follow", post(follow_user))
        .route("/unfollow", post(unfollow_user))
        .route("/follow-status", get(get_follow_status))
        .route("/sentry-tunnel", post(sentry_tunnel))
        .layer(CompressionLayer::new())
        .layer(cors);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("Listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
