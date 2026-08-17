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
use language_utils::{Language, MovieMetadataBasic};
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

    /// Show how each near-miss under `subtitles-unmatched/` differs from the
    /// stored JSONL, without spending any downloads.
    #[arg(long)]
    inspect_unmatched: bool,

    /// Port the stored JSONL's wording into its near-miss SRT and promote it.
    ///
    /// Only for differences that are edits of the same line — the hand-made OCR
    /// corrections that are the reason these files do not match. Anything else
    /// is a different subtitle, not a correctable one, and is left alone.
    #[arg(long)]
    adopt_unmatched: bool,

    /// Take the candidate SRT wholesale for the films listed in `--approved`,
    /// replacing the stored wording instead of preserving it.
    ///
    /// The mirror image of `--adopt-unmatched`, for the films where review
    /// found the *stored* side to be the damaged one — a dropped `œ` turning
    /// `cœur` into `cur`, a mangled Roman numeral, a lost apostrophe. The
    /// candidate becomes the raw SRT and the derived JSONL is rebuilt from it,
    /// which is what repairs the shipped course text.
    #[arg(long)]
    prefer_candidate: bool,

    /// Largest per-line edit distance still treated as a correction rather than
    /// a different line, as a fraction of the line's length.
    ///
    /// Only a triage hint for the report — whether a difference is really a
    /// correction is a judgement about language, not about character counts,
    /// and is made by review rather than by this number.
    #[arg(long, default_value_t = 0.25)]
    max_edit_ratio: f64,

    /// Write the full near-miss differences here as JSON, for review.
    #[arg(long)]
    json: Option<PathBuf>,

    /// Adopt only the films listed — the output of that review.
    ///
    /// One `<language> <imdb_id>` pair per line. The language is part of the
    /// key because the same film appears in several packs with *different*
    /// subtitles; a verdict on the French one says nothing about the Italian.
    /// Without this, `--adopt-unmatched` adopts nothing.
    #[arg(long)]
    approved: Option<PathBuf>,
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

