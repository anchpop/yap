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

/// Films whose only Chinese audio is Cantonese, which no course teaches.
/// Radarr calls every one of them "Chinese" and their discs label the
/// track no better, so this is the place the knowledge lives. Evidence
/// (2026-09-03): both passed the verbatim check on a Mandarin subtitle yet
/// scored 3–15% of placed clips at phoneme ratios near −3.5, where Mandarin
/// films run 50% at −1 to −2; A Better Tomorrow's disc also carries an
/// "Original Cantonese Mono Track" beside its unlabelled remixes.
const CANTONESE_ONLY: &[&str] = &[
    "tt0093978", // A Chinese Ghost Story (1987)
    "tt0092263", // A Better Tomorrow (1986)
];

/// The film's original language as the pipeline should treat it: Radarr's
/// answer, corrected where the disc's audio is known to be a variety no
/// course teaches.
pub fn original_language(imdb_id: &str, radarr: String) -> String {
    if CANTONESE_ONLY.contains(&imdb_id) {
        "Cantonese".to_string()
    } else {
        radarr
    }
}

/// The yap course whose downloaded subtitles would be in this language, if any.
/// Cantonese audio serves no course: the zho-hans course is Mandarin.
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
        "Chinese" | "Mandarin" => "zho-hans",
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
/// Every code any course's `stream_codes` answers with, so a filename token
/// naming *some other* language is recognized as such rather than read as
/// release-name junk. Without this, `Haider...en.hi.srt` walks past `en`
/// (not a Hindi code, so "junk") and lands on the `hi` fallback — an English
/// subtitle adopted as Hindi, which is how four English SDH tracks ended up
/// as hin/jpn corpus subtitles (found by the 2026-09-01 transcript audit).
const LANGUAGE_CODES: &[&str] = &[
    "eng", "en", "fra", "fre", "fr", "deu", "ger", "de", "spa", "es", "ita", "it", "por", "pt",
    "pob", "rus", "ru", "jpn", "ja", "jp", "kor", "ko", "tha", "th", "hin", "hi", "zho", "chi",
    "zh", "cmn", "yue", "cn", "mar", "mr", "swe", "sv", "dan", "da", "nld", "dut", "nl", "pol",
    "pl", "tur", "tr", "heb", "he", "ara", "ar", "tel", "te", "tam", "ta", "fas", "per", "fa",
];

/// Does this text plausibly carry the course's writing system?
///
/// A sidecar's language tag is a claim, not a fact: `Audition...jpn.srt` and
/// `Dil.Chahta.Hai...hin.srt` both carried English. For a course whose script
/// is not Latin the check is trivial and decisive — most letters must come
/// from the expected ranges. Latin-script courses pass unchecked (French vs
/// English can't be told apart this cheaply; the transcript cross-check
/// catches those downstream).
fn script_plausible(course_code: &str, text: &str) -> bool {
    let expected: fn(char) -> bool = match course_code {
        "hin" => |c| ('\u{0900}'..='\u{097F}').contains(&c),
        "tha" => |c| ('\u{0E00}'..='\u{0E7F}').contains(&c),
        "kor" => {
            |c| ('\u{AC00}'..='\u{D7AF}').contains(&c) || ('\u{1100}'..='\u{11FF}').contains(&c)
        }
        "rus" => |c| ('\u{0400}'..='\u{04FF}').contains(&c),
        "jpn" => {
            |c| ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c)
        }
        "zho" => |c| ('\u{4E00}'..='\u{9FFF}').contains(&c),
        _ => return true,
    };
    let letters = text.chars().filter(|c| c.is_alphabetic());
    let (mut hits, mut total) = (0usize, 0usize);
    for c in letters.take(20_000) {
        total += 1;
        if expected(c) {
            hits += 1;
        }
    }
    // An empty or unreadable file is not evidence either way; let the sync
    // gates deal with it rather than silently skipping the tier.
    total == 0 || hits * 10 >= total * 3
}

/// Names look like `Movie (1999).fr.srt`, often with modifiers appended:
/// `.en.hi.srt` is *English, hearing-impaired*, not Hindi — so the tokens are
/// walked from the right, skipping modifiers, and `hi` is only read as Hindi
/// when no token anywhere in the name identifies *any* language. A candidate
/// must also carry the course's writing system ([`script_plausible`]) — the
/// tag on the filename is a claim the bytes get to veto.
pub fn sidecar(video: &Path, codes: &[&str]) -> Option<PathBuf> {
    const MODIFIERS: &[&str] = &["forced", "sdh", "cc", "hi", "default", "full"];
    let stem = video.file_stem()?.to_str()?;
    let dir = video.parent()?;
    let course = codes.first().copied().unwrap_or_default();
    let plausible = |path: &Path| {
        std::fs::read_to_string(path)
            .map(|text| script_plausible(course, &text))
            .unwrap_or(true)
    };

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
        let mut saw_hi = false;
        for token in tokens.iter().rev() {
            if MODIFIERS.contains(&token.as_str()) {
                saw_hi |= token == "hi";
                continue;
            }
            if codes.contains(&token.as_str()) {
                if plausible(&entry.path()) {
                    return Some(entry.path());
                }
                break;
            }
            if LANGUAGE_CODES.contains(&token.as_str()) {
                // Another language's file; it cannot be ours, and its `hi`
                // (if any) meant hearing-impaired.
                break;
            }
        }
        if saw_hi
            && codes.contains(&"hin")
            && !tokens
                .iter()
                .any(|t| t != "hi" && LANGUAGE_CODES.contains(&t.as_str()))
            && plausible(&entry.path())
        {
            hindi_fallback = Some(entry.path());
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
            let language = original_language(
                &imdb,
                m.original_language.map(|l| l.name).unwrap_or_default(),
            );
            Some(LibraryEntry {
                imdb_id: imdb,
                title: m.title,
                year: m.year,
                path: PathBuf::from(path),
                original_language: language,
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
