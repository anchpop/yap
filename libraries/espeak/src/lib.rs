//! Phrase-level IPA via `espeak-ng`.
//!
//! Why not just wikipron word-by-word? Wikipron stores per-word
//! pronunciations including some liaison variants, but it can't capture
//! *which* liaison happens in a given phrase (e.g. `on est` is
//! `/ɔ̃ n ɛ/` in connected speech, but neither wikipron's `/ɔ̃/` for
//! "on" nor its `/ɔ . n ‿/` decomposed-liaison variant matches what
//! native speakers actually produce). espeak-ng's `--ipa` mode applies
//! standard phonological rules of the target language, including
//! liaison, elision, and reduction, so its output is closer to what an
//! ASR model will actually transcribe from connected speech audio.
//!
//! Two entry points, for two different consumers:
//!
//! * [`phonemize_phrase`] — per-phoneme tokenization matching the
//!   wav2vec2 model's training labels, for the pronunciation verifier.
//! * [`phonemize_phrase_ipa`] — espeak's standard readable IPA (word
//!   boundaries and stress intact), for showing an LLM. Async, with a
//!   timeout, so it's safe on the backend's request path.
//!
//! **`phonemize_phrase` tokenization must match the model's training
//! preprocessing.** The wav2vec2 phoneme model was trained on labels
//! produced by the lexide pronunciation pipeline
//! (`pronunciation/train/scripts/preprocess.py` and `relabel-french`),
//! which invokes espeak as `-q --ipa -x` (no `--sep`) and parses the
//! continuous output character-by-character: stress markers and word
//! boundaries are dropped, vowel-continuation diacritics (length,
//! nasalization, etc.) are appended to the preceding phoneme so
//! combined forms like `iː`/`ɛ̃` match the tokenizer's precomposed
//! vocab, and every other char becomes its own phoneme. We replicate
//! that exactly here so the ground-truth phoneme sequence segments the
//! same way the model's output does — notably, diphthongs like `aɪ`
//! come out as two phonemes (`a`, `ɪ`), matching the model rather than
//! espeak's `--sep` single-token form.

use anyhow::{Context, Result};
use language_utils::Language;
use std::process::Command;
use std::time::Duration;

// Phoneme-class tables, copied verbatim from the lexide pronunciation
// preprocessing pipeline so our tokenization matches the model's labels.
// (lexide's `ESPEAK_VOWELS` table is omitted: it only distinguishes
// stress-bearing vowel nuclei, and we discard stress, so vowels and
// consonants are tokenized identically — each its own phoneme.)
//
/// Combining diacritics that continue the preceding phoneme (length,
/// nasalization, retraction, etc.). Emitted standalone they'd be UNK and
/// silently strip detail from the label, so they attach to the prior char.
/// Mirrors the lexide preprocessing pipeline's continuation table —
/// includes length marks, the standard articulatory diacritics, and the
/// pharyngealization modifier letter `ˤ`.
const VOWEL_CONTINUATIONS: &str = "ːˑ̠̞̯̥̪̩̝̃̊̈ˤ";
const WORD_BOUNDARIES: &str = " \t\n|_-";

/// Parse espeak-ng `-q --ipa -x` output into a phoneme sequence, mirroring
/// the lexide preprocessing tokenizer (minus stress tracking, which we
/// don't use here). Stress markers and word boundaries are dropped;
/// continuation diacritics attach to the previous phoneme; everything else
/// is its own phoneme.
fn parse_espeak_ipa(raw: &str) -> Vec<String> {
    let mut phonemes: Vec<String> = Vec::new();
    for ch in raw.chars() {
        if ch == 'ˈ' || ch == 'ˌ' || WORD_BOUNDARIES.contains(ch) {
            // Stress markers and word boundaries are not emitted.
        } else if VOWEL_CONTINUATIONS.contains(ch) {
            match phonemes.last_mut() {
                Some(last) => last.push(ch),
                // Stray diacritic at the start with nothing to attach to.
                None => phonemes.push(ch.to_string()),
            }
        } else {
            // Vowel or consonant — its own phoneme.
            phonemes.push(ch.to_string());
        }
    }
    phonemes
}

