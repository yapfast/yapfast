//! A channel's overview page: the name, and for a text channel its topic,
//! slowmode and age restriction. The Slint side: `OverviewSection`
//! (`ui/channel_settings.slint:158`): 16 px between the groups, 8 within.
//! Nothing can be written while the draft is on its way (`saving`).

use iced::widget::{column, container, row, text};
use iced::{Fill, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::channel::{Channel, Intent, Kind, SLOWMODES};
use anomi_widgets::button::icon_button;
use anomi_widgets::field::field;
use yapfast_lang::tr;

use crate::kit::{area, heading, switch};

const GROUP: f32 = 16.0;
const INNER: f32 = 8.0;
/// The stepper's buttons and the word between them.
const STEP: f32 = 32.0;
const STEP_GLYPH: f32 = 18.0;
const SHOWN: f32 = 48.0;

/// The words of the slowmode at `index` of the [`SLOWMODES`] Discord offers.
fn slowmode_word(index: usize) -> &'static str {
    match index {
        0 => tr!("Off"),
        1 => tr!("5s"),
        2 => tr!("10s"),
        3 => tr!("15s"),
        4 => tr!("30s"),
        5 => tr!("1m"),
        6 => tr!("2m"),
        7 => tr!("5m"),
        8 => tr!("10m"),
        9 => tr!("15m"),
        10 => tr!("30m"),
        11 => tr!("1h"),
        12 => tr!("2h"),
        _ => tr!("6h"),
    }
}

pub fn view(channel: &Channel, motion: Motion) -> impl View<Intent> + '_ {
    let ready = !channel.saving;
    let category = channel.kind == Kind::Category;
    // Slint's `TextField` as it is: 16 px text, 12 px in.
    let mut name = field(
        if category {
            tr!("Category name")
        } else {
            tr!("Channel name")
        },
        &channel.name,
    )
    .motion(motion);
    if ready {
        name = name.on_input(Intent::Name);
    }
    let mut groups = vec![
        column![
            heading(if category {
                tr!("CATEGORY NAME")
            } else {
                tr!("CHANNEL NAME")
            }),
            name.view(),
        ]
        .spacing(INNER)
        .boxed(),
    ];
    if channel.is_text() {
        groups.push(topic(channel, ready, motion));
        groups.push(slowmode(channel, ready, motion));
        let mut age = switch(tr!("Age-Restricted Channel"), channel.nsfw, motion)
            .description(tr!(
                "Users will need to confirm they are of over legal age to view the content in this channel. Age-restricted channels are exempt from the explicit content filter."
            ))
            .enabled(ready);
        if ready {
            age = age.on_toggle(Intent::Nsfw);
        }
        groups.push(age.view());
    }
    column(groups).spacing(GROUP).width(Fill)
}

/// "CHANNEL TOPIC" and its lines: Discord's, several, and a topic longer
/// than Discord takes is said when saving.
fn topic(channel: &Channel, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let mut lines =
        area(&channel.topic, motion).placeholder(tr!("Let everyone know how to use this channel!"));
    if ready {
        lines = lines.on_edit(Intent::Topic);
    }
    column![heading(tr!("CHANNEL TOPIC")), lines.view()]
        .spacing(INNER)
        .boxed()
}

/// "SLOWMODE": the stepper with its word and what the interval means. Its
/// buttons wait with the other fields while the draft is on its way.
fn slowmode(channel: &Channel, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let last = SLOWMODES - 1;
    let now = channel.slowmode.min(last);
    let stepper = row![
        container(
            icon_button(Icon::ChevronLeft, tr!("Shorter slowmode"), motion)
                .size(STEP)
                .glyph(STEP_GLYPH)
                .on_press(Intent::Slowmode(now.saturating_sub(1)))
                .enabled(ready)
                .view()
        ),
        container(
            text(slowmode_word(now))
                .size(typography::size::BODY)
                .style(catalog::text::primary)
        )
        .center_x(SHOWN)
        .center_y(STEP),
        container(
            icon_button(Icon::ChevronRight, tr!("Longer slowmode"), motion)
                .size(STEP)
                .glyph(STEP_GLYPH)
                .on_press(Intent::Slowmode((now + 1).min(last)))
                .enabled(ready)
                .view()
        ),
    ]
    .spacing(INNER)
    .align_y(alignment::Vertical::Center);
    column![
        heading(tr!("SLOWMODE")),
        stepper,
        text(tr!("Members will be restricted to sending one message and creating one thread per this interval, unless they have Bypass Slowmode, Manage Channel or Manage Messages permissions."))
            .size(typography::size::LABEL)
            .style(catalog::text::muted),
    ]
    .spacing(INNER)
    .boxed()
}
