//! Tools for building a machine-readable, correctly-synced subtitle for every
//! movie in the library — the substrate everything else (clip cutting, audio
//! datasets) needs first.
//!
//! Sources are preferred in the order that costs least and is most trustworthy:
//! a text track already on the disc, then the disc's bitmap track read by OCR,
//! then a downloaded subtitle synchronised to the file. The first two are
//! authored against this exact file, so their timings need no correction at
//! all — which is why two thirds of the library is free.

mod library;
mod ocr;
mod pgs;
mod sync;
mod transcript;
mod vad;
mod vobsub;

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use library::{Movie, Source};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command_,
}

#[derive(Subcommand, Debug)]
enum Command_ {
    /// Probe every movie and decide where its subtitle will come from.
    Inventory {
        /// JSON from `arr radarr raw GET /movie`.
        #[arg(long)]
        library: PathBuf,
        /// Root of the yap language data (for already-downloaded subtitles).
        #[arg(long, default_value = "./generate-data/data")]
        data_root: PathBuf,
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value_t = 8)]
        jobs: usize,
    },
    /// Bring the corpus up to date with whatever arrived since last time.
    ///
    /// Runs the whole pipeline in order — inventory, extract, ocr, text-sync,
    /// sync, vad-sync, check — each step resumable and skipping finished work,
    /// so a run where nothing changed costs nearly nothing. OCR is included:
    /// its spend per new film is trivial and its batches are cached, so an
    /// interrupted film simply completes on the next refresh.
    Refresh {
        /// JSON from `arr radarr raw GET /movie`.
        #[arg(long)]
        library: PathBuf,
        #[arg(long, default_value = "./generate-data/data")]
        data_root: PathBuf,
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Limit full-film transcription and its audio extraction to these
        /// IMDb ids.
        ///
        /// May be repeated. Every other refresh stage still considers the whole
        /// inventory. With no values, refresh processes every film as before.
        #[arg(long)]
        transcribe_imdb: Vec<String>,
    },
    /// Pull out the subtitles that are already text on the disc.
    Extract {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value_t = 6)]
        jobs: usize,
        /// Stop after this many movies (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Pull each film's original-language audio out as a seekable opus file.
    ///
    /// Written into the corpus (never beside the videos) as `audio.opus`, with
    /// `audio.json` recording the source file *and* the exact stream it came
    /// from — a changed video or a reshuffled remux evicts the track rather
    /// than posing as it. The syncers prefer this artifact when it is current:
    /// whisper windows and VAD profiles then read a few hundred MB of opus
    /// instead of demuxing a lossless track out of a 30GB remux.
    ExtractAudio {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value_t = 6)]
        jobs: usize,
        /// Stop after this many movies (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Extract audio for this film alone, by IMDb id.
        #[arg(long)]
        imdb: Option<String>,
    },
    /// Measure OCR cost and quality on a random sample before the full run.
    OcrSample {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// How many movies to sample.
        #[arg(long, default_value_t = 5)]
        movies: usize,
        /// How many cues to transcribe per sampled movie.
        #[arg(long, default_value_t = 20)]
        cues: usize,
        #[arg(long, default_value = "gpt-5.6-luna")]
        model: String,
    },
    /// Read every bitmap subtitle track in the library back into text.
    Ocr {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value = "gpt-5.6-luna")]
        model: String,
        /// Films whose batches run at once.
        #[arg(long, default_value_t = 8)]
        films_in_flight: usize,
        /// Stop after this many movies (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Cues a film may fail to read and still be written out.
        ///
        /// Zero by default: a film with any unreadable cue is left for the next
        /// run, where cached cues are free and only the failures are retried.
        /// Raise it only to force through a film that never converges.
        #[arg(long, default_value_t = 0)]
        allow_unreadable: usize,
    },
    /// OCR a standalone bitmap subtitle into an SRT with the disc's timings.
    ///
    /// For retail subtitle rips that arrive outside any library film — a
    /// VobSub idx/sub pair muxed into an MKV (`ffmpeg -f vobsub -i file.idx
    /// -map 0:s -c copy file.mkv`) or a bare PGS `.sup`. Drop the output into
    /// `subtitles-raw/` and it syncs like any downloaded subtitle.
    OcrFile {
        /// MKV holding a dvd_subtitle track, or a bare PGS .sup.
        #[arg(long)]
        input: PathBuf,
        /// ffmpeg stream index of the bitmap track within the MKV.
        #[arg(long, default_value_t = 0)]
        index: u32,
        #[arg(long, default_value = "gpt-5.6-luna")]
        model: String,
        /// Where to write the SRT.
        #[arg(long)]
        srt: PathBuf,
        /// Cues allowed to stay unreadable while still writing the SRT.
        #[arg(long, default_value_t = 0)]
        allow_unreadable: usize,
    },
    /// Align downloaded subtitles to the films on disk, using Whisper.
    Sync {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Root of the yap language data, where recovered raw SRTs live.
        #[arg(long, default_value = "./generate-data/data")]
        data_root: PathBuf,
        /// Audio windows to transcribe per film.
        #[arg(long, default_value_t = 5)]
        windows: usize,
        /// Seconds of audio per window.
        #[arg(long, default_value_t = 60)]
        window_secs: u32,
        /// Films aligned at once.
        #[arg(long, default_value_t = 4)]
        films_in_flight: usize,
        /// Stop after this many films (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Reject an alignment whose anchors disagree by more than this, in ms.
        #[arg(long, default_value_t = 1500.0)]
        max_residual_ms: f64,
        /// Reject unless this share of anchors agree on the same shift.
        ///
        /// A handful of anchors can agree by chance while most contradict them,
        /// which is what a wrong match or a different cut looks like. Consensus
        /// separates "found the offset" from "found an offset".
        #[arg(long, default_value_t = 0.35)]
        min_agreement: f64,
        /// Print every anchor's position and delta, to see a failure's shape:
        /// a flat band is an offset, a slope is a rate, a staircase is a
        /// splice, shotgun noise is a wrong subtitle.
        #[arg(long)]
        debug_anchors: bool,
    },
    /// Cross-examine already-written alignments with Whisper word anchors.
    ///
    /// Calibration showed VAD can lock confidently onto the wrong shift —
    /// Scary Movie sat 20.3s off at double the margin gate on a subtitle that
    /// was in fact correct. Word anchors fail differently, so on a correct
    /// subtitle the fit comes back as the identity: offset ≈ 0, rate ≈ 1.
    /// This spends a few transcription windows per film asking exactly that,
    /// and appends findings to `whisper-check.jsonl` (resumable; already-
    /// checked films are skipped).
    Check {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Only films whose subtitle came from this source (substring of the
        /// tier label, e.g. "sidecar", "downloaded").
        #[arg(long)]
        tier: Option<String>,
        /// Audio windows to transcribe per film.
        #[arg(long, default_value_t = 5)]
        windows: usize,
        #[arg(long, default_value_t = 60)]
        window_secs: u32,
        #[arg(long, default_value_t = 4)]
        films_in_flight: usize,
        /// Stop after this many films (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Transcribe every film in full, so its subtitle can be checked against
    /// what was actually said.
    ///
    /// Reads the extracted `audio.opus`, cuts it at the quietest seams the
    /// film's speech profile offers, and writes `transcript.jsonl` beside the
    /// subtitle. Responses are cached in the shared store by chunk bytes, so
    /// re-running is free for everything already transcribed.
    Transcribe {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Films transcribed at once.
        #[arg(long, default_value_t = 4)]
        films_in_flight: usize,
        /// Stop after this many films (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Transcribe this film alone, by IMDb id.
        #[arg(long)]
        imdb: Option<String>,
    },
    /// Score how well a subtitle's timing agrees with where speech actually is.
    ///
    /// Reports the shift that best matches, so a subtitle already believed
    /// correct should score near zero. Run it on the disc-sourced ones to test
    /// that belief.
    Agreement {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Only films whose subtitle came from this source.
        #[arg(long)]
        tier: Option<String>,
        #[arg(long, default_value_t = 0)]
        limit: usize,
        #[arg(long, default_value_t = 4)]
        jobs: usize,
        /// Widest shift to consider, in seconds.
        #[arg(long, default_value_t = 60)]
        range_secs: i64,
    },
    /// Align remaining subtitles by matching speech activity, not words.
    ///
    /// Complements `sync`: it reads no vocabulary, so paraphrase, archaic
    /// speech and mishearing cannot hurt it, and it weighs the whole film
    /// rather than a few sampled windows. It finds only a constant shift.
    VadSync {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value = "./generate-data/data")]
        data_root: PathBuf,
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        #[arg(long, default_value_t = 0)]
        limit: usize,
        #[arg(long, default_value_t = 120)]
        range_secs: i64,
        /// Least correlation the winning shift must reach.
        #[arg(long, default_value_t = 0.15)]
        min_agreement: f32,
        /// Least it must beat every shift more than 2s away.
        ///
        /// The decisive number, calibrated by `calibrate` over 323 films ×
        /// 14 perturbations: at 0.08 every one of 1,615 rate-error locks
        /// (PAL/NTSC/cinema) fell below the line while 97% of 2,798 correct
        /// recoveries stayed above it. What no margin can catch is a film
        /// whose dialogue rhythm false-locks VAD outright (God of Cookery
        /// answers +12.2s at margin 0.25 regardless of perturbation) — only
        /// agreement with an independent method rules those out.
        #[arg(long, default_value_t = 0.08)]
        min_margin: f32,
    },
    /// Align a downloaded subtitle against the disc's own subtitle tracks,
    /// with no audio involved.
    ///
    /// Any subtitle track on the disc — whatever its language — was authored
    /// against this exact file, so its cue *timings* are ground truth. Even a
    /// bitmap track works: only the timestamps are read, no OCR. This is
    /// exactly where both audio methods fail — a sparse, music-heavy film
    /// starves Whisper and VAD alike, but its reference track carries the same
    /// sparseness on the correct clock.
    TextSync {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value = "./generate-data/data")]
        data_root: PathBuf,
        #[arg(long, default_value_t = 4)]
        jobs: usize,
        /// Stop after this many films (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Widest shift considered, in seconds.
        ///
        /// Wider than the audio path's default: a downloaded subtitle has been
        /// seen 203.8s out, and against a reference track the extra search is
        /// nearly free.
        #[arg(long, default_value_t = 300)]
        range_secs: i64,
        /// Least correlation the winning shift must reach against a reference.
        #[arg(long, default_value_t = 0.25)]
        min_agreement: f32,
        /// Least it must beat every shift more than 2s away.
        #[arg(long, default_value_t = 0.10)]
        min_margin: f32,
        /// Report what each reference says without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Measure the aligner against ground truth manufactured from disc tracks.
    ///
    /// A disc subtitle is correctly timed by construction — it was authored
    /// against this exact file. Shifting one by a known amount and asking the
    /// aligner to place it turns "sync accuracy cannot be measured" into an
    /// exact error curve, and calibrates the acceptance thresholds that were
    /// otherwise guesses. Speech profiles are cached beside each film's
    /// subtitle, so the first run pays for the audio decode and later runs are
    /// cheap.
    Calibrate {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        #[arg(long, default_value_t = 4)]
        jobs: usize,
        /// Stop after this many films (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Widest shift the search considers, in seconds.
        #[arg(long, default_value_t = 120)]
        range_secs: i64,
    },
    /// Flag extracted subtitles that are too sparse to be real dialogue.
    Verify {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
        /// Cues per minute below which a track is not plausibly full dialogue.
        #[arg(long, default_value_t = 2.0)]
        min_density: f64,
    },
    /// Publish finished subtitles next to their films as media-server sidecars.
    ///
    /// Writes `<video>.yap.<lang>.srt` beside each film whose corpus subtitle
    /// is verified against the file currently on disk — jellyfin shows it as
    /// a subtitle track titled "yap". Only files matching `*.yap.*.srt` are
    /// ever created or deleted, so shipped and Bazarr sidecars are untouched;
    /// stale or orphaned yap-sidecars are removed. `classify` ignores the
    /// `.yap.` namespace, so the corpus never rediscovers its own output as a
    /// source.
    ExportSidecars {
        #[arg(long, default_value = "/data/andrep/subtitle-corpus")]
        out: PathBuf,
    },
    /// Decode a bitmap subtitle track and report what is in it.
    ///
    /// Takes a PGS `.sup`, or with `--index` a VobSub track read straight out
    /// of an MKV. `--dump` writes sample cue PNGs for eyeballing a decode
    /// before trusting it with an OCR spend.
    PgsStats {
        input: PathBuf,
        /// Read this stream of an MKV as VobSub instead of a `.sup`.
        #[arg(long)]
        index: Option<u32>,
        #[arg(long, default_value_t = 0)]
        dump: usize,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
    },
}

fn plan_path(out: &std::path::Path) -> PathBuf {
    out.join("plan.json")
}

