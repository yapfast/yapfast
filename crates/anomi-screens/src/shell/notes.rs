//! What the window says over everything for a moment: the zoom level after
//! it changes (`ui/yapfast.slint:5425`), and files dragged over the window,
//! as Discord shows them, a card over the dimmed window saying where they
//! will go, or why a drop was refused until Got It or a click (`:5484`).
//! Both arrive as `PopupEntry` has them and go at once.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Color, Fill, Point, Rectangle, Shadow, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::menu::{Align, Side, Spot};
use anomi_vm::shell::{self, Files};
use anomi_vm::{Intent, Vm};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::layer::{self, Backdrop, Layer, Place};
use yapfast_lang::tr;

use super::drop_card::{self, drop_card};
use super::layers::Frame;

/// The zoom note: 12 px under the bar, 36 px tall and round, its text
/// 14 px from its ends, 15 px bold (`:5426`-`:5433`).
const ZOOM_TOP: f32 = 12.0;
const ZOOM_HEIGHT: f32 = 36.0;
const ZOOM_PAD: f32 = 14.0;
const ZOOM_TEXT: f32 = 15.0;
/// The tooltip's shadow, which the note shares (`:5429`).
const NOTE_SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
    offset: Vector::new(0.0, 3.0),
    blur_radius: 12.0,
};

/// The drop's dimming, and its card's lines (the card itself is
/// [`DropCard`](super::drop_card)'s): 20 px in (24 at the top) and 8 px
/// apart, a 40 px file, 20 px and 15 px text (`:5508`-`:5528`).
const DROP_DIM: f32 = 0.65;
const DROP_PAD: f32 = 20.0;
const DROP_PAD_TOP: f32 = 24.0;
const DROP_GAP: f32 = 8.0;
const DROP_FILE: f32 = 40.0;
const DROP_TITLE: f32 = 20.0;
const DROP_LINE: f32 = 15.0;

/// The zoom level, while it shows.
pub fn zoom(vm: &Vm, frame: Frame) -> Option<Layer<'_, Intent>> {
    let level = vm.shell.zoom.as_deref()?;
    let pill = container(
        text(level)
            .size(ZOOM_TEXT)
            .font(typography::BOLD)
            .style(anomi_theme::catalog::text::primary),
    )
    .padding([0.0, ZOOM_PAD])
    .center_y(ZOOM_HEIGHT)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().tooltip)),
        border: Border::default().rounded(ZOOM_HEIGHT / 2.0),
        shadow: NOTE_SHADOW,
        ..container::Style::default()
    });
    let spot = Spot {
        anchor: Rectangle::new(
            Point::new(frame.window.width / 2.0, frame.top + ZOOM_TOP),
            Size::ZERO,
        ),
        side: Side::Below,
        align: Align::Centre,
        gap: 0.0,
    };
    Some(
        layer::layer("zoom", pill)
            .place(Place::At(spot))
            .backdrop(Backdrop::None),
    )
}

/// Files over the window, or a drop refused, while either shows.
pub fn files(vm: &Vm, motion: Motion, frame: Frame) -> Option<Layer<'_, Intent>> {
    let files = vm.shell.drop.as_ref()?;
    let dismiss = Intent::Shell(shell::Intent::DismissDrop);
    let (lines, refused): (Element<'_, Intent>, bool) = match files {
        Files::Over { destination, .. } => (
            column![
                row![
                    text(tr!("Upload to ")).size(DROP_TITLE).style(on_accent),
                    text(destination.as_str())
                        .size(DROP_TITLE)
                        .font(typography::BOLD)
                        .wrapping(text::Wrapping::None)
                        .ellipsis(text::Ellipsis::End)
                        .style(on_accent),
                ],
                centred(tr!("You can add comments before uploading."), DROP_LINE),
            ]
            .spacing(DROP_GAP)
            .align_x(Alignment::Center)
            .boxed(),
            false,
        ),
        Files::Refused { reason, help, .. } => (
            column![
                text(reason.as_str())
                    .size(DROP_TITLE)
                    .font(typography::BOLD)
                    .width(Fill)
                    .align_x(Alignment::Center)
                    .style(on_accent),
                centred(help.as_str(), DROP_LINE),
                container(
                    action_button(tr!("Got It"), Kind::Secondary, motion).on_press(dismiss.clone())
                )
                .padding(iced::Padding::default().top(DROP_GAP)),
            ]
            .spacing(DROP_GAP)
            .align_x(Alignment::Center)
            .boxed(),
            true,
        ),
    };
    let lines = column![
        icon(Icon::File, DROP_FILE).style(|theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(theme.palette().on_accent),
        }),
        lines,
    ]
    .width(Fill)
    .spacing(DROP_GAP)
    .align_x(Alignment::Center)
    .padding(iced::Padding::new(DROP_PAD).top(DROP_PAD_TOP));
    let placed = drop_card(lines, card_left(frame, files), refused, motion);
    Some(
        layer::layer("files", placed)
            .place(Place::Fill)
            .backdrop(Backdrop::Dim(DROP_DIM))
            .on_close(dismiss),
    )
}

/// The card's left edge: in the middle of where the files go, the thread
/// beside the conversation or the rest of the window, refused or not
/// (`:5494`).
fn card_left(frame: Frame, files: &Files) -> f32 {
    let (width, aside) = (frame.window.width, frame.aside);
    if files.aside() {
        width - aside + (aside - drop_card::WIDTH) / 2.0
    } else {
        (width - aside - drop_card::WIDTH) / 2.0
    }
}

fn centred(line: &str, size: f32) -> impl View<Intent> + '_ {
    text(line)
        .size(size)
        .width(Fill)
        .align_x(Alignment::Center)
        .style(on_accent)
}

fn on_accent(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().on_accent),
        ..text::Style::default()
    }
}
