//! The card an emoji or a sticker opens (`show_emoji_card`,
//! `show_sticker_card`, `src/ui/refresh.rs:759`, `:948`), as the core's
//! `yapfast_core::emoji::card` works it out: what the emoji or sticker is
//! and, for a server's, the server it is from with how many are online
//! there once Discord said. It is made again while it shows, so the count
//! comes in when Discord answers, and goes when its message does or the
//! room changes, so that it never holds the keys unseen.

use yapfast_core::application::Application;
use yapfast_core::emoji::card::{self, Card, CardServer, Target};

use anomi_vm::chat::cards::{Cards, Emoji, Origin, Sticker};
use anomi_vm::kind::Expected;
use anomi_vm::shell::{Layer, Shell};

use crate::pictures::{self, Requests};
use crate::project::rows;

/// What the open card is of, and the room it opened in.
#[derive(Default)]
pub(crate) struct Shown {
    pub(crate) subject: Option<Subject>,
    pub(crate) room: Option<String>,
}

/// What a card is of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Subject {
    Emoji(Target),
    /// A message's sticker: its item and the attachment's place.
    Sticker(String, usize),
}

/// The core's card of `subject`; none once its message went.
pub(crate) fn of(core: &Application, subject: &Subject) -> Option<Card> {
    match subject {
        Subject::Emoji(target) => Some(card::emoji_for(core, target)),
        Subject::Sticker(item, index) => card::sticker(&core.state, item, *index),
    }
}

/// Shows the open card again, or closes it once its subject went or the
/// room changed.
pub(crate) fn show(
    core: &Application,
    shell: &mut Shell,
    cards: &mut Cards,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let Some(subject) = &shown.subject else {
        return;
    };
    let state = &core.state;
    let card = of(core, subject).filter(|_| shown.room == state.selected);
    let Some(card) = card else {
        shown.subject = None;
        shell.close(Layer::MessageCard);
        return;
    };
    let origin = card
        .server
        .as_ref()
        .map(|server| origin(state, server, requests));
    let made = match subject {
        Subject::Emoji(target) => Cards {
            emoji: Some(emoji(state, target, card, origin, requests)),
            sticker: None,
        },
        Subject::Sticker(item, index) => Cards {
            emoji: None,
            sticker: Some(Sticker {
                picture: state
                    .message(item)
                    .and_then(|message| message.attachments.get(*index))
                    .and_then(|attachment| attachment.preview.as_ref())
                    .and_then(pictures::shown),
                name: card.name,
                description: card.description.to_owned(),
                server: origin,
            }),
        },
    };
    if *cards != made {
        *cards = made;
    }
}

/// An emoji's card: Unicode's shows its glyph, a server's its picture,
/// asked for while it hasn't come.
fn emoji(
    state: &yapfast_core::application::AppState,
    target: &Target,
    card: Card,
    server: Option<Origin>,
    requests: &mut Requests,
) -> Emoji {
    let glyph = match target {
        Target::Unicode(emoji) => emoji.clone(),
        Target::Custom(_) => String::new(),
    };
    let picture = match card.picture.as_deref() {
        Some(address) => {
            requests.want(state, [address]);
            pictures::find(state, Some(address)).map_or(Expected::Waiting, Expected::Ready)
        }
        None => Expected::None,
    };
    Emoji {
        name: card.name,
        glyph,
        picture,
        description: card.description.to_owned(),
        server,
    }
}

/// The server a card names, its icon asked for while it hasn't come.
fn origin(
    state: &yapfast_core::application::AppState,
    server: &CardServer,
    requests: &mut Requests,
) -> Origin {
    let icon = match server.icon.as_deref() {
        Some(address) => {
            requests.want(state, [address]);
            pictures::find(state, Some(address)).map_or(Expected::Waiting, Expected::Ready)
        }
        None => Expected::None,
    };
    Origin {
        id: server.id.clone(),
        name: server.name.clone(),
        icon,
        initial: rows::initial(&server.name),
        online: server.online.clone(),
        note: server.note.to_owned(),
    }
}