/// The subtitle file a syncer should align for this movie, if any.
///
/// Usually the recovered raw SRT in the movie's language pack. Failing that,
/// a sidecar file counts too: `extract` used to trust sidecars as
/// already-synced, but 23 of 49 turned out to be Bazarr downloads on some
/// other release's clock (*Il Mare* 3.4s out at margin 0.30) — so a sidecar
/// whose finalized output has been removed re-enters through the same sync
/// gates as any download.
/// Films whose only available subtitle was authored against a different cut,
/// so no single offset+rate can place it: Whisper finds a strong local fit
/// that a film-wide re-check then contradicts (a fresh Still Life fit of
/// +74.71s re-measured at +14.30s with 45/101 anchors). Every sync pass
/// would re-place and re-fail these forever; they wait for a replacement
/// release instead.
const DIFFERENT_CUT: &[&str] = &[
    "tt0859765", // Still Life (2006)
    "tt0209189", // Not One Less (1999)
    "tt3742378", // The Second Mother (2015) — sync locks +26.0s on 8/20 anchors, check re-measures +123.5s
];

/// Sidecars `check` has convicted of carrying another release's clock.
///
/// `extract` trusts a sidecar's own timings, and for most that is right —
/// but a Bazarr download can be timed to a different rip of the same cut
/// (Fallen Angels −19.2s over 104/110 anchors, When Marnie Was There +23.3s
/// over 341/355, both at rate 1.0000). Deleting the finalized output is not
/// enough on its own: the next `refresh` would just re-finalize the same
/// file. Listing a film here makes `extract` leave it alone while `classify`
/// still reports the sidecar, so `subtitle_source` hands its text to the
/// syncers to be placed like any downloaded subtitle.
const SIDECAR_UNTRUSTED: &[&str] = &[
    "tt0112913", // Fallen Angels (1995)
    "tt3398268", // When Marnie Was There (2014)
    "tt1568921", // The Secret World of Arrietty (2010) — +28.5s over 257/262 anchors
];

fn subtitle_source(movie: &Movie, data_root: &std::path::Path) -> Option<PathBuf> {
    // A film with no original-language audio can never yield a speech clip,
    // so aligning a subtitle for it is work spent making a number wrong.
    if matches!(movie.source, Source::NoOriginalAudio) {
        return None;
    }
    if DIFFERENT_CUT.contains(&movie.imdb_id.as_str()) {
        return None;
    }
    if let Some(course) = library::course_dir(&movie.original_language) {
        let raw = data_root
            .join(course)
            .join("sentence-sources/movies")
            .join(format!("subtitles-raw/{}.srt", movie.imdb_id));
        if raw.exists() {
            return Some(raw);
        }
    }
    match &movie.source {
        Source::Sidecar { path } if path.exists() => Some(path.clone()),
        _ => None,
    }
}

/// Which inputs a finished subtitle was derived from, recorded next to it as
/// `film.json`. For the video: filename and duration, not byte size — a
/// re-encode keeps its timing but not its bytes. For the subtitle source (a
/// downloaded raw SRT or a sidecar; absent for disc-derived outputs):
/// filename and byte size, since subtitle files are replaced, not re-encoded.
/// When `inventory` sees either input no longer matching the stamp, the
/// derived artifacts are evicted and the film re-enters the queues — all of
/// them if the video changed, just the subtitle if only its source did (the
/// speech profile and reference timings depend on the video alone).
#[derive(serde::Serialize, serde::Deserialize)]
struct FilmStamp {
    filename: String,
    duration_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subtitle: Option<SubtitleStamp>,
}

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone)]
struct SubtitleStamp {
    filename: String,
    bytes: u64,
}

fn subtitle_stamp(path: &std::path::Path) -> Option<SubtitleStamp> {
    Some(SubtitleStamp {
        filename: path.file_name()?.to_string_lossy().into_owned(),
        bytes: std::fs::metadata(path).ok()?.len(),
    })
}

/// What a writer knows about the subtitle source it derived from.
enum StampSource<'a> {
    /// The subtitle came off the disc itself; there is no separate source.
    Disc,
    /// Derived from this subtitle file.
    File(&'a std::path::Path),
    /// Says nothing about the subtitle — a speech profile or reference cache
    /// must not erase what the last subtitle writer recorded.
    Keep,
}

fn read_stamp(dir: &std::path::Path) -> Option<FilmStamp> {
    serde_json::from_slice(&std::fs::read(dir.join("film.json")).ok()?).ok()
}

/// The subtitle file the film's output *should* currently derive from, or
/// None for disc-sourced films (their source is the video itself).
fn expected_subtitle_source(movie: &Movie, data_root: &std::path::Path) -> Option<PathBuf> {
    match &movie.source {
        Source::DiscText { .. } | Source::DiscBitmap { .. } => None,
        _ => subtitle_source(movie, data_root),
    }
}

impl FilmStamp {
    /// Same film for subtitle purposes: identical name, and a duration within
    /// 250ms (a remux of the same cut wobbles by frames; anything that moved
    /// the length by more than that has probably retimed the content too).
    fn matches(&self, other: &FilmStamp) -> bool {
        self.filename == other.filename && (self.duration_ms - other.duration_ms).abs() <= 250
    }
}

fn film_stamp(movie: &Movie) -> Result<FilmStamp> {
    Ok(FilmStamp {
        filename: movie
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        duration_ms: sync::duration_ms(&movie.path)?,
        subtitle: None,
    })
}

/// The film's speech profile, from the per-film cache when present.
///
/// Decoding a feature's audio takes minutes; the profile is ~260KB. Cached
/// next to the subtitle and stamped with the film's identity so `inventory`
/// evicts it when the file changes underneath — which matters even for films
/// with no finished subtitle yet, where only the profile exists.
fn cached_speech_profile(movie: &Movie, dir: &std::path::Path) -> Result<Vec<f32>> {
    let path = dir.join("speech-profile.f32");
    if let Some(p) = vad::read_profile(&path) {
        return Ok(p);
    }
    let (media, stream) = audio_source(movie, dir)?;
    let p = vad::speech_profile(&media, stream)?;
    std::fs::create_dir_all(dir)?;
    vad::write_profile(&path, &p)?;
    write_stamp(dir, movie, StampSource::Keep);
    Ok(p)
}

/// Cue timings for one of the disc's reference tracks, cached per film.
///
/// Reading a bitmap reference means demuxing the whole film — minutes — for a
/// product that is a few KB of timestamps. Cached like the speech profile,
/// stamped with the film's identity, evicted with it when the film changes.
/// Only timings survive the cache; text-sync never reads reference *text*.
fn cached_reference_cues(
    movie: &Movie,
    out: &std::path::Path,
    stream: &library::ReferenceStream,
    scratch: &std::path::Path,
) -> Result<Vec<sync::Cue>> {
    let dir = out.join(&movie.imdb_id);
    let path = dir.join("references.json");
    let mut cache: std::collections::HashMap<u32, Vec<(i64, i64)>> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Some(spans) = cache.get(&stream.index) {
        return Ok(spans
            .iter()
            .map(|&(start_ms, end_ms)| sync::Cue {
                start_ms,
                end_ms,
                text: String::new(),
            })
            .collect());
    }
    let cues = reference_cues(&movie.path, stream, scratch)?;
    cache.insert(
        stream.index,
        cues.iter().map(|c| (c.start_ms, c.end_ms)).collect(),
    );
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_vec(&cache)?)?;
    write_stamp(&dir, movie, StampSource::Keep);
    Ok(cues)
}

/// Best-effort: a missing stamp is backfilled by the next `inventory`, never
/// a reason to fail a sync that already succeeded.
fn write_stamp(dir: &std::path::Path, movie: &Movie, source: StampSource) {
    let Ok(mut stamp) = film_stamp(movie) else {
        return;
    };
    stamp.subtitle = match source {
        StampSource::Disc => None,
        StampSource::File(p) => subtitle_stamp(p),
        StampSource::Keep => read_stamp(dir).and_then(|s| s.subtitle),
    };
    if let Ok(json) = serde_json::to_vec_pretty(&stamp) {
        let _ = std::fs::write(dir.join("film.json"), json);
    }
    // A finalized output supersedes any recorded failure.
    if !matches!(source, StampSource::Keep) {
        let _ = std::fs::remove_file(dir.join("sync-failed.json"));
    }
}

/// The inputs a failed alignment was attempted against: the video's identity
/// plus the subtitle file it tried to place.
fn sync_failure_stamp(movie: &Movie, raw: &std::path::Path) -> Option<FilmStamp> {
    let mut stamp = film_stamp(movie).ok()?;
    stamp.subtitle = subtitle_stamp(raw);
    Some(stamp)
}

/// Alignment is deterministic in its inputs: until the video or the subtitle
/// file changes, re-running the gauntlet reproduces the same failure. Every
/// refresh used to grind all ~18 hopeless films through text-sync, sync and
/// vad-sync anyway — half an hour of audio decoding and Whisper windows to
/// learn nothing. `vad-sync` (the last gate) records the failed inputs as
/// `sync-failed.json`; a matching stamp skips the film in every sync queue. A
/// new download or release stops matching and retries automatically; deleting
/// the file forces a retry by hand.
fn sync_already_failed(movie: &Movie, dir: &std::path::Path, raw: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(dir.join("sync-failed.json")) else {
        return false;
    };
    let Ok(recorded) = serde_json::from_slice::<FilmStamp>(&bytes) else {
        return false;
    };
    let Some(current) = sync_failure_stamp(movie, raw) else {
        return false;
    };
    recorded.matches(&current) && recorded.subtitle == current.subtitle
}

fn record_sync_failure(movie: &Movie, dir: &std::path::Path, raw: &std::path::Path) {
    let Some(stamp) = sync_failure_stamp(movie, raw) else {
        return;
    };
    if let Ok(json) = serde_json::to_vec_pretty(&stamp) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join("sync-failed.json"), json);
    }
}

/// Queue-builder suffix explaining how many films were skipped as already
/// failed on identical inputs.
fn parked_note(parked: usize) -> String {
    if parked == 0 {
        String::new()
    } else {
        format!(" ({parked} skipped: same inputs already failed; delete sync-failed.json to retry)")
    }
}

/// Provenance for `audio.opus`: which file the track came out of and exactly
/// which stream, so a video swap or an audio reshuffle inside a same-named
/// remux evicts the extraction instead of being mistaken for it.
#[derive(serde::Serialize, serde::Deserialize, PartialEq)]
struct AudioStamp {
    filename: String,
    duration_ms: i64,
    stream: sync::AudioStreamIdentity,
}

fn read_audio_stamp(dir: &std::path::Path) -> Option<AudioStamp> {
    serde_json::from_slice(&std::fs::read(dir.join("audio.json")).ok()?).ok()
}

/// The film's extracted original-language audio, when it is present and still
/// belongs to the file on disk. File identity only — the per-stream probe is
/// `extract-audio`'s job; a caller here just needs to trust the artifact.
fn extracted_audio(movie: &Movie, dir: &std::path::Path) -> Option<PathBuf> {
    let path = dir.join("audio.opus");
    if !path.exists() {
        return None;
    }
    let stamp = read_audio_stamp(dir)?;
    let current = film_stamp(movie).ok()?;
    let recorded = FilmStamp {
        filename: stamp.filename,
        duration_ms: stamp.duration_ms,
        subtitle: None,
    };
    recorded.matches(&current).then_some(path)
}

/// Where to listen for this film: the extracted opus when it is current —
/// a few hundred MB that seeks instantly, instead of pulling a lossless
/// track out of a 30GB remux on the array — else the video itself.
fn audio_source(movie: &Movie, dir: &std::path::Path) -> Result<(PathBuf, usize)> {
    if let Some(audio) = extracted_audio(movie, dir) {
        return Ok((audio, 0));
    }
    let codes = library::stream_codes(&movie.original_language);
    let stream = sync::original_audio_stream(&movie.path, codes)?;
    Ok((movie.path.clone(), stream))
}

enum Freshness {
    /// No finished subtitle, or the stamp still matches the inputs on disk.
    Fine,
    /// Output predates stamping; stamped with the inputs currently on disk.
    Backfilled,
    /// An input changed underneath the output — stale artifacts evicted.
    Evicted { why: String },
}

