//! Editing a message in place: `InlineEditor` (`ui/messages.slint:2873`).
//! The row of the message being edited shows, in its body's place, the
//! composer's field with the edit's text in a box of its own, the emoji
//! button on its last line, and under it Discord's "escape to cancel •
//! enter to save", whose two words are clicked too. The suggestions over
//! the field (`ui/yapfast.slint:4674`-`:4688`) float up from right above
//! the box, as the composer's do (C15's list).
//!
//! The keys: Enter saves and Shift+Enter breaks the line; Escape closes the
//! suggestions when they show and cancels the edit when they don't
//! ([`escape`]); Up, Down, Tab and Enter are the suggestions' while they
//! show. Paste is the field's own: an edit takes text, not files
//! (`paste => { self.paste-text(); }`). The field keeps no edit of its own:
//! it shows the view-model's text under the message's key, so a row the
//! list builds again shows the edit as it was. The app gives it the keys
//! as an edit starts with `focus::into(FIELD)`, its caret at the end.
//!
//! Nothing here moves: the caret blinks only while the window is in front
//! and in use (the composer field's own rule), and the emoji button's
//! hover goes through [`Motion`].

mod escape;

use iced::advanced::widget::Id;
use iced::widget::{column, container, mouse_area, row, space, text};
use iced::{Border, Fill, Padding, alignment, mouse};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::composer::Suggest;
use anomi_vm::chat::inline_edit::{Field, InlineEdit, Intent};
use anomi_vm::icon::Icon;
use anomi_vm::menu::{Align, Spot};
use anomi_widgets::button::icon_button;
use anomi_widgets::composer::{Suggestion, composer};
use anomi_widgets::menu::opener;

use crate::composer::suggestions;

/// The field's id, which the app focuses as an edit starts. One message is
/// edited at a time, in the channel or in the thread beside it.
pub const FIELD: Id = Id::new("inline-edit");

/// The row of the message being edited ends 21 px from its right edge,
/// whether in the channel or the thread beside it, rather than leaving the
/// hover bar its room (`padding-right: root.editing ? 21px`,
/// `ui/messages.slint:3085`).
pub const END: f32 = 21.0;

/// `spacing: 5px` between the box and the hint; `padding-bottom: 4px`.
const SPACING: f32 = 5.0;
const BOTTOM: f32 = 4.0;
/// The box: `border-width: 1px; border-radius: 8px`.
const BORDER: f32 = 1.0;
const RADIUS: f32 = 8.0;
/// Its row: `padding-left: 10px; padding-right: 12px; spacing: 6px`.
const ROW_LEFT: f32 = 10.0;
const ROW_RIGHT: f32 = 12.0;
const ROW_GAP: f32 = 6.0;
/// `composer-button-inset`: the room under the emoji button.
const INSET: f32 = 14.0;
/// The emoji button's `glyph: 18px`.
const GLYPH: f32 = 18.0;
/// The emoji picker opens under the button, its right edge on the
/// button's, 4 px off (`anchor-y + anchor-height + 4px`,
/// `ui/yapfast.slint:2650`), or over it where it doesn't fit.
const PICKER_GAP: f32 = 4.0;
/// The hint's words: `font-size: 12px; font-weight: 500`, `spacing: 3px`.
const HINT_SIZE: f32 = typography::size::CAPTION;
const HINT_GAP: f32 = 3.0;

/// The editor in the row of the message being edited; nothing for the
/// others.
pub fn view(inline_edit: &InlineEdit, motion: Motion) -> impl View<Intent> + '_ {
    let Some(field) = inline_edit.field.as_deref() else {
        return space().width(0).height(0).boxed();
    };
    column![
        column![
            suggestions::view(&field.suggestions, motion).map(Intent::Offered),
            boxed(field, motion),
        ]
        .width(Fill),
        hint(),
    ]
    .spacing(SPACING)
    .padding(Padding::ZERO.bottom(BOTTOM))
    .width(Fill)
    .boxed()
}

/// The box: the field, and the emoji button on its last line.
fn boxed(field: &Field, motion: Motion) -> Element<'_, Intent> {
    let line = row![
        escape::escape(input(field, motion), Intent::Cancel),
        emoji(motion),
    ]
    .spacing(ROW_GAP)
    .padding(Padding {
        top: 0.0,
        right: ROW_RIGHT,
        bottom: 0.0,
        left: ROW_LEFT,
    })
    .align_y(alignment::Vertical::Bottom)
    .width(Fill);
    container(line)
        .width(Fill)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.composer().into()),
                border: Border {
                    color: palette.outline_field,
                    width: BORDER,
                    radius: RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The composer's field, as the channel's is but for the edit: Enter
/// saves, and it neither stages files nor opens the pickers by keys.
fn input(field: &Field, motion: Motion) -> Element<'_, Intent> {
    let key = field.key;
    composer(&field.text, motion)
        .key(key)
        .id(FIELD)
        .placeholder(tr!("Edit your message"))
        .pad(metrics::COMPOSER_PADDING)
        .min_height(metrics::COMPOSER_HEIGHT)
        .marks(&field.marks)
        .suggesting(field.suggesting())
        .on_edit(move |text, caret| Intent::Edited { key, text, caret })
        .on_caret(move |caret| Intent::Caret { key, caret })
        .on_send(Intent::Save)
        .on_suggestion(|key| {
            Intent::Suggest(match key {
                Suggestion::Move(step) => Suggest::Move(step),
                Suggestion::Choose => Suggest::Choose,
                Suggestion::Dismiss => Suggest::Dismiss,
            })
        })
        .boxed()
}

/// The emoji button, 14 px over the box's bottom: the picker opens beside
/// it, where the window says it is.
fn emoji<'a>(motion: Motion) -> Element<'a, Intent> {
    let button = icon_button(Icon::Smile, tr!("Add an emoji"), motion).glyph(GLYPH);
    container(opener(button.view()).on_press(|button| {
        Intent::Emoji(Spot {
            align: Align::End,
            gap: PICKER_GAP,
            ..Spot::below(button)
        })
    }))
    .padding(Padding::ZERO.bottom(INSET))
    .boxed()
}

/// "escape to cancel • enter to save", the two verbs clicked as links.
fn hint<'a>() -> Element<'a, Intent> {
    row![
        words(tr!("escape to")),
        link(tr!("cancel"), Intent::Cancel),
        words(tr!("• enter to")),
        link(tr!("save"), Intent::Save),
    ]
    .spacing(HINT_GAP)
    .boxed()
}

fn says(words: &str) -> text::Text<'_, Theme> {
    text(words)
        .size(HINT_SIZE)
        .font(typography::MEDIUM)
        .wrapping(text::Wrapping::None)
}

fn words(words: &str) -> Element<'_, Intent> {
    says(words).style(catalog::text::primary).boxed()
}

/// A verb of the hint, in the link's colour, with the pointer's hand.
fn link(words: &str, intent: Intent) -> Element<'_, Intent> {
    mouse_area(says(words).style(catalog::text::link))
        .on_press(intent)
        .interaction(mouse::Interaction::Pointer)
        .boxed()
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`). No golden screenshot shows an edit: they compare with
/// the Slint app under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::chat::inline_edit as fixture;
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "chan",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("inline_edit", fixture::channel),
        shot("inline_edit_mention", fixture::channel_suggesting),
        shot("inline_edit_emoji", fixture::channel_emoji),
        shot("inline_edit_compact", fixture::channel_compact),
    ]
}
