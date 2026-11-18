//! What a frame of the window costs, with no window: the whole window from
//! its sample view-model, driven as iced's event loop drives it (events,
//! `update` and a new view when a message comes, the frame's own update,
//! `draw`) and drawn into two buffers taken in turn, each redrawing only
//! what differs from the frame it holds, as the window does. Frames are
//! paced at `--fps`, so the busy share of each second is the main thread's
//! CPU. Measure with `--profile fast` or `measure`.
//!
//!     cargo run --profile fast -p anomi-shots --example frames -- CASE [--frames 240]
//!       [--fps 60] [--events 1] [--delta 25] [--lines] [--scale 1] [--png OUT.png]
//!
//! CASE is `scroll` (#general's history of 400 messages, a 480x320 picture
//! in every fourth as the fake's `--images 4` sends, scrolled by `--events`
//! reports of `--delta` pixels a frame, or a wheel's notches with `--lines`;
//! up for two seconds, then down, in turn), `presence` (a member list of 300
//! dressed as the fake's `--member-looks`, one member in view changing
//! presence each frame) or `churn` (one member going offline or coming back
//! each frame, as the fake's churn picks them, which moves the rows between).
//!
//! It prints the time a frame took by step and the renderer's by kind of
//! primitive (`iced_tiny_skia::stats`), and what the picture store and the
//! renderer hold. `--verify` draws each frame whole too and tells where the
//! buffer differs; `FRAMES_REGIONS=1` prints each frame's damaged regions,
//! `FRAMES_MESSAGES=1` the messages the window sent.

#[path = "frames/members.rs"]
mod members;
#[path = "frames/report.rs"]
mod report;
#[path = "frames/scroll.rs"]
mod scroll;

use std::time::{Duration, Instant};

use iced::advanced::renderer::{self, Headless};
use iced::advanced::{mouse, shell};
use iced::theme::Base;
use iced::{Event, Point, Size, window};
use iced_runtime::{UserInterface, user_interface};
use iced_tiny_skia::graphics::Viewport;
use iced_tiny_skia::window::offscreen::Offscreen;

use anomi_screens::shots;
use anomi_theme::{Motion, Renderer, Theme, fonts, typography};
use anomi_vm::{Intent, Vm};
use anomi_widgets::picture;

pub struct Options {
    pub frames: usize,
    pub fps: f64,
    pub events: usize,
    pub delta: f32,
    pub lines: bool,
    pub scale: f32,
    pub png: Option<String>,
    pub verify: bool,
}

/// The time each step took over the frames timed, and what was redrawn.
#[derive(Default)]
pub struct Totals {
    pub frames: usize,
    pub events: Duration,
    pub views: usize,
    pub view: Duration,
    pub build: Duration,
    pub redraw: Duration,
    pub layers: Duration,
    pub diff: Duration,
    pub draw: Duration,
    pub area: f64,
}

pub struct Window {
    pub renderer: Renderer,
    pub offscreen: Offscreen,
    pub viewport: Viewport,
    pub size: Size,
    pub theme: Theme,
    pub motion: Motion,
    pub cursor: mouse::Cursor,
    pub messages: shell::Bus<Intent>,
    /// The messages that came since the app last took them.
    pub inbox: Vec<Intent>,
    pub totals: Totals,
    pub verify: bool,
    pub wrong: usize,
}

pub type Ui<'a> = UserInterface<'a, Intent, Theme, Renderer>;

impl Window {
    fn new(options: &Options, motion: Motion) -> Self {
        let size = Size::new(shots::GOLDEN.0, shots::GOLDEN.1);
        let settings = renderer::Settings {
            font: typography::UI,
            ..renderer::Settings::default()
        };
        let renderer = block_on(<Renderer as Headless>::new(settings, None)).expect("a renderer");
        let physical = Size::new(
            (size.width * options.scale).round() as u32,
            (size.height * options.scale).round() as u32,
        );
        let scale = renderer::Scale {
            window: options.scale,
            application: 1.0,
        };
        Self {
            renderer,
            offscreen: Offscreen::new(physical.width, physical.height, 2),
            viewport: Viewport::with_physical_size(physical, scale),
            size,
            theme: Theme::built_in(Some("bonfire")),
            motion,
            cursor: mouse::Cursor::Available(Point::new(700.0, 450.0)),
            messages: shell::Bus::new(),
            inbox: Vec::new(),
            totals: Totals::default(),
            verify: options.verify,
            wrong: 0,
        }
    }

