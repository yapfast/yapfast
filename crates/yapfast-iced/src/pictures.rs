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
    Color, Element, Point, Rectangle, Size,
    advanced::{graphics::text::font_system, image::Handle},
    widget::{container, stack, text},
};
use yapfast_core::chat::{Picture, Presence};

use crate::{
    app::{App, Message},
    looks,
    paint::{self, fitted},
    style::{faded, hex, rounded},
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
    Logo,
    TriangleAlert,
    Check,
    Lock,
    Folder,
    Speaker,
    ChevronDown,
    /// The chevron turned over, as an open menu's.
    ChevronUp,
    ChevronRight,
    UserPlus,
    Calendar,
    SquarePen,
    Inbox,
    Hash,
    HashLock,
    HashWarning,
    Voice,
    VoiceLock,
    VoiceWarning,
    Forum,
    ForumLock,
    ForumWarning,
    Announcement,
    AnnouncementLock,
    AnnouncementWarning,
    Media,
    MediaLock,
    MediaWarning,
    Stage,
    StageLock,
    Rules,
    Dm,
    Phone,
    Mic,
    MicOff,
    Headphones,
    HeadphoneOff,
    Settings,
    Community,
    Id,
    History,
    Forward,
    BellOff,
    User,
    Ellipsis,
    ChartBar,
    Search,
    ForumPost,
    /// The curve from a reply's line down to its avatar, in the top 13
    /// of its 33 px square.
    ReplySpine,
    File,
    Download,
    /// A filled triangle, the mark over a video's poster.
    PlayMark,
    Play,
    Pause,
    Volume,
    VolumeOff,
    Link,
    Chain,
    Eye,
    EyeOff,
    Gif,
    Sticker,
    Star,
    StarFilled,
    Clock,
    MessageCircle,
    PinOff,
    CircleStop,
    CirclePlus,
    TrendingUp,
    /// Discord's plain plus, the composer's.
    Add,
    ChevronLeft,
    /// An empty box, an unticked choice's.
    Square,
    Gem,
    Hand,
    Discord,
    PhoneOff,
    ScreenShare,
    ScreenShareOff,
    Soundboard,
    VideoOff,
    Maximize,
    Minimize,
    SignalHigh,
    SignalMedium,
    SignalLow,
    NoiseBars,
    NoiseBarsOff,
    Palette,
    Help,
    Refresh,
    LogOut,
    UserPen,
    Ban,
    GitHub,
    Image,
    Shop,
    Brush,
    Pen,
    ChevronsRight,
    Shield,
    Gavel,
    StickyNote,
    Slash,
}

