//! A theme's mouse cursors on the window (`docs/themes.md`, "Cursors").
//! [`load`] decodes each picture once, when a theme is shown; [`show`] hands
//! them to Slint's core (`yapfast_cursors` in `vendor/i-slint-core`), which
//! gives the window's adapter a theme cursor in place of a system one of the
//! same kind, and the winit backend shows it as a custom cursor on Windows,
//! X11, Wayland and macOS alike. [`hide`] puts the system's cursors back, as
//! `theme_look`'s reset does for the rest of a theme, so a protected dialog
//! shows them.
//!
//! A theme without cursors sets nothing: the core's hook stays empty and
//! every cursor is the system's, as before.
//!
//! A theme's animated cursors (frames, transitions, pointer motion) swap
//! the real system cursor's picture: [`Handler`] sees the window's pointer
//! events and, after each turn of the event loop, gives winit the frame
//! that should show, each frame made into a winit cursor once and reused.
//! A timer wakes the loop at the next frame's time only while something
//! plays; with the pointer still and no idle loop, nothing runs. The trail
//! moves sprites in the `CursorTrail` global while some are alive.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use i_slint_core::items::BuiltInMouseCursor;
use slint::winit_030::winit::event::{ElementState, MouseButton, WindowEvent};
use slint::winit_030::winit::event_loop::ActiveEventLoop;
use slint::winit_030::winit::window::{CursorIcon, CustomCursor};
use slint::winit_030::{CustomApplicationHandler, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model as _, Rgba8Pixel, SharedPixelBuffer, VecModel};
use yapfast_core::themes::Performance;
use yapfast_core::themes::cursors::{self, Animated, Kind};
use yapfast_core::themes::package::Resolved;
use yapfast_ui::{CursorTrail, TrailSprite, YapfastWindow};

/// One kind's cursor: each size's picture and width, smallest first, and
/// its hotspot on each.
struct Sizes(Vec<(Image, u32, (i32, i32))>);

#[derive(Default)]
struct State {
    cursors: Rc<Vec<(BuiltInMouseCursor, Sizes)>>,
    /// The person's switch for the shown theme.
    off: bool,
    /// The theme is painted: no protected dialog hides it.
    painted: bool,
    /// The core has the theme's cursors.
    given: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    /// The animated cursors while they play: see [`Handler`].
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    /// What's loaded, until the theme's cursors are shown or hidden.
    static LOADED: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<slint::Weak<YapfastWindow>>> = const { RefCell::new(None) };
    /// Whether motion may play: not under reduced motion or Light.
    static MOVES: Cell<bool> = const { Cell::new(true) };
    static TRAIL_ON: Cell<bool> = const { Cell::new(true) };
    /// Milliseconds between the trail's steps: 60 a second in Full, 30 in
    /// Balanced.
    static TRAIL_MS: Cell<u64> = const { Cell::new(16) };
    /// Wakes the event loop at the next frame's time.
    static WAKE: slint::Timer = slint::Timer::default();
}

/// The Slint cursor of a kind.
fn builtin(kind: Kind) -> BuiltInMouseCursor {
    use BuiltInMouseCursor as Cursor;
    match kind {
        Kind::Default => Cursor::Default,
        Kind::Pointer => Cursor::Pointer,
        Kind::Text => Cursor::Text,
        Kind::Grab => Cursor::Grab,
        Kind::Grabbing => Cursor::Grabbing,
        Kind::NotAllowed => Cursor::NotAllowed,
        Kind::NoDrop => Cursor::NoDrop,
        Kind::Wait => Cursor::Wait,
        Kind::Progress => Cursor::Progress,
        Kind::Help => Cursor::Help,
        Kind::Crosshair => Cursor::Crosshair,
        Kind::Move => Cursor::Move,
        Kind::Copy => Cursor::Copy,
        Kind::Alias => Cursor::Alias,
        Kind::ColResize => Cursor::ColResize,
        Kind::RowResize => Cursor::RowResize,
        Kind::EwResize => Cursor::EwResize,
        Kind::NsResize => Cursor::NsResize,
        Kind::NeswResize => Cursor::NeswResize,
        Kind::NwseResize => Cursor::NwseResize,
        Kind::NResize => Cursor::NResize,
        Kind::EResize => Cursor::EResize,
        Kind::SResize => Cursor::SResize,
        Kind::WResize => Cursor::WResize,
        Kind::NeResize => Cursor::NeResize,
        Kind::NwResize => Cursor::NwResize,
        Kind::SeResize => Cursor::SeResize,
        Kind::SwResize => Cursor::SwResize,
    }
}