/// Walk the near-misses, reporting (and optionally adopting) their differences.
///
/// A near-miss is almost always a JSONL that was hand-edited to fix OCR typos
/// after it was written: the candidate is the right subtitle, it just predates
/// the corrections. Adopting means putting those corrections into the SRT, so
/// the file that becomes the source of truth carries them and rebuilding cannot
/// silently undo them.
fn inspect_unmatched(args: &Args) -> Result<()> {
    let data_root = Path::new("./generate-data/data");
    let mut languages: Vec<String> = args.language.clone();
    if languages.is_empty() {
        for entry in std::fs::read_dir(data_root)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            languages.push(name);
        }
        languages.sort();
    }

    let approved: Option<std::collections::HashSet<(String, String)>> = match &args.approved {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let mut set = std::collections::HashSet::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (language, imdb_id) = line
                    .split_once(char::is_whitespace)
                    .with_context(|| format!("expected `<language> <imdb_id>`, got `{line}`"))?;
                set.insert((language.trim().to_string(), imdb_id.trim().to_string()));
            }
            Some(set)
        }
        None => None,
    };
    let mut report: Vec<serde_json::Value> = Vec::new();
    let (mut correctable, mut adopted, mut incomparable, mut edits) = (0, 0, 0, 0);
    for dir_name in languages {
        let movies_dir = opensubtitles_downloader::movies_dir(&dir_name);
        let unmatched_dir = movies_dir.join("subtitles-unmatched");
        let Ok(entries) = std::fs::read_dir(&unmatched_dir) else {
            continue;
        };
        let mut ids: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_suffix(".srt")
                    .map(str::to_owned)
            })
            .collect();
        ids.sort();

        for imdb_id in ids {
            // A film recovered since this near-miss was saved needs nothing.
            if movie_subtitles::raw_srt_path(&movies_dir, &imdb_id).exists() {
                continue;
            }
            let jsonl = movie_subtitles::derived_jsonl_path(&movies_dir, &imdb_id);
            let Ok(stored) = movie_subtitles::read_derived_jsonl(&jsonl) else {
                continue;
            };
            let srt_path = unmatched_path(&movies_dir, &imdb_id);
            let srt = std::fs::read_to_string(&srt_path)?;
            let candidate = match movie_subtitles::parse_srt(&srt) {
                Ok(c) => c,
                Err(e) => {
                    println!("  {dir_name:9} {imdb_id:12} unparseable: {e}");
                    incomparable += 1;
                    continue;
                }
            };

            match diverge(&stored, &candidate) {
                Err(reason) => {
                    // Review can approve what the heuristic cannot compare: a
                    // candidate that is the same translation *continued past*
                    // a truncated stored file (The Leopard's stored JSONL was
                    // a 707-line "CD1" of a 1,900-cue film) diverges by line
                    // count alone, and taking it wholesale is the fix.
                    let key = (dir_name.clone(), imdb_id.clone());
                    if args.prefer_candidate && approved.as_ref().is_some_and(|a| a.contains(&key))
                    {
                        match prefer_candidate(&movies_dir, &imdb_id, &srt) {
                            Ok(n) => {
                                adopted += 1;
                                println!(
                                    "  {dir_name:9} {imdb_id:12} ✓ took the candidate \
                                     ({n} lines) despite: {reason}"
                                );
                                let pending = unmatched_path(&movies_dir, &imdb_id);
                                if let Err(e) = std::fs::remove_file(&pending) {
                                    println!("      ! left in the pending pile: {e}");
                                }
                            }
                            Err(e) => {
                                println!("  {dir_name:9} {imdb_id:12} ✗ not taken: {e}")
                            }
                        }
                        continue;
                    }
                    println!("  {dir_name:9} {imdb_id:12} ✗ {reason}");
                    incomparable += 1;
                }
                Ok(diffs) => {
                    let all_edits = diffs.iter().all(|d| d.is_edit(args.max_edit_ratio));
                    println!(
                        "  {dir_name:9} {imdb_id:12} {} differing line(s){}",
                        diffs.len(),
                        if all_edits {
                            ""
                        } else {
                            "  ← includes a rewrite"
                        }
                    );
                    for d in diffs.iter().take(6) {
                        println!("      [{:>5}] stored: {}", d.index, d.stored);
                        println!("              cand: {}", d.candidate);
                    }
                    if diffs.len() > 6 {
                        println!("      … {} more", diffs.len() - 6);
                    }
                    edits += diffs.len();
                    report.push(serde_json::json!({
                        "language": dir_name,
                        "imdb_id": imdb_id,
                        "looks_like_edits_only": all_edits,
                        "differences": diffs.iter().map(|d| serde_json::json!({
                            "line": d.index,
                            "stored": d.stored,
                            "candidate": d.candidate,
                        })).collect::<Vec<_>>(),
                    }));
                    if all_edits {
                        correctable += 1;
                    }
                    // Adoption is gated on review, never on the heuristic: only
                    // a reader can tell a corrected typo from a different word.
                    // One film that will not patch cleanly must not abort the
                    // sweep over all the others.
                    let key = (dir_name.clone(), imdb_id.clone());
                    let mut settled = false;
                    if args.prefer_candidate && approved.as_ref().is_some_and(|a| a.contains(&key))
                    {
                        match prefer_candidate(&movies_dir, &imdb_id, &srt) {
                            Ok(n) => {
                                adopted += 1;
                                settled = true;
                                println!("      ✓ took the candidate ({n} lines)");
                            }
                            Err(e) => println!("      ✗ not taken: {e}"),
                        }
                    }
                    if args.adopt_unmatched && approved.as_ref().is_some_and(|a| a.contains(&key)) {
                        match adopt(&movies_dir, &imdb_id, &srt, &stored, &candidate) {
                            Ok(()) => {
                                adopted += 1;
                                settled = true;
                                println!("      ✓ adopted");
                            }
                            Err(e) => println!("      ✗ not adopted: {e}"),
                        }
                    }
                    // A film that has been judged is done, so retire its copy
                    // from the pending pile. Leaving it there would hand the
                    // same diffs back to the next review sweep, and this queue
                    // is meant to be drained repeatedly as downloads arrive —
                    // re-judging settled films is exactly the cost that makes
                    // keeping up with it feel expensive.
                    if settled {
                        let pending = unmatched_path(&movies_dir, &imdb_id);
                        if let Err(e) = std::fs::remove_file(&pending) {
                            println!("      ! left in the pending pile: {e}");
                        }
                    }
                }
            }
        }
    }

    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        println!(
            "\nwrote {} films' differences to {}",
            report.len(),
            path.display()
        );
    }
    println!("\n{correctable} look like edits only, {incomparable} not comparable, {edits} differing lines");
    if args.adopt_unmatched {
        println!("{adopted} adopted into subtitles-raw/");
        if approved.is_none() {
            println!("(nothing adopted: --adopt-unmatched needs --approved <file> from review)");
        }
    }
    Ok(())
}

