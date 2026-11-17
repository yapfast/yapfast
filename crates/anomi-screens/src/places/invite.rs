//! A server's or channel's invite dialog: the link with a button to copy it,
//! and how long a new one lasts and how many may use it. The Slint side:
//! `InviteDialog` (`ui/server.slint:27`).

mod choices;
pub(crate) mod frame;

use iced::widget::Id;

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::places::invite::{Intent, Invite};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use choices::{choices, natural};
use frame::{CANCEL, PADDING, buttons, copyable, dialog, failure, heading, note};

/// The button that makes a new link.
pub const REMAKE: &str = "invite-remake";

/// The card's width at least (`:44`).
const WIDTH: f32 = 460.0;

pub fn view(invite: &Invite, motion: Motion) -> impl View<Intent> + '_ {
    let ages = [
        tr!("30 min"),
        tr!("1 hr"),
        tr!("6 hr"),
        tr!("12 hr"),
        tr!("1 day"),
        tr!("7 days"),
        tr!("Never"),
    ];
    let uses = [tr!("No limit"), "1", "5", "10", "25", "50", "100"];
    // The card fits itself to its troughs, inside its padding (`:44`).
    let width = WIDTH.max(natural(&ages).max(natural(&uses)) + 2.0 * PADDING);
    let link: Element<'_, Intent> = if invite.making {
        frame::line(tr!("Making a link..."), typography::size::BODY, true)
    } else {
        frame::line(&invite.link, typography::size::BODY, false)
    };
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if !invite.detail.is_empty() {
        parts.push(note(&invite.detail));
    }
    parts.push(heading(tr!("INVITE LINK")));
    parts.push(copyable(
        link,
        true,
        invite.copied,
        !invite.link.is_empty(),
        Intent::Copy(invite.link.clone()),
        motion,
    ));
    if !invite.note.is_empty() {
        parts.push(note(&invite.note));
    }
    if !invite.error.is_empty() {
        parts.push(failure(&invite.error));
    }
    parts.push(heading(tr!("EXPIRE AFTER")));
    parts.push(choices(&ages, invite.age, motion, Intent::Age));
    parts.push(heading(tr!("MAX NUMBER OF USES")));
    parts.push(choices(&uses, invite.uses, motion, Intent::Uses));
    let done = action_button(tr!("Done"), Kind::Ghost, motion)
        .id(Id::new(CANCEL))
        .on_press(Intent::Close)
        .boxed();
    let remake = action_button(tr!("Generate a new link"), Kind::Secondary, motion)
        .id(Id::new(REMAKE))
        .enabled(!invite.making)
        .on_press(invite.remake())
        .boxed();
    parts.push(buttons(vec![done, remake]));
    dialog(&invite.title, width, Intent::Close, motion, parts)
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::places::invite;
    vec![Shot {
        area: "dialog",
        name: "invite_to_server",
        size: GOLDEN,
        theme: "bonfire",
        vm: invite::made,
    }]
}