fn freshen_output(movie: &Movie, out: &std::path::Path, data_root: &std::path::Path) -> Freshness {
    let dir = out.join(&movie.imdb_id);
    let has_subtitle = dir.join("subtitle.srt").exists();
    // A stamp with no subtitle is a cached speech profile — still worth
    // checking, since a stale profile would poison the next vad-sync.
    if !has_subtitle && !dir.join("film.json").exists() {
        return Freshness::Fine;
    }
    // No film on disk is not evidence of change — the array may be offline.
    // Evict only when a present file positively fails to match.
    let Ok(current) = film_stamp(movie) else {
        return Freshness::Fine;
    };
    let Some(old) = read_stamp(&dir) else {
        // Legacy output predating stamps: it was verified against what is on
        // disk today, so record today's inputs as its provenance.
        match expected_subtitle_source(movie, data_root) {
            Some(p) => write_stamp(&dir, movie, StampSource::File(&p)),
            None => write_stamp(&dir, movie, StampSource::Disc),
        }
        return Freshness::Backfilled;
    };
    if !old.matches(&current) {
        // The video changed: everything derived from it is stale.
        let _ = std::fs::remove_file(dir.join("subtitle.srt"));
        let _ = std::fs::remove_file(dir.join("speech-profile.f32"));
        let _ = std::fs::remove_file(dir.join("references.json"));
        let _ = std::fs::remove_file(dir.join("transcript.jsonl"));
        let _ = std::fs::remove_file(dir.join("audio.opus"));
        let _ = std::fs::remove_file(dir.join("audio.json"));
        let _ = std::fs::remove_file(dir.join("film.json"));
        let _ = std::fs::remove_file(dir.join("sync-failed.json"));
        return Freshness::Evicted {
            why: format!("film changed ({} → {})", old.filename, current.filename),
        };
    }
    // Video unchanged; is the subtitle still derived from the right source?
    let expected_path = expected_subtitle_source(movie, data_root);
    let expected = expected_path.as_deref().and_then(subtitle_stamp);
    match (&old.subtitle, &expected) {
        (a, b) if a == b => Freshness::Fine,
        (None, None) => Freshness::Fine,
        (None, Some(_)) => {
            // Pre-subtitle-stamp output: record its source, don't evict.
            if !has_subtitle {
                return Freshness::Fine;
            }
            if let Some(p) = &expected_path {
                write_stamp(&dir, movie, StampSource::File(p));
            }
            Freshness::Backfilled
        }
        (Some(was), now) => {
            // Only the subtitle source moved; the audio-derived caches
            // (speech profile, reference timings) are still good.
            let _ = std::fs::remove_file(dir.join("subtitle.srt"));
            write_stamp(&dir, movie, StampSource::Disc);
            if !has_subtitle {
                return Freshness::Fine;
            }
            Freshness::Evicted {
                why: match now {
                    Some(n) => format!(
                        "subtitle source changed ({} → {})",
                        was.filename, n.filename
                    ),
                    None => format!("subtitle source gone ({})", was.filename),
                },
            }
        }
    }
}

fn read_plan(out: &std::path::Path) -> Result<Vec<Movie>> {
    let p = plan_path(out);
    let raw = std::fs::read(&p).with_context(|| {
        format!(
            "No inventory at {} — run `subtitle-corpus inventory` first",
            p.display()
        )
    })?;
    Ok(serde_json::from_slice(&raw)?)
}

/// Run `f` over `items` on `jobs` threads, reporting progress as it goes.
///
/// The work is IO-bound on ffmpeg reading whole films off the array, so the
/// results come back out of order and are re-sorted into the input order at the
/// end rather than being written into shared slots.
fn parallel<T: Send + Sync, R: Send>(
    items: Vec<T>,
    jobs: usize,
    label: &str,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let total = items.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                let value = f(&items[i]);
                results.lock().unwrap().push((i, value));
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(25) || n == total {
                    eprintln!("  {label}: {n}/{total}");
                }
            });
        }
    });

    let mut out = results.into_inner().unwrap();
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, v)| v).collect()
}

fn inventory(library: PathBuf, data_root: PathBuf, out: PathBuf, jobs: usize) -> Result<()> {
    let movies = library::load_library(&library)?;
    println!("{} movies on disk", movies.len());

    let probed = parallel(movies, jobs, "probing", |entry| {
        let source = library::classify(
            &entry.imdb_id,
            &entry.path,
            &entry.original_language,
            &data_root,
        )
        .unwrap_or(Source::Missing);
        let movie = Movie {
            imdb_id: entry.imdb_id.clone(),
            title: entry.title.clone(),
            year: entry.year,
            path: entry.path.clone(),
            original_language: entry.original_language.clone(),
            source,
        };
        let freshness = freshen_output(&movie, &out, &data_root);
        (movie, freshness)
    });

    let mut backfilled = 0usize;
    for (movie, freshness) in &probed {
        match freshness {
            Freshness::Fine => {}
            Freshness::Backfilled => backfilled += 1,
            Freshness::Evicted { why } => println!(
                "  ✗ {} — {why}, evicted for re-derivation",
                truncate(&movie.title, 40),
            ),
        }
    }
    if backfilled > 0 {
        println!("  stamped {backfilled} existing subtitles with their film's identity");
    }
    let classified: Vec<Movie> = probed.into_iter().map(|(m, _)| m).collect();

    std::fs::create_dir_all(&out)?;
    std::fs::write(plan_path(&out), serde_json::to_vec_pretty(&classified)?)?;

    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for m in &classified {
        *counts.entry(m.source.label()).or_default() += 1;
    }
    println!("\nsubtitle source for each movie:");
    for (label, n) in &counts {
        println!("  {label:26}{n:>5}");
    }
    let ready = counts.get("disc text").copied().unwrap_or(0)
        + counts.get("disc bitmap (OCR)").copied().unwrap_or(0);
    println!(
        "\n{ready} of {} need no synchronisation at all (the disc's own track).",
        classified.len()
    );
    println!("wrote {}", plan_path(&out).display());
    Ok(())
}

/// Is this film's finished subtitle verified against the file on disk?
///
/// Read-only twin of [`freshen_output`]: an unstamped output is not fresh
/// (the next `inventory` will backfill it), and a changed film is not fresh
/// (the next `inventory` will evict it).
fn output_is_fresh(movie: &Movie, dir: &std::path::Path) -> bool {
    if !dir.join("subtitle.srt").exists() {
        return false;
    }
    let Ok(current) = film_stamp(movie) else {
        return false;
    };
    std::fs::read(dir.join("film.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<FilmStamp>(&b).ok())
        .is_some_and(|stored| stored.matches(&current))
}

/// Publish verified subtitles as `<video>.yap.<lang>.srt` sidecars, and
/// retract the ones the corpus no longer stands behind.
fn export_sidecars(out: PathBuf) -> Result<()> {
    let plan = read_plan(&out)?;
    let (mut written, mut kept, mut removed) = (0usize, 0usize, 0usize);
    for movie in &plan {
        let Some(video_dir) = movie.path.parent() else {
            continue;
        };
        let Some(stem) = movie.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let lang = library::stream_codes(&movie.original_language)
            .first()
            .copied()
            .unwrap_or("und");
        let expected = format!("{stem}.yap.{lang}.srt");
        let fresh = output_is_fresh(movie, &out.join(&movie.imdb_id));

        // Everything in our namespace that is not the one file we currently
        // stand behind — old video names, evicted films — gets retracted.
        for entry in std::fs::read_dir(video_dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.contains(".yap.") || !name.ends_with(".srt") {
                continue;
            }
            if !(fresh && name == expected) {
                let _ = std::fs::remove_file(entry.path());
                removed += 1;
            }
        }

        if fresh {
            let srt = std::fs::read(out.join(&movie.imdb_id).join("subtitle.srt"))?;
            let dest = video_dir.join(&expected);
            if std::fs::read(&dest).ok().as_deref() == Some(srt.as_slice()) {
                kept += 1;
            } else {
                std::fs::write(&dest, srt)?;
                written += 1;
            }
        }
    }
    println!("{written} sidecars written, {kept} already current, {removed} retracted");
    Ok(())
}

/// The whole pipeline, in dependency order, with each step's own defaults.
///
/// Inventory failing aborts — every later step would read a stale plan and
/// quietly do the wrong work. Any other step failing is reported and skipped
/// past: a Whisper outage is no reason not to run text-sync, and the next
/// refresh retries whatever was left undone.
fn refresh(
    library: PathBuf,
    data_root: PathBuf,
    out: PathBuf,
    transcribe_imdb: Vec<String>,
) -> Result<()> {
    println!("━━━ inventory ━━━");
    inventory(library, data_root.clone(), out.clone(), 8)?;
    let audio_imdb = transcribe_imdb.clone();

    type Step<'a> = (&'a str, Box<dyn FnOnce() -> Result<()>>);
    let steps: Vec<Step> = vec![
        ("extract", {
            let out = out.clone();
            Box::new(move || extract(out, 6, 0))
        }),
        ("ocr", {
            let out = out.clone();
            Box::new(move || ocr_all(out, "gpt-5.6-luna".into(), 8, 0, 0))
        }),
        ("extract-audio", {
            let out = out.clone();
            Box::new(move || {
                if audio_imdb.is_empty() {
                    extract_audio(out, 6, 0, None)
                } else {
                    for imdb in audio_imdb {
                        extract_audio(out.clone(), 1, 0, Some(&imdb))?;
                    }
                    Ok(())
                }
            })
        }),
        ("text-sync", {
            let (out, data_root) = (out.clone(), data_root.clone());
            Box::new(move || text_sync(out, data_root, 4, 0, 300, 0.25, 0.10, false))
        }),
        ("sync", {
            let (out, data_root) = (out.clone(), data_root.clone());
            Box::new(move || {
                sync_all(
                    out,
                    data_root,
                    4,
                    0,
                    SyncOptions {
                        windows: 5,
                        window_secs: 60,
                        max_residual_ms: 1500.0,
                        min_agreement: 0.35,
                        debug_anchors: false,
                    },
                )
            })
        }),
        ("vad-sync", {
            let (out, data_root) = (out.clone(), data_root.clone());
            Box::new(move || vad_sync(out, data_root, 3, 0, 120, 0.15, 0.08))
        }),
        ("check", {
            let out = out.clone();
            Box::new(move || check_all(out, None, 5, 60, 4, 0))
        }),
        ("transcribe", {
            let out = out.clone();
            Box::new(move || {
                if transcribe_imdb.is_empty() {
                    transcribe_all(out, 4, 0, None)
                } else {
                    for imdb in transcribe_imdb {
                        transcribe_all(out.clone(), 1, 0, Some(imdb))?;
                    }
                    Ok(())
                }
            })
        }),
        ("sidecars", {
            let out = out.clone();
            Box::new(move || export_sidecars(out))
        }),
    ];

    let mut failed: Vec<&str> = Vec::new();
    for (name, run) in steps {
        println!("\n━━━ {name} ━━━");
        if let Err(e) = run() {
            println!("  ✗ {name} failed: {e:#}");
            failed.push(name);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        bail!("steps failed: {}", failed.join(", "))
    }
}

/// Convert one embedded text track to SRT.
///
/// Subtitle packets are interleaved through the container, so this reads the
/// whole file — it is the slow part, and why it runs in parallel.
fn extract_one(movie: &Movie, out: &std::path::Path) -> Result<usize> {
    let dir = out.join(&movie.imdb_id);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join("subtitle.srt");
    if dest.exists() {
        let text = std::fs::read_to_string(&dest)?;
        return Ok(movie_subtitles_len(&text));
    }
    let tmp = dir.join("subtitle.srt.tmp");

    // Both paths go through ffmpeg: a sidecar may be ASS/SSA, and even an SRT
    // can carry a byte-order mark or non-UTF-8 encoding that a plain copy would
    // preserve and every later stage would then have to cope with.
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-y", "-i"]);
    match &movie.source {
        Source::DiscText { index, .. } => {
            cmd.arg(&movie.path)
                .args(["-map", &format!("0:{index}"), "-f", "srt"]);
        }
        Source::Sidecar { path } => {
            cmd.arg(path).args(["-f", "srt"]);
        }
        other => bail!("{} is not a text source", other.label()),
    }
    let status = cmd.arg(&tmp).status().context("ffmpeg failed to start")?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("ffmpeg exited with {status}");
    }
    let text = std::fs::read_to_string(&tmp)?;
    let cues = movie_subtitles_len(&text);
    if cues == 0 {
        let _ = std::fs::remove_file(&tmp);
        bail!("extracted track had no cues");
    }
    std::fs::rename(&tmp, &dest)?;
    match &movie.source {
        Source::Sidecar { path } => write_stamp(&dir, movie, StampSource::File(path)),
        _ => write_stamp(&dir, movie, StampSource::Disc),
    }
    Ok(cues)
}

/// Cue count, as a cheap sanity check that the extraction produced something.
fn movie_subtitles_len(srt: &str) -> usize {
    srt.lines().filter(|l| l.contains("-->")).count()
}

fn extract(out: PathBuf, jobs: usize, limit: usize) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut todo: Vec<Movie> = plan
        .into_iter()
        .filter(|m| matches!(m.source, Source::DiscText { .. } | Source::Sidecar { .. }))
        .filter(|m| {
            !(matches!(m.source, Source::Sidecar { .. })
                && SIDECAR_UNTRUSTED.contains(&m.imdb_id.as_str()))
        })
        .collect();
    if limit > 0 {
        todo.truncate(limit);
    }
    println!("{} movies have a text subtitle already", todo.len());

    let results = parallel(todo, jobs, "extracting", |m| {
        (m.imdb_id.clone(), m.title.clone(), extract_one(m, &out))
    });

    let mut ok = 0usize;
    let mut cues = 0usize;
    for (imdb, title, r) in &results {
        match r {
            Ok(n) => {
                ok += 1;
                cues += n;
            }
            Err(e) => println!("  ✗ {imdb} {}: {e}", truncate(title, 40)),
        }
    }
    println!(
        "\nextracted {ok}/{} subtitles, {cues} cues total",
        results.len()
    );
    println!("into {}", out.display());
    Ok(())
}

/// What happened to one film in an `extract-audio` pass.
enum AudioOutcome {
    Extracted(sync::AudioStreamIdentity),
    Current,
    NoStream,
    Failed(anyhow::Error),
}

