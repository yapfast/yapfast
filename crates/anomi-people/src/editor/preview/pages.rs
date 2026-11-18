//! What the preview lists under its tabs: the activity cards, the board's
//! widgets and the wishlist's grid, or Yapfast's words for a tab with
//! nothing (`ProfileTabs`, `ui/server.slint:1062`; `ShowcasePicture`,
//! `ui/people.slint:528`; `BoardCard`, `:554`; `WishlistCard`, `:584`).

use iced::widget::{Column, Row, container, space, text};
use iced::{Alignment, Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::Expected;
use anomi_vm::people::editor_preview::{EditorPreview, Intent, Page};
use anomi_vm::people::full_profile::{Game, Widget, Wish};
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::{tr, trn};

use super::look::{Look, Shade};

const GAP: f32 = 12.0;
const RADIUS: f32 = 12.0;
const WISH_MAX: f32 = 168.0;
/// Three cards to a row where they are 120 px wide at least, else two.
const WISH_LEAST: f32 = 120.0;
const WISH_NAME: f32 = 26.0;
const WISH_LINE: f32 = 16.0;
const WISH_PADDING: f32 = 10.0;
const WISH_GAP: f32 = 8.0;

/// What the shown tab holds, in a column `width` wide.
pub fn content<'a>(
    preview: &'a EditorPreview,
    width: f32,
    look: Look,
    motion: Motion,
) -> Element<'a, Intent> {
    if preview.blank() {
        return empty(preview, look);
    }
    let soon = || -> Element<'a, Intent> {
        look.words(
            tr!("Adding more from Yapfast is coming soon."),
            typography::size::SMALL,
            typography::UI,
            Shade::Ink2,
        )
        .boxed()
    };
    let mut page = Column::<Element<'a, Intent>>::new()
        .spacing(GAP)
        .width(Fill);
    match preview.page {
        Page::Board => {
            if !preview.board_note.is_empty() {
                page = page.push(note(&preview.board_note, look));
            }
            page = page.push(title(tr!("Your Widgets"), look));
            page = page.push(
                Column::with_children(
                    preview
                        .widgets
                        .iter()
                        .map(|widget| widget_card(widget, look, motion)),
                )
                .spacing(GAP)
                .boxed(),
            );
            if preview.board_note.is_empty() {
                page = page.push(soon());
            }
        }
        Page::Activity => {
            let shown = Look::activity(preview);
            for entry in &preview.activities {
                page = page.push(
                    crate::activity::card::view(entry, preview.now, shown, false, motion)
                        .map(|intent| match intent {
                            anomi_vm::people::activity::card::Intent::Link(url) => {
                                Intent::Link(url)
                            }
                            _ => Intent::Choose(Page::Activity),
                        })
                        .boxed(),
                );
            }
        }
        Page::Wishlist => {
            page = page.push(title(
                trn!("{n} Item", "{n} Items", preview.wishlist.len()),
                look,
            ));
            if !preview.wishlist_note.is_empty() {
                page = page.push(note(&preview.wishlist_note, look));
            }
            page = page.push(wishlist(&preview.wishlist, width, look, motion));
            page = page.push(soon());
        }
    }
    page.boxed()
}

