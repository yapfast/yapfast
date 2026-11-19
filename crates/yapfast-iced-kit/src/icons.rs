//! The client's icons, from the kit's `icons/`, and the pictures drawn of
//! them: each at a size and colour drawn once and kept until the window
//! goes or the theme changes.

use std::{cell::RefCell, collections::HashMap};

use iced::advanced::image::Handle;

use crate::canvas::{self, handle};

/// Icons drawn, by icon, size and colour; [`Look`](crate::look::Look)
/// draws from it.
#[derive(Default)]
pub struct Icons {
    drawn: RefCell<HashMap<Drawn, Handle>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Drawn {
    /// In one colour.
    Coloured(Icon, u32, u32),
    /// In its own colours, such as the verified badge's green and white.
    Own(Icon, u32),
}

impl Icons {
    /// `icon` at `size` in `rgb`, drawn once.
    pub fn coloured(&self, icon: Icon, size: u32, rgb: u32) -> Handle {
        self.get(Drawn::Coloured(icon, size, rgb))
    }

    /// `icon` at `size` in its own colours, drawn once.
    pub fn own(&self, icon: Icon, size: u32) -> Handle {
        self.get(Drawn::Own(icon, size))
    }

    fn get(&self, drawn: Drawn) -> Handle {
        if let Some(handle) = self.drawn.borrow().get(&drawn) {
            return handle.clone();
        }
        let canvas = match drawn {
            Drawn::Coloured(icon, size, rgb) => canvas::svg_icon(icon.svg(), size, rgb),
            Drawn::Own(icon, size) => canvas::svg_drawn(icon.svg(), size),
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
    FolderPlus,
    CopyPlus,
}

impl Icon {
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
            Icon::ArrowsUpDown => icon!("arrows-up-down.svg"),
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
