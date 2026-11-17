//! The Reactions window: `ReactionsDialog` (`ui/dialogs.slint:797`). The
//! reactions (or a poll's answers) down the left, the most first, and the
//! chosen one's people on the right, a page at a time.

use iced::widget::{
    button, column, container, hover, row, space, text, text::Ellipsis, text::Wrapping,
};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::reactions::{Dialog, Intent, Pill, Reactor};
use anomi_vm::icon::Icon;
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::icon_button;
use anomi_widgets::list::{list, scroll_view};
use yapfast_lang::tr;

use super::emoji;

/// The card: 600 by 720 (`min(600px, root.width - 48px)` by
/// `min(720px, root.height - 96px)`; the layer keeps it inside the window).
const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 720.0;
const SHADOW_ALPHA: f32 = 0.4;
/// The header: 20 px bold, padded 16 above, 24 and 16 at the sides, 12 under.
const TITLE: f32 = 20.0;
const HEADER: Padding = Padding {
    top: 16.0,
    right: 16.0,
    bottom: 12.0,
    left: 24.0,
};
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The tabs' column, 98 px wide, 220 for answers, a hairline at its end.
const TABS: f32 = 98.0;
const TABS_VOTERS: f32 = 220.0;
const TABS_PAD: Padding = Padding {
    top: 12.0,
    right: 16.0,
    bottom: 0.0,
    left: 0.0,
};
/// A tab: 32 px high in a 40 px slot, 8 px corners, its emoji at 20.
const TAB: f32 = 32.0;
const TAB_GAP: f32 = 8.0;
const TAB_RADIUS: f32 = 8.0;
const TAB_EMOJI: f32 = 20.0;
/// Before the tabs.
const BODY_INSET: f32 = 24.0;
const TAB_INSET: Padding = Padding {
    top: 0.0,
    right: 8.0,
    bottom: 0.0,
    left: 4.0,
};
/// A person: a 52 px row, 16 px in, a 32 px avatar and 10 between.
const PERSON: f32 = 52.0;
const PERSON_INSET: f32 = 16.0;
const AVATAR: f32 = 32.0;
const PERSON_GAP: f32 = 10.0;
const PEOPLE_PAD: f32 = 8.0;
const REMOVE: f32 = 28.0;
const REMOVE_GLYPH: f32 = 16.0;
/// A note under the people: 13 px, muted; an error 16 px in, wrapping.
const NOTE: f32 = typography::size::LABEL;
const ERROR_INSET: f32 = 16.0;

pub fn view(dialog: &Dialog, motion: Motion) -> impl View<Intent> + '_ {
    let title = if dialog.voters {
        tr!("Votes")
    } else {
        tr!("Reactions")
    };
    let close = icon_button(Icon::X, tr!("Close"), motion)
        .size(CLOSE)
        .glyph(CLOSE_GLYPH)
        .on_press(Intent::Close)
        .view();
    let header = container(
        row![
            text(title)
                .size(TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .width(Fill)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End),
            close
        ]
        .align_y(Alignment::Center),
    )
    .padding(HEADER);
    let body = row![tabs(dialog, motion), people(dialog, motion)]
        .height(Fill)
        .padding(Padding::default().left(BODY_INSET));
    container(column![header, rule(), body])
        .width(WIDTH)
        .height(HEIGHT)
        .style(card)
}

fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

fn rule<'a>() -> Element<'a, Intent> {
    container(space().width(Fill).height(1))
        .style(|theme: &Theme| container::Style::default().background(theme.palette().line))
        .boxed()
}

/// The reactions or answers, one tab each; the chosen one lit.
fn tabs<'a>(dialog: &'a Dialog, motion: Motion) -> Element<'a, Intent> {
    let width = if dialog.voters { TABS_VOTERS } else { TABS };
    let all = dialog
        .pills
        .iter()
        .map(|pill| tab(pill, dialog, motion))
        .collect::<Vec<_>>();
    let content = container(column(all).spacing(TAB_GAP)).padding(TABS_PAD);
    let edge = container(space().width(1).height(Fill))
        .style(|theme: &Theme| container::Style::default().background(theme.palette().line));
    row![
        container(scroll_view(content, motion)).width(width - 1.0),
        edge
    ]
    .width(width)
    .boxed()
}

