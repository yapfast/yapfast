//! The popout's tabs (`LinkTab`, `ui/voice.slint:171`): bold text on a 2 px
//! underline, lit while picked or under the pointer.

use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Paragraph as _, Shaping, Text, Wrapping,
};
use iced::widget::{button, column, container, row, text};
use iced::{Fill, Length, Pixels, Size};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Renderer, Theme, catalog, typography};
use anomi_vm::voice::connection::{Connection, Intent, Tab};
use yapfast_lang::tr;

const HEIGHT: f32 = 28.0;
/// The gap between tabs, and the room they share (the card's inside).
const SPACING: f32 = 16.0;
const ROOM: f32 = super::WIDTH - 2.0 * super::PADDING;

type Paragraph = <Renderer as iced::advanced::text::Renderer>::Paragraph;

/// The width one line of the tab's text takes.
fn measure(label: &str) -> f32 {
    Paragraph::with_text(Text {
        content: label,
        bounds: Size::INFINITE,
        size: Pixels(typography::size::SMALL),
        line_height: LineHeight::default(),
        font: typography::BOLD,
        align_x: Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    })
    .min_bounds()
    .width
    .ceil()
}

/// The tabs, each at its text's width; when they don't fit, each gives up
/// an equal share of the shortfall, as Slint's layout does with
/// `min-width: 0px` tabs (`ui/voice.slint:177`).
pub(super) fn tabs(connection: &Connection, _motion: Motion) -> impl View<Intent> + '_ {
    let shown = connection.shown();
    let mut all = vec![(tr!("Connection"), Tab::Connection)];
    if !connection.code.is_empty() {
        all.push((tr!("Privacy"), Tab::Privacy));
    }
    if connection.stream.is_some() {
        all.push((tr!("Stream"), Tab::Stream));
    }
    let widths: Vec<f32> = all.iter().map(|(label, _)| measure(label)).collect();
    let spaces = SPACING * (all.len() - 1) as f32;
    let share = ((widths.iter().sum::<f32>() + spaces - ROOM) / all.len() as f32).max(0.0);
    let mut tabs = row![].spacing(SPACING);
    for ((label, which), width) in all.into_iter().zip(widths) {
        tabs = tabs.push(tab(label, which, shown, (width - share).max(1.0)).boxed());
    }
    tabs.width(Fill)
}

fn tab(label: &'static str, tab: Tab, shown: Tab, width: f32) -> impl View<Intent> {
    let on = tab == shown;
    let face = column![
        text(label)
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .height(HEIGHT - 2.0)
            .align_y(iced::alignment::Vertical::Top),
        container(iced::widget::space().height(2))
            .width(Fill)
            .style(move |theme: &Theme| container::Style {
                background: on.then(|| theme.palette().accent.into()),
                ..container::Style::default()
            }),
    ];
    button(face)
        .padding(0)
        .width(Length::Fixed(width))
        .on_press(Intent::Tab(tab))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let lit = on || matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                text_color: if lit { palette.text } else { palette.muted },
                ..catalog::button::bare(theme, status)
            }
        })
}
