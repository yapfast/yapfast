//! The upload dialog: the emoji as chat will show it, its name, Cancel and
//! Upload. Slint's `EmojiUploadDialog` (`ui/server_expressions.slint:182`).
//! The card is the dialog's own; the backdrop, the way in, Escape
//! (`Intent::Cancel`) and the focus are the layer's.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::emoji::{Intent, Upload};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use crate::kit;

const WIDTH: f32 = 440.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const BUTTONS: f32 = 8.0;
const TITLE: f32 = 18.0;
const PREVIEW: f32 = 144.0;
const SIZES: [f32; 3] = [128.0, 48.0, 22.0];

/// The name field's id: the app focuses it as the dialog opens, as Slint's
/// does (`init => { self.focus-input(); }`).
pub const NAME: &str = "emoji-upload-name";

fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: catalog::float::dialog(theme).shadow,
        ..container::Style::default()
    }
}

fn sunken(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().sunken.into()),
        border: Border::default().rounded(8),
        ..container::Style::default()
    }
}

/// The dialog over the settings; `upload` is what `Emoji::upload` holds.
pub fn dialog(upload: &Upload, motion: Motion) -> Element<'_, Intent> {
    let mut sizes = row![].spacing(24).align_y(Alignment::Center);
    if let Some(handle) = upload.preview.ready() {
        for side in SIZES {
            sizes = sizes.push(picture(handle.clone()).side(side).fit(Fit::Contain));
        }
    }
    let mut name = field(tr!("Emoji name"), &upload.name)
        .id(NAME)
        .motion(motion);
    if !upload.saving {
        name = name.on_input(Intent::Name).on_submit(Intent::Finish);
    }
    let mut parts = column![
        text(tr!("Upload Emoji"))
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        container(sizes)
            .center(Length::Fill)
            .height(PREVIEW)
            .style(sunken),
        column![
            kit::heading(tr!("EmojiUploadDialog" => "NAME")),
            name.view()
        ]
        .spacing(8),
    ]
    .spacing(SPACING);
    if !upload.error.is_empty() {
        parts = parts.push(
            text(&upload.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Length::Fill)
                .boxed(),
        );
    }
    let mut cancel = action_button(tr!("Cancel"), Kind::Ghost, motion).enabled(!upload.saving);
    let mut send = action_button(
        if upload.saving {
            tr!("Uploading...")
        } else {
            tr!("Upload")
        },
        Kind::Primary,
        motion,
    )
    .enabled(upload.may_finish && !upload.saving);
    if !upload.saving {
        cancel = cancel.on_press(Intent::Cancel);
    }
    if upload.may_finish && !upload.saving {
        send = send.on_press(Intent::Finish);
    }
    parts = parts.push(
        row![space().width(Length::Fill), cancel, send]
            .spacing(BUTTONS)
            .boxed(),
    );
    container(parts)
        .padding(PADDING)
        .width(WIDTH)
        .style(card)
        .boxed()
}
