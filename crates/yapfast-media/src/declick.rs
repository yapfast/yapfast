//! Takes what DeepFilterNet leaves of a mouse click under speech. A click is
//! a burst above 4 kHz that jumps 25 dB or more above 12 kHz within half a
//! millisecond, where most of its energy is; speech's sounds there, such as an
//! "s" or a "t", are weaker against the rest of the voice and come on slower.
//! The suppressor turns a click down about 27 dB under speech, which still
//! leaves it as loud as the voice above 4 kHz in a quarter of clicks. So the
//! click is found in the suppressor's input, and when it comes out, 30 ms
//! later, the band above 4 kHz is held for up to 30 ms to the level it had
//! just before. Below 4 kHz, and everything outside clicks, passes as it was,
//! [`DELAY`] later: the bands are split by a linear-phase filter, since one
//! that shifts phase leaves much of a click between 4 and 10 kHz.
use std::collections::VecDeque;

/// Samples in a frame the levels are measured over: 0.5 ms at 48 kHz.
const FRAME: usize = 24;
/// Frames before a click that its context is measured over, ending 1 ms
/// before it: 10 ms.
const CONTEXT: usize = 20;
const GAP: usize = 2;
/// A click: the band above 4 kHz this many dB over its context, the band
/// above 12 kHz this many over its own, and the band above 12 kHz within this
/// many dB of the whole. Tuned on recorded mouse clicks over two readers'
/// speech: 107 of 121 clicks found. Eight more readers' speech set it off 1.2
/// times a minute; the prototype's first try, at -35 dB and a 15 dB jump, 14
/// times.
const JUMP: f32 = 12.0;
const JUMP_HIGH: f32 = 25.0;
const SHARE_HIGH: f32 = -15.0;
/// Quieter than this, in dBFS, the band above 4 kHz is never a click.
const FLOOR: f32 = -75.0;
/// The most frames a click's band is held down for: 30 ms.
const LONGEST: usize = 60;
/// Samples over which the band's gain moves to each frame's.
const RAMP: usize = 8;
/// Taps of the linear-phase high-pass that splits the output's bands, and
/// the samples it puts the output behind: 1 ms.
const TAPS: usize = 95;
pub(crate) const DELAY: usize = (TAPS - 1) / 2;

/// A windowed-sinc high-pass at 4 kHz, with a Hamming window.
fn split() -> Vec<f32> {
    let cutoff = 4000.0 / 24_000.0;
    let mut taps: Vec<f64> = (0..TAPS)
        .map(|index| {
            let t = index as f64 - DELAY as f64;
            let sinc = if t == 0.0 {
                cutoff
            } else {
                (std::f64::consts::PI * cutoff * t).sin() / (std::f64::consts::PI * t)
            };
            let window =
                0.54 - 0.46 * (std::f64::consts::TAU * index as f64 / (TAPS - 1) as f64).cos();
            sinc * window
        })
        .collect();
    // A low-pass of unit gain at 0 Hz, taken from an impulse.
    let sum: f64 = taps.iter().sum();
    for tap in &mut taps {
        *tap = -*tap / sum;
    }
    taps[DELAY] += 1.0;
    taps.into_iter().map(|tap| tap as f32).collect()
}

/// A fourth-order Butterworth high-pass, as two second-order sections.
#[derive(Clone)]
struct HighPass {
    sections: [[f32; 5]; 2],
    state: [[f32; 4]; 2],
}

impl HighPass {
    fn new(cutoff: f32) -> Self {
        let w = std::f32::consts::TAU * cutoff / 48_000.0;
        let section = |q: f32| {
            let alpha = w.sin() / (2.0 * q);
            let a0 = 1.0 + alpha;
            let b0 = (1.0 + w.cos()) / 2.0 / a0;
            [b0, -2.0 * b0, b0, -2.0 * w.cos() / a0, (1.0 - alpha) / a0]
        };
        Self {
            sections: [section(0.541_196_1), section(1.306_563)],
            state: [[0.0; 4]; 2],
        }
    }

    fn sample(&mut self, mut x: f32) -> f32 {
        for ([b0, b1, b2, a1, a2], [x1, x2, y1, y2]) in
            self.sections.iter().zip(self.state.iter_mut())
        {
            let y = b0 * x + b1 * *x1 + b2 * *x2 - a1 * *y1 - a2 * *y2;
            (*x2, *x1, *y2, *y1) = (*x1, x, *y1, y);
            x = y;
        }
        x
    }
}

fn decibels(energy: f32) -> f32 {
    10.0 * (energy / FRAME as f32 + 1e-14).log10()
}

