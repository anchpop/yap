//! Aligning a subtitle by *when* people talk, ignoring what they say.
//!
//! Whisper anchoring depends on words surviving both the model and the
//! subtitler. Often they do not: subtitles are condensed for reading speed, so
//! a character says a whole sentence and the cue keeps a fragment. On one film
//! whole-line matching found zero matches out of a thousand candidates.
//!
//! Speech detection sidesteps that entirely. A subtitle is a claim about when
//! someone is speaking, and voice activity detection measures the same thing
//! from the audio; lining the two profiles up needs no vocabulary at all. It
//! also uses the *whole* film rather than a few sampled windows, so the
//! evidence behind the answer is far larger.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::sync::Cue;

/// earshot's native frame: 256 samples at 16 kHz, i.e. 16 ms.
pub const FRAME: usize = 256;
pub const SAMPLE_RATE: usize = 16_000;
/// Frames per bucket. Six is ~96 ms — fine enough for an offset (a cue lasts
/// seconds) and six times cheaper to search than the raw frame rate. This is a
/// *read-time* view now: the persisted profile keeps earshot's native 16 ms
/// (see [`speech_profile`]) and the offset search buckets it down on the way
/// in ([`bucketed`]), so nothing that needs finer resolution — a clip's
/// clean-cut margin is a ~100 ms question — has to re-decode the film.
pub const FRAMES_PER_BUCKET: usize = 6;

/// Milliseconds per bucket, *derived* from the frame arithmetic rather than
/// chosen.
///
/// Stating a round 100 ms here while the audio side actually produced
/// `6 × 16 = 96 ms` buckets put the two profiles on different clocks. They
/// drifted 4% apart — nearly five minutes across a feature — and correlation
/// collapsed to 0.07 on a subtitle known to be correctly timed.
pub const BUCKET_MS: i64 = (FRAMES_PER_BUCKET * FRAME * 1000 / SAMPLE_RATE) as i64;

/// earshot's speech score for each native 16 ms frame of the film.
///
/// One value per [`FRAME`], not per bucket: earshot resolves at 16 ms and
/// that is what gets persisted, because throwing five sixths of it away to
/// save ~1.5 MB would make the cached profile useless for anything finer than
/// an offset — and re-deriving it costs minutes of audio decode. Consumers
/// that only need a coarse view call [`bucketed`] on the way in.
pub fn speech_profile(video: &Path, audio_stream: usize) -> Result<Vec<f32>> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(video)
        .args([
            "-map",
            &format!("0:a:{audio_stream}"),
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "s16le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("ffmpeg failed to start")?;
    let mut stdout = child.stdout.take().context("no ffmpeg output")?;

    let mut detector = earshot::Detector::default_boxed();
    let mut profile: Vec<f32> = Vec::new();
    let mut raw = vec![0u8; FRAME * 2 * 64];
    let mut pending: Vec<i16> = Vec::new();

    use std::io::Read;
    loop {
        let read = stdout.read(&mut raw)?;
        if read == 0 {
            break;
        }
        pending.extend(
            raw[..read - read % 2]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]])),
        );
        let usable = pending.len() - pending.len() % FRAME;
        for frame in pending[..usable].chunks_exact(FRAME) {
            profile.push(detector.predict_i16(frame));
        }
        pending.drain(..usable);
    }
    let _ = child.wait();
    if profile.is_empty() {
        bail!("no audio decoded");
    }
    Ok(profile)
}

