//! Joining a voice channel or a DM's call: a voice channel's join screen
//! (`JoinVoice`, `ui/voice.slint:629`) and a DM's bar above its messages
//! (`CallInvite`, `:598`), on the stage's ground the conversation gives
//! them. The glue: `src/ui/callbacks.rs:3061` (`join-voice`) and `:1095`
//! (`answer-call`); the text is `src/ui/refresh.rs:4764`-`:4818`.

use iced::font::Weight;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::join::{Call, Channel, Intent, Join};
use anomi_widgets::avatar::circle;
use anomi_widgets::button::{Kind, action_button, call_button};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// The channel's screen: padding 32, spacing 14, centred (`:636`).
const PAD: f32 = 32.0;
const GAP: f32 = 14.0;
/// The round speaker: 80 px with a 38 px icon (`:640`).
const DISC: f32 = 80.0;
const SPEAKER: f32 = 38.0;
/// The name (`:645`) and the encryption notice (`:647`).
const NAME: f32 = 26.0;
const NAME_WEIGHT: Weight = Weight::Black;
const NOTICE: f32 = 15.0;
/// The button's row is padded 6 on top and 42 px tall (`:649`).
const BUTTON_TOP: f32 = 6.0;
const BUTTON_HEIGHT: f32 = 42.0;
/// A DM's bar: padding 20, spacing 12, 56 px avatars 8 apart, buttons
/// 16 apart (`:605`).
const BAR_PAD: f32 = 20.0;
const BAR_GAP: f32 = 12.0;
const FACE: f32 = 56.0;
const FACES_GAP: f32 = 8.0;
const BUTTONS_GAP: f32 = 16.0;

pub fn view(join: &Join, motion: Motion) -> impl View<Intent> + '_ {
    match join {
        Join::None => space().boxed(),
        Join::Channel(channel) => channel_screen(channel, motion).boxed(),
        Join::Call(call) => call_bar(call, motion).boxed(),
    }
}

fn channel_screen(channel: &Channel, motion: Motion) -> Element<'_, Intent> {
    let speaker =
        container(
            icon(Icon::Volume, SPEAKER).style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }),
        )
        .center(DISC)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().raised.into()),
            border: iced::Border::default().rounded(DISC / 2.0),
            ..container::Style::default()
        });
    let name = text(channel.name.as_str())
        .size(NAME)
        .font(typography::weight(NAME_WEIGHT))
        .width(Fill)
        .align_x(Alignment::Center)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::primary);
    let description = text(channel.description.as_str())
        .size(typography::size::BODY)
        .width(Fill)
        .align_x(Alignment::Center)
        .style(catalog::text::secondary);
    let label = if channel.switching {
        tr!("Switch voice channel")
    } else {
        tr!("Join voice")
    };
    let mut button = action_button(label, Kind::Primary, motion)
        .height(BUTTON_HEIGHT)
        .enabled(channel.can_join);
    if channel.can_join {
        button = button.on_press(Intent::Join);
    }
    let mut body = column![speaker.boxed(), name.boxed(), description.boxed()]
        .spacing(GAP)
        .align_x(Alignment::Center)
        .width(Fill);
    if !channel.preview && !channel.encrypted {
        body = body.push(
            text(tr!(
                "This channel is unencrypted. Yapfast only joins encrypted voice channels."
            ))
            .size(NOTICE)
            .width(Fill)
            .align_x(Alignment::Center)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().accent),
                ..text::Style::default()
            })
            .boxed(),
        );
    }
    body = body.push(
        container(button)
            .padding(Padding::ZERO.top(BUTTON_TOP))
            .boxed(),
    );
    container(body)
        .padding(PAD)
        .center_y(Fill)
        .width(Fill)
        .height(Fill)
        .boxed()
}

fn call_bar(call: &Call, motion: Motion) -> Element<'_, Intent> {
    let faces = row(call
        .people
        .iter()
        .map(|avatar| circle(avatar, FACE).boxed()))
    .spacing(FACES_GAP);
    let note = text(call.note.as_str())
        .size(NOTICE)
        .width(Fill)
        .align_x(Alignment::Center)
        .style(catalog::text::secondary);
    let answer = |accept| Intent::Answer {
        room: call.room.clone(),
        accept,
    };
    let mut buttons = row![].spacing(BUTTONS_GAP);
    if call.incoming {
        buttons = buttons.push(
            call_button(Icon::PhoneOff, tr!("Decline"), |p| p.danger, motion)
                .on_press(answer(false))
                .boxed(),
        );
    }
    let label = if call.incoming {
        tr!("Accept")
    } else {
        tr!("Join the call")
    };
    let mut accept = call_button(Icon::Phone, label, |p| p.live, motion).enabled(call.can_join);
    if call.can_join {
        accept = accept.on_press(answer(true));
    }
    buttons = buttons.push(accept.boxed());
    container(
        column![faces.boxed(), note.boxed(), buttons.boxed()]
            .spacing(BAR_GAP)
            .align_x(Alignment::Center)
            .width(Fill),
    )
    .padding(BAR_PAD)
    .center_y(Fill)
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// Its shots for `anomi-shots`, named after the golden screenshots where
/// one matches.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::voice::join as fixtures;

    use crate::shots::{GOLDEN, Shot};

    let shot = |area, name, theme, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme,
        vm,
    };
    vec![
        shot("voice", "join_channel", "bonfire", || {
            fixtures::channel_window(fixtures::channel())
        }),
        shot("voice", "join_switching", "bonfire", || {
            fixtures::channel_window(fixtures::switching())
        }),
        shot("voice", "join_unencrypted", "bonfire", || {
            fixtures::channel_window(fixtures::unencrypted())
        }),
        shot("voice", "join_locked", "bonfire", || {
            fixtures::channel_window(fixtures::locked())
        }),
        shot("voice", "join_preview", "bonfire", || {
            fixtures::channel_window(fixtures::preview())
        }),
        shot("dmcall", "join_bob_dm", "bonfire", || {
            fixtures::call_window(fixtures::call())
        }),
        shot("dmcall", "join_crowd", "bonfire", || {
            fixtures::call_window(fixtures::crowd())
        }),
        shot("dmcall", "join_ringing", "bonfire", || {
            fixtures::call_window(fixtures::ringing())
        }),
        shot("dmcall", "join_ringing_empty", "bonfire", || {
            fixtures::call_window(fixtures::ringing_empty())
        }),
        shot("light", "dm_bob_call", "catppuccin-latte", || {
            fixtures::call_window(fixtures::call())
        }),
    ]
}
