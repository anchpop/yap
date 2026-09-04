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
    Course, Language, TtsProvider, TtsRequest, autograde,
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

mod tts_verify;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tysm::chat_completions::ChatClient;

static CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let my_api =
        "https://g7edusstdonmn3vxdh3qdypkrq0wzttx.lambda-url.us-east-1.on.aws/v1/".to_string();
    ChatClient::from_env("gpt-5.6-sol")
        .unwrap()
        .with_url(my_api)
        .with_reasoning_effort("medium")
        .with_service_tier("priority")
        .with_max_concurrent_requests(3)
});

/// Translation autograding runs on Gemini 3.5 Flash (low reasoning), hitting the
/// Gemini API directly via its OpenAI-compatible endpoint. An offline eval over
/// real user mistranslations found it ties the frontier models on
/// agreement-with-production while being the fastest, so all translation
/// challenges use it regardless of difficulty or auth.
static TRANSLATION_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");
    ChatClient::new(&api_key, "gemini-3.5-flash")
        .with_url("https://generativelanguage.googleapis.com/v1beta/openai/")
        .with_reasoning_effort("low")
        .with_max_concurrent_requests(10)
});

const PERSONALITY: &str = r#"You are a helpful assistant that helps users learn languages. You are friendly and encouraging, and you always try to help the user learn from their mistakes. When correcting the user's mistakes, first congratulate them on the parts they did well on, and then explain the mistakes they made and how they can improve. But the main thing to do is to explain the mistakes in a helpful (but concise) way, and encourage the user. You speak conversationally, as if you were speaking to the user directly. You don't use bullet points or headings, but you do break concepts into individual lines as necessary."#;

fn language_data_for_course(course: &Course, part: LanguageDataPart) -> Option<&'static [u8]> {
    LANGUAGE_DATA.get(course).map(|parts| match part {
        LanguageDataPart::Core => parts.core,
        LanguageDataPart::Sentences => parts.sentences,
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LanguageDataPart {
    Core,
    Sentences,
}

#[derive(Debug, Deserialize)]
struct LanguageDataRequest {
    course: Course,
    part: LanguageDataPart,
    chunk_index: Option<usize>,
    chunk_size: Option<usize>,
}

struct LanguageDataParts {
    core: &'static [u8],
    sentences: &'static [u8],
}

// Include the split language pack archives at compile time
static LANGUAGE_DATA: LazyLock<BTreeMap<Course, LanguageDataParts>> = LazyLock::new(|| {
    macro_rules! course {
        ($map:ident, $native:ident, $target:ident, $dir:literal) => {
            $map.insert(
                Course {
                    native_language: Language::$native,
                    target_language: Language::$target,
                },
                LanguageDataParts {
                    core: include_bytes!(concat!("../../out/", $dir, "/language_data_core.rkyv")),
                    sentences: include_bytes!(concat!(
                        "../../out/",
                        $dir,
                        "/language_data_sentences.rkyv"
                    )),
                },
            );
        };
    }
    let mut data = BTreeMap::new();
    course!(data, English, French, "fra_for_eng");
    course!(data, French, English, "eng_for_fra");
    course!(data, English, Spanish, "spa_for_eng");
    course!(data, English, Korean, "kor_for_eng");
    course!(data, English, German, "deu_for_eng");
    course!(data, English, Italian, "ita_for_eng");
    course!(data, English, Portuguese, "por_for_eng");
    course!(data, French, Portuguese, "por_for_fra");
    course!(data, English, Russian, "rus_for_eng");
    course!(data, English, Hindi, "hin_for_eng");
    course!(data, English, Thai, "tha_for_eng");
    course!(data, English, ChineseSimplified, "zho-hans_for_eng");
    course!(data, English, Japanese, "jpn_for_eng");
    data
});

#[derive(Serialize, Clone)]
struct ElevenLabsRequest {
    text: String,
    model_id: String,
    voice_settings: VoiceSettings,
}

#[derive(Serialize, Clone)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
}

/// Retry budget per provider. Each attempt now costs a synthesis *and* (for
/// gated languages) a transcription, so this is deliberately smaller than the
/// 5 `google-tts` used to run on its own.
const TTS_MAX_ATTEMPTS: usize = 3;

/// How much longer the race may run once an audible clip is in hand. Generous
/// on purpose: a wrong clip is very bad, so any arm that could still produce a
/// right one gets ample time — this only cuts off the tail where nothing was
/// ever going to pass.
const SALVAGE_GRACE: std::time::Duration = std::time::Duration::from_secs(15);

/// When to stop waiting on the race: [`SALVAGE_GRACE`] after the first
/// *audible* clip lands in salvage. A defective clip never starts the clock —
/// hurrying toward silence would be trading latency for a dead button. An
/// already-armed deadline is kept, not extended, so later clips can't push
/// the ceiling out.
fn salvage_deadline(
    salvage: &[(Rejection, bool, Vec<u8>)],
    current: Option<tokio::time::Instant>,
) -> Option<tokio::time::Instant> {
    if current.is_some() {
        return current;
    }
    salvage
        .iter()
        .any(|(grade, _, _)| *grade == Rejection::WrongWords)
        .then(|| tokio::time::Instant::now() + SALVAGE_GRACE)
}

/// Why a synthesis attempt produced no audio.
enum SynthError {
    /// This provider can never serve this request — a language it has no
    /// voice for, or credentials it wasn't given. Retrying is pointless, and
    /// so is reporting it: move to the next provider.
    Unsupported,
    /// Something went wrong talking to the provider — a network blip, a 429,
    /// a 5xx. Transient by nature, so it costs an attempt rather than the
    /// whole arm; the status is just what we report if nothing else works.
    Failed(StatusCode),
}

/// The providers to try, in order, for a request that asked for `primary`.
///
/// Gemini is deliberately never a fallback *target*. It's the one provider
/// that can rewrite what it was asked to read — it reproducibly turned
/// "Les Baxter ici ?" into "Laisse Baxter ici" — so falling back to it would
/// risk trading one wrong reading for another. ElevenLabs and Google are
/// faithful readers, which is exactly what's wanted once a content check has
/// already failed once.
///
/// SSML gets no fallback at all: only Google interprets it, and handing raw
/// markup to a provider that doesn't would have it read the tags aloud.
fn fallback_chain(primary: TtsProvider, request: &TtsRequest) -> Vec<TtsProvider> {
    if request.is_ssml {
        return vec![primary];
    }

    // Because the fallbacks race, arrival order picks the winner and list
    // order buys nothing. A provider that can't honor the request therefore
    // has to be excluded outright rather than merely ranked last: ElevenLabs
    // has no speaking-rate control, so a slowed request it won would come
    // back at full speed, quietly dropping the one thing the learner asked
    // for. Substituting a voice is fine; substituting the request is not.
    let honors_request = |provider: &TtsProvider| match provider {
        TtsProvider::ElevenLabs => (request.speed - 1.0).abs() <= f64::EPSILON,
        _ => true,
    };

    std::iter::once(primary)
        .chain(
            [TtsProvider::ElevenLabs, TtsProvider::Google]
                .into_iter()
                .filter(|p| *p != primary && honors_request(p)),
        )
        .collect()
}

/// Synthesize with one specific provider, once.
async fn synthesize_once(
    http: &reqwest::Client,
    provider: TtsProvider,
    request: &TtsRequest,
) -> Result<Option<Vec<u8>>, SynthError> {
    match provider {
        TtsProvider::ElevenLabs => elevenlabs_synthesize(http, request).await,
        TtsProvider::Google => google_synthesize(request).await,
        TtsProvider::OpenAI => openai_synthesize(http, request).await,
        TtsProvider::Gemini => gemini_synthesize(http, request).await,
    }
}

/// How bad a rejected clip is, worst first.
///
/// The two gates fail in genuinely different ways and the difference decides
/// what we hand back when nothing passes. A content mismatch is *audible* —
/// the learner hears speech, just not quite the right words. A signal defect
/// is silence, truncation, or garbage, which is a dead button however it was
/// produced. So an audible clip outranks a broken one even when the broken one
/// came from the provider that was actually asked for.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rejection {
    /// Failed `audio_defect`: broken as a signal.
    Defective,
    /// Failed the transcript check: fine as a recording, wrong as speech.
    WrongWords,
}

