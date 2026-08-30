//! Score a phoneme sequence against the model's per-frame distribution.
//!
//! Greedy decode + edit distance asks "what did the model think it heard,
//! and does that string match?" — it throws the distribution away and turns
//! every soft disagreement into a hard edit. CTC asks the question we care
//! about directly: the total probability, over every frame alignment, that
//! the audio spells *this* target. Mass the model put on the right phoneme
//! still counts when it lost the argmax, and no per-language equivalence
//! table is needed for the number to mean something.
//!
//! The endpoint returns the whole `(T, V)` log-prob matrix (fp16, zlib) so
//! the forward pass is paid once per clip and any sequence — a re-segmented
//! sentence, a corrected subtitle, a rebuilt espeak reference — is scored
//! here, locally, without a GPU. The arithmetic mirrors
//! `modal-envs/wav2vec2_phoneme.py::_score_target` (`torch.ctc_loss`, CTC
//! blank = the checkpoint's blank id); the server's own score is reproduced
//! to ~1e-3, which the integration test checks.

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;

/// The frame matrix exactly as the endpoint ships it: compressed, so a cache
/// entry stays ~24 KB per audio-second.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMatrixPayload {
    /// `[T, V]`.
    pub shape: Vec<usize>,
    pub dtype: String,
    pub encoding: String,
    pub blank_id: usize,
    /// Row labels, from the tokenizer's own vocab (not `decode`, which
    /// renders 78 of 461 entries differently and would misindex rows).
    pub vocab: Vec<String>,
    /// zlib-compressed little-endian fp16, row-major.
    pub data: String,
}

/// Decoded per-frame log-probabilities.
#[derive(Debug, Clone)]
pub struct FrameMatrix {
    pub frames: usize,
    pub vocab: Vec<String>,
    pub blank_id: usize,
    index: HashMap<String, usize>,
    /// Row-major `[frames × vocab.len()]`.
    log_probs: Vec<f32>,
}

impl FrameMatrix {
    pub fn decode(payload: &FrameMatrixPayload) -> Result<Self> {
        if payload.dtype != "float16" || payload.encoding != "zlib+base64" {
            bail!(
                "unsupported frame matrix format {}/{}",
                payload.dtype,
                payload.encoding
            );
        }
        let [frames, width] = payload.shape[..] else {
            bail!("frame matrix shape {:?} is not [T, V]", payload.shape);
        };
        if width != payload.vocab.len() {
            bail!(
                "frame matrix has {width} columns but {} vocab entries",
                payload.vocab.len()
            );
        }
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(&payload.data)
            .context("frame matrix base64")?;
        let mut raw = Vec::with_capacity(frames * width * 2);
        flate2::read::ZlibDecoder::new(compressed.as_slice())
            .read_to_end(&mut raw)
            .context("frame matrix zlib")?;
        if raw.len() != frames * width * 2 {
            bail!(
                "frame matrix holds {} bytes, expected {}×{}×2",
                raw.len(),
                frames,
                width
            );
        }
        let log_probs: Vec<f32> = raw
            .chunks_exact(2)
            .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
            .collect();
        let index = payload
            .vocab
            .iter()
            .enumerate()
            .map(|(i, tok)| (tok.clone(), i))
            .collect();
        Ok(Self {
            frames,
            vocab: payload.vocab.clone(),
            blank_id: payload.blank_id,
            index,
            log_probs,
        })
    }

    #[inline]
    fn lp(&self, t: usize, v: usize) -> f64 {
        f64::from(self.log_probs[t * self.vocab.len() + v])
    }

    /// Vocab id of a phoneme token, if the model knows it.
    pub fn id(&self, token: &str) -> Option<usize> {
        self.index.get(token).copied()
    }

