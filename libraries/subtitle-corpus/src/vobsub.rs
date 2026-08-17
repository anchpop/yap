//! Decoding DVD (`dvd_subtitle` / VobSub) bitmap subtitles out of an MKV.
//!
//! DVD-sourced rips carry their subtitles as SPUs — run-length-encoded 2-bit
//! pictures — where Blu-rays carry PGS. Same idea, older bones: each MKV block
//! is one Sub-Picture Unit whose control sequences say where on screen it
//! goes, which of the track's 16 palette colours the four 2-bit values mean,
//! and when to stop showing it. The 16-colour palette itself lives in the
//! track's codec-private data as `.idx`-style text.
//!
//! ffmpeg cannot write these to a standalone file (the `.sup` muxer is
//! PGS-only), so the MKV is demuxed in-process instead.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use matroska_demuxer::{ContentCompAlgo, ContentEncodingValue, Frame, MatroskaFile, TrackEntry};

use crate::pgs::Cue;

/// How this track's frames were mangled at mux time.
///
/// mkvmerge zlib-compresses VobSub tracks by default, and the demuxer only
/// *reports* the encoding — undoing it is on us. Header stripping (a shared
/// prefix chopped off every frame) also shows up in the wild.
enum Transform {
    None,
    Zlib,
    Prefix(Vec<u8>),
}

impl Transform {
    fn of(track: &TrackEntry) -> Result<(Transform, bool)> {
        let Some(encodings) = track.content_encodings() else {
            return Ok((Transform::None, false));
        };
        let mut on_frames = Transform::None;
        let mut private_zlibbed = false;
        for enc in encodings {
            let ContentEncodingValue::Compression(c) = enc.encoding() else {
                bail!("track uses an encoding that is not a compression");
            };
            let transform = match c.algo() {
                ContentCompAlgo::Zlib => Transform::Zlib,
                ContentCompAlgo::Stripping => {
                    Transform::Prefix(c.settings().unwrap_or_default().to_vec())
                }
                other => bail!("unsupported track compression {other:?}"),
            };
            if enc.scope() & 2 != 0 {
                private_zlibbed = matches!(transform, Transform::Zlib);
            }
            if enc.scope() & 1 != 0 {
                on_frames = transform;
            }
        }
        Ok((on_frames, private_zlibbed))
    }

    fn apply(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Transform::None => Ok(data.to_vec()),
            Transform::Zlib => inflate(data),
            Transform::Prefix(prefix) => {
                let mut whole = prefix.clone();
                whole.extend_from_slice(data);
                Ok(whole)
            }
        }
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .context("zlib")?;
    Ok(out)
}

/// Timed bitmaps for the `ffmpeg_index`-th stream of the file, decoded to the
/// same [`Cue`] the PGS path produces.
pub fn cues(video: &Path, ffmpeg_index: u32) -> Result<Vec<Cue>> {
    let file = std::fs::File::open(video)?;
    let mut mkv = MatroskaFile::open(file).context("not a matroska file")?;

    // ffmpeg numbers streams in file order, which for MKV is track order.
    let track = mkv
        .tracks()
        .get(ffmpeg_index as usize)
        .with_context(|| format!("no track at index {ffmpeg_index}"))?;
    if track.codec_id() != "S_VOBSUB" {
        bail!("track {ffmpeg_index} is {}, not S_VOBSUB", track.codec_id());
    }
    let track_number = track.track_number().get();
    let (transform, private_zlibbed) = Transform::of(track)?;
    let palette = match track.codec_private() {
        Some(p) if private_zlibbed => idx_palette(&String::from_utf8_lossy(&inflate(p)?)),
        Some(p) => idx_palette(&String::from_utf8_lossy(p)),
        None => DEFAULT_PALETTE,
    };

    // Ticks -> ms. The scale is in ns per tick; the common default is 1ms.
    let ns_per_tick = mkv.info().timestamp_scale().get();

    let mut out: Vec<Cue> = Vec::new();
    let mut open: Option<usize> = None;
    let mut frame = Frame::default();
    while mkv.next_frame(&mut frame)? {
        if frame.track != track_number {
            continue;
        }
        let start_ms = (frame.timestamp * ns_per_tick / 1_000_000) as u32;
        let data = transform.apply(&frame.data)?;
        let Some(spu) = Spu::parse(&data) else {
            continue;
        };

        // Whatever this SPU says, it replaces what is on screen.
        if let Some(i) = open.take() {
            if out[i].end_ms == 0 {
                out[i].end_ms = start_ms;
            }
        }
        let Some(mut cue) = spu.render(&palette, start_ms) else {
            continue; // a clear — it already closed the open cue
        };
        if let Some(stop) = spu.stop_delay_ms {
            cue.end_ms = start_ms + stop;
        }
        out.push(cue);
        open = Some(out.len() - 1);
    }

    // A final cue nothing ever cleared: give it a typical dwell.
    if let Some(i) = open {
        if out[i].end_ms == 0 {
            out[i].end_ms = out[i].start_ms + 4_000;
        }
    }
    out.retain(|c| c.end_ms > c.start_ms);
    Ok(out)
}