fn title<'a, M: 'a>(words: impl text::IntoFragment<'a>, look: Look) -> Element<'a, M> {
    look.words(
        words,
        typography::size::LABEL,
        typography::MEDIUM,
        Shade::Ink,
    )
    .boxed()
}

fn note<'a, M: 'a>(words: &'a str, look: Look) -> Element<'a, M> {
    look.words(words, typography::size::SMALL, typography::UI, Shade::Ink2)
        .boxed()
}

/// Yapfast's words for an empty tab, in the middle of the column
/// (`ProfileTabEmpty`, `ui/people.slint:2046`).
fn empty<'a>(preview: &'a EditorPreview, look: Look) -> Element<'a, Intent> {
    let soon = tr!("Adding more from Yapfast is coming soon.");
    let (heading, body) = match preview.page {
        Page::Board if preview.board_note.is_empty() => (tr!("Nothing on your board yet."), soon),
        Page::Board => ("", preview.board_note.as_str()),
        Page::Activity => (
            tr!("You don't have any activity here"),
            tr!(
                "Connect your accounts to share your in-game status and activity. You're in control of what you share."
            ),
        ),
        Page::Wishlist if preview.wishlist_note.is_empty() => {
            (tr!("Nothing on your wishlist yet."), soon)
        }
        Page::Wishlist => ("", preview.wishlist_note.as_str()),
    };
    let mut lines = Column::<Element<'a, Intent>>::new()
        .spacing(8)
        .align_x(Alignment::Center)
        .padding(Padding::ZERO.left(32).right(32));
    if !heading.is_empty() {
        lines = lines.push(
            look.words(
                heading,
                typography::size::TITLE,
                typography::BOLD,
                Shade::Ink,
            )
            .align_x(Alignment::Center)
            .boxed(),
        );
    }
    lines = lines.push(
        look.words(body, typography::size::SMALL, typography::UI, Shade::Ink2)
            .align_x(Alignment::Center)
            .boxed(),
    );
    container(lines).center(Fill).boxed()
}

/// A tile: a card of the column's width.
fn tile<'a, M: 'a>(content: Element<'a, M>, padding: f32, look: Look) -> Element<'a, M> {
    sized(content, padding, Fill, Length::Shrink, look)
}

fn sized<'a, M: 'a>(
    content: Element<'a, M>,
    padding: f32,
    width: impl Into<Length>,
    height: impl Into<Length>,
    look: Look,
) -> Element<'a, M> {
    container(content)
        .width(width)
        .height(height)
        .padding(padding)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(look.tile(theme.palette()))),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// A picture of the board or the wishlist, contained in its room: the room
/// is empty while the picture is on its way and holds the initial when
/// there is none.
#[allow(clippy::too_many_arguments)]
fn showcase<'a, M: 'a>(
    shown: &'a Expected,
    initial: &'a str,
    size: (f32, f32),
    initial_size: f32,
    well: bool,
    look: Look,
    motion: Motion,
) -> Element<'a, M> {
    let inner: Element<'a, M> = match shown {
        Expected::Ready(handle) => picture(handle.clone())
            .width(size.0)
            .height(size.1)
            .fit(Fit::Contain)
            .radius(8.0)
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
        .style(move |theme: &Theme| container::Style {
            background: well.then(|| Background::Color(look.well(theme.palette()))),
            border: Border::default().rounded(8.0),
            ..container::Style::default()
        })
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
    tile(body.boxed(), 14.0, look)
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
            true,
            look,
            motion,
        ))
        .push(lines.boxed())
        .spacing(10)
        .align_y(Alignment::Center)
        .boxed()
}

/// The wishlist's items, three to a row where they fit, a name's height
/// taller than wide, at most 168 px wide.
fn wishlist<'a, M: 'a>(
    items: &'a [Wish],
    width: f32,
    look: Look,
    motion: Motion,
) -> Element<'a, M> {
    let columns = if width >= 3.0 * WISH_LEAST + 2.0 * GAP {
        3
    } else {
        2
    };
    let side = ((width - (columns - 1) as f32 * GAP) / columns as f32).clamp(0.0, WISH_MAX);
    let mut grid = Column::<Element<'a, M>>::new().spacing(GAP);
    for line in items.chunks(columns) {
        grid = grid.push(
            Row::with_children(line.iter().map(|wish| wish_card(wish, side, look, motion)))
                .spacing(GAP)
                .boxed(),
        );
    }
    grid.boxed()
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
            false,
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
    sized(body.boxed(), WISH_PADDING, side, side + WISH_NAME, look)
}
