//! SVG icons. The default colours an icon as muted text, brighter under the
//! pointer, as the interface's icon buttons are; `colored` paints one colour.

use iced::Color;
use iced::widget::svg::{Status, Style};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(svg, icon);

pub fn icon(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    Style {
        color: Some(match status {
            Status::Idle => palette.muted,
            Status::Hovered => palette.text,
        }),
    }
}

/// One colour whatever the pointer does.
pub fn colored(color: Color) -> impl Fn(&Theme, Status) -> Style {
    move |_theme, _status| Style { color: Some(color) }
}

/// The SVG's own colours.
pub fn original(_theme: &Theme, _status: Status) -> Style {
    Style { color: None }
}
