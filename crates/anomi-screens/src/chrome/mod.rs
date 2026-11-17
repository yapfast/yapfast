//! The window's own frame, drawn when the desktop's is off: a 32 px bar on
//! the rail's ground with back and forward on the left, where the person is
//! in the middle, the inbox and help (while the account's screens show) and
//! the minimize, maximize and close buttons on the right; and the edges that
//! resize it. The Slint side: `WindowBar`, `CaptionButton`, `BarButton`
//! (`ui/yapfast/window-bar.slint:74`, `:4`, `:31`), `TitleBar`, `WindowButton`
//! and `ResizeEdges` (`ui/title_bar.slint:28`, `:4`, `:87`) and
//! `src/ui/title_bar.rs`. `TitleBar` and `WindowButton` are the classic
//! style's, which the iced interface leaves out (docs/iced/tasks.md): their
//! job is `WindowBar`'s.
//!
//! [`view`] is the bar, empty while the desktop draws the frame or the
//! window is full screen; [`edges`] is the layer of resize strips, which the
//! window stacks over everything while the frame is ours and the window is
//! not maximized. Both ask with [`Intent`]s; the app moves, resizes and
//! closes the window (task I14).

mod bar;
mod button;
#[cfg(test)]
mod button_tests;
mod drag;
mod edges;
mod ink;

use iced::widget::{container, space};

use anomi_theme::prelude::*;
use anomi_vm::chrome::{Chrome, Intent};

pub use edges::{CORNER, EDGE, edge_at};

/// The bar's height (`ui/yapfast/window-bar.slint:91`); the window lowers
/// its content by it while the bar shows.
pub const HEIGHT: f32 = 32.0;

/// The bar, or nothing where the desktop draws the frame.
pub fn view(chrome: &Chrome, motion: Motion) -> impl View<Intent> + '_ {
    let content: Element<'_, Intent> = if chrome.bar() {
        bar::bar(chrome, motion)
    } else {
        space().boxed()
    };
    container(content)
}

/// The resize edges, over the whole window; nothing while the window is
/// maximized or the frame isn't ours.
pub fn edges(chrome: &Chrome) -> impl View<Intent> + '_ {
    let (content, length): (Element<'_, Intent>, _) = if chrome.edges() {
        (edges::edges(Intent::Resize).boxed(), iced::Length::Fill)
    } else {
        (space().boxed(), iced::Length::Shrink)
    };
    container(content).width(length).height(length)
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`): the bar in front and behind, with the history, signed
/// out, and the desktop's frame (no bar).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::{GOLDEN, Shot, chrome, shell};

    fn window(chrome: Chrome) -> Vm {
        Vm {
            chrome,
            ..shell::dm_home()
        }
    }
    fn shot(name: &'static str, vm: fn() -> Vm) -> Shot {
        Shot {
            area: "chrome",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("bar", || window(chrome::framed())),
        shot("bar_history_inbox", || window(chrome::history())),
        shot(
            "bar_maximized_behind",
            || window(chrome::maximized_behind()),
        ),
        shot("bar_signed_out", || Vm {
            chrome: chrome::signed_out(),
            ..shell::signed_out()
        }),
        shot("system_frame", || window(chrome::system())),
    ]
}
