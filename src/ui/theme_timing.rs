//! How long a theme switch takes, from the click to the first frames drawn
//! with it, in the log: each step of a switch says how long after the click
//! it came, so a slow one shows where its time went.
//!
//! `YAPFAST_FRAME_LOG=1` also logs how many frames were drawn each five
//! seconds, to find what keeps redrawing a window that looks still.

use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, RenderingState};
use yapfast_ui::YapfastWindow;

/// The switch going on: what was clicked, and when. A static, so the thread
/// that makes a theme ready can say when it started and finished.
static SWITCH: Mutex<Option<(Instant, &'static str)>> = Mutex::new(None);

/// How many frames after the values were set are still logged.
const FRAMES: u8 = 3;

thread_local! {
    static FRAMES_LEFT: Cell<u8> = const { Cell::new(0) };
    static FRAME_START: Cell<Option<Instant>> = const { Cell::new(None) };
    static COUNT: Cell<(u32, Option<Instant>)> = const { Cell::new((0, None)) };
}

fn switch() -> Option<(Instant, &'static str)> {
    *SWITCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A switch starts: `what` was clicked.
pub fn start(what: &'static str) {
    *SWITCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((Instant::now(), what));
    FRAMES_LEFT.set(0);
    tracing::info!(what, "theme switch: clicked");
}

/// A step of the switch going on, if there is one, from any thread.
pub fn step(step: &'static str) {
    if let Some((started, what)) = switch() {
        tracing::info!(
            what,
            step,
            ms = started.elapsed().as_millis() as u64,
            "theme switch"
        );
    }
}

/// The switch's values are set: the next frames drawn are logged.
pub fn values_set() {
    if switch().is_some() {
        step("values set");
        FRAMES_LEFT.set(FRAMES);
    }
}

/// Logs the frames a switch draws, and with `YAPFAST_FRAME_LOG` how many
/// frames are drawn.
pub fn watch(window: &YapfastWindow) {
    let counting = std::env::var_os("YAPFAST_FRAME_LOG").is_some();
    let watched = window
        .window()
        .set_rendering_notifier(move |state, _| match state {
            RenderingState::BeforeRendering => {
                if FRAMES_LEFT.get() == FRAMES {
                    step("first frame starts");
                }
                FRAME_START.set(Some(Instant::now()));
            }
            RenderingState::AfterRendering => {
                let left = FRAMES_LEFT.get();
                if left > 0
                    && let Some((started, what)) = switch()
                {
                    let frame = FRAME_START.get().map_or(0, |at| at.elapsed().as_millis());
                    tracing::info!(
                        what,
                        frame = FRAMES - left + 1,
                        frame_ms = frame as u64,
                        ms = started.elapsed().as_millis() as u64,
                        "theme switch: frame drawn"
                    );
                    FRAMES_LEFT.set(left - 1);
                    if left == 1 {
                        *SWITCH
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                    }
                }
                if counting {
                    let (count, since) = COUNT.get();
                    let since = since.unwrap_or_else(Instant::now);
                    if since.elapsed() >= Duration::from_secs(5) {
                        tracing::info!(frames = count + 1, "frames drawn in 5 s");
                        COUNT.set((0, Some(Instant::now())));
                    } else {
                        COUNT.set((count + 1, Some(since)));
                    }
                }
            }
            _ => {}
        });
    if let Err(error) = watched {
        tracing::info!(?error, "no frame timings: the renderer has no notifier");
    }
}
