//! An ASR gate for synthesized speech.
//!
//! `google_tts::audio_defect` catches audio that is broken as a *signal* —
//! silent, undecodable, truncated. It cannot catch audio that is a perfectly
//! healthy recording of the wrong words, because it never sees the text. That
//! gap is not theoretical: Gemini reproducibly read the French sentence
//! "Les Baxter ici ?" as "Laisse Baxter ici" — swapping an article for a verb
//! and inverting the meaning — and sailed through every existing check, since
//! `audio_defect` returns early for any clip at or over a second.
//!
//! So we transcribe what the provider actually produced and compare it to what
//! we asked for, using `whisper-large-v3-turbo` — hosted by both Cloudflare
//! Workers AI (~$0.0005/audio-minute, so a two-second clip costs about 1.7e-5
//! dollars) and Groq (~$0.04/audio-hour, the same order of magnitude). The two
//! hosts race and the first transcript wins: same model, so the transcripts
//! agree, but measured on the same clip Cloudflare took 0.8–4.3s per call
//! while Groq took 0.3–0.9s. Racing rather than preferring Groq means a Groq
//! outage costs nothing — the race degrades to exactly the old behaviour.
//!
//! Three design points worth keeping:
//!
//! 1. **Compare with punctuation stripped.** Whisper's punctuation is its own
//!    invention — across our fixtures it variously dropped a period, added a
//!    question mark, and added a comma, all on correct audio. Comparing raw
//!    strings would reject good clips constantly.
//! 2. **Condition on proper nouns, never on the full text.** Whisper's
//!    `initial_prompt` supplies vocabulary without dictating the transcript,
//!    so a name like "Baxter" stops reading as a mispronunciation while a
//!    genuinely wrong article still comes back wrong. Its sibling parameter
//!    `prefix` would be actively harmful — it forces the output to *start*
//!    with the text you give it, which would paper over precisely the
//!    sentence-initial error that motivated this module.
//! 3. **Fail open.** A verifier that is down, rate-limited, or unconfigured
//!    must never cost a learner their audio. Every uncertain path returns
//!    "no defect".

use language_utils::{Language, TtsRequest};

/// Cloudflare's Whisper. `large-v3-turbo` is the variant that exposes
/// `initial_prompt`, and it reads proper nouns markedly better than OpenAI's
/// hosted `whisper-1` — on our fixture it transcribed "Baxter" correctly with
/// no conditioning at all, where `whisper-1` produced "backstairs".
const WHISPER_MODEL: &str = "@cf/openai/whisper-large-v3-turbo";

/// Languages where a punctuation-stripped transcript match is a trustworthy
/// pass/fail signal.
///
/// Deliberately conservative. The excluded languages aren't excluded because
/// Whisper can't read them — it's that comparing its output to our text
/// orthographically isn't meaningful there. Japanese may come back in kana
/// where the pack has kanji, Chinese may switch script, and Thai has no word
/// spacing to normalize. Each needs its own comparison (phoneme distance is
/// the obvious candidate) and its own calibration run before it can be
/// gated, so until then they synthesize exactly as they did before.
///
/// Returns the ISO code Whisper wants for the language.
fn whisper_language(language: Language) -> Option<&'static str> {
    match language {
        Language::French => Some("fr"),
        Language::Spanish => Some("es"),
        Language::German => Some("de"),
        Language::Italian => Some("it"),
        Language::Portuguese => Some("pt"),
        // Not yet calibrated — see the note above. English and Russian are
        // plausible next additions; the CJK/Thai courses need a different
        // comparison entirely.
        Language::English
        | Language::Russian
        | Language::Korean
        | Language::Japanese
        | Language::Hindi
        | Language::Thai
        | Language::ChineseSimplified
        | Language::ChineseTraditional => None,
    }
}

/// Collapse a string to the part of it a transcript can be held to: lowercase
/// alphanumerics and single spaces. Everything else — punctuation, the
/// narrow no-break spaces French puts before `?`, stray quotes — becomes a
/// separator, because none of it is audible and none of it is stable.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

