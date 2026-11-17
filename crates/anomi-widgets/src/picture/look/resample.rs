//! Scaling part of a picture to a size, with an area average down and
//! bilinear up: each source row across to the copy's width, then down, one
//! row of the copy at a time. Besides the copy it holds only the source
//! rows the row being made reads, at the copy's width: a few rows, never
//! the picture.

use super::Crop;

/// For each pixel of a row or column of the copy, the source pixels it
/// reads and how much of each, flat: pixel `i` reads `weights[at[i]..at[i
/// + 1]]` of the pixels from `first[i]`.
struct Taps {
    first: Vec<usize>,
    at: Vec<usize>,
    weights: Vec<f32>,
}

impl Taps {
    /// `out` pixels from `extent` source pixels at `start`, of `len`: the
    /// area each covers when shrinking, the two nearest when growing.
    fn new(start: f32, extent: f32, len: u32, out: u32) -> Self {
        let last = len.saturating_sub(1) as usize;
        let clamp = |at: f32| (at.max(0.0) as usize).min(last);
        let scale = extent / out as f32;
        let mut taps = Self {
            first: Vec::with_capacity(out as usize),
            at: Vec::with_capacity(out as usize + 1),
            weights: Vec::new(),
        };
        taps.at.push(0);
        let mut each: Vec<(usize, f32)> = Vec::new();
        for index in 0..out {
            each.clear();
            if scale > 1.0 {
                let from = start + index as f32 * scale;
                let to = from + scale;
                let mut at = from.floor();
                while at < to {
                    let cover = (at + 1.0).min(to) - at.max(from);
                    if cover > 0.0 {
                        each.push((clamp(at), cover));
                    }
                    at += 1.0;
                }
            } else {
                let centre = start + (index as f32 + 0.5) * scale - 0.5;
                let below = centre.floor();
                let part = centre - below;
                each.push((clamp(below), 1.0 - part));
                each.push((clamp(below + 1.0), part));
            }
            let first = each.iter().map(|tap| tap.0).min().unwrap_or(0);
            let span = each.iter().map(|tap| tap.0).max().unwrap_or(0) - first + 1;
            let begin = taps.weights.len();
            taps.weights.resize(begin + span, 0.0);
            let row = &mut taps.weights[begin..];
            for &(at, weight) in &each {
                row[at - first] += weight;
            }
            let total: f32 = row.iter().sum();
            if total > 0.0 {
                row.iter_mut().for_each(|weight| *weight /= total);
            }
            taps.first.push(first);
            taps.at.push(taps.weights.len());
        }
        taps
    }

    /// The first source pixel pixel `index` reads, and the weights.
    fn of(&self, index: usize) -> (usize, &[f32]) {
        (
            self.first[index],
            &self.weights[self.at[index]..self.at[index + 1]],
        )
    }

    /// The most source pixels one pixel reads.
    fn widest(&self) -> usize {
        self.at
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .max()
            .unwrap_or(1)
    }
}

/// `crop` of a `width` by `height` picture, scaled to `w` by `h`. Colours
/// are weighed by their alpha, so a clear pixel's colour never shows.
pub(super) fn resample(
    rgba: &[u8],
    (width, height): (u32, u32),
    crop: Crop,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let columns = Taps::new(crop.x, crop.width, width, w);
    let rows = Taps::new(crop.y, crop.height, height, h);
    let (stride, from) = (w as usize * 4, width as usize * 4);
    // The source rows read lately, across, each at `row % held`: those the
    // row being made reads are among them, as rows only go down.
    let held = rows.widest();
    let mut ring = vec![0f32; held * stride];
    let mut sum = vec![0f32; stride];
    let mut next = 0;
    let mut out = vec![0u8; h as usize * stride];
    for (y, line) in out.chunks_exact_mut(stride).enumerate() {
        let (first, weights) = rows.of(y);
        next = next.max(first);
        while next < first + weights.len() {
            let source = &rgba[next * from..(next + 1) * from];
            let slot = next % held;
            across(
                source,
                &columns,
                &mut ring[slot * stride..(slot + 1) * stride],
            );
            next += 1;
        }
        sum.fill(0.0);
        for (offset, &weight) in weights.iter().enumerate() {
            let slot = (first + offset) % held;
            let row = &ring[slot * stride..(slot + 1) * stride];
            for (total, &value) in sum.iter_mut().zip(row) {
                *total += value * weight;
            }
        }
        // Rounded by adding a half: `round` is a call per channel on a
        // processor without SSE4.1, and a cast holds the value to a byte.
        for (pixel, total) in line.chunks_exact_mut(4).zip(sum.chunks_exact(4)) {
            let alpha = total[3];
            if alpha > 0.0 {
                let inverse = 1.0 / alpha;
                for channel in 0..3 {
                    pixel[channel] = (total[channel] * inverse + 0.5) as u8;
                }
            }
            pixel[3] = (alpha + 0.5) as u8;
        }
    }
    out
}

/// One source row at the copy's width, each colour weighed by its alpha.
fn across(source: &[u8], columns: &Taps, line: &mut [f32]) {
    for (x, out) in line.chunks_exact_mut(4).enumerate() {
        let (first, weights) = columns.of(x);
        let pixels = &source[first * 4..(first + weights.len()) * 4];
        let mut sum = [0f32; 4];
        for (pixel, &weight) in pixels.chunks_exact(4).zip(weights) {
            let alpha = f32::from(pixel[3]) * weight;
            sum[0] += f32::from(pixel[0]) * alpha;
            sum[1] += f32::from(pixel[1]) * alpha;
            sum[2] += f32::from(pixel[2]) * alpha;
            sum[3] += alpha;
        }
        out.copy_from_slice(&sum);
    }
}