impl Icon {
    fn svg(self) -> &'static str {
        macro_rules! icon {
            ($file:literal) => {
                include_str!(concat!("../../../ui/icons/", $file))
            };
        }
        match self {
            Icon::History => icon!("history.svg"),
            Icon::File => icon!("file.svg"),
            Icon::Download => icon!("download.svg"),
            Icon::PlayMark => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20"><path d="M2 0 L20 10 L2 20 Z" fill="currentColor" /></svg>"#
            }
            Icon::Play => icon!("play.svg"),
            Icon::Pause => icon!("pause.svg"),
            Icon::Volume => icon!("volume-2.svg"),
            Icon::VolumeOff => icon!("volume-x.svg"),
            Icon::Link => icon!("external-link.svg"),
            Icon::Chain => icon!("link.svg"),
            Icon::Eye => icon!("eye.svg"),
            Icon::EyeOff => icon!("eye-off.svg"),
            Icon::Gif => icon!("gif.svg"),
            Icon::Sticker => icon!("sticker.svg"),
            Icon::Star => icon!("star.svg"),
            Icon::StarFilled => icon!("star-filled.svg"),
            Icon::Clock => icon!("clock.svg"),
            Icon::MessageCircle => icon!("message-circle.svg"),
            Icon::PinOff => icon!("pin-off.svg"),
            Icon::CircleStop => icon!("circle-stop.svg"),
            Icon::CirclePlus => icon!("circle-plus.svg"),
            Icon::TrendingUp => icon!("trending-up.svg"),
            Icon::Add => icon!("plus.svg"),
            Icon::ChevronLeft => icon!("chevron-left.svg"),
            Icon::Forward => icon!("forward.svg"),
            Icon::BellOff => icon!("bell-off.svg"),
            Icon::User => icon!("user.svg"),
            Icon::Ellipsis => icon!("ellipsis.svg"),
            Icon::ChartBar => icon!("chart-bar.svg"),
            Icon::Search => icon!("search.svg"),
            Icon::ForumPost => icon!("message-square-text.svg"),
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
            Icon::Logo => icon!("logo.svg"),
            Icon::TriangleAlert => icon!("triangle-alert.svg"),
            Icon::Check => icon!("check.svg"),
            Icon::Square => icon!("square.svg"),
            Icon::Gem => icon!("gem.svg"),
            Icon::Hand => icon!("hand.svg"),
            Icon::Discord => icon!("discord.svg"),
            Icon::Lock => icon!("lock.svg"),
            Icon::Folder => icon!("folder.svg"),
            Icon::Speaker => icon!("volume-2-bold.svg"),
            Icon::ChevronDown => icon!("chevron-down.svg"),
            Icon::ChevronUp => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 15 6-6 6 6" /></svg>"#
            }
            Icon::ChevronRight => icon!("chevron-right.svg"),
            Icon::ReplySpine => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="33" height="33" viewBox="0 0 33 33" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 14 V7 A6 6 0 0 1 7 1 H33" /></svg>"#
            }
            Icon::UserPlus => icon!("user-plus.svg"),
            Icon::Calendar => icon!("calendar.svg"),
            Icon::SquarePen => icon!("square-pen.svg"),
            Icon::Inbox => icon!("inbox.svg"),
            Icon::Hash => icon!("hash.svg"),
            Icon::HashLock => icon!("hash-lock.svg"),
            Icon::HashWarning => icon!("hash-warning.svg"),
            Icon::Voice => icon!("volume-2.svg"),
            Icon::VoiceLock => icon!("volume-2-lock.svg"),
            Icon::VoiceWarning => icon!("volume-2-warning.svg"),
            Icon::Forum => icon!("message-square-text.svg"),
            Icon::ForumLock => icon!("message-square-text-lock.svg"),
            Icon::ForumWarning => icon!("message-square-text-warning.svg"),
            Icon::Announcement => icon!("megaphone.svg"),
            Icon::AnnouncementLock => icon!("megaphone-lock.svg"),
            Icon::AnnouncementWarning => icon!("megaphone-warning.svg"),
            Icon::Media => icon!("image.svg"),
            Icon::MediaLock => icon!("image-lock.svg"),
            Icon::MediaWarning => icon!("image-warning.svg"),
            Icon::Stage => icon!("stage.svg"),
            Icon::StageLock => icon!("stage-lock.svg"),
            Icon::Rules => icon!("rules.svg"),
            Icon::Dm => icon!("message-circle.svg"),
            Icon::Phone => icon!("phone.svg"),
            Icon::Mic => icon!("mic.svg"),
            Icon::MicOff => icon!("mic-off.svg"),
            Icon::Headphones => icon!("headphones.svg"),
            Icon::HeadphoneOff => icon!("headphone-off.svg"),
            Icon::Settings => icon!("settings.svg"),
            Icon::Community => icon!("community.svg"),
            Icon::Id => icon!("id-filled.svg"),
            Icon::PhoneOff => icon!("phone-off.svg"),
            Icon::ScreenShare => icon!("screen-share.svg"),
            Icon::ScreenShareOff => icon!("screen-share-off.svg"),
            Icon::Soundboard => icon!("soundboard.svg"),
            Icon::Palette => icon!("palette.svg"),
            Icon::Help => icon!("help-circle.svg"),
            Icon::Refresh => icon!("refresh-cw.svg"),
            Icon::LogOut => icon!("log-out.svg"),
            Icon::UserPen => icon!("user-pen.svg"),
            Icon::Ban => icon!("ban.svg"),
            Icon::GitHub => icon!("github.svg"),
            Icon::Image => icon!("image.svg"),
            Icon::Shop => icon!("shop.svg"),
            Icon::Brush => icon!("brush.svg"),
            Icon::Pen => icon!("pen.svg"),
            Icon::ChevronsRight => icon!("chevrons-right.svg"),
            Icon::Shield => icon!("shield.svg"),
            Icon::Gavel => icon!("gavel.svg"),
            Icon::StickyNote => icon!("sticky-note.svg"),
            Icon::Slash => icon!("slash.svg"),
            Icon::VideoOff => icon!("video-off.svg"),
            Icon::Maximize => icon!("maximize.svg"),
            Icon::Minimize => icon!("minimize.svg"),
            Icon::SignalHigh => icon!("signal-high.svg"),
            Icon::SignalMedium => icon!("signal-medium.svg"),
            Icon::SignalLow => icon!("signal-low.svg"),
            Icon::NoiseBars => icon!("noise-bars.svg"),
            Icon::NoiseBarsOff => icon!("noise-bars-off.svg"),
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
            // The theme's live green for online; the marks go when the
            // theme changes.
            Mark::Dot(presence, ring) => {
                paint::status_dot(presence, ring, crate::theme::rgb(self.theme.live))?
            }
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

    /// One of `ui/icons` as a picture, `size` square, in `color`.
    pub fn glyph(&self, icon: Icon, size: u32, color: Color) -> Pic {
        Pic::square(self.icon(icon, size, crate::theme::rgb(color)), size as f32)
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
            return Pic::square(handle, size)
                .radius(radius)
                .opacity(opacity)
                .into();
        }
        let tint = faded(hex(tint(name)), opacity);
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

/// A tint, as 0xRRGGBB, for an avatar without a picture, as the Slint
/// client picks it: mid tones that keep a white initial legible, the same
/// key always the same one.
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

/// The first letter, in capitals, as the Slint client's avatars show it.
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
            let _ = self.jobs.send((key, picture.clone()));
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
