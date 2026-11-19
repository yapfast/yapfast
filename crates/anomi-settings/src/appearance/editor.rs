//! The theme editor, drawn in place of the Appearance page while a theme is
//! edited: a name with Save, Cancel and Export, what a new theme starts from,
//! whether it is dark, and a hex field for each of the 37 colours. The window
//! shows each change at once; Save keeps them and Cancel restores the theme
//! from before (the app's). The Slint side: the editor half of
//! `AppearanceSettings` (`ui/settings.slint:1561`).

mod colour_row;
mod columns;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Column, column, container, row, text};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::settings::theme_editor::{Intent, ThemeEditor};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

use crate::kit::{heading, picker, switch};

#[cfg(feature = "shots")]
pub use shots::shots;

/// Between the editor's parts, and inside a part (`spacing: 16px`, `8px`).
const SPACING: f32 = 16.0;
const GROUP: f32 = 8.0;
/// The name field's height and text.
const NAME_HEIGHT: f32 = 36.0;
const NAME_SIZE: f32 = 15.0;

pub fn view(editor: &ThemeEditor, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = vec![top(editor, motion)];
    if !editor.saved {
        parts.push(
            column![
                heading(tr!("START FROM")),
                picker(
                    &editor.starts,
                    editor.start,
                    Some(Intent::Start as fn(usize) -> Intent),
                ),
            ]
            .spacing(GROUP)
            .boxed(),
        );
    }
    parts.push(
        switch(tr!("Dark"), editor.dark, motion)
            .description(tr!("Drop-down lists and scroll bars draw dark to match."))
            .on_toggle(Intent::Dark)
            .view(),
    );
    parts.push(colours(editor, motion));
    if editor.saved {
        parts.push(delete(editor, motion));
    }
    column(parts).spacing(SPACING).width(Length::Fill)
}

/// The heading, the name with its buttons, and how the last export went.
fn top(editor: &ThemeEditor, motion: Motion) -> Element<'_, Intent> {
    let heading = heading(if editor.saved {
        tr!("EDIT THEME")
    } else {
        tr!("NEW THEME")
    });
    let can = editor.can_save();
    let mut name = field(tr!("AppearanceSettings" => "Name"), &editor.name)
        .on_input(Intent::Name)
        .text_size(NAME_SIZE)
        .height(NAME_HEIGHT)
        .motion(motion);
    if can {
        name = name.on_submit(Intent::Save);
    }
    let buttons = row![
        container(name.view()).width(Length::Fill),
        action_button(tr!("Save"), Kind::Primary, motion)
            .enabled(can)
            .on_press(Intent::Save),
        action_button(tr!("Cancel"), Kind::Secondary, motion).on_press(Intent::Cancel),
        action_button(tr!("Export"), Kind::Secondary, motion)
            .leading(Icon::Download)
            .enabled(can)
            .on_press(Intent::Export),
    ]
    .spacing(GROUP)
    .align_y(alignment::Vertical::Center);
    let mut parts: Column<Element<'_, Intent>> = column![heading, buttons].spacing(GROUP);
    if !editor.note.is_empty() {
        let wrong = editor.note_wrong;
        parts = parts.push(
            text(&editor.note)
                .size(typography::size::LABEL)
                .width(Length::Fill)
                .style(move |theme: &Theme| iced::widget::text::Style {
                    color: Some(if wrong {
                        theme.palette().danger_text()
                    } else {
                        theme.palette().secondary
                    }),
                    ..Default::default()
                })
                .boxed(),
        );
    }
    parts.boxed()
}

fn colours(editor: &ThemeEditor, motion: Motion) -> Element<'_, Intent> {
    let rows = editor
        .colours
        .iter()
        .enumerate()
        .map(|(index, colour)| colour_row::colour_row(colour, index, motion))
        .collect();
    column![
        heading(tr!("COLOURS")),
        typography::label(tr!(
            "Each as #rgb, #rrggbb or #rrggbbaa. The window shows a change as you type it."
        )),
        columns::columns(rows),
    ]
    .spacing(GROUP)
    .boxed()
}

/// Delete asks twice: the first press changes the button's words.
fn delete(editor: &ThemeEditor, motion: Motion) -> Element<'_, Intent> {
    let (words, intent) = if editor.confirming {
        (tr!("Delete it for good"), Intent::ConfirmDelete)
    } else {
        (tr!("Delete theme"), Intent::Delete)
    };
    action_button(words, Kind::Danger, motion)
        .small()
        .leading(Icon::Trash)
        .on_press(intent)
        .boxed()
}