/// Both gates, in the order they should run: the free signal check first, the
/// paid transcription only if that passes. `Some` means reject.
async fn audio_rejection(
    http: &reqwest::Client,
    request: &TtsRequest,
    audio: &[u8],
) -> Option<(Rejection, String)> {
    if let Some(defect) = google_tts::audio_defect(audio) {
        return Some((Rejection::Defective, defect.to_string()));
    }
    let reason = tts_verify::content_defect(http, request, audio).await?;
    Some((Rejection::WrongWords, reason))
}

/// What a provider arm produced when it couldn't return passing audio.
struct ProviderFailure {
    /// The best clip it produced, if any. Worth keeping even though it failed:
    /// silence is a worse card than a doubtful clip.
    rejected: Option<(Rejection, Vec<u8>)>,
    /// What to report if no provider anywhere works. Preserved per-arm so a
    /// language nobody has a voice for still surfaces as 501 rather than
    /// being flattened into a generic 502.
    status: StatusCode,
}

/// One provider's whole budget, checks included.
async fn synthesize_provider_checked(
    http: &reqwest::Client,
    request: &TtsRequest,
    provider: TtsProvider,
    max_attempts: usize,
) -> Result<Vec<u8>, ProviderFailure> {
    let mut rejected: Option<(Rejection, Vec<u8>)> = None;
    let mut status = StatusCode::BAD_GATEWAY;

    for n in 1..=max_attempts {
        let audio = match synthesize_once(http, provider, request).await {
            Ok(Some(audio)) => audio,
            Ok(None) => {
                eprintln!("{provider:?} TTS: no audio on attempt {n}, retrying");
                continue;
            }
            // The only permanent no. Everything else gets the same budget as
            // a bad clip does — the alternative is that the retry count means
            // one thing for defective audio and nothing at all for a 503,
            // which matters most for Chinese and Thai, where the fallbacks
            // have no voice and this provider is the entire race.
            Err(SynthError::Unsupported) => {
                return Err(ProviderFailure {
                    rejected,
                    status: StatusCode::NOT_IMPLEMENTED,
                });
            }
            Err(SynthError::Failed(failed)) => {
                eprintln!("{provider:?} TTS: attempt {n} failed ({failed}), retrying");
                status = failed;
                continue;
            }
        };

        match audio_rejection(http, request, &audio).await {
            None => return Ok(audio),
            Some((grade, reason)) => {
                eprintln!("{provider:?} TTS: rejected attempt {n} ({reason}), retrying");
                // Keep the best of this arm's failures, not the first: an
                // attempt that came back silent must not outrank a later one
                // that at least made a sound.
                if rejected.as_ref().is_none_or(|(best, _)| grade > *best) {
                    rejected = Some((grade, audio));
                }
            }
        }
    }

    Err(ProviderFailure { rejected, status })
}

