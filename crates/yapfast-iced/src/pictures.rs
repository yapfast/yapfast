//! Pictures in the UI's own buffers, and the pictures the client makes.
//!
//! The core decodes every picture into the format of the renderer chosen as
//! Yapfast starts (`renderer::choose`): on the CPU tiny-skia's premultiplied
//! BGRA ([`install`]), which the patched
//! renderer copies onto the frame as it is (patch 1 in
//! `vendor/iced_tiny_skia`), on the GPU the straight RGBA iced_wgpu
//! uploads. So the one buffer the core keeps is the one the renderer reads,
//! and an image `Handle` wraps it without a copy ([`shared`]). A picture
//! drawn at another size than it came at is fitted once per size and let go
//! of with the core's picture ([`Converted`]). Moving pictures play through
//! [`Player`], a frame decoded only as it falls due, on the window's one
//! redraw clock (`animated::Animated`).

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use iced::{
    Color, Element, Point, Rectangle, Size,
    advanced::{graphics::text::font_system, image::Handle},
    widget::{container, stack, text},
};
use yapfast_core::chat::{Picture, Presence};

use crate::{
    animated::{Animated, Players},
    app::{App, Message},
    looks,
    paint::{self, fitted},
    style::{faded, hex, rounded},
    widgets::Pic,
};

/// A picture's pixels as tiny-skia draws them, premultiplied BGRA.
struct NativePixels(Vec<u8>);

impl yapfast_core::chat::PixelBuffer for NativePixels {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Has the core decode pictures into the renderer's format, once the
/// renderer is chosen. The GPU keeps the core's own straight RGBA, which is
/// what iced_wgpu uploads.
pub fn install() {
    if crate::renderer::gpu() {
        return;
    }
    yapfast_core::chat::set_pixel_buffers(|width, height, rgba| {
        let mut pixels = rgba.to_vec();
        debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
        paint::to_native(&mut pixels);
        Box::new(NativePixels(pixels))
    });
}

/// The order of a picture's bytes: tiny-skia's where [`install`] made its
/// buffer, else the core's own RGBA (the GPU's, and the tests').
pub fn byte_order(picture: &Picture) -> yapfast_core::dominant_colour::Order {
    if picture.rgba.buffer::<NativePixels>().is_some() {
        return yapfast_core::dominant_colour::Order::Bgra;
    }
    yapfast_core::dominant_colour::Order::Rgba
}

/// A picture's pixels as a byte slice that keeps the picture alive.
struct PictureBytes(Arc<Picture>);

impl AsRef<[u8]> for PictureBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0.rgba
    }
}

/// A handle on a picture's own pixels, shared with the core.
pub fn shared(picture: &Arc<Picture>) -> Handle {
    Handle::from_rgba(
        picture.width,
        picture.height,
        bytes::Bytes::from_owner(PictureBytes(picture.clone())),
    )
}

pub use crate::canvas::handle;

/// Images the client holds for the core's pictures, by the picture they
/// show: one each, let go of once the core lets go of the picture, so the
/// core's picture budget stays the budget.
pub struct Converted<T> {
    held: HashMap<usize, (Weak<Picture>, T)>,
}

impl<T> Converted<T> {
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

impl<T> Default for Converted<T> {
    fn default() -> Self {
        Self {
            held: HashMap::new(),
        }
    }
}

impl<T: Clone> Converted<T> {
    /// The image of `picture`, made with `convert` the first time.
    pub fn get(&mut self, picture: &Arc<Picture>, convert: impl FnOnce(&Arc<Picture>) -> T) -> T {
        let key = Arc::as_ptr(picture) as usize;
        if let Some((_, image)) = self.held.get(&key) {
            return image.clone();
        }
        let image = convert(picture);
        self.held
            .insert(key, (Arc::downgrade(picture), image.clone()));
        image
    }

