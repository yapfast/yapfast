//! The invite dialog's first page, as Discord's: where the invite lands
//! people, a field that finds whom to send it to, the notice of a private
//! channel, the people (see [`rows`](super::rows)), and at its foot, under
//! a rule, the link to copy with how long it lasts and the way to its
//! settings. The card fits a short window: the list gives up its room, the
//! foot keeps its own.

use iced::widget::{column, container, mouse_area, rich_text, row, space, span, stack, svg, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding};

use anomi_theme::metrics::{SPACE_L, SPACE_M, SPACE_S, SPACE_XS};
use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::places::invite::{Intent, Invite, Page};
use anomi_widgets::field::{Id, field};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

use super::frame::{self, PADDING, copyable, failure};
use super::rows;

pub use super::rows::LIST;

/// The search field's id: the app gives it the keys as the dialog opens.
pub const SEARCH: &str = "invite-search";
/// The field's height and padding, as the new DM's.
const FIELD: f32 = 36.0;
const FIELD_PAD: f32 = 10.0;
/// The tallest the card grows to: the list scrolls in what the head and
/// the foot leave of it, or of a shorter window.
const TALLEST: f32 = 640.0;
/// The channel's icon after "Recipients will land in", and the private
/// notice's dot.
const CHANNEL_ICON: f32 = 16.0;
const DOT: f32 = 8.0;

pub fn view(invite: &Invite, width: f32, motion: Motion) -> Element<'_, Intent> {
    let search = field(tr!("Search for friends"), &invite.query)
        .id(Id::new(SEARCH))
        .on_input(Intent::Search)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(FIELD)
        .trailing(Icon::Search, None)
        .motion(motion)
        .view();
    let mut finding = column![search].spacing(SPACE_S);
    if invite.private {
        finding = finding.push(private());
    }
    let head = column![
        column![
            frame::bar(&invite.title, Intent::Close, motion),
            lands(invite)
        ]
        .spacing(SPACE_XS),
        finding,
    ]
    .spacing(SPACE_L);
    let mut card = column![container(head).padding(Padding::from(PADDING).bottom(SPACE_M))];
    if let Some(people) = rows::view(invite, motion) {
        card = card.push(people);
    }
    card = card.push(rule()).push(foot(invite, motion));
    let card = container(card.height(Length::Fit))
        .width(width)
        .height(Length::Fit.max(TALLEST))
        .style(frame::card);
    // The layer centres this box, so the card's top stays where it opened
    // as a search shortens the list, the card growing downwards; a press
    // under the card closes, as the backdrop does.
    column![
        card,
        mouse_area(space().width(Fill).height(Fill)).on_press(Intent::Close)
    ]
    .width(width)
    .height(TALLEST)
    .boxed()
}

/// "Recipients will land in" the channel, with its icon.
fn lands(invite: &Invite) -> Element<'_, Intent> {
    let muted = |words: &str| {
        text(words.to_owned())
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
    };
    let mut line = row![muted(tr!("Recipients will land in"))]
        .spacing(SPACE_XS)
        .align_y(Alignment::Center);
    if let Some(shown) = invite.icon {
        let mark = icon(shown, CHANNEL_ICON).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().muted),
        });
        line = line.push(mark.boxed());
    }
    let channel = text(invite.channel.as_str())
        .size(typography::size::SMALL)
        .font(typography::MEDIUM)
        .style(catalog::text::secondary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    line.push(channel.boxed()).boxed()
}

/// The private channel's notice, behind the theme's loud dot.
fn private<'a>() -> Element<'a, Intent> {
    let dot = container(space())
        .width(DOT)
        .height(DOT)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().loud)),
            border: Border::default().rounded(DOT / 2.0),
            ..container::Style::default()
        });
    let words = text(tr!(
        "This channel is private. Only select members and roles can view it."
    ))
    .size(typography::size::CAPTION)
    .style(catalog::text::muted)
    .width(Fill);
    row![container(dot).padding(Padding::ZERO.top(SPACE_XS)), words]
        .spacing(SPACE_S)
        .align_y(Alignment::Start)
        .boxed()
}

/// The line between the people and the link, the card's width.
fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        })
        .boxed()
}

/// The link to copy, how long it lasts and how often it works, and the
/// way to its settings; why making it failed, if it did.
fn foot(invite: &Invite, motion: Motion) -> Element<'_, Intent> {
    let link = if invite.making {
        frame::line(tr!("Making a link..."), typography::size::BODY, true)
    } else {
        frame::line(&invite.link, typography::size::BODY, false)
    };
    let mut foot = column![
        text(tr!("Or, send a server invite link to a friend"))
            .size(typography::size::SMALL)
            .font(typography::MEDIUM),
        copyable(
            link,
            true,
            invite.copied,
            !invite.link.is_empty(),
            Intent::Copy(invite.link.clone()),
            motion,
        ),
    ]
    .spacing(SPACE_S);
    if !invite.error.is_empty() {
        foot = foot.push(failure(&invite.error));
    }
    foot.push(lasts(&invite.note))
        .padding(Padding::from(PADDING).top(SPACE_L))
        .boxed()
}

/// How long the link lasts, then "Edit invite link." in the link's colour,
/// which turns to the settings. The two colours are two layers of the same
/// words, each leaving the other's part clear, so they wrap as one line.
fn lasts(note: &str) -> Element<'_, Intent> {
    let edit = tr!("Edit invite link.");
    let gap = if note.is_empty() { "" } else { " " };
    let words = rich_text([span(note), span(gap), span(edit).color(Color::TRANSPARENT)])
        .on_link_click(iced::never)
        .size(typography::size::CAPTION)
        .width(Fill)
        .style(catalog::text::muted);
    let link = rich_text([
        span(note).color(Color::TRANSPARENT),
        span(gap),
        span(edit).link(Page::Link),
    ])
    .on_link_click(Intent::Show)
    .size(typography::size::CAPTION)
    .width(Fill)
    .style(catalog::text::link);
    stack![words, link].boxed()
}