/// The `palette:` line of the `.idx` text: 16 RRGGBB values.
fn idx_palette(idx: &str) -> [[u8; 3]; 16] {
    let mut palette = DEFAULT_PALETTE;
    for line in idx.lines() {
        let Some(rest) = line.trim().strip_prefix("palette:") else {
            continue;
        };
        for (slot, hex) in palette.iter_mut().zip(rest.split(',')) {
            if let Ok(rgb) = u32::from_str_radix(hex.trim(), 16) {
                *slot = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
            }
        }
    }
    palette
}

/// VLC's default DVD palette, for tracks whose rip lost the `.idx` text.
const DEFAULT_PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x80, 0x80, 0x80],
    [0x20, 0x20, 0x20],
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x80, 0x80, 0x80],
    [0x20, 0x20, 0x20],
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x80, 0x80, 0x80],
    [0x20, 0x20, 0x20],
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x80, 0x80, 0x80],
    [0x20, 0x20, 0x20],
];

/// One Sub-Picture Unit, parsed as far as its control sequences.
struct Spu<'a> {
    data: &'a [u8],
    /// Display area on the DVD canvas.
    width: u16,
    height: u16,
    /// Byte offsets of the two interlaced RLE fields (even rows, odd rows).
    field_offsets: Option<(usize, usize)>,
    /// For each 2-bit value, an index into the track's 16-colour palette.
    colors: [u8; 4],
    /// For each 2-bit value, opacity 0-15.
    alphas: [u8; 4],
    /// STP_DSP delay relative to the SPU's own timestamp.
    stop_delay_ms: Option<u32>,
}

impl<'a> Spu<'a> {
    fn parse(data: &'a [u8]) -> Option<Spu<'a>> {
        if data.len() < 4 {
            return None;
        }
        let be16 = |i: usize| -> usize { u16::from_be_bytes([data[i], data[i + 1]]) as usize };
        let mut spu = Spu {
            data,
            width: 0,
            height: 0,
            field_offsets: None,
            colors: [0; 4],
            alphas: [0; 4],
            stop_delay_ms: None,
        };

        let mut off = be16(2);
        // The last sequence points at itself; a damaged one could loop.
        for _ in 0..64 {
            if off + 4 > data.len() {
                break;
            }
            let delay_ms = (be16(off) * 1024 / 90) as u32;
            let next = be16(off + 2);
            let mut p = off + 4;
            while let Some(&cmd) = data.get(p) {
                p += 1;
                match cmd {
                    0x00 | 0x01 => {} // (forced) start of display
                    0x02 => spu.stop_delay_ms = Some(delay_ms),
                    0x03 => {
                        if p + 2 > data.len() {
                            break;
                        }
                        spu.colors = [
                            data[p + 1] & 0xF,
                            data[p + 1] >> 4,
                            data[p] & 0xF,
                            data[p] >> 4,
                        ];
                        p += 2;
                    }
                    0x04 => {
                        if p + 2 > data.len() {
                            break;
                        }
                        spu.alphas = [
                            data[p + 1] & 0xF,
                            data[p + 1] >> 4,
                            data[p] & 0xF,
                            data[p] >> 4,
                        ];
                        p += 2;
                    }
                    0x05 => {
                        if p + 6 > data.len() {
                            break;
                        }
                        let x1 = (data[p] as u16) << 4 | (data[p + 1] >> 4) as u16;
                        let x2 = ((data[p + 1] & 0xF) as u16) << 8 | data[p + 2] as u16;
                        let y1 = (data[p + 3] as u16) << 4 | (data[p + 4] >> 4) as u16;
                        let y2 = ((data[p + 4] & 0xF) as u16) << 8 | data[p + 5] as u16;
                        spu.width = x2.saturating_sub(x1) + 1;
                        spu.height = y2.saturating_sub(y1) + 1;
                        p += 6;
                    }
                    0x06 => {
                        if p + 4 > data.len() {
                            break;
                        }
                        spu.field_offsets = Some((be16(p), be16(p + 2)));
                        p += 4;
                    }
                    0x07 => {
                        // CHG_COLCON: first two bytes are its own size.
                        if p + 2 > data.len() {
                            break;
                        }
                        p += be16(p).max(2);
                    }
                    _ => break, // 0xff terminator, or junk — either way stop
                }
            }
            if next == off {
                break;
            }
            off = next;
        }
        Some(spu)
    }

    /// The SPU as a [`Cue`], or `None` for a clear (no picture).
    fn render(&self, palette: &[[u8; 3]; 16], start_ms: u32) -> Option<Cue> {
        let (even, odd) = self.field_offsets?;
        let (w, h) = (self.width as usize, self.height as usize);
        if w == 0 || h == 0 || self.alphas.iter().all(|&a| a == 0) {
            return None;
        }
        let mut pixels = vec![0u8; w * h];
        decode_field(self.data, even, &mut pixels, w, (0..h).step_by(2));
        decode_field(self.data, odd, &mut pixels, w, (1..h).step_by(2));

        let mut cue_palette = HashMap::new();
        for v in 0..4u8 {
            let [r, g, b] = palette[self.colors[v as usize] as usize & 0xF];
            cue_palette.insert(v, [r, g, b, self.alphas[v as usize] * 17]);
        }
        Some(Cue {
            start_ms,
            end_ms: 0,
            width: self.width,
            height: self.height,
            pixels,
            palette: cue_palette,
        })
    }
}

/// One interlaced field of DVD RLE: nibble codes, byte-aligned per line.
fn decode_field(
    data: &[u8],
    byte_offset: usize,
    out: &mut [u8],
    width: usize,
    rows: impl Iterator<Item = usize>,
) {
    let mut nib = Nibbles {
        data,
        pos: byte_offset * 2,
    };
    for row in rows {
        let mut col = 0usize;
        while col < width {
            let Some((run, color)) = rle_value(&mut nib) else {
                return;
            };
            // A zero run length means "the rest of the line".
            let run = if run == 0 {
                width - col
            } else {
                (run as usize).min(width - col)
            };
            out[row * width + col..row * width + col + run].fill(color);
            col += run;
        }
        nib.align();
    }
}

/// A run: 4 to 16 bits, growing while the value is too small for its width.
fn rle_value(nib: &mut Nibbles) -> Option<(u16, u8)> {
    let mut v = nib.next()? as u16;
    if v < 0x4 {
        v = v << 4 | nib.next()? as u16;
        if v < 0x10 {
            v = v << 4 | nib.next()? as u16;
            if v < 0x40 {
                v = v << 4 | nib.next()? as u16;
            }
        }
    }
    Some((v >> 2, (v & 3) as u8))
}

struct Nibbles<'a> {
    data: &'a [u8],
    /// In nibbles, not bytes.
    pos: usize,
}