fn median(values: impl Iterator<Item = f32>, scratch: &mut Vec<f32>) -> f32 {
    scratch.clear();
    scratch.extend(values);
    scratch.sort_unstable_by(f32::total_cmp);
    scratch[scratch.len() / 2]
}

pub(crate) struct Declicker {
    /// How many frames the suppressor's output is behind its input.
    lag: usize,
    /// The input's bands above 4 and 12 kHz, and its whole level, per frame,
    /// newest last, in dBFS.
    high: HighPass,
    higher: HighPass,
    levels: VecDeque<[f32; 3]>,
    /// Input frames heard, and the first a click may start at again.
    heard: usize,
    after: usize,
    /// Clicks found, as the output frame each starts at.
    found: VecDeque<usize>,
    /// The output's last samples, newest last, the high-pass that takes its
    /// band above 4 kHz, the band's levels, newest last, and the frame it's
    /// held down to a ceiling until.
    out_history: Vec<f32>,
    taps: Vec<f32>,
    out_levels: VecDeque<f32>,
    out_frames: usize,
    holding: Option<(usize, f32)>,
    gain: f32,
    band: Vec<f32>,
    scratch: Vec<f32>,
}

impl Declicker {
    /// For a suppressor `lag` samples behind its input.
    pub(crate) fn new(lag: usize) -> Self {
        Self {
            lag: (lag + DELAY) / FRAME,
            high: HighPass::new(4000.0),
            higher: HighPass::new(12_000.0),
            levels: VecDeque::with_capacity(CONTEXT + GAP + 3),
            heard: 0,
            after: 0,
            found: VecDeque::new(),
            out_history: vec![0.0; TAPS - 1],
            taps: split(),
            out_levels: VecDeque::with_capacity(CONTEXT + GAP + 1),
            out_frames: 0,
            holding: None,
            gain: 1.0,
            band: Vec::new(),
            scratch: Vec::with_capacity(CONTEXT),
        }
    }

    /// Hears the suppressor's input, in 16-bit range, for clicks.
    pub(crate) fn input(&mut self, block: &[f32]) {
        for frame in block.chunks_exact(FRAME) {
            let [mut high, mut higher, mut whole] = [0.0f32; 3];
            for &sample in frame {
                let sample = sample / 32768.0;
                let (h, hh) = (self.high.sample(sample), self.higher.sample(sample));
                high += h * h;
                higher += hh * hh;
                whole += sample * sample;
            }
            if self.levels.len() == CONTEXT + GAP + 3 {
                self.levels.pop_front();
            }
            self.levels
                .push_back([decibels(high), decibels(higher), decibels(whole)]);
            self.heard += 1;
            self.find();
        }
    }

    /// Looks for a click starting two frames back, now that the frames
    /// after it, where it peaks, have come.
    fn find(&mut self) {
        if self.levels.len() < CONTEXT + GAP + 3 || self.heard < self.after {
            return;
        }
        let at = CONTEXT + GAP;
        let level = |frame: usize, band: usize| self.levels[frame][band];
        let start = level(at, 0);
        let context = median((0..CONTEXT).map(|frame| level(frame, 0)), &mut self.scratch);
        if start <= FLOOR || start - context < JUMP {
            return;
        }
        let peak = (at..at + 3)
            .max_by(|a, b| level(*a, 0).total_cmp(&level(*b, 0)))
            .unwrap_or(at);
        let context_high = median((0..CONTEXT).map(|frame| level(frame, 1)), &mut self.scratch);
        if level(peak, 1) - context_high >= JUMP_HIGH
            && level(peak, 1) - level(peak, 2) >= SHARE_HIGH
        {
            // The input frame the click starts at, one before the jump.
            let frame = self.heard - 3;
            self.found.push_back((frame + self.lag).saturating_sub(1));
            self.after = self.heard + 20;
        }
    }

