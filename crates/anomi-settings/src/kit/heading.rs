//! The words of a settings page's top: its title, and the heading over a
//! group of settings or sections. The Slint side: `SettingsTitle` and
//! `SettingsHeading` (`ui/settings.slint:17`, `:21`). Discord's layout puts
//! the title in the frame's bar (15 px), so a page draws it only in the
//! classic layout, which Anomi doesn't have; what the pages use is
//! [`heading`] and, for a section's own title in a page, [`title`].

use iced::widget::{Text, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};

/// A page's own title: 24 px bold (`ui/settings.slint:18`).
const TITLE: f32 = 24.0;

/// A page's own title: 24 px bold (`ClientStyle.enabled`).
pub fn title<'a>(words: impl text::IntoFragment<'a>) -> Text<'a, Theme> {
    text(words)
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
}

/// A group's heading: 12 px bold, muted. Slint spaces its letters by
/// 0.6 px, which iced's text can't.
pub fn heading<'a>(words: impl text::IntoFragment<'a>) -> Text<'a, Theme> {
    text(words)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(catalog::text::muted)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}
