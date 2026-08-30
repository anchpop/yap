//! The movie library, and where each film's subtitle should come from.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Radarr's language name -> every code an ffprobe stream tag might carry.
///
/// Rips are tagged with a mix of ISO 639-2/B ("fre", "ger"), 639-2/T ("fra",
/// "deu") and 639-1 ("fr", "de"), so matching on any single standard silently
/// misses a third of the library.
pub fn stream_codes(language: &str) -> &'static [&'static str] {
    match language {
        "English" => &["eng", "en"],
        "French" => &["fra", "fre", "fr"],
        "German" => &["deu", "ger", "de"],
        "Spanish" => &["spa", "es"],
        "Italian" => &["ita", "it"],
        "Portuguese" => &["por", "pt", "pob"],
        "Russian" => &["rus", "ru"],
        "Japanese" => &["jpn", "ja", "jp"],
        "Korean" => &["kor", "ko"],
        "Thai" => &["tha", "th"],
        "Hindi" => &["hin", "hi"],
        // "cn" is not ISO anything, but rips carry it.
        "Chinese" | "Cantonese" | "Mandarin" => &["zho", "chi", "zh", "cmn", "yue", "cn"],
        "Marathi" => &["mar", "mr"],
        "Swedish" => &["swe", "sv"],
        "Danish" => &["dan", "da"],
        "Dutch" => &["nld", "dut", "nl"],
        "Polish" => &["pol", "pl"],
        "Turkish" => &["tur", "tr"],
        "Hebrew" => &["heb", "he"],
        "Arabic" => &["ara", "ar"],
        "Telugu" => &["tel", "te"],
        "Tamil" => &["tam", "ta"],
        "Persian" => &["fas", "per", "fa"],
        _ => &[],
    }
}

/// The yap course whose downloaded subtitles would be in this language, if any.
pub fn course_dir(language: &str) -> Option<&'static str> {
    Some(match language {
        "English" => "eng",
        "French" => "fra",
        "German" => "deu",
        "Spanish" => "spa",
        "Italian" => "ita",
        "Portuguese" => "por",
        "Russian" => "rus",
        "Japanese" => "jpn",
        "Korean" => "kor",
        "Thai" => "tha",
        "Hindi" => "hin",
        "Chinese" | "Cantonese" | "Mandarin" => "zho-hans",
        _ => return None,
    })
}

