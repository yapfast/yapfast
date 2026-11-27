//! The client's icons, from the kit's `icons/`, and the pictures drawn of
//! them: each at a size and colour drawn once and kept until the window
//! goes, the theme changes or the window's pixels per unit do.
//!
//! An icon is drawn at the window's own pixels, its size times the display
//! scale and zoom, and shown at the size asked: drawn at the size asked and
//! stretched by the renderer, as at 110%, every edge of a flat icon blurs.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::Arc,
};

use iced::advanced::image::Handle;

use crate::canvas::{self, handle};

/// Icons drawn, by icon, size in the window's pixels and colour;
/// [`Look`](crate::look::Look) draws from it.
pub struct Icons {
    drawn: RefCell<HashMap<Drawn, Handle>>,
    replacements: HashMap<String, Replacement>,
    /// The window's pixels to the view's units, which icons are drawn at.
    scale: Cell<f32>,
}

impl Default for Icons {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

/// A checked installed-theme SVG and how it is coloured.
#[derive(Clone)]
pub struct Replacement {
    pub svg: Arc<str>,
    pub colour: ReplacementColour,
}

#[derive(Clone, Copy)]
pub enum ReplacementColour {
    /// The colour the call site gives the stock icon.
    Tint,
    /// The SVG's own colours.
    Keep,
    /// One resolved theme colour.
    Colour(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Drawn {
    /// In one colour.
    Coloured(Icon, u32, u32),
    /// In its own colours, such as the verified badge's green and white.
    Own(Icon, u32),
    /// In one colour even when the pack keeps a replacement's own.
    Tinted(Icon, u32, u32),
}

impl Icons {
    pub fn new(replacements: HashMap<String, Replacement>) -> Self {
        Self {
            drawn: RefCell::default(),
            replacements,
            scale: Cell::new(1.0),
        }
    }

    /// Draws icons at `scale` window pixels to the view's unit from now on
    /// (the display's scale times the zoom), letting go of those drawn at
    /// another.
    pub fn set_scale(&self, scale: f32) {
        if !scale.is_finite() || scale <= 0.0 || scale == self.scale.get() {
            return;
        }
        self.scale.set(scale);
        self.forget();
    }

    /// The window's pixels across an icon `size` units across, rounded as
    /// the renderer rounds a picture's span, so it's copied in unscaled.
    fn pixels(&self, size: u32) -> u32 {
        ((size as f32 * self.scale.get()).round() as u32).max(1)
    }

    /// Replaces the active package's icon pack and lets go of every raster
    /// made from the one before it.
    pub fn replace(&mut self, replacements: HashMap<String, Replacement>) {
        self.replacements = replacements;
        self.forget();
    }

    /// `icon` at `size` in `rgb`, drawn once.
    pub fn coloured(&self, icon: Icon, size: u32, rgb: u32) -> Handle {
        self.get(Drawn::Coloured(icon, size, rgb))
    }

    /// `icon` at `size` in `rgb` even where the pack keeps its own
    /// colours, as a theme list's own icon colours take Slint's pack icons.
    pub fn tinted(&self, icon: Icon, size: u32, rgb: u32) -> Handle {
        self.get(Drawn::Tinted(icon, size, rgb))
    }

    /// `icon` at `size` in its own colours, drawn once.
    pub fn own(&self, icon: Icon, size: u32) -> Handle {
        self.get(Drawn::Own(icon, size))
    }

    fn get(&self, drawn: Drawn) -> Handle {
        let drawn = drawn.at(self.pixels(drawn.size()));
        if let Some(handle) = self.drawn.borrow().get(&drawn) {
            return handle.clone();
        }
        let replacement = drawn
            .icon()
            .name()
            .and_then(|name| self.replacements.get(name.as_ref()));
        let canvas = match (drawn, replacement) {
            (Drawn::Coloured(_, size, rgb), Some(replacement)) => match replacement.colour {
                ReplacementColour::Tint => canvas::svg_icon(&replacement.svg, size, rgb),
                ReplacementColour::Keep => canvas::svg_drawn(&replacement.svg, size),
                ReplacementColour::Colour(colour) => {
                    canvas::svg_icon(&replacement.svg, size, colour)
                }
            },
            (Drawn::Own(_, size), Some(replacement)) => match replacement.colour {
                ReplacementColour::Keep | ReplacementColour::Tint => {
                    canvas::svg_drawn(&replacement.svg, size)
                }
                ReplacementColour::Colour(colour) => {
                    canvas::svg_icon(&replacement.svg, size, colour)
                }
            },
            (Drawn::Tinted(_, size, rgb), Some(replacement)) => {
                canvas::svg_icon(&replacement.svg, size, rgb)
            }
            (Drawn::Coloured(icon, size, rgb) | Drawn::Tinted(icon, size, rgb), None) => {
                canvas::svg_icon(icon.svg(), size, rgb)
            }
            (Drawn::Own(icon, size), None) => canvas::svg_drawn(icon.svg(), size),
        };
        let handle = handle(canvas);
        self.drawn.borrow_mut().insert(drawn, handle.clone());
        handle
    }

    /// Lets go of every icon drawn: the window went, or the theme changed.
    pub fn forget(&self) {
        self.drawn.borrow_mut().clear();
    }

    /// Whether it holds no icon: the window let them go.
    pub fn is_empty(&self) -> bool {
        self.drawn.borrow().is_empty()
    }
}

impl Drawn {
    fn icon(self) -> Icon {
        match self {
            Self::Coloured(icon, ..) | Self::Own(icon, ..) | Self::Tinted(icon, ..) => icon,
        }
    }

    fn size(self) -> u32 {
        match self {
            Self::Coloured(_, size, _) | Self::Own(_, size) | Self::Tinted(_, size, _) => size,
        }
    }

    /// The same icon `size` pixels across.
    fn at(self, size: u32) -> Self {
        match self {
            Self::Coloured(icon, _, rgb) => Self::Coloured(icon, size, rgb),
            Self::Own(icon, _) => Self::Own(icon, size),
            Self::Tinted(icon, _, rgb) => Self::Tinted(icon, size, rgb),
        }
    }
}

/// The icons the client draws, from the kit's `icons/`.
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
    /// A solid calendar, the Events row's.
    CalendarFilled,
    /// A speech bubble with a check at its corner: the unread banner's mark
    /// as read.
    MarkRead,
    /// The point of the "NEW" tag's left end, in the top left 7 px of its
    /// 13 px square.
    NoticeNotch,
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
    Keyboard,
    Headphones,
    HeadphoneOff,
    Settings,
    Community,
    Verified,
    /// A scalloped badge, which holds a server's boost level.
    Scallop,
    /// The note's filled info disc, the friends' kebab and the new group
    /// DM, which Discord draws in these shapes.
    CircleInfoFilled,
    EllipsisVertical,
    MessageCirclePlus,
    UserCheck,
    /// The title bar's minimize, maximize, restore and close, drawn in 1 px
    /// strokes in a 10 px square, as Discord's Windows frame draws them.
    CaptionMin,
    CaptionMax,
    CaptionRestore,
    CaptionClose,
    Id,
    History,
    Forward,
    BellOff,
    User,
    Ellipsis,
    ChartBar,
    Search,
    /// Up and down arrows: a Sort menu.
    ArrowsUpDown,
    /// Two sliders: a Filter menu, as the inbox's Mentions tab has.
    FiltersHorizontal,
    ForumPost,
    /// The curve from a reply's line down to its avatar, in the top 13
    /// of its 33 px square.
    ReplySpine,
    /// Sidebar connectors spanning a 32 px thread row and its 2 px gap.
    ThreadBranch,
    ThreadEnd,
    /// The rounded top of a thread connector in the gap below its parent.
    ThreadCap,
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
    /// Watching a stream beside those watched, as Discord's EyePlusIcon.
    EyePlus,
    Gif,
    Sticker,
    Star,
    StarFilled,
    Clock,
    /// Discord's stopwatch, by a slowmode's note.
    Timer,
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
    Accessibility,
    Help,
    /// The window's own title bar: back and forward, minimize, restore.
    ArrowLeft,
    ArrowRight,
    Minus,
    Restore,
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
    Send,
    Eyedropper,
    Moon,
    Sun,
    AtSign,
    UserMinus,
    UserX,
    /// A disc with a person cut out: Switch Accounts'.
    UserCircle,
    FolderPlus,
    CopyPlus,
}

impl Icon {
    /// The public theme-icon name for this glyph. The two kit-only drawings
    /// have no package replacement.
    fn name(self) -> Option<Cow<'static, str>> {
        let special = match self {
            Self::PlayMark => "play",
            Self::Volume => "volume-2",
            Self::VolumeOff => "volume-x",
            Self::Link => "external-link",
            Self::Chain => "link",
            Self::Paste => "clipboard-paste",
            Self::Plus => "circle-plus",
            Self::Add => "plus",
            Self::Trash => "trash-2",
            Self::Speaker => "volume-2-bold",
            Self::Voice => "volume-2",
            Self::VoiceLock => "volume-2-lock",
            Self::VoiceWarning => "volume-2-warning",
            Self::Forum | Self::ForumPost => "message-square-text",
            Self::ForumLock => "message-square-text-lock",
            Self::ForumWarning => "message-square-text-warning",
            Self::Announcement => "megaphone",
            Self::AnnouncementLock => "megaphone-lock",
            Self::AnnouncementWarning => "megaphone-warning",
            Self::Media => "image",
            Self::MediaLock => "image-lock",
            Self::MediaWarning => "image-warning",
            Self::Dm => "message-circle",
            Self::Help => "help-circle",
            Self::Id => "id-filled",
            Self::Refresh => "refresh-cw",
            Self::Send => "send-horizontal",
            Self::GitHub => "github",
            Self::ChevronUp | Self::ReplySpine => return None,
            _ => return Some(Cow::Owned(kebab(&format!("{self:?}")))),
        };
        Some(Cow::Borrowed(special))
    }

