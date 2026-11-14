//! Pictures in the UI's own buffers, and the pictures the client makes.
//!
//! The core decodes every picture into the renderer's own format: on the CPU
//! build tiny-skia's premultiplied BGRA ([`install`]), which the patched
//! renderer copies onto the frame as it is (patch 1 in
//! `vendor/iced_tiny_skia`), on the GPU build the straight RGBA iced_wgpu
//! uploads. So the one buffer the core keeps is the one the renderer reads,
//! and an image `Handle` wraps it without a copy ([`shared`]). A picture
//! drawn at another size than it came at is fitted once per size and let go
//! of with the core's picture ([`Converted`]). Moving pictures play through
//! [`Player`], a frame decoded only as it falls due, on the window's one
//! redraw clock (`widgets::Animated`).

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use iced::{
    Element, Point, Rectangle, Size,
    advanced::{graphics::text::font_system, image::Handle},
    widget::{container, stack, text},
};
use yapfast_core::chat::{Picture, Presence};

use crate::{
    app::{App, Message},
    looks,
    paint::{self, fitted},
    style::{LIVE, TEXT, faded, hex, rounded},
    widgets::{Animated, Pic},
};

/// A picture's pixels as tiny-skia draws them, premultiplied BGRA.
#[cfg(feature = "cpu")]
struct NativePixels(Vec<u8>);

#[cfg(feature = "cpu")]
impl yapfast_core::chat::PixelBuffer for NativePixels {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Has the core decode pictures into the renderer's format. The GPU build
/// keeps the core's own straight RGBA, which is what iced_wgpu uploads.
pub fn install() {
    #[cfg(feature = "cpu")]
    yapfast_core::chat::set_pixel_buffers(|width, height, rgba| {
        let mut pixels = rgba.to_vec();
        debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
        paint::to_native(&mut pixels);
        Box::new(NativePixels(pixels))
    });
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

/// A handle on pixels the client made, in the renderer's format.
pub fn handle(mut canvas: paint::Canvas) -> Handle {
    paint::finish(&mut canvas.pixels);
    Handle::from_rgba(canvas.width, canvas.height, canvas.pixels)
}

/// Images the client holds for the core's pictures, by the picture they
/// show: one each, let go of once the core lets go of the picture, so the
/// core's picture budget stays the budget.
pub struct Converted<T> {
    held: HashMap<usize, (Weak<Picture>, T)>,
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

/// Plays a moving picture as the Slint client does: frames decoded one at a
/// time as they fall due, from the core's picture, at its own delays, with
/// nothing kept but the frame showing.
pub struct Player {
    picture: Arc<Picture>,
    frames: Box<dyn Iterator<Item = Vec<u8>>>,
    index: usize,
    due: Instant,
    played: u32,
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
        })
    }

    /// When the next frame is due; none once a picture that plays a set
    /// number of times has finished.
    pub fn due(&self) -> Option<Instant> {
        let animation = self.picture.animation.as_ref()?;
        (animation.plays == 0 || self.played < animation.plays).then_some(self.due)
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
                self.frames.next()?
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

/// Small pictures the client draws for marks: presence dots and icons.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    /// A presence dot on a ring of the ground colour.
    Dot(Presence, u32),
    /// An icon at its size, in its colour.
    Icon(Icon, u32, u32),
}

/// The Slint client's icons the client draws, from `ui/icons`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    SmilePlus,
    Smile,
    Pencil,
    Reply,
    Copy,
    TextSelect,
    Trash,
    Thread,
    Bell,
    Pin,
    Users,
    X,
    Scissors,
    Paste,
    Plus,
    Crown,
    Gamepad,
    Music,
    Tv,
    Video,
    Trophy,
}

impl Icon {
    fn svg(self) -> &'static str {
        macro_rules! icon {
            ($file:literal) => {
                include_str!(concat!("../../../ui/icons/", $file))
            };
        }
        match self {
            Icon::SmilePlus => icon!("smile-plus.svg"),
            Icon::Smile => icon!("smile.svg"),
            Icon::Pencil => icon!("pencil.svg"),
            Icon::Reply => icon!("reply.svg"),
            Icon::Copy => icon!("copy.svg"),
            Icon::TextSelect => icon!("text-select.svg"),
            Icon::Trash => icon!("trash-2.svg"),
            Icon::Thread => icon!("thread.svg"),
            Icon::Bell => icon!("bell.svg"),
            Icon::Pin => icon!("pin.svg"),
            Icon::Users => icon!("users.svg"),
            Icon::X => icon!("x.svg"),
            Icon::Scissors => icon!("scissors.svg"),
            Icon::Paste => icon!("clipboard-paste.svg"),
            Icon::Plus => icon!("circle-plus.svg"),
            Icon::Crown => icon!("crown.svg"),
            Icon::Gamepad => icon!("gamepad.svg"),
            Icon::Music => icon!("music.svg"),
            Icon::Tv => icon!("tv.svg"),
            Icon::Video => icon!("video.svg"),
            Icon::Trophy => icon!("trophy.svg"),
        }
    }

    /// The icon of an activity line's kind (1 playing, 2 listening,
    /// 3 watching, 4 streaming, 5 competing).
    pub fn activity(kind: u8) -> Option<Self> {
        Some(match kind {
            1 => Icon::Gamepad,
            2 => Icon::Music,
            3 => Icon::Tv,
            4 => Icon::Video,
            5 => Icon::Trophy,
            _ => return None,
        })
    }
}

