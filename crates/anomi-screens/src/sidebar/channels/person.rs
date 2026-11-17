//! Someone under a voice channel (`ui/rail.slint:1485`-`:1607`): a click
//! opens their profile beside the row and a right click their menu, as in
//! Discord; their LIVE tag watches their stream; they drag onto another
//! voice channel (`PersonTouch`, `:799`, through the list). This account in
//! the call on another client is half as strong until pointed at, without
//! its marks, and says so in a tooltip. In this account's own call each
//! shows whether they speak and are connected yet (`:1553`).

use iced::widget::{Row, container, svg, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::channels::{Channels, Intent, VoicePerson};
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::card::muted;
use super::items::{self, Room};
use super::live::live;
use super::pointed::{Fill as Ground, Ink, Look, pointed};

/// Their rows start 32 px in past the channel's 8 (`:1486`), with 4 px
/// corners, 4 px before the avatar and 8 after the marks (`:1494`, `:1524`).
const INDENT: f32 = 40.0;
const RADIUS: f32 = 4.0;
const LEFT: f32 = 4.0;
/// A 22 px avatar, a 15 px name, 16 px marks (`:1525`-`:1547`).
const AVATAR: f32 = 22.0;
const NAME: f32 = 15.0;
const MARK: f32 = 16.0;
/// This account on another client at rest (`:1497`).
const ELSEWHERE: f32 = 0.5;

pub fn view<'a>(
    channels: &'a Channels,
    r: usize,
    p: usize,
    room: Room,
    motion: Motion,
) -> Element<'a, Intent> {
    let row_vm = &channels.rows[r];
    let person = &row_vm.people[p];
    let in_call = row_vm.in_call;
    let elsewhere = person.elsewhere && !in_call;
    let look = Look {
        fill: Ground::Hover,
        ink: Ink::Inherit,
        radius: RADIUS,
        ring: None,
        rest: if elsewhere { ELSEWHERE } else { 1.0 },
        ..Look::default()
    };
    let channel = row_vm.id.as_str();
    let user = person.user.as_str();
    let mut row = pointed(body(channel, person, in_call), items::PERSON, look, motion);
    // Someone without a user id opens neither (`open-voice-menu`,
    // `ui/yapfast.slint:1612`).
    if !user.is_empty() {
        row = row
            .on_press_at(move |bounds| Intent::Profile {
                channel: channel.to_owned(),
                user: user.to_owned(),
                at: Spot::beside(bounds),
            })
            .on_menu(move |at| Intent::PersonMenu {
                channel: channel.to_owned(),
                user: user.to_owned(),
                at: Spot::pointer(at),
            });
    }
    let row: Element<'a, Intent> = if elsewhere {
        tip(row, tr!("Connected on another client"), Side::Above, motion).boxed()
    } else {
        row.boxed()
    };
    container(row)
        .padding(Padding {
            top: room.above,
            right: metrics::ROW_INSET,
            bottom: room.under,
            left: INDENT,
        })
        .width(Fill)
        .boxed()
}

/// Their avatar, name and marks.
fn body<'a>(channel: &'a str, person: &'a VoicePerson, in_call: bool) -> Element<'a, Intent> {
    let speaking = in_call && person.speaking;
    let connected = !in_call || person.connected;
    let name = text(person.name.as_str())
        .size(NAME)
        .font(if speaking {
            typography::BOLD
        } else {
            typography::MEDIUM
        })
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            text::Style {
                color: Some(if speaking {
                    palette.text
                } else if connected {
                    palette.secondary
                } else {
                    palette.muted
                }),
                ..text::Style::default()
            }
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let face = portrait(&person.avatar, AVATAR).speaking(speaking).view();
    let mut parts: Row<Element<'a, Intent>> = Row::new()
        .push(face.boxed())
        .push(name.boxed())
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center);
    let marked = person.marked(in_call);
    if person.video && marked {
        parts = parts.push(icon(Icon::Video, MARK).style(muted).boxed());
    }
    if person.live && marked {
        // Made on the click, not on every view.
        let watch = (!person.local).then_some(move || Intent::Watch {
            channel: channel.to_owned(),
            person: person.id.clone(),
        });
        parts = parts.push(live(watch));
    }
    if (person.muted || person.deafened) && marked {
        parts = parts.push(mark(Icon::MicOff, person.server_muted));
    }
    if person.deafened && marked {
        parts = parts.push(mark(Icon::HeadphoneOff, person.server_deafened));
    }
    container(parts)
        .padding(Padding::ZERO.left(LEFT).right(metrics::SPACE_S))
        .center_y(items::PERSON)
        .width(Fill)
        .boxed()
}

/// A mute or deafen mark: red where the server did it.
fn mark<'a>(glyph: Icon, by_server: bool) -> Element<'a, Intent> {
    icon(glyph, MARK)
        .style(move |theme: &Theme, _| {
            let palette = theme.palette();
            svg::Style {
                color: Some(if by_server {
                    palette.danger
                } else {
                    palette.muted
                }),
            }
        })
        .boxed()
}
