//! The body of every button: a leaf widget that draws its own fill, icons
//! and text and keeps its hover, press and focus. `ActionButton`,
//! `PingToggle`, `IconButton`, `DeviceToggle`'s caret, `CallButton` and
//! `HeaderToggle` (`ui/controls.slint:8`, `:84`, `:105`, `:179`, `:1070`,
//! `:1674`) are this widget given another look; see their modules.

use iced::advanced::widget::Id;
use iced::{Font, Length, Point, Rectangle, Size};

use anomi_theme::{Motion, Theme, metrics, typography};

use super::look::{Colours, Look, between};
use super::state::State;
use crate::icon::Icon;

/// The gap between the shape and its caption (`ui/controls.slint:1077`).
pub(super) const CAPTION_GAP: f32 = 6.0;
/// A caption's line, 13 px text (`:1095`).
pub(super) const CAPTION_HEIGHT: f32 = 17.0;
pub(super) const CAPTION_SIZE: f32 = typography::size::LABEL;
/// A pressed face gives to this much of its size (`ui/controls.slint:38`).
pub const PRESSED: f32 = 0.97;
/// The focus ring's width (`ui/controls.slint:40`).
pub(super) const RING: f32 = 2.0;

pub struct Face<'a, Message> {
    pub(super) id: Option<Id>,
    pub(super) look: Look<'a>,
    pub(super) motion: Motion,
    pub(super) on_press: Option<Message>,
    pub(super) enabled: bool,
    /// The flag the look varies on (a menu open, a toggle on, an alert): its
    /// colours blend as it eases, and a `turns` face turns half way round.
    pub(super) open: bool,
    /// The leading icon is a chevron drawn as a path, turned by `open`.
    pub(super) turns: bool,
    pub(super) leading: Option<Icon>,
    pub(super) text: &'a str,
    pub(super) trailing: Option<Icon>,
    pub(super) glyph: f32,
    pub(super) text_size: f32,
    pub(super) trailing_glyph: f32,
    pub(super) font: Font,
    pub(super) padding: f32,
    pub(super) spacing: f32,
    /// The shape's smallest size; its width is exactly this when `fixed`.
    pub(super) min: Size,
    pub(super) fixed: bool,
    /// The width a caller asks for; none leaves it to the face's content.
    pub(super) width: Option<Length>,
    pub(super) radius: f32,
    /// How far a press gives: [`PRESSED`], or 1 for a face that doesn't.
    pub(super) press: f32,
    pub(super) caption: Option<&'a str>,
}

impl<'a, Message> Face<'a, Message> {
    pub(super) fn new(
        motion: Motion,
        look: impl Fn(&Theme, bool, bool, bool) -> Colours + 'a,
    ) -> Self {
        Self {
            id: None,
            look: Box::new(look),
            motion,
            on_press: None,
            enabled: true,
            open: false,
            turns: false,
            leading: None,
            text: "",
            trailing: None,
            glyph: metrics::ICON_ROW,
            text_size: typography::size::BODY,
            trailing_glyph: super::action::TRAILING,
            font: typography::BOLD,
            padding: 0.0,
            spacing: 0.0,
            min: Size::ZERO,
            fixed: false,
            width: None,
            radius: metrics::RADIUS_ROW,
            press: PRESSED,
            caption: None,
        }
    }

    /// What it sends when pressed: a click, or Space or Enter while focused.
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    /// A disabled face ignores the pointer and the keys.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Names the face for `focusable::focus`.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// The face's width: `Length::Fill` stretches the shape across the room
    /// it is given, the icons and text staying centred in it and the text
    /// elided when the shape is narrower than it. Without one the face is as
    /// wide as its content, or as `fixed` says.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// The shape's height, instead of the look's.
    pub fn height(mut self, height: f32) -> Self {
        self.min.height = height;
        self
    }

    fn parts(&self) -> usize {
        usize::from(self.leading.is_some())
            + usize::from(!self.text.is_empty())
            + usize::from(self.trailing.is_some())
    }

    pub(super) fn content_width(&self, text_width: f32) -> f32 {
        let icons = self.leading.map_or(0.0, |_| self.glyph)
            + self.trailing.map_or(0.0, |_| self.trailing_glyph);
        icons + text_width + self.spacing * self.parts().saturating_sub(1) as f32
    }

    /// The visible shape inside the widget's bounds: all of them, or with a
    /// caption the shape at the top, centred.
    pub(super) fn shape(&self, bounds: Rectangle) -> Rectangle {
        if self.caption.is_some() {
            Rectangle::new(
                Point::new(bounds.center_x() - self.min.width / 2.0, bounds.y),
                self.min,
            )
        } else {
            bounds
        }
    }

    pub(super) fn live(&self) -> bool {
        self.enabled
    }

    /// The colours now: each flag that is easing blends its two looks.
    pub(super) fn colours(&self, theme: &Theme, state: &State) -> Colours {
        let at = |animation| State::amount(animation, state.now);
        between(at(&state.enabled), |enabled| {
            between(at(&state.turn), |open| {
                between(at(&state.lit), |hot| (self.look)(theme, hot, enabled, open))
            })
        })
    }

    /// What the face's width asks of its parent.
    pub(super) fn width_length(&self) -> Length {
        self.width
            .unwrap_or(if self.fixed && self.caption.is_none() {
                Length::Fixed(self.min.width)
            } else {
                Length::Shrink
            })
    }

    /// The widget's height: the shape and the caption under it.
    pub(super) fn total_height(&self) -> f32 {
        self.min.height + self.caption.map_or(0.0, |_| CAPTION_GAP + CAPTION_HEIGHT)
    }

    /// The text's width as laid out, elided if it didn't fit.
    pub(super) fn text_width(&self, state: &State) -> f32 {
        if self.text.is_empty() {
            0.0
        } else {
            state.text.min_width().ceil()
        }
    }

    /// What the shape keeps for icons and padding, which the text may not use.
    pub(super) fn chrome(&self) -> f32 {
        2.0 * self.padding + self.content_width(0.0)
    }
}
