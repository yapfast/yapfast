//! The multi-line editor: the composer and longer fields.

use iced::widget::text_editor::{Status, Style};
use iced::{Background, Border};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(text_editor, composer);

/// The message box: Discord's composer colour with its outline.
pub fn composer(theme: &Theme, _status: Status) -> Style {
    let palette = theme.palette();
    Style {
        background: Background::Color(palette.composer()),
        border: Border::default()
            .rounded(crate::metrics::RADIUS_CARD)
            .width(1)
            .color(palette.outline_field),
        placeholder: palette.placeholder(),
        value: palette.text,
        selection: palette.accent.scale_alpha(0.35),
    }
}
