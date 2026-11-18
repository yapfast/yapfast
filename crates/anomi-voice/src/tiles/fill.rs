//! A tile's ground and what fills it: someone's picture on the raised
//! ground, or on black their camera, a watched stream (`StreamPicture`,
//! `ui/voice.slint:840`), a stream's preview (`StreamPreview`, `:849`), a
//! stream offered in a tile of its own (`:1034`) or this account's screen
//! (`:999`).
//!
//! Frames are read as they're drawn, so a new one builds no view, and are
//! drawn square and whole, never copied; `ring` rounds the corners over
//! them.

use iced::widget::{Space, column, container, responsive, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Size};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::{Avatar, Live};
use anomi_vm::voice::tiles::{Fill as Filling, Intent, Tile};
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::picture::{Fit, live, picture};

use super::{GREY, waiting};

/// Someone's picture: 45% of the tile's height, at most 80 px, 8 px above
/// its middle (`:1085`, `:1086`), at half strength until they connect
/// (`:1087`).
const AVATAR_SHARE: f32 = 0.45;
const AVATAR_MAX: f32 = 80.0;
const AVATAR_RAISE: f32 = 8.0;
const UNCONNECTED: f32 = 0.5;
/// What dims a preview so what's over it reads (`:852`).
const DIM: Color = anomi_theme::rgba(0x0000_0059);
/// The screen over a stream offered, 30 px, and over this account's screen
/// before its first picture, 34; each 10 px above what follows (`:1001`,
/// `:1003`, `:1035`, `:1036`).
const OFFER_ICON: f32 = 30.0;
const SHARE_ICON: f32 = 34.0;
const GAP: f32 = 10.0;
/// This account's screen's status, 14 px (`:1004`).
const STATUS: f32 = typography::size::SMALL;

pub fn view<'a>(tile: &'a Tile, radius: f32, motion: Motion) -> Element<'a, Intent> {
    let content: Element<'a, Intent> = match &tile.fill {
        Filling::Avatar { avatar, connected } => face(avatar, *connected),
        Filling::Camera(frame)
        | Filling::Watched {
            frame: Some(frame), ..
        }
        | Filling::OwnScreen {
            frame: Some(frame), ..
        } => whole(frame),
        Filling::Watched {
            frame: None,
            status,
        } => waiting::waiting(status, motion).boxed(),
        Filling::Preview(preview) => dimmed(preview),
        Filling::Offer { preview } => offer(preview.as_ref()),
        Filling::OwnScreen {
            frame: None,
            status,
        } => screen_starting(status),
    };
    let black = tile.fill.on_black();
    container(content)
        .width(Fill)
        .height(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(if black {
                Color::BLACK
            } else {
                theme.palette().raised
            })),
            border: Border::default().rounded(radius),
            ..container::Style::default()
        })
        .boxed()
}

/// Their picture in the middle, sized by the tile; a not yet connected
/// one's under a veil of the ground at half strength, which is what half
/// its opacity looks like on that ground.
fn face(avatar: &Avatar, connected: bool) -> Element<'_, Intent> {
    responsive(move |size: Size| {
        let side = (size.height * AVATAR_SHARE).min(AVATAR_MAX).floor();
        let face = portrait(avatar, side).view();
        let face: Element<'_, Intent> = if connected {
            face.boxed()
        } else {
            stack![face, veil(side)].boxed()
        };
        container(face)
            .center(Fill)
            .padding(Padding::ZERO.bottom(2.0 * AVATAR_RAISE))
    })
    .boxed()
}

fn veil<'a>(side: f32) -> impl View<Intent> + 'a {
    container(Space::new())
        .width(side)
        .height(side)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(
                theme.palette().raised.scale_alpha(1.0 - UNCONNECTED),
            )),
            border: Border::default().rounded(side / 2.0),
            ..container::Style::default()
        })
}

/// The newest frame, whole and as large as fits.
fn whole(frames: &Live) -> Element<'_, Intent> {
    live(frames)
        .width(Fill)
        .height(Fill)
        .fit(Fit::Contain)
        .boxed()
}

/// A stream's preview filling the tile, dimmed (`StreamPreview`).
fn dimmed(preview: &iced::widget::image::Handle) -> Element<'_, Intent> {
    let dim = container(Space::new())
        .width(Fill)
        .height(Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(DIM)),
            ..container::Style::default()
        });
    stack![
        picture(preview.clone())
            .width(Fill)
            .height(Fill)
            .fit(Fit::Cover),
        dim,
    ]
    .boxed()
}

/// A stream not watched, in a tile of its own: the screen over the pill
/// that offers it, white on its preview, grey on black.
fn offer(preview: Option<&iced::widget::image::Handle>) -> Element<'_, Intent> {
    let ink = if preview.is_some() {
        Color::WHITE
    } else {
        GREY
    };
    let middle = column![
        icon(Icon::ScreenShare, OFFER_ICON).style(anomi_theme::catalog::svg::colored(ink)),
        super::marks::watch_pill(),
    ]
    .spacing(GAP)
    .align_x(Alignment::Center);
    let middle = container(middle).center(Fill);
    match preview {
        Some(preview) => stack![dimmed(preview), middle].boxed(),
        None => middle.boxed(),
    }
}

/// This account's screen before its first picture: the screen over how
/// it's going.
fn screen_starting(status: &str) -> Element<'_, Intent> {
    let status = text(status)
        .size(STATUS)
        .color(GREY)
        .width(Fill)
        .align_x(Alignment::Center);
    let icon = icon(Icon::ScreenShare, SHARE_ICON).style(anomi_theme::catalog::svg::colored(GREY));
    container(
        column![icon, status]
            .spacing(GAP)
            .align_x(Alignment::Center),
    )
    .center_y(Fill)
    .width(Fill)
    .boxed()
}
