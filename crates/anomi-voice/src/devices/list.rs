//! The list of devices under the menu's head (`ui/controls.slint:1627`): a
//! row each, the one in use checked, five of them high before it scrolls, or
//! the line saying there are none. It opens and closes in place.

use iced::widget::{column, container};
use iced::{Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::voice::devices::{Devices, Intent};
use anomi_widgets::{list, menu};

use super::reveal::reveal;

/// A device's row (`MenuItem` with no subtitle).
const ROW: f32 = 32.0;
/// How many rows show before the list scrolls.
const SHOWN: usize = 5;
/// The room under the list, which Slint's `list-height` keeps.
const BELOW: f32 = 4.0;

pub fn list(devices: &Devices, motion: Motion) -> impl View<Intent> + '_ {
    let body: Element<'_, Intent> = if devices.devices.is_empty() {
        container(typography::label(devices.empty()))
            .padding(
                Padding::new(0.0)
                    .left(metrics::ROW_INSET)
                    .right(metrics::ROW_INSET),
            )
            .width(Length::Fill)
            .height(ROW)
            .boxed()
    } else {
        let rows = column(
            devices
                .devices
                .iter()
                .map(|row| menu::item(row, motion).boxed()),
        );
        if devices.devices.len() > SHOWN {
            container(list::scroll_view(rows, motion))
                .height(SHOWN as f32 * ROW)
                .boxed()
        } else {
            rows.boxed()
        }
    };
    reveal(
        column![body, iced::widget::space().height(BELOW)],
        devices.listing,
        motion,
    )
}