    /// Holds down the band above 4 kHz of the suppressor's output, in place,
    /// where clicks come out, and puts it [`DELAY`] samples later.
    pub(crate) fn output(&mut self, block: &mut [f32]) {
        self.out_history.extend_from_slice(block);
        for (index, frame) in block.chunks_exact_mut(FRAME).enumerate() {
            let offset = index * FRAME;
            for (index, sample) in frame.iter_mut().enumerate() {
                *sample = self.out_history[offset + index + TAPS - 1 - DELAY];
            }
            // The band, only over a click's context before it, while it's held
            // down and while the gain comes back: clicks are found long
            // enough ahead.
            let needed = self.holding.is_some()
                || self.gain < 1.0
                || self
                    .found
                    .front()
                    .is_some_and(|start| *start <= self.out_frames + CONTEXT + GAP);
            self.band.clear();
            let level = if needed {
                for index in offset..offset + FRAME {
                    let window = &self.out_history[index..index + TAPS];
                    let band: f32 = window.iter().zip(&self.taps).map(|(x, h)| x * h).sum();
                    self.band.push(band / 32768.0);
                }
                decibels(self.band.iter().map(|value| value * value).sum())
            } else {
                f32::NAN
            };
            if self
                .found
                .front()
                .is_some_and(|start| *start <= self.out_frames)
            {
                self.found.pop_front();
                if self.out_levels.len() > CONTEXT {
                    let ceiling = median(
                        self.out_levels.iter().rev().skip(1).take(CONTEXT).copied(),
                        &mut self.scratch,
                    );
                    if !ceiling.is_nan() {
                        self.holding = Some((self.out_frames + LONGEST, ceiling));
                    }
                }
            }
            let gain = match self.holding {
                Some((until, ceiling)) if self.out_frames < until && level > ceiling => {
                    10f32.powf((ceiling - level) / 20.0)
                }
                _ => 1.0,
            };
            if self
                .holding
                .is_some_and(|(until, _)| self.out_frames >= until)
            {
                self.holding = None;
            }
            if gain < 1.0 || self.gain < 1.0 {
                for (index, (sample, band)) in frame.iter_mut().zip(&self.band).enumerate() {
                    let step = (index as f32 / RAMP as f32).min(1.0);
                    let gain = self.gain + (gain - self.gain) * step;
                    *sample -= (1.0 - gain) * band * 32768.0;
                }
            }
            self.gain = gain;
            if self.out_levels.len() == CONTEXT + GAP {
                self.out_levels.pop_front();
            }
            self.out_levels.push_back(level);
            self.out_frames += 1;
        }
        self.out_history.drain(..block.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of quiet hiss, made the same each time.
    fn hiss(seed: &mut u32, level: f32) -> Vec<f32> {
        (0..480)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((*seed >> 8) as f32 / 16_777_216.0 - 0.5) * level
            })
            .collect()
    }

    #[test]
    fn holds_down_a_click_and_leaves_the_rest() {
        let mut declicker = Declicker::new(1440);
        let mut seed = 1;
        let mut blocks: Vec<Vec<f32>> = (0..20).map(|_| hiss(&mut seed, 60.0)).collect();
        // A click: a sharp, decaying burst in block 10, mostly above 4 kHz
        // as a mouse's is: differenced noise.
        let mut last = 0.0;
        for (index, sample) in blocks[10][200..320].iter_mut().enumerate() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / 16_777_216.0 - 0.5;
            *sample += (noise - last) * 10_000.0 * (-(index as f32) / 30.0).exp();
            last = noise;
        }
        let energy = |block: &[f32]| block.iter().map(|sample| sample * sample).sum::<f32>();
        // As a suppressor 3 blocks behind its input would give it back.
        let delayed: Vec<f32> = std::iter::repeat_n(0.0, 1440 + DELAY)
            .chain(blocks.iter().flatten().copied())
            .collect();
        let mut out = Vec::new();
        for index in 0..blocks.len() {
            declicker.input(&blocks[index]);
            let mut block = if index >= 3 {
                blocks[index - 3].clone()
            } else {
                vec![0.0; 480]
            };
            declicker.output(&mut block);
            out.push(block);
        }
        let expected = |index: usize| &delayed[index * 480..(index + 1) * 480];
        assert!(
            energy(&out[13]) < energy(expected(13)) / 10.0,
            "the click's block lost less than 10 dB"
        );
        // Untouched before the click comes out, and once 30 ms have passed.
        for index in [5, 8, 12, 17, 19] {
            assert_eq!(out[index], expected(index), "block {index}");
        }
    }

    #[test]
    fn leaves_steady_sound_alone() {
        let mut declicker = Declicker::new(1440);
        let mut seed = 3;
        let mut last = vec![0.0; 480];
        for index in 0..50 {
            let block = hiss(&mut seed, 2000.0 + 100.0 * index as f32);
            declicker.input(&block);
            let mut out = block.clone();
            declicker.output(&mut out);
            // The same, DELAY samples later.
            let expected: Vec<f32> = last[480 - DELAY..]
                .iter()
                .chain(&block[..480 - DELAY])
                .copied()
                .collect();
            assert_eq!(out, expected);
            last = block;
        }
    }
}