/// Pull the original-language audio track out of one film as `audio.opus`.
///
/// The artifact lives in the corpus, not beside the video: it is internal
/// substrate (whisper windows, VAD, future audio work), not part of the media
/// collection. Channels are kept — dialogue lives in the centre channel of a
/// surround mix, and folding it away now would close that door — and the
/// original timeline is preserved, so a timestamp in the opus *is* a timestamp
/// in the film.
fn extract_audio_one(movie: &Movie, dir: &std::path::Path) -> AudioOutcome {
    let codes = library::stream_codes(&movie.original_language);
    let Ok(stream) = sync::original_audio_stream(&movie.path, codes) else {
        return AudioOutcome::NoStream;
    };
    let identity = match sync::audio_stream_identity(&movie.path, stream) {
        Ok(i) => i,
        Err(e) => return AudioOutcome::Failed(e),
    };
    let current = match film_stamp(movie) {
        Ok(s) => s,
        Err(e) => return AudioOutcome::Failed(e),
    };
    if dir.join("audio.opus").exists() {
        if let Some(stamp) = read_audio_stamp(dir) {
            let recorded = FilmStamp {
                filename: stamp.filename.clone(),
                duration_ms: stamp.duration_ms,
                subtitle: None,
            };
            if recorded.matches(&current) && stamp.stream == identity {
                return AudioOutcome::Current;
            }
        }
    }

    let result = (|| -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("audio.opus.tmp");
        // ~64 kbps per channel is transparent-enough opus for speech work;
        // libopus rejects ffmpeg's "(side)" surround names, so aformat maps
        // each layout onto the nearest one opus can carry.
        let bitrate = (u64::from(identity.channels.max(1)) * 64_000).min(510_000);
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&movie.path)
            .args([
                "-map",
                &format!("0:a:{stream}"),
                "-vn",
                "-sn",
                "-af",
                "aformat=channel_layouts=7.1|6.1|5.1|5.0|quad|3.0|stereo|mono",
                "-c:a",
                "libopus",
                "-b:a",
                &bitrate.to_string(),
                "-f",
                "ogg",
            ])
            .arg(&tmp)
            .status()
            .context("ffmpeg failed to start")?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            bail!("ffmpeg exited with {status}");
        }
        std::fs::rename(&tmp, dir.join("audio.opus"))?;
        let stamp = AudioStamp {
            filename: current.filename.clone(),
            duration_ms: current.duration_ms,
            stream: identity.clone(),
        };
        std::fs::write(dir.join("audio.json"), serde_json::to_vec_pretty(&stamp)?)?;
        write_stamp(dir, movie, StampSource::Keep);
        Ok(())
    })();
    match result {
        Ok(()) => AudioOutcome::Extracted(identity),
        Err(e) => AudioOutcome::Failed(e),
    }
}

fn extract_audio(out: PathBuf, jobs: usize, limit: usize, imdb: Option<&str>) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut todo: Vec<Movie> = plan
        .into_iter()
        .filter(|m| imdb.is_none_or(|id| m.imdb_id == id))
        .filter(|m| m.path.exists())
        .collect();
    if limit > 0 {
        todo.truncate(limit);
    }
    println!("{} films to check for extracted audio", todo.len());

    let results = parallel(todo, jobs, "extracting audio", |m| {
        let outcome = extract_audio_one(m, &out.join(&m.imdb_id));
        if let AudioOutcome::Extracted(id) = &outcome {
            println!(
                "  {} ✓ {} {}ch → opus",
                truncate(&m.title, 40),
                id.codec,
                id.channels
            );
        }
        (m.imdb_id.clone(), m.title.clone(), outcome)
    });

    let mut extracted = 0usize;
    let mut current = 0usize;
    let mut no_stream = 0usize;
    for (imdb, title, outcome) in &results {
        match outcome {
            AudioOutcome::Extracted(_) => extracted += 1,
            AudioOutcome::Current => current += 1,
            AudioOutcome::NoStream => no_stream += 1,
            AudioOutcome::Failed(e) => println!("  ✗ {imdb} {}: {e:#}", truncate(title, 40)),
        }
    }
    println!(
        "\n{extracted} tracks extracted, {current} already current, {no_stream} with no original-language stream"
    );
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

/// Measure what OCR of the whole library would cost, on a sample.
///
/// Tokens scale with image pixels, so the estimate has to come from real cue
/// images at their real sizes rather than a guess. Movies are sampled across
/// the queue and cues across each film, since the opening minutes (titles,
/// credits) are not representative of dialogue.
#[tokio::main]
async fn ocr_sample(out: PathBuf, movies: usize, cues: usize, model: String) -> Result<()> {
    let plan = read_plan(&out)?;
    let queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| matches!(m.source, Source::DiscBitmap { .. }))
        .collect();
    let picked: Vec<Movie> = ocr::spread(&queue, movies).into_iter().cloned().collect();
    println!(
        "{} movies need OCR; sampling {} of them, {cues} cues each\n",
        queue.len(),
        picked.len()
    );

    let client = ocr::client(&model)?;
    let mut total_cues = 0usize;
    let mut png_bytes = 0usize;
    let mut pixels = 0u64;
    let mut transcribed = 0usize;
    let mut shown = Vec::new();

    for movie in &picked {
        let Source::DiscBitmap { index, .. } = &movie.source else {
            continue;
        };
        let sup = ocr::sup_path(&out, &movie.imdb_id);
        eprintln!(
            "  extracting {} ({})",
            truncate(&movie.title, 40),
            movie.imdb_id
        );
        if let Err(e) = ocr::extract_sup(&movie.path, *index, &sup) {
            println!("  ✗ {}: {e}", movie.imdb_id);
            continue;
        }
        let images = ocr::cue_images(&sup)?;
        total_cues += images.len();
        println!(
            "  {} — {} text cues",
            truncate(&movie.title, 40),
            images.len()
        );

        for img in ocr::spread(&images, cues) {
            png_bytes += img.png.len();
            pixels += img.width as u64 * img.height as u64;
            match ocr::transcribe(&client, &img.png).await {
                Ok(t) => {
                    transcribed += 1;
                    if shown.len() < 10 && !t.not_text {
                        shown.push((movie.imdb_id.clone(), t.text.clone()));
                    }
                }
                Err(e) => println!("    ✗ transcribe failed: {e}"),
            }
        }
    }

    if transcribed == 0 {
        bail!("no cues were transcribed — nothing to estimate from");
    }

    let usage = client.usage();
    let cost = client.cost();
    let per_cue_cost = cost.map(|c| c / transcribed as f64);
    let library_cues: usize = if picked.is_empty() {
        0
    } else {
        total_cues / picked.len() * queue.len()
    };

    println!("\n──────── sample ────────");
    for (imdb, text) in &shown {
        println!("  {imdb}  {:?}", truncate(text, 60));
    }
    println!("\n──────── measured ────────");
    println!("cues transcribed      {transcribed}");
    println!(
        "mean image            {:.0} px, {:.1} KiB PNG",
        pixels as f64 / transcribed as f64,
        png_bytes as f64 / transcribed as f64 / 1024.0
    );
    println!(
        "tokens                {} prompt, {} total  ({:.0} prompt/cue)",
        usage.prompt_tokens,
        usage.total_tokens,
        usage.prompt_tokens as f64 / transcribed as f64
    );
    match (cost, per_cue_cost) {
        (Some(c), Some(pc)) => {
            println!("cost                  ${c:.4} for the sample  (${pc:.6}/cue)");
            println!("\n──────── extrapolated ────────");
            println!("~{library_cues} cues across {} movies", queue.len());
            println!("estimated total       ${:.2}", pc * library_cues as f64);
            println!("  (Batch API halves this; caching makes any re-run free)");
        }
        _ => println!("cost                  unknown — {model} not in the price table"),
    }
    Ok(())
}

