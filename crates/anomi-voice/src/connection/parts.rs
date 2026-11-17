//! What each tab holds, and the notice under all of them
//! (`ui/voice.slint:230`-`:322`).

use iced::alignment::Horizontal;
use iced::widget::text::{Rich, Span, span};
use iced::widget::{Id, column, container, row, stack, text};
use iced::{Border, Color, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, catalog, metrics, typography};
use anomi_vm::voice::connection::{Connection, Intent, Stream};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

use super::graph::graph;

/// The Copy button's id, for focus and tests.
pub const COPY: &str = "voice-privacy-copy";
const NOTE: f32 = 13.0;
/// The privacy code's groups (`ui/voice.slint:287`) and the heading (`:274`).
const CODE: f32 = 18.0;
const HEADING: f32 = 15.0;
/// The notice's wash over the live green (`ui/voice.slint:311`).
const WASH: f32 = 0.12;

/// A label and its value in bold (`LinkRow`, `ui/voice.slint:193`): one
/// paragraph, so a long label wraps at words inside the card and the value
/// follows it. A span can't read the theme, so two copies lie over each
/// other, each inking its own half and leaving the other clear.
fn line<'a, M: 'a>(label: &'a str, value: &'a str) -> impl View<M> + 'a {
    let half = |label_ink: Option<Color>, value_ink: Option<Color>| {
        let label: Span<'a, ()> = span(label).color_maybe(label_ink);
        let value: Span<'a, ()> = span(value).font(typography::BOLD).color_maybe(value_ink);
        Rich::<(), M>::with_spans([label, value])
            .size(typography::size::SMALL)
            .width(Fill)
    };
    stack![
        half(None, Some(Color::TRANSPARENT)).style(catalog::text::secondary),
        half(Some(Color::TRANSPARENT), None).style(catalog::text::primary),
    ]
}

fn note<'a, M: 'a>(words: impl text::IntoFragment<'a>, secondary: bool) -> impl View<M> + 'a {
    text(words).size(NOTE).width(Fill).style(if secondary {
        catalog::text::secondary
    } else {
        catalog::text::muted
    })
}

/// The ping graph, the averages and what delayed audio means.
pub(super) fn connection(connection: &Connection) -> impl View<Intent> + '_ {
    let average = if connection.average.is_empty() {
        tr!("Unknown")
    } else {
        &connection.average
    };
    let mut lines = column![
        graph(connection).boxed(),
        line(tr!("Average ping:"), average).boxed(),
    ]
    .spacing(8);
    if !connection.ping.is_empty() {
        lines = lines.push(line(tr!("Last ping:"), &connection.ping).boxed());
    }
    if !connection.loss.is_empty() {
        lines = lines.push(line(tr!("Outbound packet loss rate:"), &connection.loss).boxed());
    }
    let mut words = tr!(
        "You may notice delayed audio at 250 ms or higher. If the problem persists, disconnect and try again."
    )
    .to_owned();
    if !connection.loss.is_empty() {
        words.push_str(tr!(
            " You may sound robotic if your packet loss rate is over 10%."
        ));
    }
    lines.push(note(words, false).boxed())
}

/// How the shared screen goes out.
pub(super) fn stream<'a, M: 'a>(stream: &'a Stream) -> impl View<M> + 'a {
    column![
        line(tr!("Encoder:"), &stream.encoder).boxed(),
        line(tr!("Codec:"), &stream.codec).boxed(),
        line(tr!("Resolution:"), &stream.picture).boxed(),
        line(tr!("Sending:"), &stream.sent).boxed(),
        line(tr!("Aiming for:"), &stream.target).boxed(),
        line(tr!("Keyframes viewers asked for:"), &stream.keyframes).boxed(),
        line(tr!("Packets resent:"), &stream.resent).boxed(),
        note(
            tr!("The bitrate follows what Discord estimates your connection carries. Fast motion at a low bitrate looks blocky to viewers."),
            false
        )
        .boxed(),
    ]
    .spacing(8)
}

/// The voice privacy code, two rows of three groups, and Copy.
pub(super) fn privacy(connection: &Connection, motion: Motion) -> impl View<Intent> + '_ {
    let group = |index: usize| {
        text(connection.code.get(index).map_or("", String::as_str))
            .size(CODE)
            .font(typography::BOLD)
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Center)
            .style(catalog::text::primary)
    };
    let digits = container(
        column![
            row![group(0), group(1), group(2)],
            row![group(3), group(4), group(5)],
        ]
        .spacing(6),
    )
    .padding(10)
    .width(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().sunken.into()),
        border: Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    });
    let words = if connection.copied {
        tr!("Copied")
    } else {
        tr!("Copy")
    };
    let copy = action_button(words, Kind::Secondary, motion)
        .small()
        .leading(Icon::Copy)
        .id(Id::new(COPY))
        .on_press(Intent::Copy(connection.code.join(" ")));
    column![
        text(tr!("Voice Privacy Code"))
            .size(HEADING)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        note(
            tr!("A new code is generated when people join or leave this call. Compare it with the others in the call: everyone sees the same numbers."),
            true
        ),
        digits,
        container(copy).center_x(Fill),
        container(iced::widget::space().height(1))
            .width(Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().line.into()),
                ..container::Style::default()
            }),
        note(
            tr!("Only you and the others in this call can hear you. Nobody else, not even Discord, can listen in."),
            true
        ),
    ]
    .spacing(10)
}

/// "Secure connection", or "End-to-end encrypted", in the live green.
pub(super) fn notice<'a, M: 'a>(end_to_end: bool) -> impl View<M> + 'a {
    let words = if end_to_end {
        tr!("End-to-end encrypted")
    } else {
        tr!("Secure connection")
    };
    let green = |theme: &Theme| theme.palette().live;
    container(
        row![
            icon(Icon::Lock, 16.0).style(move |theme: &Theme, _| iced::widget::svg::Style {
                color: Some(green(theme)),
            }),
            text(words)
                .size(NOTE)
                .font(typography::BOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(green(theme)),
                    ..text::Style::default()
                }),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .padding(8)
    .width(Fill)
    .style(move |theme: &Theme| container::Style {
        background: Some(green(theme).scale_alpha(WASH).into()),
        border: Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    })
}
