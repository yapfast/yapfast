//! The interface's icons: the SVGs in `ui/icons`, built in, coloured by the
//! theme. Add an icon by adding its variant and file here.

use iced::widget::{Svg, svg};

use anomi_theme::Theme;

/// An icon of `ui/icons`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Hash,
    Headphones,
    HeadphoneOff,
    Inbox,
    MessageSquareText,
    Mic,
    MicOff,
    Plus,
    Search,
    Settings,
    SquarePen,
    Thread,
    UserPlus,
    Users,
    Volume,
}

impl Icon {
    /// The SVG's bytes.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Hash => include_bytes!("../../../ui/icons/hash.svg"),
            Self::Headphones => include_bytes!("../../../ui/icons/headphones.svg"),
            Self::HeadphoneOff => include_bytes!("../../../ui/icons/headphone-off.svg"),
            Self::Inbox => include_bytes!("../../../ui/icons/inbox.svg"),
            Self::MessageSquareText => {
                include_bytes!("../../../ui/icons/message-square-text.svg")
            }
            Self::Mic => include_bytes!("../../../ui/icons/mic.svg"),
            Self::MicOff => include_bytes!("../../../ui/icons/mic-off.svg"),
            Self::Plus => include_bytes!("../../../ui/icons/plus.svg"),
            Self::Search => include_bytes!("../../../ui/icons/search.svg"),
            Self::Settings => include_bytes!("../../../ui/icons/settings.svg"),
            Self::SquarePen => include_bytes!("../../../ui/icons/square-pen.svg"),
            Self::Thread => include_bytes!("../../../ui/icons/thread.svg"),
            Self::UserPlus => include_bytes!("../../../ui/icons/user-plus.svg"),
            Self::Users => include_bytes!("../../../ui/icons/users.svg"),
            Self::Volume => include_bytes!("../../../ui/icons/volume-2.svg"),
        }
    }

    /// A handle to draw it with. The bytes are borrowed, not copied.
    pub fn handle(self) -> svg::Handle {
        svg::Handle::from_memory(self.bytes())
    }
}

/// An icon at a square size, muted until the pointer is over it (the svg
/// catalog's default); `.style(...)` paints it otherwise.
pub fn icon<'a>(icon: Icon, size: f32) -> Svg<'a, Theme> {
    svg(icon.handle()).width(size).height(size)
}
