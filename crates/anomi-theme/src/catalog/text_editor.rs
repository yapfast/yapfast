//! The multi-line editor: the composer and longer fields.

use iced::widget::text_editor::{Status, Style};
use iced::{Background, Border};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(text_editor, composer);

/// `ComposerFrame`'s corner radius (`ui/composer.slint:77`).
const COMPOSER_RADIUS: f32 = 10.0;

/// The message box: Discord's composer colour with its outline, which a
/// focused one swaps for the muted ring (`ui/composer.slint:72-78`). Its text
/// is `ui/composer-field.slint:102`, `:175`.
pub fn composer(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let (border, placeholder, value) = match status {
        Status::Focused { .. } => (
            palette.muted.scale_alpha(0.55),
            palette.placeholder(),
            palette.text,
        ),
        Status::Disabled => (palette.outline_field, palette.faint, palette.muted),
        Status::Active | Status::Hovered => {
            (palette.outline_field, palette.placeholder(), palette.text)
        }
    };
    Style {
        background: Background::Color(palette.composer()),
        border: Border::default()
            .rounded(COMPOSER_RADIUS)
            .width(1)
            .color(border),
        placeholder,
        value,
        selection: palette.accent.scale_alpha(0.35),
    }
}
