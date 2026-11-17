//! Tables: both separators are the line colour, as the settings' dividers
//! (`ui/moderation.slint:352`).

use iced::Background;
use iced::widget::table::Style;

use super::catalog;
use crate::Theme;

catalog!(table, default);

pub fn default(theme: &Theme) -> Style {
    let line = Background::Color(theme.palette().line);
    Style {
        separator_x: line,
        separator_y: line,
    }
}