/// Decodes the shown theme's cursors, each file once. A cursor whose
/// pictures don't all load keeps the system's, so a theme never loses its
/// pointer to a bad file.
pub fn load(resolved: &Resolved, folder: &Path) {
    let mut decoded: BTreeMap<&PathBuf, Option<(Image, u32, u32)>> = BTreeMap::new();
    let mut cursors = Vec::new();
    for (kind, cursor) in &resolved.cursors {
        let mut sizes = Vec::new();
        // An animated kind's first frame, cut from its sheet.
        if let Some(cell) = cursor.cell {
            let mut first_width = 0;
            for path in &cursor.images {
                let picture = std::fs::read(folder.join(path))
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| cursors::decode_sheet(&bytes))
                    .ok()
                    .and_then(|sheet| {
                        if first_width == 0 {
                            first_width = sheet.width;
                        }
                        cursors::cut(&sheet, cell, (sheet.width / first_width.max(1)).max(1))
                    });
                sizes.push(picture.map(|picture| {
                    let (width, height) = (picture.width, picture.height);
                    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                        &picture.rgba,
                        width,
                        height,
                    );
                    (Image::from_rgba8(buffer), width, height)
                }));
            }
        } else {
            for path in &cursor.images {
                let picture = decoded.entry(path).or_insert_with(|| {
                let full = folder.join(path);
                let picture = std::fs::read(&full)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| cursors::decode(&bytes));
                match picture {
                    Ok(picture) => {
                        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &picture.rgba,
                            picture.width,
                            picture.height,
                        );
                        Some((Image::from_rgba8(buffer), picture.width, picture.height))
                    }
                    Err(error) => {
                        tracing::warn!(file = %full.display(), %error, "a theme cursor didn't load");
                        None
                    }
                }
            });
                sizes.push(picture.clone());
            }
        }
        let Some(sizes) = sizes.into_iter().collect::<Option<Vec<_>>>() else {
            continue;
        };
        let first = (sizes[0].1, sizes[0].2);
        let sizes = sizes
            .into_iter()
            .map(|(image, width, height)| {
                let (x, y) = cursors::scaled_hotspot(cursor.hotspot, first, (width, height));
                (image, width, (x as i32, y as i32))
            })
            .collect();
        cursors.push((builtin(*kind), Sizes(sizes)));
    }
    STATE.with_borrow_mut(|state| state.cursors = Rc::new(cursors));
    // A theme shown before stops; this one's starts as `update` says.
    if let Some(mut running) = ENGINE.take() {
        running.stop();
    }
    LOADED.set(
        (!resolved.animated_cursors.is_still())
            .then(|| Engine::new(resolved, &resolved.animated_cursors, folder)),
    );
    update();
}

/// The window whose cursor animates; called once per window.
pub fn init(window: &YapfastWindow) {
    WINDOW.set(Some(window.as_weak()));
}

/// Whether the animated cursors and the trail may move: still under
/// reduced motion and in Light, and no trail there either.
pub fn set_motion(performance: Performance, reduced: bool) {
    let moves = !reduced && performance != Performance::Light;
    if MOVES.replace(moves) != moves {
        update();
    }
    TRAIL_ON.set(moves);
    TRAIL_MS.set(if performance == Performance::Full {
        16
    } else {
        33
    });
}

/// The window's pace changed (see `theme_pace`): the next turn of the
/// loop shows the frame for it, the still picture when held.
pub fn repace() {
    ENGINE.with(|engine| {
        if let Ok(mut engine) = engine.try_borrow_mut()
            && let Some(engine) = engine.as_mut()
        {
            engine.force = true;
            // Wakes the loop for it.
            WAKE.with(|timer| {
                timer.start(slint::TimerMode::SingleShot, Duration::ZERO, || {});
            });
        }
    });
}

/// Whether the shown theme has cursors.
pub fn has_cursors() -> bool {
    STATE.with_borrow(|state| !state.cursors.is_empty())
}

/// Shows the theme's cursors, unless the person turned them off.
pub fn show() {
    STATE.with_borrow_mut(|state| state.painted = true);
    update();
}

/// Puts the system's cursors back until [`show`].
pub fn hide() {
    STATE.with_borrow_mut(|state| state.painted = false);
    update();
}

/// The person's switch for the shown theme's cursors.
pub fn set_on(on: bool) {
    STATE.with_borrow_mut(|state| state.off = !on);
    update();
}

/// Forgets the theme's cursors.
pub fn forget() {
    STATE.with_borrow_mut(|state| {
        state.cursors = Rc::default();
        state.painted = false;
    });
    LOADED.set(None);
    if let Some(mut running) = ENGINE.take() {
        running.stop();
    }
    update();
}

/// Gives the core the theme's cursors, or takes them back, when that
/// changes.
fn update() {
    let wanted = STATE.with_borrow(|state| {
        (state.painted && !state.off && !state.cursors.is_empty()).then(|| state.cursors.clone())
    });
    let given = STATE.with_borrow(|state| state.given);
    if wanted.is_none() && !given {
        return;
    }
    STATE.with_borrow_mut(|state| state.given = wanted.is_some());
    // The animated cursors play only while the still ones show, and motion
    // may move.
    let animate = wanted.is_some() && MOVES.get();
    let running = ENGINE.with_borrow(Option::is_some);
    if animate != running {
        if animate {
            ENGINE.set(LOADED.with_borrow_mut(Option::take));
        } else if let Some(mut engine) = ENGINE.with_borrow_mut(Option::take) {
            engine.stop();
            LOADED.set(Some(engine));
        }
    }
    let hook = wanted.map(|cursors| -> i_slint_core::yapfast_cursors::Hook {
        Box::new(move |kind, scale| {
            // An animated cursor's frame, if one plays for this kind.
            let frame = ENGINE.with(|engine| {
                let mut engine = engine.try_borrow_mut().ok()?;
                engine.as_mut()?.hook(kind, scale)
            });
            if frame.is_some() {
                return frame;
            }
            let (_, Sizes(sizes)) = cursors.iter().find(|(shown, _)| *shown == kind)?;
            let widths: Vec<u32> = sizes.iter().map(|(_, width, _)| *width).collect();
            let (image, _, (x, y)) = &sizes[cursors::pick(&widths, scale)];
            Some((image.clone(), *x, *y))
        })
    });
    i_slint_core::yapfast_cursors::set(hook);
}

/// Frames that play in turn, as indexes into the engine's frames.
struct Seq {
    frames: Vec<usize>,
    ms: Vec<u32>,
    looped: bool,
}

impl Seq {
    fn of(sequence: &cursors::Sequence, frames: &mut Frames) -> Self {
        Self {
            frames: sequence
                .frames
                .iter()
                .map(|frame| frames.add(frame, sequence.hotspot))
                .collect(),
            ms: sequence.ms.clone(),
            looped: sequence.looped,
        }
    }

