//! A post's card in the list of a forum: its tags, its title, who wrote its
//! first message and how it starts, then its top reaction, its replies and
//! when it was last active; its first picture on the right. The Slint side:
//! `PostCard`, `TagPill`, `PostEmoji` and `StoredImage` (`ui/forum.slint:92`,
//! `:74`, `:61`, `:43`). A heading above a later section sits 24 px under
//! the card before it and close over its own (`:99`).

mod counts;
mod press;

use iced::font::Weight;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding};
use iced_runtime::core::length::Constraint;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::forum::{Intent, Mark, Post, Tag};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use counts::counts;
use press::Press;

/// The card's height with tags and without (`card`, `:98`).
const WITH_TAGS: f32 = 120.0;
const WITHOUT_TAGS: f32 = 96.0;
/// Between a card and what follows, and from the row's sides (`:99`, `:107`).
const BELOW: f32 = 8.0;
const SIDE: f32 = 16.0;
/// Above a heading: the first row's, and a later one's; the heading's own
/// height (`:99`-`:102`).
const HEADING_FIRST: f32 = 8.0;
const HEADING_LATER: f32 = 16.0;
const HEADING: f32 = 24.0;
/// Above a first card with no heading (`:99`).
const TOP: f32 = 16.0;
/// The card's inner padding, its left being wider, and the gap between its
/// text and its picture (`:124`).
const PAD: f32 = 12.0;
const PAD_LEFT: f32 = 16.0;
const GAP: f32 = 16.0;
/// Between the card's lines (`:126`).
const LINES: f32 = 6.0;
/// The thumbnail (`:192`).
const THUMBNAIL: f32 = 72.0;
/// The tag pill's height and its emoji (`:76`, `:81`), the heights of the
/// author's line and the counts' (`:156`, `:167`), the reaction chip's
/// radius (`:170`), the NEW badge's height (`:149`), the glyphs (`:140`).
const PILL: f32 = 20.0;
const PILL_EMOJI: f32 = 12.0;
/// Slint's 16 px title line is 20.8 px; its layout lands on whole pixels, so
/// the title row is 21 (`:135`).
const TITLE_LINE: f32 = 21.0;
/// The NEW badge's letter spacing (`letter-spacing: 0.3px`, `:150`), added to
/// the sides as the foundation's pills do.
const TRACKING: f32 = 0.3;
const AUTHOR_LINE: f32 = 18.0;
const COUNTS_LINE: f32 = 22.0;
const CHIP_RADIUS: f32 = 6.0;
const BADGE: f32 = 16.0;
const GLYPH: f32 = 14.0;
/// The pill's and the chip's spacing between emoji and text (`:80`, `:173`).
const INNER: f32 = 4.0;
/// The NEW badge: its text at 10 px, white, and its sides, 10 px wider than
/// the text (`:149`-`:150`); the SPOILER word at 10 px (`:196`); the chip's
/// sides (`:172`).
const SMALL_CAPS: f32 = 10.0;
const BADGE_SIDE: f32 = 5.0;
const CHIP_SIDE: f32 = 6.0;
const BADGE_TEXT: Color = Color::WHITE;
/// An emoji's text is a little smaller than its box (`font-size: size * 0.95`).
const EMOJI_TEXT: f32 = 0.95;

/// The list's entry: a heading, a card, or both.
pub(super) fn view<'a>(post: &'a Post, first: bool, motion: Motion) -> Element<'a, Intent> {
    let top = match (post.heading.is_empty(), first) {
        (false, true) => HEADING_FIRST,
        (false, false) => HEADING_LATER,
        (true, true) => TOP,
        (true, false) => 0.0,
    };
    let mut rows: Vec<Element<'a, Intent>> = Vec::new();
    if !post.heading.is_empty() {
        rows.push(
            text(post.heading.as_str())
                .size(typography::size::CAPTION)
                .font(typography::BOLD)
                .style(catalog::text::muted)
                .height(HEADING)
                .align_y(Alignment::Center)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        );
    }
    let bottom = if post.room.is_empty() {
        0.0
    } else {
        rows.push(card(post, motion));
        BELOW
    };
    container(column(rows).width(Fill))
        .padding(Padding {
            top,
            right: SIDE,
            bottom,
            left: SIDE,
        })
        .width(Fill)
        .boxed()
}

