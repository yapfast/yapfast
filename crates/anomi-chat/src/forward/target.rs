//! One place in the list (`ui/dialogs.slint:296`-`:327`): its picture or
//! channel icon, name, where it is, and a check box. A place the limit leaves
//! out is dimmed and takes no press.

use iced::widget::{button, column, container, row, svg, text};
use iced::{Alignment, Border, Color, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::forward::{Forward, Intent, Mark};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Avatar;
use anomi_widgets::avatar::circle;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};

pub const HEIGHT: f32 = 48.0;
/// `spacing: 12px`, the picture and the channel icon's round (`32px`, `18px`).
const PICTURE: f32 = metrics::AVATAR_ROW;
const GLYPH: f32 = 18.0;
/// The initial's size as a share of the picture (`ui/controls.slint:614`).
const INITIAL: f32 = 0.42;
/// The check box: 22 px, 6 px corners, a 2 px edge, a 16 px tick.
const BOX: f32 = 22.0;
const TICK: f32 = 16.0;
const BOX_EDGE: f32 = 2.0;
/// A place the limit leaves out (`opacity: 0.5`): iced has no layer opacity,
/// so each part fades by itself (the picture widget has `.opacity()`).
const DIM: f32 = 0.5;

pub fn view(forward: &Forward, index: usize, _motion: Motion) -> Element<'_, Intent> {
    let target = &forward.targets[index];
    let allowed = forward.can_choose(index);
    let mark: Element<'_, Intent> = match &target.mark {
        Mark::Avatar(avatar) if allowed => circle(avatar, PICTURE).boxed(),
        Mark::Avatar(avatar) => faded(avatar),
        Mark::Icon(glyph) => channel_icon(*glyph, allowed),
    };
    let lines = column![
        text(target.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| ink(theme.palette().text, allowed)),
        text(target.place.as_str())
            .size(typography::size::LABEL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| ink(theme.palette().muted, allowed)),
    ]
    .width(Fill);
    let content = row![mark, lines, check(target.chosen, allowed)]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center);
    let button = button(container(content).center_y(HEIGHT))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .height(HEIGHT)
        .style(catalog::button::plain);
    if allowed {
        button.on_press(Intent::Choose(target.id.clone())).boxed()
    } else {
        button.boxed()
    }
}

fn fade(color: Color, allowed: bool) -> Color {
    if allowed {
        color
    } else {
        Color {
            a: color.a * DIM,
            ..color
        }
    }
}

fn ink(color: Color, allowed: bool) -> text::Style {
    text::Style {
        color: Some(fade(color, allowed)),
        ..text::Style::default()
    }
}

/// A picture or initial of a place the limit leaves out, faded.
fn faded(avatar: &Avatar) -> Element<'_, Intent> {
    match &avatar.picture {
        Some(handle) => picture(handle.clone())
            .side(PICTURE)
            .fit(Fit::Cover)
            .round()
            .opacity(DIM)
            .boxed(),
        None => {
            let tint = avatar.tint;
            container(
                text(avatar.initial.as_str())
                    .size(PICTURE * INITIAL)
                    .font(typography::BOLD)
                    .color(Color::WHITE.scale_alpha(DIM)),
            )
            .center(PICTURE)
            .style(move |theme: &Theme| {
                let base = if tint.a == 0.0 {
                    theme.palette().avatar
                } else {
                    tint
                };
                container::Style {
                    background: Some(base.scale_alpha(DIM).into()),
                    border: Border::default().rounded(PICTURE / 2.0),
                    ..container::Style::default()
                }
            })
            .boxed()
        }
    }
}

/// The channel's icon on a round `field` ground.
fn channel_icon(glyph: Icon, allowed: bool) -> Element<'static, Intent> {
    container(
        icon(glyph, GLYPH)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().secondary),
            })
            .opacity(if allowed { 1.0 } else { DIM }),
    )
    .center(PICTURE)
    .style(move |theme: &Theme| container::Style {
        background: Some(fade(theme.palette().field, allowed).into()),
        border: Border::default().rounded(PICTURE / 2.0),
        ..container::Style::default()
    })
    .boxed()
}

/// The box: filled with the accent and ticked when chosen (a chosen place is
/// always allowed, so these never fade), an empty edge otherwise, faded when
/// the limit leaves the place out.
fn check(chosen: bool, allowed: bool) -> Element<'static, Intent> {
    let tick: Element<'static, Intent> = if chosen {
        icon(Icon::Check, TICK)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed()
    } else {
        container("").boxed()
    };
    container(tick)
        .center(BOX)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let edge = if chosen {
                palette.accent
            } else {
                fade(palette.faint, allowed)
            };
            container::Style {
                background: chosen.then(|| palette.accent.into()),
                border: Border::default()
                    .rounded(metrics::RADIUS_ROW)
                    .width(BOX_EDGE)
                    .color(edge),
                ..container::Style::default()
            }
        })
        .boxed()
}
