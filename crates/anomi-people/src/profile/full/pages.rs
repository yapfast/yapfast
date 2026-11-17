//! What the right column lists under the tabs: the activity cards and what
//! they did lately, the board's widgets and the wishlist's grid
//! (`ProfileModal`, `ui/people.slint:2067`; `ShowcasePicture`, `:528`;
//! `BoardCard`, `:554`; `WishlistCard`, `:584`).

use iced::widget::{Column, Row, container, space, text};
use iced::{Alignment, Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::Expected;
use anomi_vm::people::full_profile::{FullProfile, Game, Recent, Widget, Wish};
use anomi_vm::people::profile::Intent as CardIntent;
use anomi_vm::people::{self};
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::look::{Look, Shade};
use crate::profile::card::look::Inks;

const TILE_RADIUS: f32 = 12.0;
const GAP: f32 = 12.0;
/// The wishlist's columns and widest card.
const COLUMNS: usize = 3;
const WISH_MAX: f32 = 168.0;
/// A card is this much taller than wide; its name's line, 12 px text.
const WISH_NAME: f32 = 26.0;
const WISH_LINE: f32 = 16.0;
const WISH_PADDING: f32 = 10.0;
const WISH_GAP: f32 = 8.0;

/// What a showcase picture's room is filled with.
#[derive(Clone, Copy)]
enum Room {
    Nothing,
    Tile,
    Well,
}

/// A tile of the modal: a card of the column's width.
fn tile<'a, M: 'a>(
    content: impl View<M> + 'a,
    padding: impl Into<Padding>,
    look: Look,
) -> Element<'a, M> {
    sized(content, padding, Fill, Length::Shrink, look)
}

fn sized<'a, M: 'a>(
    content: impl View<M> + 'a,
    padding: impl Into<Padding>,
    width: impl Into<Length>,
    height: impl Into<Length>,
    look: Look,
) -> Element<'a, M> {
    container(content.boxed())
        .width(width)
        .height(height)
        .padding(padding)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(look.tile(theme.palette()))),
            border: Border::default().rounded(TILE_RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// A picture of the Board, the Wishlist or a recent entry, contained in its
/// room: the room is empty while the picture is on its way and holds the
/// initial when there is none.
#[allow(clippy::too_many_arguments)]
fn showcase<'a, M: 'a>(
    shown: &'a Expected,
    initial: &'a str,
    size: (f32, f32),
    initial_size: f32,
    radius: f32,
    room: Room,
    look: Look,
    motion: Motion,
) -> Element<'a, M> {
    let inner: Element<'a, M> = match shown {
        Expected::Ready(handle) => picture(handle.clone())
            .width(size.0)
            .height(size.1)
            .fit(Fit::Contain)
            .radius(radius)
            .motion(motion)
            .boxed(),
        Expected::Waiting => space().boxed(),
        Expected::None => look
            .words(initial, initial_size, typography::BOLD, Shade::Ink3)
            .boxed(),
    };
    container(inner)
        .center_x(size.0)
        .center_y(size.1)
        .clip(true)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: match room {
                    Room::Nothing => None,
                    Room::Tile => Some(Background::Color(look.tile(palette))),
                    Room::Well => Some(Background::Color(look.well(palette))),
                },
                border: Border::default().rounded(radius),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The Activity tab: what they are doing, a card each, then a title and
/// what they did lately.
pub fn activity<'a>(
    full: &'a FullProfile,
    look: Look,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let card = &full.card;
    let shown = Inks::activity(card);
    let mut page = Column::<Element<'a, people::Intent>>::new()
        .spacing(GAP)
        .width(Fill);
    for entry in &card.activities {
        let privacy = card.own && entry.game;
        let body = crate::activity::card::view(entry, card.now, shown, false, motion)
            .map(move |intent| people::Intent::from(CardIntent::Activity { intent, privacy }));
        page = page.push(tile(body, 16, look));
    }
    if !full.recent.is_empty() {
        let title = look.words(
            tr!("Recent activity"),
            typography::size::HEADING,
            typography::MEDIUM,
            Shade::Ink,
        );
        page = page.push(
            container(title)
                .padding(Padding::ZERO.top(if card.activities.is_empty() { 0.0 } else { 8.0 }))
                .boxed(),
        );
    }
    for entry in &full.recent {
        page = page.push(recent(entry, look, motion));
    }
    page.boxed()
}