fn tab<'a>(pill: &'a Pill, dialog: &'a Dialog, motion: Motion) -> Element<'a, Intent> {
    let chosen = pill.key == dialog.chosen;
    let mut face = row![].spacing(TAB_GAP).align_y(Alignment::Center);
    if let Some(shown) = &pill.face {
        face = face.push(emoji(shown, TAB_EMOJI, motion));
    }
    if dialog.voters {
        face = face.push(
            text(pill.name.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::primary)
                .width(Fill)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .boxed(),
        );
    }
    face = face.push(
        text(pill.count.to_string())
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .boxed(),
    );
    button(face.width(Fill))
        .width(Fill)
        .height(TAB)
        .padding(TAB_INSET)
        .on_press(Intent::Select(pill.key.clone()))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let background = if chosen {
                Some(palette.active)
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(palette.hover)
            } else {
                None
            };
            button::Style {
                background: background.map(Background::Color),
                border: Border::default().rounded(TAB_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
}

/// Who reacted with the chosen one, then what the next page is doing.
fn people<'a>(dialog: &'a Dialog, motion: Motion) -> Element<'a, Intent> {
    let count = dialog.people.len();
    let tail = usize::from(dialog.loading || dialog.more) + usize::from(dialog.error.is_some());
    let mut rows = list(
        count + tail,
        move |at| at as u64,
        move |at| {
            if let Some(person) = dialog.people.get(at) {
                return person_row(person, motion);
            }
            if at == count && (dialog.loading || dialog.more) {
                container(note(tr!("Loading...")))
                    .center_x(Fill)
                    .center_y(PERSON)
                    .boxed()
            } else {
                container(note(dialog.error.as_deref().unwrap_or_default()))
                    .width(Fill)
                    .padding(ERROR_INSET)
                    .boxed()
            }
        },
        motion,
    )
    .padding(Padding::new(PEOPLE_PAD).left(0).right(0));
    // The next page comes as the list nears its end, as in Discord's client.
    if dialog.more && !dialog.loading {
        rows = rows.on_viewport(|seen| Intent::Scrolled {
            near_end: seen.near_end,
        });
    }
    container(rows).width(Fill).height(Fill).boxed()
}

fn note(words: &str) -> iced::widget::Text<'_, Theme> {
    text(words).size(NOTE).style(catalog::text::muted)
}

fn person_row<'a>(person: &'a Reactor, motion: Motion) -> Element<'a, Intent> {
    let mut names = column![
        text(person.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::MEDIUM)
            .style(catalog::text::primary)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
    ]
    .width(Fill);
    // Room kept for the X, which shows over the row's end on hover.
    if person.removable {
        names = names.padding(Padding::default().right(REMOVE + PERSON_GAP));
    }
    if !person.username.is_empty() {
        names = names.push(
            text(person.username.as_str())
                .size(typography::size::CAPTION)
                .style(catalog::text::muted)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .boxed(),
        );
    }
    let base = container(
        row![
            portrait(&person.avatar, AVATAR).view(),
            container(names).center_y(Fill)
        ]
        .spacing(PERSON_GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding::default().left(PERSON_INSET).right(PERSON_INSET))
    .height(PERSON)
    .center_y(PERSON)
    .width(Fill);
    if !person.removable {
        return base.boxed();
    }
    let remove = icon_button(Icon::X, tr!("Remove Reaction"), motion)
        .size(REMOVE)
        .glyph(REMOVE_GLYPH)
        .on_press(Intent::Remove(person.id.clone()))
        .view();
    // Holds the row, its X among it, so it stays shown over that.
    hover(
        base,
        container(remove)
            .align_right(Fill)
            .center_y(PERSON)
            .padding(Padding::default().right(PERSON_INSET)),
    )
    .boxed()
}
