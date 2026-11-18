//! Samples of the profile editor's preview and colours for tests and shots,
//! after the fake Discord's data: task P15's. Alice's tabs as
//! `profile/editor.png` shows them: the board, her activity and her
//! wishlist, and the popover on her banner's tan.

use iced::Color;

use crate::fixtures::people::activity as activities;
use crate::fixtures::pictures;
use crate::kind::Expected;
use crate::people::editor_preview::{Colour, EditorPreview, Page};
use crate::people::full_profile::{Game, Widget, Wish};
use crate::people::profile::Theme;

fn picture(path: &str) -> Expected {
    Expected::Ready(pictures::picture(path))
}

/// Nothing on any tab yet, the Board shown: Yapfast's words.
pub fn empty() -> EditorPreview {
    EditorPreview::default()
}

/// The tabs with something on each, on `page`.
pub fn on(page: Page) -> EditorPreview {
    EditorPreview {
        page,
        activities: vec![activities::game(), activities::song()],
        now: activities::NOW,
        widgets: vec![Widget {
            title: "Games I play".into(),
            games: vec![
                Game {
                    name: "Factorio".into(),
                    initial: "F".into(),
                    icon: picture("/app-icons/steam.png"),
                    comment: "Building the factory".into(),
                },
                Game {
                    name: "Celeste".into(),
                    initial: "C".into(),
                    icon: Expected::None,
                    comment: String::new(),
                },
            ],
            elsewhere: false,
        }],
        wishlist: ["Hades II", "Hollow Knight: Silksong", "Balatro", "Tunic"]
            .into_iter()
            .map(|name| Wish {
                name: name.into(),
                initial: name[..1].into(),
                picture: Expected::None,
            })
            .collect(),
        ..EditorPreview::default()
    }
}

/// The same under a dark profile theme.
pub fn themed(page: Page) -> EditorPreview {
    EditorPreview {
        theme: Some(Theme::new([
            Color::from_rgb8(0x1f, 0x3a, 0x5f),
            Color::from_rgb8(0x6b, 0x2d, 0x5c),
        ])),
        ..on(page)
    }
}

/// The popover opened on the banner's tan.
pub fn colour() -> Colour {
    Colour::start("#b8a587")
}
