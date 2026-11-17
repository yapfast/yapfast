//! The server's header at the top of its channel list: its name; the
//! banner, verified tick, invite button and dropdown come with N05
//! (docs/iced/tasks/shell.md). The Slint side: `ServerHeader`
//! (`ui/rail.slint:946`).

use iced::widget::{container, text};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::sidebar::server_header::{Intent, ServerHeader};

pub fn view(header: &ServerHeader, _motion: Motion) -> impl View<Intent> + '_ {
    container(
        text(header.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::SEMIBOLD),
    )
    .padding([0.0, metrics::SPACE_L])
    .center_y(metrics::HEADER_HEIGHT - 1.0)
}

/// Its shots for `anomi-shots`: none until N05 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
