//! The interface's icons: the SVGs in `ui/icons`, built in. Add an icon by
//! adding its line to the list below. Every SVG of the folder has one
//! variant, named after its file in CamelCase (`volume-2` is `Volume`,
//! `trash-2` is `Trash`); `anomi_widgets::icon`'s test
//! `every_file_has_a_variant` fails for a file without one.
//!
//! [`Icon`] lives here, with the view-models, so a view-model holds the icon
//! a row shows (a menu row, a settings row, a channel's kind) as an `Icon`
//! rather than a name: `anomi_widgets::icon::icon(icon, size)` draws it,
//! coloured by the theme. Task F04.

use std::sync::OnceLock;

use iced::widget::svg;

/// Declares [`Icon`], its list and its file names from one table.
macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// An icon of `ui/icons`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            /// Every icon, in file-name order.
            pub const ALL: &[Icon] = &[$(Icon::$variant,)*];

            /// The SVG's file name in `ui/icons`, without its extension.
            pub fn file(self) -> &'static str {
                match self {
                    $(Self::$variant => $file,)*
                }
            }

            /// The SVG's bytes, built into the binary.
            pub fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!("../../../ui/icons/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    ArrowLeft => "arrow-left",
    ArrowRight => "arrow-right",
    ArrowUpRight => "arrow-up-right",
    AtSign => "at-sign",
    Ban => "ban",
    Battledotnet => "battledotnet",
    Bell => "bell",
    BellOff => "bell-off",
    Bluesky => "bluesky",
    Brush => "brush",
    Bungie => "bungie",
    Calendar => "calendar",
    ChartBar => "chart-bar",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    ChevronsRight => "chevrons-right",
    CirclePlus => "circle-plus",
    CircleStop => "circle-stop",
    ClipboardPaste => "clipboard-paste",
    Clock => "clock",
    Community => "community",
    Copy => "copy",
    CopyPlus => "copy-plus",
    Crown => "crown",
    Crunchyroll => "crunchyroll",
    Discord => "discord",
    Download => "download",
    Ebay => "ebay",
    Ellipsis => "ellipsis",
    Epicgames => "epicgames",
    ExternalLink => "external-link",
    Eye => "eye",
    EyeOff => "eye-off",
    Eyedropper => "eyedropper",
    Facebook => "facebook",
    File => "file",
    Folder => "folder",
    FolderPlus => "folder-plus",
    Forward => "forward",
    Gamepad => "gamepad",
    Gavel => "gavel",
    Gem => "gem",
    Gif => "gif",
    Github => "github",
    Globe => "globe",
    Hand => "hand",
    Hash => "hash",
    HashLock => "hash-lock",
    HashWarning => "hash-warning",
    HeadphoneOff => "headphone-off",
    Headphones => "headphones",
    HelpCircle => "help-circle",
    History => "history",
    IdFilled => "id-filled",
    Image => "image",
    ImageLock => "image-lock",
    ImageWarning => "image-warning",
    Inbox => "inbox",
    Instagram => "instagram",
    Keyboard => "keyboard",
    Leagueoflegends => "leagueoflegends",
    Link => "link",
    Loader => "loader",
    Lock => "lock",
    LogOut => "log-out",
    Mastodon => "mastodon",
    Maximize => "maximize",
    Megaphone => "megaphone",
    MegaphoneLock => "megaphone-lock",
    MegaphoneWarning => "megaphone-warning",
    MessageCircle => "message-circle",
    MessageSquareText => "message-square-text",
    MessageSquareTextLock => "message-square-text-lock",
    MessageSquareTextWarning => "message-square-text-warning",
    Mic => "mic",
    MicOff => "mic-off",
    Minimize => "minimize",
    Minus => "minus",
    Moon => "moon",
    Music => "music",
    NoiseBars => "noise-bars",
    NoiseBarsOff => "noise-bars-off",
    Palette => "palette",
    Pause => "pause",
    Paypal => "paypal",
    Pen => "pen",
    Pencil => "pencil",
    Phone => "phone",
    PhoneOff => "phone-off",
    Pin => "pin",
    PinOff => "pin-off",
    Play => "play",
    Playstation => "playstation",
    Plus => "plus",
    Reddit => "reddit",
    RefreshCw => "refresh-cw",
    Reply => "reply",
    Restore => "restore",
    Riotgames => "riotgames",
    Roblox => "roblox",
    Rules => "rules",
    Samsung => "samsung",
    Scissors => "scissors",
    ScreenShare => "screen-share",
    ScreenShareOff => "screen-share-off",
    Search => "search",
    SendHorizontal => "send-horizontal",
    Settings => "settings",
    Shield => "shield",
    Shop => "shop",
    SignalHigh => "signal-high",
    SignalLow => "signal-low",
    SignalMedium => "signal-medium",
    Slash => "slash",
    Smile => "smile",
    SmilePlus => "smile-plus",
    Soundboard => "soundboard",
    Soundcloud => "soundcloud",
    Spotify => "spotify",
    Square => "square",
    SquarePen => "square-pen",
    Stage => "stage",
    StageLock => "stage-lock",
    Star => "star",
    StarFilled => "star-filled",
    Steam => "steam",
    Sticker => "sticker",
    StickyNote => "sticky-note",
    Sun => "sun",
    TextSelect => "text-select",
    Thread => "thread",
    Tiktok => "tiktok",
    Trash => "trash-2",
    TrendingUp => "trending-up",
    TriangleAlert => "triangle-alert",
    Trophy => "trophy",
    Tv => "tv",
    Twitch => "twitch",
    User => "user",
    UserCheck => "user-check",
    UserMinus => "user-minus",
    UserPen => "user-pen",
    UserPlus => "user-plus",
    UserX => "user-x",
    Users => "users",
    Verified => "verified",
    Video => "video",
    VideoOff => "video-off",
    Volume => "volume-2",
    VolumeBold => "volume-2-bold",
    VolumeLock => "volume-2-lock",
    VolumeWarning => "volume-2-warning",
    VolumeX => "volume-x",
    X => "x",
    XLogo => "x-logo",
    Youtube => "youtube",
}

impl Icon {
    /// A handle to draw it with (`anomi_widgets::icon::icon` draws one). The handle is made once, on first use, and
    /// shared: a clone is a reference count, with no hashing of the bytes.
    pub fn handle(self) -> svg::Handle {
        static HANDLES: [OnceLock<svg::Handle>; Icon::ALL.len()] =
            [const { OnceLock::new() }; Icon::ALL.len()];
        HANDLES[self as usize]
            .get_or_init(|| svg::Handle::from_memory(self.bytes()))
            .clone()
    }
}