/// Install the candidate SRT as the source of truth, discarding the stored
/// wording, and rebuild the derived JSONL from it.
///
/// Used where review found the stored side damaged. Rebuilding the JSONL is the
/// point: it is what the course reads for films with no raw SRT, and leaving a
/// stale one beside a corrected SRT would keep shipping `cur` for `cœur`. The
/// JSONL it overwrites is in version control, so the previous text is not lost.
fn prefer_candidate(movies_dir: &Path, imdb_id: &str, srt: &str) -> Result<usize> {
    let lines = movie_subtitles::parse_srt(srt)?;
    if lines.is_empty() {
        anyhow::bail!("candidate parses to no dialogue");
    }
    movie_subtitles::write_raw_srt(movies_dir, imdb_id, srt)?;
    movie_subtitles::write_derived_jsonl(
        &movie_subtitles::derived_jsonl_path(movies_dir, imdb_id),
        &lines,
    )?;
    Ok(lines.len())
}

/// Rewrite a near-miss SRT so the stored JSONL's wording wins, then promote it.
///
/// The substitution is done on the SRT's own text, not regenerated from the
/// JSONL, so everything cleaning throws away — markup, sound cues, and the
/// millisecond timings that are the whole reason for recovering the file —
/// survives untouched.
fn adopt(
    movies_dir: &Path,
    imdb_id: &str,
    srt: &str,
    stored: &[SubtitleLine],
    candidate: &[SubtitleLine],
) -> Result<()> {
    // Patch the smallest span that actually differs, not the whole sentence.
    //
    // A cleaned sentence rarely appears verbatim in the SRT: cues wrap across
    // lines, carry markup, and lead with speaker dashes, all of which cleaning
    // removes. Replacing "lch" with "Ich" survives that; replacing an entire
    // reflowed sentence does not. The cursor only moves forward, so a short
    // fragment cannot be matched against an earlier cue that happens to
    // contain the same characters.
    //
    // Patch the *repaired* text, not the raw bytes: the candidate lines the
    // patterns are built from went through `parse_srt`, which repairs CP1252
    // mojibake, so a pattern holding `didn’t` would never occur in a haystack
    // still holding the raw 0x92. The promoted file is therefore repaired
    // form — which `parse_srt` maps to the same text either way.
    let mut patched = movie_subtitles::repair_cp1252_mojibake(srt);
    let mut cursor = 0usize;
    for (want, have) in std::iter::zip(stored, candidate) {
        let want = as_cleaned_today(&want.sentence);
        if want == have.sentence {
            continue;
        }
        let Some((pattern, to)) = anchored_change(&have.sentence, &want, &patched, cursor) else {
            continue; // validation below will catch the shortfall
        };
        // The anchor only says *where*; only the captured fragment is
        // rewritten, so the file's own line breaks and markup are untouched.
        let Some((lo, hi)) = locate(&patched, cursor, &pattern) else {
            continue;
        };
        patched.replace_range(lo..hi, &to);
        cursor = lo + to.len();
    }
    // Only promote if the patch actually achieves what it set out to. A
    // substitution can miss — the sentence as cleaned need not appear verbatim
    // in the SRT — and quietly installing a file that still differs would put
    // the wrong text beyond reach of the exactness check that found it.
    let reparsed = movie_subtitles::parse_srt(&patched)
        .map_err(|e| anyhow::anyhow!("patched SRT no longer parses: {e}"))?;
    if stored.len() != reparsed.len() {
        anyhow::bail!(
            "patching changed the line count: {} stored vs {} patched",
            stored.len(),
            reparsed.len()
        );
    }
    let misses: Vec<String> = std::iter::zip(stored, &reparsed)
        .enumerate()
        .filter(|(_, (a, b))| as_cleaned_today(&a.sentence) != b.sentence)
        .map(|(i, (a, b))| {
            format!(
                "[{i}] want `{}` still `{}`",
                as_cleaned_today(&a.sentence),
                b.sentence
            )
        })
        .collect();
    if !misses.is_empty() {
        anyhow::bail!(
            "patch did not reproduce the stored wording on {} line(s):\n        {}",
            misses.len(),
            misses.join("\n        ")
        );
    }
    movie_subtitles::write_raw_srt(movies_dir, imdb_id, &patched)?;
    Ok(())
}

