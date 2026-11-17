//! The switcher's rows: a section's heading, and a place (`PlaceRow` in the
//! repeater, `ui/dialogs.slint:451`-`:500`).

use iced::widget::{button, container, row, svg, text};
use iced::{Alignment, Background, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::switcher::Intent;
use anomi_vm::switcher::{Entry, Heading, Mark, Place, Switcher};
use anomi_widgets::avatar::circle;
use anomi_widgets::badge::badge;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// A place's row (`pitch: 40px`) and a heading's (`head: 28px`).
pub const PITCH: f32 = 40.0;
const HEAD: f32 = 28.0;
/// The heading's text starts 8 px in (`x: 8px`), at 12 px bold.
const HEAD_INSET: f32 = metrics::SPACE_S;
/// A row's insets and gap (`padding-left: 8px; spacing: 10px`).
const INSET: f32 = metrics::SPACE_S;
const GAP: f32 = 10.0;
/// A person's, group's or server's picture and a channel's icon.
const AVATAR: f32 = 24.0;
const ICON: f32 = metrics::ICON_ROW;
/// The name (`font-size: 16px`) and where it is (`13px`).
const NAME: f32 = typography::size::TITLE;
const PLACE: f32 = 13.0;

/// Row `at` of the list: a heading or the place it shows.
pub fn view<'a>(
    switcher: &'a Switcher,
    at: usize,
    current: Option<usize>,
    _motion: Motion,
) -> Element<'a, Intent> {
    match switcher.entry(at) {
        Entry::Heading(heading) => heading_row(heading),
        Entry::Place(index) => place_row(&switcher.places[index], current == Some(index)),
    }
}

fn heading_row<'a>(heading: Heading) -> Element<'a, Intent> {
    let words = match heading {
        Heading::Mentions => tr!("MENTIONS"),
        Heading::Previous => tr!("PREVIOUS CHANNELS"),
    };
    container(
        text(words)
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(catalog::text::muted),
    )
    .padding([0.0, HEAD_INSET])
    .center_y(HEAD)
    .width(Fill)
    .boxed()
}

fn place_row(place: &Place, current: bool) -> Element<'_, Intent> {
    let mark: Element<'_, Intent> = match &place.mark {
        Mark::Avatar(avatar) => container(circle(avatar, AVATAR))
            .center_y(Length::Fill)
            .boxed(),
        Mark::Icon(glyph) => container(icon(*glyph, ICON).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().faint),
        }))
        .center_y(Length::Fill)
        .boxed(),
    };
    let lit = place.unread || current;
    let name = text(place.name.as_str())
        .size(NAME)
        .font(if place.unread {
            typography::BOLD
        } else {
            typography::MEDIUM
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(if lit {
            catalog::text::primary
        } else {
            catalog::text::secondary
        });
    let whereabouts = text(place.place.as_str())
        .size(PLACE)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::muted)
        .width(Fill);
    // The badge is laid out before the words, so a long name elides inside
    // `words` and the badge keeps its width.
    let words = row![name, whereabouts]
        .spacing(GAP)
        .align_y(Alignment::Center)
        .width(Fill);
    let mut content = row![mark, words]
        .spacing(GAP)
        .align_y(Alignment::Center)
        .height(Fill);
    if !place.badge.is_empty() {
        content = content.push(
            container(badge(&place.badge))
                .center_y(Length::Fill)
                .boxed(),
        );
    }
    button(content)
        .padding([0.0, INSET])
        .width(Fill)
        .height(PITCH)
        .on_press(Intent::Go(place.id.clone()))
        .style(move |theme: &Theme, status| place_style(theme, status, current))
        .boxed()
}

/// The chosen row's active fill, which the keys move at once; the pointer's
/// hover fill on another.
fn place_style(theme: &Theme, status: button::Status, current: bool) -> button::Style {
    let palette = theme.palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let fill = if current {
        Some(palette.active)
    } else if hovered {
        Some(palette.hover)
    } else {
        None
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: palette.text,
        border: Border::default().rounded(metrics::RADIUS_ROW),
        ..button::Style::default()
    }
}
