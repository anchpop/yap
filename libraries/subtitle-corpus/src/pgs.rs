//! Decoding Blu-ray PGS (`.sup`) subtitle streams into timed bitmaps.
//!
//! A disc's own subtitle track is authored against the exact file we hold, so
//! its timings need no synchronisation — unlike a downloaded SRT, which is
//! timed to whatever release its author happened to have and can be seconds
//! out. The price is that the text is a picture, so it has to be read back.
//!
//! Stream layout, per segment:
//! `b"PG"`, PTS `u32` (90 kHz), DTS `u32`, type `u8`, size `u16`, payload.
//! Types: `0x14` palette, `0x15` object, `0x16` composition, `0x17` window,
//! `0x80` end-of-display-set.

use std::collections::HashMap;

/// One subtitle image and the window it is on screen for.
#[derive(Debug, Clone)]
pub struct Cue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub width: u16,
    pub height: u16,
    /// Palette-indexed pixels, row-major, `width * height` long.
    pub pixels: Vec<u8>,
    /// Palette index -> RGBA.
    pub palette: HashMap<u8, [u8; 4]>,
}

impl Cue {
    pub fn duration_ms(&self) -> u32 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    /// Composite onto a solid background.
    ///
    /// Subtitle glyphs are drawn with a soft alpha edge over transparency, so
    /// leaving them unflattened would hand an OCR model whatever happens to be
    /// behind them. Black matches how the text is authored to appear.
    pub fn to_rgb(&self, background: [u8; 3]) -> image::RgbImage {
        let mut img = image::RgbImage::from_pixel(
            self.width as u32,
            self.height as u32,
            image::Rgb(background),
        );
        for y in 0..self.height as u32 {
            for x in 0..self.width as u32 {
                let idx = self.pixels[(y * self.width as u32 + x) as usize];
                let [r, g, b, a] = *self.palette.get(&idx).unwrap_or(&[0, 0, 0, 0]);
                if a == 0 {
                    continue;
                }
                let f = a as f32 / 255.0;
                let mix = |c: u8, bg: u8| (c as f32 * f + bg as f32 * (1.0 - f)) as u8;
                img.put_pixel(
                    x,
                    y,
                    image::Rgb([
                        mix(r, background[0]),
                        mix(g, background[1]),
                        mix(b, background[2]),
                    ]),
                );
            }
        }
        img
    }

    /// How much of the image is inked, and how many distinct colours it uses.
    ///
    /// Discs interleave non-text graphics (rating bars, logos) into the same
    /// stream; they are solid blocks of one or two colours, where real text is
    /// thin strokes over many antialiased shades.
    pub fn ink_and_colours(&self) -> (f32, usize) {
        let mut colours = [false; 256];
        let mut inked = 0usize;
        for &p in &self.pixels {
            let a = self.palette.get(&p).map(|c| c[3]).unwrap_or(0);
            if a > 32 {
                inked += 1;
                colours[p as usize] = true;
            }
        }
        let total = (self.width as usize * self.height as usize).max(1);
        (
            inked as f32 / total as f32,
            colours.iter().filter(|c| **c).count(),
        )
    }

    /// Is this plausibly a line of dialogue rather than a disc graphic?
    ///
    /// Graphics (rating bars, logos) are solid blocks of one or two colours
    /// with most of the box inked. Text is looser on both counts — but not
    /// as loose as first assumed: *Anything Goes* renders dialogue flat, with
    /// no antialiasing at all — 3 colours, thick glyphs at ink 0.56 — and a
    /// `colours >= 4 && ink < 0.6` filter silently discarded all 1,976 of its
    /// cues. The LLM's `not_text` verdict backstops whatever this lets past.
    pub fn looks_like_text(&self) -> bool {
        let (ink, colours) = self.ink_and_colours();
        self.height >= 16 && colours >= 3 && ink < 0.75
    }
}

struct Segment<'a> {
    pts_ms: u32,
    kind: u8,
    payload: &'a [u8],
}

fn segments(data: &[u8]) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 13 <= data.len() {
        if &data[off..off + 2] != b"PG" {
            off += 1; // resync rather than abort on a damaged stream
            continue;
        }
        let pts = u32::from_be_bytes([data[off + 2], data[off + 3], data[off + 4], data[off + 5]]);
        let kind = data[off + 10];
        let size = u16::from_be_bytes([data[off + 11], data[off + 12]]) as usize;
        let end = (off + 13 + size).min(data.len());
        out.push(Segment {
            pts_ms: pts / 90,
            kind,
            payload: &data[off + 13..end],
        });
        off += 13 + size;
    }
    out
}