    /// The frame at `elapsed`, and how long until the next; none once a
    /// sequence that plays once is over.
    fn at(&self, elapsed: Duration) -> Option<(usize, Duration)> {
        let elapsed = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);
        let total = self.ms.iter().sum::<u32>().max(1);
        let mut left = if self.looped {
            elapsed % total
        } else if elapsed >= total {
            return None;
        } else {
            elapsed
        };
        for (index, ms) in self.ms.iter().enumerate() {
            if left < *ms {
                return Some((
                    self.frames[index],
                    Duration::from_millis(u64::from(ms - left)),
                ));
            }
            left -= ms;
        }
        None
    }

    /// Milliseconds before frame `index` starts.
    fn start_of(&self, index: usize) -> Duration {
        Duration::from_millis(u64::from(self.ms.iter().take(index).sum::<u32>()))
    }
}

/// A frame's pixels at one size, and its hotspot there.
type Pixels = (SharedPixelBuffer<Rgba8Pixel>, (i32, i32));

/// Every frame the theme's animated cursors use, decoded only once one
/// first shows, at the size the screen's scale wants, and made into a
/// winit cursor once.
struct Frames {
    folder: PathBuf,
    paths: Vec<(cursors::Frame, (u32, u32))>,
    /// Each size's pixels across and down, read from the files' headers.
    dims: HashMap<usize, Vec<(u32, u32)>>,
    /// Decoded pixels by frame and size; `None` for one that didn't load.
    pixels: HashMap<(usize, usize), Option<Pixels>>,
    cursors: HashMap<(usize, usize), CustomCursor>,
}

impl Frames {
    fn add(&mut self, frame: &cursors::Frame, hotspot: (u32, u32)) -> usize {
        if let Some(index) = self
            .paths
            .iter()
            .position(|(known, spot)| known == frame && *spot == hotspot)
        {
            return index;
        }
        self.paths.push((frame.clone(), hotspot));
        self.paths.len() - 1
    }

    /// Each size of `frame`, from its files' headers, read once: a cell's
    /// is the cell's at that size of its sheet.
    fn dims(&mut self, frame: usize) -> &[(u32, u32)] {
        let (folder, paths) = (&self.folder, &self.paths);
        self.dims.entry(frame).or_insert_with(|| {
            let (shown, _) = &paths[frame];
            let files: Vec<(u32, u32)> = shown
                .files
                .iter()
                .map(|path| image::image_dimensions(folder.join(path)).unwrap_or((0, 0)))
                .collect();
            match shown.cell {
                Some(cell) => {
                    let first = files.first().map_or(1, |size| size.0.max(1));
                    files
                        .iter()
                        .map(|(width, _)| {
                            let scale = (width / first).max(1);
                            (cell.width * scale, cell.height * scale)
                        })
                        .collect()
                }
                None => files,
            }
        })
    }

    /// Which size of `frame` shows at `scale`.
    fn size_at(&mut self, frame: usize, scale: f32) -> usize {
        if self.paths[frame].0.files.len() == 1 {
            return 0;
        }
        let widths: Vec<u32> = self.dims(frame).iter().map(|(width, _)| *width).collect();
        cursors::pick(&widths, scale)
    }

    /// A frame's pixels at one size, and its hotspot there.
    fn pixels(&mut self, frame: usize, size: usize) -> Option<Pixels> {
        if self.paths[frame].0.cell.is_some() && !self.pixels.contains_key(&(frame, size)) {
            self.cut_sheet(frame, size);
        }
        let first = self.dims(frame).first().copied().unwrap_or((0, 0));
        let folder = &self.folder;
        let paths = &self.paths;
        self.pixels
            .entry((frame, size))
            .or_insert_with(|| {
                let (shown, hotspot) = &paths[frame];
                let sizes = &shown.files;
                let decode = |path: &PathBuf| {
                    std::fs::read(folder.join(path))
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| cursors::decode(&bytes))
                };
                let picture = match decode(&sizes[size]) {
                    Ok(picture) => picture,
                    Err(error) => {
                        tracing::warn!(file = %sizes[size].display(), %error, "a cursor frame didn't load");
                        return None;
                    }
                };
                let first = if size == 0 || first.0 == 0 {
                    (picture.width, picture.height)
                } else {
                    first
                };
                let (x, y) = cursors::scaled_hotspot(*hotspot, first, (picture.width, picture.height));
                let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &picture.rgba,
                    picture.width,
                    picture.height,
                );
                Some((buffer, (x as i32, y as i32)))
            })
            .clone()
    }

    /// Decodes the sheet `frame` is cut from, at one size, and cuts every
    /// frame of it there at once, so a sheet is read once a size; the sheet
    /// itself isn't kept.
    fn cut_sheet(&mut self, frame: usize, size: usize) {
        let path = self.paths[frame].0.files[size].clone();
        let first_width = self.paths[frame]
            .0
            .files
            .first()
            .and_then(|first| image::image_dimensions(self.folder.join(first)).ok())
            .map_or(1, |(width, _)| width.max(1));
        let sheet = std::fs::read(self.folder.join(&path))
            .map_err(|error| error.to_string())
            .and_then(|bytes| cursors::decode_sheet(&bytes));
        let sheet = match sheet {
            Ok(sheet) => sheet,
            Err(error) => {
                tracing::warn!(file = %path.display(), %error, "a cursor sheet didn't load");
                self.pixels.insert((frame, size), None);
                return;
            }
        };
        let scale = (sheet.width / first_width).max(1);
        for (index, (shown, hotspot)) in self.paths.iter().enumerate() {
            let Some(cell) = shown.cell else { continue };
            if shown.files.get(size) != Some(&path) || self.pixels.contains_key(&(index, size)) {
                continue;
            }
            let pixels = cursors::cut(&sheet, cell, scale).map(|picture| {
                let (x, y) = cursors::scaled_hotspot(
                    *hotspot,
                    (cell.width, cell.height),
                    (picture.width, picture.height),
                );
                let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &picture.rgba,
                    picture.width,
                    picture.height,
                );
                (buffer, (x as i32, y as i32))
            });
            self.pixels.insert((index, size), pixels);
        }
    }

    /// A frame as Slint's custom cursor, for the core's hook.
    fn image(&mut self, frame: usize, scale: f32) -> Option<(Image, i32, i32)> {
        let size = self.size_at(frame, scale);
        let (buffer, (x, y)) = self.pixels(frame, size)?;
        Some((Image::from_rgba8(buffer), x, y))
    }

    /// A frame as winit's cursor, made once.
    fn cursor(
        &mut self,
        event_loop: &ActiveEventLoop,
        frame: usize,
        scale: f32,
    ) -> Option<CustomCursor> {
        let size = self.size_at(frame, scale);
        if let Some(cursor) = self.cursors.get(&(frame, size)) {
            return Some(cursor.clone());
        }
        let (buffer, (x, y)) = self.pixels(frame, size)?;
        let source = CustomCursor::from_rgba(
            buffer.as_bytes(),
            u16::try_from(buffer.width()).ok()?,
            u16::try_from(buffer.height()).ok()?,
            u16::try_from(x).ok()?,
            u16::try_from(y).ok()?,
        )
        .ok()?;
        let cursor = event_loop.create_custom_cursor(source);
        self.cursors.insert((frame, size), cursor.clone());
        Some(cursor)
    }
}

