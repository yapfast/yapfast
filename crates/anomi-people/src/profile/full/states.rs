//! A tab with nothing to list: rows the shape of what's coming while Discord
//! hasn't said, then its words in the middle of the column
//! (`ProfileTabEmpty`, `ui/people.slint:2046`; the rows at `:2224`).

use std::borrow::Cow;

use iced::widget::{Column, Row, container};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::people::full_profile::{FullProfile, Page};
use anomi_vm::people::profile::Intent as CardIntent;
use anomi_vm::people::{self};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::skeleton::skeleton;
use yapfast_lang::tr;

use super::look::{Look, Shade};

/// The widths of the lines of the rows still to come, the second line 60%
/// of the first.
const LINES: [f32; 6] = [180.0, 140.0, 210.0, 120.0, 170.0, 150.0];
const FACE: f32 = 40.0;

/// Six rows the shape of what's coming; a server's face is square.
pub fn loading<'a, M: 'a>(page: Page, motion: Motion) -> Element<'a, M> {
    let radius = if page == Page::Servers { 12.0 } else { 20.0 };
    Column::with_children(LINES.iter().map(|width| {
        let lines = Column::<Element<'a, M>>::new()
            .spacing(8)
            .push(
                skeleton(motion)
                    .width(*width)
                    .height(12)
                    .radius(6.0)
                    .boxed(),
            )
            .push(
                skeleton(motion)
                    .width(width * 0.6)
                    .height(10)
                    .radius(5.0)
                    .boxed(),
            );
        Row::<Element<'a, M>>::new()
            .spacing(12)
            .align_y(Alignment::Center)
            .push(
                skeleton(motion)
                    .width(FACE)
                    .height(FACE)
                    .radius(radius)
                    .boxed(),
            )
            .push(lines.boxed())
            .boxed()
    }))
    .spacing(12)
    .boxed()
}

/// Discord's words for an empty tab, in the middle of the column.
pub fn empty<'a>(full: &'a FullProfile, look: Look, motion: Motion) -> Element<'a, people::Intent> {
    let card = &full.card;
    let (heading, body) = words(full);
    let mut lines = Column::<Element<'a, people::Intent>>::new()
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
    if full.page() == Page::Activity && !card.own && card.can_message {
        lines = lines.push(
            container(
                action_button(tr!("Message"), Kind::Primary, motion)
                    .leading(Icon::MessageCircle)
                    .on_press(CardIntent::Message(card.id.clone()).into()),
            )
            .padding(Padding::ZERO.top(12))
            .boxed(),
        );
    }
    container(lines).center(Fill).boxed()
}

/// The heading and the line under it, as the client words them; a bot's
/// profile words its servers its own way.
fn words(full: &FullProfile) -> (Cow<'_, str>, Cow<'_, str>) {
    let card = &full.card;
    match full.page() {
        Page::Activity if card.own => (
            tr!("You don't have any activity here").into(),
            tr!("Connect your accounts to share your in-game status and activity. You're in control of what you share.").into(),
        ),
        Page::Activity => (
            tr!("{} doesn't have any activity to share here", card.name).into(),
            tr!("Looks like this profile is still a work in progress - send them a message to say hi!").into(),
        ),
        Page::Friends => (
            tr!("You don't have any friends in common").into(),
            tr!("No mutual friends yet - perfect time to make a new connection").into(),
        ),
        Page::Servers if card.bot => ("".into(), tr!("No servers in common").into()),
        Page::Servers => (
            tr!("You don't have any servers in common").into(),
            tr!("Time to impress them with your taste...drop an invite link to your favorite server").into(),
        ),
        Page::Board => (
            "".into(),
            if full.board_note.is_empty() {
                tr!("Nothing on the board yet.").into()
            } else {
                full.board_note.as_str().into()
            },
        ),
        Page::Wishlist => (
            "".into(),
            if !full.wishlist_note.is_empty() {
                full.wishlist_note.as_str().into()
            } else if card.own {
                tr!("Nothing on your wishlist yet.").into()
            } else {
                tr!("Nothing on their wishlist.").into()
            },
        ),
    }
}