/// Read every bitmap track in the library back into text.
///
/// Resumable at movie granularity by the finished `subtitle.srt`, and at cue
/// granularity by tysm's response cache — an interrupted run re-reads nothing
/// it already paid for.
#[tokio::main]
async fn ocr_all(
    out: PathBuf,
    model: String,
    films_in_flight: usize,
    limit: usize,
    allow_unreadable: usize,
) -> Result<()> {
    use futures::stream::StreamExt;
    use std::sync::Arc;

    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| matches!(m.source, Source::DiscBitmap { .. }))
        .filter(|m| !out.join(&m.imdb_id).join("subtitle.srt").exists())
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!("{total} movies still need OCR, {films_in_flight} batches in flight");

    let client = Arc::new(ocr::client(&model)?);
    let out = Arc::new(out);
    let progress = AtomicUsize::new(0);

    // Only the verdict per film is needed; the name is printed as it lands.
    let results: Vec<Result<(usize, usize)>> = futures::stream::iter(queue.into_iter())
        .map(|movie| {
            let client = Arc::clone(&client);
            let out = Arc::clone(&out);
            let progress = &progress;
            async move {
                let outcome = ocr_one(&client, &movie, &out, allow_unreadable).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                match &outcome {
                    Ok((lines, cues)) => println!(
                        "[{n}/{total}] {} ✓ {lines} lines (of {cues} cues)",
                        truncate(&movie.title, 42)
                    ),
                    Err(e) => println!("[{n}/{total}] {} ✗ {e}", truncate(&movie.title, 42)),
                }
                outcome
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    let done = results.iter().filter(|r| r.is_ok()).count();
    println!("\n{done} transcribed, {} failed", results.len() - done);
    if let Some(cost) = client.cost() {
        println!("spent ${cost:.2}");
    }
    Ok(())
}

/// OCR one film: extract its bitmap track, read every cue in a single batch,
/// and write the SRT with the disc's own timings.
async fn ocr_one(
    client: &tysm::chat_completions::ChatClient,
    movie: &Movie,
    out: &std::path::Path,
    allow_unreadable: usize,
) -> Result<(usize, usize)> {
    let Source::DiscBitmap { index, codec } = &movie.source else {
        bail!("not a bitmap source");
    };
    let sup = ocr::sup_path(out, &movie.imdb_id);

    // Reading a whole film blocks its thread for minutes; keep it off the
    // async runtime so other films' batches keep progressing.
    let images = if codec == "dvd_subtitle" {
        let (video, index) = (movie.path.clone(), *index);
        tokio::task::spawn_blocking(move || ocr::vobsub_cue_images(&video, index))
            .await?
            .context("decode")?
    } else {
        let (video, index, sup_for_task) = (movie.path.clone(), *index, sup.clone());
        tokio::task::spawn_blocking(move || ocr::extract_sup(&video, index, &sup_for_task))
            .await?
            .context("extract")?;
        ocr::cue_images(&sup).context("decode")?
    };
    if images.is_empty() {
        bail!("no text cues in the bitmap track");
    }

    // One batch per film. Half the price of live requests, and a film's cues are
    // a natural unit: wanted together, finished together, and a failed batch
    // costs exactly one film's retry. tysm consults the same response cache
    // first, so cues already read are never resubmitted.
    let requests: Vec<_> = images
        .iter()
        .map(|img| ocr::messages_for(&img.png))
        .collect();
    let results = client
        .batch_chat_with_messages::<ocr::Transcription>(requests, |_| {})
        .await
        .map_err(|e| anyhow::anyhow!("batch: {e}"))?;

    let unreadable = results.iter().filter(|r| r.is_err()).count();
    let lines: Vec<(u32, u32, String)> = std::iter::zip(&images, results)
        .filter_map(|(img, r)| {
            let t = r.ok()?;
            let text = t.text.trim().to_string();
            (!t.not_text && !text.is_empty()).then_some((img.start_ms, img.end_ms, text))
        })
        .collect();

    // Leaving the film unwritten is what makes it retry on a later run, and on
    // that run tysm serves every cue already read from cache, so only the
    // failures are resubmitted. Repeated runs therefore converge on a complete
    // film for almost nothing — which is why the default tolerance is zero
    // rather than a percentage. A dropped cue is invisible in the output, so
    // accepting even a few means silently losing dialogue nothing goes back for.
    if unreadable > allow_unreadable {
        bail!(
            "{unreadable}/{} cues unreadable — left for a retry",
            images.len()
        );
    }
    if lines.is_empty() {
        bail!("no text recovered from {} cues", images.len());
    }

    std::fs::write(
        out.join(&movie.imdb_id).join("subtitle.srt"),
        ocr::to_srt(&lines),
    )?;
    write_stamp(&out.join(&movie.imdb_id), movie, StampSource::Disc);
    // The .sup is large and fully derived from the film; the SRT replaces it.
    let _ = std::fs::remove_file(&sup);
    Ok((lines.len(), images.len()))
}

/// OCR one standalone bitmap subtitle file into an SRT. The same read-it-all
/// batch as `ocr_one`, without a film attached: cached cues are free on a
/// rerun, so a run with unreadable cues converges by being repeated.
#[tokio::main]
async fn ocr_file(
    input: PathBuf,
    index: u32,
    model: String,
    srt: PathBuf,
    allow_unreadable: usize,
) -> Result<()> {
    let client = ocr::client(&model)?;
    let images = if input.extension().is_some_and(|e| e == "sup") {
        ocr::cue_images(&input).context("decode")?
    } else {
        ocr::vobsub_cue_images(&input, index).context("decode")?
    };
    if images.is_empty() {
        bail!("no text cues in the bitmap track");
    }
    println!("{} cues to read", images.len());

    let requests: Vec<_> = images
        .iter()
        .map(|img| ocr::messages_for(&img.png))
        .collect();
    let results = client
        .batch_chat_with_messages::<ocr::Transcription>(requests, |_| {})
        .await
        .map_err(|e| anyhow::anyhow!("batch: {e}"))?;

    let unreadable = results.iter().filter(|r| r.is_err()).count();
    let lines: Vec<(u32, u32, String)> = std::iter::zip(&images, results)
        .filter_map(|(img, r)| {
            let t = r.ok()?;
            let text = t.text.trim().to_string();
            (!t.not_text && !text.is_empty()).then_some((img.start_ms, img.end_ms, text))
        })
        .collect();
    if unreadable > allow_unreadable {
        bail!(
            "{unreadable}/{} cues unreadable — rerun to retry just those",
            images.len()
        );
    }
    if lines.is_empty() {
        bail!("no text recovered from {} cues", images.len());
    }

    std::fs::write(&srt, ocr::to_srt(&lines))?;
    println!(
        "{} lines (of {} cues) → {}",
        lines.len(),
        images.len(),
        srt.display()
    );
    if let Some(cost) = client.cost() {
        println!("spent ${cost:.2}");
    }
    Ok(())
}

/// Align each downloadable subtitle to the film on disk and write it out.
/// The knobs that decide how hard to listen and how sure to be.
#[derive(Clone, Copy)]
struct SyncOptions {
    windows: usize,
    window_secs: u32,
    max_residual_ms: f64,
    min_agreement: f64,
    debug_anchors: bool,
}

/// The anchor scatter, one line per anchor: where in the subtitle it sits and
/// how far the audio disagrees. Reading the shape tells failure modes apart —
/// a flat band is a plain offset, a slope is a rate, a staircase is a splice
/// (ad breaks, an extended scene), and shotgun noise is a wrong subtitle.
fn print_anchor_scatter(title: &str, anchors: &[sync::Anchor]) {
    let mut sorted: Vec<_> = anchors.iter().collect();
    sorted.sort_by_key(|a| a.subtitle_ms);
    println!("      anchor scatter for {title} (subtitle time → spoken-subtitle delta):");
    for a in sorted {
        let s = a.subtitle_ms / 1000;
        println!(
            "        {:>3}:{:02}  {:+7.2}s",
            s / 60,
            s % 60,
            (a.spoken_ms - a.subtitle_ms) as f64 / 1000.0
        );
    }
}

#[tokio::main]
async fn sync_all(
    out: PathBuf,
    data_root: PathBuf,
    films_in_flight: usize,
    limit: usize,
    opts: SyncOptions,
) -> Result<()> {
    use futures::stream::StreamExt;
    use std::sync::Arc;

    // Fail here rather than one window at a time — see `check_all`.
    let account = Arc::new(sync::WhisperAccount::from_env()?);

    let plan = read_plan(&out)?;
    let mut queue: Vec<(Movie, PathBuf)> = Vec::new();
    let mut parked = 0usize;
    for movie in plan {
        let dir = out.join(&movie.imdb_id);
        if dir.join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                if sync_already_failed(&movie, &dir, &raw) {
                    parked += 1;
                } else {
                    queue.push((movie, raw));
                }
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!(
        "{total} films have a subtitle to align, {films_in_flight} at a time{}",
        parked_note(parked)
    );

    let http = Arc::new(reqwest::Client::new());
    let out = Arc::new(out);
    let progress = AtomicUsize::new(0);

    let results: Vec<bool> = futures::stream::iter(queue.into_iter())
        .map(|(movie, raw)| {
            let http = Arc::clone(&http);
            let account = Arc::clone(&account);
            let out = Arc::clone(&out);
            let progress = &progress;
            async move {
                let outcome = sync_one(&http, &account, &movie, &raw, &out, opts).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                match &outcome {
                    Ok(a) => println!(
                        "[{n}/{total}] {} ✓ {:+.2}s{} ({}/{} anchors, worst {:.0}ms)",
                        truncate(&movie.title, 34),
                        a.offset_ms / 1000.0,
                        if (a.rate - 1.0).abs() > 1e-5 {
                            format!(", rate {:.5}", a.rate)
                        } else {
                            String::new()
                        },
                        a.anchors_used,
                        a.anchors_seen,
                        a.worst_residual_ms
                    ),
                    Err(e) => println!("[{n}/{total}] {} ✗ {e}", truncate(&movie.title, 34)),
                }
                outcome.is_ok()
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    let done = results.iter().filter(|ok| **ok).count();
    println!("\n{done} aligned, {} left unaligned", results.len() - done);
    Ok(())
}

/// Align one film: transcribe a few windows, match lines, fit, write.
async fn sync_one(
    http: &reqwest::Client,
    account: &sync::WhisperAccount,
    movie: &Movie,
    raw_srt: &std::path::Path,
    out: &std::path::Path,
    opts: SyncOptions,
) -> Result<sync::Alignment> {
    let cues = sync::parse_cues(&std::fs::read_to_string(raw_srt)?);
    if cues.is_empty() {
        bail!("subtitle has no cues");
    }
    let (media, stream) = audio_source(movie, &out.join(&movie.imdb_id))?;
    let duration = sync::duration_ms(&movie.path)?;
    let language = library::course_dir(&movie.original_language)
        .and_then(whisper_language)
        .unwrap_or("en");

    // Spread the windows across the body of the film. Openings are logos and
    // credits, endings are credits again — neither carries much dialogue, and
    // anchors clustered at one end cannot reveal a rate.
    let mut heard = Vec::new();
    for at in sync::choose_windows(&cues, duration, opts.windows, opts.window_secs) {
        match sync::transcribe_window(
            http,
            account,
            &media,
            stream,
            at,
            opts.window_secs,
            language,
        )
        .await
        {
            Ok(words) => heard.extend(words),
            // One refused window is survivable; the fit needs several anyway.
            Err(e) => eprintln!("      window at {}s failed: {e}", at / 1000),
        }
    }
    if heard.is_empty() {
        bail!("no audio could be transcribed");
    }

    // A subtitle that covers only part of the film cannot be placed reliably:
    // there is no way to tell a correctly-timed first-half subtitle from the
    // same file shifted onto the second half, and the anchors are too few to
    // arbitrate. Lust, Caution covered 46 of 158 minutes — a "CD1" subtitle —
    // and was confidently shifted 104 minutes to the end of the film with 88%
    // of its anchors agreeing. Films with genuinely long silent stretches
    // (2001 spans 61%) still clear this.
    let span = cues.iter().map(|c| c.end_ms).max().unwrap_or(0)
        - cues.iter().map(|c| c.start_ms).min().unwrap_or(0);
    if duration > 0 && (span as f64) < 0.5 * duration as f64 {
        bail!(
            "subtitle covers only {:.0}% of the film — partial, cannot be placed",
            span as f64 / duration as f64 * 100.0
        );
    }

    let anchors = sync::find_anchors(&cues, &heard, 4);
    if opts.debug_anchors {
        print_anchor_scatter(&movie.title, &anchors);
    }
    let Some(alignment) = sync::fit(&anchors, 3000.0) else {
        bail!("only {} anchors, too few to trust", anchors.len());
    };
    // A poor fit means the anchors disagree about what the shift is, which is
    // how a wrong match or a different cut of the film shows up. Writing a
    // plausible-looking wrong alignment is worse than writing none.
    let agreement = alignment.anchors_used as f64 / alignment.anchors_seen.max(1) as f64;
    if agreement < opts.min_agreement {
        bail!(
            "only {:.0}% of {} anchors agree on the shift",
            agreement * 100.0,
            alignment.anchors_seen
        );
    }
    if alignment.worst_residual_ms > opts.max_residual_ms {
        bail!(
            "anchors disagree by {:.0}ms (limit {:.0})",
            alignment.worst_residual_ms,
            opts.max_residual_ms
        );
    }

    // The aligned subtitle must fit the film it claims to describe. A fit can
    // be internally consistent and still absurd — anchors agreeing on a shift
    // that pushes the last line past the end credits — and this catches that
    // for the price of one comparison.
    let last = cues.iter().map(|c| c.end_ms).max().unwrap_or(0);
    let aligned_end = alignment.apply(last);
    if aligned_end > duration + 120_000 || alignment.apply(cues[0].start_ms) < -60_000 {
        bail!(
            "alignment puts the subtitle at {:.1}–{:.1} min of a {:.1} min film",
            alignment.apply(cues[0].start_ms) as f64 / 60_000.0,
            aligned_end as f64 / 60_000.0,
            duration as f64 / 60_000.0
        );
    }

    let shifted: Vec<sync::Cue> = cues
        .iter()
        .map(|c| sync::Cue {
            start_ms: alignment.apply(c.start_ms),
            end_ms: alignment.apply(c.end_ms),
            text: c.text.clone(),
        })
        .collect();
    let dir = out.join(&movie.imdb_id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("subtitle.srt"), sync::write_cues(&shifted))?;
    write_stamp(&dir, movie, StampSource::File(raw_srt));
    Ok(alignment)
}

/// How far a confirmed subtitle may sit from where Whisper heard the words.
///
/// The subtitle being checked is already aligned, so a truthful fit is the
/// identity — up to Whisper's own clock, which skews ~0.6s on tracks VAD
/// places within ±0.2s. Half a second is Whisper noise, not a finding; 1.5s
/// is a clip landing on the wrong dialogue.
const CHECK_MAX_OFFSET_MS: f64 = 1500.0;
/// Drift the whole file shares, rather than a constant displacement.
const CHECK_MAX_RATE_ERROR: f64 = 5e-4;

/// What one cross-examination measured. Deliberately holds no verdict.
///
/// An earlier version stored the verdict beside the evidence, and `check`
/// skips films already in the ledger — so when the offset gate moved to 1.5s
/// every row written under the older, stricter rule kept its old label
/// forever. 84 films read as `contradicted` while their own recorded numbers
/// said otherwise. Storing only what was measured means moving a threshold
/// re-labels the whole corpus at once, and repairing a film no longer needs
/// its row purged by hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckRow {
    imdb_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    tier: String,
    #[serde(flatten)]
    outcome: CheckOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum CheckOutcome {
    /// Anchors placed the subtitle; these are the fit's terms.
    Fit {
        offset_ms: f64,
        rate: f64,
        anchors_used: usize,
        anchors_seen: usize,
        worst_residual_ms: f64,
    },
    /// No fit was possible — sparse or musical films starve the anchors,
    /// exactly like the VAD margin going flat. Not a contradiction.
    NoFit { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Confirmed,
    Contradicted,
    Undecided,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Confirmed => "confirmed",
            Verdict::Contradicted => "contradicted",
            Verdict::Undecided => "undecided",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            Verdict::Confirmed => "✓",
            Verdict::Contradicted => "✗",
            Verdict::Undecided => "?",
        }
    }
}

impl CheckRow {
    /// The verdict today's thresholds give this evidence.
    fn verdict(&self) -> Verdict {
        match &self.outcome {
            CheckOutcome::NoFit { .. } => Verdict::Undecided,
            CheckOutcome::Fit {
                offset_ms, rate, ..
            } => {
                let offset_ok = offset_ms.abs() <= CHECK_MAX_OFFSET_MS;
                let rate_ok = (rate - 1.0).abs() < CHECK_MAX_RATE_ERROR;
                if offset_ok && rate_ok {
                    Verdict::Confirmed
                } else {
                    Verdict::Contradicted
                }
            }
        }
    }

    fn detail(&self) -> String {
        match &self.outcome {
            CheckOutcome::NoFit { reason } => reason.clone(),
            CheckOutcome::Fit {
                offset_ms,
                rate,
                anchors_used,
                anchors_seen,
                worst_residual_ms,
            } => format!(
                "{:+.2}s rate {rate:.4} ({anchors_used}/{anchors_seen} anchors, worst {worst_residual_ms:.0}ms)",
                offset_ms / 1000.0
            ),
        }
    }
}

/// Every cross-examination recorded so far.
///
/// Rows written before the verdict was derived still carry a `verdict` field;
/// it is ignored, and their measurements re-judged like everything else.
fn read_check_log(path: &std::path::Path) -> Vec<CheckRow> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<CheckRow>(line).ok())
        .collect()
}

/// One row per film — the most recent, since the ledger is append-only.
///
/// A film re-checked after a repair leaves both measurements on disk, and
/// counting the file line by line reports it twice, once under each verdict.
/// The latest row is the one that describes the subtitle as it stands.
fn latest_per_film(rows: &[CheckRow]) -> Vec<&CheckRow> {
    let mut newest: std::collections::HashMap<&str, &CheckRow> = Default::default();
    for row in rows {
        newest.insert(row.imdb_id.as_str(), row);
    }
    let mut rows: Vec<&CheckRow> = newest.into_values().collect();
    rows.sort_by(|a, b| a.imdb_id.cmp(&b.imdb_id));
    rows
}

#[tokio::main]
async fn check_all(
    out: PathBuf,
    tier: Option<String>,
    windows: usize,
    window_secs: u32,
    films_in_flight: usize,
    limit: usize,
) -> Result<()> {
    use futures::stream::StreamExt;
    use std::sync::Arc;

    // Before any film is touched: a run without credentials would fail every
    // window of every film and record each one `undecided`, which reads as
    // "unverifiable" forever after.
    let account = Arc::new(sync::WhisperAccount::from_env()?);

    let log_path = out.join("whisper-check.jsonl");
    let existing = read_check_log(&log_path);
    let done: std::collections::HashSet<String> =
        existing.iter().map(|r| r.imdb_id.clone()).collect();

    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| out.join(&m.imdb_id).join("subtitle.srt").exists())
        .filter(|m| tier.as_deref().is_none_or(|t| m.source.label().contains(t)))
        .filter(|m| !done.contains(&m.imdb_id) && m.path.exists())
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!(
        "{total} aligned films to cross-examine ({} already checked)",
        done.len()
    );

    let log = Arc::new(Mutex::new(std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    )));
    let http = Arc::new(reqwest::Client::new());
    let out = Arc::new(out);
    let progress = AtomicUsize::new(0);

    let fresh: Vec<CheckRow> = futures::stream::iter(queue.into_iter())
        .map(|movie| {
            let http = Arc::clone(&http);
            let account = Arc::clone(&account);
            let out = Arc::clone(&out);
            let log = Arc::clone(&log);
            let progress = &progress;
            async move {
                let outcome = check_one(&http, &account, &movie, &out, windows, window_secs).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                let row = CheckRow {
                    imdb_id: movie.imdb_id.clone(),
                    title: movie.title.clone(),
                    tier: movie.source.label().to_string(),
                    outcome: match &outcome {
                        Ok(a) => CheckOutcome::Fit {
                            offset_ms: a.offset_ms,
                            rate: a.rate,
                            anchors_used: a.anchors_used,
                            anchors_seen: a.anchors_seen,
                            worst_residual_ms: a.worst_residual_ms,
                        },
                        // Recorded so the film is not re-transcribed next run.
                        Err(e) => CheckOutcome::NoFit {
                            reason: e.to_string(),
                        },
                    },
                };
                let verdict = row.verdict();
                println!(
                    "[{n}/{total}] {} {} {}: {}",
                    truncate(&movie.title, 34),
                    verdict.mark(),
                    verdict.label(),
                    row.detail()
                );
                {
                    use std::io::Write;
                    let mut log = log.lock().unwrap();
                    let _ = serde_json::to_writer(&mut *log, &row);
                    let _ = writeln!(log);
                    let _ = log.flush();
                }
                row
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    // Report over the whole ledger, not just this run: the verdicts are
    // derived, so every film's standing reflects today's thresholds whether
    // or not it was re-transcribed.
    let all: Vec<CheckRow> = existing.into_iter().chain(fresh).collect();
    let all = latest_per_film(&all);
    let count = |v: Verdict| all.iter().filter(|r| r.verdict() == v).count();
    println!(
        "\n{} confirmed, {} contradicted, {} undecided across {} films — details in {}",
        count(Verdict::Confirmed),
        count(Verdict::Contradicted),
        count(Verdict::Undecided),
        all.len(),
        log_path.display()
    );
    Ok(())
}

/// Fit Whisper anchors against a film's *already aligned* subtitle.
async fn check_one(
    http: &reqwest::Client,
    account: &sync::WhisperAccount,
    movie: &Movie,
    out: &std::path::Path,
    windows: usize,
    window_secs: u32,
) -> Result<sync::Alignment> {
    let srt = out.join(&movie.imdb_id).join("subtitle.srt");
    let cues = sync::parse_cues(&std::fs::read_to_string(&srt)?);
    if cues.is_empty() {
        bail!("subtitle has no cues");
    }
    // Deliberately the video, not the extracted opus: check is the last line
    // of defense that the placed subtitle fits the file clips are cut from.
    // Syncing against the extraction and checking against the original means
    // a defect in the extraction's timeline gets caught instead of ratified.
    let codes = library::stream_codes(&movie.original_language);
    let stream = sync::original_audio_stream(&movie.path, codes)?;
    let duration = sync::duration_ms(&movie.path)?;
    let language = library::course_dir(&movie.original_language)
        .and_then(whisper_language)
        .unwrap_or("en");

    let mut heard = Vec::new();
    for at in sync::choose_windows(&cues, duration, windows, window_secs) {
        match sync::transcribe_window(
            http,
            account,
            &movie.path,
            stream,
            at,
            window_secs,
            language,
        )
        .await
        {
            Ok(words) => heard.extend(words),
            Err(e) => eprintln!("      window at {}s failed: {e}", at / 1000),
        }
    }
    if heard.is_empty() {
        bail!("no audio could be transcribed");
    }
    let anchors = sync::find_anchors(&cues, &heard, 4);
    let Some(alignment) = sync::fit(&anchors, 3000.0) else {
        bail!("only {} anchors, too few to trust", anchors.len());
    };
    let agreement = alignment.anchors_used as f64 / alignment.anchors_seen.max(1) as f64;
    if agreement < 0.35 {
        bail!(
            "only {:.0}% of {} anchors agree",
            agreement * 100.0,
            alignment.anchors_seen
        );
    }
    Ok(alignment)
}

/// Whisper's language hint for one of our language packs.
fn whisper_language(course: &str) -> Option<&'static str> {
    Some(match course {
        "eng" => "en",
        "fra" => "fr",
        "deu" => "de",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "rus" => "ru",
        "jpn" => "ja",
        "kor" => "ko",
        "tha" => "th",
        "hin" => "hi",
        "zho-hans" => "zh",
        _ => return None,
    })
}

/// Transcribe one film in full and write it beside the subtitle.
/// Does this film need transcribing — because it has none, or because the one
/// it has was made under settings we no longer use?
///
/// The chunk cache would make an unnecessary re-run nearly free, so the
/// tempting simplification is to drop this and always recompute. What stops
/// that is the "nearly": a cache key is a hash of the *decoded* samples, and
/// anything that perturbs decoding — a different ffmpeg, a different opus
/// decoder — turns a free re-run into the corpus billed again at full price.
/// So the artifact carries its own provenance and is trusted while it matches.
fn transcript_is_stale(movie: &Movie, dir: &std::path::Path) -> bool {
    let Some(stored) = transcript::stored_provenance(&dir.join("transcript.jsonl")) else {
        // No file, or one written before provenance was recorded.
        return true;
    };
    match library::course_dir(&movie.original_language)
        .and_then(whisper_language)
        .map(transcript::provenance)
    {
        Some(Ok(current)) => stored != current,
        // No language, or no provenance to compare against: there is nothing
        // this run could produce, so leave what is there alone.
        _ => false,
    }
}

async fn transcribe_one(
    http: &reqwest::Client,
    account: &transcript::ScribeAccount,
    store: &osmo::Store,
    movie: &Movie,
    out: &std::path::Path,
) -> Result<usize> {
    let dir = out.join(&movie.imdb_id);
    let language = library::course_dir(&movie.original_language)
        .and_then(whisper_language)
        .context("no Whisper language for this film's original language")?;
    let audio = extracted_audio(movie, &dir).context("no extracted audio")?;

    // The profile has to come from the whole film in one pass — see the
    // module docs on earshot's recurrence — which `cached_speech_profile`
    // already guarantees, and caches for everyone else.
    let profile = cached_speech_profile(movie, &dir)?;
    // The audio's own duration, not the video's: it is what we slice.
    let film_ms = sync::duration_ms(&audio)?;

    let transcript =
        transcript::transcribe_film(http, account, store, &audio, &profile, film_ms, language)
            .await?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("transcript.jsonl"), transcript.to_jsonl()?)?;
    write_stamp(&dir, movie, StampSource::Keep);
    Ok(transcript.words.len())
}

#[tokio::main]
async fn transcribe_all(
    out: PathBuf,
    films_in_flight: usize,
    limit: usize,
    imdb: Option<String>,
) -> Result<()> {
    use futures::stream::StreamExt;
    use std::sync::Arc;

    // Not the Whisper credentials the syncers use: transcription runs against
    // ElevenLabs. Fail here rather than once per chunk.
    let account = Arc::new(transcript::ScribeAccount::from_env()?);

    let plan = read_plan(&out)?;
    let mut queue: Vec<Movie> = plan
        .into_iter()
        .filter(|m| imdb.as_deref().is_none_or(|id| m.imdb_id == id))
        .filter(|m| out.join(&m.imdb_id).join("subtitle.srt").exists())
        .filter(|m| transcript_is_stale(m, &out.join(&m.imdb_id)))
        .filter(|m| extracted_audio(m, &out.join(&m.imdb_id)).is_some())
        .collect();
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!("{total} films to transcribe in full");
    if total == 0 {
        return Ok(());
    }

    // Opened only once there is work: this is the store generate-data fills,
    // 10M keys and 43GB of it, and opening it costs tens of seconds. A
    // refresh with every film already transcribed should cost nothing at all.
    // Sharing it is the point — a transcript outlives eviction of the
    // artifact and rides the R2 mirror to other machines.
    let store = Arc::new(osmo::Store::open("./.cache"));
    let http = Arc::new(reqwest::Client::new());
    let out = Arc::new(out);
    let progress = AtomicUsize::new(0);

    let done: Vec<bool> = futures::stream::iter(queue.into_iter())
        .map(|movie| {
            let http = Arc::clone(&http);
            let account = Arc::clone(&account);
            let store = Arc::clone(&store);
            let out = Arc::clone(&out);
            let progress = &progress;
            async move {
                let outcome = transcribe_one(&http, &account, &store, &movie, &out).await;
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                match &outcome {
                    Ok(words) => println!(
                        "[{n}/{total}] {} ✓ {words} words",
                        truncate(&movie.title, 34)
                    ),
                    Err(e) => println!("[{n}/{total}] {} ✗ {e:#}", truncate(&movie.title, 34)),
                }
                outcome.is_ok()
            }
        })
        .buffer_unordered(films_in_flight.max(1))
        .collect()
        .await;

    println!(
        "\n{} transcribed, {} failed",
        done.iter().filter(|ok| **ok).count(),
        done.iter().filter(|ok| !**ok).count()
    );
    Ok(())
}

/// Score each subtitle against where the audio says people are talking.
fn agreement(
    out: PathBuf,
    tier: Option<String>,
    limit: usize,
    jobs: usize,
    range_secs: i64,
) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut films: Vec<Movie> = plan
        .into_iter()
        .filter(|m| out.join(&m.imdb_id).join("subtitle.srt").exists())
        .filter(|m| tier.as_deref().is_none_or(|t| m.source.label().contains(t)))
        .collect();
    if limit > 0 {
        films.truncate(limit);
    }
    println!(
        "scoring {} subtitles against their films' audio",
        films.len()
    );

    let rows = parallel(films, jobs, "scoring", |movie| {
        let srt = std::fs::read_to_string(out.join(&movie.imdb_id).join("subtitle.srt")).ok()?;
        let cues = sync::parse_cues(&srt);
        if cues.is_empty() {
            return None;
        }
        let speech = cached_speech_profile(movie, &out.join(&movie.imdb_id)).ok()?;
        let subtitle = vad::subtitle_profile(&cues, speech.len());
        Some((
            movie.title.clone(),
            movie.source.label(),
            vad::find_offset(&speech, &subtitle, range_secs * 1000),
        ))
    });

    let found: Vec<_> = rows.into_iter().flatten().collect();
    println!(
        "\n{:34}{:20}{:>9}{:>10}{:>9}",
        "film", "source", "shift", "agree", "margin"
    );
    println!("{}", "-".repeat(82));
    let mut aligned = 0;
    for (title, source, v) in &found {
        if v.offset_ms.abs() <= 500 {
            aligned += 1;
        }
        println!(
            "{:34}{:20}{:>8.1}s{:>10.2}{:>9.2}",
            truncate(title, 32),
            source,
            v.offset_ms as f64 / 1000.0,
            v.agreement,
            v.margin()
        );
    }
    println!(
        "\n{aligned}/{} already sit within 0.5s of where the speech is",
        found.len()
    );
    Ok(())
}

/// Align by speech activity the films that word-matching could not place.
fn vad_sync(
    out: PathBuf,
    data_root: PathBuf,
    jobs: usize,
    limit: usize,
    range_secs: i64,
    min_agreement: f32,
    min_margin: f32,
) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut queue: Vec<(Movie, PathBuf)> = Vec::new();
    let mut parked = 0usize;
    for movie in plan {
        let dir = out.join(&movie.imdb_id);
        if dir.join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                if sync_already_failed(&movie, &dir, &raw) {
                    parked += 1;
                } else {
                    queue.push((movie, raw));
                }
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    println!(
        "{} films left for speech-activity alignment{}",
        queue.len(),
        parked_note(parked)
    );

    let out = &out;
    let results = parallel(queue, jobs, "aligning", move |(movie, raw)| {
        let outcome = (|| -> Result<vad::VadOffset> {
            let cues = sync::parse_cues(&std::fs::read_to_string(raw)?);
            if cues.is_empty() {
                bail!("no cues");
            }
            let duration = sync::duration_ms(&movie.path)?;
            // A subtitle that covers only part of the film cannot be placed reliably:
            // there is no way to tell a correctly-timed first-half subtitle from the
            // same file shifted onto the second half, and the anchors are too few to
            // arbitrate. Lust, Caution covered 46 of 158 minutes — a "CD1" subtitle —
            // and was confidently shifted 104 minutes to the end of the film with 88%
            // of its anchors agreeing. Films with genuinely long silent stretches
            // (2001 spans 61%) still clear this.
            let span = cues.iter().map(|c| c.end_ms).max().unwrap_or(0)
                - cues.iter().map(|c| c.start_ms).min().unwrap_or(0);
            if duration > 0 && (span as f64) < 0.5 * duration as f64 {
                bail!(
                    "subtitle covers only {:.0}% of the film — partial, cannot be placed",
                    span as f64 / duration as f64 * 100.0
                );
            }

            let speech = cached_speech_profile(movie, &out.join(&movie.imdb_id))?;
            let subtitle = vad::subtitle_profile(&cues, speech.len());
            let found = vad::find_offset(&speech, &subtitle, range_secs * 1000);
            if found.agreement < min_agreement {
                bail!("speech matches weakly ({:.2})", found.agreement);
            }
            if found.margin() < min_margin {
                bail!(
                    "no clear peak: {:.2} vs {:.2} elsewhere",
                    found.agreement,
                    found.runner_up
                );
            }
            // Same sanity as the word-matching path: the result has to fit the
            // film it describes.
            let last = cues.iter().map(|c| c.end_ms).max().unwrap_or(0) + found.offset_ms;
            if last > duration + 120_000 || cues[0].start_ms + found.offset_ms < -60_000 {
                bail!("shift puts the subtitle outside the film");
            }
            let shifted: Vec<sync::Cue> = cues
                .iter()
                .map(|c| sync::Cue {
                    start_ms: c.start_ms + found.offset_ms,
                    end_ms: c.end_ms + found.offset_ms,
                    text: c.text.clone(),
                })
                .collect();
            let dir = out.join(&movie.imdb_id);
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join("subtitle.srt"), sync::write_cues(&shifted))?;
            write_stamp(&dir, movie, StampSource::File(raw));
            Ok(found)
        })();
        match &outcome {
            Ok(v) => println!(
                "  {} ✓ {:+.2}s (agree {:.2}, margin {:.2})",
                truncate(&movie.title, 34),
                v.offset_ms as f64 / 1000.0,
                v.agreement,
                v.margin()
            ),
            Err(e) => {
                println!("  {} ✗ {e}", truncate(&movie.title, 34));
                // vad-sync is the last gate: in refresh order the film has
                // just failed text-sync and sync too, so these inputs are a
                // proven dead end until one of them changes.
                record_sync_failure(movie, &out.join(&movie.imdb_id), raw);
            }
        }
        outcome.is_ok()
    });
    let done = results.iter().filter(|ok| **ok).count();
    println!(
        "\n{done} aligned by speech activity, {} still unaligned",
        results.len() - done
    );
    Ok(())
}