/// How the pointer's moving: still, slow or fast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Band {
    Slow = 0,
    Fast = 1,
}

/// One kind's motion, compiled.
struct MotionSeqs {
    moving: Vec<[usize; 2]>,
    settle: Vec<Seq>,
    slow_start: usize,
    idle: Option<Seq>,
    idle_after: Duration,
}

/// One sprite of the trail.
struct Sprite {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    born: Instant,
    sprite: usize,
}

struct Trail {
    sheet: Option<(Image, f32)>,
    cell: u32,
    sprites: Vec<(u32, u32)>,
    life: Duration,
    every: f32,
    per_step: u32,
    jitter: f32,
    count: usize,
    press: u32,
    opacity: f32,
    alive: Vec<Sprite>,
    /// Motion since the last spawn.
    travelled: f32,
    model: Rc<VecModel<TrailSprite>>,
    last_step: Option<Instant>,
    corrupt: Option<cursors::Corrupt>,
    /// The next glitch, and the one playing: until when, and its seed.
    next_glitch: Option<Instant>,
    glitch: Option<(Instant, u64)>,
}

/// The animated cursors of the theme showing: what plays now, and the
/// pointer's motion.
struct Engine {
    frames: Frames,
    kinds: HashMap<Kind, Seq>,
    still: HashMap<Kind, usize>,
    transitions: HashMap<(Kind, Kind), Seq>,
    press: HashMap<Kind, Seq>,
    release: HashMap<Kind, Seq>,
    motion: HashMap<Kind, MotionSeqs>,
    slow: f32,
    fast: f32,
    trail: Option<Trail>,
    // What plays.
    kind: Kind,
    kind_since: Instant,
    transition: Option<(Kind, Kind, Instant)>,
    pressed: Option<(bool, Kind, Instant)>,
    // The pointer.
    scale: f32,
    at: Option<(f32, f32, Instant)>,
    velocity: (f32, f32),
    band: Option<Band>,
    direction: usize,
    last_move: Option<Instant>,
    settle: Option<(usize, Duration, Instant)>,
    inside: bool,
    // What winit was given: the frame and its size, or the system's.
    shown: Option<Option<(usize, usize)>>,
    /// Slint gave the window a cursor of its own, which shows at the next
    /// window event: give ours again after it.
    slint_pending: bool,
    force: bool,
    seed: u64,
}

