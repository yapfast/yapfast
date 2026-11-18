//! A server's Emoji settings: Discord's words, Upload Emoji, and the tables
//! of emoji. Task X10's (docs/iced/tasks/settings.md). The Slint side:
//! `EmojiSection` (`ui/server_expressions.slint:90`); the upload dialog is
//! [`dialog`].

mod dialog;
mod table;

use iced::widget::{column, container, text};
use iced::{Alignment, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::emoji::{Emoji, Intent};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::{tr, trn};

pub use dialog::{NAME, dialog};

const GAP: f32 = 16.0;
const EMPTY: f32 = 8.0;
const EMPTY_TITLE: f32 = 17.0;

pub fn view(emoji: &Emoji, motion: Motion) -> impl View<Intent> + '_ {
    let mut page = column![
        text(trn!(
            "Add up to {n} custom emoji that anyone can use in this server. Animated GIF emojis may be used by members with Discord Nitro. Emoji names must be at least 2 characters long and can only contain alphanumeric characters and underscores. Emoji must be under 256 KB in size.",
            "Add up to {n} custom emojis that anyone can use in this server. Animated GIF emojis may be used by members with Discord Nitro. Emoji names must be at least 2 characters long and can only contain alphanumeric characters and underscores. Emoji must be under 256 KB in size.",
            emoji.slots
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::muted),
    ]
    .spacing(GAP)
    .height(Length::Fill);
    if emoji.may_upload {
        page = page.push(
            action_button(tr!("Upload Emoji"), Kind::Primary, motion)
                .on_press(Intent::Upload)
                .boxed(),
        );
    }
    if emoji.tables.iter().all(|table| table.rows.is_empty()) {
        page.push(empty())
    } else {
        page.push(table::tables(emoji, motion))
    }
}

/// Nothing uploaded: said in the middle of what is left.
fn empty<'a>() -> Element<'a, Intent> {
    container(
        column![
            text(tr!("No Emoji"))
                .size(EMPTY_TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary),
            text(tr!("Get the party started by uploading an emoji"))
                .size(typography::size::BODY)
                .align_x(Alignment::Center)
                .style(catalog::text::secondary),
        ]
        .spacing(EMPTY)
        .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .boxed()
}

/// Its shots for `anomi-shots`: the tables, and an upload over them.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::emoji::{page, uploading};
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| Shot {
        area: "ssettings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![shot("emoji", page), shot("emoji_upload", uploading)]
}