/// Read one reference track's cues — parsed text for a text stream, bare
/// timestamps for a bitmap one.
fn reference_cues(
    video: &std::path::Path,
    stream: &library::ReferenceStream,
    scratch: &std::path::Path,
) -> Result<Vec<sync::Cue>> {
    if stream.is_text {
        let tmp = scratch.with_extension("srt");
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(video)
            .args(["-map", &format!("0:{}", stream.index)])
            .arg(&tmp)
            .status()
            .context("ffmpeg failed to start")?;
        if !status.success() {
            bail!("ffmpeg could not convert stream {}", stream.index);
        }
        let cues = sync::parse_cues(&std::fs::read_to_string(&tmp)?);
        let _ = std::fs::remove_file(&tmp);
        Ok(cues)
    } else {
        let bitmaps = if stream.codec == "dvd_subtitle" {
            vobsub::cues(video, stream.index)?
        } else {
            let tmp = scratch.with_extension("sup");
            let _ = std::fs::remove_file(&tmp);
            ocr::extract_sup(video, stream.index, &tmp)?;
            let data = std::fs::read(&tmp)?;
            let _ = std::fs::remove_file(&tmp);
            pgs::cues(&data)
        };
        Ok(bitmaps
            .into_iter()
            .map(|c| sync::Cue {
                start_ms: c.start_ms as i64,
                end_ms: c.end_ms as i64,
                text: String::new(),
            })
            .collect())
    }
}

