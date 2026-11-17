//! Samples of a message for tests and shots, after the fake Discord's data:
//! Bob's numbered messages in #general, a minute apart; the showcase's
//! reply to the welcome, a blocked person's message and Bob's link; the
//! call in Bob's DM; and one of each other kind (a notice, a forward, an
//! app's answer, a long body, a mention, one on its way, one only this
//! account sees). [`timelines`] puts them in the timeline as the golden
//! screenshots show it. Task C02's.

mod timelines;

use iced::Color;
use iced::widget::image;
use yapfast_lang::{tr, trn};

use crate::chat::message::{
    Author, Bar, Command, Delivery, Fold, Header, May, Message, Notice, Quick, Reply, Thinking,
    Thread,
};
use crate::chat::timeline::group::key;
use crate::fixtures::kind::bob_tag;
use crate::fixtures::pictures::{self, pictured};
use crate::fixtures::text::{self, MODS};
use crate::fixtures::{avatar, chat::timeline::message_key};
use crate::kind::{Expected, NameStyle, Tag};
use crate::text::{Builder, Ink, Link, Source, Style, Text};

pub use timelines::{
    blocked_showcase, call_in_dm, confirming, gallery, history, history_lit, kinds, kinds_compact,
    shift_held,
};

/// Bob, as #general shows him: his picture with its ring, the Mods' blue,
/// his name's face, his server's tag.
pub fn bob() -> Author {
    Author {
        name: "Bob".into(),
        avatar: pictured(avatar("B", 0xc2554d), "Bob"),
        decoration: Some(ring()),
        color: Some(MODS),
        style: NameStyle::new(14, 3, &[0x00ff99]).map(NameStyle::font_only),
        tag: Some(tag()),
        bot: false,
        known: true,
    }
}

/// Bob's server tag with its badge, as the fake serves it.
fn tag() -> Tag {
    Tag {
        badge: Expected::Ready(pictures::picture(
            "/clan-badges/1400000000000000100/bobtag.png",
        )),
        ..bob_tag()
    }
}

/// Bob in his DM, as the golden shows him: no role's colour; his tag.
pub fn bob_in_dm() -> Author {
    Author {
        color: None,
        ..bob()
    }
}

/// #general's message `index`, Bob's, sent at 16:(index + 4) on
/// 1 October 2026; `first` when it starts a group.
pub fn bobs(index: usize, first: bool) -> Message {
    let minute = index + 4;
    Message {
        key: message_key(index),
        first,
        author: bob(),
        time: format!("01.10.2026 16:{minute:02}"),
        short_time: format!("16:{minute:02}"),
        body: text::history(index),
        ..Message::default()
    }
}

/// The hover bar on someone else's message: the device's three frequent
/// reactions, the picker, Reply, Forward and More.
pub fn bar() -> Bar {
    let quick = |glyph: &str, name: &str| Quick {
        key: glyph.into(),
        name: format!(":{name}:"),
        picture: None,
    };
    Bar {
        frequent: vec![
            quick("👍", "thumbsup"),
            quick("❤️", "heart"),
            quick("😂", "joy"),
        ],
        may: May {
            react: true,
            reply: true,
            forward: true,
            pin: true,
            thread: true,
            ..May::default()
        },
        ..Bar::default()
    }
}

/// `message` under the pointer, its bar over it.
pub fn lit(mut message: Message) -> Message {
    message.lit = true;
    message.bar = Some(bar());
    message
}

/// Bob's answer to the welcome, quoting it (`postShowcase`).
pub fn reply() -> Message {
    Message {
        key: key("reply"),
        first: true,
        author: bob(),
        time: "01:10".into(),
        short_time: "01:10".into(),
        header: Header::Reply(Box::new(Reply {
            author: "Bob".into(),
            avatar: bob().avatar,
            color: Some(MODS),
            body: "Welcome to Yapfast, with code and a link".into(),
            jumps: true,
        })),
        body: markdown("As the welcome said"),
        ..Message::default()
    }
}

/// A blocked person's message, as the projection hides it until asked
/// (`src/ui/messages.rs:885`): "Blocked user" on a grey "?", and "Show it"
/// linking to it.
pub fn blocked() -> Message {
    let mut body = Builder::new();
    let italic = Style {
        italic: true,
        ..Style::default()
    };
    body.text(tr!("Blocked message."), italic);
    body.text(" ", Style::default());
    body.link(
        tr!("Show it"),
        Style::default(),
        Link::Url("yapfast:blocked/blocked".into()),
        None,
    );
    Message {
        key: key("blocked"),
        first: true,
        author: Author {
            name: tr!("Blocked user").into(),
            avatar: avatar("?", 0x747f8d),
            ..Author::default()
        },
        time: "01:10".into(),
        short_time: "01:10".into(),
        body: body.finish(),
        ..Message::default()
    }
}

/// Bob's link to the article, which its preview follows.
pub fn article() -> Message {
    Message {
        key: key("article"),
        first: true,
        author: bob(),
        time: "01:10".into(),
        short_time: "01:10".into(),
        body: text::article(),
        ..Message::default()
    }
}