fn card<'a>(post: &'a Post, motion: Motion) -> Element<'a, Intent> {
    let height = if post.tags.is_empty() {
        WITHOUT_TAGS
    } else {
        WITH_TAGS
    };
    let mut lines: Vec<Element<'a, Intent>> = Vec::new();
    if !post.tags.is_empty() {
        lines.push(tags(post, motion).boxed());
    }
    lines.push(title(post).boxed());
    lines.push(author(post).boxed());
    lines.push(counts(post, motion).boxed());
    let lines = column(lines).spacing(LINES).width(Fill);
    // Slint starts the lines at the top padding and lets them run into the
    // bottom one (3 px with tags, 1 px without): the card keeps no bottom
    // padding, so the lines keep their heights, and the thumbnail carries it
    // to stay centred in the card.
    let mut body = row![container(lines).align_top(Fill).width(Fill)]
        .spacing(GAP)
        .align_y(Alignment::Center);
    if let Some(side) = thumbnail(post, motion) {
        body = body.push(
            container(side)
                .padding(Padding::default().bottom(PAD))
                .boxed(),
        );
    }
    Press::new(
        container(body)
            .padding(Padding {
                top: PAD,
                right: PAD,
                bottom: 0.0,
                left: PAD_LEFT,
            })
            .height(height)
            .width(Fill),
        motion,
        move |beside| Intent::Open {
            room: post.room.clone(),
            beside,
        },
    )
    .boxed()
}

/// An emoji on a tag or a reaction: a Unicode one's text, a custom one's
/// picture, or the room its picture will take (`PostEmoji`, `:61`).
pub(super) fn mark<'a, M: 'a>(mark: &'a Mark, size: f32, motion: Motion) -> Option<Element<'a, M>> {
    match mark {
        Mark::None => None,
        Mark::Unicode(emoji) if emoji.is_empty() => None,
        Mark::Unicode(emoji) => Some(
            text(emoji.as_str())
                .size(size * EMOJI_TEXT)
                .wrapping(text::Wrapping::None)
                .boxed(),
        ),
        Mark::Custom(Expected::Ready(handle)) => Some(
            picture(handle.clone())
                .side(size)
                .fit(Fit::Contain)
                .motion(motion)
                .boxed(),
        ),
        Mark::Custom(_) => Some(space().width(size).height(size).boxed()),
    }
}

/// The first tags as pills, and "+2" for the others (`:127`-`:134`).
fn tags<'a>(post: &'a Post, motion: Motion) -> impl View<Intent> + 'a {
    let mut items: Vec<Element<'a, Intent>> = post
        .tags
        .iter()
        .map(|tag| pill(tag, motion).boxed())
        .collect();
    if !post.more_tags.is_empty() {
        items.push(
            text(post.more_tags.as_str())
                .size(typography::size::CAPTION)
                .font(typography::SEMIBOLD)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    row(items)
        .spacing(LINES)
        .align_y(Alignment::Center)
        .height(PILL)
}

/// A tag as a pill, a step off the card whether or not it is hovered
/// (`TagPill`, `:74`).
fn pill<'a>(tag: &'a Tag, motion: Motion) -> impl View<Intent> + 'a {
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    items.extend(mark(&tag.mark, PILL_EMOJI, motion));
    items.push(
        text(tag.name.as_str())
            .size(typography::size::CAPTION)
            .font(typography::SEMIBOLD)
            .style(catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .boxed(),
    );
    container(row(items).spacing(INNER).align_y(Alignment::Center))
        .padding([0.0, metrics::SPACE_S])
        .height(PILL)
        .center_y(PILL)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().active)),
            border: Border::default().rounded(PILL / 2.0),
            ..container::Style::default()
        })
}