#[allow(clippy::too_many_arguments)]
fn text_sync(
    out: PathBuf,
    data_root: PathBuf,
    jobs: usize,
    limit: usize,
    range_secs: i64,
    min_agreement: f32,
    min_margin: f32,
    dry_run: bool,
) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut queue: Vec<(Movie, PathBuf)> = Vec::new();
    let mut parked = 0usize;
    for movie in plan {
        let dir = out.join(&movie.imdb_id);
        if dir.join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                if sync_already_failed(&movie, &dir, &raw) {
                    parked += 1;
                } else {
                    queue.push((movie, raw));
                }
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    println!(
        "{} films left to align against their discs' own tracks{}",
        queue.len(),
        parked_note(parked)
    );

    let out = &out;
    let results = parallel(queue, jobs, "aligning", move |(movie, raw)| {
        let outcome = (|| -> Result<(String, f64, vad::VadOffset, usize)> {
            let cues = sync::parse_cues(&std::fs::read_to_string(raw)?);
            if cues.is_empty() {
                bail!("no cues");
            }
            let duration = sync::duration_ms(&movie.path)?;
            // Same guard as the audio path: a partial subtitle cannot be
            // placed, only mistaken for a placed one.
            let span = cues.iter().map(|c| c.end_ms).max().unwrap_or(0)
                - cues.iter().map(|c| c.start_ms).min().unwrap_or(0);
            if duration > 0 && (span as f64) < 0.5 * duration as f64 {
                bail!(
                    "subtitle covers only {:.0}% of the film — partial, cannot be placed",
                    span as f64 / duration as f64 * 100.0
                );
            }
            let refs = library::reference_subtitle_streams(&movie.path)?;
            if refs.is_empty() {
                bail!("disc carries no usable subtitle track");
            }

            let buckets = (duration / vad::BUCKET_MS) as usize;
            // A subtitle authored against a PAL (25fps), cinema (24fps) or
            // NTSC-film (23.976fps) transfer of the same film runs fast or
            // slow by a constant factor. A pure shift search sees that as a
            // wide, flat peak — high agreement, no margin — so each rate is
            // searched as its own hypothesis and the sharpest peak wins. The
            // 0.1% cinema/NTSC pair matters despite its size: it is 5.4s of
            // drift across a feature (measured on Delicatessen), enough to
            // flatten the peak and drag the compromise offset seconds off at
            // both ends.
            const RATES: &[f64] = &[
                23.976 / 25.0,
                23.976 / 24.0,
                1.0,
                24.0 / 23.976,
                25.0 / 23.976,
            ];
            let candidates: Vec<(f64, Vec<f32>)> = RATES
                .iter()
                .map(|&rate| {
                    let scaled: Vec<sync::Cue> = cues
                        .iter()
                        .map(|c| sync::Cue {
                            start_ms: (rate * c.start_ms as f64) as i64,
                            end_ms: (rate * c.end_ms as f64) as i64,
                            text: String::new(),
                        })
                        .collect();
                    (rate, vad::subtitle_profile(&scaled, buckets))
                })
                .collect();
            // Each reference votes independently; they were all authored
            // against this file, so they must agree with each other. One
            // reference passing the gates is an answer, two agreeing is a
            // cross-check no single-method threshold can give.
            let mut votes: Vec<(String, f64, vad::VadOffset)> = Vec::new();
            for (n, stream) in refs.iter().take(4).enumerate() {
                let scratch = std::env::temp_dir().join(format!(
                    "textsync-{}-{}-{n}",
                    std::process::id(),
                    movie.imdb_id
                ));
                let Ok(ref_cues) = cached_reference_cues(movie, out, stream, &scratch) else {
                    continue;
                };
                let ref_span = ref_cues.iter().map(|c| c.end_ms).max().unwrap_or(0)
                    - ref_cues.iter().map(|c| c.start_ms).min().unwrap_or(0);
                // An unflagged forced track looks exactly like a real one
                // until you count its cues.
                if ref_cues.len() < 50 || (ref_span as f64) < 0.4 * duration as f64 {
                    continue;
                }
                let reference = vad::subtitle_profile(&ref_cues, buckets);
                // The best hypothesis is the one that *correlates* best. Margin
                // cannot arbitrate here: cue-vs-cue peaks are broad (a cue
                // lasts seconds, so a 2s-away shift still overlaps most of it),
                // leaving every hypothesis's margin near zero — and choosing
                // among near-ties by margin picks noise.
                let (rate, found) = candidates
                    .iter()
                    .map(|(rate, profile)| {
                        (
                            *rate,
                            vad::find_offset(&reference, profile, range_secs * 1000),
                        )
                    })
                    .max_by(|a, b| a.1.agreement.total_cmp(&b.1.agreement))
                    .expect("RATES is never empty");
                let label = format!(
                    "{}:{}",
                    if stream.language.is_empty() {
                        "und"
                    } else {
                        &stream.language
                    },
                    if stream.is_text { "text" } else { "pgs" }
                );
                if dry_run {
                    println!(
                        "    {} {label} {:+.2}s ×{rate:.4} (agree {:.2}, margin {:.2})",
                        truncate(&movie.title, 24),
                        found.offset_ms as f64 / 1000.0,
                        found.agreement,
                        found.margin()
                    );
                }
                votes.push((label, rate, found));
            }
            // Two ways to believe an answer. A single reference is enough when
            // its peak stands clear of every rival (margin). But cue-vs-cue
            // peaks are broad — a cue lasts seconds, so near shifts correlate
            // almost as well and margin stays near zero even when the answer
            // is right. What margin cannot supply, *independent agreement*
            // can: two tracks, authored separately against this same file,
            // naming the same offset within half a second is not chance.
            let confident: Vec<_> = votes
                .iter()
                .filter(|(_, _, v)| v.agreement >= min_agreement && v.margin() >= min_margin)
                .cloned()
                .collect();
            let chosen = if !confident.is_empty() {
                confident
            } else {
                /// Correlation each track must reach before its vote counts
                /// toward a consensus that overrides the margin gate.
                const CONSENSUS_AGREEMENT: f32 = 0.4;
                let strong: Vec<_> = votes
                    .iter()
                    .filter(|(_, _, v)| v.agreement >= CONSENSUS_AGREEMENT)
                    .cloned()
                    .collect();
                let within = |a: &vad::VadOffset, b: &vad::VadOffset| {
                    (a.offset_ms - b.offset_ms).abs() <= 500
                };
                let consensus: Vec<_> = strong
                    .iter()
                    .filter(|(_, r1, v1)| {
                        strong
                            .iter()
                            .filter(|(_, r2, v2)| r1 == r2 && within(v1, v2))
                            .count()
                            >= 2
                    })
                    .cloned()
                    .collect();
                if consensus.is_empty() {
                    bail!("no reference track produced a confident offset");
                }
                consensus
            };
            let (label, rate, best) = chosen
                .iter()
                .max_by(|a, b| a.2.agreement.total_cmp(&b.2.agreement))
                .cloned()
                .expect("chosen is non-empty");
            // The rate is a property of the candidate file, so two references
            // concluding different rates is as damning as two different
            // offsets.
            if chosen.iter().any(|(_, r, _)| *r != rate) {
                bail!("references disagree on the playback rate");
            }
            let spread = chosen
                .iter()
                .map(|(_, _, v)| v.offset_ms)
                .fold((i64::MAX, i64::MIN), |(lo, hi), o| (lo.min(o), hi.max(o)));
            if chosen.len() > 1 && spread.1 - spread.0 > 500 {
                bail!(
                    "references disagree: offsets span {:.1}s across {} tracks",
                    (spread.1 - spread.0) as f64 / 1000.0,
                    chosen.len()
                );
            }
            let votes = chosen;
            let place = |ms: i64| (rate * ms as f64) as i64 + best.offset_ms;
            let last = place(cues.iter().map(|c| c.end_ms).max().unwrap_or(0));
            if last > duration + 120_000 || place(cues[0].start_ms) < -60_000 {
                bail!("shift puts the subtitle outside the film");
            }
            if !dry_run {
                let shifted: Vec<sync::Cue> = cues
                    .iter()
                    .map(|c| sync::Cue {
                        start_ms: place(c.start_ms),
                        end_ms: place(c.end_ms),
                        text: c.text.clone(),
                    })
                    .collect();
                let dir = out.join(&movie.imdb_id);
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("subtitle.srt"), sync::write_cues(&shifted))?;
                write_stamp(&dir, movie, StampSource::File(raw));
            }
            Ok((label, rate, best, votes.len()))
        })();
        match &outcome {
            Ok((label, rate, v, n)) => println!(
                "  {} ✓ {:+.2}s ×{rate:.4} via {label} ({n} track(s) agree, agree {:.2}, margin {:.2})",
                truncate(&movie.title, 34),
                v.offset_ms as f64 / 1000.0,
                v.agreement,
                v.margin()
            ),
            Err(e) => println!("  {} ✗ {e}", truncate(&movie.title, 34)),
        }
        outcome.is_ok()
    });
    let done = results.iter().filter(|ok| **ok).count();
    println!(
        "\n{done} aligned against disc tracks, {} still unaligned",
        results.len() - done
    );
    Ok(())
}