    /// Drops images whose picture nobody holds but `self_owners` images:
    /// an image over the picture's own bytes ([`shared`]) is one owner.
    pub fn prune(&mut self, self_owners: usize) {
        self.held
            .retain(|_, (picture, _)| picture.strong_count() > self_owners);
    }
}

/// Every picture the window keeps to draw, besides the core's own: the
/// images over the core's pixels, the copies made at other sizes, the
/// marks drawn once, and the moving pictures playing. One place lets go of
/// them all ([`Pictures::forget`]), and one guard keeps a view's
/// let-go of what it no longer shows ([`Pictures::view`]).
pub struct Pictures {
    /// Previews, and avatars that come at the size they're drawn at,
    /// sharing the core's pixels.
    pub previews: RefCell<Converted<Handle>>,
    /// Copies of other pictures, at each size they're drawn at (width,
    /// height, cut to cover).
    pub scaled: RefCell<HashMap<(u32, u32, bool), Converted<Handle>>>,
    /// Presence dots, by what they are and their ring's colour.
    pub marks: RefCell<HashMap<Mark, Handle>>,
    /// Emoji for the menu's quick reactions and the reactions' tooltips.
    pub emoji: RefCell<HashMap<(&'static str, u32), Option<Handle>>>,
    /// Moving pictures playing: those the last view didn't show go after
    /// it.
    pub players: Players,
    /// Copies of pictures at the sizes messages show them at.
    pub fitter: RefCell<Fitter>,
    /// Whether each picture shown in a box is opaque all over, looked at
    /// once: one that is hides the box's ground, which isn't drawn then.
    opaque: RefCell<Converted<bool>>,
}

impl Pictures {
    pub fn new(fitter: Fitter) -> Self {
        Self {
            previews: Default::default(),
            scaled: Default::default(),
            marks: Default::default(),
            emoji: Default::default(),
            players: Players::default(),
            fitter: RefCell::new(fitter),
            opaque: Default::default(),
        }
    }

    /// Whether `picture` is opaque all over.
    pub fn opaque(&self, picture: &Arc<Picture>) -> bool {
        self.opaque.borrow_mut().get(picture, |picture| {
            picture.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255)
        })
    }

    /// A view is being built until the guard drops: what it shows is noted
    /// afresh, and then the moving pictures' decoders and the copies it
    /// didn't show go, whatever the view returned.
    pub fn view(&self) -> Viewing<'_> {
        self.players.begin();
        self.fitter.borrow_mut().begin();
        Viewing(self)
    }

    /// `picture` playing in the view being built, drawn at `at` within a
    /// layout of `size`.
    pub fn animated(
        &self,
        picture: &Arc<Picture>,
        size: Size,
        at: Rectangle,
        cover: bool,
    ) -> Animated<'_> {
        self.players.animated(picture, size, at, cover)
    }

    /// After the core's state changed: images of pictures it let go of go
    /// too, and the sizes no picture is drawn at any more.
    pub fn prune(&self) {
        self.previews.borrow_mut().prune(1);
        self.opaque.borrow_mut().prune(0);
        self.scaled.borrow_mut().retain(|_, converted| {
            converted.prune(0);
            !converted.is_empty()
        });
    }

    /// The window went: every picture made for it goes.
    pub fn forget(&self) {
        *self.previews.borrow_mut() = Default::default();
        *self.opaque.borrow_mut() = Default::default();
        self.scaled.borrow_mut().clear();
        self.marks.borrow_mut().clear();
        self.emoji.borrow_mut().clear();
        self.players.clear();
        self.fitter.borrow_mut().forget();
    }

    /// Whether nothing is kept: no image, copy, mark or player.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.previews.borrow().is_empty()
            && self.opaque.borrow().is_empty()
            && self.scaled.borrow().is_empty()
            && self.marks.borrow().is_empty()
            && self.emoji.borrow().is_empty()
            && self.players.is_empty()
            && self.fitter.borrow().is_empty()
    }
}

/// A view being built ([`Pictures::view`]).
pub struct Viewing<'a>(&'a Pictures);

impl Drop for Viewing<'_> {
    fn drop(&mut self) {
        let pictures = self.0;
        pictures.fitter.borrow_mut().end();
        pictures.players.end();
    }
}

