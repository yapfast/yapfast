//! Leaving a server or a group DM, and the move-sync confirmation: Slint's
//! `ChannelDialog` (`ui/server.slint:517`) over what `show_leave` and
//! `show_move_sync` fill (`src/ui/refresh.rs:3530`, `:3566`). Its title with
//! the X, what it asks, perhaps a box ("Leave without notifying other
//! members"), why Discord refused in red, then Cancel (or the alternative
//! answer) and the main button, red for a leave. Enter confirms, Escape and a
//! press outside close (the shell's layer), and nothing may be asked again
//! while it is sent.

pub(crate) mod card;
mod enter;

use iced::widget::{button, container, row, space, svg, text};
use iced::{Background, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::places::leave::{Intent, Leave};
use anomi_widgets::button::Kind;
use anomi_widgets::icon::{Icon as IconKind, icon};
use yapfast_lang::tr;

pub(super) use card::{buttons, card as dialog, title, title_row};
pub(super) use enter::enter;

/// The card's width: `max(460px, ..)` (`ui/server.slint:526`).
const WIDTH: f32 = 460.0;
/// The option's box (`ui/server.slint:572`): 18 px, radius 4, a 1.5 px ring,
/// a 12 px tick, 10 px before the label (`:567`), the label 14 px.
const BOX: f32 = 18.0;
const BOX_RADIUS: f32 = 4.0;
const BOX_RING: f32 = 1.5;
const TICK: f32 = 12.0;
const BOX_GAP: f32 = 10.0;
/// The box's column has `padding-top: 1px` (`ui/server.slint:573`); the label
/// sits at the top of the row.
const BOX_TOP: f32 = 1.0;
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
        container(buttons(
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

/// The box below the detail, its words beside it, the whole row one press
/// with a pointer cursor, inert while it is sent.
fn option(leave: &Leave, ready: bool) -> Element<'_, Intent> {
    let words = text(&leave.option)
        .size(typography::size::SMALL)
        .style(catalog::text::secondary)
        .width(Fill);
    let content = row![
        container(mark(leave.ticked)).padding(iced::Padding::ZERO.top(BOX_TOP)),
        words
    ]
    .spacing(BOX_GAP);
    button(content)
        .padding(0)
        .width(Fill)
        .style(catalog::button::bare)
        .on_press_maybe(ready.then_some(Intent::Tick))
        .boxed()
}

/// The box of `ChannelDialog`: the accent fill and ring ticked, a `muted`
/// ring empty, Lucide's check inside.
fn mark<'a>(on: bool) -> Element<'a, Intent> {
    let inner: Element<'a, Intent> = if on {
        icon(IconKind::Check, TICK)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed()
    } else {
        space().boxed()
    };
    container(inner)
        .center(Length::Fixed(BOX))
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let ring = if on { palette.accent } else { palette.muted };
            container::Style {
                background: on.then_some(Background::Color(palette.accent)),
                border: Border {
                    color: ring,
                    width: BOX_RING,
                    radius: BOX_RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
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
        shot("leave_group", || {
            sample::asked_in(anomi_vm::fixtures::shell::dm_home(), sample::group())
        }),
        shot("leave_refused", || sample::asked(sample::refused())),
        shot("leave_group_silent", || {
            sample::asked_in(anomi_vm::fixtures::shell::dm_home(), sample::silent())
        }),
        shot("leave_sending", || sample::asked(sample::leaving())),
        shot("move_sync", || sample::asked(sample::move_sync())),
    ]
}
