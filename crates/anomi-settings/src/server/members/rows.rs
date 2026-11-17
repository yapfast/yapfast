//! A member's row of the Members table and its parts: the name over the
//! username, the times, the way they joined, the role's chip, the signals
//! and More Options. The Slint side: `MembersSection`'s rows and `RoleChip`
//! (`ui/server_people.slint:117`, `:98`).

use iced::widget::{column, container, row, space, stack, text};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::settings::members::{Intent, Member, Members};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::icon_button;
use anomi_widgets::menu::opener;
use anomi_widgets::tip::tip_icon;
use yapfast_lang::tr;

use crate::kit::{Ink, Leaf, mix};

/// A row's height; its hover reaches this far past the columns (the list
/// does, see `reach`), so their contents start on the column's edge under the
/// names above.
pub const HEIGHT: f32 = 56.0;
pub const OUTSET: f32 = 8.0;
/// The gap between columns, the avatar, and the chip's parts.
pub const GAP: f32 = 8.0;
const AVATAR: f32 = 32.0;
const BESIDE: f32 = 10.0;
const CHIP: f32 = 24.0;
const DOT: f32 = 10.0;
/// More Options' side, and its glyph.
pub const MORE: f32 = 32.0;
const MORE_GLYPH: f32 = 18.0;

/// How the columns share the width, doubled from Slint's 4, 2, 2, 2, 2 and
/// 1.5 so each is whole.
pub const NAME: u16 = 8;
pub const SINCE: u16 = 4;
pub const AGE: u16 = 4;
pub const JOIN: u16 = 4;
pub const ROLES: u16 = 4;
pub const SIGNALS: u16 = 3;

/// A column that takes its share and cuts what doesn't fit.
pub fn cell<'a>(share: u16, content: impl View<Intent> + 'a) -> Element<'a, Intent> {
    container(content.boxed())
        .width(Length::FillPortion(share))
        .clip(true)
        .boxed()
}

fn line<'a>(words: &'a str, size: f32) -> iced::widget::Text<'a, Theme> {
    text(words)
        .size(size)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}

/// The role as a chip: its colour's dot and name, then "+2" for more.
fn role<'a>(member: &'a Member) -> Element<'a, Intent> {
    if member.role.is_empty() {
        return space().boxed();
    }
    let color = member.role_color;
    let dot = container(space())
        .width(DOT)
        .height(DOT)
        .style(move |_: &Theme| container::Style {
            background: Some(color.into()),
            border: Border::default().rounded(DOT / 2.0),
            ..container::Style::default()
        });
    let chip = container(
        row![
            dot.boxed(),
            line(&member.role, typography::size::CAPTION)
                .style(catalog::text::primary)
                .boxed()
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .height(CHIP)
    .padding(iced::Padding {
        left: 6.0,
        right: 8.0,
        ..iced::Padding::ZERO
    })
    .align_y(iced::alignment::Vertical::Center)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().lift(0.08).into()),
        border: Border::default().rounded(4),
        ..container::Style::default()
    });
    let mut parts = row![chip.boxed()].spacing(4).align_y(Alignment::Center);
    if !member.more_roles.is_empty() {
        parts = parts.push(
            text(&member.more_roles)
                .size(typography::size::CAPTION)
                .font(typography::SEMIBOLD)
                .style(catalog::text::secondary)
                .boxed(),
        );
    }
    parts.boxed()
}

/// The signals' icons, each with what it means on hover.
fn signals<'a>(member: &'a Member, motion: Motion) -> Element<'a, Intent> {
    let mut icons = row![].spacing(2).align_y(Alignment::Center);
    if member.unusual_dms {
        icons = icons.push(
            tip_icon(
                Icon::MessageCircle,
                |theme: &Theme| theme.palette().muted,
                tr!("Sent excessive DMs to non-friend server members in last 24 hrs"),
                motion,
            )
            .boxed(),
        );
    }
    if !member.timed_out.is_empty() {
        icons = icons.push(
            tip_icon(
                Icon::Clock,
                |theme: &Theme| theme.palette().danger,
                &member.timed_out,
                motion,
            )
            .boxed(),
        );
    }
    if member.spammer {
        icons = icons.push(
            tip_icon(
                Icon::TriangleAlert,
                |theme: &Theme| theme.palette().danger,
                tr!("Engaged in suspected spam activity"),
                motion,
            )
            .boxed(),
        );
    }
    if member.quarantined {
        icons = icons.push(
            tip_icon(
                Icon::Shield,
                |theme: &Theme| theme.palette().muted,
                tr!("User can't talk in server until they change their member name"),
                motion,
            )
            .boxed(),
        );
    }
    icons.boxed()
}

/// The avatar and the name over the username, the name in its role's colour.
fn who<'a>(member: &'a Member, motion: Motion) -> Element<'a, Intent> {
    let color = member.name_color;
    let name = line(&member.name, typography::size::BODY)
        .font(typography::SEMIBOLD)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(color.unwrap_or(theme.palette().text)),
            ..Default::default()
        });
    let mut names = column![name.boxed()].spacing(1);
    if !member.username.is_empty() {
        names = names.push(
            line(&member.username, typography::size::LABEL)
                .style(catalog::text::secondary)
                .boxed(),
        );
    }
    row![
        portrait(&member.avatar, AVATAR)
            .motion(motion)
            .view()
            .boxed(),
        container(names).width(Length::Fill).clip(true).boxed()
    ]
    .spacing(BESIDE)
    .align_y(Alignment::Center)
    .boxed()
}

/// One member: pressing it opens their profile, a right click or More
/// Options their menu.
pub fn member<'a>(member: &'a Member, page: &'a Members, motion: Motion) -> Element<'a, Intent> {
    let id = member.id.clone();
    let mut backdrop = Leaf::new("", motion, |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        Ink {
            fill: mix(Color::TRANSPARENT, palette.lift(0.06), lit),
            text: palette.text,
            glyph: palette.text,
        }
    })
    .on_press(Intent::Open(id.clone()));
    backdrop.stretch = true;
    backdrop.height = HEIGHT;

    let secondary = |words: &'a str| {
        cell(
            SINCE,
            line(words, typography::size::LABEL).style(catalog::text::secondary),
        )
    };
    let mut cells: Vec<Element<'a, Intent>> = vec![cell(NAME, who(member, motion))];
    cells.push(secondary(&member.since));
    if !page.narrow {
        cells.push(cell(
            AGE,
            line(&member.age, typography::size::LABEL).style(catalog::text::secondary),
        ));
    }
    if page.join_method {
        let unknown = member.join_unknown;
        cells.push(cell(
            JOIN,
            line(&member.join, typography::size::LABEL).style(move |theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(if unknown {
                        theme.palette().muted
                    } else {
                        theme.palette().text
                    }),
                    ..Default::default()
                }
            }),
        ));
    }
    cells.push(cell(ROLES, role(member)));
    cells.push(cell(SIGNALS, signals(member, motion)));
    let more_id = id.clone();
    cells.push(
        opener(
            icon_button(Icon::Ellipsis, tr!("More Options"), motion)
                .size(MORE)
                .glyph(MORE_GLYPH)
                .view(),
        )
        .on_press(move |bounds| Intent::Menu(more_id.clone(), Spot::below(bounds)))
        .boxed(),
    );
    let content = container(row(cells).spacing(GAP).align_y(Alignment::Center))
        .height(HEIGHT)
        .padding([0.0, OUTSET])
        .align_y(iced::alignment::Vertical::Center);
    opener(stack![backdrop, content])
        .on_right_press(move |at| Intent::Menu(id.clone(), Spot::pointer(at)))
        .boxed()
}