/// Plays a moving picture: frames decoded one at a time as they fall due,
/// from the core's picture, at its own delays, with nothing kept but the
/// frame showing.
pub struct Player {
    picture: Arc<Picture>,
    frames: Box<dyn Iterator<Item = Vec<u8>>>,
    index: usize,
    due: Instant,
    played: u32,
    /// A pass that gave no frame (its file gone, or one that doesn't
    /// decode): it plays no more, rather than asking for a frame at once,
    /// again, for ever.
    spent: bool,
}

impl Player {
    /// None for a still picture.
    pub fn new(picture: &Arc<Picture>) -> Option<Self> {
        picture.animation.as_ref()?;
        Some(Self {
            picture: picture.clone(),
            frames: picture.frames(),
            index: 0,
            due: Instant::now(),
            played: 0,
            spent: false,
        })
    }

    /// When the next frame is due; none once a picture that plays a set
    /// number of times has finished.
    pub fn due(&self) -> Option<Instant> {
        let animation = self.picture.animation.as_ref()?;
        (!self.spent && (animation.plays == 0 || self.played < animation.plays)).then_some(self.due)
    }

    /// The next frame's straight RGBA, if it's due by `now`.
    pub fn next(&mut self, now: Instant) -> Option<Vec<u8>> {
        let due = self.due()?;
        if now < due {
            return None;
        }
        let animation = self.picture.animation.as_ref()?;
        let frame = match self.frames.next() {
            Some(frame) => frame,
            None => {
                self.played += 1;
                if animation.plays != 0 && self.played >= animation.plays {
                    return None;
                }
                self.frames = self.picture.frames();
                self.index = 0;
                let Some(frame) = self.frames.next() else {
                    self.spent = true;
                    return None;
                };
                frame
            }
        };
        let delay = animation
            .delays
            .get(self.index)
            .copied()
            .unwrap_or(100)
            .max(20);
        self.index += 1;
        // From when it was due, so the pace holds; from now after a stall.
        self.due = (due + Duration::from_millis(u64::from(delay))).max(now);
        Some(frame)
    }
}

/// Small pictures the client draws for marks: presence dots, in the
/// theme's colours. Icons are [`crate::icons::Icons`]'.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    /// A presence dot on a ring of the ground colour.
    Dot(Presence, u32),
}

pub use crate::icons::Icon;

/// Whether moving pictures play, as Messages' Animate choice and the
/// window's focus say, and not while nobody is using Yapfast, whatever
/// Animate says (`crate::away`); while they don't, each shows its first
/// frame and nothing asks for frames.
pub fn plays(app: &App) -> bool {
    if app.away.inactive {
        return false;
    }
    match app.layout.prefs.animate {
        crate::layout::Animate::Focused => app.shell.focused,
        crate::layout::Animate::Always => true,
        crate::layout::Animate::Never => false,
    }
}

/// What [`App::face`] draws.
pub struct Face<'a> {
    pub url: Option<&'a str>,
    pub name: &'a str,
    /// What picks the tint without a picture, when not the name: a group
    /// DM's by its room id.
    pub key: Option<&'a str>,
    pub size: f32,
    pub opacity: f32,
    pub decoration: Option<&'a str>,
    /// Whether the decoration plays: its row or card is pointed at.
    pub play: bool,
    /// The presence dot, the ring's colour, and where the dot goes.
    pub dot: Option<(Presence, u32, Point)>,
}

impl<'a> Face<'a> {
    /// A face of `size` from its picture's address, or else `name`'s
    /// initial on its tint: fully drawn, with no decoration or dot.
    pub fn new(url: Option<&'a str>, name: &'a str, size: f32) -> Self {
        Self {
            url,
            name,
            key: None,
            size,
            opacity: 1.0,
            decoration: None,
            play: false,
            dot: None,
        }
    }

    pub fn key(self, key: Option<&'a str>) -> Self {
        Self { key, ..self }
    }

    pub fn opacity(self, opacity: f32) -> Self {
        Self { opacity, ..self }
    }

    pub fn decoration(self, decoration: Option<&'a str>, play: bool) -> Self {
        Self {
            decoration,
            play,
            ..self
        }
    }

    pub fn dot(self, dot: Option<(Presence, u32, Point)>) -> Self {
        Self { dot, ..self }
    }
}