/// The change to apply, widened with enough surrounding context to identify
/// where it belongs.
///
/// The bare difference is often a single character — `ln` → `In` reduces to
/// `l` → `I` — which would match the first stray `l` in the file and patch the
/// wrong line. Context makes the anchor unique. Too much context is the
/// opposite failure: cleaning joins cues that wrap across lines and strips
/// markup, so a wide window may not appear in the SRT at all. Hence a ladder,
/// widest first, taking the first width that actually occurs.
fn anchored_change(
    have: &str,
    want: &str,
    haystack: &str,
    cursor: usize,
) -> Option<(String, String)> {
    let (core_from, core_to) = minimal_change(have, want)?;
    let h: Vec<char> = have.chars().collect();
    let w: Vec<char> = want.chars().collect();
    // `minimal_change` trimmed exactly the shared prefix and suffix, so the
    // difference begins where the two stop agreeing and runs for as long as
    // the trimmed fragment.
    let start = h.iter().zip(w.iter()).take_while(|(a, b)| a == b).count();
    let end = start + core_from.chars().count();

    for context in [20usize, 12, 6, 3, 1, 0] {
        let lo = start.saturating_sub(context);
        let hi = (end + context).min(h.len());
        if lo >= hi {
            continue;
        }
        let pattern = anchor_pattern(&h[lo..hi], start - lo, end - lo);
        if locate(haystack, cursor, &pattern).is_some() {
            return Some((pattern, core_to));
        }
    }
    None
}

/// A whitespace- and markup-flexible regex for `anchor`, capturing exactly the
/// differing fragment at `[core_start, core_end)`.
///
/// The capture is what makes this safe. Searching the matched window for the
/// fragment by text would find the wrong one — repairing `ln` to `In` inside
/// `Stai calmo. ln quelle` means looking for `l`, and the `l` in "calmo" comes
/// first. The group pins the position the anchor was built around.
///
/// Markup may sit between any two characters of the cleaned sentence:
/// `<i>desde que...</i>.` cleans to `desde que....`, with the final dot on the
/// far side of the tag. Without allowing for that, no context width can match —
/// and the ladder then degrades to a bare one-character pattern that patches
/// whatever it hits first.
fn anchor_pattern(anchor: &[char], core_start: usize, core_end: usize) -> String {
    /// Zero or more inline tags, HTML or brace-style.
    const TAGS: &str = r"(?:<[^>\n]*>|\{[^}\n]*\})*";
    /// A gap the cleaner collapsed to one space: whitespace and tags, mixed.
    const GAP: &str = r"(?:\s|<[^>\n]*>|\{[^}\n]*\})+";
    let mut out = String::new();
    let mut prev_ws = false;
    for (i, c) in anchor.iter().enumerate() {
        if i == core_end && core_end > core_start {
            out.push(')');
        }
        // Tags between two glyphs sit *outside* the capture, so replacing the
        // core never swallows a tag.
        if i > 0 && !prev_ws && !c.is_whitespace() {
            out.push_str(TAGS);
        }
        if i == core_start {
            out.push('(');
            // A pure insertion has a zero-width core: an empty capture marking
            // where the text goes.
            if core_end == core_start {
                out.push(')');
            }
        }
        if c.is_whitespace() {
            if !prev_ws {
                out.push_str(GAP);
                prev_ws = true;
            }
        } else {
            out.push_str(&regex::escape(&c.to_string()));
            prev_ws = false;
        }
    }
    if core_start >= anchor.len() {
        out.push_str("()");
    } else if core_end >= anchor.len() && core_end > core_start {
        out.push(')');
    }
    out
}

