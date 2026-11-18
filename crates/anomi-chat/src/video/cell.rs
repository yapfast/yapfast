//! A video in a gallery's cell (`VideoCell`, `ui/messages.slint:524`),
//! drawn over its poster by the gallery that holds it: the play disc until
//! it plays here; then its frame on black once one came, "Loading..." until
//! then, and its controls at the bottom on a dark fade while the pointer is
//! on it or it's paused. An ended video shows the disc again while the
//! pointer is off it; one that failed says why at the bottom, with the disc
//! to try again.
//!
//! A click on it plays or pauses the video playing here and starts it
//! otherwise, at the cell's size; a right click is the gallery's (its media
//! menu), so this layer takes no right press.

use iced::widget::{container, space, stack};
use iced::{Background, Color, Fill, gradient};

use anomi_theme::prelude::*;
use anomi_vm::chat::video::{Intent, Video};
use anomi_widgets::menu::opener;

use super::controls::controls;
use super::hovered::hovered;
use super::look::{self, DARK};

/// The controls' strip (`:570`).
const STRIP: f32 = 36.0;

pub fn cell(video: &Video, motion: Motion) -> Element<'_, Intent> {
    let mut ground: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(player) = &video.player {
        if let Some(frame) = &player.frame {
            ground.push(look::frame(frame));
        }
        if video.loading() {
            ground.push(look::loading());
        }
        if let Some(why) = &player.error {
            ground.push(look::failure(why, true));
        }
    }
    ground.push(space().width(Fill).height(Fill).boxed());
    let surface = opener(stack(ground).width(Fill).height(Fill))
        .on_press(move |bounds| video.click(bounds.width, bounds.height));
    stack![
        surface,
        hovered(over(video, false, motion), over(video, true, motion))
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// What lies over the picture with the pointer `on` it or not: the disc,
/// the controls, or neither.
fn over(video: &Video, on: bool, motion: Motion) -> Element<'_, Intent> {
    let mut layers: Vec<Element<'_, Intent>> = Vec::new();
    if video.disc(on) {
        layers.push(look::disc());
    }
    if let Some(player) = video.player.as_ref().filter(|_| video.controls(on)) {
        let strip = container(controls(player, false, motion))
            .width(Fill)
            .height(STRIP)
            .style(|_: &Theme| container::Style {
                background: Some(fade()),
                ..container::Style::default()
            });
        layers.push(
            container(strip)
                .width(Fill)
                .height(Fill)
                .align_bottom(Fill)
                .boxed(),
        );
    }
    if layers.is_empty() {
        return space().width(Fill).height(Fill).boxed();
    }
    stack(layers).width(Fill).height(Fill).boxed()
}

/// Dark at the bottom, clear at the top (`@linear-gradient(0deg, #000000cc
/// 0%, #00000000 100%)`, `:571`).
fn fade() -> Background {
    Background::Gradient(
        gradient::Linear::new(iced::Degrees(180.0))
            .add_stop(0.0, Color::TRANSPARENT)
            .add_stop(1.0, DARK)
            .into(),
    )
}
