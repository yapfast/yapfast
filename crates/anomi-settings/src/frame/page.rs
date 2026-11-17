//! What each page of the person's settings is called, which icon its row
//! has, and which view draws it. The Slint side: the `page-title` and
//! `rows` of `SettingsScreen` (`ui/settings.slint:603`) and the entries of
//! `SettingsNavigation` (`ui/yapfast/settings-nav.slint:55`), with the
//! children `YapfastWindow` gives it (`ui/yapfast.slint:4703`).

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::settings::{self, Settings, frame::Page};
use yapfast_lang::tr;

use crate::{
    about, account, advanced, appearance, discord, games, keybinds, messages, notifications,
    sounds, voice,
};

/// The page's name in the sections and as the bar's title.
pub fn label(page: Page) -> &'static str {
    match page {
        Page::Account => tr!("My Account"),
        Page::Voice => tr!("Voice & Video"),
        Page::Messages => tr!("Messages"),
        Page::Notifications => tr!("Notifications"),
        Page::Sounds => tr!("Sounds"),
        Page::Appearance => tr!("Appearance"),
        Page::Discord => "Discord",
        Page::Games => tr!("Game Activity"),
        Page::Keybinds => tr!("Keybinds"),
        Page::Advanced => tr!("Advanced"),
        Page::About => tr!("About"),
    }
}

/// The page's icon in the sections.
pub fn icon(page: Page) -> Icon {
    match page {
        Page::Account => Icon::User,
        Page::Voice => Icon::Mic,
        Page::Messages => Icon::MessageCircle,
        Page::Notifications => Icon::Bell,
        Page::Sounds => Icon::Volume,
        Page::Appearance => Icon::Palette,
        Page::Discord => Icon::Discord,
        Page::Games => Icon::Play,
        Page::Keybinds => Icon::Keyboard,
        Page::Advanced => Icon::Settings,
        Page::About => Icon::HelpCircle,
    }
}

/// The page the frame shows, from its own module's view.
pub fn body<'a>(settings: &'a Settings, motion: Motion) -> Element<'a, settings::Intent> {
    use settings::Intent;
    match settings.frame.page {
        Page::Account => account::view(&settings.account, motion)
            .map(Intent::from)
            .boxed(),
        Page::Voice => voice::view(&settings.voice, motion)
            .map(Intent::from)
            .boxed(),
        Page::Messages => messages::view(&settings.messages, motion)
            .map(Intent::from)
            .boxed(),
        Page::Notifications => notifications::view(&settings.notifications, motion)
            .map(Intent::from)
            .boxed(),
        Page::Sounds => sounds::view(&settings.sounds, motion)
            .map(Intent::from)
            .boxed(),
        Page::Appearance => appearance::view(&settings.appearance, motion).boxed(),
        Page::Discord => discord::view(&settings.discord, motion)
            .map(Intent::from)
            .boxed(),
        Page::Games => games::view(&settings.games, motion)
            .map(Intent::from)
            .boxed(),
        Page::Keybinds => keybinds::view(&settings.keybinds, motion)
            .map(Intent::from)
            .boxed(),
        Page::Advanced => advanced::view(&settings.advanced, motion)
            .map(Intent::from)
            .boxed(),
        Page::About => about::view(&settings.about, motion)
            .map(Intent::from)
            .boxed(),
    }
}