fn parse_palette(payload: &[u8]) -> HashMap<u8, [u8; 4]> {
    let mut pal = HashMap::new();
    let mut i = 2usize;
    while i + 5 <= payload.len() {
        let (idx, y, cr, cb, a) = (
            payload[i],
            payload[i + 1] as f32,
            payload[i + 2] as f32,
            payload[i + 3] as f32,
            payload[i + 4],
        );
        // BT.601 limited-range YCrCb -> RGB
        let (c, d, e) = (y - 16.0, cb - 128.0, cr - 128.0);
        let clamp = |v: f32| v.clamp(0.0, 255.0) as u8;
        pal.insert(
            idx,
            [
                clamp(1.164 * c + 1.596 * e),
                clamp(1.164 * c - 0.392 * d - 0.813 * e),
                clamp(1.164 * c + 2.017 * d),
                a,
            ],
        );
        i += 5;
    }
    pal
}

/// PGS run-length encoding -> one palette index per pixel.
fn decode_rle(rle: &[u8], width: u16, height: u16) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; w * h];
    let (mut row, mut col, mut i) = (0usize, 0usize, 0usize);
    while i < rle.len() && row < h {
        let b = rle[i];
        i += 1;
        if b != 0 {
            if col < w {
                out[row * w + col] = b;
                col += 1;
            }
            continue;
        }
        if i >= rle.len() {
            break;
        }
        let b2 = rle[i];
        i += 1;
        if b2 == 0 {
            row += 1;
            col = 0;
            continue;
        }
        let two_byte = b2 & 0x40 != 0;
        let explicit = b2 & 0x80 != 0;
        let mut count = (b2 & 0x3F) as usize;
        if two_byte {
            if i >= rle.len() {
                break;
            }
            count = (count << 8) | rle[i] as usize;
            i += 1;
        }
        let colour = if explicit {
            if i >= rle.len() {
                break;
            }
            let c = rle[i];
            i += 1;
            c
        } else {
            0
        };
        let n = count.min(w.saturating_sub(col));
        if colour != 0 {
            out[row * w + col..row * w + col + n].fill(colour);
        }
        col += count;
    }
    out
}

/// Timed bitmaps, one per display set.
///
/// A display set is PCS → WDS → PDS → ODS → END, and the composition segment
/// announcing the objects arrives *before* the bitmap that fills them. Emitting
/// at composition time therefore pairs every cue with the **previous** set's
/// image — text and timings silently shifted by one for the whole film. The set
/// is only complete at END, so that is where a cue is emitted. A set carrying
/// no object is the clear that ends whatever is on screen.
pub fn cues(data: &[u8]) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    let mut palette: HashMap<u8, [u8; 4]> = HashMap::new();
    let mut set_pts = 0u32;
    let mut set_objects = 0u8;
    let mut objects: Vec<(u8, u16, u16, Vec<u8>)> = Vec::new();
    let mut open: Option<usize> = None;

    for seg in segments(data) {
        match seg.kind {
            0x16 => {
                set_pts = seg.pts_ms;
                set_objects = seg.payload.get(10).copied().unwrap_or(0);
                objects.clear();
            }
            0x14 => {
                let p = parse_palette(seg.payload);
                if !p.is_empty() {
                    palette = p;
                }
            }
            0x15 => {
                if seg.payload.len() < 4 {
                    continue;
                }
                if seg.payload[3] & 0x80 != 0 {
                    if seg.payload.len() < 11 {
                        continue;
                    }
                    let w = u16::from_be_bytes([seg.payload[7], seg.payload[8]]);
                    let h = u16::from_be_bytes([seg.payload[9], seg.payload[10]]);
                    objects.push((seg.payload[0], w, h, seg.payload[11..].to_vec()));
                } else if let Some(last) = objects.last_mut() {
                    last.3.extend_from_slice(&seg.payload[4..]);
                }
            }
            0x80 => {
                if let Some(i) = open.take() {
                    out[i].end_ms = set_pts;
                }
                if set_objects > 0 && !objects.is_empty() {
                    let (_, w, h, rle) = objects
                        .iter()
                        .max_by_key(|(_, w, h, _)| *w as u32 * *h as u32)
                        .unwrap();
                    out.push(Cue {
                        start_ms: set_pts,
                        end_ms: 0,
                        width: *w,
                        height: *h,
                        pixels: decode_rle(rle, *w, *h),
                        palette: palette.clone(),
                    });
                    open = Some(out.len() - 1);
                }
                objects.clear();
            }
            _ => {}
        }
    }

    out.retain(|c| c.end_ms > c.start_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_decodes_runs_and_line_ends() {
        // 4x2: explicit-colour run of 4 (colour 7), line end, four literal 1s.
        let rle = [0x00, 0x84, 0x07, 0x00, 0x00, 1, 1, 1, 1, 0x00, 0x00];
        assert_eq!(decode_rle(&rle, 4, 2), vec![7, 7, 7, 7, 1, 1, 1, 1]);
    }

    #[test]
    fn rle_zero_colour_runs_stay_transparent() {
        // A plain 3-long run of colour 0 must not overwrite with garbage.
        let rle = [0x00, 0x03, 0x00, 0x00];
        assert_eq!(decode_rle(&rle, 3, 1), vec![0, 0, 0]);
    }
}
