//! One-off recovery of the raw SRTs the downloader used to throw away.
//!
//! Every movie already has `subtitles/<imdb>.jsonl` — dialogue that was
//! cleaned on the way to disk, with the original SRT discarded. This binary
//! re-downloads the candidates OpenSubtitles offers for that movie and looks
//! for the one which, run back through the *same* automatic preprocessing,
//! reproduces the stored JSONL **exactly**.
//!
//! Exactness is the whole point and is deliberately not relaxed. Some JSONL
//! files were hand-edited afterwards to fix OCR typos, and those edits exist
//! nowhere else. A fuzzy match would happily accept the pre-correction SRT,
//! promote it to source-of-truth, and silently revert those fixes on the next
//! build. So: an exact match is recovered, and anything short of one is left
//! alone and reported. A near-miss is itself the useful signal — it usually
//! means *that* file is one of the hand-corrected ones.
//!
//! Downloads are metered by OpenSubtitles, so the run is resumable: progress
//! is appended to a per-language ledger, an exhausted daily quota stops the
//! run cleanly rather than burning through the remaining movies recording
//! false verdicts, and no SRT that cost a download is ever discarded — the
//! best near-miss is kept under `subtitles-unmatched/` for later inspection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use language_utils::Language;
use movie_subtitles::SubtitleLine;
use opensubtitles_downloader::{rank_by_quality, OpenSubtitlesClient, Throttled};
use serde::{Deserialize, Serialize};

/// Recover the original SRT for already-downloaded movie subtitles
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Language packs to work through (ISO 639-3 dir names). Defaults to all.
    #[arg(short, long, num_args = 1..)]
    language: Vec<String>,

    /// How many candidate subtitles to try per movie before giving up.
    #[arg(long, default_value_t = 4)]
    max_candidates: usize,

    /// Stop after attempting this many movies (0 = no limit).
    #[arg(long, default_value_t = 0)]
    limit: usize,

    /// Stop after spending this many downloads (0 = until the quota runs out).
    ///
    /// Bazarr holds the *same* OpenSubtitles account, so an uncapped recovery
    /// run eats the whole daily allowance and starves its subtitle backlog.
    /// Capping here leaves the remainder for it.
    #[arg(long, default_value_t = 0)]
    max_downloads: usize,

    /// Re-attempt movies a previous run gave up on.
    #[arg(long)]
    retry_failed: bool,

    /// Report what would be attempted without spending any downloads.
    #[arg(long)]
    dry_run: bool,
}

/// What became of one movie's recovery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    /// A candidate reproduced the stored JSONL exactly; the SRT is now saved.
    Recovered,
    /// Candidates were tried and none reproduced the JSONL exactly.
    Exhausted,
    /// OpenSubtitles has no (non-machine-translated) subtitles for this movie.
    NoCandidates,
}

#[derive(Debug, Serialize, Deserialize)]
struct LedgerEntry {
    imdb_id: String,
    outcome: Outcome,
    candidates_tried: usize,
    /// Fraction of stored lines the closest candidate reproduced, for triage.
    best_line_ratio: f64,
}

fn ledger_path(movies_dir: &Path) -> PathBuf {
    movies_dir.join("recovery-log.jsonl")
}

fn unmatched_path(movies_dir: &Path, imdb_id: &str) -> PathBuf {
    movies_dir.join(format!("subtitles-unmatched/{imdb_id}.srt"))
}

fn read_ledger(movies_dir: &Path) -> Result<HashMap<String, LedgerEntry>> {
    let path = ledger_path(movies_dir);
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let content = std::fs::read_to_string(&path)?;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // A truncated final line (killed mid-append) should not poison a resume.
        if let Ok(entry) = serde_json::from_str::<LedgerEntry>(line) {
            out.insert(entry.imdb_id.clone(), entry);
        }
    }
    Ok(out)
}

