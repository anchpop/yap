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
mod vad;

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use library::{Movie, Source};

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
        /// The decisive number. A talky film correlates passably at many
        /// shifts, so a high score alone proves nothing; a correctly-timed
        /// subtitle showed 0.17-0.29 here while one with no real answer
        /// managed 0.03.
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
    /// Decode a PGS `.sup` and report what is in it.
    PgsStats {
        sup: PathBuf,
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
fn subtitle_source(movie: &Movie, data_root: &std::path::Path) -> Option<PathBuf> {
    // A film with no original-language audio can never yield a speech clip,
    // so aligning a subtitle for it is work spent making a number wrong.
    if matches!(movie.source, Source::NoOriginalAudio) {
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

    let classified = parallel(movies, jobs, "probing", |entry| {
        let source = library::classify(
            &entry.imdb_id,
            &entry.path,
            &entry.original_language,
            &data_root,
        )
        .unwrap_or(Source::Missing);
        Movie {
            imdb_id: entry.imdb_id.clone(),
            title: entry.title.clone(),
            year: entry.year,
            path: entry.path.clone(),
            original_language: entry.original_language.clone(),
            source,
        }
    });

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
    let Source::DiscBitmap { index, .. } = &movie.source else {
        bail!("not a bitmap source");
    };
    let sup = ocr::sup_path(out, &movie.imdb_id);

    // ffmpeg blocks its thread for minutes reading a whole film; keep it off
    // the async runtime so other films' batches keep progressing.
    let (video, index, sup_for_task) = (movie.path.clone(), *index, sup.clone());
    tokio::task::spawn_blocking(move || ocr::extract_sup(&video, index, &sup_for_task))
        .await?
        .context("extract")?;

    let images = ocr::cue_images(&sup).context("decode")?;
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
    // The .sup is large and fully derived from the film; the SRT replaces it.
    let _ = std::fs::remove_file(&sup);
    Ok((lines.len(), images.len()))
}

/// Align each downloadable subtitle to the film on disk and write it out.
/// The knobs that decide how hard to listen and how sure to be.
#[derive(Clone, Copy)]
struct SyncOptions {
    windows: usize,
    window_secs: u32,
    max_residual_ms: f64,
    min_agreement: f64,
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

    let plan = read_plan(&out)?;
    let mut queue: Vec<(Movie, PathBuf)> = Vec::new();
    for movie in plan {
        if out.join(&movie.imdb_id).join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                queue.push((movie, raw));
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    let total = queue.len();
    println!("{total} films have a subtitle to align, {films_in_flight} at a time");

    let http = Arc::new(reqwest::Client::new());
    let out = Arc::new(out);
    let progress = AtomicUsize::new(0);

    let results: Vec<bool> = futures::stream::iter(queue.into_iter())
        .map(|(movie, raw)| {
            let http = Arc::clone(&http);
            let out = Arc::clone(&out);
            let progress = &progress;
            async move {
                let outcome = sync_one(&http, &movie, &raw, &out, opts).await;
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
    movie: &Movie,
    raw_srt: &std::path::Path,
    out: &std::path::Path,
    opts: SyncOptions,
) -> Result<sync::Alignment> {
    let cues = sync::parse_cues(&std::fs::read_to_string(raw_srt)?);
    if cues.is_empty() {
        bail!("subtitle has no cues");
    }
    let codes = library::stream_codes(&movie.original_language);
    let stream = sync::original_audio_stream(&movie.path, codes)?;
    let duration = sync::duration_ms(&movie.path)?;
    let language = library::course_dir(&movie.original_language)
        .and_then(whisper_language)
        .unwrap_or("en");

    // Spread the windows across the body of the film. Openings are logos and
    // credits, endings are credits again — neither carries much dialogue, and
    // anchors clustered at one end cannot reveal a rate.
    let mut heard = Vec::new();
    for at in sync::choose_windows(&cues, duration, opts.windows, opts.window_secs) {
        match sync::transcribe_window(http, &movie.path, stream, at, opts.window_secs, language)
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
        let codes = library::stream_codes(&movie.original_language);
        let stream = sync::original_audio_stream(&movie.path, codes).ok()?;
        let speech = vad::speech_profile(&movie.path, stream).ok()?;
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
    for movie in plan {
        if out.join(&movie.imdb_id).join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                queue.push((movie, raw));
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    println!("{} films left for speech-activity alignment", queue.len());

    let out = &out;
    let results = parallel(queue, jobs, "aligning", move |(movie, raw)| {
        let outcome = (|| -> Result<vad::VadOffset> {
            let cues = sync::parse_cues(&std::fs::read_to_string(raw)?);
            if cues.is_empty() {
                bail!("no cues");
            }
            let codes = library::stream_codes(&movie.original_language);
            let stream = sync::original_audio_stream(&movie.path, codes)?;
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

            let speech = vad::speech_profile(&movie.path, stream)?;
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
            Err(e) => println!("  {} ✗ {e}", truncate(&movie.title, 34)),
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
        let tmp = scratch.with_extension("sup");
        let _ = std::fs::remove_file(&tmp);
        ocr::extract_sup(video, stream.index, &tmp)?;
        let data = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        Ok(pgs::cues(&data)
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
    for movie in plan {
        if out.join(&movie.imdb_id).join("subtitle.srt").exists() {
            continue;
        }
        if let Some(raw) = subtitle_source(&movie, &data_root) {
            if movie.path.exists() {
                queue.push((movie, raw));
            }
        }
    }
    if limit > 0 {
        queue.truncate(limit);
    }
    println!(
        "{} films left to align against their discs' own tracks",
        queue.len()
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
                let Ok(ref_cues) = reference_cues(&movie.path, stream, &scratch) else {
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
            let profile_path = dir.join("speech-profile.f32");
            let speech = match vad::read_profile(&profile_path) {
                Some(p) => p,
                None => {
                    let codes = library::stream_codes(&movie.original_language);
                    let stream = sync::original_audio_stream(&movie.path, codes)?;
                    let p = vad::speech_profile(&movie.path, stream)?;
                    vad::write_profile(&profile_path, &p)?;
                    p
                }
            };
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

fn pgs_stats(sup: PathBuf, dump: usize, out_dir: PathBuf) -> Result<()> {
    let data = std::fs::read(&sup).with_context(|| format!("Failed to read {}", sup.display()))?;
    let cues = pgs::cues(&data);
    let text: Vec<_> = cues.iter().filter(|c| c.looks_like_text()).collect();

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
    if dump > 0 {
        std::fs::create_dir_all(&out_dir)?;
        for (i, c) in text.iter().take(dump).enumerate() {
            c.to_rgb([0, 0, 0])
                .save(out_dir.join(format!("rs_cue_{i:03}.png")))?;
        }
        println!("wrote {dump} sample PNGs to {}", out_dir.display());
    }
    Ok(())
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command_::Inventory {
            library,
            data_root,
            out,
            jobs,
        } => inventory(library, data_root, out, jobs),
        Command_::Extract { out, jobs, limit } => extract(out, jobs, limit),
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
        Command_::Sync {
            out,
            data_root,
            windows,
            window_secs,
            films_in_flight,
            limit,
            max_residual_ms,
            min_agreement,
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
            },
        ),
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
        Command_::PgsStats { sup, dump, out_dir } => pgs_stats(sup, dump, out_dir),
    }
}