/// Find `anchor` in `haystack` at or after `cursor`, treating any run of
/// whitespace as matching any other.
///
/// Cleaning joins a cue's wrapped lines with a space, so an anchor taken from
/// the cleaned sentence reads `Er sagt: lch` where the file holds
/// `Er sagt:\nlch`. Matching whitespace loosely is what lets a fragment span
/// that line break; without it every window wide enough to be unique is also
/// wide enough to straddle a newline and never match.
fn locate(haystack: &str, cursor: usize, pattern: &str) -> Option<(usize, usize)> {
    let re = regex::Regex::new(pattern).ok()?;
    let caps = re.captures_at(haystack, cursor)?;
    let core = caps.get(1)?;
    Some((core.start(), core.end()))
}

/// The smallest substring of `have` that must change to turn it into `want`,
/// with its replacement — i.e. the difference with the shared prefix and suffix
/// trimmed off. `None` when nothing needs removing (a pure insertion), which
/// has no anchor to search for.
fn minimal_change(have: &str, want: &str) -> Option<(String, String)> {
    let h: Vec<char> = have.chars().collect();
    let w: Vec<char> = want.chars().collect();
    let mut prefix = 0;
    while prefix < h.len() && prefix < w.len() && h[prefix] == w[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < h.len() - prefix
        && suffix < w.len() - prefix
        && h[h.len() - 1 - suffix] == w[w.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let from: String = h[prefix..h.len() - suffix].iter().collect();
    let to: String = w[prefix..w.len() - suffix].iter().collect();
    // An empty `from` is a pure insertion — a space OCR swallowed, as in
    // "dojogo" for "do jogo". It still has a place to go, marked by a
    // zero-width capture between the surrounding context, so it is worth
    // returning; only a no-op change is not.
    (!from.is_empty() || !to.is_empty()).then_some((from, to))
}

/// Levenshtein distance, for judging whether two lines are the same line.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// One line where a near-miss SRT disagrees with the stored JSONL.
struct Divergence {
    index: usize,
    stored: String,
    candidate: String,
}

impl Divergence {
    /// Is this plausibly the same line with a correction applied, rather than a
    /// different line altogether?
    fn is_edit(&self, max_ratio: f64) -> bool {
        let longest = self
            .stored
            .chars()
            .count()
            .max(self.candidate.chars().count());
        longest > 0
            && (edit_distance(&self.stored, &self.candidate) as f64 / longest as f64) <= max_ratio
    }
}

/// The stored wording as today's preprocessing would leave it.
///
/// Some JSONL files were written before the cleaner stripped SSA override tags,
/// so they still carry `{\1c&H3697DE&}` and friends. That is stale output, not
/// a correction — the candidate having dropped it is the candidate being right.
/// Normalising here keeps such lines from being mistaken for edits worth
/// porting back into the SRT.
fn as_cleaned_today(sentence: &str) -> String {
    static SSA: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\{\\[^}]*\}").unwrap());
    SSA.replace_all(sentence, "").trim().to_string()
}

/// Compare a near-miss SRT against the JSONL it failed to reproduce.
///
/// `Err` when the two cannot be lined up at all — a differing line count means
/// a different subtitle, not a corrected one, and there is nothing to port.
fn diverge(stored: &[SubtitleLine], candidate: &[SubtitleLine]) -> Result<Vec<Divergence>, String> {
    if stored.len() != candidate.len() {
        return Err(format!(
            "{} lines vs {} — different subtitle, not a correction",
            stored.len(),
            candidate.len()
        ));
    }
    let timing_off = std::iter::zip(stored, candidate)
        .filter(|(a, b)| a.start_ms / 1000 != b.start_ms / 1000)
        .count();
    if timing_off > 0 {
        return Err(format!(
            "{timing_off} lines differ in timing, not just wording"
        ));
    }
    Ok(std::iter::zip(stored, candidate)
        .enumerate()
        .filter_map(|(index, (a, b))| {
            let want = as_cleaned_today(&a.sentence);
            (want != b.sentence).then(|| Divergence {
                index,
                stored: want,
                candidate: b.sentence.clone(),
            })
        })
        .collect())
}

/// One movie to try to recover.
struct Job {
    language_dir: String,
    opensubtitles_code: String,
    movies_dir: PathBuf,
    imdb_id: String,
    /// Whether this is the film's *own* language rather than a translation.
    own_language: bool,
}

/// ISO 639-1 code for a language-pack directory, to compare against the
/// `original_language` TMDB records for each film.
fn iso_639_1(language_dir: &str) -> Option<&'static str> {
    Some(match language_dir {
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
        "zho-hans" | "zho-hant" => "zh",
        _ => return None,
    })
}