    /// The model's own reading: argmax per frame, blanks and repeats
    /// collapsed — the reference every target is measured against.
    pub fn greedy_ids(&self) -> Vec<usize> {
        let width = self.vocab.len();
        let mut out = Vec::new();
        let mut prev = None;
        for t in 0..self.frames {
            let row = &self.log_probs[t * width..(t + 1) * width];
            let (best, _) = row
                .iter()
                .enumerate()
                .fold(
                    (0, f32::NEG_INFINITY),
                    |acc, (i, &v)| {
                        if v > acc.1 { (i, v) } else { acc }
                    },
                );
            if best != self.blank_id && prev != Some(best) {
                out.push(best);
            }
            prev = Some(best);
        }
        out
    }

    /// `log P(target | audio)` summed over all CTC alignments — the standard
    /// forward recursion in log space over the blank-interleaved target.
    /// `None` when the target is empty or longer than the frames allow.
    pub fn log_likelihood(&self, target: &[usize]) -> Option<f64> {
        if target.is_empty() || target.len() > self.frames {
            return None;
        }
        // Extended label sequence: blank, l1, blank, l2, …, blank.
        let ext_len = 2 * target.len() + 1;
        let label = |s: usize| -> usize {
            if s.is_multiple_of(2) {
                self.blank_id
            } else {
                target[s / 2]
            }
        };
        let mut alpha = vec![f64::NEG_INFINITY; ext_len];
        alpha[0] = self.lp(0, self.blank_id);
        if ext_len > 1 {
            alpha[1] = self.lp(0, label(1));
        }
        let mut next = vec![f64::NEG_INFINITY; ext_len];
        for t in 1..self.frames {
            for (s, slot) in next.iter_mut().enumerate() {
                let mut acc = alpha[s];
                if s >= 1 {
                    acc = log_add(acc, alpha[s - 1]);
                }
                // A skip over the blank is allowed between two different labels.
                if s >= 2 && s % 2 == 1 && label(s) != label(s - 2) {
                    acc = log_add(acc, alpha[s - 2]);
                }
                *slot = if acc == f64::NEG_INFINITY {
                    acc
                } else {
                    acc + self.lp(t, label(s))
                };
            }
            std::mem::swap(&mut alpha, &mut next);
        }
        let total = log_add(alpha[ext_len - 1], alpha[ext_len - 2]);
        (total != f64::NEG_INFINITY).then_some(total)
    }

    /// Score `target` the way the endpoint's `target_phonemes` does.
    pub fn score_target(&self, target: &[String]) -> TargetScore {
        let mut ids = Vec::with_capacity(target.len());
        let mut oov = Vec::new();
        for tok in target {
            match self.id(tok) {
                Some(id) => ids.push(id),
                None => oov.push(tok.clone()),
            }
        }
        let free = self.greedy_ids();
        let logp_target = if ids.is_empty() {
            None
        } else {
            self.log_likelihood(&ids)
        };
        let logp_free = self.log_likelihood(&free);
        let ratio = match (logp_target, logp_free) {
            (Some(t), Some(f)) => Some((t - f) / ids.len() as f64),
            _ => None,
        };
        TargetScore {
            logp_target,
            logp_target_per_phoneme: logp_target.map(|t| t / ids.len() as f64),
            logp_free,
            ratio,
            target_len: ids.len(),
            free_len: free.len(),
            oov,
        }
    }
}

/// How well the audio supports one specific phoneme sequence.
///
/// `ratio = (logP(target) − logP(free)) / target_len`: log-odds per phoneme
/// of the claimed sentence against the model's own preferred reading. 0
/// means the target *is* what the model would have said; more negative means
/// the audio increasingly fails to support it. Scale-free across clip
/// lengths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetScore {
    pub logp_target: Option<f64>,
    pub logp_target_per_phoneme: Option<f64>,
    pub logp_free: Option<f64>,
    pub ratio: Option<f64>,
    pub target_len: usize,
    pub free_len: usize,
    /// Target phonemes outside the model's vocabulary — dropped from the
    /// scored sequence, so a non-empty list means the score is of a
    /// *shorter* target than asked for.
    pub oov: Vec<String>,
}