/// Synthesize `request`, checking every clip before returning it, and racing
/// the faithful providers when the requested one can't get it right.
///
/// Before this existed the four TTS handlers disagreed about robustness:
/// `google-tts` retried five times on a defect, Gemini three, and ElevenLabs
/// and OpenAI not at all — and none of them looked at *what the audio said*.
/// Funnelling all four through here means a new check is added once and every
/// provider inherits it, rather than being something each handler has to
/// remember to do.
///
/// Shape of the work, and why:
///
/// - **One attempt with the requested provider first.** Nearly every request
///   ends here, so the fan-out below costs nothing in the common case.
/// - **Then every provider at once, not one after another.** A clip that
///   failed the transcript check has already told us this provider is getting
///   the words wrong; asking the alternatives in sequence just stacks their
///   latencies. Racing them turns three round-trips into one.
/// - **First clip to pass wins.** Not "best" — there's no meaningful ranking
///   among clips that all say the right words, and waiting for a preferred
///   one would give back the latency we just bought. The trade is that the
///   voice on a rescued sentence isn't deterministic across runs.
/// - **Fifteen seconds after the first audible clip, stop waiting.** Some
///   sentences can never pass — "la mer" transcribes as "la mère" on
///   perfectly correct audio — and before this bound they burned every arm's
///   full budget on every fetch. The grace is deliberately generous: an arm
///   that is going to pass does so within one synth-and-verify cycle, a few
///   seconds, so fifteen keeps essentially every genuine rescue and a wrong
///   clip is only ever returned when the alternatives had ample time to beat
///   it and couldn't. It starts at the first *audible* clip rather than the
///   request, because until something is worth returning there is nothing to
///   trade latency against.
///
/// The audio returned may not come from the requested provider. That's
/// intended: the client treats `provider` as a preference rather than a
/// guarantee, and caches whatever comes back under the key it asked with —
/// so the correct clip is the one that gets kept.
async fn synthesize_checked(
    http: &reqwest::Client,
    request: &TtsRequest,
    primary: TtsProvider,
) -> Result<String, StatusCode> {
    // Fast path. Clips that fail a check are kept rather than discarded, so a
    // learner never lands on a silent card; the requested provider's is
    // preferred, which makes giving up entirely degrade to the old behaviour.
    // Every clip that failed a check, kept for the last resort below. Ranked
    // rather than first-wins, so a silent clip can't beat an audible one.
    let mut salvage: Vec<(Rejection, bool, Vec<u8>)> = Vec::new();

    let mut failure_status = match synthesize_provider_checked(http, request, primary, 1).await {
        Ok(audio) => return Ok(base64::engine::general_purpose::STANDARD.encode(&audio)),
        Err(failure) => {
            salvage.extend(failure.rejected.map(|(grade, audio)| (grade, true, audio)));
            failure.status
        }
    };

    let chain = fallback_chain(primary, request);
    if chain.len() > 1 {
        eprintln!("{primary:?} TTS: racing {} providers", chain.len());
    }

    // Spawned rather than merely joined so the providers genuinely overlap.
    // Dropping the set at any exit aborts whatever is still in flight, which
    // is what keeps the losing arms from costing a full budget each.
    let mut racers = tokio::task::JoinSet::new();
    for provider in chain {
        let http = http.clone();
        let request = request.clone();
        racers.spawn(async move {
            let outcome =
                synthesize_provider_checked(&http, &request, provider, TTS_MAX_ATTEMPTS).await;
            (provider, outcome)
        });
    }

    // Arms when the first audible clip lands in salvage — possibly already,
    // from the primary's fast path above. Defective clips don't arm it:
    // returning silence early would be trading latency for a dead button.
    let mut deadline = salvage_deadline(&salvage, None);

    loop {
        let joined = match deadline {
            Some(deadline) => tokio::select! {
                joined = racers.join_next() => joined,
                _ = tokio::time::sleep_until(deadline) => {
                    eprintln!(
                        "{primary:?} TTS: grace period expired with arms still \
                         running; returning best effort"
                    );
                    break;
                }
            },
            None => racers.join_next().await,
        };
        let Some(joined) = joined else {
            break; // every arm has finished
        };
        let Ok((provider, outcome)) = joined else {
            continue; // task panicked; the other arms can still win
        };
        match outcome {
            Ok(audio) => {
                if provider != primary {
                    eprintln!("{primary:?} TTS: {provider:?} won the race and passed its checks");
                }
                return Ok(base64::engine::general_purpose::STANDARD.encode(&audio));
            }
            Err(failure) => {
                if provider == primary {
                    // The requested provider's verdict is the one worth
                    // reporting; a fallback being unsupported says nothing
                    // about what the caller actually asked for.
                    failure_status = failure.status;
                }
                let is_primary = provider == primary;
                salvage.extend(
                    failure
                        .rejected
                        .map(|(grade, audio)| (grade, is_primary, audio)),
                );
                deadline = salvage_deadline(&salvage, deadline);
            }
        }
    }

    // Nothing passed anywhere — every arm exhausted itself, or the grace
    // period called time. Imperfect audio still beats a dead button: a
    // learner staring at a silent card is a worse outcome than one clip we
    // have doubts about. Audibility first, then the requested provider — an
    // ElevenLabs clip that says the wrong thing is worth more than a silent
    // one from the provider that was actually asked for.
    let audio = salvage
        .into_iter()
        .max_by_key(|(grade, is_primary, _)| (*grade, *is_primary))
        .map(|(_, _, audio)| audio)
        .ok_or_else(|| {
            eprintln!("{primary:?} TTS: no audio at all from any provider ({failure_status})");
            failure_status
        })?;
    eprintln!("{primary:?} TTS: every provider failed its checks; returning best effort");
    Ok(base64::engine::general_purpose::STANDARD.encode(&audio))
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

    verify_jwt_with_secret(token, jwt_secret.as_ref())
}

fn verify_jwt_with_secret(token: &str, secret: &[u8]) -> Result<Claims, StatusCode> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["authenticated"]);

    let decoding_key = DecodingKey::from_secret(secret);

    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => Ok(token_data.claims),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Health check that exercises the JWT crypto path. jsonwebtoken panics at
/// runtime (not compile time) if the crate is built without a crypto provider
/// feature (`rust_crypto`/`aws_lc_rs`), which would otherwise only surface on
/// the first authenticated request. A panic here drops the connection, so the
/// fly health check fails and the deploy is rejected.
async fn health() -> Result<&'static str, StatusCode> {
    let secret = b"health-check-secret";
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs() as usize
        + 60;
    let claims = serde_json::json!({
        "sub": uuid::Uuid::nil(),
        "exp": exp,
        "aud": "authenticated",
    });
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    verify_jwt_with_secret(&token, secret)?;
    Ok("ok")
}

async fn elevenlabs_synthesize(
    http: &reqwest::Client,
    request: &TtsRequest,
) -> Result<Option<Vec<u8>>, SynthError> {
    let api_key = std::env::var("ELEVENLABS_API_KEY").map_err(|_| SynthError::Unsupported)?;

    // Select voice based on language
    let voice_id = match request.language {
        Language::French => "ohItIVrXTBI80RrUECOD", // Existing French voice
        Language::Spanish => "8mBRP99B2Ng2QwsJMFQl", // Mexican Spanish voice
        Language::English => "ohItIVrXTBI80RrUECOD", // Default to French voice for now
        Language::Korean => "nbrxrAz3eYm9NgojrmFK", // Korean
        Language::German => "IWm8DnJ4NGjFI7QAM5lM", // Stephan - German voice
        Language::Italian => "sKbNSlHXq99bttvf8rRF", // Nicola Lorusso - Italian voice
        Language::Portuguese => "tS45q0QcrDHqHoaWdCDR", // Lax - Portuguese voice
        Language::Russian => "hLjwV7lYzk15SWLUmhEH", // Russian voice
        Language::Japanese => "GxhGYQesaQaYKePCZDEC", // Japanese voice
        Language::Hindi => "K24eC7JpUgk8zMtQYrpV",  // Hindi voice

        // Haoran (Beijing Mandarin) and Anna Su (Taiwan Mandarin). One voice
        // could cover both, since the model reads Traditional and Simplified
        // to the same speech — but the two courses exist precisely because
        // they aren't the same target, so accent follows script here exactly
        // as it does across Google's cmn-CN and cmn-TW voices.
        Language::ChineseSimplified => "pU9NaAwkoR3v0Mrg3uKz",
        Language::ChineseTraditional => "r6qgCCGI7RWKXCagm158",
        // Genuinely absent: `eleven_multilingual_v2` has no Thai, and it's
        // only in `eleven_v3`, a different model with a different contract.
        // Google is Thai's whole race until that changes.
        Language::Thai => return Err(SynthError::Unsupported),
    };

    let body = ElevenLabsRequest {
        text: request.text.clone(),
        model_id: "eleven_multilingual_v2".to_string(),
        voice_settings: VoiceSettings {
            stability: 0.5,
            similarity_boost: 0.75,
        },
    };

    let response = http
        .post(format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}"
        ))
        .header("Accept", "audio/mpeg")
        .header("Content-Type", "application/json")
        .header("xi-api-key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| SynthError::Failed(StatusCode::INTERNAL_SERVER_ERROR))?;

    if !response.status().is_success() {
        eprintln!("ElevenLabs TTS Error: {response:?}");
        return Err(SynthError::Failed(StatusCode::BAD_GATEWAY));
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|_| SynthError::Failed(StatusCode::INTERNAL_SERVER_ERROR))?;

    Ok(Some(audio_bytes.to_vec()))
}