/// Bound on a single espeak-ng invocation in [`phonemize_phrase_ipa`].
/// Normal runs finish in <10 ms; this only exists so a wedged binary or
/// pathological input can't hang a backend request forever.
const ESPEAK_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve the espeak-ng binary. Prefer ESPEAK_NG_BIN (the lexide
/// preprocessing convention); fall back to ESPEAK_NG_BINARY for callers
/// who already adopted the longer name.
fn espeak_binary() -> String {
    std::env::var("ESPEAK_NG_BIN")
        .or_else(|_| std::env::var("ESPEAK_NG_BINARY"))
        .unwrap_or_else(|_| "espeak-ng".to_string())
}

/// Build the espeak-ng invocation shared by both entry points.
///
/// `-q` suppresses audio synthesis; `--ipa -x` writes IPA phonemes to
/// stdout. No `--sep` — [`phonemize_phrase`] tokenizes the continuous
/// output itself in `parse_espeak_ipa` to match the model's training
/// preprocessing, and [`phonemize_phrase_ipa`] wants it verbatim.
fn espeak_command(binary: &str, code: &str, text: &str) -> Command {
    let mut cmd = Command::new(binary);
    if let Ok(p) = std::env::var("ESPEAK_NG_DATA_PATH") {
        cmd.arg(format!("--path={p}"));
    }
    cmd.args(["-v", code, "-q", "--ipa", "-x", text]);
    cmd
}

/// Extract stdout from a finished espeak-ng invocation, turning a non-zero
/// exit into a diagnosable error.
fn espeak_stdout(text: &str, output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let data_hint = if std::env::var("ESPEAK_NG_DATA_PATH").is_err() {
            " (set ESPEAK_NG_DATA_PATH=<parent of espeak-ng-data> if using a custom build)"
        } else {
            ""
        };
        anyhow::bail!(
            "espeak-ng exited with status {} for {text:?}: {stderr}{data_hint}",
            output.status
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn invoke_failure_context(binary: &str) -> String {
    format!(
        "Failed to invoke {binary:?} — is it installed? \
         (set ESPEAK_NG_BIN to point at a custom build)"
    )
}

/// Phonemize a single phrase using espeak-ng. Returns the per-token IPA
/// sequence, tokenized identically to the model's training labels (see
/// module docs): stress markers dropped, continuation diacritics attached
/// to the preceding phoneme. Call sites further fold length etc. via
/// [`normalize_phoneme`].
///
/// Returns `Ok(None)` if `language` has no [`Language::espeak_code`] —
/// caller should fall back to wikipron word-by-word.
///
/// Returns `Err(_)` only on espeak-ng failure (missing binary, segfault,
/// non-zero exit). An empty output is `Ok(Some(vec![]))`.
///
/// **Environment overrides:**
/// - `ESPEAK_NG_BIN` (preferred, matches the lexide training pipeline) /
///   `ESPEAK_NG_BINARY` (alias) — full path to the espeak-ng binary.
///   Defaults to `espeak-ng` (resolved via PATH). Useful for pointing at
///   a custom build (e.g. one with French phrase-level liaison/stress
///   patches).
/// - `ESPEAK_NG_DATA_PATH` — directory containing the `espeak-ng-data`
///   subdirectory. Passed as `--path=…` when set. Required for custom
///   builds that don't install data to `/usr/local/share/espeak-ng-data`.
pub fn phonemize_phrase(text: &str, language: Language) -> Result<Option<Vec<String>>> {
    let Some(code) = language.espeak_code() else {
        return Ok(None);
    };

    let binary = espeak_binary();
    let output = espeak_command(&binary, code, text)
        .output()
        .with_context(|| invoke_failure_context(&binary))?;
    let stdout = espeak_stdout(text, output)?;
    Ok(Some(parse_espeak_ipa(&stdout)))
}

/// Phonemize a single phrase into espeak-ng's standard readable IPA, word
/// boundaries and stress markers intact (e.g. `ɔ̃ nˈɛ` rather than the
/// model-label tokenization of [`phonemize_phrase`]). This is the form to
/// show an LLM or a human; the per-phoneme tokenization exists only to
/// match wav2vec2 training labels in the pronunciation verifier.
///
/// Async and bounded: the subprocess runs via `tokio::process` (no
/// executor thread is blocked) and is killed after [`ESPEAK_TIMEOUT`].
///
/// Same `Ok(None)` / `Err(_)` contract and environment overrides as
/// [`phonemize_phrase`]. An empty output is `Ok(Some(String::new()))`.
pub async fn phonemize_phrase_ipa(text: &str, language: Language) -> Result<Option<String>> {
    let Some(code) = language.espeak_code() else {
        return Ok(None);
    };

    let binary = espeak_binary();
    let mut cmd = tokio::process::Command::from(espeak_command(&binary, code, text));
    // Ensure the child is reaped if the timeout (or the caller) drops us.
    cmd.kill_on_drop(true);

    let output = tokio::time::timeout(ESPEAK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("espeak-ng timed out after {ESPEAK_TIMEOUT:?} for {text:?}"))?
        .with_context(|| invoke_failure_context(&binary))?;
    let stdout = espeak_stdout(text, output)?;

    // espeak emits one line per clause; collapse all whitespace runs to
    // single spaces so the result reads as one phrase.
    Ok(Some(
        stdout.split_whitespace().collect::<Vec<_>>().join(" "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure string test — no espeak binary needed, so this runs in CI and
    // locks in the model-label tokenization that the ignored
    // binary-dependent tests can't.
    #[test]
    fn parses_raw_espeak_output_like_the_model_labels() {
        // Stress markers and word boundaries are dropped; the combining
        // nasalization tilde attaches to the preceding vowel.
        assert_eq!(parse_espeak_ipa("ˈɔ̃ n ɛ"), vec!["ɔ̃", "n", "ɛ"]);
        // Length marks attach; diphthongs split into two phonemes
        // (matching the model, unlike espeak's --sep form).
        assert_eq!(parse_espeak_ipa("sˈiː aɪ"), vec!["s", "iː", "a", "ɪ"]);
        // Newlines between clauses are word boundaries too.
        assert_eq!(parse_espeak_ipa("wˌi\nɡˈoʊ"), vec!["w", "i", "ɡ", "o", "ʊ"]);
        // A stray leading diacritic has nothing to attach to and stands
        // alone rather than panicking.
        assert_eq!(parse_espeak_ipa("ːa"), vec!["ː", "a"]);
    }

    // Ignored in CI: requires the espeak-ng binary (and the liaison output
    // depends on our custom French-stress-liaison build). Run locally with
    // ESPEAK_NG_BIN set: `cargo test -p espeak -- --ignored`.
    #[test]
    #[ignore = "requires espeak-ng binary (custom French-liaison build)"]
    fn french_handles_liaison() {
        // The motivating case: "on est" should produce /ɔ̃ n ɛ/ with
        // both the nasal vowel preserved and the liaison /n/ — neither
        // of the wikipron per-word variants captures this connected
        // form.
        let phonemes = phonemize_phrase("on est", Language::French)
            .expect("espeak invocation failed")
            .expect("French has espeak support");
        // Strip stress so the test is comparing actual phonemes.
        let bare: Vec<String> = phonemes
            .iter()
            .map(|p| p.replace(['ˈ', 'ˌ'], ""))
            .filter(|s| !s.is_empty())
            .collect();
        assert!(
            bare.contains(&"ɔ̃".to_string()),
            "expected nasal vowel preserved, got {bare:?}"
        );
        assert!(
            bare.contains(&"n".to_string()),
            "expected liaison /n/, got {bare:?}"
        );
        assert!(
            bare.contains(&"ɛ".to_string()),
            "expected /ɛ/, got {bare:?}"
        );
    }

    #[test]
    fn unsupported_language_returns_none() {
        // Korean is currently marked unsupported (until we validate
        // espeak's Korean output against ground truth).
        let result =
            phonemize_phrase("안녕하세요", Language::Korean).expect("espeak invocation failed");
        assert!(result.is_none());
    }

    // Ignored in CI: requires the espeak-ng binary. Run with `--ignored`.
    #[tokio::test]
    #[ignore = "requires espeak-ng binary (custom French-liaison build)"]
    async fn readable_ipa_keeps_word_boundaries() {
        let ipa = phonemize_phrase_ipa("on est", Language::French)
            .await
            .expect("espeak invocation failed")
            .expect("French has espeak support");
        assert!(
            ipa.contains(' '),
            "expected word boundaries preserved, got {ipa:?}"
        );
        assert!(!ipa.contains('\n'), "expected single line, got {ipa:?}");
    }

    // Ignored in CI: requires the espeak-ng binary. Run with `--ignored`.
    #[test]
    #[ignore = "requires espeak-ng binary"]
    fn empty_input_is_ok() {
        let phonemes = phonemize_phrase("", Language::French)
            .expect("espeak invocation failed")
            .expect("French has espeak support");
        // Either empty or just a tail whitespace token after split — we
        // accept either; the contract is "no error, not None".
        assert!(phonemes.is_empty() || phonemes.iter().all(|t| t.is_empty()));
    }
}