    fn svg(self) -> &'static str {
        macro_rules! icon {
            ($file:literal) => {
                include_str!(concat!("../icons/", $file))
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
            Icon::EyePlus => icon!("eye-plus.svg"),
            Icon::Gif => icon!("gif.svg"),
            Icon::Sticker => icon!("sticker.svg"),
            Icon::Star => icon!("star.svg"),
            Icon::StarFilled => icon!("star-filled.svg"),
            Icon::Clock => icon!("clock.svg"),
            Icon::Timer => icon!("timer.svg"),
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
            Icon::ArrowsUpDown => icon!("arrows-up-down.svg"),
            Icon::FiltersHorizontal => icon!("filters-horizontal.svg"),
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
            // A single silhouette keeps the 2 px stem and arm joined.
            // The terminal bend has a taller outer curve than its inner one.
            Icon::ThreadBranch => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="34" height="34" viewBox="0 0 34 34" fill="currentColor"><path d="M16 0 H18 V12 A3.25 3 0 0 0 21.25 15 H25.4 A.6 .6 0 0 1 26 15.6 V16.4 A.6 .6 0 0 1 25.4 17 H21.25 A3.25 3 0 0 0 18 20 V34 H16 Z" /></svg>"#
            }
            Icon::ThreadEnd => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="34" height="34" viewBox="0 0 34 34" fill="currentColor"><path d="M16 0 H18 V12 A3.25 3 0 0 0 21.25 15 H24.6 A1.4 1 0 0 1 26 16 A1.05 1 0 0 1 24.95 17 H20.75 A4.75 6.75 0 0 1 16 10.25 Z" /></svg>"#
            }
            Icon::ThreadCap => {
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2" viewBox="0 0 2 2" fill="currentColor"><path d="M0 1 A1 1 0 0 1 2 1 Z" /></svg>"#
            }
            Icon::UserPlus => icon!("user-plus.svg"),
            Icon::Calendar => icon!("calendar.svg"),
            Icon::CalendarFilled => icon!("calendar-filled.svg"),
            Icon::MarkRead => icon!("mark-read.svg"),
            Icon::NoticeNotch => icon!("notice-notch.svg"),
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
            Icon::Keyboard => icon!("keyboard.svg"),
            Icon::Headphones => icon!("headphones.svg"),
            Icon::HeadphoneOff => icon!("headphone-off.svg"),
            Icon::Settings => icon!("settings.svg"),
            Icon::Community => icon!("community.svg"),
            Icon::Verified => icon!("verified.svg"),
            Icon::Scallop => icon!("scallop.svg"),
            Icon::CircleInfoFilled => icon!("circle-info-filled.svg"),
            Icon::EllipsisVertical => icon!("ellipsis-vertical.svg"),
            Icon::MessageCirclePlus => icon!("message-circle-plus.svg"),
            Icon::UserCheck => icon!("user-check.svg"),
            Icon::CaptionMin => icon!("caption-min.svg"),
            Icon::CaptionMax => icon!("caption-max.svg"),
            Icon::CaptionRestore => icon!("caption-restore.svg"),
            Icon::CaptionClose => icon!("caption-close.svg"),
            Icon::Id => icon!("id-filled.svg"),
            Icon::PhoneOff => icon!("phone-off.svg"),
            Icon::ScreenShare => icon!("screen-share.svg"),
            Icon::ScreenShareOff => icon!("screen-share-off.svg"),
            Icon::Soundboard => icon!("soundboard.svg"),
            Icon::Palette => icon!("palette.svg"),
            Icon::Accessibility => icon!("accessibility.svg"),
            Icon::Help => icon!("help-circle.svg"),
            Icon::ArrowLeft => icon!("arrow-left.svg"),
            Icon::ArrowRight => icon!("arrow-right.svg"),
            Icon::Minus => icon!("minus.svg"),
            Icon::Restore => icon!("restore.svg"),
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
            Icon::Send => icon!("send-horizontal.svg"),
            Icon::Eyedropper => icon!("eyedropper.svg"),
            Icon::Moon => icon!("moon.svg"),
            Icon::Sun => icon!("sun.svg"),
            Icon::AtSign => icon!("at-sign.svg"),
            Icon::UserMinus => icon!("user-minus.svg"),
            Icon::UserX => icon!("user-x.svg"),
            Icon::UserCircle => icon!("user-circle.svg"),
            Icon::FolderPlus => icon!("folder-plus.svg"),
            Icon::CopyPlus => icon!("copy-plus.svg"),
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

fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, letter) in name.chars().enumerate() {
        if index > 0 && letter.is_ascii_uppercase() {
            out.push('-');
        }
        out.push(letter.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use iced::advanced::image::Handle;

    use super::{Icon, Icons};

    fn pixels(handle: &Handle) -> (u32, u32) {
        match handle {
            Handle::Rgba { width, height, .. } => (*width, *height),
            _ => panic!("icons are drawn as pixels"),
        }
    }

    #[test]
    fn icons_are_drawn_at_the_windows_pixels() {
        let icons = Icons::default();
        assert_eq!(
            pixels(&icons.coloured(Icon::Gamepad, 15, 0x23a55a)),
            (15, 15)
        );
        // At 110%, 15 units span 16.5 pixels, which the renderer rounds to 17.
        icons.set_scale(1.1);
        assert_eq!(icons.drawn.borrow().len(), 0, "the 100% drawings go");
        assert_eq!(
            pixels(&icons.coloured(Icon::Gamepad, 15, 0x23a55a)),
            (17, 17)
        );
        assert_eq!(pixels(&icons.own(Icon::Gamepad, 16)), (18, 18));
        icons.set_scale(1.1);
        assert_eq!(icons.drawn.borrow().len(), 2, "the same scale keeps them");
    }
}
