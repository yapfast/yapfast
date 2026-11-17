//! The sound that plays as the person joins a voice channel: the server it is
//! for, its name, and the list to choose from (`ui/settings.slint:1060-1101`).
//! The list opens under the row where Slint hung a popup from its button.

use iced::widget::{column, container, row, space, text};
use iced::{Border, Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::voice::{Group, Intent, Voice};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use crate::kit::{heading, picker};

use super::{GROUP, note};

const PICKER: f32 = 200.0;
const HEIGHT: f32 = 36.0;
const RADIUS: f32 = 8.0;
const GAP: f32 = 8.0;
/// A sound's button and how many sit in a line, as the soundboard's rows.
const PER_LINE: usize = 3;
const LIST_HEIGHT: f32 = 260.0;
const PANEL_PADDING: f32 = 12.0;

pub fn view(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    let named = !page.entrance_sound.is_empty();
    let about = if page.nitro {
        tr!(
            "Plays to everyone in a voice channel or DM call as you join it, outside stage channels. A server's own comes before the one for all servers."
        )
    } else {
        tr!("Entrance sounds play to everyone as you join a voice channel. They need Nitro.")
    };
    let name = text(if named {
        page.entrance_sound.as_str()
    } else {
        tr!("None")
    })
    .size(typography::size::BODY)
    .style(move |theme: &Theme| {
        if named {
            catalog::text::primary(theme)
        } else {
            catalog::text::muted(theme)
        }
    })
    .wrapping(text::Wrapping::None)
    .ellipsis(text::Ellipsis::End);
    let well = container(name)
        .width(Length::Fill)
        .height(HEIGHT)
        .padding([0.0, 12.0])
        .align_y(alignment::Vertical::Center)
        .clip(true)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sunken.into()),
                border: Border {
                    radius: RADIUS.into(),
                    width: 1.0,
                    color: palette.line,
                },
                ..container::Style::default()
            }
        });
    let mut line = row![
        container(picker(
            &page.entrance_servers,
            page.entrance_server,
            (!page.entrance_servers.is_empty())
                .then_some(Intent::EntranceServer as fn(usize) -> Intent),
        ))
        .width(PICKER),
        well,
        action_button(tr!("Choose"), Kind::Secondary, motion)
            .enabled(page.nitro)
            .on_press(Intent::EntranceList(!page.entrance_open))
            .boxed(),
    ]
    .spacing(GAP)
    .align_y(alignment::Vertical::Center);
    if named {
        line = line.push(
            action_button(tr!("Remove"), Kind::Ghost, motion)
                .on_press(Intent::Entrance(String::new()))
                .boxed(),
        );
    }
    let mut parts = column![heading(tr!("ENTRANCE SOUNDS")), line, note(about)].spacing(GROUP);
    if page.entrance_open {
        parts = parts.push(chooser(&page.entrance_groups, motion));
    }
    parts.boxed()
}

/// The sounds to choose from, a heading and lines of buttons for each group.
fn chooser(groups: &[Group], motion: Motion) -> Element<'_, Intent> {
    let mut list: Vec<Element<'_, Intent>> = Vec::new();
    for group in groups {
        list.push(heading(group.heading.as_str()).boxed());
        for sounds in group.sounds.chunks(PER_LINE) {
            // A short last line keeps the columns of the full ones.
            let line = sounds
                .iter()
                .map(|sound| {
                    action_button(sound.name.as_str(), Kind::Secondary, motion)
                        .small()
                        .width(Length::Fill)
                        .on_press(Intent::Entrance(sound.id.clone()))
                        .boxed()
                })
                .chain((sounds.len()..PER_LINE).map(|_| space().width(Length::Fill).boxed()));
            list.push(row(line).spacing(GAP).boxed());
        }
    }
    let top = row![
        text(tr!("Choose an entrance sound"))
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .width(Length::Fill),
        action_button(tr!("Close"), Kind::Ghost, motion)
            .small()
            .on_press(Intent::EntranceList(false))
            .boxed(),
    ]
    .align_y(alignment::Vertical::Center);
    container(
        column![
            top,
            container(scroll_view(
                container(column(list).spacing(GAP)).padding(Padding {
                    right: PANEL_PADDING,
                    ..Padding::ZERO
                }),
                motion
            ))
            .height(Length::Fixed(LIST_HEIGHT)),
        ]
        .spacing(GAP),
    )
    .padding(PANEL_PADDING)
    .width(Length::Fill)
    .style(catalog::container::raised)
    .boxed()
}
