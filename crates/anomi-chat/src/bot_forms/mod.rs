//! A bot's forms and select menus: Slint's `BotModalDialog`,
//! `BotSelectDropdown`, `BotSelectBox` and the marks
//! (`ui/interactions.slint:155`, `:24`, `ui/bot_select.slint:15`) over what
//! `show_bot_interactions` fills (`src/ui/refresh.rs:784`); the handlers are
//! `src/ui/callbacks.rs:843`-`:917`. Task C20.
//!
//! [`view`] is the form (`Layer::BotModal`): a card 520 px wide with its
//! title and X, the fields in a column that scrolls when the window is too
//! short, why Discord refused it in red, and Cancel and Submit. Escape and a
//! press outside close it (the shell's layer) unless it is being sent;
//! nothing can be changed while it is, and Submit then reads "Submitting…".
//! [`select`] is a select menu's dropdown (`Layer::BotSelect`), whether it
//! opened from a message or from a field of the form.

mod card;
mod fields;
mod marks;
mod select;
#[cfg(feature = "shots")]
mod shots;
mod stack;

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::bot_forms::{BotForms, Intent};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

pub use select::{focus_search, view as select};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The card's width: `min(520px, root.width - 40px)`; the layer keeps it
/// inside a narrow window.
const WIDTH: f32 = 520.0;
/// Inside the card and between its parts (`padding: 20px; spacing: 20px`).
const PADDING: f32 = 20.0;
const SPACING: f32 = 20.0;
const TITLE: f32 = 18.0;
const DETAIL: f32 = typography::size::SMALL;

pub fn view(forms: &BotForms, motion: Motion) -> impl View<Intent> + '_ {
    let form = &forms.form;
    let ready = !form.saving;
    let title = row![
        text(&form.heading)
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(28.0)
            .glyph(18.0)
            .enabled(ready)
            .on_press(Intent::Close)
            .view(),
    ]
    .align_y(Alignment::Center);
    let mut fields = column(
        form.fields
            .iter()
            .enumerate()
            .map(|(index, field)| fields::field(index, field, ready, motion))
            .collect::<Vec<_>>(),
    )
    .spacing(fields::FIELD_GAP);
    if !form.supported {
        fields = fields.push(
            text(tr!(
                "This form has a part Yapfast can't fill yet. Use it in Discord."
            ))
            .size(DETAIL)
            .style(catalog::text::danger)
            .width(Fill)
            .boxed(),
        );
    }
    let mut foot = column(Vec::<Element<'_, Intent>>::new()).spacing(SPACING);
    if !form.error.is_empty() {
        foot = foot.push(
            text(&form.error)
                .size(DETAIL)
                .style(catalog::text::danger)
                .width(Fill)
                .boxed(),
        );
    }
    let submit = if form.saving {
        tr!("Submitting…")
    } else {
        tr!("Submit")
    };
    foot = foot.push(
        row![
            space().width(Fill),
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .on_press(Intent::Close)
                .enabled(ready),
            action_button(submit, Kind::Primary, motion)
                .on_press(Intent::Submit)
                .enabled(form.supported && ready),
        ]
        .spacing(metrics::SPACE_S)
        .boxed(),
    );
    let body = scroll_view(fields, motion).height(Length::Shrink);
    let parts = stack::parts(title, body, foot, SPACING);
    container(parts)
        .padding(PADDING)
        .width(Length::Fixed(WIDTH))
        .style(card::dialog)
}