/// Read a profile cached by [`write_profile`]. `None` means compute it afresh.
pub fn read_profile(path: &Path) -> Option<Vec<f32>> {
    let raw = std::fs::read(path).ok()?;
    if raw.is_empty() || !raw.len().is_multiple_of(4) {
        return None;
    }
    Some(
        raw.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// Cache a speech profile beside the film's other artifacts.
///
/// Decoding a feature's audio costs minutes; the profile it reduces to is a
/// few hundred kilobytes and never changes for a given file, so anything that
/// scores subtitles repeatedly — calibration above all — reads this instead.
pub fn write_profile(path: &Path, profile: &[f32]) -> Result<()> {
    let mut raw = Vec::with_capacity(profile.len() * 4);
    for v in profile {
        raw.extend_from_slice(&v.to_le_bytes());
    }
    Ok(std::fs::write(path, raw)?)
}

/// Average `factor` native frames into each coarse bucket.
///
/// The offset search runs on ~96 ms buckets (`factor = 6`): a cue lasts
/// seconds, so that resolution loses nothing and is six times cheaper to
/// correlate. This is applied on *read*, not on write — the persisted profile
/// stays at 16 ms so a finer question can still be answered without decoding
/// the film again. A trailing partial bucket is kept, averaged over whatever
/// frames it has.
pub fn bucketed(fine: &[f32], factor: usize) -> Vec<f32> {
    if factor <= 1 {
        return fine.to_vec();
    }
    fine.chunks(factor)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect()
}

/// The same profile, as the subtitle claims it: 1 where a cue is on screen.
pub fn subtitle_profile(cues: &[Cue], buckets: usize) -> Vec<f32> {
    let mut p = vec![0.0f32; buckets];
    for c in cues {
        let from = (c.start_ms / BUCKET_MS).max(0) as usize;
        let to = ((c.end_ms / BUCKET_MS).max(0) as usize).min(buckets);
        for slot in p.iter_mut().take(to).skip(from.min(buckets)) {
            *slot = 1.0;
        }
    }
    p
}

/// How well two profiles agree, as a correlation in [-1, 1].
///
/// Plain overlap would reward a subtitle that simply claimed speech everywhere,
/// so this is centred on each profile's mean: agreement means matching the
/// *pattern* of talk and silence, not the amount of it.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f32>() / n as f32;
    let mb = b.iter().sum::<f32>() / n as f32;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for i in 0..n {
        let (x, y) = (a[i] - ma, b[i] - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da.sqrt() * db.sqrt())
}

/// The shift that best lines the subtitle up with the speech.
#[derive(Debug, Clone, Copy)]
pub struct VadOffset {
    pub offset_ms: i64,
    /// Correlation at that shift.
    pub agreement: f32,
    /// Best correlation anywhere else, at least 2s away.
    pub runner_up: f32,
}

impl VadOffset {
    /// How far the winning shift stands above the next plausible one.
    ///
    /// A high correlation alone is not enough — a talky film correlates
    /// reasonably at many shifts. What distinguishes a real answer is a peak
    /// that beats its rivals.
    pub fn margin(&self) -> f32 {
        self.agreement - self.runner_up
    }
}

/// Search shifts within `range_ms` for the one that best matches the audio.
pub fn find_offset(speech: &[f32], subtitle: &[f32], range_ms: i64) -> VadOffset {
    let range = (range_ms / BUCKET_MS) as isize;
    let mut scored: Vec<(isize, f32)> = Vec::new();
    for shift in -range..=range {
        let score = if shift >= 0 {
            let s = shift as usize;
            correlation(&speech[s.min(speech.len())..], subtitle)
        } else {
            let s = (-shift) as usize;
            correlation(speech, &subtitle[s.min(subtitle.len())..])
        };
        scored.push((shift, score));
    }
    let (best_shift, best) =
        scored.iter().copied().fold(
            (0isize, f32::MIN),
            |acc, x| if x.1 > acc.1 { x } else { acc },
        );
    let runner_up = scored
        .iter()
        .filter(|(s, _)| (s - best_shift).abs() > 20) // at least 2s away
        .map(|(_, v)| *v)
        .fold(f32::MIN, f32::max);
    VadOffset {
        offset_ms: best_shift as i64 * BUCKET_MS,
        agreement: best,
        runner_up: if runner_up.is_finite() {
            runner_up
        } else {
            0.0
        },
    }
}
