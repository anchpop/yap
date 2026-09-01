//! Subtitle track → course-keyed sentences: the one implementation that both
//! language-pack ingestion (generate-data) and the subtitle corpus's clip
//! mapping call, so a sentence is spelled and keyed identically everywhere.
//! Course ingestion keeps only `course_worthy` sentences; the clip mapper
//! takes them all (clips are useful beyond the course) and carries the flag.
//!
//! `sentence` is the pack key: segmented, trimmed, then
//! [`language_utils::text_cleanup::cleanup_sentence`] (French thin-nbsp
//! before high punctuation, whitespace normalization). `course_worthy` is
//! judged on the raw trimmed sentence, before that cleanup — the historical
//! order, kept so existing pack contents don't shift.

use std::sync::LazyLock;

use language_utils::Language;
use regex::Regex;

use crate::segment::{timed_passages, SubtitleSegmenter};
use crate::SubtitleLine;

/// One sentence of a subtitle track, with the span of the passage it came
/// from and whether course ingestion would keep it.
#[derive(Debug, Clone)]
pub struct KeyedSentence {
    /// The pack key: exactly how a language pack spells this sentence.
    pub sentence: String,
    pub start_ms: u32,
    pub end_ms: u32,
    /// Passes [`should_include_sentence`] — what the course ingests.
    pub course_worthy: bool,
}

/// Sentences of a subtitle track, keyed as the language pack keys them.
///
/// Expects cue text already through [`crate::cleanup_subtitle_text`] (both
/// [`crate::parse_srt`] and the corpus's cue parser do this). Cues shorter
/// than 3 bytes are dropped here so callers whose parsers keep them agree
/// with [`crate::parse_srt`], which drops them at parse time.
pub fn keyed_sentences(
    lines: &[SubtitleLine],
    language: Language,
    segmenter: &SubtitleSegmenter,
) -> Vec<KeyedSentence> {
    let repaired: Vec<SubtitleLine> = lines
        .iter()
        .filter(|l| l.sentence.len() >= 3)
        .map(|l| SubtitleLine {
            sentence: repair_latin_homoglyphs(&l.sentence),
            start_ms: l.start_ms,
            end_ms: l.end_ms,
        })
        .collect();
    let mut out = Vec::new();
    for passage in timed_passages(&repaired) {
        for sentence in segmenter.segment(&passage.text) {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }
            out.push(KeyedSentence {
                course_worthy: should_include_sentence(sentence, language),
                sentence: language_utils::text_cleanup::cleanup_sentence(
                    sentence.to_string(),
                    language,
                ),
                start_ms: passage.start_ms,
                end_ms: passage.end_ms,
            });
        }
    }
    out
}

/// Undo subtitle copy-protection homoglyphs: Greek/Cyrillic characters that
/// are visual twins of Latin ones, swapped into otherwise-Latin text. Only
/// applied when the confusables are rare relative to the Latin text — a
/// genuinely Greek or Cyrillic line is left alone.
pub fn repair_latin_homoglyphs(text: &str) -> String {
    let latin = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let confusable = text
        .chars()
        .filter(|c| ('\u{0370}'..='\u{03FF}').contains(c) || ('\u{0400}'..='\u{04FF}').contains(c))
        .count();
    if confusable == 0 || confusable * 3 > latin.max(1) {
        return text.to_string();
    }
    text.chars()
        .map(|c| match c {
            // Greek capitals that are visual twins of Latin ones.
            'Α' => 'A',
            'Β' => 'B',
            'Ε' => 'E',
            'Ζ' => 'Z',
            'Η' => 'H',
            'Ι' => 'I',
            'Κ' => 'K',
            'Μ' => 'M',
            'Ν' => 'N',
            'Ο' => 'O',
            'Ρ' => 'P',
            'Τ' => 'T',
            'Υ' => 'Y',
            'Χ' => 'X',
            'ο' => 'o',
            'ν' => 'v',
            'ι' => 'i',
            // Cyrillic capitals likewise.
            'А' => 'A',
            'В' => 'B',
            'Е' => 'E',
            'К' => 'K',
            'М' => 'M',
            'Н' => 'H',
            'О' => 'O',
            'Р' => 'P',
            'С' => 'C',
            'Т' => 'T',
            'Х' => 'X',
            'У' => 'Y',
            'а' => 'a',
            'е' => 'e',
            'о' => 'o',
            'с' => 'c',
            'р' => 'p',
            'х' => 'x',
            other => other,
        })
        .collect()
}