/// A muted glyph (`colorize: Theme.muted`).
fn glyph<'a>(which: Icon) -> Element<'a, Intent> {
    icon(which, GLYPH)
        .style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().muted),
        })
        .boxed()
}

/// The pin, the title and NEW (`:135`-`:154`).
fn title<'a>(post: &'a Post) -> impl View<Intent> + 'a {
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if post.pinned {
        items.push(glyph(Icon::Pin));
    }
    let read = !(post.unread || post.new);
    items.push(
        text(post.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .style(if read {
                catalog::text::secondary
            } else {
                catalog::text::primary
            })
            .width(Length::Fluid(Constraint::Max))
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
    );
    if post.new {
        items.push(new_badge().boxed());
    }
    row(items)
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center)
        .height(TITLE_LINE)
}

/// The accent pill after a new post's title (`:147`-`:152`).
fn new_badge<'a>() -> impl View<Intent> + 'a {
    let word = tr!("NEW");
    let side = BADGE_SIDE + TRACKING * word.chars().count() as f32 / 2.0;
    container(
        text(word)
            .size(SMALL_CAPS)
            .font(typography::weight(Weight::ExtraBold))
            .style(|_: &Theme| text::Style {
                color: Some(BADGE_TEXT),
                ..text::Style::default()
            })
            .wrapping(text::Wrapping::None),
    )
    .padding([0.0, side])
    .height(BADGE)
    .center_y(BADGE)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().accent)),
        border: Border::default().rounded(BADGE / 2.0),
        ..container::Style::default()
    })
}

/// Who wrote the first message, in their role colour, and how it starts
/// (`:155`-`:165`).
fn author<'a>(post: &'a Post) -> impl View<Intent> + 'a {
    let muted = |content: &'a str| {
        text(content)
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
    };
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if !post.person.is_empty() {
        let color = post.color;
        items.push(
            text(post.person.as_str())
                .size(typography::size::SMALL)
                .font(typography::SEMIBOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(color.unwrap_or(theme.palette().secondary)),
                    ..text::Style::default()
                })
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        );
        if !post.text.is_empty() {
            items.push(muted(": ").wrapping(text::Wrapping::None).boxed());
        }
    }
    items.push(
        muted(post.text.as_str())
            .width(Fill)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
    );
    row(items).align_y(Alignment::Center).height(AUTHOR_LINE)
}

/// The first picture, 72 px with its corners round, or the word for one
/// that is a spoiler (`:190`-`:199`). A picture on its way keeps its room.
fn thumbnail<'a>(post: &'a Post, motion: Motion) -> Option<Element<'a, Intent>> {
    let handle = match &post.picture {
        Expected::None => return None,
        Expected::Waiting => None,
        Expected::Ready(handle) => Some(handle),
    };
    let ground = |theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().raised)),
        border: Border::default().rounded(metrics::RADIUS_CARD),
        ..container::Style::default()
    };
    let inside: Element<'a, Intent> = match handle {
        _ if post.spoiler => text(tr!("SPOILER"))
            .size(SMALL_CAPS)
            .font(typography::weight(Weight::ExtraBold))
            .style(catalog::text::muted)
            .boxed(),
        Some(handle) => picture(handle.clone())
            .side(THUMBNAIL)
            .radius(metrics::RADIUS_CARD)
            .fit(Fit::Cover)
            .motion(motion)
            .boxed(),
        None => space().boxed(),
    };
    Some(
        container(inside)
            .width(THUMBNAIL)
            .height(THUMBNAIL)
            .center(THUMBNAIL)
            .style(ground)
            .boxed(),
    )
}