    /// Builds the window's view of `vm` with what the last one left.
    pub fn build<'a>(&mut self, vm: &'a Vm, cache: user_interface::Cache) -> Ui<'a> {
        let start = Instant::now();
        let view = anomi_screens::window(vm, self.motion);
        let built = Instant::now();
        let ui = UserInterface::build(view, self.size, cache, &mut self.renderer);
        self.totals.views += 1;
        self.totals.view += built - start;
        self.totals.build += built.elapsed();
        ui
    }

    /// Hands `events` to the window; true when a message came or the
    /// widgets asked for a new view, as iced then runs `update` and builds
    /// the view again.
    pub fn events(&mut self, ui: &mut Ui<'_>, events: &[Event]) -> bool {
        let start = Instant::now();
        let (state, _) = ui.update(
            &window::Headless,
            &shell::Waker::noop(),
            events,
            self.cursor,
            &mut self.renderer,
            &mut self.messages,
        );
        self.totals.events += start.elapsed();
        self.outdated(state)
    }

    /// The frame's own update, as iced's loop sends it.
    pub fn redraw(&mut self, ui: &mut Ui<'_>, now: Instant) -> bool {
        let start = Instant::now();
        let event = Event::Window(window::Event::RedrawRequested(now));
        let (state, _) = ui.update(
            &window::Headless,
            &shell::Waker::noop(),
            &[event],
            self.cursor,
            &mut self.renderer,
            &mut self.messages,
        );
        self.totals.redraw += start.elapsed();
        self.outdated(state)
    }

    fn outdated(&mut self, state: user_interface::State) -> bool {
        let new = !self.messages.is_empty() || matches!(state, user_interface::State::Outdated);
        for (message, _) in self.messages.drain() {
            if std::env::var_os("FRAMES_MESSAGES").is_some() {
                let text = format!("{message:?}");
                println!("{}", &text[..text.len().min(160)]);
            }
            self.inbox.push(message);
        }
        new
    }

    /// Draws the window and redraws what changed in the next buffer.
    pub fn draw(&mut self, ui: &mut Ui<'_>) {
        let start = Instant::now();
        let base = self.theme.base();
        ui.draw(
            &mut self.renderer,
            &self.theme,
            &renderer::Style {
                text_color: base.text_color,
            },
            self.cursor,
        );
        self.totals.layers += start.elapsed();
        let drawn = self
            .offscreen
            .draw(&mut self.renderer, &self.viewport, base.background_color);
        if std::env::var_os("FRAMES_REGIONS").is_some() {
            let regions: Vec<String> = drawn
                .regions
                .iter()
                .map(|r| format!("{:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.width, r.height))
                .collect();
            println!("regions {}", regions.join(" | "));
        }
        if self.verify {
            // The frame drawn whole, as a check that the damage missed
            // nothing: it must equal what the buffer holds.
            let size = self.viewport.physical_size();
            let whole =
                self.renderer
                    .screenshot(size, self.viewport.scale_factor(), base.background_color);
            let held = self.offscreen.rgba();
            let width = size.width as usize;
            let (mut wrong, mut most) = (0, 0u8);
            let mut box_ = (usize::MAX, usize::MAX, 0, 0);
            for (at, (a, b)) in whole.chunks(4).zip(held.chunks(4)).enumerate() {
                if a != b {
                    wrong += 1;
                    let (x, y) = (at % width, at / width);
                    box_ = (box_.0.min(x), box_.1.min(y), box_.2.max(x), box_.3.max(y));
                    most = most.max(
                        a.iter()
                            .zip(b)
                            .map(|(p, q)| p.abs_diff(*q))
                            .max()
                            .unwrap_or(0),
                    );
                }
            }
            if wrong > 0 {
                self.wrong += 1;
                println!(
                    "frame {}: {wrong} pixels differ from the frame drawn whole, in {:?}, by up to {most}",
                    self.totals.frames, box_
                );
            }
        }
        self.totals.diff += drawn.diff;
        self.totals.draw += drawn.draw;
        self.totals.area += f64::from(drawn.area);
        self.totals.frames += 1;
    }

    pub fn png(&self, path: &str) {
        let size = self.viewport.physical_size();
        let file = std::io::BufWriter::new(std::fs::File::create(path).expect("the PNG file"));
        let mut encoder = png::Encoder::new(file, size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("a PNG header");
        writer
            .write_image_data(&self.offscreen.rgba())
            .expect("the PNG's pixels");
    }
}

/// Waits out the rest of the frame's time at `fps`.
pub fn pace(started: Instant, frame: usize, fps: f64) {
    let due = started + Duration::from_secs_f64((frame + 1) as f64 / fps);
    if let Some(left) = due.checked_duration_since(Instant::now()) {
        std::thread::sleep(left);
    }
}

fn main() {
    let mut case = String::new();
    let mut options = Options {
        frames: 240,
        fps: 60.0,
        events: 1,
        delta: 25.0,
        lines: false,
        scale: 1.0,
        png: None,
        verify: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || arguments.next().expect("a value");
        match argument.as_str() {
            "--frames" => options.frames = value().parse().expect("a count"),
            "--fps" => options.fps = value().parse().expect("a rate"),
            "--events" => options.events = value().parse().expect("a count"),
            "--delta" => options.delta = value().parse().expect("pixels"),
            "--lines" => options.lines = true,
            "--scale" => options.scale = value().parse().expect("a scale"),
            "--png" => options.png = Some(value()),
            "--verify" => options.verify = true,
            other => case = other.to_owned(),
        }
    }
    fonts::install();
    picture::set_scale(options.scale);
    let motion = Motion {
        front: true,
        never: !options.lines,
        idle: false,
    };
    let mut window = Window::new(&options, motion);
    iced_tiny_skia::stats::enable(true);
    let started = Instant::now();
    match case.as_str() {
        "scroll" => scroll::run(&mut window, &options),
        "presence" | "churn" => members::run(&mut window, &options, case == "churn"),
        _ => {
            eprintln!("CASE is scroll, presence or churn");
            std::process::exit(2);
        }
    }
    let wall = started.elapsed().as_secs_f64();
    report::report(&window.totals, wall);
    report::kinds(window.totals.frames);
    println!(
        "held: picture store {:.2} MiB, renderer's converted copies {:.2} MiB",
        picture::bytes() as f64 / f64::from(1 << 20),
        iced_tiny_skia::converted_bytes() as f64 / f64::from(1 << 20),
    );
    if options.verify {
        println!(
            "verified: {} frames differ from the frame drawn whole",
            window.wrong
        );
    }
    if let Some(path) = &options.png {
        window.png(path);
    }
}

/// Waits for a future on this thread; the headless renderer is ready at once.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}
