//! A row of a settings sidebar: an icon and a page's name on the sidebar's
//! ground, lit as the page chosen or under the pointer, red for a dangerous
//! one. The Slint side: `ChannelRow` (`ui/rail.slint:397`) as the settings'
//! `SectionsNavigation` and `SettingsNavigation` use it
//! (`ui/settings.slint:368`, `ui/yapfast/settings-nav.slint:10`).

use std::borrow::Cow;

use iced::widget::{container, stack};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::icon::Icon;
use anomi_widgets::new_tag::new_tag;

use super::leaf::{Ink, Leaf};
use super::press::mix;

/// A row's height (`height: 40px`).
pub const HEIGHT: f32 = 40.0;
/// The icon's side and the row's insets (`ui/rail.slint:600`, `:628`).
const GLYPH: f32 = metrics::ICON_ROW;
const INSET: f32 = metrics::ROW_INSET;
/// The gap between the glyph and the words (`rail.slint:600`).
const GAP: f32 = 8.0;
/// The NEW tag's width, in Russian the widest (`НОВОЕ` and its padding).
const TAG: f32 = 48.0;

/// A section's row: `chosen` raises it, `danger` reddens it, `fresh` ends it
/// with a NEW tag.
pub fn nav_row<'a, M: Clone + 'a>(
    icon: Icon,
    text: impl Into<Cow<'a, str>>,
    chosen: bool,
    danger: bool,
    fresh: bool,
    on_press: M,
    motion: Motion,
) -> Element<'a, M> {
    let mut leaf = Leaf::new(text, motion, move |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        let rest = if chosen {
            palette.active
        } else {
            iced::Color::TRANSPARENT
        };
        let fill = if chosen {
            rest
        } else {
            mix(rest, palette.hover, lit)
        };
        let text = if danger {
            palette.danger
        } else if chosen {
            palette.text
        } else {
            mix(palette.muted, palette.secondary, lit)
        };
        let glyph = if danger {
            palette.danger
        } else if chosen {
            palette.text
        } else {
            palette.faint
        };
        Ink { fill, text, glyph }
    })
    .on_press(on_press);
    leaf.icon = Some(icon);
    leaf.glyph = GLYPH;
    leaf.size = typography::size::BODY;
    leaf.font = typography::MEDIUM;
    leaf.pad = INSET;
    if fresh {
        leaf.tail = TAG + GAP;
    }
    leaf.gap = GAP;
    leaf.height = HEIGHT;
    leaf.stretch = true;
    leaf.radius = metrics::RADIUS_ROW;
    let row = container(leaf).width(Length::Fill);
    if !fresh {
        return row.boxed();
    }
    // The NEW tag ends the row, over its ground.
    let tag = container(new_tag())
        .width(Length::Fill)
        .height(Length::Fixed(HEIGHT))
        .padding(Padding::ZERO.right(INSET))
        .align_x(alignment::Horizontal::Right)
        .align_y(alignment::Vertical::Center);
    stack![row, tag].boxed()
}