impl Engine {
    fn new(resolved: &Resolved, animated: &Animated, folder: &Path) -> Self {
        let mut frames = Frames {
            folder: folder.to_owned(),
            paths: Vec::new(),
            dims: HashMap::new(),
            pixels: HashMap::new(),
            cursors: HashMap::new(),
        };
        fn seqs(
            map: &BTreeMap<Kind, cursors::Sequence>,
            frames: &mut Frames,
        ) -> HashMap<Kind, Seq> {
            map.iter()
                .map(|(kind, sequence)| (*kind, Seq::of(sequence, frames)))
                .collect()
        }
        let kinds = seqs(&animated.kinds, &mut frames);
        let press = seqs(&animated.press, &mut frames);
        let release = seqs(&animated.release, &mut frames);
        let transitions = animated
            .transitions
            .iter()
            .map(|(change, sequence)| (*change, Seq::of(sequence, &mut frames)))
            .collect();
        // Each kind's still picture, so a frame can give way to it.
        let still = resolved
            .cursors
            .iter()
            .map(|(kind, cursor)| {
                let still = cursors::Frame {
                    files: cursor.images.clone(),
                    cell: cursor.cell,
                };
                (*kind, frames.add(&still, cursor.hotspot))
            })
            .collect();
        let motion = animated
            .motion
            .iter()
            .map(|(kind, motion)| {
                let moving = motion
                    .moving
                    .iter()
                    .map(|[slow, fast]| {
                        [
                            frames.add(slow, motion.hotspot),
                            frames.add(fast, motion.hotspot),
                        ]
                    })
                    .collect();
                let compiled = MotionSeqs {
                    moving,
                    settle: motion
                        .settle
                        .iter()
                        .map(|seq| Seq::of(seq, &mut frames))
                        .collect(),
                    slow_start: motion.slow_start,
                    idle: motion.idle.as_ref().map(|seq| Seq::of(seq, &mut frames)),
                    idle_after: Duration::from_millis(u64::from(motion.idle_after)),
                };
                (*kind, compiled)
            })
            .collect();
        let trail = animated.trail.as_ref().map(|trail| {
            // The sheet at each size: the window picks by its scale when it
            // shows the first sprite.
            Trail {
                sheet: None,
                cell: trail.cell,
                sprites: trail.sprites.clone(),
                life: Duration::from_millis(trail.life as u64),
                every: trail.every,
                per_step: trail.per_step,
                jitter: trail.jitter,
                count: trail.count as usize,
                press: trail.press,
                opacity: trail.opacity,
                alive: Vec::new(),
                travelled: 0.0,
                model: Rc::new(VecModel::default()),
                last_step: None,
                corrupt: trail.corrupt.clone(),
                next_glitch: None,
                glitch: None,
            }
        });
        let sheet_paths = animated.trail.as_ref().map(|trail| trail.sheet.clone());
        let mut engine = Self {
            frames,
            kinds,
            still,
            transitions,
            press,
            release,
            motion,
            slow: animated.slow.max(1.0),
            fast: animated.fast.max(animated.slow + 1.0),
            trail,
            kind: Kind::Default,
            kind_since: Instant::now(),
            transition: None,
            pressed: None,
            scale: 1.0,
            at: None,
            velocity: (0.0, 0.0),
            band: None,
            direction: 0,
            last_move: None,
            settle: None,
            inside: false,
            shown: None,
            slint_pending: false,
            force: false,
            seed: 0x2545_f491_4f6c_dd1d,
        };
        if let (Some(trail), Some(paths)) = (engine.trail.as_mut(), sheet_paths) {
            trail.sheet = load_sheet(folder, &paths);
        }
        engine
    }

