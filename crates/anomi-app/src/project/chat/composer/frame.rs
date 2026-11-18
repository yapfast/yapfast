//! The composer's frame (`show_composer`, `src/ui/refresh.rs:3951`): whether
//! the field takes text, the reply bar, the plus and the picker buttons,
//! the hint under it, and what stands in its place where nothing may be
//! sent or a message request waits.

use std::borrow::Cow;

use yapfast_core::application::{AppState, Application, Compose};
use yapfast_core::discord;
use yapfast_core::network::Network;
use yapfast_lang::tr;

use anomi_vm::chat::composer::{Composer, Hint, More, Reply};
use anomi_widgets::composer::{Writing, command_placeholder};

use crate::layout::Layout;
use crate::project::menus::notify::until;

/// Everything of the frame but the draft and what's suggested.
pub(super) fn show(core: &Application, layout: &Layout, composer: &mut Composer) {
    let state = &core.state;
    let room = state.selected_room();
    let discord = room.is_some_and(|room| room.network == Network::Discord);
    // An unverified Matrix device can type; the hint says why it can't send.
    let enabled = state.room_ready
        && !state.busy
        && state.shows_text()
        && room.is_some_and(|room| {
            if discord {
                state.writable(room)
            } else {
                state.preview || room.encrypted
            }
        });
    set(&mut composer.enabled, enabled);
    let reply = state.compose().map(
        |Compose::Reply {
             author, body, ping, ..
         }| Reply {
            label: tr!("Replying to {}: {}", author, body),
            ping: *ping,
        },
    );
    set(&mut composer.reply, reply);
    set(&mut composer.can_spoil, state.can_mark_spoilers());
    let more = More {
        attach: state.can_attach(),
        threads: core.can_start_threads(),
        polls: core.can_create_polls(),
    };
    set(&mut composer.more, more);
    set(&mut composer.gifs, state.can_pick_gifs());
    set(&mut composer.stickers, state.can_pick_stickers());
    set(&mut composer.sending, state.sending.is_some());
    let request = room.filter(|room| room.request).map(|room| {
        if room.category == discord::SPAM {
            tr!("Discord thinks {} may be spam", room.name)
        } else {
            tr!("{} wants to message you", room.name)
        }
    });
    set(&mut composer.request, request);
    let (hint, locked) = hint(state, layout);
    set(&mut composer.hint, hint);
    set(&mut composer.locked, locked);
}

/// What the empty field says: a reply's "Write a reply", the room's
/// "Message #general", or while an app's command is written what Enter
/// does or what its option wants.
pub(super) fn placeholder(state: &AppState) -> Cow<'_, str> {
    if let Some(command) = state.command_draft() {
        let writing = command.options.get(command.focused).map(|option| Writing {
            name: &option.name,
            description: &option.description,
            file: option.is_attachment(),
        });
        return command_placeholder(&command.name, writing);
    }
    if state.compose().is_some() {
        return Cow::Borrowed(tr!("Write a reply"));
    }
    // Discord's words: `Message @sam` in a DM, `Message #general` in a
    // channel, and a group's or thread's own name as it is.
    match state.selected_room() {
        Some(room) => Cow::Owned(
            match (room.direct, room.person.is_some(), room.parent.is_none()) {
                (true, true, _) => tr!("Message @{}", room.name),
                (false, _, true) => tr!("Message #{}", room.name),
                _ => tr!("Message {}", room.name),
            },
        ),
        None => Cow::Borrowed(tr!("Write a message")),
    }
}

/// The line under the frame, and in Discord's look the bar that stands in
/// the frame's place where nothing may be sent here.
fn hint(state: &AppState, layout: &Layout) -> (Hint, Option<String>) {
    let room = state.selected_room();
    let discord = room.is_some_and(|room| room.network == Network::Discord);
    // Without Matrix everything is Discord, so the encryption and key hints
    // say nothing new; what stops sending still shows.
    let discord_only = !cfg!(feature = "matrix");
    let timed_out = room
        .and_then(|room| room.timed_out_until)
        .filter(|_| discord)
        .map(|until_ms| {
            tr!(
                "You're timed out in this server until {}.",
                until(until_ms / 1000, layout.twelve_hour_clock)
            )
        });
    let shut = room.is_some_and(|room| room.read_only || room.locked);
    // Discord swaps the composer for one disabled bar saying so, with no
    // buttons and no caption.
    let no_permission = timed_out.is_none()
        && !state.preview
        && discord
        && room.is_some_and(|room| !room.request)
        && shut;
    let locked = no_permission
        .then(|| tr!("You do not have permission to send messages in this channel.").to_owned());
    let lock = room.is_some_and(|room| room.encrypted) || state.preview;
    let text: Cow<'_, str> = if room.is_none() {
        Cow::Borrowed(tr!("Choose a DM or server channel to write a message."))
    } else if state.preview || no_permission {
        Cow::Borrowed("")
    } else if room.is_some_and(|room| room.request) {
        Cow::Borrowed(if discord_only {
            ""
        } else {
            tr!("Discord · not end-to-end encrypted.")
        })
    } else if let Some(timed_out) = timed_out {
        Cow::Owned(timed_out)
    } else if discord && shut {
        Cow::Borrowed(tr!("You can't send messages in this Discord channel."))
    } else if discord && !state.discord_online {
        Cow::Borrowed(tr!("Discord is disconnected. Yapfast is reconnecting."))
    } else if discord_only {
        Cow::Borrowed("")
    } else if discord {
        Cow::Borrowed(tr!(
            "Discord · not end-to-end encrypted. Enter to send, Shift+Enter for a new line."
        ))
    } else if !state.trusted {
        Cow::Borrowed(tr!("Sending is locked until this device is verified."))
    } else if room.is_some_and(|room| room.encrypted) {
        Cow::Borrowed(tr!(
            "End-to-end encrypted. Enter to send, Shift+Enter for a new line."
        ))
    } else {
        Cow::Borrowed(tr!("Unencrypted room. Reading only in this build."))
    };
    let hint = Hint {
        text: text.into_owned(),
        lock,
    };
    (hint, locked)
}

/// Sets `field` to `value` unless it holds it already.
fn set<T: PartialEq>(field: &mut T, value: T) {
    if *field != value {
        *field = value;
    }
}
