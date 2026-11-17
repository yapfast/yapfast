//! Leaving a server or a group DM, and the move-sync confirmation: Slint's
//! `ChannelDialog` (`ui/server.slint:517`) over what `show_leave` and
//! `show_move_sync` fill (`src/ui/refresh.rs:3530`, `:3566`). Its title with
//! the X, what it asks, perhaps a box ("Leave without notifying other
//! members"), why Discord refused in red, then Cancel (or the alternative
//! answer) and the main button, red for a leave. Enter confirms, Escape and a
//! press outside close (the shell's layer), and nothing may be asked again
//! while it is sent.

mod card;
mod enter;

use iced::widget::{checkbox, text};
use iced::{Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::places::leave::{Intent, Leave};
use anomi_widgets::button::Kind;
use yapfast_lang::tr;

pub(super) use card::{buttons, card as dialog, title, title_row};
pub(super) use enter::enter;

/// The card's width: `max(460px, ..)` (`ui/server.slint:526`).
const WIDTH: f32 = 460.0;
/// The detail is a `Label` (`ui/theme.slint:158`): 13 px, muted.
const DETAIL: f32 = typography::size::LABEL;

pub fn view(leave: &Leave, motion: Motion) -> impl View<Intent> + '_ {
    let ready = !leave.saving;
    let mut parts = vec![title_row(&leave.title, Intent::Close, motion)];
    if !leave.detail.is_empty() {
        parts.push(
            text(&leave.detail)
                .size(DETAIL)
                .style(catalog::text::muted)
                .width(Fill)
                .boxed(),
        );
    }
    if !leave.option.is_empty() {
        parts.push(option(leave, ready));
    }
    if !leave.error.is_empty() {
        parts.push(
            text(&leave.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Fill)
                .boxed(),
        );
    }
    let (cancel, on_cancel) = if leave.alternative.is_empty() {
        (tr!("Cancel"), Intent::Close)
    } else {
        (leave.alternative.as_str(), Intent::Alternative)
    };
    let kind = if leave.danger {
        Kind::Danger
    } else {
        Kind::Primary
    };
    parts.push(
        iced::widget::container(buttons(
            (cancel, on_cancel),
            (&leave.confirm, kind, Intent::Confirm),
            ready,
            motion,
        ))
        .padding(iced::Padding::ZERO.top(metrics::SPACE_XS))
        .boxed(),
    );
    enter(dialog(WIDTH, parts), ready.then_some(Intent::Confirm))
}

/// The box below the detail, its words beside it, the whole row a press.
fn option(leave: &Leave, ready: bool) -> Element<'_, Intent> {
    let mark = checkbox(leave.ticked)
        .size(metrics::MARK_SIZE)
        .spacing(metrics::SPACE_S + 2.0)
        .on_toggle_maybe(ready.then_some(|_| Intent::Tick));
    mark.label(&leave.option)
        .text_size(typography::size::SMALL)
        .width(Length::Fill)
        .boxed()
}

/// Its shots for `anomi-shots`: no golden shot exists, so they are compared
/// with the Slint app's dialogs under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::places::leave as sample;

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "dialog",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("leave_server", || sample::asked(sample::server())),
        shot("leave_group", || sample::asked(sample::group())),
        shot("leave_refused", || sample::asked(sample::refused())),
        shot("move_sync", || sample::asked(sample::move_sync())),
    ]
}