fn append_ledger(movies_dir: &Path, entry: &LedgerEntry) -> Result<()> {
    use std::io::Write;
    let path = ledger_path(movies_dir);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    Ok(())
}

/// Does re-running the automatic preprocessing over `candidate` reproduce
/// `stored` exactly?
///
/// Timings are compared at second granularity because the JSONL was written by
/// a version that truncated them (`secs() * 1000`), while parsing now keeps
/// full millisecond precision — that difference is the preprocessing changing,
/// not the content differing. Sentences must be identical outright.
fn reproduces_exactly(stored: &[SubtitleLine], candidate: &[SubtitleLine]) -> bool {
    stored.len() == candidate.len()
        && std::iter::zip(stored, candidate).all(|(a, b)| {
            a.sentence == b.sentence
                && a.start_ms / 1000 == b.start_ms / 1000
                && a.end_ms / 1000 == b.end_ms / 1000
        })
}

/// Fraction of stored lines the candidate also contains, for triaging a miss.
fn line_ratio(stored: &[SubtitleLine], candidate: &[SubtitleLine]) -> f64 {
    if stored.is_empty() {
        return 0.0;
    }
    let have: std::collections::HashSet<&str> =
        candidate.iter().map(|l| l.sentence.as_str()).collect();
    let hits = stored
        .iter()
        .filter(|l| have.contains(l.sentence.as_str()))
        .count();
    hits as f64 / stored.len() as f64
}

/// One movie to try to recover.
struct Job {
    language_dir: String,
    opensubtitles_code: String,
    movies_dir: PathBuf,
    imdb_id: String,
}

/// Every (language, movie) pair that still has no raw SRT, interleaved across
/// languages so a quota that runs out mid-run leaves all languages equally far
/// along rather than one finished and the rest untouched.
fn build_queue(args: &Args) -> Result<Vec<Job>> {
    let data_root = Path::new("./generate-data/data");
    let mut wanted: Vec<String> = args.language.clone();
    if wanted.is_empty() {
        for entry in std::fs::read_dir(data_root)
            .with_context(|| format!("Failed to read {}", data_root.display()))?
        {
            let entry = entry?;
            if entry
                .path()
                .join("sentence-sources/movies/subtitles")
                .is_dir()
            {
                wanted.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        wanted.sort();
    }

    let mut per_language: Vec<Vec<Job>> = Vec::new();
    for dir_name in wanted {
        let Some(language) = Language::from_code(&dir_name) else {
            eprintln!("⚠ skipping unrecognised language dir '{dir_name}'");
            continue;
        };
        let movies_dir = opensubtitles_downloader::movies_dir(&dir_name);
        let subtitles_dir = movies_dir.join("subtitles");
        if !subtitles_dir.is_dir() {
            continue;
        }
        let ledger = read_ledger(&movies_dir)?;

        let mut ids: Vec<String> = std::fs::read_dir(&subtitles_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".jsonl").map(str::to_owned)
            })
            .collect();
        ids.sort();

        let jobs: Vec<Job> = ids
            .into_iter()
            .filter(|imdb_id| {
                if movie_subtitles::raw_srt_path(&movies_dir, imdb_id).exists() {
                    return false;
                }
                match ledger.get(imdb_id) {
                    Some(e) if e.outcome == Outcome::Recovered => false,
                    Some(_) => args.retry_failed,
                    None => true,
                }
            })
            .map(|imdb_id| Job {
                language_dir: dir_name.clone(),
                opensubtitles_code: language.opensubtitles_language_code().to_string(),
                movies_dir: movies_dir.clone(),
                imdb_id,
            })
            .collect();

        if !jobs.is_empty() {
            per_language.push(jobs);
        }
    }

    // Round-robin interleave.
    let longest = per_language.iter().map(Vec::len).max().unwrap_or(0);
    let mut remaining: Vec<_> = per_language.into_iter().map(Vec::into_iter).collect();
    let mut queue = Vec::new();
    for _ in 0..longest {
        for jobs in &mut remaining {
            queue.extend(jobs.next());
        }
    }
    Ok(queue)
}