/// Each film's original language, from the `metadata.jsonl` the downloader
/// writes beside the subtitles.
fn original_languages(movies_dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let path = movies_dir.join("metadata.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return out;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(movie) = serde_json::from_str::<MovieMetadataBasic>(line) {
            if let Some(lang) = movie.original_language {
                out.insert(movie.id, lang);
            }
        }
    }
    out
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
        let originals = original_languages(&movies_dir);
        let own_code = iso_639_1(&dir_name);

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
                own_language: own_code
                    .is_some_and(|c| originals.get(&imdb_id).is_some_and(|o| o == c)),
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

    // A film's own-language subtitle comes first, everything else after.
    //
    // Only the original language transcribes what is actually spoken, so it is
    // the one that can anchor audio; a translation of a foreign film is useful
    // text but can never be aligned to speech. Both are recovered — this only
    // decides what a finite daily quota buys first.
    //
    // Within each group, languages are interleaved round-robin so a quota that
    // runs out mid-run leaves every language equally far along rather than one
    // finished and the rest untouched.
    let mut queue = Vec::new();
    for own_language in [true, false] {
        let groups: Vec<Vec<Job>> = per_language
            .iter_mut()
            .map(|jobs| {
                let (matching, rest) = std::mem::take(jobs)
                    .into_iter()
                    .partition(|j| j.own_language == own_language);
                *jobs = rest;
                matching
            })
            .collect();
        let longest = groups.iter().map(Vec::len).max().unwrap_or(0);
        let mut remaining: Vec<_> = groups.into_iter().map(Vec::into_iter).collect();
        for _ in 0..longest {
            for jobs in &mut remaining {
                queue.extend(jobs.next());
            }
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

    // Every mode that works on already-downloaded near-misses dispatches here.
    // Omitting one falls through to the download path instead, which spends
    // quota doing something entirely different from what was asked.
    if args.inspect_unmatched || args.adopt_unmatched || args.prefer_candidate {
        return inspect_unmatched(&args);
    }

    let queue = build_queue(&args)?;
    let total = queue.len();
    println!("{total} movies still have no raw SRT");
    if args.dry_run {
        let mut per_language: HashMap<&str, (usize, usize)> = HashMap::new();
        for job in &queue {
            let entry = per_language.entry(job.language_dir.as_str()).or_default();
            if job.own_language {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
        let mut rows: Vec<_> = per_language.into_iter().collect();
        rows.sort();
        let own_total: usize = rows.iter().map(|(_, (own, _))| own).sum();
        println!("{:10}{:>14}{:>14}", "", "own language", "translation");
        for (language, (own, other)) in rows {
            println!("  {language:10}{own:>12}{other:>14}");
        }
        println!(
            "\n{own_total} own-language subtitles come first — only those transcribe \
             what is actually spoken, so only they can anchor audio."
        );
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
