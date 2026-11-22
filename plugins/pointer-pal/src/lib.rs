//! Pointer Pal: a cat that chases the pointer over the window, the oneko
//! idea, drawn on the overlay.
//!
//! The cat is five frames of one picture asset (`cat.png`): sitting, two of
//! running, asleep and jumping. Each frame the overlay hands it, with where
//! the pointer is, it runs a few pixels towards it, sits once it is there,
//! and after ten seconds of a still pointer falls asleep, when it asks for two
//! frames a second rather than twelve, just enough to notice the pointer move
//! again. Clicking it makes it jump. Nothing it draws takes a click but the
//! cat.

use yapfast_pdk::overlay::{self, Sprite};
use yapfast_pdk::prelude::*;

/// One frame of the sheet is 32 pixels square; it is drawn at twice that.
const CELL: u32 = 32;
const SHOWN: f32 = 48.0;
/// How far it runs in a frame, and how close counts as caught.
const STEP: f32 = 7.0;
const NEAR: f32 = 28.0;
const AWAKE_FPS: u8 = 12;
const ASLEEP_FPS: u8 = 2;
const SLEEP_AFTER_MS: u64 = 10_000;

#[derive(Default)]
struct PointerPal {
    x: f32,
    y: f32,
    placed: bool,
    frame: u32,
    pointer: Option<(i32, i32)>,
    still_since: u64,
    asleep: bool,
    jump: u8,
}

impl PointerPal {
    fn draw(&self) -> Result<()> {
        let cell = if self.jump > 0 {
            4
        } else if self.asleep {
            3
        } else {
            self.frame
        };
        let cat = Sprite {
            id: "cat".into(),
            image: "cat".into(),
            x: self.x,
            y: self.y,
            width: Some(SHOWN),
            height: Some(SHOWN),
            crop: Some([cell * CELL, 0, CELL, CELL]),
            clickable: true,
            ..Sprite::default()
        };
        overlay::set(
            &[cat],
            if self.asleep { ASLEEP_FPS } else { AWAKE_FPS },
        )
    }

    fn step(&mut self, at_ms: u64, pointer: Option<(i32, i32)>, size: (u32, u32)) {
        if !self.placed {
            self.x = size.0 as f32 / 2.0;
            self.y = size.1 as f32 / 2.0;
            self.placed = true;
            self.still_since = at_ms;
        }
        if pointer != self.pointer {
            self.pointer = pointer;
            self.still_since = at_ms;
            self.asleep = false;
        }
        self.jump = self.jump.saturating_sub(1);
        let Some((px, py)) = pointer else {
            // The pointer left the window: it sits where it is.
            self.frame = 0;
            return;
        };
        // It aims to sit beside the pointer, not under it.
        let (tx, ty) = (px as f32 - SHOWN / 2.0, py as f32 + 8.0);
        let (dx, dy) = (tx - self.x, ty - self.y);
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > NEAR {
            self.x += dx / distance * STEP;
            self.y += dy / distance * STEP;
            self.frame = if self.frame == 1 { 2 } else { 1 };
        } else {
            self.frame = 0;
            if at_ms.saturating_sub(self.still_since) > SLEEP_AFTER_MS {
                self.asleep = true;
            }
        }
    }
}

impl Plugin for PointerPal {
    fn activate(&mut self, _ctx: &Context<'_>, _activation: Activation) -> Result<()> {
        // Until the first frame says how large the window is, it waits
        // off-screen; the first frame places it in the middle.
        self.x = -SHOWN;
        self.y = -SHOWN;
        self.draw()
    }

    fn input(&mut self, _ctx: &Context<'_>, input: Input) {
        match input {
            Input::OverlayFrame {
                at_ms,
                pointer,
                width,
                height,
            } => self.step(at_ms, pointer, (width, height)),
            Input::OverlayClick { .. } => {
                self.jump = 4;
                self.asleep = false;
            }
            _ => return,
        }
        if let Err(error) = self.draw() {
            log::error(format!("the cat couldn't be drawn: {error}"));
        }
    }
}

yapfast_plugin!(PointerPal);