/// Recovery verdict for one movie, plus how many downloads it cost.
struct Attempt {
    outcome: Outcome,
    candidates_tried: usize,
    best_line_ratio: f64,
}

async fn recover_one(
    client: &OpenSubtitlesClient,
    job: &Job,
    max_candidates: usize,
) -> Result<Attempt> {
    let stored = movie_subtitles::read_derived_jsonl(&movie_subtitles::derived_jsonl_path(
        &job.movies_dir,
        &job.imdb_id,
    ))?;

    let imdb_num: u64 = job
        .imdb_id
        .strip_prefix("tt")
        .unwrap_or(&job.imdb_id)
        .parse()
        .with_context(|| format!("Bad IMDb id {}", job.imdb_id))?;

    let mut candidates = client
        .search_subtitles_for_movie(imdb_num, &job.opensubtitles_code)
        .await?;
    candidates.retain(|s| !s.attributes.ai_translated && !s.attributes.machine_translated);
    if candidates.is_empty() {
        return Ok(Attempt {
            outcome: Outcome::NoCandidates,
            candidates_tried: 0,
            best_line_ratio: 0.0,
        });
    }
    rank_by_quality(&mut candidates);

    let mut tried = 0usize;
    let mut best_ratio = 0.0f64;
    let mut best_srt: Option<String> = None;

    for candidate in candidates.iter().take(max_candidates) {
        let Some(file_id) = candidate.attributes.files.first().map(|f| f.file_id) else {
            continue;
        };
        let srt = client.download_subtitle(file_id).await?;
        tried += 1;

        let Ok(parsed) = movie_subtitles::parse_srt(&srt) else {
            continue;
        };
        if reproduces_exactly(&stored, &parsed) {
            movie_subtitles::write_raw_srt(&job.movies_dir, &job.imdb_id, &srt)?;
            return Ok(Attempt {
                outcome: Outcome::Recovered,
                candidates_tried: tried,
                best_line_ratio: 1.0,
            });
        }
        let ratio = line_ratio(&stored, &parsed);
        if ratio > best_ratio {
            best_ratio = ratio;
            best_srt = Some(srt);
        }
    }

    // A download already paid for is never thrown away, even unmatched: the
    // closest candidate is the starting point for reconciling a hand-edited
    // JSONL, and re-fetching it later would cost quota again.
    if let Some(srt) = best_srt {
        let path = unmatched_path(&job.movies_dir, &job.imdb_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, srt)?;
    }

    Ok(Attempt {
        outcome: Outcome::Exhausted,
        candidates_tried: tried,
        best_line_ratio: best_ratio,
    })
}