/// Whether this request can be meaningfully checked at all.
///
/// SSML is excluded because the "text" is markup — a transcript of
/// `<say-as interpret-as="characters">` audio has no reason to resemble the
/// tags that produced it. Text containing digits is excluded because TTS
/// speaks "1964" as "mille neuf cent soixante-quatre" while our text keeps
/// the numerals, so every such clip would fail the comparison and burn its
/// full retry budget for nothing.
///
/// Isolated single words are excluded for a deeper reason: which homophone was
/// *spelled* is not recoverable from the audio, by anyone. French "verre",
/// "vert" and "vers" are all /vɛʁ/, so Whisper can only fall back on raw word
/// frequency and returns whichever is commonest. Measured on correct Google
/// clips of isolated words, it heard "verre" as "vert", "maire" as "mère", and
/// both "foie" and "foi" as "fois" — four rejections out of seven words, every
/// one of them audio that was perfectly right.
///
/// A single function word of context is enough to fix all four: "un verre",
/// "le maire", "le foie" and "la foi" each came back exactly. So the line is
/// drawn precisely where the evidence puts it — at two words. Dictionary and
/// single-gram audio goes unchecked, which is the honest outcome, since for
/// those the check was never measuring pronunciation in the first place.
fn is_checkable(request: &TtsRequest) -> bool {
    !request.is_ssml
        && !request.text.chars().any(|c| c.is_numeric())
        && normalize(&request.text).split_whitespace().count() >= 2
}

/// The Cloudflare credentials the gate runs on, if both are present.
fn cloudflare_credentials() -> Option<(String, String)> {
    let account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID").ok()?;
    let api_token = std::env::var("CLOUDFLARE_API_TOKEN").ok()?;
    Some((account_id, api_token))
}

/// The Groq credential, if present.
fn groq_api_key() -> Option<String> {
    std::env::var("GROQ_API_KEY").ok()
}

/// The transcription hosts that can actually run, by name — for the boot log.
///
/// Failing open means an unconfigured gate is *silent* — every clip passes and
/// nothing distinguishes that from every clip being correct. So the one place
/// that must not fail open is startup: a missing secret is a deploy mistake,
/// and it should be a line in the boot log, not a mystery six weeks later.
pub fn configured_transcribers() -> Vec<&'static str> {
    let mut hosts = Vec::new();
    if cloudflare_credentials().is_some() {
        hosts.push("cloudflare");
    }
    if groq_api_key().is_some() {
        hosts.push("groq");
    }
    hosts
}

/// Transcribe `audio` with Whisper, conditioned on the request's proper
/// nouns. Races every configured host and returns the first transcript to
/// arrive; a host that errors just cedes the race to the other. `None` only
/// when no host produced anything — see the fail-open rule.
async fn transcribe(
    http: &reqwest::Client,
    request: &TtsRequest,
    audio: &[u8],
    language: &str,
) -> Option<String> {
    let cloudflare = transcribe_cloudflare(http, request, audio, language);
    let groq = transcribe_groq(http, request, audio, language);
    tokio::pin!(cloudflare, groq);
    // An unconfigured host resolves to None immediately, so its arm just
    // hands the race to the other — no special-casing per configuration.
    tokio::select! {
        first = &mut cloudflare => match first {
            Some(transcript) => Some(transcript),
            None => groq.await,
        },
        first = &mut groq => match first {
            Some(transcript) => Some(transcript),
            None => cloudflare.await,
        },
    }
}

/// One transcription attempt against Cloudflare Workers AI.
async fn transcribe_cloudflare(
    http: &reqwest::Client,
    request: &TtsRequest,
    audio: &[u8],
    language: &str,
) -> Option<String> {
    use base64::Engine;

    let (account_id, api_token) = cloudflare_credentials()?;

    let mut body = serde_json::json!({
        "audio": base64::engine::general_purpose::STANDARD.encode(audio),
        "task": "transcribe",
        "language": language,
        // Single isolated utterances: there is no previous text worth
        // conditioning on, and leaving it enabled invites hallucination loops.
        "condition_on_previous_text": false,
    });

    if !request.verification_hints.is_empty() {
        body["initial_prompt"] = serde_json::Value::String(request.verification_hints.join(", "));
    }

    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{WHISPER_MODEL}"
    );

    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {api_token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| eprintln!("TTS verify: Whisper request failed: {e}"))
        .ok()?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        eprintln!("TTS verify: Whisper returned {status}: {detail}");
        return None;
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|e| eprintln!("TTS verify: Whisper response was not JSON: {e}"))
        .ok()?;

    payload
        .pointer("/result/text")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Groq's hosting of the same model, behind an OpenAI-compatible endpoint.
const GROQ_WHISPER_MODEL: &str = "whisper-large-v3-turbo";

