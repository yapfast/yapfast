//! A member's two lines of words: the name with its role's picture, server
//! tag, APP chip, crown, boost gem and timeout clock, and the activity
//! under it. The Slint side: the text column of the member row
//! (`ui/people.slint:299`-`:347`), `BotTag` (`ui/messages.slint:2031`).

use iced::widget::{self, container, text};
use iced::{Alignment, Color, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics, typography};
use anomi_vm::kind::Expected;
use anomi_vm::people::members::{Intent, Member};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::name::styled_name;
use anomi_widgets::nameplate::extent;
use anomi_widgets::presence::status_text;
use anomi_widgets::tag::server_tag;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::super::OFFLINE;
use super::super::group::role_picture;

mod fit;
use fit::fit;

/// The name's size (`font-size: 16px`) and the activity's (`:343`).
const NAME: f32 = typography::size::TITLE;
const ACTIVITY: f32 = typography::size::CAPTION;
/// Between the name and its marks (`spacing: 4px`).
const GAP: f32 = 4.0;
/// The role's picture, the crown, the gem and the clock (`size: 16px`,
/// `width: 14px`).
const ROLE: f32 = 16.0;
const MARK: f32 = 14.0;
/// The owner's crown, Discord's boosting pink: the same in every theme
/// (`colorize: #f0b232`, `#ff73fa`).
const CROWN: Color = Color::from_rgb(
    0xf0 as f32 / 255.0,
    0xb2 as f32 / 255.0,
    0x32 as f32 / 255.0,
);
const GEM: Color = Color::from_rgb(1.0, 0x73 as f32 / 255.0, 0xfa as f32 / 255.0);
/// The APP chip: 16 px tall with 4 px corners in Discord's blurple, its
/// words 10 px bold (`ui/messages.slint:2032`-`:2035`).
const CHIP: f32 = 16.0;
const CHIP_TEXT: f32 = 10.0;
const BLURPLE: Color = Color::from_rgb(
    0x58 as f32 / 255.0,
    0x65 as f32 / 255.0,
    0xf2 as f32 / 255.0,
);

/// The name's line and the one under it, as wide as their words, which the
/// nameplate fades in after.
/// `hovered` is the copy under the pointer, the only one whose gem opens
/// a card.
pub fn column<'a>(
    member: &'a Member,
    gems: bool,
    hovered: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut lines = widget::column![name_line(member, gems, hovered, motion)]
        .width(Length::Shrink)
        .align_x(Alignment::Start);
    let said = !member.status.text.is_empty() || member.status.emoji != Expected::None;
    if said {
        let color: fn(&Palette) -> Color = if member.offline() {
            |palette| palette.muted.scale_alpha(OFFLINE)
        } else {
            |palette| palette.muted
        };
        lines = lines.push(
            status_text(&member.status, motion)
                .size(ACTIVITY)
                .bullet(member.bullet)
                .color(color)
                .view()
                .boxed(),
        );
    }
    container(extent(lines)).center_y(Fill).width(Fill).boxed()
}

fn name_line<'a>(
    member: &'a Member,
    gems: bool,
    hovered: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let faded = if member.offline() { OFFLINE } else { 1.0 };
    let name = styled_name(&member.name, member.name_style)
        .size(NAME)
        .weight(iced::font::Weight::Medium)
        .opacity(faded);
    let name = match member.color {
        Some(color) => name.ink(color),
        None => name.ink((|palette: &Palette| palette.secondary) as fn(&Palette) -> Color),
    };
    let mut marks: Vec<Element<'a, Intent>> = Vec::new();
    if let Some(role) = role_picture(&member.role_icon, ROLE, faded) {
        marks.push(role);
    }
    if let Some(tag) = &member.tag {
        // It opens no card here, as in Discord's client.
        marks.push(server_tag(tag, motion).view().boxed());
    }
    if member.bot {
        marks.push(bot(faded));
    }
    if member.owner {
        marks.push(mark(Icon::Crown, |_| CROWN, faded));
    }
    if gems && !member.boosting.is_empty() {
        let gem = mark(Icon::Gem, |_| GEM, 1.0);
        marks.push(if hovered {
            tip(gem, member.boosting.as_str(), Side::Above, motion).boxed()
        } else {
            gem
        });
    }
    if member.timed_out {
        marks.push(mark(Icon::Clock, |theme| theme.palette().danger, 1.0));
    }
    fit(name.boxed(), marks, GAP).boxed()
}

fn mark<'a>(
    glyph: Icon,
    color: impl Fn(&Theme) -> Color + 'a,
    opacity: f32,
) -> Element<'a, Intent> {
    icon(glyph, MARK)
        .style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(color(theme).scale_alpha(opacity)),
        })
        .boxed()
}

/// An application's mark, in the words of every language (`APP`).
fn bot<'a>(opacity: f32) -> Element<'a, Intent> {
    container(
        text(tr!("APP"))
            .size(CHIP_TEXT)
            .font(typography::BOLD)
            .color(Color::WHITE.scale_alpha(opacity))
            .wrapping(text::Wrapping::None),
    )
    .padding([0.0, metrics::SPACE_XS])
    .height(CHIP)
    .center_y(CHIP)
    .style(move |_: &Theme| container::Style {
        background: Some(iced::Background::Color(BLURPLE.scale_alpha(opacity))),
        border: iced::Border::default().rounded(metrics::RADIUS_FIELD),
        ..container::Style::default()
    })
    .boxed()
}