/// A past thing: its picture and up to three lines.
fn recent<'a, M: 'a>(entry: &'a Recent, look: Look, motion: Motion) -> Element<'a, M> {
    let when = if entry.music {
        format!("\u{266b} {}", entry.when)
    } else {
        entry.when.clone()
    };
    let mut lines = Column::<Element<'a, M>>::new().spacing(2).width(Fill).push(
        look.words(
            entry.title.as_str(),
            typography::size::SMALL,
            typography::SEMIBOLD,
            Shade::Ink,
        )
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .boxed(),
    );
    if !entry.subtitle.is_empty() {
        lines = lines.push(
            look.words(
                entry.subtitle.as_str(),
                typography::size::SMALL,
                typography::UI,
                Shade::Ink2,
            )
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
        );
    }
    if !entry.when.is_empty() {
        lines = lines.push(
            look.words(when, typography::size::CAPTION, typography::UI, Shade::Ink3)
                .boxed(),
        );
    }
    let body = Row::<Element<'a, M>>::new()
        .spacing(12)
        .push(showcase(
            &entry.picture,
            &entry.initial,
            (60.0, 60.0),
            22.0,
            6.0,
            Room::Tile,
            look,
            motion,
        ))
        .push(container(lines).center_y(60).boxed());
    tile(body, 12, look)
}

/// The Board tab: a card for each widget.
pub fn board<'a, M: 'a>(full: &'a FullProfile, look: Look, motion: Motion) -> Element<'a, M> {
    Column::with_children(
        full.widgets
            .iter()
            .map(|widget| widget_card(widget, look, motion)),
    )
    .spacing(GAP)
    .width(Fill)
    .boxed()
}

fn widget_card<'a, M: 'a>(widget: &'a Widget, look: Look, motion: Motion) -> Element<'a, M> {
    let mut body = Column::<Element<'a, M>>::new().spacing(10).push(
        look.words(
            widget.title.as_str(),
            typography::size::SMALL,
            typography::SEMIBOLD,
            Shade::Ink,
        )
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .boxed(),
    );
    for game in &widget.games {
        body = body.push(game_row(game, look, motion));
    }
    if widget.elsewhere {
        body = body.push(
            look.words(
                tr!("This one only shows in Discord for now."),
                typography::size::CAPTION,
                typography::UI,
                Shade::Ink3,
            )
            .boxed(),
        );
    }
    tile(body, 14, look)
}

fn game_row<'a, M: 'a>(game: &'a Game, look: Look, motion: Motion) -> Element<'a, M> {
    let mut lines = Column::<Element<'a, M>>::new().spacing(2).width(Fill).push(
        look.words(
            game.name.as_str(),
            typography::size::LABEL,
            typography::MEDIUM,
            Shade::Ink,
        )
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .boxed(),
    );
    if !game.comment.is_empty() {
        lines = lines.push(
            look.words(
                game.comment.as_str(),
                typography::size::CAPTION,
                typography::UI,
                Shade::Ink2,
            )
            .boxed(),
        );
    }
    Row::<Element<'a, M>>::new()
        .push(showcase(
            &game.icon,
            &game.initial,
            (44.0, 44.0),
            16.0,
            8.0,
            Room::Well,
            look,
            motion,
        ))
        .push(lines.boxed())
        .spacing(10)
        .align_y(Alignment::Center)
        .boxed()
}

/// The Wishlist tab: its note, then a grid three across in cards a third of
/// the column wide, at most 168 px, a name's height taller than wide.
/// `column_width` is the tabs' column's.
pub fn wishlist<'a, M: 'a>(
    full: &'a FullProfile,
    column_width: f32,
    look: Look,
    motion: Motion,
) -> Element<'a, M> {
    let side = ((column_width - 2.0 * GAP) / COLUMNS as f32).clamp(0.0, WISH_MAX);
    let mut page = Column::<Element<'a, M>>::new().spacing(GAP).width(Fill);
    if !full.wishlist_note.is_empty() {
        page = page.push(
            look.words(
                full.wishlist_note.as_str(),
                typography::size::SMALL,
                typography::UI,
                Shade::Ink2,
            )
            .boxed(),
        );
    }
    for line in full.wishlist.chunks(COLUMNS) {
        page = page.push(
            Row::with_children(line.iter().map(|wish| wish_card(wish, side, look, motion)))
                .spacing(GAP)
                .boxed(),
        );
    }
    page.boxed()
}

fn wish_card<'a, M: 'a>(wish: &'a Wish, side: f32, look: Look, motion: Motion) -> Element<'a, M> {
    // The picture takes what the name leaves of the card.
    let inner = side - 2.0 * WISH_PADDING;
    let high = (side + WISH_NAME - 2.0 * WISH_PADDING - WISH_GAP - WISH_LINE).max(0.0);
    let body = Column::<Element<'a, M>>::new()
        .spacing(WISH_GAP)
        .push(showcase(
            &wish.picture,
            &wish.initial,
            (inner, high),
            40.0,
            8.0,
            Room::Nothing,
            look,
            motion,
        ))
        .push(
            container(
                look.words(
                    wish.name.as_str(),
                    typography::size::CAPTION,
                    typography::MEDIUM,
                    Shade::Ink,
                )
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
            )
            .center_x(inner)
            .boxed(),
        );
    sized(body, WISH_PADDING, side, side + WISH_NAME, look)
}
