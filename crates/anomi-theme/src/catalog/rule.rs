//! Rules: the dividers between parts of the window and inside lists.

use iced::widget::rule::{FillMode, Style};

use super::catalog;
use crate::Theme;

catalog!(rule, line);

/// A divider on any ground.
pub fn line(theme: &Theme) -> Style {
    Style {
        color: theme.palette().line,
        radius: 0.0.into(),
        fill_mode: FillMode::Full,
        snap: true,
    }
}

/// The outline under the header and beside the sidebar.
pub fn outline(theme: &Theme) -> Style {
    Style {
        color: theme.palette().outline,
        ..line(theme)
    }
}

/// The lines on the conversation's side.
pub fn outline_chat(theme: &Theme) -> Style {
    Style {
        color: theme.palette().outline_chat,
        ..line(theme)
    }
}