/// A tint, as 0xRRGGBB, for an avatar without a picture: mid tones that
/// keep a white initial legible, the same key always the same one.
pub fn tint(key: &str) -> u32 {
    const TINTS: [u32; 8] = [
        0xc2554d, 0xb86e22, 0x3a9357, 0x2b8a98, 0x4a74c9, 0x7a5fc9, 0xb34f8c, 0x646b7a,
    ];
    let hash = key
        .to_lowercase()
        .bytes()
        .fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
    // FNV-1a mixes its high bits far better than its low ones.
    TINTS[(hash >> 20) as usize % TINTS.len()]
}

/// The first letter, in capitals, which an avatar without a picture shows.
pub fn initials(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".into(), |first| first.to_uppercase().collect())
}

/// A copy of a core picture at another size: the picture, the size, and
/// whether it's cut to cover the size (else stretched to it).
pub type FitKey = (usize, u32, u32, bool);

/// A copy the fitter made.
#[derive(Clone)]
pub struct Fitted {
    key: FitKey,
    picture: Weak<Picture>,
    handle: Handle,
}

/// Copies of the core's pictures at the sizes messages show them at (a
/// gallery's tiles, a preview narrower than it came, a spoiler's blur),
/// made on a thread of their own so the window never waits on one, and let
/// go of once no view shows them; until a copy comes its place shows its
/// ground. A copy made comes back as a [`Fitted`].
pub struct Fitter {
    jobs: std::sync::mpsc::Sender<(FitKey, Arc<Picture>)>,
    /// Copies made, by what they copy, with the picture so a copy of a
    /// picture the core let go of goes too.
    done: HashMap<FitKey, (Weak<Picture>, Handle)>,
    /// Asked for and not back yet.
    asked: std::collections::HashSet<FitKey>,
    /// What the view being built shows.
    shown: std::collections::HashSet<FitKey>,
}