const TEXT_CODECS: &[&str] = &["subrip", "ass", "ssa", "mov_text", "webvtt", "text"];
const BITMAP_CODECS: &[&str] = &["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "xsub"];

/// Where a movie's subtitle will come from, and how far its timing can be trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tier", rename_all = "snake_case")]
pub enum Source {
    /// A text track on the disc. Authored against this file, so already synced.
    DiscText { index: u32, codec: String },
    /// A text file alongside the video, e.g. `Movie (1999).fr.srt`.
    ///
    /// **Not guaranteed to be in sync.** Two different things land here: a file
    /// that shipped inside the rip, which is timed to this exact release, and
    /// one Bazarr fetched afterwards from the same pool the downloads come
    /// from. 23 of 49 were written over an hour after their video, and scoring
    /// them against the audio put 27 of 48 within half a second but caught
    /// *Il Mare* 3.4s out at a decisive 0.30 margin. Better than a download,
    /// short of a disc track.
    Sidecar { path: PathBuf },
    /// A bitmap track on the disc. Also already synced, but needs OCR.
    DiscBitmap { index: u32, codec: String },
    /// A downloaded SRT. Correct text, but timed to some other release.
    Downloaded { path: PathBuf },
    /// A downloaded subtitle exists but only as pre-cleaned JSONL, whose
    /// timings were truncated to whole seconds — too coarse to sync against.
    /// `recover-subtitles` is refetching the originals.
    AwaitingRecovery { path: PathBuf },
    /// Nothing to work from; would need a transcript.
    Missing,
    /// The rip carries no audio in the language the film was made in, so it
    /// cannot contribute original-language speech whatever its subtitles say.
    NoOriginalAudio,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::DiscText { .. } => "disc text",
            Source::Sidecar { .. } => "sidecar text",
            Source::DiscBitmap { .. } => "disc bitmap (OCR)",
            Source::Downloaded { .. } => "downloaded (needs sync)",
            Source::AwaitingRecovery { .. } => "awaiting SRT recovery",
            Source::Missing => "missing",
            Source::NoOriginalAudio => "no original audio",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Movie {
    pub imdb_id: String,
    pub title: String,
    pub year: Option<u16>,
    pub path: PathBuf,
    pub original_language: String,
    pub source: Source,
}

#[derive(Debug, Deserialize)]
struct Stream {
    index: u32,
    codec_name: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    disposition: HashMap<String, u8>,
}

impl Stream {
    fn title(&self) -> String {
        self.tags
            .get("title")
            .cloned()
            .unwrap_or_default()
            .to_lowercase()
    }

    /// A "forced" track only translates foreign dialogue and on-screen signs —
    /// a handful of cues across a whole film. Picking one silently yields a
    /// subtitle that looks valid and contains almost no dialogue.
    fn is_forced(&self) -> bool {
        self.disposition.get("forced").copied().unwrap_or(0) == 1
            || self.title().contains("forced")
            || self.title().contains("signs")
    }

    fn is_commentary(&self) -> bool {
        self.disposition.get("comment").copied().unwrap_or(0) == 1
            || self.title().contains("commentary")
    }

    /// Hearing-impaired tracks carry the full dialogue plus sound descriptions.
    /// Usable, but a plain track is preferred when both exist.
    fn is_sdh(&self) -> bool {
        let t = self.title();
        t.contains("sdh") || t.contains("hearing") || t.contains("cc")
    }
}

#[derive(Debug, Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
}

fn probe(path: &Path, kind: &str) -> Result<Vec<Stream>> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            kind,
            "-show_entries",
            "stream=index,codec_name:stream_tags=language,title:stream_disposition=forced,comment,default",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("ffprobe failed to start — is ffmpeg installed?")?;
    let parsed: Probe = serde_json::from_slice(&out.stdout).unwrap_or(Probe { streams: vec![] });
    Ok(parsed.streams)
}

/// A subtitle stream usable as a *timing* reference, whatever its language.
///
/// Every track on the disc was authored against this exact file, so its cue
/// timings are ground truth even when its language is useless as text. A
/// bitmap track qualifies too: only the timestamps are read, no OCR.
pub struct ReferenceStream {
    pub index: u32,
    pub language: String,
    pub is_text: bool,
    pub codec: String,
}

pub fn reference_subtitle_streams(path: &Path) -> Result<Vec<ReferenceStream>> {
    let mut refs: Vec<ReferenceStream> = probe(path, "s")?
        .into_iter()
        .filter(|s| !s.is_forced() && !s.is_commentary())
        .filter_map(|s| {
            let codec = s.codec_name.clone()?;
            let is_text = TEXT_CODECS.contains(&codec.as_str());
            (is_text || codec == "hdmv_pgs_subtitle" || codec == "dvd_subtitle").then(|| {
                ReferenceStream {
                    index: s.index,
                    language: lang_of(&s),
                    is_text,
                    codec,
                }
            })
        })
        .collect();
    // Text streams cost nothing to read; bitmaps need a full-track demux.
    refs.sort_by_key(|r| !r.is_text);
    Ok(refs)
}

