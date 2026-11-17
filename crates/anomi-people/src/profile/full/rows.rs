//! The mutual tabs' rows (`ProfileListRow`, `ui/people.slint:1079`): a
//! person's round face with its status dot, or a server's rounded square,
//! its name in 15 px medium and, for a server they go by a name in, that
//! name under it. A click opens the friend's full profile or the server's
//! card beside the row.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{Column, Row, button, container, hover, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::{Avatar, Expected, Presence};
use anomi_vm::people::full_profile::{Friend, Intent, Server};
use anomi_widgets::avatar::portrait;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};

use super::look::Look;

/// A row's height, taller with a line under its name.
pub const PLAIN: f32 = 40.0;
pub const DETAILED: f32 = 46.0;
const RADIUS: f32 = 6.0;
const INSET: f32 = 8.0;
const GAP: f32 = 12.0;
const FACE: f32 = 32.0;
const MARK_RADIUS: f32 = 10.0;

/// A row's identity, which stays with it as others come and go.
pub fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// What stands before the name.
#[derive(Clone, Copy)]
enum Face<'a> {
    Person(&'a Avatar, Presence),
    Server(&'a str, &'a Expected),
}

/// A mutual friend's row: it opens their full profile.
pub fn friend<'a>(friend: &'a Friend, look: Look, motion: Motion) -> Element<'a, Intent> {
    let face = Face::Person(&friend.avatar, friend.presence);
    let (rest, over) = (
        body(face, &friend.name, "", look, false, motion),
        body(face, &friend.name, "", look, true, motion),
    );
    button(hover(rest, over))
        .padding(0)
        .width(Fill)
        .style(catalog::button::bare)
        .on_press(Intent::OpenFriend(friend.id.clone()))
        .boxed()
}

/// A mutual server's row: it opens the server's card beside the row.
pub fn server<'a>(server: &'a Server, look: Look, motion: Motion) -> Element<'a, Intent> {
    let face = Face::Server(&server.initial, &server.icon);
    let (rest, over) = (
        body(face, &server.name, &server.nick, look, false, motion),
        body(face, &server.name, &server.nick, look, true, motion),
    );
    let id = server.id.clone();
    opener(hover(rest, over))
        .on_press(move |bounds| Intent::OpenServer(id.clone(), bounds))
        .boxed()
}

/// The row at rest or under the pointer, which takes the hover ground and
/// cuts the dot's ring from it.
fn body<'a>(
    face: Face<'a>,
    name: &'a str,
    detail: &'a str,
    look: Look,
    hovered: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mark: Element<'a, Intent> = match face {
        Face::Person(avatar, presence) => portrait(avatar, FACE)
            .presence(presence)
            .ground(look.ring(hovered))
            .motion(motion)
            .view()
            .boxed(),
        Face::Server(initial, icon) => server_mark(initial, icon, look, motion),
    };
    let mut lines = Column::<Element<'a, Intent>>::new().width(Fill).push(
        text(name)
            .size(typography::size::BODY)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| text::Style {
                color: Some(look.ink(theme.palette())),
                ..text::Style::default()
            })
            .boxed(),
    );
    if !detail.is_empty() {
        lines = lines.push(
            text(detail)
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(move |theme: &Theme| text::Style {
                    color: Some(look.ink3(theme.palette())),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    let content = Row::<Element<'a, Intent>>::new()
        .spacing(GAP)
        .align_y(Alignment::Center)
        .padding(Padding::ZERO.left(INSET).right(INSET))
        .push(mark)
        .push(lines.boxed());
    container(content)
        .center_y(if detail.is_empty() { PLAIN } else { DETAILED })
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: hovered.then(|| Background::Color(look.hover(theme.palette()))),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// A server's icon, else its initial on the raised ground.
fn server_mark<'a>(
    initial: &'a str,
    icon: &'a Expected,
    look: Look,
    motion: Motion,
) -> Element<'a, Intent> {
    match icon {
        Expected::Ready(handle) => picture(handle.clone())
            .side(FACE)
            .fit(Fit::Cover)
            .radius(MARK_RADIUS)
            .motion(motion)
            .boxed(),
        _ => container(text(initial).size(13).font(typography::BOLD).style(
            move |theme: &Theme| text::Style {
                color: Some(look.ink(theme.palette())),
                ..text::Style::default()
            },
        ))
        .center(FACE)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default().rounded(MARK_RADIUS),
            ..container::Style::default()
        })
        .boxed(),
    }
}