/// The Google locale and voice to synthesize `language` with.
///
/// Two voices per language, because Chirp3-HD mishandles `<break>` — and it
/// doesn't error, it eats the text next to the tag. Our pronunciation cards
/// are built entirely out of breaks:
///
/// ```text
/// <break time="100ms"/><say-as interpret-as="characters">e</say-as>
/// <break time="100ms"/>comme dans<break time="200ms"/>je
/// ```
///
/// Chirp3-HD reads that back as "comme dans", losing both the letter being
/// taught and the example word. Measured on the "e" card (transcribed with
/// the same Whisper model the ASR gate uses):
///
/// | payload                     | Chirp3-HD     | Neural2            |
/// |-----------------------------|---------------|--------------------|
/// | breaks + say-as (we ship)   | "comme dans"  | "euh comme dans J" |
/// | breaks only                 | "comme dans"  | "euh comme dans J" |
/// | say-as only, no breaks      | "e comme dans jeu" | "e comme dans J" |
/// | say-as, digraph "ch"        | "CH comme dans chat" | "CH comme dans chat" |
///
/// So `<say-as>` is *fine* on Chirp3-HD; `<break>` is the whole problem. This
/// is worth stating precisely because the obvious fix — drop the breaks, keep
/// the better voice — doesn't work either, for two reasons:
///
/// 1. Google's documented alternative, `[pause]` tags in the `markup` input
///    field, fails identically: `e [pause short] comme dans [pause] je` comes
///    back as "comme dans jeu", letter gone. Chirp3-HD has no working way to
///    put a pause before a short token.
/// 2. Chirp3-HD is generative, and on fragments this short it embellishes.
///    Asked for "e comme dans je" with no pauses at all it produced "c'est
///    comme dans le jeu" and "e comme dans je t'aide" — inventing words we
///    never sent. Neural2 and Wavenet returned the exact text every time.
///
/// For a card whose entire job is to demonstrate one letter, saying precisely
/// the requested words beats naturalness. That's the real axis here, and it's
/// why the split isn't just "older tier for SSML": Chirp3-HD stays the better
/// choice on the plain-text paths (grams, dictionary, sentence fallback).
///
/// # How this regressed without us touching it
///
/// The card shipped working in March 2026 and broke months later with no
/// change on our side. Google's release notes date the cause to 2025-10-17:
/// "Chirp 3 HD now supports speech synthesis using SSML input. Supported SSML
/// tags are: `<phoneme>`, `<p>`, `<s>`, `<sub>`, and `<say-as>`." No
/// `<break>` — so our breaks were silently *ignored*, the rest was read
/// correctly, and the card sounded right. Google later added `<break>` to the
/// supported list (it's on the Chirp3-HD docs page now, with no release-note
/// entry), and the implementation ate the adjacent text. Support for the tag
/// turned out to be worse than not having it.
///
/// Nothing downstream catches this: the audio is a healthy recording so
/// `audio_defect` passes it, and the ASR gate skips SSML because a transcript
/// can't resemble markup. It has to be caught here, at the point where a
/// voice that can't honor the request would otherwise be picked anyway.
fn google_voice(language: Language, is_ssml: bool) -> (&'static str, &'static str) {
    // `generative` reads more naturally; `literal` says exactly what it's given.
    let (language_code, generative_voice, literal_voice) = match language {
        Language::French => ("fr-FR", "fr-FR-Chirp3-HD-Achernar", "fr-FR-Neural2-F"),
        Language::Spanish => ("es-US", "es-US-Chirp3-HD-Achernar", "es-US-Neural2-A"),
        Language::English => ("en-US", "en-US-Chirp3-HD-Achernar", "en-US-Neural2-A"),
        Language::Korean => ("ko-KR", "ko-KR-Chirp3-HD-Achernar", "ko-KR-Neural2-A"),
        Language::German => ("de-DE", "de-DE-Chirp3-HD-Achernar", "de-DE-Neural2-G"),
        Language::Italian => ("it-IT", "it-IT-Chirp3-HD-Achernar", "it-IT-Neural2-A"),
        Language::Portuguese => ("pt-BR", "pt-BR-Chirp3-HD-Achernar", "pt-BR-Neural2-A"),
        // Russian and both Chinese locales have no Neural2 voice at all.
        Language::Russian => ("ru-RU", "ru-RU-Chirp3-HD-Aoede", "ru-RU-Wavenet-A"),
        Language::Japanese => ("ja-JP", "ja-JP-Chirp3-HD-Achernar", "ja-JP-Neural2-B"),
        Language::Hindi => ("hi-IN", "hi-IN-Chirp3-HD-Achernar", "hi-IN-Neural2-A"),
        Language::ChineseSimplified => ("cmn-CN", "cmn-CN-Chirp3-HD-Achernar", "cmn-CN-Wavenet-A"),
        Language::Thai => ("th-TH", "th-TH-Chirp3-HD-Achernar", "th-TH-Neural2-C"),
        // Taiwanese Mandarin, and the one language here without a Chirp3-HD
        // voice. `cmn-CN-Chirp3-HD` does read Traditional characters
        // correctly — the script is input encoding, the speech is Mandarin
        // either way — but a Traditional course is a Taiwan course, and a
        // mainland accent teaches the wrong pronunciation. Accent fidelity is
        // worth more to a learner here than a newer voice model. Being a
        // Wavenet voice already, it handles `<break>`, so both columns agree.
        Language::ChineseTraditional => ("cmn-TW", "cmn-TW-Wavenet-A", "cmn-TW-Wavenet-A"),
    };
    // `is_ssml` is a proxy for "contains `<break>`", which is exact today:
    // pronunciation cards are the only SSML we send, and they're all breaks.
    (
        language_code,
        if is_ssml {
            literal_voice
        } else {
            generative_voice
        },
    )
}

