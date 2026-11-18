//! Anomi: a window's frames drawn without a window, as `present` draws them,
//! for measuring what a frame costs: each frame goes into the oldest of
//! `buffers` buffers taken in turn (two, as softbuffer's Wayland surface
//! keeps), redrawing only what differs from the frame that buffer holds.

use crate::Renderer;
use crate::core::{Color, Rectangle};
use crate::graphics::{Viewport, damage};

use super::compositor::{Frame, layers, merged};

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct Offscreen {
    buffers: Vec<Vec<u32>>,
    next: usize,
    drawn: usize,
    clip_mask: tiny_skia::Mask,
    frames: VecDeque<Frame>,
    width: u32,
    height: u32,
}

/// What a frame took.
#[derive(Debug, Default, Clone)]
pub struct Drawn {
    /// The regions redrawn, in logical pixels.
    pub regions: Vec<Rectangle>,
    /// Their area in the buffer's pixels.
    pub area: f32,
    /// Working the damage out, against the buffer's frame.
    pub diff: Duration,
    /// Drawing the regions.
    pub draw: Duration,
}

impl Offscreen {
    pub fn new(width: u32, height: u32, buffers: usize) -> Self {
        let pixels = width as usize * height as usize;
        Self {
            buffers: (0..buffers.max(1)).map(|_| vec![0; pixels]).collect(),
            next: 0,
            drawn: 0,
            clip_mask: tiny_skia::Mask::new(width, height).expect("a clip mask"),
            frames: VecDeque::new(),
            width,
            height,
        }
    }

    /// Draws the renderer's layers into the next buffer, as `present` does.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        viewport: &Viewport,
        background: Color,
    ) -> Drawn {
        let count = self.buffers.len();
        let age = if self.drawn >= count { count } else { 0 };
        let start = Instant::now();
        let bounds = Rectangle::with_size(viewport.logical_size());
        let damage = (age > 0)
            .then(|| self.frames.get(age - 1))
            .flatten()
            .filter(|frame| frame.background == background)
            .map(|frame| layers(&frame.layers, renderer.layers()))
            .unwrap_or_else(|| vec![bounds]);
        let damage = merged(damage::group(damage, bounds));
        let diff = start.elapsed();
        self.frames.push_front(Frame {
            background,
            layers: renderer.layers().to_vec(),
        });
        self.frames.truncate(count);
        let start = Instant::now();
        if !damage.is_empty() {
            let buffer = &mut self.buffers[self.next];
            let mut pixels = tiny_skia::PixmapMut::from_bytes(
                bytemuck::cast_slice_mut(buffer),
                self.width,
                self.height,
            )
            .expect("a pixel map");
            renderer.draw(
                &mut pixels,
                &mut self.clip_mask,
                viewport,
                &damage,
                background,
            );
        }
        let draw = start.elapsed();
        self.next = (self.next + 1) % count;
        self.drawn += 1;
        let scale = viewport.scale_factor();
        Drawn {
            area: damage
                .iter()
                .map(|region| region.area() * scale * scale)
                .sum(),
            regions: damage,
            diff,
            draw,
        }
    }

    /// The buffer drawn last, as straight RGBA.
    pub fn rgba(&self) -> Vec<u8> {
        let count = self.buffers.len();
        let last = &self.buffers[(self.next + count - 1) % count];
        last.iter()
            .flat_map(|pixel| {
                let [b, g, r, a] = pixel.to_le_bytes();
                [r, g, b, a]
            })
            .collect()
    }
}