/// Was this failure OpenSubtitles telling us to stop?
fn throttle(err: &anyhow::Error) -> Option<Throttled> {
    err.downcast_ref::<Throttled>().copied()
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();

    let queue = build_queue(&args)?;
    let total = queue.len();
    println!("{total} movies still have no raw SRT");
    if args.dry_run {
        let mut per_language: HashMap<&str, usize> = HashMap::new();
        for job in &queue {
            *per_language.entry(job.language_dir.as_str()).or_default() += 1;
        }
        let mut rows: Vec<_> = per_language.into_iter().collect();
        rows.sort();
        for (language, n) in rows {
            println!("  {language:10}{n:>6}");
        }
        return Ok(());
    }

    let api_key = std::env::var("OPENSUBTITLES_API_KEY")
        .context("OPENSUBTITLES_API_KEY environment variable not set")?;
    let mut client = OpenSubtitlesClient::new(api_key);
    match (
        std::env::var("OPENSUBTITLES_USERNAME").ok(),
        std::env::var("OPENSUBTITLES_PASSWORD").ok(),
    ) {
        (Some(user), Some(pass)) => client.login(&user, &pass).await?,
        _ => anyhow::bail!(
            "OPENSUBTITLES_USERNAME and OPENSUBTITLES_PASSWORD must be set — \
             unauthenticated downloads are capped far too low for a recovery run"
        ),
    }

    let mut recovered = 0usize;
    let mut exhausted = 0usize;
    let mut no_candidates = 0usize;
    let mut downloads = 0usize;
    let mut near_misses: Vec<(String, String, f64)> = Vec::new();

    for (i, job) in queue.iter().enumerate() {
        if args.limit > 0 && i >= args.limit {
            println!("\nReached --limit of {}", args.limit);
            break;
        }
        // Checked before the movie rather than after, so the cap is a ceiling
        // on what this run can spend, not a threshold it overshoots by up to
        // --max-candidates downloads.
        if args.max_downloads > 0 && downloads + args.max_candidates > args.max_downloads {
            println!(
                "\nStopping at {downloads} downloads to stay within --max-downloads {} \
                 (the rest of today's quota is left for Bazarr)",
                args.max_downloads
            );
            break;
        }
        print!(
            "[{}/{}] {} {} ... ",
            i + 1,
            total,
            job.language_dir,
            job.imdb_id
        );
        use std::io::Write;
        std::io::stdout().flush().ok();

        let attempt = match recover_one(&client, job, args.max_candidates).await {
            Ok(a) => a,
            Err(e) => {
                match throttle(&e) {
                    Some(Throttled::QuotaExhausted) => {
                        println!("quota exhausted");
                        println!(
                            "\nDaily download allowance is spent — stopping here. \
                             Re-run tomorrow and it resumes from the ledger."
                        );
                        break;
                    }
                    Some(Throttled::TooManyRequests) => {
                        println!("rate limited, pausing 60s");
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        continue;
                    }
                    None => {
                        println!("error: {e}");
                        continue;
                    }
                };
            }
        };

        downloads += attempt.candidates_tried;
        match attempt.outcome {
            Outcome::Recovered => {
                recovered += 1;
                println!(
                    "✓ recovered (after {} download(s))",
                    attempt.candidates_tried
                );
            }
            Outcome::Exhausted => {
                exhausted += 1;
                println!(
                    "✗ no exact match (best {:.1}% of lines, {} tried)",
                    attempt.best_line_ratio * 100.0,
                    attempt.candidates_tried
                );
                if attempt.best_line_ratio >= 0.95 {
                    near_misses.push((
                        job.language_dir.clone(),
                        job.imdb_id.clone(),
                        attempt.best_line_ratio,
                    ));
                }
            }
            Outcome::NoCandidates => {
                no_candidates += 1;
                println!("– no candidates offered");
            }
        }

        append_ledger(
            &job.movies_dir,
            &LedgerEntry {
                imdb_id: job.imdb_id.clone(),
                outcome: attempt.outcome,
                candidates_tried: attempt.candidates_tried,
                best_line_ratio: attempt.best_line_ratio,
            },
        )?;
    }

    println!("\n──────── summary ────────");
    println!("recovered:      {recovered}");
    println!("no exact match: {exhausted}");
    println!("no candidates:  {no_candidates}");
    println!("downloads used: {downloads}");
    if !near_misses.is_empty() {
        println!(
            "\n{} movies had a candidate reproducing >=95% of lines but not exactly.\n\
             These are the likely hand-corrected JSONLs — the closest SRT for each is\n\
             saved under subtitles-unmatched/ so the diff can be reconciled by hand:",
            near_misses.len()
        );
        near_misses.sort_by(|a, b| b.2.total_cmp(&a.2));
        for (language, imdb_id, ratio) in near_misses.iter().take(25) {
            println!("  {language:10} {imdb_id:12} {:.2}%", ratio * 100.0);
        }
    }
    Ok(())
}