async fn google_synthesize(request: &TtsRequest) -> Result<Option<Vec<u8>>, SynthError> {
    let api_key = std::env::var("GOOGLE_CLOUD_API_KEY").map_err(|_| SynthError::Unsupported)?;

    let (language_code, voice_name) = google_voice(request.language, request.is_ssml);

    // One attempt per call: `synthesize_checked` owns the retry budget now, so
    // leaving the library's own loop enabled would multiply the two.
    let client = google_tts::GoogleTtsClient::new(api_key).with_max_attempts(1);
    let outcome = client
        .synthesize(&google_tts::GoogleTtsRequest {
            text: request.text.clone(),
            language_code: language_code.to_string(),
            voice_name: voice_name.to_string(),
            speed: request.speed,
            is_ssml: request.is_ssml,
        })
        .await
        .map_err(|e| {
            eprintln!("Google TTS error: {e:?}");
            SynthError::Failed(StatusCode::BAD_GATEWAY)
        })?;

    Ok(Some(outcome.audio_bytes))
}

async fn openai_synthesize(
    http: &reqwest::Client,
    request: &TtsRequest,
) -> Result<Option<Vec<u8>>, SynthError> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| SynthError::Unsupported)?;

    let mut body = serde_json::json!({
        "model": "gpt-4o-mini-tts",
        "input": request.text,
        "voice": "coral",
        "response_format": "mp3",
    });

    if let Some(instructions) = &request.instructions {
        body["instructions"] = serde_json::Value::String(instructions.clone());
    }

    let response = http
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|_| SynthError::Failed(StatusCode::INTERNAL_SERVER_ERROR))?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        eprintln!("OpenAI TTS Error ({status}): {detail}");
        return Err(SynthError::Failed(StatusCode::BAD_GATEWAY));
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|_| SynthError::Failed(StatusCode::INTERNAL_SERVER_ERROR))?;

    Ok(Some(audio_bytes.to_vec()))
}

async fn gemini_synthesize(
    http: &reqwest::Client,
    request: &TtsRequest,
) -> Result<Option<Vec<u8>>, SynthError> {
    let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| SynthError::Unsupported)?;
    let prompt = gemini_tts_prompt(request);
    gemini_tts_attempt(http, &api_key, &prompt)
        .await
        .map_err(SynthError::Failed)
}

async fn text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    synthesize_checked(&reqwest::Client::new(), &request, TtsProvider::ElevenLabs).await
}

async fn google_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    synthesize_checked(&reqwest::Client::new(), &request, TtsProvider::Google).await
}

async fn openai_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    let _claims = verify_jwt(auth.token()).await;

    synthesize_checked(&reqwest::Client::new(), &request, TtsProvider::OpenAI).await
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

/// Gemini's TTS model. Unlike Google Cloud TTS it has no per-language voice
/// list — one voice speaks every language, picked from the text itself — and
/// no `speakingRate` knob, so delivery is steered entirely by the prompt that
/// precedes the text. Language coverage includes every [`Language`] we ship.
const GEMINI_TTS_MODEL: &str = "gemini-3.1-flash-tts-preview";

/// House style for Gemini TTS when the caller doesn't ask for something else.
const GEMINI_TTS_DEFAULT_INSTRUCTIONS: &str = "Read aloud in a warm welcoming tone";

/// Builds the prompt Gemini speaks. Everything before the newline is
/// direction, everything after is the text to voice — which is also how the
/// speaking rate gets expressed, since the API has no rate parameter.
fn gemini_tts_prompt(request: &TtsRequest) -> String {
    let instructions = request
        .instructions
        .as_deref()
        .unwrap_or(GEMINI_TTS_DEFAULT_INSTRUCTIONS);

    let pace = if request.speed < 0.95 {
        ", speaking slowly and deliberately"
    } else if request.speed > 1.05 {
        ", speaking briskly"
    } else {
        ""
    };

    format!("{instructions}{pace}\n{}", request.text)
}

/// One synthesis round-trip. Returns WAV bytes, or `None` when the response
/// carried no audio (a transient Gemini failure worth retrying).
async fn gemini_tts_attempt(
    client: &reqwest::Client,
    api_key: &str,
    prompt: &str,
) -> Result<Option<Vec<u8>>, StatusCode> {
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
        "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_TTS_MODEL}:generateContent?key={api_key}"
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

    let response_body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Audio arrives as one or more inlineData parts of raw linear16 PCM.
    let mut pcm = Vec::new();
    for part in response_body
        .pointer("/candidates/0/content/parts")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if let Some(data) = part.pointer("/inlineData/data").and_then(|v| v.as_str()) {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            pcm.extend_from_slice(&bytes);
        }
    }

    if pcm.is_empty() {
        return Ok(None);
    }

    // Gemini returns raw linear16 PCM at 24kHz mono - wrap in a WAV header
    Ok(Some(wrap_pcm_in_wav(&pcm, 24000, 1, 16)))
}

async fn gemini_text_to_speech(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<TtsRequest>,
) -> Result<String, StatusCode> {
    let _claims = verify_jwt(auth.token()).await;

    synthesize_checked(&reqwest::Client::new(), &request, TtsProvider::Gemini).await
}