impl App {
    /// The picture at `url` at `width` by `height`: the core's own pixels
    /// when it came at that size, else a copy made once (cut to cover the
    /// size, or stretched to it).
    pub fn picture(&self, url: &str, width: u32, height: u32, cover: bool) -> Option<Handle> {
        let picture = self.client.state().pictures.get(url)?;
        self.fit(picture, width, height, cover)
    }

    pub fn fit(
        &self,
        picture: &Arc<Picture>,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        if picture.width == 0 || picture.height == 0 || width == 0 || height == 0 {
            return None;
        }
        if picture.width == width && picture.height == height {
            return Some(self.previews.borrow_mut().get(picture, shared));
        }
        Some(
            self.scaled
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

    /// A moving picture at `url`, once it has come, as an `Animated` drawn
    /// at `at` within a layout of `size`; it shows nothing until its first
    /// frame unless given a still.
    pub fn moving(
        &self,
        url: &str,
        size: Size,
        at: Rectangle,
        cover: bool,
    ) -> Option<Animated<'_>> {
        let picture = self.client.state().pictures.get(url)?;
        picture.animation.as_ref()?;
        let animated = Animated::new(picture.clone(), size, at, cover, &self.playing);
        self.moving.borrow_mut().insert(animated.key());
        Some(animated)
    }

    /// A presence dot or an icon, drawn once.
    pub fn mark(&self, mark: Mark) -> Option<Handle> {
        if let Some(handle) = self.marks.borrow().get(&mark) {
            return Some(handle.clone());
        }
        let canvas = match mark {
            Mark::Dot(presence, ring) => paint::status_dot(presence, ring, LIVE)?,
            Mark::Icon(icon, size, color) => paint::svg_icon(icon.svg(), size, color),
        };
        let handle = handle(canvas);
        self.marks.borrow_mut().insert(mark, handle.clone());
        Some(handle)
    }

    /// One of `ui/icons` at `size` in `color`, drawn once.
    pub fn icon(&self, icon: Icon, size: u32, color: u32) -> Handle {
        self.mark(Mark::Icon(icon, size, color))
            .expect("icons are always drawn")
    }

    /// An emoji's picture at `size`, for the menu's quick reactions and
    /// tooltips: made once, kept with the client's marks.
    pub fn emoji_picture(&self, key: &'static str, size: u32) -> Option<Handle> {
        if let Some(handle) = self.emoji.borrow().get(&(key, size)) {
            return handle.clone();
        }
        let canvas = {
            let mut system = font_system().write().expect("font system");
            paint::emoji(system.raw(), key, size)
        };
        let handle = canvas.map(handle);
        self.emoji.borrow_mut().insert((key, size), handle.clone());
        handle
    }

    /// An avatar or icon: the picture once it has come, else initials on a
    /// tint; `opacity` fades both.
    pub fn avatar(
        &self,
        url: Option<&str>,
        name: &str,
        size: f32,
        radius: f32,
        opacity: f32,
    ) -> Element<'_, Message> {
        let pixels = size as u32;
        if let Some(handle) = url.and_then(|url| self.picture(url, pixels, pixels, true)) {
            return Pic::new(handle, Size::new(size, size))
                .radius(radius)
                .opacity(opacity)
                .into();
        }
        let tint = faded(hex(tint(name)), opacity);
        container(
            text(initials(name))
                .size(size * 0.4)
                .color(faded(hex(TEXT), opacity)),
        )
        .center(size)
        .style(move |_| rounded(tint, radius))
        .into()
    }

    /// An avatar of `size` with the decoration round it (a fifth larger,
    /// centred over its edge, as Discord draws it), playing while `play`,
    /// and a presence dot ringed in `ring` at `dot`.
    pub fn face(&self, face: Face<'_>) -> Element<'_, Message> {
        let Face {
            url,
            name,
            size,
            opacity,
            decoration,
            play,
            dot,
        } = face;
        let mut layers = stack![self.avatar(url, name, size, size / 2.0, opacity)];
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

/// What [`App::face`] draws.
pub struct Face<'a> {
    pub url: Option<&'a str>,
    pub name: &'a str,
    pub size: f32,
    pub opacity: f32,
    pub decoration: Option<&'a str>,
    /// Whether the decoration plays: its row or card is pointed at.
    pub play: bool,
    /// The presence dot, the ring's colour, and where the dot goes.
    pub dot: Option<(Presence, u32, Point)>,
}

/// A tint, as 0xRRGGBB, for an avatar without a picture, from its owner's
/// name.
pub fn tint(key: &str) -> u32 {
    const TINTS: [u32; 6] = [0x5865f2, 0x3ba55d, 0xfaa61a, 0xed4245, 0xeb459e, 0x7289da];
    let sum: usize = key.bytes().map(usize::from).sum();
    TINTS[sum % TINTS.len()]
}

/// Up to two initials, in capitals.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}