static TITLE_ABBREVIATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?P<title>M|Mme|Mlle|Mr|Mrs|Ms|Dr|St|Sr|Jr|Sra|Hr|Fr)\.(?P<name>\s+\p{Lu})")
        .unwrap()
});

/// Sentences spoken in a subtitle track.

/// Check if a single sentence should be included (for sources without translations like movies)
pub fn should_include_sentence(sentence: &str, language: Language) -> bool {
    // 1. Skip sentences that are too short or too long
    if sentence.len() < 5 || sentence.len() > 80 {
        return false;
    }

    // 2. Skip sentences ending with ellipsis
    if sentence.ends_with("...") {
        return false;
    }

    // 3. Skip sentences containing ellipsis anywhere
    if sentence.contains("...") {
        return false;
    }

    // 4. Skip music markers (common in subtitles)
    if sentence.contains('♪') {
        return false;
    }

    // 5. Check if sentence is "proper" according to language rules
    if !is_proper_sentence(sentence, language) {
        return false;
    }

    // 6. Skip sentences with multiple punctuation marks — two sentences the
    // segmenter left joined. The period of a title abbreviation (M. Godefroy,
    // Mrs. Smith) is not a sentence boundary and doesn't count.
    let without_titles = TITLE_ABBREVIATION.replace_all(sentence, "$title$name");
    // A run like `?!` or `!!!` is one mark, not several sentences.
    let punct_count = without_titles
        .chars()
        .fold((0, false), |(count, in_run), c| {
            let terminal = matches!(c, '.' | '!' | '?');
            (count + usize::from(terminal && !in_run), terminal)
        })
        .0;

    if punct_count > 1 {
        return false;
    }

    // 7. Skip sentences with numbers
    if sentence.chars().any(|c| c.is_numeric()) {
        return false;
    }

    // 8. Skip sentences with encoding corruption or garbage characters
    if has_encoding_corruption(sentence) {
        return false;
    }

    // 9. Skip ALL_CAPS sentences (shouting in subtitles)
    {
        let alpha_chars: Vec<char> = sentence.chars().filter(|c| c.is_alphabetic()).collect();
        if alpha_chars.len() >= 2 && alpha_chars.iter().all(|c| c.is_uppercase()) {
            return false;
        }
    }

    // 10. Skip sentences with malformed spacing: a comma or semicolon glued directly
    // to the next word with no space (e.g. "Vous savez,à avoir un boulot ici.").
    // Well-formed text always puts a space after these marks.
    {
        let mut chars = sentence.chars().peekable();
        while let Some(c) = chars.next() {
            if matches!(c, ',' | ';') && chars.peek().is_some_and(|n| n.is_alphabetic()) {
                return false;
            }
        }
    }

    true
}

/// Check if a sentence has encoding corruption or garbage characters that make it unusable.
pub fn has_encoding_corruption(sentence: &str) -> bool {
    // BOM character in text
    if sentence.contains('\u{FEFF}') {
        return true;
    }
    // &nbsp; HTML entity (common in corrupted subtitle files)
    if sentence.contains("&nbsp;") {
        return true;
    }
    sentence.chars().any(|c| {
        matches!(
            c,
            // Literal backslash (from corrupted subtitle escapes like \n, \h)
            '\\' |
        // @ symbol (not a real word)
        '@' |
        // MacRoman encoding artifacts (ˆ instead of à, Ž instead of é)
        '\u{02C6}' | '\u{017D}' |
        // Backtick and acute accent used as apostrophe in corrupted subtitles
        '`' | '\u{00B4}' |
        // Unicode replacement character (indicates failed decoding)
        '\u{FFFD}' |
        // Soft hyphen used incorrectly (e.g., as ¡ in Spanish OCR)
        '\u{00AD}' |
        // Zero-width space (typically from copy-paste corruption)
        '\u{200B}'
        ) || matches!(c,
            // C1 control characters indicate mojibake (e.g., U+009C instead of œ)
            '\u{0080}'..='\u{009F}' |
            // Greek letter homoglyphs mixed into Latin text (subtitle copy-protection).
            // These look identical to Latin letters but are different Unicode codepoints.
            '\u{0370}'..='\u{03FF}'
        )
    })
}

