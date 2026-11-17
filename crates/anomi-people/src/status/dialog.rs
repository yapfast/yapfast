//! The custom status dialog's card (`StatusDialog`, `ui/dialogs.slint:27`):
//! a title with its X, the emoji button beside the words, "Clear after" with
//! its five options, what went wrong, and Clear status, Cancel and Save.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Background, Border, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::people::status::{ClearAfter, Intent, Status};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::field::{Id, field};
use yapfast_lang::tr;

use super::choices::choices;
use super::emoji_button::emoji_button;
use super::link::link;

/// The card's width (`max(460px, ..)`, `:42`): the five options fit it in
/// both languages, so it is the width that holds.
const WIDTH: f32 = 460.0;
/// `padding: 20px; spacing: 12px`.
const PAD: f32 = 20.0;
const GAP: f32 = 12.0;
/// The title's size and its row, the X's 28 px (`:56`).
const TITLE_SIZE: f32 = 18.0;
const TITLE: f32 = 28.0;
/// The X's glyph (`ui/dialogs.slint:59`).
const GLYPH: f32 = 18.0;
/// The words' field (`min-height: 40px; text-size: 15px; pad: 10px`).
const FIELD: f32 = 40.0;
const FIELD_PAD: f32 = 10.0;
/// The emoji button's well (`:77`) and what the remove badge overhangs it
/// by, above and to its right: the stack keeps room for it.
pub(super) const WELL: f32 = 40.0;
pub(super) const OVERHANG: f32 = 5.0;
/// The room between the well's right edge and the field (`spacing: 12px`)
/// less the badge's overhang, which lies in it.
const BESIDE: f32 = GAP - OVERHANG;
/// The card's shadow (`:49`): 32 px blur, 12 px down, `#00000066`.
const SHADOW_ALPHA: u8 = 0x66;
const SHADOW_DOWN: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// The buttons' row sits 4 px lower than the column's gap (`padding-top`).
const BUTTONS_DOWN: f32 = 4.0;
/// `spacing: 8px`.
const BUTTONS_GAP: f32 = 8.0;
/// A clear-after heading's size (`SettingsHeading`, 12 px).
const HEADING: f32 = typography::size::CAPTION;
/// SettingsHeading is 16 px high; the text's own line would be 16.4.
const HEADING_LINE: f32 = 16.0;

/// The buttons' ids, to focus them from a test or a task.
pub const SAVE: &str = "status-save";
pub const CANCEL: &str = "status-cancel";

/// The field's id: the app focuses it as the dialog opens (`init`, `:99`)
/// with `anomi_widgets::field::focus`.
pub fn text_id() -> Id {
    Id::new("status-text")
}

pub(super) fn view(status: &Status, motion: Motion) -> impl View<Intent> + '_ {
    let title = row![
        text(tr!("Set a custom status"))
            .size(TITLE_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(TITLE)
            .glyph(GLYPH)
            .on_press(Intent::Close)
            .view(),
    ]
    .align_y(Alignment::Center)
    .height(TITLE);

    let mut words = field(tr!("What's happening?"), &status.text)
        .id(text_id())
        .on_input(Intent::Text)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(FIELD)
        .motion(motion);
    if !status.saving {
        words = words.on_submit(Intent::Save);
    }
    let top = row![
        emoji_button(status.emoji.as_ref()),
        space().width(BESIDE),
        words.view(),
    ]
    .align_y(Alignment::End);

    let labels = [
        tr!("Never"),
        tr!("30 min"),
        tr!("1 hour"),
        tr!("4 hours"),
        tr!("Today"),
    ];
    let heading = text(tr!("CLEAR AFTER"))
        .size(HEADING)
        .line_height(iced::Pixels(HEADING_LINE))
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::muted);
    let after = choices(&labels, status.clear_after.index(), |index| {
        Intent::ClearAfter(ClearAfter::at(index))
    });

    let mut buttons: Vec<Element<'_, Intent>> = Vec::new();
    if status.has_custom {
        // Its words on the edge the rows above start on, as a link.
        buttons.push(link(tr!("Clear status"), !status.saving, Intent::Clear));
    }
    let saving = status.saving;
    buttons.push(space().width(Fill).boxed());
    buttons.push(
        action_button(tr!("Cancel"), Kind::Ghost, motion)
            .id(iced::advanced::widget::Id::new(CANCEL))
            .on_press(Intent::Close)
            .boxed(),
    );
    let button = action_button(
        if saving {
            tr!("Saving...")
        } else {
            tr!("Save")
        },
        Kind::Primary,
        motion,
    )
    .id(iced::advanced::widget::Id::new(SAVE))
    .enabled(!saving);
    buttons.push(
        if saving {
            button
        } else {
            button.on_press(Intent::Save)
        }
        .boxed(),
    );
    let buttons = row(buttons).align_y(Alignment::Center).spacing(BUTTONS_GAP);

    // The gaps are explicit: the badge on the emoji's corner lies in the one
    // above it, so that one is shorter by its overhang.
    let mut card: Vec<Element<'_, Intent>> = vec![
        title.boxed(),
        space().height(GAP - OVERHANG).boxed(),
        top.boxed(),
        space().height(GAP).boxed(),
        heading.boxed(),
        space().height(GAP).boxed(),
        after,
        space().height(GAP).boxed(),
    ];
    if !status.error.is_empty() {
        card.push(
            text(status.error.as_str())
                .size(typography::size::SMALL)
                .width(Fill)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().danger),
                    ..text::Style::default()
                })
                .boxed(),
        );
        card.push(space().height(GAP).boxed());
    }
    card.push(space().height(BUTTONS_DOWN).boxed());
    card.push(buttons.boxed());
    let card = column(card).padding(PAD);
    container(card).width(WIDTH).style(card_style)
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow (32 px blur, 12 px down, `#00000066`).
fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA as f32 / 255.0),
            offset: Vector::new(0.0, SHADOW_DOWN),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}