/// Known perturbations to inflict on a correctly-timed subtitle:
/// `(name, shift_ms, rate)`. The shifts bracket what downloads actually show
/// (median 7.46s, max 203.8s); the rate cases are the PAL/NTSC speedups, which
/// `find_offset` cannot represent — those rows measure whether the gates
/// *refuse* them rather than whether they are solved.
const PERTURBATIONS: &[(&str, i64, f64)] = &[
    ("control", 0, 1.0),
    ("+2s", 2_000, 1.0),
    ("-2s", -2_000, 1.0),
    ("+7.5s", 7_500, 1.0),
    ("-7.5s", -7_500, 1.0),
    ("+30s", 30_000, 1.0),
    ("-30s", -30_000, 1.0),
    ("+90s", 90_000, 1.0),
    ("-90s", -90_000, 1.0),
    ("pal", 0, 25.0 / 23.976),
    ("ntsc", 0, 23.976 / 25.0),
    ("pal+10s", 10_000, 25.0 / 23.976),
    // The subtle pair: 0.1% is still 5.4s of drift across a feature.
    ("cinema", 0, 24.0 / 23.976),
    ("cinema-inv", 0, 23.976 / 24.0),
];

fn calibrate(out: PathBuf, jobs: usize, limit: usize, range_secs: i64) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut films: Vec<Movie> = plan
        .into_iter()
        .filter(|m| {
            matches!(
                m.source,
                Source::DiscText { .. } | Source::DiscBitmap { .. }
            )
        })
        .filter(|m| out.join(&m.imdb_id).join("subtitle.srt").exists() && m.path.exists())
        .collect();
    if limit > 0 {
        films.truncate(limit);
    }

    // Rows already measured survive across runs, so a stopped run resumes and
    // a finished one is free to re-run.
    let log_path = out.join("calibration.jsonl");
    let mut done: std::collections::HashSet<(String, String)> = Default::default();
    if let Ok(existing) = std::fs::read_to_string(&log_path) {
        for line in existing.lines() {
            if let Ok(row) = serde_json::from_str::<serde_json::Value>(line) {
                if let (Some(id), Some(p)) = (row["imdb_id"].as_str(), row["perturbation"].as_str())
                {
                    done.insert((id.to_string(), p.to_string()));
                }
            }
        }
    }
    films.retain(|m| {
        PERTURBATIONS
            .iter()
            .any(|(name, _, _)| !done.contains(&(m.imdb_id.clone(), name.to_string())))
    });
    println!(
        "calibrating against {} disc-sourced films ({} rows already measured)",
        films.len(),
        done.len()
    );

    let log = Mutex::new(std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    ));
    let out = &out;
    let done = &done;
    let log = &log;
    let counted = parallel(films, jobs, "calibrating", move |movie| {
        let outcome = (|| -> Result<usize> {
            let dir = out.join(&movie.imdb_id);
            let cues = sync::parse_cues(&std::fs::read_to_string(dir.join("subtitle.srt"))?);
            if cues.is_empty() {
                bail!("no cues");
            }
            let speech = cached_speech_profile(movie, &dir)?;
            let minutes = speech.len() as f64 * vad::BUCKET_MS as f64 / 60_000.0;

            let mut rows = 0;
            for (name, shift_ms, rate) in PERTURBATIONS {
                if done.contains(&(movie.imdb_id.clone(), name.to_string())) {
                    continue;
                }
                // A shift can push early cues before the start of the film;
                // a real subtitle timed that way would simply not have them.
                let perturbed: Vec<sync::Cue> = cues
                    .iter()
                    .map(|c| sync::Cue {
                        start_ms: (*rate * c.start_ms as f64) as i64 + shift_ms,
                        end_ms: (*rate * c.end_ms as f64) as i64 + shift_ms,
                        text: c.text.clone(),
                    })
                    .filter(|c| c.start_ms >= 0)
                    .collect();
                if perturbed.is_empty() {
                    continue;
                }
                let subtitle = vad::subtitle_profile(&perturbed, speech.len());
                let found = vad::find_offset(&speech, &subtitle, range_secs * 1000);
                // Only a pure shift has a recoverable answer; a rate change is
                // outside the model, and "expected" would be a lie.
                let expected = (*rate == 1.0).then_some(-shift_ms);
                let row = serde_json::json!({
                    "imdb_id": movie.imdb_id,
                    "title": movie.title,
                    "tier": movie.source.label(),
                    "cues": cues.len(),
                    "cues_per_min": cues.len() as f64 / minutes.max(1.0),
                    "perturbation": name,
                    "shift_ms": shift_ms,
                    "rate": rate,
                    "expected_ms": expected,
                    "offset_ms": found.offset_ms,
                    "err_ms": expected.map(|e| (found.offset_ms - e).abs()),
                    "agreement": found.agreement,
                    "runner_up": found.runner_up,
                    "margin": found.margin(),
                });
                use std::io::Write;
                let mut log = log.lock().unwrap();
                serde_json::to_writer(&mut *log, &row)?;
                writeln!(log)?;
                log.flush()?;
                rows += 1;
            }
            Ok(rows)
        })();
        match &outcome {
            Ok(rows) => println!("  {} ✓ {rows} rows", truncate(&movie.title, 34)),
            Err(e) => println!("  {} ✗ {e}", truncate(&movie.title, 34)),
        }
        outcome.unwrap_or(0)
    });
    println!(
        "\n{} rows written to {}",
        counted.iter().sum::<usize>(),
        log_path.display()
    );
    Ok(())
}

/// Report subtitles too sparse to be a full dialogue track.
///
/// Forced tracks — which only translate foreign lines and on-screen signs — are
/// usually flagged in the container, but not always: some discs leave the
/// disposition unset and some sidecars are saved without any marker in the name.
/// Density catches every variant, because the thing that actually distinguishes
/// them is having a handful of cues across a whole feature.
fn verify(out: PathBuf, min_density: f64) -> Result<()> {
    let plan = read_plan(&out)?;
    let mut thin = Vec::new();
    let mut checked = 0usize;

    for movie in &plan {
        let path = out.join(&movie.imdb_id).join("subtitle.srt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        checked += 1;
        let cues = movie_subtitles_len(&text);
        // Last timestamp stands in for runtime: it is in the file already, and a
        // track that stops early is itself the problem being looked for.
        let span_min = text
            .rsplit_once(" --> ")
            .and_then(|(_, rest)| rest.split('\n').next())
            .and_then(parse_stamp_min)
            .unwrap_or(0.0);
        if span_min < 1.0 {
            continue;
        }
        let density = cues as f64 / span_min;
        if density < min_density {
            thin.push((density, cues, span_min, movie));
        }
    }

    thin.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!(
        "checked {checked} subtitles, {} below {min_density} cues/min\n",
        thin.len()
    );
    for (density, cues, span, movie) in &thin {
        println!(
            "  {density:5.2}/min  {cues:>5} cues over {span:5.0} min  {:12} [{}] {}",
            movie.imdb_id,
            movie.source.label(),
            truncate(&movie.title, 34)
        );
    }
    if !thin.is_empty() {
        println!("\nThese are almost certainly forced/partial tracks — the film's\nfull dialogue has to come from another source.");
    }
    Ok(())
}

/// Minutes from an SRT timestamp like `01:52:13,480`.
fn parse_stamp_min(stamp: &str) -> Option<f64> {
    let mut parts = stamp.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.replace(',', ".").parse().ok()?;
    Some(h * 60.0 + m + s / 60.0)
}

fn pgs_stats(input: PathBuf, index: Option<u32>, dump: usize, out_dir: PathBuf) -> Result<()> {
    let cues = match index {
        Some(i) => vobsub::cues(&input, i)?,
        None => {
            let data = std::fs::read(&input)
                .with_context(|| format!("Failed to read {}", input.display()))?;
            pgs::cues(&data)
        }
    };
    // The PGS text filter wants ≥4 antialiased colours; a DVD subpicture only
    // has 4 palette entries total, so its filter is just "something is inked".
    let filter = |c: &pgs::Cue| match index {
        Some(_) => c.height >= 8 && c.ink_and_colours().0 > 0.001,
        None => c.looks_like_text(),
    };
    let text: Vec<_> = cues.iter().filter(|c| filter(c)).collect();

    let mut durations: Vec<u32> = cues.iter().map(|c| c.duration_ms()).collect();
    durations.sort_unstable();
    let median = durations.get(durations.len() / 2).copied().unwrap_or(0);

    println!("cues            {}", cues.len());
    println!("  look like text{:>10}", text.len());
    println!("  disc graphics {:>10}", cues.len() - text.len());
    println!("median duration {:.2}s", median as f64 / 1000.0);
    if let (Some(first), Some(last)) = (cues.first(), cues.last()) {
        println!(
            "span            {:.2}s .. {:.2}s",
            first.start_ms as f64 / 1000.0,
            last.end_ms as f64 / 1000.0
        );
    }
    // Why cues pass or fail the text filter, in aggregate: the filter wants
    // height ≥ 16, ≥ 4 inked colours, ink < 0.6.
    let mut inks: Vec<f32> = Vec::new();
    let mut colour_counts: Vec<usize> = Vec::new();
    for c in &cues {
        let (ink, colours) = c.ink_and_colours();
        inks.push(ink);
        colour_counts.push(colours);
    }
    inks.sort_by(f32::total_cmp);
    colour_counts.sort_unstable();
    if let (Some(ink), Some(colours), Some(c)) = (
        inks.get(inks.len() / 2),
        colour_counts.get(colour_counts.len() / 2),
        cues.first(),
    ) {
        println!(
            "median cue      {}x{}, ink {ink:.3}, {colours} colours",
            c.width, c.height
        );
    }
    if dump > 0 {
        // Fall back to unfiltered cues: when the filter rejects everything,
        // seeing what it rejected is the whole point of dumping.
        let pool: Vec<&pgs::Cue> = if text.is_empty() {
            cues.iter().collect()
        } else {
            text.clone()
        };
        std::fs::create_dir_all(&out_dir)?;
        let picked = ocr::spread(&pool, dump);
        for (i, c) in picked.iter().enumerate() {
            c.to_rgb([0, 0, 0])
                .save(out_dir.join(format!("rs_cue_{i:03}.png")))?;
        }
        println!(
            "wrote {} sample PNGs to {}{}",
            picked.len(),
            out_dir.display(),
            if text.is_empty() {
                " (filter rejected all — dumping unfiltered)"
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    // The Cloudflare and OpenAI keys live in the repo's `.env`, which nothing
    // in the environment exports: `.envrc` is only `use flake`, and the flake's
    // shellHook handles R2 and GCP alone. Without this, every invocation
    // outside an interactive shell that happened to have them is one missing
    // variable away from a run that quietly verifies nothing.
    dotenvy::dotenv().ok();
    match Args::parse().command {
        Command_::Inventory {
            library,
            data_root,
            out,
            jobs,
        } => inventory(library, data_root, out, jobs),
        Command_::Refresh {
            library,
            data_root,
            out,
            transcribe_imdb,
        } => refresh(library, data_root, out, transcribe_imdb),
        Command_::Extract { out, jobs, limit } => extract(out, jobs, limit),
        Command_::ExtractAudio {
            out,
            jobs,
            limit,
            imdb,
        } => extract_audio(out, jobs, limit, imdb.as_deref()),
        Command_::OcrSample {
            out,
            movies,
            cues,
            model,
        } => ocr_sample(out, movies, cues, model),
        Command_::Ocr {
            out,
            model,
            films_in_flight,
            limit,
            allow_unreadable,
        } => ocr_all(out, model, films_in_flight, limit, allow_unreadable),
        Command_::OcrFile {
            input,
            index,
            model,
            srt,
            allow_unreadable,
        } => ocr_file(input, index, model, srt, allow_unreadable),
        Command_::Sync {
            out,
            data_root,
            windows,
            window_secs,
            films_in_flight,
            limit,
            max_residual_ms,
            min_agreement,
            debug_anchors,
        } => sync_all(
            out,
            data_root,
            films_in_flight,
            limit,
            SyncOptions {
                windows,
                window_secs,
                max_residual_ms,
                min_agreement,
                debug_anchors,
            },
        ),
        Command_::Check {
            out,
            tier,
            windows,
            window_secs,
            films_in_flight,
            limit,
        } => check_all(out, tier, windows, window_secs, films_in_flight, limit),
        Command_::Transcribe {
            out,
            films_in_flight,
            limit,
            imdb,
        } => transcribe_all(out, films_in_flight, limit, imdb),
        Command_::Agreement {
            out,
            tier,
            limit,
            jobs,
            range_secs,
        } => agreement(out, tier, limit, jobs, range_secs),
        Command_::VadSync {
            out,
            data_root,
            jobs,
            limit,
            range_secs,
            min_agreement,
            min_margin,
        } => vad_sync(
            out,
            data_root,
            jobs,
            limit,
            range_secs,
            min_agreement,
            min_margin,
        ),
        Command_::TextSync {
            out,
            data_root,
            jobs,
            limit,
            range_secs,
            min_agreement,
            min_margin,
            dry_run,
        } => text_sync(
            out,
            data_root,
            jobs,
            limit,
            range_secs,
            min_agreement,
            min_margin,
            dry_run,
        ),
        Command_::Calibrate {
            out,
            jobs,
            limit,
            range_secs,
        } => calibrate(out, jobs, limit, range_secs),
        Command_::Verify { out, min_density } => verify(out, min_density),
        Command_::ExportSidecars { out } => export_sidecars(out),
        Command_::PgsStats {
            input,
            index,
            dump,
            out_dir,
        } => pgs_stats(input, index, dump, out_dir),
    }
}
