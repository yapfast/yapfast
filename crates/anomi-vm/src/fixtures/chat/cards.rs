//! Samples of the card a custom emoji or a sticker opens for tests and shots,
//! after the fake Discord's data: task C08's.

use iced::{Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::chat::cards::{Cards, Emoji, Origin, Sticker};
use crate::fixtures::pictures::{badge, icon};
use crate::fixtures::shell;
use crate::kind::Expected;
use crate::menu::Spot;
use crate::shell::Layer;

/// The fake's server, as an emoji's or sticker's card names it.
pub fn origin() -> Origin {
    Origin {
        id: "discord:g1".into(),
        name: "Server".into(),
        icon: Expected::Ready(icon("1400000000000000100", "server1")),
        initial: "S".into(),
        online: tr!("{} Online", "3,999"),
        note: tr!("Discoverable Server").into(),
    }
}

/// A server without an icon or counts: its initial on the overlay ground.
pub fn origin_plain() -> Origin {
    Origin {
        id: "discord:g2".into(),
        name: "Filler 2".into(),
        initial: "F".into(),
        ..Origin::default()
    }
}

/// A custom emoji of the fake's server.
pub fn emoji() -> Emoji {
    Emoji {
        name: ":pog:".into(),
        picture: Expected::Ready(badge("1400000000000000200", 48)),
        description: tr!("This emoji is from one of your servers.").into(),
        server: Some(origin()),
        ..Emoji::default()
    }
}

/// A Unicode emoji Twemoji has no picture of: its glyph, no server.
pub fn emoji_unicode() -> Emoji {
    Emoji {
        name: ":sob:".into(),
        glyph: "\u{1f62d}".into(),
        description: tr!("This is a default emoji. It can be used anywhere.").into(),
        ..Emoji::default()
    }
}

/// A custom emoji whose picture hasn't come (its room stays empty), from a
/// server with no icon or counts.
pub fn emoji_late() -> Emoji {
    Emoji {
        name: ":waiting:".into(),
        picture: Expected::Waiting,
        description: tr!("This emoji is from one of your servers.").into(),
        server: Some(origin_plain()),
        ..Emoji::default()
    }
}

/// A sticker of the fake's server.
pub fn sticker() -> Sticker {
    Sticker {
        name: "wave".into(),
        picture: Some(badge("1400000000000000300", 96)),
        description: tr!("This sticker is from one of your servers.").into(),
        server: Some(origin()),
    }
}

/// A default sticker: no server.
pub fn sticker_default() -> Sticker {
    Sticker {
        name: "sob".into(),
        picture: Some(badge("1400000000000000301", 96)),
        description: tr!("This is a default sticker. It can be used anywhere.").into(),
        server: None,
    }
}

pub fn showing_emoji(emoji: Emoji) -> Cards {
    Cards {
        emoji: Some(emoji),
        ..Cards::default()
    }
}

pub fn showing_sticker(sticker: Sticker) -> Cards {
    Cards {
        sticker: Some(sticker),
        ..Cards::default()
    }
}

/// Where a card opens: beside a message's emoji in the timeline.
pub fn beside() -> Spot {
    Spot::beside(Rectangle::new(
        Point::new(420.0, 300.0),
        Size::new(24.0, 24.0),
    ))
}

/// The window with the card over it.
pub fn over(cards: Cards) -> Vm {
    let mut vm = shell::open_at(shell::server_home(), Layer::MessageCard, beside());
    vm.chat.cards = cards;
    vm
}
