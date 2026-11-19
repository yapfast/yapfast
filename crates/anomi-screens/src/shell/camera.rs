//! The dialog a click on the camera opens when there is no camera to film
//! with, as Discord's (`Layer::CameraUnavailable`; the Slint app had none):
//! "Camera unavailable" with the X, "Oops, we couldn't find any camera." in
//! the muted colour, and one "OK" as wide as the card, on the card, title
//! and X of the confirmations (`places::leave::card`). It shows no
//! view-model: the X, "OK" and Enter send the shell's close of its layer,
//! as Escape and a press beside it do through the layer, and the app
//! focuses "OK" as it opens (`intent::voice::camera` in anomi-app).

use iced::widget::{Id, container, text};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::Intent;
use anomi_vm::shell::{self, Layer};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use crate::places::leave::{card, enter};

/// The card's width: Discord's small dialog.
const WIDTH: f32 = 440.0;
/// "OK", which the app focuses as the dialog opens.
pub const OK: &str = "camera-unavailable-ok";

/// The dialog, every answer closing its layer.
pub fn dialog<'a>(motion: Motion) -> Element<'a, Intent> {
    let close = Intent::Shell(shell::Intent::Close(Layer::CameraUnavailable));
    let ok = action_button(tr!("OK"), Kind::Primary, motion)
        .id(Id::new(OK))
        .width(Fill)
        .on_press(close.clone());
    let parts = vec![
        card::title_row(tr!("Camera unavailable"), close.clone(), motion),
        text(tr!("Oops, we couldn't find any camera."))
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .width(Fill)
            .boxed(),
        container(ok)
            .width(Fill)
            .padding(Padding::ZERO.top(metrics::SPACE_XS))
            .boxed(),
    ];
    enter(card::card(WIDTH, parts), Some(close)).boxed()
}

/// Its shot for `anomi-shots`: the window in a call, its camera button
/// unavailable, with the dialog over it. Compare it with
/// `discord-camera-unavailable-modal.png` among the owner's references.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    vec![Shot {
        area: "dialog",
        name: "camera_unavailable",
        size: GOLDEN,
        theme: "bonfire",
        vm: anomi_vm::fixtures::voice::dock::camera_unavailable_window,
    }]
}
