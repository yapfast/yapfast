//! The Game activity settings page: whether the account shares the game it
//! plays, the custom games named by their executable, and the form that adds
//! or changes one. The Slint side: `GameActivitySettings`
//! (`ui/settings.slint:2134`), answered by `src/ui/games.rs`.

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{column, row, text};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::games::{Games, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

use crate::kit::{heading, switch};

/// Between the page's parts (`16px`), inside a group and a game (`8px`,
/// `4px`) and between a game's buttons (`8px`).
const SPACING: f32 = 16.0;
const GROUP: f32 = 8.0;
const LINES: f32 = 4.0;

pub fn view(page: &Games, motion: Motion) -> impl View<Intent> + '_ {
    column![
        switch(tr!("Share the game you play"), page.shown, motion)
            .description(tr!(
                "Yapfast looks for games among the programs running, and on Linux takes the Rich Presence games send. Your Discord account keeps this setting, so it's the same in Discord's apps."
            ))
            .enabled(page.linked)
            .locked(if page.linked { "" } else { tr!("Link a Discord account first.") })
            .on_toggle(Intent::Share)
            .view(),
        custom(page, motion),
        form(page, motion),
    ]
    .spacing(SPACING)
    .width(Length::Fill)
}

/// The games named so far, each with Edit and Remove.
fn custom(page: &Games, motion: Motion) -> Element<'_, Intent> {
    let mut list = column![
        heading(tr!("CUSTOM GAMES")),
        typography::label(tr!(
            "Name a game by its executable, a file name or a full path, and what to show after Playing. These come before the games Yapfast knows, but a game's Rich Presence comes first."
        ))
        .width(Length::Fill),
    ]
    .spacing(GROUP)
    .width(Length::Fill);
    for (index, game) in page.games.iter().enumerate() {
        list = list.push(
            column![
                row![
                    text(game.text.as_str())
                        .size(typography::size::BODY)
                        .style(catalog::text::primary)
                        .wrapping(Wrapping::None)
                        .ellipsis(Ellipsis::End)
                        .width(Length::Fill),
                    action_button(tr!("Edit"), Kind::Secondary, motion)
                        .small()
                        .on_press(Intent::Edit(index)),
                    action_button(tr!("Remove"), Kind::Secondary, motion)
                        .small()
                        .on_press(Intent::Remove(index)),
                ]
                .spacing(GROUP)
                .align_y(alignment::Vertical::Center),
                text(game.executable.as_str())
                    .size(typography::size::CAPTION)
                    .style(catalog::text::muted),
            ]
            .spacing(LINES)
            .boxed(),
        );
    }
    list.boxed()
}

/// The two fields and the buttons: Add game, or Save game and Cancel while
/// one is being edited.
fn form(page: &Games, motion: Motion) -> Element<'_, Intent> {
    let mut form = column![
        field(
            tr!("Executable name or full path"),
            page.executable_draft.as_str()
        )
        .motion(motion)
        .on_input(Intent::Executable)
        .view(),
        field(
            tr!("Text after Playing (up to 128 characters)"),
            page.text_draft.as_str()
        )
        .motion(motion)
        .on_input(Intent::Text)
        .view(),
    ]
    .spacing(GROUP);
    if page.invalid {
        form = form.push(
            typography::label(tr!(
                "Enter an executable and a name of up to 128 characters, without line breaks."
            ))
            .width(Length::Fill)
            .boxed(),
        );
    }
    let ready = !page.executable_draft.is_empty() && !page.text_draft.is_empty();
    let mut buttons = row![
        action_button(
            if page.editing.is_none() {
                tr!("Add game")
            } else {
                tr!("Save game")
            },
            Kind::Secondary,
            motion
        )
        .enabled(ready)
        .on_press(Intent::Save),
    ]
    .spacing(GROUP);
    if page.editing.is_some() {
        buttons = buttons.push(
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .on_press(Intent::Cancel)
                .boxed(),
        );
    }
    form.push(buttons.boxed()).boxed()
}

/// Its shots for `anomi-shots`: `settings/game_activity.png` as the Slint
/// app showed it, and the page with custom games, one being edited.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::games::{custom, page, with};

    use crate::shots::{GOLDEN, Shot};

    [
        ("game_activity", page as fn() -> anomi_vm::Vm),
        ("game_activity_custom", || with(custom())),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