async fn autograde_translation(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request): Json<autograde::AutoGradeTranslationRequest>,
) -> Result<Json<autograde::AutoGradeTranslationResponse>, StatusCode> {
    // Verify JWT token
    // actually, disable authentication for now until people start abusing it:
    let _claims = verify_jwt(auth.token()).await;

    let response = autograde_core::grade_translation(&TRANSLATION_CLIENT, &request)
        .await
        .map_err(|e| match e {
            autograde_core::GradeError::Llm(msg) => {
                eprintln!("Error: {msg}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    eprintln!("Response: {response:?}");

    Ok(Json(response))
}

/// Best-effort readable IPA for a sentence (word boundaries kept — the
/// model-label tokenization exists for the pronunciation verifier, not
/// for an LLM). Pure enrichment: if the g2p crate has no G2P for the
/// language or anything fails, returns `None` and grading proceeds without
/// the IPA line. In-process and a few milliseconds, so it runs inline. Uses
/// the crate's current (corrected) Hindi labels: this is for reading, not
/// for scoring against the model.
fn readable_ipa(sentence: &str, language: Language) -> Option<String> {
    let lang = language.g2p_lang()?;
    match g2p::phonemize_lang(lang, sentence) {
        Ok(p) if !p.raw.is_empty() => Some(p.raw),
        Ok(_) => None,
        Err(e) => {
            eprintln!("readable IPA unavailable: {e:#}");
            None
        }
    }
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

    let language_specific_phonetic_note = match target_language {
        Language::Korean => {
            "The ㅐ/ㅔ distinction has collapsed in modern Seoul Korean — most speakers can't reliably distinguish them in production or perception. So if the user wrote a word that makes sense and only differs from the expected word by ㅐ/ㅔ, treat it as Perfect (or at worst PhoneticallyIdenticalButContextuallyIncorrect if the resulting word doesn't quite fit the context).\n\n"
        }
        _ => "",
    };

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

The sentence the user heard and the text they wrote may each be followed by an IPA pronunciation line. Use the IPA to judge whether a mistake is phonetically identical, merely similar, or neither. The IPA is generated by espeak and may be incorrect — especially the IPA of what the user wrote, if their submission contains typos. Either IPA line may be absent.

{language_specific_phonetic_note}You should always provide encouragement highlighting what the user did right and acknowledging their progress. If there are any errors, also provide a brief explanation focusing on where they made mistakes and how they can improve.

Respond with JSON in this format:
{{
  "encouragement": "Always provide this: warm, encouraging message highlighting what the user got right and their progress.",
  "explanation": "Only if there are errors: brief explanation of where they made mistakes and how they can improve.",
  "grades": [{{"grade": "Perfect", "wrote": "the word the user wrote"}}, {{"grade": "PhoneticallyIdenticalButContextuallyIncorrect", "wrote": "the word the user wrote"}}, {{"grade": "Missed", "wrote": null}}, ...]
}}

The grades array should have one grade for each word the user was asked to transcribe, in the order they appear. Set `wrote` to the word the user wrote for that position, or null for Missed.

The encouragement should always be provided, be in {native_language_name}, be a short positive message (1-2 sentences), and focus on what they got right. The explanation should only be provided if there are errors, be in {native_language_name}, focus on their mistakes and how to improve, and help the user learn from their errors. Markdown formatting is allowed, and encouraged for emphasis (just no bullet points or numbered lists). If the user appeared to confuse some words, you can include those words in the compare array, and a TTS example for each word will be generated for the user to hear. {}

The explanation should avoid generic advice like "try to listen more carefully" — the goal is to only write information helpful for the user to understand their mistakes. When a user confuses or misses a word, it is often helpful to break the word down into its component parts (morphemes, compound elements, prefix/suffix structure) so the user can see how simpler pieces they may already know combine into the target word. For example, if the target word was "malheureusement", you might point out that it builds from "mal" (bad) + "heur" (luck) + "euse" (french builds adverbs from the feminine adjective) + "ment" (adverb suffix). This is about practical decomposition — showing recognizable building blocks — not historical etymology. Only do this when the decomposition actually clarifies something (why the word sounds the way it does, why it's spelled that way, how to remember it). If the word is monomorphemic or the breakdown wouldn't help, skip it.

When you mention a {target_language_name} word or phrase inside the encouragement or explanation, wrap it in a <word>...</word> tag (e.g. <word>word</word>). This lets the UI style and pronounce it correctly. Do not wrap {native_language_name} text.

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
            Language::Thai =>
                r#"For example, if the user confused "ใกล้" (near) and "ไกล" (far), or "หมา" and "ม้า", you could generate ["ใกล้", "ไกล"] or ["หมา", "ม้า"] in the compare array."#,

            Language::ChineseSimplified | Language::ChineseTraditional =>
                r#"For example, if the user confused "在" and "再", or "他" and "她", you could generate ["在", "再"] or ["他", "她"] in the compare array."#,
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

    // Phonemize what the user heard and what they wrote (both target
    // language) so the LLM can judge the phonetic grades — homophones,
    // near-homophones — from actual pronunciations instead of spelling.
    // espeak applies connected-speech rules (liaison, elision) that
    // per-word pronunciation data can't.
    let heard_ipa_line = readable_ipa(&full_sentence, target_language)
        .map(|ipa| format!("User heard IPA: /{ipa}/\n"))
        .unwrap_or_default();
    let wrote_ipa_line = readable_ipa(&user_sentence, target_language)
        .map(|ipa| {
            format!(
                "User wrote IPA: /{ipa}/ (reminder: potentially incorrect if the user's submission contains typos)\n"
            )
        })
        .unwrap_or_default();

    let prompt = format!(
        r#"User heard: "{full_sentence}"
{heard_ipa_line}User saw: {sentence_shown}
User wrote: {user_sentence}
{wrote_ipa_line}
Words that need grading:
{words_to_grade}"#,
        words_to_grade = words_to_grade_list.join("\n")
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
    pub enum GradeKind {
        Perfect,
        CorrectWithTypo,
        PhoneticallyIdenticalButContextuallyIncorrect,
        PhoneticallySimilarButContextuallyIncorrect,
        Incorrect,
        Missed,
    }

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
    pub struct WordGradeResponse {
        grade: GradeKind,
        wrote: Option<String>,
    }

    impl From<WordGradeResponse> for transcription_challenge::WordGrade {
        fn from(response: WordGradeResponse) -> Self {
            let WordGradeResponse { grade, wrote } = response;
            match grade {
                GradeKind::Perfect => transcription_challenge::WordGrade::Perfect { wrote },
                GradeKind::CorrectWithTypo => transcription_challenge::WordGrade::CorrectWithTypo { wrote },
                GradeKind::PhoneticallyIdenticalButContextuallyIncorrect => transcription_challenge::WordGrade::PhoneticallyIdenticalButContextuallyIncorrect { wrote },
                GradeKind::PhoneticallySimilarButContextuallyIncorrect => transcription_challenge::WordGrade::PhoneticallySimilarButContextuallyIncorrect { wrote },
                GradeKind::Incorrect => transcription_challenge::WordGrade::Incorrect { wrote },
                GradeKind::Missed => transcription_challenge::WordGrade::Missed {},
            }
        }
    }

    // Get response from LLM
    #[derive(Deserialize, schemars::JsonSchema, Debug)]
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
    eprintln!("Response from LLM: {llm_response:?}");

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

    eprintln!("Response to user: {grade:?}");

    Ok(Json(grade))
}

// --- Pronunciation Feedback ---

#[derive(Deserialize)]
struct PronunciationFeedbackRequest {
    /// The sentence being practiced
    sentence: String,
    /// Target language
    language: Language,
    /// Base64-encoded user audio (mp3, ogg opus, or wav)
    user_audio: String,
    /// Base64-encoded reference audio (mp3, ogg opus, or wav)
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

    // Decode the audio (mp3 or ogg opus) to f32 samples at whatever sample
    // rate, send with sample_rate
    let (samples, sample_rate) = google_tts::decode_audio_to_f32(audio_bytes).map_err(|e| {
        eprintln!("Failed to decode audio: {e}");
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

/// Gemini's OpenAI-compatible endpoint only accepts "wav" and "mp3" input
/// audio, so Ogg Opus (what /tts/google returns) is re-encoded as 16-bit PCM
/// WAV. WAV (what /tts/gemini returns) passes through labeled as such; MP3 is
/// the fallback for everything else.
fn gemini_input_audio(
    audio_bytes: &[u8],
) -> Result<tysm::chat_completions::InputAudio, StatusCode> {
    use tysm::chat_completions::InputAudio;
    if audio_bytes.starts_with(b"RIFF") {
        return Ok(InputAudio::wav(audio_bytes));
    }
    if !audio_bytes.starts_with(b"OggS") {
        return Ok(InputAudio::mp3(audio_bytes));
    }
    let (samples, sample_rate) = google_tts::decode_audio_to_f32(audio_bytes).map_err(|e| {
        eprintln!("Failed to decode audio: {e}");
        StatusCode::BAD_REQUEST
    })?;
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    Ok(InputAudio::wav(wrap_pcm_in_wav(&pcm, sample_rate, 1, 16)))
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

    use tysm::chat_completions::{ChatMessage, ChatMessageContent, Role};

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
                input_audio: gemini_input_audio(&user_audio_bytes)?,
            },
            ChatMessageContent::InputAudio {
                input_audio: gemini_input_audio(&reference_audio_bytes)?,
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
    if let Some(language_data) = language_data_for_course(&request.course, request.part) {
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

fn app() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    Router::new()
        .route("/", get(|| async { "Hello from fly.io!" }))
        .route("/health", get(health))
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
        .layer(cors)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // Say out loud whether synthesized speech gets checked. The gate fails
    // open by design, so an unset secret produces no errors and no rejections
    // — exactly what a perfectly working gate produces. This is the only
    // moment the difference is visible.
    let transcribers = tts_verify::configured_transcribers();
    if transcribers.is_empty() {
        eprintln!(
            "TTS verification: OFF — neither Cloudflare (CLOUDFLARE_ACCOUNT_ID + \
             CLOUDFLARE_API_TOKEN) nor Groq (GROQ_API_KEY) is configured, \
             so audio will not be checked against its text"
        );
    } else {
        println!("TTS verification: on ({})", transcribers.join(" + "));
    }

    let app = app();

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("Listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    const TEST_SECRET: &[u8] = b"test-jwt-secret";

    fn make_token(secret: &[u8], aud: &str, exp_offset_secs: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let claims = serde_json::json!({
            "sub": uuid::Uuid::new_v4(),
            "exp": now + exp_offset_secs,
            "aud": aud,
        });
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .unwrap()
    }

    /// jsonwebtoken only fails at *runtime* when built without a crypto
    /// provider feature (`rust_crypto`/`aws_lc_rs`) — `cargo check` and clippy
    /// pass fine. This roundtrip panics, failing the test, in that case.
    #[test]
    fn jwt_roundtrip_accepts_valid_token() {
        let token = make_token(TEST_SECRET, "authenticated", 60);
        let claims =
            verify_jwt_with_secret(&token, TEST_SECRET).expect("valid token should verify");
        assert!(claims.exp > 0);
    }

    #[test]
    fn jwt_rejects_wrong_secret() {
        let token = make_token(b"some-other-secret", "authenticated", 60);
        assert!(verify_jwt_with_secret(&token, TEST_SECRET).is_err());
    }

    #[test]
    fn jwt_rejects_expired_token() {
        let token = make_token(TEST_SECRET, "authenticated", -3600);
        assert!(verify_jwt_with_secret(&token, TEST_SECRET).is_err());
    }

    #[test]
    fn jwt_rejects_wrong_audience() {
        let token = make_token(TEST_SECRET, "anon", 60);
        assert!(verify_jwt_with_secret(&token, TEST_SECRET).is_err());
    }

    /// Drive a request through the real router. The primary value is that any
    /// panic in the pre-network handler path (routing, extraction, JWT decode,
    /// lazy statics) aborts the test — the returned status is secondary.
    async fn smoke(method: &str, path: &str, body: Option<serde_json::Value>) -> StatusCode {
        unsafe {
            // Make sure handlers take the full JWT-decode path instead of
            // bailing out early on a missing secret...
            std::env::set_var(
                "SUPABASE_JWT_SECRET",
                std::str::from_utf8(TEST_SECRET).unwrap(),
            );
            // ...and that they bail before calling external providers, even on
            // machines where real API keys are exported.
            for key in [
                "ELEVENLABS_API_KEY",
                "GOOGLE_CLOUD_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
                "SUPABASE_URL",
                "SUPABASE_SERVICE_ROLE_KEY",
            ] {
                std::env::remove_var(key);
            }
        }

        let token = make_token(TEST_SECRET, "authenticated", 60);
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {token}"));
        let body = match body {
            Some(json) => {
                request = request.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&json).unwrap())
            }
            None => Body::empty(),
        };
        app()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn root_responds() {
        assert_eq!(smoke("GET", "/", None).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn health_responds_ok() {
        assert_eq!(smoke("GET", "/health", None).await, StatusCode::OK);
    }

    fn tts_request(text: &str) -> TtsRequest {
        TtsRequest {
            text: text.to_string(),
            language: Language::French,
            is_ssml: false,
            instructions: None,
            speed: 1.0,
            verification_hints: Vec::new(),
        }
    }

    #[test]
    fn fallback_starts_with_the_requested_provider() {
        for primary in [
            TtsProvider::Gemini,
            TtsProvider::ElevenLabs,
            TtsProvider::Google,
            TtsProvider::OpenAI,
        ] {
            let chain = fallback_chain(primary, &tts_request("bonjour"));
            assert_eq!(chain.first(), Some(&primary));
            // A provider must never be tried twice.
            let mut seen = chain.clone();
            seen.sort();
            seen.dedup();
            assert_eq!(
                seen.len(),
                chain.len(),
                "{primary:?} chain repeats: {chain:?}"
            );
        }
    }

    #[test]
    fn gemini_is_never_a_fallback_target() {
        // It's the provider that rewrites what it reads — falling back to it
        // would risk swapping one wrong reading for another.
        for primary in [
            TtsProvider::ElevenLabs,
            TtsProvider::Google,
            TtsProvider::OpenAI,
        ] {
            let chain = fallback_chain(primary, &tts_request("bonjour"));
            assert!(
                !chain.contains(&TtsProvider::Gemini),
                "{primary:?} fell back to Gemini: {chain:?}"
            );
        }
        // As a primary it's still tried first, then handed off to faithful ones.
        let chain = fallback_chain(TtsProvider::Gemini, &tts_request("bonjour"));
        assert_eq!(
            chain,
            vec![
                TtsProvider::Gemini,
                TtsProvider::ElevenLabs,
                TtsProvider::Google
            ]
        );
    }

    #[test]
    fn ssml_never_falls_back() {
        // Only Google interprets SSML; anyone else would read the tags aloud.
        let mut request = tts_request("<speak>bonjour</speak>");
        request.is_ssml = true;
        assert_eq!(
            fallback_chain(TtsProvider::Google, &request),
            vec![TtsProvider::Google]
        );
    }

    #[test]
    fn ssml_never_gets_a_chirp3_voice() {
        // Chirp3-HD eats the text adjacent to a `<break>` instead of erroring,
        // so this is the only thing standing between a pronunciation card and
        // audio that silently omits the letter it exists to teach. Asserted
        // across every language so adding a course can't quietly reintroduce
        // it — see `google_voice` for the measurements.
        for language in [
            Language::French,
            Language::Spanish,
            Language::English,
            Language::Korean,
            Language::German,
            Language::Italian,
            Language::Portuguese,
            Language::Russian,
            Language::Japanese,
            Language::Hindi,
            Language::ChineseSimplified,
            Language::ChineseTraditional,
            Language::Thai,
        ] {
            let (locale, voice) = google_voice(language, true);
            assert!(
                !voice.contains("Chirp"),
                "{language:?} would synthesize SSML with {voice}"
            );
            assert!(
                voice.starts_with(locale),
                "{language:?}: voice {voice} doesn't belong to locale {locale}"
            );

            // The plain-text path is the one that should keep the good voice.
            let (_, plain) = google_voice(language, false);
            assert!(plain.contains("Chirp3") || language == Language::ChineseTraditional);
        }
    }

    #[test]
    fn a_slowed_request_excludes_the_provider_with_no_rate_control() {
        let mut request = tts_request("bonjour");
        request.speed = 0.8;
        let chain = fallback_chain(TtsProvider::Gemini, &request);
        // Not merely ranked last — absent. Ordering means nothing in a race,
        // so an ElevenLabs clip could win and arrive at full speed.
        assert!(!chain.contains(&TtsProvider::ElevenLabs));
        assert!(chain.contains(&TtsProvider::Google));

        // At default speed it has nothing to drop, so it races.
        let chain = fallback_chain(TtsProvider::Gemini, &tts_request("bonjour"));
        assert!(chain.contains(&TtsProvider::ElevenLabs));
    }

    /// The best-effort pick, exercised without touching the network. Ordering
    /// is the whole point: an earlier version took the primary's first reject
    /// unconditionally, which handed back silence whenever the requested
    /// provider's first attempt was the broken one.
    fn best_effort(salvage: Vec<(Rejection, bool, &str)>) -> Option<&str> {
        salvage
            .into_iter()
            .max_by_key(|(grade, is_primary, _)| (*grade, *is_primary))
            .map(|(_, _, audio)| audio)
    }

    #[test]
    fn an_audible_clip_beats_a_silent_one_from_the_requested_provider() {
        assert_eq!(
            best_effort(vec![
                (Rejection::Defective, true, "primary-silence"),
                (Rejection::WrongWords, false, "fallback-audible"),
            ]),
            Some("fallback-audible")
        );
    }

    #[test]
    fn the_requested_provider_only_wins_ties() {
        assert_eq!(
            best_effort(vec![
                (Rejection::WrongWords, false, "fallback"),
                (Rejection::WrongWords, true, "primary"),
            ]),
            Some("primary")
        );
        // ...and with nothing to choose from at all, the caller gets an error
        // rather than an empty clip.
        assert_eq!(best_effort(vec![]), None);
    }

    #[tokio::test]
    async fn tts_endpoints_respond_without_panicking() {
        let tts_body = serde_json::to_value(TtsRequest {
            text: "bonjour".to_string(),
            language: Language::French,
            is_ssml: false,
            instructions: None,
            speed: 1.0,
            verification_hints: Vec::new(),
        })
        .unwrap();

        for path in ["/tts", "/tts/google", "/tts/openai", "/tts/gemini"] {
            // Provider API keys are stripped, so each handler verifies the JWT
            // and then errors out before reaching the network. What matters is
            // that the whole pre-network path runs without panicking.
            let status = smoke("POST", path, Some(tts_body.clone())).await;
            assert!(status.is_server_error(), "{path} returned {status}");
        }
    }

    #[tokio::test]
    async fn supabase_endpoints_respond_without_panicking() {
        // Supabase env vars are stripped, so each handler errors out before
        // reaching the network — after running JWT verification, including the
        // endpoints that enforce it.
        let nil_id = uuid::Uuid::nil().to_string();
        let gets = [
            format!("/profile?id={nil_id}"),
            format!("/user-language-stats?id={nil_id}"),
            format!("/follow-status?id={nil_id}"),
        ];
        for path in &gets {
            let status = smoke("GET", path, None).await;
            assert!(status.is_server_error(), "{path} returned {status}");
        }

        let stats_body = serde_json::to_value(UpdateLanguageStatsRequest {
            language: Language::French,
            total_count: 1,
            daily_streak: 1,
            daily_streak_expiry: None,
            xp: 1.0,
            percent_known: 0.5,
            start_time: None,
        })
        .unwrap();
        let follow_body = serde_json::to_value(FollowRequest {
            user_id: nil_id.clone(),
        })
        .unwrap();
        let posts = [
            ("/language-stats", stats_body),
            ("/follow", follow_body.clone()),
            ("/unfollow", follow_body),
        ];
        for (path, body) in posts {
            let status = smoke("POST", path, Some(body)).await;
            assert!(status.is_server_error(), "{path} returned {status}");
        }
    }

    #[tokio::test]
    async fn language_data_serves_first_chunk_of_both_parts() {
        for part in ["core", "sentences"] {
            let body = serde_json::json!({
                "course": Course {
                    native_language: Language::English,
                    target_language: Language::French,
                },
                "part": part,
                "chunk_index": 0,
                "chunk_size": 1024,
            });
            assert_eq!(
                smoke("POST", "/language-data", Some(body)).await,
                StatusCode::OK,
                "part {part}"
            );
        }
    }

    /// The jsonwebtoken outage was misreported by browsers as a CORS failure
    /// (fly's proxy 502s carry no CORS headers). Keep actual preflight
    /// behavior pinned so real CORS regressions are distinguishable.
    #[tokio::test]
    async fn cors_preflight_allows_any_origin() {
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/tts")
            .header("origin", "https://yap.town")
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "authorization,content-type",
            )
            .body(Body::empty())
            .unwrap();
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
}
