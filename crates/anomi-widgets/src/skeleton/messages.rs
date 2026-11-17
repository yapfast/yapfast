//! Messages on their way: rows the shape of messages (`MessagesSkeleton`,
//! `ui/controls.slint:1123`), standing at the bottom of the conversation.

use iced::widget::{Column, Row, Space, column};
use iced::{Length, alignment};

use anomi_theme::prelude::*;

use super::skeleton;

/// A row's name, then the width of each line under it.
const ROWS: [(f32, &[f32]); 5] = [
    (92.0, &[380.0, 220.0]),
    (120.0, &[300.0]),
    (76.0, &[420.0, 340.0, 160.0]),
    (104.0, &[260.0]),
    (88.0, &[360.0, 200.0]),
];

const PADDING: f32 = 16.0;
const ROW_GAP: f32 = 22.0;
const PART_GAP: f32 = 16.0;
const LINE_GAP: f32 = 8.0;
const AVATAR: f32 = 40.0;
const LINE: f32 = 12.0;

/// Five placeholder messages, each an avatar's circle with a name and lines.
pub fn messages_skeleton<'a, Message: 'a>(motion: Motion) -> impl View<Message> + 'a {
    let rows = ROWS.iter().map(|(name, lines)| {
        let bar = |width: f32| -> Element<'a, Message> {
            skeleton(motion)
                .width(width)
                .height(LINE)
                .radius(LINE / 2.0)
                .boxed()
        };
        let text: Element<'a, Message> =
            Column::with_children(std::iter::once(*name).chain(lines.iter().copied()).map(bar))
                .spacing(LINE_GAP)
                .padding(iced::Padding::ZERO.top(4.0))
                .boxed();
        let avatar = skeleton(motion)
            .width(AVATAR)
            .height(AVATAR)
            .radius(AVATAR / 2.0);
        let row: Element<'a, Message> = Row::new()
            .spacing(PART_GAP)
            .align_y(alignment::Vertical::Top)
            .push(avatar.boxed())
            .push(text)
            .boxed();
        row
    });
    let top: Element<'a, Message> = Space::new().height(Length::Fill).boxed();
    column![top]
        .extend(rows)
        .spacing(ROW_GAP)
        .padding(PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
}
