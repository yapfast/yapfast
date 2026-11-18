//! A channel's permissions page: the synced notice, the private switch and
//! the advanced permissions, a list of roles and members beside the shown
//! one's permissions with their three buttons. The Slint side:
//! `PermissionsSection` (`ui/channel_settings.slint:221`). The frame draws
//! it at the full height of its card, so each list scrolls by itself. Add's
//! picker and the questions are [`add`] and [`confirm`], which the shell
//! draws as dialogs over the settings.

mod dialogs;
mod rows;
mod three;

use iced::widget::{column, container, row, text};
use iced::{Border, Fill, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::permissions::{Intent, Line, Permissions, Sync};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::list::list;
use yapfast_lang::tr;

use crate::kit::{heading, switch};

pub use dialogs::{SEARCH, add, confirm};

const SPACING: f32 = 16.0;
const COLUMNS: f32 = 24.0;
const LEFT: f32 = 220.0;
const HEAD: f32 = 28.0;
const ROW_GAP: f32 = 4.0;

fn card(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().raised.into()),
        border: Border::default().rounded(metrics::RADIUS_CARD),
        ..container::Style::default()
    }
}

pub fn view(permissions: &Permissions, motion: Motion) -> impl View<Intent> + '_ {
    let ready = permissions.ready();
    let mut parts: Vec<Element<'_, Intent>> =
        vec![typography::label(&permissions.subtitle).width(Fill).boxed()];
    if let Some(notice) = notice(permissions, motion) {
        parts.push(notice);
    }
    let private = switch(&permissions.private_title, permissions.private, motion)
        .description(&permissions.private_text)
        .locked(&permissions.private_lock)
        .enabled(ready && permissions.private_lock.is_empty())
        .on_toggle(Intent::Private);
    parts.push(
        container(private.view())
            .padding(16)
            .width(Fill)
            .style(card)
            .boxed(),
    );
    parts.push(heading(tr!("ADVANCED PERMISSIONS")).boxed());
    parts.push(
        row![
            overwrites(permissions, ready, motion),
            lines(permissions, ready, motion)
        ]
        .spacing(COLUMNS)
        .height(Fill)
        .boxed(),
    );
    column(parts).spacing(SPACING).width(Fill).height(Fill)
}

/// Whether the channel follows its category, and Sync Now when it doesn't.
fn notice(permissions: &Permissions, motion: Motion) -> Option<Element<'_, Intent>> {
    let words = match permissions.sync {
        Sync::None => return None,
        Sync::Synced => tr!("Permissions synced with category: {}", permissions.category),
        Sync::Apart => tr!(
            "Permissions not synced with category: {}",
            permissions.category
        ),
    };
    let mut line = row![
        text(words)
            .size(typography::size::SMALL)
            .style(catalog::text::secondary)
            .width(Fill)
            .boxed()
    ]
    .spacing(12)
    .align_y(alignment::Vertical::Center);
    if permissions.sync == Sync::Apart {
        let mut now =
            action_button(tr!("Sync Now"), Kind::Primary, motion).enabled(permissions.ready());
        if permissions.ready() {
            now = now.on_press(Intent::SyncNow);
        }
        line = line.push(now.boxed());
    }
    Some(
        container(line)
            .padding(Padding {
                top: 12.0,
                bottom: 12.0,
                left: 16.0,
                right: 12.0,
            })
            .width(Fill)
            .style(card)
            .boxed(),
    )
}

/// "ROLES/MEMBERS" with its add button, over the overwrites.
fn overwrites(permissions: &Permissions, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let mut add = icon_button(Icon::CirclePlus, tr!("Add a role or member"), motion)
        .size(HEAD)
        .glyph(16.0)
        .enabled(ready);
    if ready {
        add = add.on_press(Intent::Add);
    }
    let head = row![
        container(heading(tr!("ROLES/MEMBERS"))).width(Fill),
        add.view()
    ]
    .spacing(8)
    .height(HEAD)
    .align_y(alignment::Vertical::Center);
    let list = list(
        permissions.overwrites.len(),
        |i| i as u64,
        move |i| rows::overwrite(&permissions.overwrites[i], motion),
        motion,
    )
    .height(Fill);
    column![head, list]
        .spacing(ROW_GAP)
        .width(LEFT)
        .height(Fill)
        .boxed()
}

/// The shown overwrite's permissions, made only in sight as Slint's
/// `ListView` does, and its Remove.
fn lines(permissions: &Permissions, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let lines = list(
        permissions.lines.len(),
        |i| i as u64,
        move |i| entry(&permissions.lines[i], i, ready, motion),
        motion,
    )
    .height(Fill);
    let mut parts: Vec<Element<'_, Intent>> = vec![lines.boxed()];
    if !permissions.shown_everyone {
        parts.push(remove(&permissions.remove, ready, motion));
    }
    column(parts).spacing(8).width(Fill).height(Fill).boxed()
}

/// A group's heading or a permission's line, with the room above it.
fn entry(line: &Line, index: usize, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let above = if !line.heading {
        4.0
    } else if index == 0 {
        0.0
    } else {
        8.0
    };
    let inner = if line.heading {
        container(heading(&line.title))
            .height(HEAD)
            .align_y(alignment::Vertical::Center)
            .boxed()
    } else {
        three::line(line, ready, motion)
    };
    container(inner).padding(Padding::ZERO.top(above)).boxed()
}

fn remove<'a>(words: &'a str, ready: bool, motion: Motion) -> Element<'a, Intent> {
    let mut button = action_button(words, Kind::Danger, motion).enabled(ready);
    if ready {
        button = button.on_press(Intent::Remove);
    }
    button.boxed()
}

/// Its shots for `anomi-shots`: the page as the golden screenshot has it,
/// and its other states.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::permissions as sample;

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "csettings",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("permissions", || sample::asked(sample::permissions())),
        shot("permissions_apart", || sample::asked(sample::apart())),
        shot("permissions_locked", || sample::asked(sample::locked())),
        shot("permissions_saving", || sample::asked(sample::saving())),
        shot("permissions_confirm", || {
            sample::confirm_open(anomi_vm::settings::permissions::Confirm::Remove)
        }),
        shot("permissions_add", sample::adding_open),
    ]
}