/// Does the text use an apostrophe as a quotation mark — that is, anywhere
/// other than between two letters?
pub fn has_quote_apostrophe(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        matches!(c, '\'' | '’' | '‘')
            && !(i > 0
                && chars[i - 1].is_alphabetic()
                && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()))
    })
}

/// Check if a sentence is "proper" - language-specific validation
pub fn is_proper_sentence(text: &str, language: Language) -> bool {
    if text.is_empty() {
        return false;
    }

    // Reject sentences starting with dash/hyphen
    if text.starts_with('-') || text.starts_with('—') || text.starts_with('–') {
        return false;
    }

    let first_char = text.chars().next().unwrap();
    let last_char = text.chars().last().unwrap();

    // Language-specific checks
    match language {
        Language::English
        | Language::French
        | Language::Spanish
        | Language::German
        | Language::Portuguese
        | Language::Italian => {
            // Must start with uppercase letter
            if !first_char.is_uppercase() || !first_char.is_alphabetic() {
                return false;
            }

            // Must end with period, exclamation mark, or question mark
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Russian => {
            // Russian sentences should not contain Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must start with uppercase Cyrillic letter
            if !first_char.is_uppercase() {
                return false;
            }

            // Must end with period, exclamation mark, or question mark
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::ChineseSimplified | Language::ChineseTraditional => {
            // Chinese sentences should not contain Latin letters (except maybe proper nouns)
            // But we'll be strict and reject any with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with Chinese or Western punctuation
            if last_char != '。'
                && last_char != '！'
                && last_char != '？'
                && last_char != '.'
                && last_char != '!'
                && last_char != '?'
            {
                return false;
            }
        }
        Language::Japanese => {
            // Japanese sentences should not contain Latin letters (except maybe proper nouns)
            // But we'll be strict and reject any with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with Japanese or Western punctuation
            if last_char != '。'
                && last_char != '！'
                && last_char != '？'
                && last_char != '.'
                && last_char != '!'
                && last_char != '?'
            {
                return false;
            }
        }
        Language::Korean => {
            // Korean sentences should not contain Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }

            // Must end with appropriate Korean punctuation or period/exclamation/question
            if last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Hindi => {
            // Devanagari script — reject sentences with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }
            // Must end with Devanagari danda, or Western punctuation
            if last_char != '।' && last_char != '.' && last_char != '!' && last_char != '?' {
                return false;
            }
        }
        Language::Thai => {
            // Thai script — reject sentences with Latin letters
            if text
                .chars()
                .any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            {
                return false;
            }
            // Must start with a Thai-script character. No requirement on the
            // last character: Thai does not use sentence-final punctuation.
            if !('\u{0E00}'..='\u{0E7F}').contains(&first_char) {
                return false;
            }
        }
    }

    // Reject sentences with quotation marks (often dialogue or non-standard).
    // An apostrophe *inside* a word is elision or contraction (c'est, l'ami,
    // don't) and stays; one at a word edge is a quote and goes.
    if text.contains(['"', '“', '”', '„']) || has_quote_apostrophe(text) {
        return false;
    }

    // Reject sentences with special characters that indicate non-standard text
    if text.contains('~') || text.contains('*') || text.contains('_') {
        return false;
    }

    // Reject sentences with slashes (subtitle line-break markers or other artifacts)
    if text.contains('/') || text.contains('\\') {
        return false;
    }

    // Reject sentences containing `j"` (a subtitle OCR/encoding artifact)
    if text.contains("j\"") {
        return false;
    }

    // Reject sentences with colons (often speaker attribution in subtitles)
    if text.contains(':') {
        return false;
    }

    true
}
