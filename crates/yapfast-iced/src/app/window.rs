//! What lives only as long as the window: what's open over it, Settings,
//! and the pictures and icons made to draw it. Closing the window to the
//! tray, or the desktop closing it, tears it all down at once
//! (`App::forget_window`), which names every field, so a field added
//! here fails the build until it says what becomes of it.

use crate::{
    menus::Menu, picker::Picker, pictures::Pictures, settings::Settings, slot::Slot,
    tag_card::Shown as TagCard,
};

pub struct Window {
    /// The menu open, if any.
    pub menu: Option<Menu>,
    /// The expression picker, and a server tag's card.
    pub picker: Slot<Picker>,
    pub tag_card: Slot<TagCard>,
    /// Settings while they're open.
    pub settings: Slot<Settings>,
    /// The section Settings last showed, which they open on again from
    /// the account card's cog.
    pub settings_section: crate::settings::Section,
    /// The pictures made to draw, and the moving ones playing.
    pub pictures: Pictures,
    /// The icons drawn, which [`crate::app::App::look`] draws from.
    pub icons: crate::icons::Icons,
}

impl Window {
    pub fn new(
        pictures: Pictures,
        icons: std::collections::HashMap<String, yapfast_iced_kit::icons::Replacement>,
    ) -> Self {
        Self {
            menu: None,
            picker: Slot::default(),
            tag_card: Slot::default(),
            settings: Slot::default(),
            settings_section: crate::settings::Section::Account,
            pictures,
            icons: crate::icons::Icons::new(icons),
        }
    }
}

/// What a window made again keeps of the last: Settings stay open as they
/// are when the title bar's switch, which is in them, makes the window
/// again.
#[derive(Clone, Copy, Default)]
pub struct Keep {
    pub settings: bool,
}
