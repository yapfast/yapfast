//! The invite dialog's second page, the link's settings: how long a new
//! link lasts and how many may use it, and Generate a new link, which
//! shows it on the first page; Back goes there as it is
//! (`InviteDialog`, `ui/server.slint:27`).

use iced::widget::Id;

use anomi_theme::prelude::*;
use anomi_vm::places::invite::{Intent, Invite, Page};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use super::choices::choices;
use super::frame::{CANCEL, buttons, dialog, failure, heading};

/// The button that makes a new link.
pub const REMAKE: &str = "invite-remake";

pub fn view(invite: &Invite, width: f32, motion: Motion) -> Element<'_, Intent> {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if !invite.error.is_empty() {
        parts.push(failure(&invite.error));
    }
    parts.push(heading(tr!("EXPIRE AFTER")));
    parts.push(choices(&super::ages(), invite.age, motion, Intent::Age));
    parts.push(heading(tr!("MAX NUMBER OF USES")));
    parts.push(choices(&super::uses(), invite.uses, motion, Intent::Uses));
    let back = action_button(tr!("Back"), Kind::Ghost, motion)
        .id(Id::new(CANCEL))
        .on_press(Intent::Show(Page::People))
        .boxed();
    let remake = action_button(tr!("Generate a new link"), Kind::Primary, motion)
        .id(Id::new(REMAKE))
        .enabled(!invite.making)
        .on_press(invite.remake())
        .boxed();
    parts.push(buttons(vec![back, remake]));
    dialog(
        tr!("Server invite link settings"),
        width,
        Intent::Close,
        motion,
        parts,
    )
}
