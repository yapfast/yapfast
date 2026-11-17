//! Text fields: a search field on the sidebar, a field in a dialog.

use iced::widget::text_input::{Status, Style};
use iced::{Background, Border};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(text_input, field);

/// `TextField` (`ui/controls.slint:987`): the sunken well with a 1 px rim in
/// the line colour, the accent at 80% while it has the focus.
pub fn field(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let disabled = status == Status::Disabled;
    let outline = match status {
        Status::Focused { .. } => palette.accent.scale_alpha(0.8),
        Status::Active | Status::Hovered | Status::Disabled => palette.line,
    };
    Style {
        background: Background::Color(palette.sunken),
        border: Border::default()
            .rounded(crate::metrics::RADIUS_ROW)
            .width(1)
            .color(outline),
        placeholder: if disabled {
            palette.faint
        } else {
            palette.muted
        },
        value: if disabled {
            palette.muted
        } else {
            palette.text
        },
        selection: palette.accent.scale_alpha(0.35),
    }
}

/// The sidebar's search field, as "Find a DM": the field colour, no outline.
pub fn search(theme: &Theme, _status: Status) -> Style {
    let palette = theme.palette();
    Style {
        background: Background::Color(palette.field),
        border: Border::default().rounded(crate::metrics::RADIUS_ROW),
        placeholder: palette.muted,
        value: palette.text,
        selection: palette.accent.scale_alpha(0.35),
    }
}
