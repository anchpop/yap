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
        Command_::Verify { out, min_density } => verify(out, min_density),
        Command_::PgsStats { sup, dump, out_dir } => pgs_stats(sup, dump, out_dir),
    }
}