fn log_add(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        b
    } else if b == f64::NEG_INFINITY {
        a
    } else if a > b {
        a + (b - a).exp().ln_1p()
    } else {
        b + (a - b).exp().ln_1p()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(vocab: &[&str], blank_id: usize, rows: &[&[f32]]) -> FrameMatrix {
        let width = vocab.len();
        let mut log_probs = Vec::new();
        for row in rows {
            assert_eq!(row.len(), width);
            let z: f32 = row.iter().map(|p| p.exp()).sum::<f32>();
            assert!((z - 1.0).abs() < 1e-4, "rows must be log-probabilities");
            log_probs.extend_from_slice(row);
        }
        FrameMatrix {
            frames: rows.len(),
            vocab: vocab.iter().map(|s| s.to_string()).collect(),
            blank_id,
            index: vocab
                .iter()
                .enumerate()
                .map(|(i, s)| (s.to_string(), i))
                .collect(),
            log_probs,
        }
    }

    fn lp(p: &[f32]) -> Vec<f32> {
        p.iter().map(|x| x.ln()).collect()
    }

    /// Two frames, vocab {blank, a}. P("a") = every path whose collapse is
    /// "a": (a,a), (a,-), (-,a) = 0.7·0.7 + 0.7·0.3 + 0.3·0.7 = 0.91.
    #[test]
    fn sums_over_all_alignments() {
        let r = lp(&[0.3, 0.7]);
        let m = matrix(&["<pad>", "a"], 0, &[&r, &r]);
        let got = m.log_likelihood(&[m.id("a").unwrap()]).unwrap();
        assert!((got - 0.91f64.ln()).abs() < 1e-5, "{got}");
        // "a a" needs a blank between repeats: impossible in two frames.
        assert!(m.log_likelihood(&[1, 1]).is_none());
    }

    /// Target longer than the frames can spell has no alignment.
    #[test]
    fn impossible_targets_are_none() {
        let r = lp(&[0.5, 0.5]);
        let m = matrix(&["<pad>", "a"], 0, &[&r]);
        assert!(m.log_likelihood(&[1, 1]).is_none());
        assert!(m.log_likelihood(&[]).is_none());
    }

    #[test]
    fn greedy_collapses_blanks_and_repeats() {
        let a = lp(&[0.1, 0.8, 0.1]);
        let b = lp(&[0.1, 0.1, 0.8]);
        let blank = lp(&[0.8, 0.1, 0.1]);
        let m = matrix(&["<pad>", "a", "b"], 0, &[&a, &a, &blank, &a, &b, &b]);
        assert_eq!(m.greedy_ids(), vec![1, 1, 2]);
        let s = m.score_target(&["a".into(), "a".into(), "b".into()]);
        assert_eq!(s.free_len, 3);
        assert_eq!(s.target_len, 3);
        // The greedy path is the model's preferred reading: ratio is ≈ 0.
        assert!(s.ratio.unwrap().abs() < 1e-9, "{s:?}");
        let worse = m.score_target(&["b".into(), "a".into()]);
        assert!(worse.ratio.unwrap() < s.ratio.unwrap());
        assert_eq!(
            m.score_target(&["zz".into(), "a".into()]).oov,
            vec!["zz".to_string()]
        );
    }

    /// Round-trips the endpoint's wire format.
    #[test]
    fn decodes_the_wire_format() {
        use std::io::Write;
        let rows: Vec<f32> = vec![-0.1, -2.5, -3.0, -0.2, -1.0, -2.0];
        let mut bytes = Vec::new();
        for v in &rows {
            bytes.extend_from_slice(&half::f16::from_f32(*v).to_le_bytes());
        }
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&bytes).unwrap();
        let payload = FrameMatrixPayload {
            shape: vec![2, 3],
            dtype: "float16".into(),
            encoding: "zlib+base64".into(),
            blank_id: 0,
            vocab: vec!["<pad>".into(), "a".into(), "b".into()],
            data: base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap()),
        };
        let m = FrameMatrix::decode(&payload).unwrap();
        assert_eq!(m.frames, 2);
        assert!((m.lp(1, 2) - -2.0).abs() < 1e-3);
        assert!(m.greedy_ids().is_empty()); // blank wins both frames
    }
}
