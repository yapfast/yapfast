//! A file as a card (`FileCard`, `ui/messages.slint:1750`): its icon, name
//! and size, and Save. A voice message is C11's player in its place.

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{column, container, responsive, row, svg, text};
use iced::{Border, Fill, Padding, Shrink, Size, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::attachments::{Attachments, File, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::{Icon, icon};

use crate::voice_message;

/// `max-width: 432px`, `padding: 12px` (8 at the end), `spacing: 12px`,
/// `border-radius: 8px`, the icon 30 px wide.
const MAX_WIDTH: f32 = 432.0;
const PAD: f32 = 12.0;
const PAD_END: f32 = 8.0;
const GAP: f32 = 12.0;
const RADIUS: f32 = 8.0;
const ICON: f32 = 30.0;
const TOP: f32 = 4.0;

pub fn view<'a>(
    attachments: &'a Attachments,
    file: &'a File,
    motion: Motion,
) -> Element<'a, Intent> {
    let content: Element<'a, Intent> = match &file.voice {
        Some(voice) => voice_message::view(voice, motion)
            .map(Intent::Voice)
            .boxed(),
        // `max-width: 432px`: as wide when there is room, narrower with less.
        None => responsive(move |room: Size| card(attachments, file, room.width, motion))
            .height(Shrink)
            .boxed(),
    };
    container(content).padding(Padding::ZERO.top(TOP)).boxed()
}

fn card<'a>(
    attachments: &'a Attachments,
    file: &'a File,
    room: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut names = column![].push(
        text(&file.name)
            .size(typography::size::BODY)
            .font(typography::MEDIUM)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
            .style(catalog::text::primary)
            .boxed(),
    );
    if !file.detail.is_empty() {
        names = names.push(typography::label(&file.detail).boxed());
    }
    let glyph = icon(Icon::File, ICON).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().accent),
    });
    let save = icon_button(Icon::Download, tr!("Save"), motion)
        .enabled(attachments.acts())
        .on_press(Intent::Save {
            item: attachments.item,
            index: file.index,
        })
        .view();
    let line = row![
        glyph.boxed(),
        container(names).width(Fill).boxed(),
        save.boxed()
    ]
    .spacing(GAP)
    .align_y(alignment::Vertical::Center);
    container(line.boxed())
        .padding(Padding {
            top: PAD,
            right: PAD_END,
            bottom: PAD,
            left: PAD,
        })
        .width(MAX_WIDTH.min(room))
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sidebar.into()),
                border: Border {
                    color: palette.edge,
                    width: 1.0,
                    radius: RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
        .boxed()
}
