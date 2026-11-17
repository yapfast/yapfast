//! The interface's icons: the SVGs in `ui/icons`, built in, coloured by the
//! theme. Add an icon by adding its line to the list below. Every SVG of the
//! folder has one variant, named after its file in CamelCase (`volume-2` is
//! `Volume`, `trash-2` is `Trash`); the test `every_file_has_a_variant` fails for a
//! file without one.

use iced::widget::{Svg, svg};

use std::sync::OnceLock;

use anomi_theme::catalog;
use anomi_theme::prelude::*;

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
    /// A handle to draw it with. The handle is made once, on first use, and
    /// shared: a clone is a reference count, with no hashing of the bytes.
    pub fn handle(self) -> svg::Handle {
        static HANDLES: [OnceLock<svg::Handle>; Icon::ALL.len()] =
            [const { OnceLock::new() }; Icon::ALL.len()];
        HANDLES[self as usize]
            .get_or_init(|| svg::Handle::from_memory(self.bytes()))
            .clone()
    }
}

/// An icon at a square size, muted until the pointer is over it (the svg
/// catalog's default); `.style(...)` paints it otherwise. `Verified` keeps its
/// own colours (green with a white check), as in the Slint interface.
pub fn icon<'a>(icon: Icon, size: f32) -> Svg<'a, Theme> {
    let svg = svg(icon.handle()).width(size).height(size);
    if icon == Icon::Verified {
        svg.style(catalog::svg::original)
    } else {
        svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resvg::{tiny_skia, usvg};

    /// Every SVG of `ui/icons` has a variant, and every variant's file exists.
    #[test]
    fn every_file_has_a_variant() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/icons");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".svg").map(str::to_owned)
            })
            .collect();
        files.sort();
        let mut have: Vec<String> = Icon::ALL.iter().map(|i| i.file().to_owned()).collect();
        have.sort();
        assert_eq!(files, have);
    }

    /// Each icon parses and, drawn at 20 px, puts something on the canvas.
    #[test]
    fn every_icon_draws_at_20px() {
        for icon in Icon::ALL {
            let tree = usvg::Tree::from_data(icon.bytes(), &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{}: {e}", icon.file()));
            let size = tree.size();
            let scale = 20.0 / size.width().max(size.height());
            let mut pixmap = tiny_skia::Pixmap::new(20, 20).unwrap();
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            assert!(
                pixmap.pixels().iter().any(|p| p.alpha() > 0),
                "{} draws nothing at 20 px",
                icon.file()
            );
        }
    }
}