    fn random(&mut self) -> f32 {
        // xorshift64*: enough for where a sprite lands.
        self.seed ^= self.seed >> 12;
        self.seed ^= self.seed << 25;
        self.seed ^= self.seed >> 27;
        (self.seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// The core's hook asks for `kind`'s picture: a kind change starts its
    /// transition. The frame that shows now, if an animated one does.
    fn hook(&mut self, cursor: BuiltInMouseCursor, scale: f32) -> Option<(Image, i32, i32)> {
        let kind = Kind::ALL
            .into_iter()
            .find(|kind| builtin(*kind) == cursor)?;
        self.scale = scale;
        let now = Instant::now();
        self.set_kind(kind, now);
        self.slint_pending = true;
        let (frame, _) = self.frame(now);
        self.frames.image(frame?, scale)
    }

    fn set_kind(&mut self, kind: Kind, now: Instant) {
        if kind == self.kind {
            return;
        }
        if self.transitions.contains_key(&(self.kind, kind)) {
            self.transition = Some((self.kind, kind, now));
        } else {
            self.transition = None;
        }
        self.pressed = None;
        self.kind = kind;
        self.kind_since = now;
    }

    fn moved(&mut self, x: f32, y: f32, now: Instant) {
        self.inside = true;
        if let Some((before_x, before_y, then)) = self.at {
            let seconds = now.duration_since(then).as_secs_f32();
            if seconds > 0.0005 {
                let (dx, dy) = (x - before_x, y - before_y);
                // An exponential average over about 40 ms.
                let amount = 1.0 - (-seconds / 0.040).exp();
                let gap_reset = seconds > 0.2;
                let (vx, vy) = (dx / seconds, dy / seconds);
                self.velocity = if gap_reset {
                    (vx, vy)
                } else {
                    (
                        self.velocity.0 + amount * (vx - self.velocity.0),
                        self.velocity.1 + amount * (vy - self.velocity.1),
                    )
                };
                let speed = self.velocity.0.hypot(self.velocity.1);
                self.band = if speed > self.fast
                    || (self.band == Some(Band::Fast) && speed > self.fast - 60.0)
                {
                    Some(Band::Fast)
                } else if speed > self.slow
                    || (self.band.is_some() && speed > self.slow * 2.0 / 3.0)
                {
                    Some(Band::Slow)
                } else {
                    self.band
                };
                if speed > self.slow * 2.0 / 3.0 {
                    self.direction = cursors::direction(self.velocity.0, self.velocity.1);
                    self.last_move = Some(now);
                    self.settle = None;
                }
                if let Some(trail) = &mut self.trail {
                    trail.travelled += dx.hypot(dy);
                }
                self.spawn_along(before_x, before_y, x, y, now);
            }
        }
        self.at = Some((x, y, now));
    }

    fn spawn_along(&mut self, from_x: f32, from_y: f32, to_x: f32, to_y: f32, now: Instant) {
        if !TRAIL_ON.get() {
            return;
        }
        let Some(trail) = &self.trail else { return };
        let count = (trail.travelled / trail.every).floor() as usize;
        // Spread along the motion since the last event.
        let spawns: Vec<(f32, f32)> = (0..count.min(8))
            .map(|step| {
                let along = (step + 1) as f32 / count.min(8) as f32;
                (
                    from_x + (to_x - from_x) * along,
                    from_y + (to_y - from_y) * along,
                )
            })
            .collect();
        if let Some(trail) = &mut self.trail {
            trail.travelled -= count as f32 * trail.every;
        }
        for (x, y) in spawns {
            let per_step = self.trail.as_ref().map_or(0, |trail| trail.per_step);
            for _ in 0..per_step {
                self.spawn(x, y, 0.0, now);
            }
        }
    }

    fn spawn(&mut self, x: f32, y: f32, kick: f32, now: Instant) {
        let (jx, jy, vx, vy, pick) = (
            self.random() - 0.5,
            self.random() - 0.5,
            self.random() - 0.5,
            self.random(),
            self.random(),
        );
        let Some(trail) = &mut self.trail else { return };
        if trail.alive.len() >= trail.count {
            trail.alive.remove(0);
        }
        trail.alive.push(Sprite {
            x: x + jx * trail.jitter,
            y: y + jy * trail.jitter,
            // Pixels a second: a drift, and a kick upward on a click.
            vx: vx * (0.4 + kick) * 60.0,
            vy: -vy * (0.3 + kick) * 60.0,
            born: now,
            sprite: ((pick * trail.sprites.len() as f32) as usize).min(trail.sprites.len() - 1),
        });
    }

    fn button(&mut self, down: bool, now: Instant) {
        let kind = self.kind;
        let table = if down { &self.press } else { &self.release };
        self.pressed = table.contains_key(&kind).then_some((down, kind, now));
        if down
            && TRAIL_ON.get()
            && let Some((x, y, _)) = self.at
        {
            let count = self.trail.as_ref().map_or(0, |trail| trail.press);
            for _ in 0..count {
                self.spawn(x, y, 1.2, now);
            }
        }
    }

    fn left(&mut self) {
        self.inside = false;
        self.at = None;
        self.band = None;
        self.settle = None;
        self.last_move = None;
    }

    /// The animated frame that shows now, if one does, and when the next
    /// change is due. In order: the kind's own frames, a press, a
    /// transition, motion, its settle, the idle loop.
    fn frame(&mut self, now: Instant) -> (Option<usize>, Option<Instant>) {
        let soonest = |left: Duration| Some(now + left);
        if let Some(seq) = self.kinds.get(&self.kind)
            && let Some((frame, left)) = seq.at(now.duration_since(self.kind_since))
        {
            return (Some(frame), soonest(left));
        }
        if let Some((down, kind, since)) = self.pressed {
            let table = if down { &self.press } else { &self.release };
            match table
                .get(&kind)
                .and_then(|seq| seq.at(now.duration_since(since)))
            {
                Some((frame, left)) => return (Some(frame), soonest(left)),
                None => self.pressed = None,
            }
        }
        if let Some((from, to, since)) = self.transition {
            match self
                .transitions
                .get(&(from, to))
                .and_then(|seq| seq.at(now.duration_since(since)))
            {
                Some((frame, left)) => return (Some(frame), soonest(left)),
                None => self.transition = None,
            }
        }
        let Some(motion) = self.motion.get(&self.kind) else {
            return (None, None);
        };
        // Moving: a frame by direction and speed, until 60 ms pass without
        // a move.
        const STOPPED: Duration = Duration::from_millis(60);
        if let (Some(band), Some(last)) = (self.band, self.last_move) {
            if now.duration_since(last) < STOPPED && self.inside {
                let frame = motion.moving[self.direction][band as usize];
                return (Some(frame), Some(last + STOPPED));
            }
            // It stopped: the settle that way, from its start after a fast
            // move or further in after a slow one.
            self.band = None;
            self.velocity = (0.0, 0.0);
            // One settle a direction, or one a direction and speed.
            let index = if motion.settle.len() == 16 {
                self.direction * 2 + band as usize
            } else {
                self.direction
            };
            if let Some(seq) = motion.settle.get(index) {
                let start = if band == Band::Slow {
                    seq.start_of(motion.slow_start)
                } else {
                    Duration::ZERO
                };
                self.settle = Some((index, start, last + STOPPED));
            }
        }
        if let Some((direction, start, since)) = self.settle {
            match motion
                .settle
                .get(direction)
                .and_then(|seq| seq.at(start + now.duration_since(since)))
            {
                Some((frame, left)) => return (Some(frame), soonest(left)),
                None => self.settle = None,
            }
        }
        // Still: the idle loop, once it's been still long enough.
        if self.inside
            && let Some(idle) = &motion.idle
        {
            let begins = self.last_move.unwrap_or(self.kind_since) + motion.idle_after;
            if now >= begins {
                if let Some((frame, left)) = idle.at(now.duration_since(begins)) {
                    return (Some(frame), soonest(left));
                }
            } else {
                return (None, Some(begins));
            }
        }
        (None, None)
    }

    /// After a turn of the event loop: gives winit the frame that shows,
    /// moves the trail, and wakes the loop when the next is due.
    fn step(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        // Held still while nobody sees the window, or the theme asks for it
        // without the focus: each kind's still picture, and no trail.
        let still = super::theme_pace::pace() == super::theme_pace::Pace::Still;
        let (frame, mut due) = if still { (None, None) } else { self.frame(now) };
        let scale = self.scale;
        // The still picture shows once no frame plays.
        let wanted = frame.or_else(|| self.still.get(&self.kind).copied());
        let sized = wanted.map(|frame| (frame, self.frames.size_at(frame, scale)));
        // Once a frame has been given, every change is ours to give; before,
        // Slint's still picture stands until a frame plays.
        let changed = self.shown != Some(sized) && (self.shown.is_some() || frame.is_some());
        if self.force || changed {
            self.force = false;
            let cursor = wanted.and_then(|frame| self.frames.cursor(event_loop, frame, scale));
            let kind = self.kind;
            with_winit(|window| match &cursor {
                Some(cursor) => window.set_cursor(cursor.clone()),
                None => window.set_cursor(icon(kind)),
            });
            self.shown = Some(sized);
        }
        if still {
            self.clear_trail();
        } else if let Some(next) = self.step_trail(now) {
            due = Some(due.map_or(next, |due| due.min(next)));
        }
        // Slower without the focus, as the theme asks.
        let wait = due.and_then(|due| {
            super::theme_pace::stretch(
                due.saturating_duration_since(now)
                    .max(Duration::from_millis(16)),
            )
        });
        match wait {
            Some(wait) => {
                WAKE.with(|timer| timer.start(slint::TimerMode::SingleShot, wait, || {}));
            }
            None => WAKE.with(slint::Timer::stop),
        }
    }

    /// Moves the trail's sprites; when the next step is due while any live.
    fn step_trail(&mut self, now: Instant) -> Option<Instant> {
        let flickers: Vec<f32> = (0..self.trail.as_ref().map_or(0, |trail| trail.alive.len()))
            .map(|_| self.random())
            .collect();
        let rolls = (self.random(), self.random(), self.seed);
        let trail = self.trail.as_mut()?;
        // At most one step a frame of its rate, however often the pointer
        // reports a move; slower without the focus.
        let rate = super::theme_pace::stretch(Duration::from_millis(TRAIL_MS.get()))
            .unwrap_or(Duration::from_millis(TRAIL_MS.get()));
        if let Some(then) = trail.last_step
            && now.duration_since(then) < rate
        {
            return Some(then + rate);
        }
        // The glitch: due at a random time while the trail is alive, then
        // for a moment. Nothing runs for it once the trail is gone.
        let glitching = match (&trail.corrupt, trail.alive.is_empty()) {
            (Some(corrupt), false) => {
                let between = |(least, most): (f32, f32), roll: f32| {
                    Duration::from_millis((least + (most - least) * roll) as u64)
                };
                let next = *trail
                    .next_glitch
                    .get_or_insert_with(|| now + between(corrupt.every, rolls.0));
                if now >= next {
                    trail.glitch = Some((now + between(corrupt.lasts, rolls.1), rolls.2));
                    trail.next_glitch = Some(now + between(corrupt.every, rolls.0));
                }
                trail
                    .glitch
                    .filter(|(until, _)| now < *until)
                    .map(|(_, seed)| seed)
            }
            _ => None,
        };
        if trail.sheet.is_none() {
            trail.alive.clear();
        }
        let seconds = trail
            .last_step
            .map_or(0.0, |then| now.duration_since(then).as_secs_f32().min(0.1));
        trail.last_step = (!trail.alive.is_empty()).then_some(now);
        let life = trail.life.as_secs_f32();
        trail
            .alive
            .retain(|sprite| now.duration_since(sprite.born).as_secs_f32() < life);
        let sprites = trail.sprites.len();
        let mut rows = Vec::with_capacity(trail.alive.len());
        let (sheet_scale, cell) = (
            trail.sheet.as_ref().map_or(1.0, |(_, scale)| *scale),
            trail.cell,
        );
        for (index, sprite) in trail.alive.iter_mut().enumerate() {
            let age = now.duration_since(sprite.born).as_secs_f32() / life;
            sprite.x += sprite.vx * seconds;
            sprite.y += sprite.vy * seconds;
            // 0.07 px a frame², at 60 frames a second.
            sprite.vy += 0.07 * 3600.0 * seconds;
            if age < 0.7
                && flickers
                    .get(index)
                    .is_some_and(|roll| *roll < 0.45 * seconds * 60.0)
            {
                sprite.sprite = (sprite.sprite + 1 + index) % sprites;
            }
            let left = 1.0 - age;
            let (mut cx, mut cy) = trail.sprites[sprite.sprite];
            let mut x = sprite.x;
            let mut opacity = trail.opacity * left.powf(1.5);
            if let (Some(seed), Some(corrupt)) = (glitching, &trail.corrupt) {
                // Each band of the trail tears sideways its own way; the
                // sprites take the glitch's glyphs and light up.
                let band = (sprite.y / corrupt.band).floor() as i64 as u64;
                let mix = (seed ^ band.wrapping_mul(0x9e37_79b9_7f4a_7c15))
                    .wrapping_mul(0x2545_f491_4f6c_dd1d);
                let tear = ((mix >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0;
                x += tear * corrupt.tear;
                if !corrupt.sprites.is_empty() {
                    (cx, cy) = corrupt.sprites[(sprite.sprite + index) % corrupt.sprites.len()];
                }
                opacity = (opacity * corrupt.boost).min(1.0);
            }
            rows.push(TrailSprite {
                x,
                y: sprite.y,
                size: cell as f32 * (6.0 + 12.0 * left.powf(1.3)) / 16.0,
                opacity,
                cx: (cx as f32 * sheet_scale) as i32,
                cy: (cy as f32 * sheet_scale) as i32,
                cw: (cell as f32 * sheet_scale) as i32,
            });
        }
        // Row by row, so the layer's sprites are kept, not made again.
        let model = &trail.model;
        let had = model.row_count();
        for (index, row) in rows.iter().enumerate() {
            if index < had {
                model.set_row_data(index, row.clone());
            } else {
                model.push(row.clone());
            }
        }
        for index in (rows.len()..had).rev() {
            model.remove(index);
        }
        if had == 0 && !rows.is_empty() {
            show_trail(Some((
                &trail.model,
                trail.sheet.as_ref().map(|(image, _)| image),
            )));
        } else if had > 0 && rows.is_empty() {
            show_trail(None);
        }
        (!rows.is_empty()).then(|| now + rate)
    }

    /// Drops the trail's sprites and takes its layer away.
    fn clear_trail(&mut self) {
        if let Some(trail) = &mut self.trail {
            trail.alive.clear();
            trail.last_step = None;
            if trail.model.row_count() > 0 {
                trail.model.set_vec(Vec::new());
                show_trail(None);
            }
        }
    }

    fn stop(&mut self) {
        self.left();
        self.transition = None;
        self.pressed = None;
        self.shown = None;
        self.clear_trail();
        WAKE.with(slint::Timer::stop);
    }
}

/// A trail's sheet, the size for a scale of 2 when there is one, and how
/// many of its pixels a pixel of the first size takes.
fn load_sheet(folder: &Path, sizes: &[PathBuf]) -> Option<(Image, f32)> {
    let read = |path: &PathBuf| {
        std::fs::read(folder.join(path))
            .map_err(|error| error.to_string())
            .and_then(|bytes| cursors::decode_sheet(&bytes))
    };
    let first = read(sizes.first()?).ok()?;
    let shown = match sizes.get(1).map(read) {
        Some(Ok(larger)) => larger,
        _ => first,
    };
    let first_width = read(&sizes[0]).map_or(shown.width, |first| first.width);
    let scale = shown.width as f32 / first_width.max(1) as f32;
    let buffer =
        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&shown.rgba, shown.width, shown.height);
    Some((Image::from_rgba8(buffer), scale))
}

/// Puts the trail's sprites on the window, or takes them off.
fn show_trail(trail: Option<(&Rc<VecModel<TrailSprite>>, Option<&Image>)>) {
    let Some(window) = WINDOW.with_borrow(|weak| weak.as_ref().and_then(slint::Weak::upgrade))
    else {
        return;
    };
    let global = window.global::<CursorTrail>();
    match trail {
        Some((model, sheet)) => {
            if let Some(sheet) = sheet {
                global.set_sheet(sheet.clone());
            }
            global.set_sprites(slint::ModelRc::from(model.clone()));
        }
        None => global.set_sprites(slint::ModelRc::default()),
    }
}

/// Acts on the window's winit window.
fn with_winit(act: impl FnOnce(&slint::winit_030::winit::window::Window)) {
    if let Some(window) = WINDOW.with_borrow(|weak| weak.as_ref().and_then(slint::Weak::upgrade)) {
        window.window().with_winit_window(act);
    }
}

/// The system's cursor of a kind, for a kind the theme leaves to it.
fn icon(kind: Kind) -> CursorIcon {
    match kind {
        Kind::Default => CursorIcon::Default,
        Kind::Pointer => CursorIcon::Pointer,
        Kind::Text => CursorIcon::Text,
        Kind::Grab => CursorIcon::Grab,
        Kind::Grabbing => CursorIcon::Grabbing,
        Kind::NotAllowed => CursorIcon::NotAllowed,
        Kind::NoDrop => CursorIcon::NoDrop,
        Kind::Wait => CursorIcon::Wait,
        Kind::Progress => CursorIcon::Progress,
        Kind::Help => CursorIcon::Help,
        Kind::Crosshair => CursorIcon::Crosshair,
        Kind::Move => CursorIcon::Move,
        Kind::Copy => CursorIcon::Copy,
        Kind::Alias => CursorIcon::Alias,
        Kind::ColResize => CursorIcon::ColResize,
        Kind::RowResize => CursorIcon::RowResize,
        Kind::EwResize => CursorIcon::EwResize,
        Kind::NsResize => CursorIcon::NsResize,
        Kind::NeswResize => CursorIcon::NeswResize,
        Kind::NwseResize => CursorIcon::NwseResize,
        Kind::NResize => CursorIcon::NResize,
        Kind::EResize => CursorIcon::EResize,
        Kind::SResize => CursorIcon::SResize,
        Kind::WResize => CursorIcon::WResize,
        Kind::NeResize => CursorIcon::NeResize,
        Kind::NwResize => CursorIcon::NwResize,
        Kind::SeResize => CursorIcon::SeResize,
        Kind::SwResize => CursorIcon::SwResize,
    }
}

/// Sees the window's pointer events before Slint does, for the animated
/// cursors, and gives winit their frames after each turn of the loop. It
/// does nothing while no theme's cursors animate.
pub struct Handler;

impl CustomApplicationHandler for Handler {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: slint::winit_030::winit::window::WindowId,
        winit_window: Option<&slint::winit_030::winit::window::Window>,
        _slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        ENGINE.with(|engine| {
            let Ok(mut engine) = engine.try_borrow_mut() else {
                return;
            };
            let Some(engine) = engine.as_mut() else {
                return;
            };
            let now = Instant::now();
            if let Some(window) = winit_window {
                engine.scale = window.scale_factor() as f32;
            }
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let scale = f64::from(engine.scale.max(0.1));
                    engine.moved(
                        (position.x / scale) as f32,
                        (position.y / scale) as f32,
                        now,
                    );
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    engine.button(*state == ElementState::Pressed, now);
                }
                WindowEvent::CursorLeft { .. } => engine.left(),
                _ => {}
            }
            // Slint shows the cursor it was given while it handles this
            // event; ours goes back on after.
            if engine.slint_pending {
                engine.slint_pending = false;
                engine.force = true;
            }
        });
        EventResult::Propagate
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> EventResult {
        ENGINE.with(|engine| {
            if let Ok(mut engine) = engine.try_borrow_mut()
                && let Some(engine) = engine.as_mut()
            {
                engine.step(event_loop);
            }
        });
        EventResult::Propagate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_its_slint_cursor() {
        let mut seen = Vec::new();
        for kind in Kind::ALL {
            let cursor = builtin(kind);
            assert!(!seen.contains(&cursor), "{kind:?}");
            seen.push(cursor);
        }
    }
}
