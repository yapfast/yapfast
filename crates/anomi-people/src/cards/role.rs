//! A role mentioned in a message, as a card: who Yapfast knows has it. The
//! role's dot and name, a note while it loads or when no one is known, and
//! the people with it, a click on whom opens their card beside the row
//! (`RoleCard`, `ui/people.slint:2327`). 260 px wide, as tall as its
//! content up to the window's room.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{Column, container, hover, row, space, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Rectangle};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::people::cards::{Intent, Member, ROLE_WIDTH, Role};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::list::list;
use anomi_widgets::menu::opener;

use super::surface;

/// The card's padding and the space between its parts.
const PADDING: f32 = metrics::SPACE_M;
const GAP: f32 = metrics::SPACE_XS;
/// The heading: a 12 px dot and the name at 16 px, 8 px apart, with 4 px
/// under it (`HorizontalLayout { spacing: 8px; padding-bottom: 4px }`).
const DOT: f32 = 12.0;
const NAME: f32 = typography::size::TITLE;
const HEADING: f32 = 24.0;
/// A note's line (13 px text on a 16 px line, the `Label` of
/// `ui/theme.slint:158`).
const NOTE: f32 = 18.0;
/// A person's row (`height: 40px`, `border-radius: 6px`), its sides, the
/// space between its avatar and name, and the avatar's side.
const ROW: f32 = 40.0;
const ROW_SIDE: f32 = 6.0;
const ROW_GAP: f32 = metrics::SPACE_S + 2.0;
const FACE: f32 = 28.0;
/// The list's tallest (`ScrollView { max-height: 320px }`).
const LIST: f32 = 320.0;

pub fn view(role: &Role, motion: Motion) -> Element<'_, Intent> {
    let mut card = Column::<Element<'_, Intent>>::new()
        .push(heading(role))
        .spacing(GAP);
    let mut room = role.limit - 2.0 * PADDING - HEADING - 2.0 - GAP;
    if !role.note.is_empty() {
        card = card.push(
            text(role.note.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .width(Fill)
                .boxed(),
        );
        room -= NOTE + GAP;
    }
    if !role.members.is_empty() {
        let members = &role.members;
        let height = (members.len() as f32 * ROW).min(LIST).min(room.max(ROW));
        card = card.push(
            container(list(
                members.len(),
                |at| key(&members[at]),
                move |at| person(&members[at], motion),
                motion,
            ))
            .height(height)
            .boxed(),
        );
    }
    surface(
        container(card).padding(PADDING).boxed(),
        ROLE_WIDTH,
        metrics::RADIUS_CARD,
        |theme| theme.palette().raised,
    )
}

/// The dot and the name, in the role's colour when it has one.
fn heading(role: &Role) -> Element<'_, Intent> {
    let color = role.color;
    let ink = move |theme: &Theme| color.unwrap_or(theme.palette().text);
    row![
        container(space())
            .width(DOT)
            .height(DOT)
            .style(move |theme: &Theme| container::Style {
                background: Some(Background::Color(color.unwrap_or(theme.palette().muted))),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            }),
        text(format!("@{}", role.name))
            .size(NAME)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme)),
                ..text::Style::default()
            })
            .width(Fill),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center)
    .padding(Padding {
        bottom: GAP,
        ..Padding::ZERO
    })
    .boxed()
}

fn key(member: &Member) -> u64 {
    let mut hasher = DefaultHasher::new();
    member.id.hash(&mut hasher);
    hasher.finish()
}

/// One person: their avatar and name, washed while the pointer is on the
/// row; a click opens their card beside the row.
fn person(member: &Member, motion: Motion) -> Element<'_, Intent> {
    let rest = body(member, false, Motion::default());
    let over = body(member, true, motion);
    opener(hover(rest, over))
        .on_press(move |bounds| {
            let card = Rectangle {
                x: bounds.x - PADDING,
                width: ROLE_WIDTH,
                ..bounds
            };
            Intent::OpenMember(member.id.clone(), Spot::beside(card))
        })
        .boxed()
}

fn body(member: &Member, lit: bool, motion: Motion) -> Element<'_, Intent> {
    let color: Option<Color> = member.color;
    let content = row![
        portrait(&member.avatar, FACE)
            .ground(Ground::Panel)
            .motion(motion)
            .view(),
        text(member.name.as_str())
            .size(typography::size::BODY)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| text::Style {
                color: Some(color.unwrap_or(theme.palette().text)),
                ..text::Style::default()
            })
            .width(Fill),
    ]
    .spacing(ROW_GAP)
    .align_y(Alignment::Center);
    container(content)
        .padding([0.0, ROW_SIDE])
        .width(Fill)
        .height(ROW)
        .align_y(Alignment::Center)
        .style(move |theme: &Theme| container::Style {
            background: lit.then(|| Background::Color(theme.palette().hover)),
            border: Border::default().rounded(metrics::RADIUS_ROW),
            ..container::Style::default()
        })
        .boxed()
}