fn lang_of(s: &Stream) -> String {
    // Tags come with whitespace attached often enough to matter: a real rip
    // carried `"cn "` and classified as having no Chinese audio.
    s.tags
        .get("language")
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// Films whose disc *text* track is not timed to the disc's own audio.
///
/// A remux can carry an srt muxed in from another cut entirely: The Money
/// Maker's French subrip drifts +74s→+240s across the film (a TV edit's
/// timing), while the disc's native PGS tracks sit exactly on the speech —
/// verified by Whisper on both ends. Classification has no way to see this
/// coming, so films the `check` command convicts of it are listed here and
/// fall through to their PGS track instead.
const TEXT_TRACK_UNTRUSTED: &[&str] = &[
    "tt35495035", // The Money Maker (2026) — subrip is the TV cut
];

/// Decide the best subtitle source for one movie, probing the file.
pub fn classify(
    imdb_id: &str,
    path: &Path,
    original_language: &str,
    data_root: &Path,
) -> Result<Source> {
    let codes = stream_codes(original_language);

    // Original-language audio first: without it the film is useless for speech
    // regardless of how good its subtitles are.
    let audio = probe(path, "a")?;
    if !audio.is_empty() {
        let tagged: Vec<String> = audio
            .iter()
            .map(lang_of)
            .filter(|l| !l.is_empty() && l != "und")
            .collect();
        // Untagged audio on a single-language rip is virtually always the
        // original, so only an explicit all-foreign tagging is disqualifying.
        // "mul" claims several languages in one track — that is what the
        // original mix of a multilingual film looks like (Brother 2 is
        // Russian with long English stretches), never what a dub looks like.
        if !tagged.is_empty()
            && !tagged
                .iter()
                .any(|l| codes.contains(&l.as_str()) || l == "mul")
        {
            return Ok(Source::NoOriginalAudio);
        }
    }

    let subs = probe(path, "s")?;
    // Forced and commentary tracks are excluded outright rather than ranked
    // last: a forced track is not a worse subtitle, it is a different thing,
    // and falling through to OCR or a downloaded file beats "3 cues".
    let own: Vec<&Stream> = subs
        .iter()
        .filter(|s| codes.contains(&lang_of(s).as_str()))
        .filter(|s| !s.is_forced() && !s.is_commentary())
        .collect();

    // Plain dialogue track first, then SDH.
    let text = |sdh: bool| {
        own.iter()
            .find(|s| {
                s.codec_name
                    .as_deref()
                    .is_some_and(|c| TEXT_CODECS.contains(&c))
                    && s.is_sdh() == sdh
            })
            .copied()
    };
    if !TEXT_TRACK_UNTRUSTED.contains(&imdb_id) {
        if let Some(s) = text(false).or_else(|| text(true)) {
            return Ok(Source::DiscText {
                index: s.index,
                codec: s.codec_name.clone().unwrap_or_default(),
            });
        }
        // A sidecar next to the video is text and usually matched to this
        // release, but it was fetched by Bazarr rather than authored on the
        // disc, so it is preferred over a bitmap track only because OCR is
        // expensive — it still has to have its sync verified.
        if let Some(p) = sidecar(path, codes) {
            return Ok(Source::Sidecar { path: p });
        }
    }

    if let Some(s) = own.iter().find(|s| {
        s.codec_name
            .as_deref()
            .is_some_and(|c| BITMAP_CODECS.contains(&c))
    }) {
        return Ok(Source::DiscBitmap {
            index: s.index,
            codec: s.codec_name.clone().unwrap_or_default(),
        });
    }

    if let Some(course) = course_dir(original_language) {
        let movies = data_root.join(course).join("sentence-sources/movies");
        let raw = movies.join(format!("subtitles-raw/{imdb_id}.srt"));
        if raw.exists() {
            return Ok(Source::Downloaded { path: raw });
        }
        let derived = movies.join(format!("subtitles/{imdb_id}.jsonl"));
        if derived.exists() {
            return Ok(Source::AwaitingRecovery { path: derived });
        }
    }
    Ok(Source::Missing)
}

/// A subtitle file sitting next to the video in one of `codes`.
///
/// Names look like `Movie (1999).fr.srt`, often with modifiers appended:
/// `.en.hi.srt` is *English, hearing-impaired*, not Hindi — so the tokens are
/// walked from the right, skipping modifiers, and `hi` is only read as Hindi
/// when nothing else in the name identifies a language.
fn sidecar(video: &Path, codes: &[&str]) -> Option<PathBuf> {
    const MODIFIERS: &[&str] = &["forced", "sdh", "cc", "hi", "default", "full"];
    let stem = video.file_stem()?.to_str()?;
    let dir = video.parent()?;

    let mut hindi_fallback = None;
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?;
        let Some(rest) = name.strip_prefix(stem) else {
            continue;
        };
        let Some(rest) = rest
            .strip_suffix(".srt")
            .or_else(|| rest.strip_suffix(".ass"))
            .or_else(|| rest.strip_suffix(".ssa"))
        else {
            continue;
        };
        let tokens: Vec<String> = rest
            .split('.')
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect();
        // Our own exported sidecars (`<stem>.yap.<lang>.srt`) must never be
        // rediscovered as a *source* — the corpus would be feeding on its own
        // output, and after an eviction it would happily re-adopt the stale
        // subtitle the eviction just threw away.
        if tokens.iter().any(|t| t == "yap") {
            continue;
        }
        for token in tokens.iter().rev() {
            if MODIFIERS.contains(&token.as_str()) {
                if token == "hi" && codes.contains(&"hin") {
                    hindi_fallback = Some(entry.path());
                }
                continue;
            }
            if codes.contains(&token.as_str()) {
                return Some(entry.path());
            }
        }
    }
    hindi_fallback
}

