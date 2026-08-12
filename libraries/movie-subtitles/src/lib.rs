//! Movie subtitle text: the `.srt` OpenSubtitles served us, and the cleaned
//! dialogue lines derived from it.
//!
//! **The raw SRT is the source of truth.** Cleaning is lossy — it strips
//! markup, sound cues, speaker labels and sub-second timing detail — so it
//! runs at *load* time, in memory, and never overwrites the original. That
//! way a change to [`cleanup_subtitle_text`] improves every course on the next
//! build instead of requiring a re-download that OpenSubtitles quota may not
//! permit.
//!
//! `subtitles/<imdb>.jsonl` stays on disk as a derived cache, and it is also
//! the *only* surviving copy for the movies downloaded before the raw file was
//! kept — [`load`] prefers the raw SRT and falls back to it.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// One cleaned line of dialogue with its position in the film.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleLine {
    pub sentence: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// Which file a movie's dialogue was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The original SRT, cleaned in memory just now.
    RawSrt,
    /// The pre-cleaned JSONL, for movies whose raw SRT was not kept.
    DerivedJsonl,
}

/// Where the untouched SRT for `imdb_id` lives under a `sentence-sources/movies` dir.
pub fn raw_srt_path(movies_dir: &Path, imdb_id: &str) -> PathBuf {
    movies_dir.join(format!("subtitles-raw/{imdb_id}.srt"))
}

/// Where the derived, pre-cleaned JSONL for `imdb_id` lives.
pub fn derived_jsonl_path(movies_dir: &Path, imdb_id: &str) -> PathBuf {
    movies_dir.join(format!("subtitles/{imdb_id}.jsonl"))
}

/// Cleaned dialogue for one movie, preferring the raw SRT.
///
/// `Ok(None)` means neither file exists. A raw SRT that fails to parse is an
/// error rather than a silent fallback: the JSONL beside it was derived from
/// *some* SRT, and quietly serving that instead would hide a corrupt file.
pub fn load(movies_dir: &Path, imdb_id: &str) -> Result<Option<(Vec<SubtitleLine>, Source)>> {
    let raw = raw_srt_path(movies_dir, imdb_id);
    if raw.exists() {
        let srt = std::fs::read_to_string(&raw)
            .with_context(|| format!("Failed to read raw subtitle {}", raw.display()))?;
        let lines = parse_srt(&srt)
            .with_context(|| format!("Failed to parse raw subtitle {}", raw.display()))?;
        return Ok(Some((lines, Source::RawSrt)));
    }

    let derived = derived_jsonl_path(movies_dir, imdb_id);
    if !derived.exists() {
        return Ok(None);
    }
    let lines = read_derived_jsonl(&derived)?;
    Ok(Some((lines, Source::DerivedJsonl)))
}

/// Read a derived JSONL file back into subtitle lines.
pub fn read_derived_jsonl(path: &Path) -> Result<Vec<SubtitleLine>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read subtitle file {}", path.display()))?;
    let mut lines = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        lines.push(
            serde_json::from_str(line)
                .with_context(|| format!("Failed to parse subtitle line in {}", path.display()))?,
        );
    }
    Ok(lines)
}

/// Write the derived JSONL cache, atomically.
///
/// Written to a sibling temp file and renamed, so an interrupted run leaves the
/// previous cache intact rather than a half-written one that parses fine and is
/// silently short.
pub fn write_derived_jsonl(path: &Path, lines: &[SubtitleLine]) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("Failed to create {}", tmp.display()))?;
        for line in lines {
            serde_json::to_writer(&mut file, line)?;
            writeln!(&mut file)?;
        }
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to move {} into place", tmp.display()))?;
    Ok(())
}

/// Save an untouched SRT as the source of truth for `imdb_id`.
pub fn write_raw_srt(movies_dir: &Path, imdb_id: &str, srt: &str) -> Result<PathBuf> {
    let path = raw_srt_path(movies_dir, imdb_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, srt).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

/// Parse SRT content into cleaned dialogue lines.
pub fn parse_srt(srt_content: &str) -> Result<Vec<SubtitleLine>> {
    use subparse::SubtitleFormat;

    let subtitle_file = subparse::parse_str(
        SubtitleFormat::SubRip,
        srt_content,
        25.0, // fps (not used for SRT but required parameter)
    )
    .map_err(|e| anyhow!("Failed to parse SRT: {e:?}"))?;

    let mut lines = Vec::new();

    for entry in subtitle_file
        .get_subtitle_entries()
        .map_err(|e| anyhow!("Failed to get subtitle entries: {e:?}"))?
    {
        let text = match &entry.line {
            Some(line) => cleanup_subtitle_text(line),
            None => continue,
        };

        // Skip empty lines or very short lines
        if text.len() < 3 {
            continue;
        }

        lines.push(SubtitleLine {
            sentence: text,
            start_ms: entry.timespan.start.msecs().max(0) as u32,
            end_ms: entry.timespan.end.msecs().max(0) as u32,
        });
    }

    Ok(lines)
}

static SSA_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\\[^}]*\}").unwrap());
static HTML_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[.*?\]").unwrap());
static PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(.*?\)").unwrap());
static SPEAKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z\s]+:\s*").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Strip markup, sound cues and speaker labels from one subtitle block.
///
/// Lossy by design, which is exactly why it is not applied before writing to
/// disk — see the module docs.
pub fn cleanup_subtitle_text(text: &str) -> String {
    let mut result = strip_html_tags(text);

    // SSA/ASS override tags ({\an8}, {\i1}, {\pos(200,100)}, …) and the ASS
    // escapes for line break / hard space.
    result = SSA_TAGS.replace_all(&result, "").to_string();
    result = result.replace("\\N", " ").replace("\\h", " ");

    // Hearing-impaired annotations, then any remaining bracketed or
    // parenthesised aside, then "JOHN:"-style speaker labels.
    result = BRACKETS.replace_all(&result, "").to_string();
    result = PARENS.replace_all(&result, "").to_string();
    result = SPEAKER.replace_all(&result, "").to_string();

    result = SPACES.replace_all(result.trim(), " ").to_string();
    result
}

/// Strip HTML tags from text
pub fn strip_html_tags(text: &str) -> String {
    HTML_TAGS.replace_all(text, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_strips_markup_but_keeps_dialogue() {
        assert_eq!(cleanup_subtitle_text("<i>Hello there</i>"), "Hello there");
        assert_eq!(cleanup_subtitle_text("{\\an8}Up here"), "Up here");
        assert_eq!(cleanup_subtitle_text("[DOOR SLAMS] Get out"), "Get out");
        assert_eq!(cleanup_subtitle_text("(sighs) Fine"), "Fine");
        assert_eq!(cleanup_subtitle_text("JOHN: Fine"), "Fine");
        assert_eq!(cleanup_subtitle_text("one\\Ntwo"), "one two");
    }

    #[test]
    fn parse_srt_keeps_millisecond_timings() {
        let srt = "1\n00:00:01,500 --> 00:00:03,250\nHello there\n\n";
        let lines = parse_srt(srt).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].sentence, "Hello there");
        assert_eq!(lines[0].start_ms, 1500);
        assert_eq!(lines[0].end_ms, 3250);
    }

    #[test]
    fn parse_srt_drops_blocks_that_clean_away_to_nothing() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n[MUSIC]\n\n\
                   2\n00:00:03,000 --> 00:00:04,000\nReal line\n\n";
        let lines = parse_srt(srt).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].sentence, "Real line");
    }
}
