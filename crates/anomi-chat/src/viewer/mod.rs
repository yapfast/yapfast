//! The image viewer: a picture shown large over the dimmed window
//! (`ImageViewer`, `ui/messages.slint:2610`), opened as `Layer::Viewer`,
//! whose backdrop and Escape are the shell's.
//!
//! - [`view`]: the picture in the middle, shrunk to fit and never enlarged
//!   ([`stage`]), the bar over it with the name, "2 / 3", Save and Close, an
//!   arrow at each side to step through the message's pictures when it has
//!   more than one, and a pill at the bottom while the full image loads or
//!   says why it can't. A press on the picture zooms in and back (the wheel,
//!   `+`, `-` and `0` too); a press anywhere else closes. The left and right
//!   arrow keys step ([`keys`]).
//! - A right click on the picture opens [`menu`]: "Copy Image", "Save
//!   Image", "Copy Link", "Open Link", as `ViewerMenu` has them. The menu is
//!   the window's (`vm.menu`), which the app fills on `Intent::Menu`.
//!
//! The picture arrives with the layer's own motion (the dim and the shell's
//! pace); a moving image plays only while the window is in front.
//! A zoomed picture is clipped to the room and a left drag pans it
//! ([`overflow`]).

mod bar;
mod keys;
pub mod menu;
pub mod overflow;
mod stage;

use iced::Fill;
use iced::widget::{mouse_area, space, stack};

use anomi_theme::prelude::*;
use anomi_vm::chat::viewer::{Intent, Viewer};

pub fn view(viewer: &Viewer, motion: Motion) -> impl View<Intent> + '_ {
    // A press on what isn't the picture, the bar's buttons or an arrow.
    let outside = mouse_area(space().width(Fill).height(Fill)).on_press(Intent::Close);
    let mut layers = vec![
        outside.boxed(),
        stage::stage(viewer, motion),
        bar::bar(viewer, motion),
    ];
    if viewer.steps() {
        layers.push(bar::arrow(false));
        layers.push(bar::arrow(true));
    }
    layers.extend(bar::note(viewer));
    let layers = stack(layers).width(Fill).height(Fill);
    keys::keys(layers, viewer.steps())
}

/// Its shots for `anomi-shots`: the whole window from a fixture (see
/// `crate::shots`). No golden shot: compare with the Slint app by hand.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::{GOLDEN, Shot, chat::viewer as fixture};

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "viewer",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("several", fixture::several),
        shot("single", fixture::single),
        shot("loading", fixture::loading),
        shot("failed", fixture::failed),
        shot("zoomed", fixture::zoomed),
        shot("menu", || {
            fixture::with_menu(fixture::several(), 400.0, 300.0, menu::rows)
        }),
    ]
}
