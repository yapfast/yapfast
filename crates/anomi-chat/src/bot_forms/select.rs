//! A bot's select menu open: `BotSelectDropdown` (`ui/interactions.slint:24`)
//! and the options as it draws them. A panel as wide as the box it opened
//! under (240 to 480 px) and at most 360 px tall, with a search field for
//! members, roles and channels, "No matching options." when nothing is left,
//! and the options: emoji, label, description, and a box that is ticked when
//! the menu takes several or a check when it takes one. The shell places it
//! under its box, above it when there is no room below, and closes it on a
//! press outside or Escape.

use iced::widget::{Id, button, column, container, row, space, svg, text};
use iced::{Alignment, Border, Fill, Length, Padding, Task};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::bot_forms::{BotForms, Choice, Intent, Select};
use anomi_widgets::field::{self, field};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::scroll_view;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::card::panel;
use super::marks::mark;

/// The panel's width bounds and its greatest height (`max(min(.., 480px),
/// 240px)`, `min(.., 360px)`).
const WIDEST: f32 = 480.0;
const NARROWEST: f32 = 240.0;
const TALLEST: f32 = 360.0;
/// The search field and its padding take this much of it (`36px` and `4px`
/// either side).
const SEARCH_ROOM: f32 = metrics::CONTROL_HEIGHT + 2.0 * PADDING + SPACING;
/// The panel's padding and the space between its parts (`4px`, `2px`).
const PADDING: f32 = 4.0;
const SPACING: f32 = 2.0;
/// An option: its least height, its corners, its padding and the gap
/// between its parts (`40px`, `4px`, `8px/6px`, `8px`).
const OPTION: f32 = 40.0;
const OPTION_RADIUS: f32 = 4.0;
const OPTION_PAD_X: f32 = 8.0;
const OPTION_PAD_Y: f32 = 6.0;
const GAP: f32 = 8.0;
/// An emoji before the label (18 px), the check of a single pick (18 px).
const EMOJI: f32 = 18.0;
/// The slot an emoji keeps before the label: Twemoji's square of 72/53 em at
/// `EMOJI`, as `paragraph` keeps it, so a server's picture and a Unicode
/// glyph are one size and the labels after them line up.
const EMOJI_SLOT: f32 = EMOJI * 72.0 / 53.0;
const LABEL: f32 = typography::size::BODY;
const DETAIL: f32 = typography::size::LABEL;

/// The search field's id.
pub const SEARCH: &str = "bot-select-search";

/// The panel's width for a box `anchor` wide.
pub fn width(anchor: f32) -> f32 {
    anchor.clamp(NARROWEST, WIDEST)
}

/// Gives the search field the keys, for the app to run as the menu opens;
/// a menu without one takes them itself.
pub fn focus_search<T: Send + 'static>() -> Task<T> {
    field::focus(Id::new(SEARCH))
}

pub fn view(forms: &BotForms, motion: Motion) -> impl View<Intent> + '_ {
    let select = &forms.select;
    let mut parts = column(Vec::<Element<'_, Intent>>::new()).spacing(SPACING);
    if select.searchable {
        parts = parts.push(
            container(
                field(tr!("Search"), &select.query)
                    .on_input(Intent::Search)
                    .id(Id::new(SEARCH))
                    .motion(motion)
                    .view(),
            )
            .padding(PADDING)
            .boxed(),
        );
    }
    if select.choices.is_empty() {
        parts = parts.push(
            container(
                text(tr!("No matching options."))
                    .size(typography::size::SMALL)
                    .style(catalog::text::secondary),
            )
            .center_x(Fill)
            .center_y(OPTION)
            .boxed(),
        );
    }
    let options = column(
        select
            .choices
            .iter()
            .map(|choice| option(choice, select))
            .collect::<Vec<_>>(),
    )
    .spacing(SPACING);
    let most = TALLEST - 2.0 * PADDING - if select.searchable { SEARCH_ROOM } else { 0.0 };
    parts = parts.push(
        scroll_view(options, motion)
            .height(Length::Shrink.max(most))
            .boxed(),
    );
    container(parts.padding(PADDING))
        .width(width(select.anchor))
        .clip(true)
        .style(panel)
}

/// One option as a press: it toggles itself.
fn option<'a>(choice: &'a Choice, select: &Select) -> Element<'a, Intent> {
    let several = select.max > 1;
    let single = select.max == 1;
    let mut words = column![
        text(&choice.label)
            .size(LABEL)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed()
    ];
    if !choice.description.is_empty() {
        words = words.push(
            text(&choice.description)
                .size(DETAIL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::secondary)
                .width(Fill)
                .boxed(),
        );
    }
    let mut line = row(Vec::<Element<'_, Intent>>::new())
        .spacing(GAP)
        .align_y(Alignment::Center);
    if let Some(handle) = &choice.picture {
        line = line.push(
            container(picture(handle.clone()).side(EMOJI_SLOT).fit(Fit::Contain))
                .center(EMOJI_SLOT)
                .boxed(),
        );
    } else if !choice.emoji.is_empty() {
        line = line.push(
            container(
                text(&choice.emoji)
                    .size(EMOJI)
                    .wrapping(text::Wrapping::None)
                    .style(catalog::text::primary),
            )
            .center(EMOJI_SLOT)
            .boxed(),
        );
    }
    line = line.push(words.boxed());
    if several {
        line = line.push(mark(choice.selected, false));
    } else if single && choice.selected {
        line = line.push(
            icon(Icon::Check, EMOJI)
                .style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().accent),
                })
                .boxed(),
        );
    }
    let chosen = choice.selected && single;
    button(row![space().height(OPTION - 2.0 * OPTION_PAD_Y), line].align_y(Alignment::Center))
        .padding(Padding::from([OPTION_PAD_Y, OPTION_PAD_X]))
        .width(Fill)
        .on_press(Intent::Toggle(choice.index))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let fill = match status {
                button::Status::Hovered | button::Status::Pressed => Some(palette.hover),
                _ if chosen => Some(palette.sunken),
                _ => None,
            };
            button::Style {
                background: fill.map(Into::into),
                text_color: palette.text,
                border: Border::default().rounded(OPTION_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
}