impl Fitter {
    /// The fitter, and the stream of copies it makes for the app to take
    /// in.
    pub fn new() -> (
        Self,
        impl iced::futures::Stream<Item = Fitted> + Send + 'static,
    ) {
        let (jobs, queue) = std::sync::mpsc::channel::<(FitKey, Arc<Picture>)>();
        let (made, copies) = iced::futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("fitter".into())
            .spawn(move || {
                while let Ok((key, picture)) = queue.recv() {
                    let (_, width, height, cover) = key;
                    let pixels = fitted(
                        &picture.rgba,
                        (picture.width, picture.height),
                        (width, height),
                        cover,
                    );
                    let fitted = Fitted {
                        key,
                        picture: Arc::downgrade(&picture),
                        handle: Handle::from_rgba(width, height, pixels),
                    };
                    drop(picture);
                    if made.unbounded_send(fitted).is_err() {
                        break;
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!("no thread to fit pictures in: {error}");
        }
        let fitter = Self {
            jobs,
            done: HashMap::new(),
            asked: Default::default(),
            shown: Default::default(),
        };
        (fitter, copies)
    }

    /// `picture` at `width` by `height`, once its copy is made; asks for it
    /// the first time.
    pub fn get(
        &mut self,
        picture: &Arc<Picture>,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        if picture.width == 0 || picture.height == 0 || width == 0 || height == 0 {
            return None;
        }
        let key = (Arc::as_ptr(picture) as usize, width, height, cover);
        self.shown.insert(key);
        if let Some((_, handle)) = self.done.get(&key) {
            return Some(handle.clone());
        }
        if self.asked.insert(key) {
            self.jobs.send((key, picture.clone())).ok();
        }
        None
    }

    /// A copy came. One of a picture the core let go of meanwhile, or that
    /// the next view doesn't show, goes then.
    pub fn made(&mut self, fitted: Fitted) {
        self.asked.remove(&fitted.key);
        self.done
            .insert(fitted.key, (fitted.picture, fitted.handle));
    }

    /// The window went: every copy goes, and those asked for go as they
    /// come, at the next view's end.
    pub fn forget(&mut self) {
        self.done.clear();
        self.asked.clear();
        self.shown.clear();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    /// The copies of `picture` asked for, made here and now, as the thread
    /// would.
    #[cfg(test)]
    pub fn make_asked(&mut self, picture: &Arc<Picture>) {
        let at = Arc::as_ptr(picture) as usize;
        let asked: Vec<FitKey> = self
            .asked
            .iter()
            .copied()
            .filter(|key| key.0 == at)
            .collect();
        for key in asked {
            let (_, width, height, cover) = key;
            let pixels = fitted(
                &picture.rgba,
                (picture.width, picture.height),
                (width, height),
                cover,
            );
            self.made(Fitted {
                key,
                picture: Arc::downgrade(picture),
                handle: Handle::from_rgba(width, height, pixels),
            });
        }
    }

    /// A view begins: what it shows is noted afresh.
    pub fn begin(&mut self) {
        self.shown.clear();
    }

    /// A view was built: copies it didn't show, and those of pictures the
    /// core let go of, go.
    pub fn end(&mut self) {
        let shown = &self.shown;
        self.done
            .retain(|key, (picture, _)| shown.contains(key) && picture.strong_count() > 0);
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// The picture at `url` at `width` by `height`: the core's own pixels
    /// when it came at that size, else a copy made once (cut to cover the
    /// size, or stretched to it).
    pub fn picture(self, url: &str, width: u32, height: u32, cover: bool) -> Option<Handle> {
        let picture = self.state.pictures.get(url)?;
        self.fit(picture, width, height, cover)
    }

    pub fn fit(
        self,
        picture: &Arc<Picture>,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        if picture.width == 0 || picture.height == 0 || width == 0 || height == 0 {
            return None;
        }
        if picture.width == width && picture.height == height {
            return Some(self.pictures.previews.borrow_mut().get(picture, shared));
        }
        Some(
            self.pictures
                .scaled
                .borrow_mut()
                .entry((width, height, cover))
                .or_default()
                .get(picture, |picture| {
                    let pixels = fitted(
                        &picture.rgba,
                        (picture.width, picture.height),
                        (width, height),
                        cover,
                    );
                    Handle::from_rgba(width, height, pixels)
                }),
        )
    }

    /// An animated custom emoji's picture at `url`, `side` across, playing
    /// in `players` while pictures play, its still showing until the first
    /// frame; none for a still emoji or one whose picture hasn't come.
    pub fn playing_emoji(self, url: &str, side: u32, players: &'a Players) -> Option<Animated<'a>> {
        if !self.plays {
            return None;
        }
        let picture = self.state.pictures.get(url)?;
        picture.animation.as_ref()?;
        let square = Size::new(side as f32, side as f32);
        let still = self.fit(picture, side, side, false);
        Some(
            players
                .animated(picture, square, Rectangle::with_size(square), false)
                .still(still),
        )
    }

    /// A moving picture at `url`, once it has come and while pictures play
    /// ([`pictures::plays`]), as an `Animated` drawn
    /// at `at` within a layout of `size`; it shows nothing until its first
    /// frame unless given a still.
    pub fn moving(self, url: &str, size: Size, at: Rectangle, cover: bool) -> Option<Animated<'a>> {
        if !self.plays {
            return None;
        }
        let picture = self.state.pictures.get(url)?;
        picture.animation.as_ref()?;
        Some(self.pictures.animated(picture, size, at, cover))
    }

    /// A presence dot, drawn once.
    pub fn mark(self, mark: Mark) -> Option<Handle> {
        if let Some(handle) = self.pictures.marks.borrow().get(&mark) {
            return Some(handle.clone());
        }
        let canvas = match mark {
            // The theme's live green for online and faint for offline; the
            // marks go when the theme changes.
            Mark::Dot(presence, ring) => paint::status_dot(
                presence,
                ring,
                crate::theme::rgb(self.look.theme.live),
                crate::theme::rgb(self.look.theme.faint),
            )?,
        };
        let handle = handle(canvas);
        self.pictures
            .marks
            .borrow_mut()
            .insert(mark, handle.clone());
        Some(handle)
    }

    /// An emoji's picture at `size`, for the menu's quick reactions and
    /// tooltips: made once, kept with the client's marks.
    pub fn emoji_picture(self, key: &'static str, size: u32) -> Option<Handle> {
        if let Some(handle) = self.pictures.emoji.borrow().get(&(key, size)) {
            return handle.clone();
        }
        let canvas = {
            let mut system = font_system().write().expect("font system");
            paint::emoji(system.raw(), key, size)
        };
        let handle = canvas.map(handle);
        self.pictures
            .emoji
            .borrow_mut()
            .insert((key, size), handle.clone());
        handle
    }

    /// An avatar or icon: the picture once it has come, else initials on a
    /// tint; `opacity` fades both.
    pub fn avatar(
        self,
        url: Option<&str>,
        name: &str,
        size: f32,
        radius: f32,
        opacity: f32,
    ) -> Element<'a, Message> {
        self.avatar_tinted(url, name, name, size, radius, opacity)
    }

    /// [`App::avatar`], its tint picked by `key` rather than the name, as
    /// the inbox tints a room's by its id.
    pub fn avatar_tinted(
        self,
        url: Option<&str>,
        name: &str,
        key: &str,
        size: f32,
        radius: f32,
        opacity: f32,
    ) -> Element<'a, Message> {
        let pixels = size as u32;
        if let Some(handle) = url.and_then(|url| self.picture(url, pixels, pixels, true)) {
            return Pic::square(handle, size)
                .radius(radius)
                .opacity(opacity)
                .into();
        }
        let tint = faded(hex(tint(key)), opacity);
        container(
            text(initials(name))
                .size(size * 0.4)
                .color(faded(Color::WHITE, opacity)),
        )
        .center(size)
        .style(move |_| rounded(tint, radius))
        .into()
    }

    /// An avatar of `size` with the decoration round it (a fifth larger,
    /// centred over its edge, as Discord draws it), playing while `play`,
    /// and a presence dot ringed in `ring` at `dot`.
    pub fn face(self, face: Face<'_>) -> Element<'a, Message> {
        let Face {
            url,
            name,
            key,
            size,
            opacity,
            decoration,
            play,
            dot,
        } = face;
        let mut layers =
            stack![self.avatar_tinted(url, name, key.unwrap_or(name), size, size / 2.0, opacity)];
        let square = Size::new(size, size);
        if let Some(decoration) = decoration {
            let side = (size * 1.2).round();
            let inset = -(side - size) / 2.0;
            let at = Rectangle::new(Point::new(inset, inset), Size::new(side, side));
            let moving = play
                .then(|| self.moving(&looks::animated_url(decoration), square, at, false))
                .flatten();
            if let Some(moving) = moving {
                layers = layers.push(moving.opacity(opacity));
            } else if let Some(still) = self.picture(decoration, side as u32, side as u32, false) {
                layers = layers.push(Pic::new(still, square).at(at).opacity(opacity));
            }
        }
        if let Some((presence, ring, at)) = dot
            && let Some(dot) = self.mark(Mark::Dot(presence, ring))
        {
            // The phone stands 5 px taller than the dot.
            let tall = if presence == Presence::Mobile {
                5.0
            } else {
                0.0
            };
            let at = Rectangle::new(Point::new(at.x, at.y - tall), Size::new(16.0, 16.0 + tall));
            layers = layers.push(Pic::new(dot, square).at(at));
        }
        layers.width(size).height(size).into()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Instant};

    use super::Player;

    #[test]
    fn a_moving_picture_whose_file_went_stops_asking_for_frames() {
        let folder = tempfile::tempdir().expect("a folder");
        let gif = folder.path().join("hello.gif");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../testing/fixtures/gifs/hello.gif"
            ),
            &gif,
        )
        .expect("the fixture");
        let picture = yapfast_core::chat::decode_picture_at(&gif, 220, 165).expect("decoded");
        std::fs::remove_file(&gif).expect("gone");
        let mut player = Player::new(&Arc::new(picture)).expect("it moves");
        assert!(player.due().is_some());
        assert!(player.next(Instant::now()).is_none(), "no frame to give");
        assert!(player.due().is_none(), "and none due");
    }
}