/// One transcription attempt against Groq.
async fn transcribe_groq(
    http: &reqwest::Client,
    request: &TtsRequest,
    audio: &[u8],
    language: &str,
) -> Option<String> {
    let api_key = groq_api_key()?;

    // Groq sniffs the container from the part's filename extension, so it has
    // to match the bytes: Google returns Ogg Opus, Gemini WAV, the rest MP3.
    let filename = if audio.starts_with(b"RIFF") {
        "audio.wav"
    } else if audio.starts_with(b"OggS") {
        "audio.ogg"
    } else {
        "audio.mp3"
    };

    let mut form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(audio.to_vec()).file_name(filename),
        )
        .text("model", GROQ_WHISPER_MODEL)
        .text("language", language.to_owned())
        .text("response_format", "json");

    // Groq's `prompt` conditions like Cloudflare's `initial_prompt`:
    // vocabulary, not a transcript to parrot.
    if !request.verification_hints.is_empty() {
        form = form.text("prompt", request.verification_hints.join(", "));
    }

    let response = http
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| eprintln!("TTS verify: Groq request failed: {e}"))
        .ok()?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        eprintln!("TTS verify: Groq returned {status}: {detail}");
        return None;
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|e| eprintln!("TTS verify: Groq response was not JSON: {e}"))
        .ok()?;

    payload
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// `Some(reason)` when the audio demonstrably says something other than
/// `request.text`, mirroring `google_tts::audio_defect`'s shape so both
/// checks read the same at the call site.
///
/// This detects gross substitutions — a swapped word, a dropped clause — not
/// subtle mispronunciation. Whisper's language model will happily snap a
/// slightly-off vowel back onto the expected word. The failure mode it does
/// catch is the one that actually teaches a learner something false.
pub async fn content_defect(
    http: &reqwest::Client,
    request: &TtsRequest,
    audio: &[u8],
) -> Option<String> {
    if !is_checkable(request) {
        return None;
    }
    let language = whisper_language(request.language)?;
    let transcript = transcribe(http, request, audio, language).await?;

    let expected = normalize(&request.text);
    let heard = normalize(&transcript);
    if heard == expected {
        return None;
    }
    Some(format!("expected {expected:?}, heard {heard:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str, language: Language) -> TtsRequest {
        TtsRequest {
            text: text.to_string(),
            language,
            is_ssml: false,
            instructions: None,
            speed: 1.0,
            verification_hints: Vec::new(),
        }
    }

    #[test]
    fn normalize_strips_the_punctuation_whisper_invents() {
        // Every one of these is a real transcript shape we observed for
        // audio that was correct.
        let expected = normalize("Les Baxter ici ?");
        assert_eq!(normalize("Les Baxter ici"), expected);
        assert_eq!(normalize("Les Baxter ici."), expected);
        assert_eq!(normalize("les baxter, ici!"), expected);
        // French's narrow no-break space before '?' must not survive either.
        assert_eq!(normalize("Les Baxter ici\u{202f}?"), expected);
    }

    #[test]
    fn normalize_keeps_a_real_word_substitution_visible() {
        assert_ne!(
            normalize("Laisse Baxter ici."),
            normalize("Les Baxter ici ?")
        );
    }

    #[test]
    fn normalize_does_not_run_words_together() {
        assert_eq!(normalize("c'était fatigant"), "c était fatigant");
    }

    #[test]
    fn ssml_and_numerals_are_not_checkable() {
        let mut ssml = request("<speak>bonjour</speak>", Language::French);
        ssml.is_ssml = true;
        assert!(!is_checkable(&ssml));

        // TTS says "mille neuf cent soixante-quatre"; our text says "1964".
        assert!(!is_checkable(&request(
            "Nous sommes en 1964.",
            Language::French
        )));

        // Nothing to compare against.
        assert!(!is_checkable(&request("...", Language::French)));

        assert!(is_checkable(&request("Les Baxter ici ?", Language::French)));
    }

    #[test]
    fn isolated_words_are_not_checkable_but_two_words_are() {
        // Measured: correct Google clips of these came back as "vert",
        // "mère" and "fois" respectively. Which homophone was spelled simply
        // isn't in the audio, so checking one would reject good pronunciation.
        for word in ["verre", "maire", "foie", "foi"] {
            assert!(!is_checkable(&request(word, Language::French)));
        }

        // One function word of context was enough to fix every one of them.
        for phrase in ["un verre", "le maire", "le foie", "la foi"] {
            assert!(is_checkable(&request(phrase, Language::French)));
        }

        // Punctuation is not a word — this is still one.
        assert!(!is_checkable(&request("Bonjour !", Language::French)));
    }

    #[test]
    fn only_calibrated_languages_are_gated() {
        assert_eq!(whisper_language(Language::French), Some("fr"));
        assert_eq!(whisper_language(Language::Portuguese), Some("pt"));
        // Orthography comparison isn't meaningful here yet.
        assert_eq!(whisper_language(Language::Japanese), None);
        assert_eq!(whisper_language(Language::ChineseSimplified), None);
        assert_eq!(whisper_language(Language::Thai), None);
    }
}
