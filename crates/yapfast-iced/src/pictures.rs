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
use yapfast_core::themes::content::{self, Media};

use crate::{
    animated::{Animated, Players},
    app::{App, Message},
    looks,
    paint::{self, fitted},
    style::{faded, hex, rounded},
    widgets::Pic,
};

/// The window's pixels across `units` of the view at `scale` (its display
/// scale times the zoom), rounded as the renderer rounds a picture's span,
/// so a picture made at it is copied in as it is rather than stretched.
pub fn window_pixels(units: u32, scale: f32) -> u32 {
    ((units as f32 * scale).round() as u32).max(1)
}

/// A picture's pixels as tiny-skia draws them, premultiplied BGRA.
struct NativePixels(Vec<u8>);

impl yapfast_core::chat::PixelBuffer for NativePixels {
    fn bytes(&self) -> &[u8] {
        &self.0
    }

    fn straight_rgba(&self) -> Vec<u8> {
        let mut pixels = self.0.clone();
        for pixel in pixels.chunks_exact_mut(4) {
            let [b, g, r, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let straight = |value: u8| match a {
                0 => 0,
                255 => value,
                _ => ((u16::from(value) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8,
            };
            pixel.copy_from_slice(&[straight(r), straight(g), straight(b), a]);
        }
        pixels
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaLook {
    pub media: Media,
    pub patterned: bool,
}

impl Default for MediaLook {
    fn default() -> Self {
        Self {
            media: Media::Normal,
            patterned: true,
        }
    }
}

fn apply_media(look: MediaLook, pixels: &mut [u8], width: u32, height: u32) {
    match look.media {
        Media::Dither(dither) if look.patterned && content::Dither::patterned(width, height) => {
            content::dither(&dither, pixels, width);
        }
        media => content::recolour(media, pixels),
    }
}

/// `picture`'s pixels under the theme's content look: borrowed as they are
/// when the look leaves them alone, so a stock fit copies nothing first.
fn themed_native(picture: &Picture, look: MediaLook) -> std::borrow::Cow<'_, [u8]> {
    if look.media == Media::Normal {
        return std::borrow::Cow::Borrowed(&picture.rgba);
    }
    let mut pixels = picture.rgba.straight_rgba();
    apply_media(look, &mut pixels, picture.width, picture.height);
    paint::to_native(&mut pixels);
    std::borrow::Cow::Owned(pixels)
}

pub(crate) fn apply_frame_media(look: MediaLook, pixels: &mut [u8], width: u32, height: u32) {
    apply_media(look, pixels, width, height);
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
    /// Presence dots, by what they are and their ring's colour, and the
    /// scale they're drawn at.
    pub marks: RefCell<HashMap<(Mark, u32), Handle>>,
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
        media: MediaLook,
    ) -> Animated<'_> {
        self.players.animated(picture, size, at, cover, media)
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

    /// Advances a picture playing from cached frames, returning the index of
    /// the frame that fell due by `now`.
    pub fn advance(&mut self, now: Instant) -> Option<usize> {
        let due = self.due()?;
        if now < due {
            return None;
        }
        let animation = self.picture.animation.as_ref()?;
        let total = animation.delays.len();
        if total == 0 {
            return None;
        }
        if self.index >= total {
            self.played += 1;
            if animation.plays != 0 && self.played >= animation.plays {
                return None;
            }
            self.index = 0;
        }
        let current = self.index;
        let delay = animation
            .delays
            .get(current)
            .copied()
            .unwrap_or(100)
            .max(20);
        self.index += 1;
        self.due = (due + Duration::from_millis(u64::from(delay))).max(now);
        Some(current)
    }

    /// Releases the frame iterator and underlying file when all frames have
    /// been cached.
    pub fn drop_frames(&mut self) {
        self.frames = Box::new(std::iter::empty());
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
    /// A theme presence dot's requested side instead of stock 16 px.
    pub dot_size: Option<f32>,
    /// A theme ring outside the avatar: ARGB, width and glow reach.
    pub ring: Option<(u32, f32, f32)>,
    /// Theme art around the avatar. A frame with a window places the
    /// avatar in that window; an ordinary frame is drawn over it.
    pub frame: Option<&'a yapfast_core::themes::lists::TileArt>,
    /// Illuminated initial opt-in (`Some((member_row, pointed))`) for
    /// message/member avatars when `content.avatars.initials` is declared.
    pub initial: Option<(bool, bool)>,
    /// Degrees a theme's frame turns about the face's centre (Zine's
    /// member prints); the picture in it stays straight.
    pub turn: f32,
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
            dot_size: None,
            ring: None,
            frame: None,
            initial: None,
            turn: 0.0,
        }
    }

    pub fn turn(self, turn: f32) -> Self {
        Self { turn, ..self }
    }

    pub fn illuminated(self, revealed: bool) -> Self {
        Self {
            initial: Some((false, revealed)),
            ..self
        }
    }

    pub fn illuminated_member(self, revealed: bool) -> Self {
        Self {
            initial: Some((true, revealed)),
            ..self
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

    pub fn dot_size(self, dot_size: Option<f32>) -> Self {
        Self { dot_size, ..self }
    }

    pub fn ring(self, ring: Option<(u32, f32, f32)>) -> Self {
        Self { ring, ..self }
    }

    pub fn frame(self, frame: Option<&'a yapfast_core::themes::lists::TileArt>) -> Self {
        Self { frame, ..self }
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
    media: MediaLook,
}

/// Copies of the core's pictures at the sizes messages show them at (a
/// gallery's tiles, a preview narrower than it came, a spoiler's blur),
/// made on a thread of their own so the window never waits on one, and let
/// go of once no view shows them; until a copy comes its place shows its
/// ground. A copy made comes back as a [`Fitted`].
pub struct Fitter {
    jobs: std::sync::mpsc::Sender<(FitKey, Arc<Picture>, MediaLook)>,
    /// Copies made, by what they copy, with the picture so a copy of a
    /// picture the core let go of goes too.
    done: HashMap<FitKey, (Weak<Picture>, Handle)>,
    /// Asked for and not back yet.
    asked: std::collections::HashSet<FitKey>,
    /// What the view being built shows.
    shown: std::collections::HashSet<FitKey>,
    media: MediaLook,
}

impl Fitter {
    /// The fitter, and the stream of copies it makes for the app to take
    /// in.
    pub fn new() -> (
        Self,
        impl iced::futures::Stream<Item = Fitted> + Send + 'static,
    ) {
        let (jobs, queue) = std::sync::mpsc::channel::<(FitKey, Arc<Picture>, MediaLook)>();
        let (made, copies) = iced::futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("fitter".into())
            .spawn(move || {
                while let Ok((key, picture, media)) = queue.recv() {
                    let (_, width, height, cover) = key;
                    let source = themed_native(&picture, media);
                    let pixels = fitted(
                        &source,
                        (picture.width, picture.height),
                        (width, height),
                        cover,
                    );
                    let fitted = Fitted {
                        key,
                        picture: Arc::downgrade(&picture),
                        handle: Handle::from_rgba(width, height, pixels),
                        media,
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
            media: MediaLook::default(),
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
        media: MediaLook,
    ) -> Option<Handle> {
        if picture.width == 0 || picture.height == 0 || width == 0 || height == 0 {
            return None;
        }
        self.set_media(media);
        let key = (Arc::as_ptr(picture) as usize, width, height, cover);
        self.shown.insert(key);
        if let Some((_, handle)) = self.done.get(&key) {
            return Some(handle.clone());
        }
        if self.asked.insert(key) {
            self.jobs.send((key, picture.clone(), media)).ok();
        }
        None
    }

    /// A copy came. One of a picture the core let go of meanwhile, or that
    /// the next view doesn't show, goes then.
    pub fn made(&mut self, fitted: Fitted) {
        self.asked.remove(&fitted.key);
        if fitted.media != self.media {
            return;
        }
        self.done
            .insert(fitted.key, (fitted.picture, fitted.handle));
    }

    fn set_media(&mut self, media: MediaLook) {
        if self.media == media {
            return;
        }
        self.media = media;
        self.done.clear();
        self.asked.clear();
        self.shown.clear();
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
                &themed_native(picture, self.media),
                (picture.width, picture.height),
                (width, height),
                cover,
            );
            self.made(Fitted {
                key,
                picture: Arc::downgrade(picture),
                handle: Handle::from_rgba(width, height, pixels),
                media: self.media,
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
    pub fn media_look(self) -> MediaLook {
        MediaLook {
            media: self
                .themes
                .content(
                    &self.look.theme.id,
                    self.layout.theme_content.contains(&self.look.theme.id),
                )
                .map_or(Media::Normal, |content| content.media),
            patterned: self.layout.theme_performance != yapfast_core::themes::Performance::Light,
        }
    }

    /// The picture at `url` at `width` by `height`: the core's own pixels
    /// when it came at that size, else a copy made once (cut to cover the
    /// size, or stretched to it).
    pub fn picture(self, url: &str, width: u32, height: u32, cover: bool) -> Option<Handle> {
        let picture = self.state.pictures.get(url)?;
        self.fit(picture, width, height, cover)
    }

    /// The window's pixels across `units` of the view (its display scale
    /// times the zoom), rounded as the renderer rounds a picture's span:
    /// what a picture shown `units` across is made at, so the renderer
    /// copies it in as it is rather than stretching it.
    pub fn pixels(self, units: u32) -> u32 {
        window_pixels(units, self.scale)
    }

    /// `picture` to show `width` by `height` units, made at the window's
    /// pixels ([`Self::pixels`]).
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
        let (width, height) = (self.pixels(width), self.pixels(height));
        if picture.width == width && picture.height == height {
            return Some(self.own_size(picture));
        }
        let media = self.media_look();
        Some(
            self.pictures
                .scaled
                .borrow_mut()
                .entry((width, height, cover))
                .or_default()
                .get(picture, |picture| {
                    let pixels = themed_native(picture, media);
                    let pixels = fitted(
                        &pixels,
                        (picture.width, picture.height),
                        (width, height),
                        cover,
                    );
                    Handle::from_rgba(width, height, pixels)
                }),
        )
    }

    /// `picture` at its own pixels: the core's own, shared, unless the
    /// theme's content look changes them.
    pub fn own_size(self, picture: &Arc<Picture>) -> Handle {
        let media = self.media_look();
        self.pictures.previews.borrow_mut().get(picture, |picture| {
            if media.media == Media::Normal {
                shared(picture)
            } else {
                Handle::from_rgba(
                    picture.width,
                    picture.height,
                    themed_native(picture, media).into_owned(),
                )
            }
        })
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
                .animated(
                    picture,
                    square,
                    Rectangle::with_size(square),
                    false,
                    self.media_look(),
                )
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
        Some(
            self.pictures
                .animated(picture, size, at, cover, self.media_look()),
        )
    }

    /// A presence dot, drawn once at the window's pixels.
    pub fn mark(self, mark: Mark) -> Option<Handle> {
        let key = (mark, self.scale.to_bits());
        if let Some(handle) = self.pictures.marks.borrow().get(&key) {
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
                self.scale,
            )?,
        };
        let handle = handle(canvas);
        self.pictures.marks.borrow_mut().insert(key, handle.clone());
        Some(handle)
    }

    /// An emoji's picture to show at `size`, for the menu's quick reactions
    /// and tooltips: made once at the window's pixels, kept with the
    /// client's marks.
    pub fn emoji_picture(self, key: &'static str, size: u32) -> Option<Handle> {
        let size = self.pixels(size);
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
        let radius = self.look.theme.avatar_radius(size, radius);
        self.avatar_tinted_exact(url, name, key, size, radius, opacity)
    }

    fn avatar_tinted_exact(
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
        // A theme's window draws the initials as Slint does: 700, 0.42 of
        // the face, in the theme's body face; otherwise the window's face.
        let theme = self.look.theme;
        let words = text(initials(name)).color(faded(Color::WHITE, opacity));
        let words = if theme.packaged() {
            let mut font = theme.font("body", crate::fonts::BOLD);
            font.weight = crate::fonts::BOLD.weight;
            words.size(size * 0.42).font(font)
        } else {
            words
                .size(size * 0.4)
                .font(theme.font("small", crate::fonts::MEDIUM))
        };
        container(words)
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
            dot_size,
            ring,
            frame,
            initial,
            turn,
        } = face;
        let content = self.themes.content(
            &self.look.theme.id,
            self.layout.theme_content.contains(&self.look.theme.id),
        );
        let window = frame.and_then(|frame| frame.window);
        let avatar_size = window.map_or(size, |window| window[2].max(1.0));
        let radius = if let (Some(frame), Some(_)) = (frame, window) {
            frame.window_radius
        } else {
            match content.map(|content| content.avatars.shape) {
                Some(yapfast_core::themes::content::AvatarShape::Circle) => avatar_size / 2.0,
                Some(yapfast_core::themes::content::AvatarShape::Square) => 0.0,
                Some(yapfast_core::themes::content::AvatarShape::Rounded) => {
                    content.map_or(8.0, |content| content.avatars.radius)
                }
                _ => self
                    .look
                    .theme
                    .avatar_radius(avatar_size, avatar_size / 2.0),
            }
        };
        let illuminated = initial.and_then(|(for_member, revealed)| {
            let initials = content.and_then(|c| c.avatars.initials)?;
            if (for_member && !initials.members) || (initials.reveal && revealed) {
                return None;
            }
            let letter = yapfast_core::themes::content::initial_letter(name)?;
            Some((initials, letter))
        });
        let avatar = if let Some((initials, letter)) = illuminated {
            let theme = self.look.theme;
            let ink = faded(
                initials
                    .colour
                    .map(crate::themes::argb)
                    .unwrap_or_else(|| theme.accent_text()),
                opacity,
            );
            let ground = initials
                .ground
                .map(crate::themes::argb)
                .map(|c| faded(c, opacity));
            let border = initials.border.map_or_else(
                || iced::Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                |(c, w)| iced::Border {
                    color: faded(crate::themes::argb(c), opacity),
                    width: w,
                    radius: radius.into(),
                },
            );
            let font = theme.font(initials.font.name(), crate::fonts::BOLD);
            container(
                text(letter)
                    .size((avatar_size * initials.size).round().max(1.0))
                    .font(font)
                    .color(ink),
            )
            .center(avatar_size)
            .style(move |_| iced::widget::container::Style {
                background: ground.map(Into::into),
                border,
                ..Default::default()
            })
            .into()
        } else {
            self.avatar_tinted_exact(url, name, key.unwrap_or(name), avatar_size, radius, opacity)
        };
        let avatar: Element<'a, Message> = match content.and_then(|content| content.avatars.ring) {
            Some((colour, width)) => container(avatar)
                .style(move |_| container::Style {
                    border: iced::Border {
                        color: hex(colour & 0x00ff_ffff),
                        width,
                        radius: radius.into(),
                    },
                    ..Default::default()
                })
                .into(),
            None => avatar,
        };
        let avatar: Element<'a, Message> = match window {
            Some([x, y, _, _]) => crate::controls::placed(avatar, x, y),
            None => avatar,
        };
        let mut layers = stack![];
        if let Some((colour, width, glow)) = ring {
            let reach = 3.0 + width;
            let side = size + reach * 2.0;
            let colour = crate::themes::argb(colour);
            layers = layers.push(crate::controls::placed(
                container(iced::widget::Space::new())
                    .width(side)
                    .height(side)
                    .style(move |_| container::Style {
                        border: iced::Border {
                            color: colour,
                            width,
                            radius: (side / 2.0).into(),
                        },
                        shadow: iced::Shadow {
                            color: colour.scale_alpha(0.6),
                            blur_radius: glow,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                -reach,
                -reach,
            ));
        }
        if window.is_some()
            && let Some(frame) = frame
            && let Some(art) = crate::theme_art::tile_art_turned(
                self,
                yapfast_core::themes::package::art::ArtSurface::Members,
                frame,
                size,
                size,
                turn,
            )
        {
            layers = layers.push(art);
        }
        layers = layers.push(avatar);
        let square = Size::new(size, size);
        let decorations = content.is_none_or(|content| {
            content.avatars.decorations != yapfast_core::themes::content::Shown::Hide
        });
        if decorations && let Some(decoration) = decoration {
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
        if window.is_none()
            && let Some(frame) = frame
            && let Some(art) = crate::theme_art::tile_art(
                self,
                yapfast_core::themes::package::art::ArtSurface::Members,
                frame,
                size,
                size,
            )
        {
            layers = layers.push(art);
        }
        if let Some((presence, ring, at)) = dot
            && let Some(dot) = self.mark(Mark::Dot(presence, ring))
        {
            // The phone stands 5 px taller than the dot.
            let side = dot_size.unwrap_or(16.0).max(1.0);
            let tall = if presence == Presence::Mobile {
                side * 5.0 / 16.0
            } else {
                0.0
            };
            let at = Rectangle::new(Point::new(at.x, at.y - tall), Size::new(side, side + tall));
            layers = layers.push(Pic::new(dot, square).at(at));
        }
        layers.width(size).height(size).into()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Instant};

    use yapfast_core::themes::content::{Dither, Media};

    use super::{MediaLook, NativePixels, Player, apply_media};

    #[test]
    fn native_pixels_leave_as_straight_rgba() {
        use yapfast_core::chat::PixelBuffer;
        // Opaque, half see-through and clear: red stays red, not blue.
        let rgba = [200, 40, 10, 255, 200, 40, 10, 128, 0, 0, 0, 0];
        let mut native = rgba.to_vec();
        crate::paint::to_native(&mut native);
        assert_ne!(native[..3], rgba[..3]);
        let straight = NativePixels(native).straight_rgba();
        assert_eq!(straight[..4], rgba[..4]);
        for (got, want) in straight[4..8].iter().zip(&rgba[4..8]) {
            assert!(got.abs_diff(*want) <= 2, "{straight:?}");
        }
        assert_eq!(straight[8..], rgba[8..]);
    }

    #[test]
    fn media_recolouring_keeps_alpha() {
        let mut pixels = [240, 60, 20, 17, 20, 180, 220, 203];
        apply_media(
            MediaLook {
                media: Media::Mono,
                patterned: true,
            },
            &mut pixels,
            2,
            1,
        );
        assert_eq!(pixels[0], pixels[1]);
        assert_eq!(pixels[1], pixels[2]);
        assert_eq!(pixels[4], pixels[5]);
        assert_eq!(pixels[5], pixels[6]);
        assert_eq!([pixels[3], pixels[7]], [17, 203]);
    }

    #[test]
    fn performance_light_turns_a_large_dither_into_a_gradient() {
        let dither = Dither {
            palette: [0xff00_0000, 0xffff_ffff, 0, 0, 0, 0, 0, 0],
            len: 2,
            matrix: 2,
            scale: 1,
        };
        let source = [128, 128, 128, 255].repeat(65);
        let mut patterned = source.clone();
        let mut gradient = source;
        apply_media(
            MediaLook {
                media: Media::Dither(dither),
                patterned: true,
            },
            &mut patterned,
            65,
            1,
        );
        apply_media(
            MediaLook {
                media: Media::Dither(dither),
                patterned: false,
            },
            &mut gradient,
            65,
            1,
        );
        assert_ne!(patterned, gradient);
        assert!(
            patterned
                .chunks_exact(4)
                .all(|pixel| { pixel[..3] == [0, 0, 0] || pixel[..3] == [255, 255, 255] })
        );
    }

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

    #[test]
    fn advance_steps_through_frames_and_loops() {
        let gif = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testing/fixtures/gifs/hello.gif"
        );
        let picture = yapfast_core::chat::decode_picture_at(std::path::Path::new(gif), 220, 165)
            .expect("decoded");
        let total = picture.animation.as_ref().unwrap().delays.len();
        let mut player = Player::new(&Arc::new(picture)).expect("it moves");
        player.drop_frames();
        let mut now = Instant::now();
        for expected in 0..total {
            let due = player.due().expect("due");
            now = now.max(due);
            let idx = player.advance(now).expect("advanced");
            assert_eq!(idx, expected);
        }
        // Loops back to frame 0
        let due = player.due().expect("due");
        now = now.max(due);
        let idx = player.advance(now).expect("advanced second loop");
        assert_eq!(idx, 0);
    }
}