impl Nibbles<'_> {
    fn next(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.pos / 2)?;
        let nib = if self.pos.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0xF
        };
        self.pos += 1;
        Some(nib)
    }

    fn align(&mut self) {
        self.pos = self.pos.next_multiple_of(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_widths_grow_with_leading_zeros() {
        // 0xB       -> run 2, colour 3 (one nibble)
        // 0x2 0x7   -> run 9, colour 3 (two nibbles)
        // 0x0 0x8 0x4 -> run 33, colour 0 (three nibbles)
        // 0x0 0x0 0x4 0x1 -> run 16, colour 1 (four nibbles)
        let mut nib = Nibbles {
            data: &[0xB2, 0x70, 0x84, 0x00, 0x41],
            pos: 0,
        };
        assert_eq!(rle_value(&mut nib), Some((2, 3)));
        assert_eq!(rle_value(&mut nib), Some((9, 3)));
        assert_eq!(rle_value(&mut nib), Some((33, 0)));
        assert_eq!(rle_value(&mut nib), Some((16, 1)));
    }

    #[test]
    fn field_decode_interlaces_and_fills_lines() {
        // Each field holds one 4px line: nibble 0x7 (run 1, colour 3), then
        // 0,0,0,0 — the 16-bit code whose run of 0 means "rest of the line",
        // colour 0 — then padding to the byte boundary. Even field decodes
        // row 0, odd field row 1.
        let data = [0x70, 0x00, 0x00, 0x70, 0x00, 0x00];
        let mut out = vec![9u8; 8];
        decode_field(&data, 0, &mut out, 4, (0..2).step_by(2));
        decode_field(&data, 3, &mut out, 4, (1..2).step_by(2));
        assert_eq!(out[0..4], [3, 0, 0, 0]);
        assert_eq!(out[4..8], [3, 0, 0, 0]);
    }

    #[test]
    fn idx_palette_parses_the_palette_line() {
        let idx = "# comment\nsize: 720x480\npalette: 000000, ff00ff, 123456";
        let p = idx_palette(idx);
        assert_eq!(p[1], [0xff, 0x00, 0xff]);
        assert_eq!(p[2], [0x12, 0x34, 0x56]);
        // Slots past the parsed values keep the default.
        assert_eq!(p[3], DEFAULT_PALETTE[3]);
    }
}