/// Bob's call in his DM, as a notice: the phone in green, "Bob started a
/// call." with when.
pub fn call() -> Message {
    let mut body = Builder::new();
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    body.link("Bob", bold, Link::Author("bob".into()), Some(Ink::Text));
    body.text(" ", Style::default());
    body.text(tr!("started a call."), Style::default());
    body.colored("  01:21", Style::default(), Ink::Muted);
    Message {
        key: key("call"),
        first: true,
        author: bob_in_dm(),
        notice: Some(Notice::Call),
        time: "01:21".into(),
        short_time: "01:21".into(),
        body: body.finish(),
        ..Message::default()
    }
}

/// Bob's notice that he pinned the welcome.
pub fn pinned() -> Message {
    Message {
        notice: Some(Notice::Pin),
        body: text::pinned(),
        key: key("pinned"),
        ..call()
    }
}

/// Erin's forward of Bob's message.
pub fn forwarded() -> Message {
    Message {
        key: key("forwarded"),
        first: true,
        author: erin(),
        time: "01:12".into(),
        short_time: "01:12".into(),
        forwarded: true,
        body: text::welcome(),
        ..Message::default()
    }
}

/// The status bot's answer to Alice's `/status`, only she sees, edited.
pub fn command() -> Message {
    Message {
        key: key("command"),
        first: true,
        author: Author {
            name: "Status".into(),
            avatar: avatar("S", 0x5865f2),
            bot: true,
            known: true,
            ..Author::default()
        },
        time: "01:13".into(),
        short_time: "01:13".into(),
        header: Header::Command(Box::new(Command {
            user: "Alice".into(),
            avatar: pictured(avatar("A", 0x9c8a6a), "Alice"),
            color: None,
            known: true,
            name: "/status".into(),
            usable: true,
            target: Some("Bob".into()),
        })),
        body: markdown("All systems normal"),
        edited: true,
        ephemeral: true,
        ..Message::default()
    }
}

/// The status bot thinking over Alice's command, or given up on.
pub fn thinking(timed_out: bool) -> Message {
    Message {
        key: key("thinking"),
        body: Text::default(),
        thinking: Some(if timed_out {
            Thinking::TimedOut
        } else {
            Thinking::Waiting
        }),
        edited: false,
        ephemeral: false,
        ..command()
    }
}

/// A body of 30 lines, folded or opened.
pub fn long(open: bool) -> Message {
    Message {
        key: key("long"),
        body: text::long(30),
        fold: if open { Fold::Open } else { Fold::Folded },
        ..bobs(20, true)
    }
}

/// Bob mentioning Alice, this account: it lies on marigold.
pub fn mention() -> Message {
    Message {
        key: key("mention"),
        body: text::mentions(),
        highlighted: true,
        ..bobs(21, true)
    }
}

/// Alice's message that didn't go, with Retry and Cancel.
pub fn failed() -> Message {
    Message {
        key: key("failed"),
        first: true,
        author: alice(),
        time: "01:14".into(),
        short_time: "01:14".into(),
        body: markdown("See you at nine"),
        delivery: Some(Delivery {
            text: tr!("Not sent. The server refused this message.").into(),
            failed: true,
            retry: true,
            cancel: true,
            quiet: false,
        }),
        ..Message::default()
    }
}

/// Bob's message that started the Plans thread.
pub fn with_thread() -> Message {
    Message {
        key: key("thread"),
        body: markdown("Let's plan the weekend"),
        thread: Some(Thread {
            name: "Plans".into(),
            detail: trn!("{n} message", "{n} messages", 3),
        }),
        ..bobs(22, true)
    }
}

/// `message` in the compact display.
pub fn compact(mut message: Message) -> Message {
    message.compact = true;
    message
}

fn alice() -> Author {
    Author {
        name: "Alice".into(),
        avatar: pictured(avatar("A", 0x9c8a6a), "Alice"),
        decoration: Some(ring()),
        known: true,
        ..Author::default()
    }
}

fn erin() -> Author {
    Author {
        name: "Erin".into(),
        avatar: pictured(avatar("E", 0x3a9357), "Erin"),
        color: Some(Color::from_rgb8(0x2e, 0xcc, 0x71)),
        known: true,
        ..Author::default()
    }
}

fn markdown(source: &str) -> Text {
    crate::text::markdown(Source::new(source))
}

/// A pink ring for a decoration, 96 px wide, which the avatar sits inside.
/// Made once a thread, so its handle keeps its id.
fn ring() -> image::Handle {
    thread_local! {
        static RING: image::Handle = {
            const SIDE: u32 = 96;
            let middle = SIDE as f32 / 2.0;
            let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let distance = (x as f32 + 0.5 - middle).hypot(y as f32 + 0.5 - middle);
                    let alpha =
                        (47.0 - distance).clamp(0.0, 1.0) * (distance - 41.0).clamp(0.0, 1.0);
                    pixels.extend([255, 105, 180, (alpha * 255.0) as u8]);
                }
            }
            image::Handle::from_rgba(SIDE, SIDE, pixels)
        };
    }
    RING.with(image::Handle::clone)
}