#[derive(Debug, Deserialize)]
struct RadarrMovie {
    #[serde(default)]
    #[serde(rename = "imdbId")]
    imdb_id: Option<String>,
    title: String,
    year: Option<u16>,
    #[serde(default, rename = "hasFile")]
    has_file: bool,
    #[serde(default, rename = "movieFile")]
    movie_file: Option<RadarrFile>,
    #[serde(default, rename = "originalLanguage")]
    original_language: Option<RadarrLanguage>,
}

#[derive(Debug, Deserialize)]
struct RadarrFile {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RadarrLanguage {
    name: String,
}

/// One movie on disk, before its subtitle source has been decided.
#[derive(Debug, Clone)]
pub struct LibraryEntry {
    pub imdb_id: String,
    pub title: String,
    pub year: Option<u16>,
    pub path: PathBuf,
    pub original_language: String,
}

/// Movies on disk, from a `arr radarr raw GET /movie` dump.
pub fn load_library(dump: &Path) -> Result<Vec<LibraryEntry>> {
    let raw = std::fs::read(dump)
        .with_context(|| format!("Failed to read library dump {}", dump.display()))?;
    let movies: Vec<RadarrMovie> =
        serde_json::from_slice(&raw).context("Library dump is not a Radarr /movie JSON array")?;
    Ok(movies
        .into_iter()
        .filter(|m| m.has_file)
        .filter_map(|m| {
            let path = m.movie_file.as_ref()?.path.as_ref()?;
            let imdb = m.imdb_id.filter(|s| !s.is_empty())?;
            Some(LibraryEntry {
                imdb_id: imdb,
                title: m.title,
                year: m.year,
                path: PathBuf::from(path),
                original_language: m.original_language.map(|l| l.name).unwrap_or_default(),
            })
        })
        .collect())
}

/// Where `inventory` writes the plan, under the corpus root.
pub fn plan_path(out: &Path) -> PathBuf {
    out.join("plan.json")
}

/// Every film the inventory knows about, in inventory order.
pub fn read_plan(out: &Path) -> Result<Vec<Movie>> {
    let p = plan_path(out);
    let raw = std::fs::read(&p).with_context(|| {
        format!(
            "No inventory at {} — run `subtitle-corpus inventory` first",
            p.display()
        )
    })?;
    Ok(serde_json::from_slice(&raw)?)
}

/// `s` cut to `n` characters with an ellipsis, for progress lines.
pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
