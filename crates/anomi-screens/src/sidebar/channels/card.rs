//! Who is in the busy voice channel under the pointer, beside its row, as
//! Discord's client shows it: `VoiceCardView` (`ui/rail.slint:741`). A line
//! summing them up, then each person with their camera and LIVE marks, the
//! first eight and how many more. This account on another client is half
//! as strong, without its marks. The row's [`Pointed`](super::pointed)
//! places it and grows it in.

use iced::widget::{Column, Row, column, container, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::channels::{ChannelRow, Intent, VoicePerson};
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

use super::live::live;

/// The card: 232 px wide, 12 px in and 10 above, its lines 4 apart, with
/// 8 px corners and a soft shadow (`:747`-`:758`).
const WIDTH: f32 = 232.0;
const PAD: f32 = 12.0;
const PAD_TOP: f32 = 10.0;
const SPACING: f32 = 4.0;
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
    offset: Vector::new(0.0, 3.0),
    blur_radius: 12.0,
};
/// Its first eight people, each 26 px tall with a 20 px avatar
/// (`:745`, `:765`, `:768`).
const SHOWN: usize = 8;
const LINE: f32 = 26.0;
const AVATAR: f32 = 20.0;
/// Their names, 14 px; the camera mark 16 (`:770`, `:774`).
const NAME: f32 = typography::size::SMALL;
const MARK: f32 = 16.0;
/// This account on another client (`:767`).
const ELSEWHERE: f32 = 0.5;

pub fn view(row_vm: &ChannelRow) -> Element<'_, Intent> {
    let summary = text(row_vm.summary.as_str())
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(catalog::text::secondary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let people = row_vm.people.iter().take(SHOWN).map(line);
    let mut lines = Column::with_children(people).spacing(SPACING);
    let more = row_vm.people.len().saturating_sub(SHOWN);
    if more > 0 {
        lines = lines.push(
            text(tr!("and {} more", more))
                .size(typography::size::CAPTION)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    container(column![summary, lines].spacing(SPACING))
        .padding(Padding::new(PAD).top(PAD_TOP))
        .width(WIDTH)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().tooltip)),
            border: Border::default().rounded(metrics::RADIUS_CARD),
            shadow: SHADOW,
            ..container::Style::default()
        })
        .boxed()
}

/// Someone on the card: their avatar, name, camera and LIVE marks.
fn line(person: &VoicePerson) -> Element<'_, Intent> {
    let strength = if person.elsewhere { ELSEWHERE } else { 1.0 };
    let name = text(person.name.as_str())
        .size(NAME)
        .font(typography::MEDIUM)
        .style(move |theme: &Theme| text::Style {
            color: Some(theme.palette().text.scale_alpha(strength)),
            ..text::Style::default()
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let face = portrait(&person.avatar, AVATAR).view();
    let mut parts: Row<Element<'_, Intent>> = Row::new()
        .push(face.boxed())
        .push(name.boxed())
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center)
        .height(LINE);
    if person.video && !person.elsewhere {
        parts = parts.push(icon(Icon::Video, MARK).style(muted).boxed());
    }
    if person.live && !person.elsewhere {
        parts = parts.push(live(None));
    }
    parts.boxed()
}

/// A mark in the muted colour, whatever the pointer does.
pub fn muted(theme: &Theme, _status: iced::widget::svg::Status) -> iced::widget::svg::Style {
    iced::widget::svg::Style {
        color: Some(theme.palette().muted),
    }
}
