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
//! **Tokenization must match the model's training preprocessing.** The
//! wav2vec2 phoneme model was trained on labels produced by the lexide
//! pronunciation pipeline (`pronunciation/train/scripts/preprocess.py`
//! and `relabel-french`), which invokes espeak as `-q --ipa -x` (no
//! `--sep`) and parses the continuous output character-by-character:
//! stress markers and word boundaries are dropped, vowel-continuation
//! diacritics (length, nasalization, etc.) are appended to the
//! preceding phoneme so combined forms like `iː`/`ɛ̃` match the
//! tokenizer's precomposed vocab, and every other char becomes its own
//! phoneme. We replicate that exactly here so the ground-truth phoneme
//! sequence segments the same way the model's output does — notably,
//! diphthongs like `aɪ` come out as two phonemes (`a`, `ɪ`), matching
//! the model rather than espeak's `--sep` single-token form.

use anyhow::{Context, Result};
use language_utils::Language;
use std::process::Command;

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

    // Prefer ESPEAK_NG_BIN (the lexide preprocessing convention); fall back
    // to ESPEAK_NG_BINARY for callers who already adopted the longer name.
    let binary = std::env::var("ESPEAK_NG_BIN")
        .or_else(|_| std::env::var("ESPEAK_NG_BINARY"))
        .unwrap_or_else(|_| "espeak-ng".to_string());
    let data_path = std::env::var("ESPEAK_NG_DATA_PATH").ok();

    // `-q` suppresses audio synthesis; `--ipa -x` writes IPA phonemes to
    // stdout. No `--sep` — we tokenize the continuous output ourselves in
    // `parse_espeak_ipa` to match the model's training preprocessing.
    let mut cmd = Command::new(&binary);
    if let Some(p) = &data_path {
        cmd.arg(format!("--path={p}"));
    }
    cmd.args(["-v", code, "-q", "--ipa", "-x", text]);

    let output = cmd.output().with_context(|| {
        format!(
            "Failed to invoke {binary:?} — is it installed? \
             (set ESPEAK_NG_BIN to point at a custom build)"
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let data_hint = if data_path.is_none() {
            " (set ESPEAK_NG_DATA_PATH=<parent of espeak-ng-data> if using a custom build)"
        } else {
            ""
        };
        anyhow::bail!(
            "espeak-ng exited with status {} for {text:?}: {stderr}{data_hint}",
            output.status
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(Some(parse_espeak_ipa(&stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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

    #[test]
    fn empty_input_is_ok() {
        let phonemes = phonemize_phrase("", Language::French)
            .expect("espeak invocation failed")
            .expect("French has espeak support");
        // Either empty or just a tail whitespace token after split — we
        // accept either; the contract is "no error, not None".
        assert!(phonemes.is_empty() || phonemes.iter().all(|t| t.is_empty()));
    }
}
