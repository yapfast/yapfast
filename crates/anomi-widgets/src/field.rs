//! Text fields of the interface's own looks. `TextField`
//! (`ui/controls.slint:987`): a sunken well with a rim, a ring in the accent
//! while it has the keys, a password mode, a trailing picture and a draft
//! that survives a refusal ([`Draft`]); and the sidebar's search field.
//!
//! [`field`] builds a [`Field`], which becomes an element with
//! [`Field::view`]. It wraps iced's `text_input`, so editing, selection and
//! the clipboard are iced's; the look is the Slint one's.

use iced::widget::{TextInput, column, stack, text, text_input as input};
use iced::{Color, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};

use crate::icon::Icon;

mod draft;
mod parts;
#[cfg(test)]
mod tests;
mod well;

use parts::{Looks, trailing};
use well::{Ground, Well};

pub use draft::Draft;
pub use iced::widget::Id;

/// `text-size: 16px`.
const TEXT_SIZE: f32 = typography::size::TITLE;
/// `pad: 12px`, the text's inset from the well's edge.
const PAD: f32 = 12.0;
/// The room a trailing picture takes (`trailing: 28px`) and its size.
const TRAILING: f32 = 28.0;
const TRAILING_ICON: f32 = 20.0;
/// The line height, relative to the text size, that the input is set to and
/// the vertical padding is worked out from (iced's own default is 1.375).
const LINE: f32 = 1.3;
/// The gap between the well and its error line: the dialog column's
/// `spacing: 12px` (`ui/server.slint:166`).
const ERROR_GAP: f32 = 12.0;

/// A text field; see [`field`].
pub struct Field<'a, Message> {
    placeholder: &'a str,
    value: &'a str,
    error: Option<&'a str>,
    on_input: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_submit: Option<Message>,
    id: Option<Id>,
    password: bool,
    size: f32,
    pad: f32,
    fill: Option<Color>,
    ring: bool,
    rim: Option<bool>,
    ink: Option<Color>,
    placeholder_ink: Option<Color>,
    italic: bool,
    trailing: Option<(Icon, Option<Color>)>,
    height: f32,
    radius: f32,
    motion: Motion,
    on_focus: Option<Box<dyn Fn(bool) -> Message + 'a>>,
}

/// An empty well showing `placeholder`, or the `value`. Give it
/// [`on_input`](Field::on_input) or it is disabled.
pub fn field<'a, Message>(placeholder: &'a str, value: &'a str) -> Field<'a, Message> {
    Field {
        placeholder,
        value,
        error: None,
        on_input: None,
        on_submit: None,
        id: None,
        password: false,
        size: TEXT_SIZE,
        pad: PAD,
        fill: None,
        ring: true,
        rim: None,
        ink: None,
        placeholder_ink: None,
        italic: false,
        trailing: None,
        height: metrics::CONTROL_HEIGHT,
        radius: metrics::RADIUS_ROW,
        motion: Motion::default(),
        on_focus: None,
    }
}

impl<'a, Message: Clone + 'a> Field<'a, Message> {
    /// Shows a draft and the reason it was refused, if any, under the well.
    pub fn draft(mut self, draft: &'a Draft) -> Self {
        self.value = draft.text();
        self.error = draft.error();
        self
    }

    /// The reason the text was refused, in danger red under the well. For a
    /// screen drawing from a view-model, which holds the text and the error
    /// as plain values: `field(ph, &vm.code).error(vm.error.as_deref())`.
    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    /// Asks for the whole text after each edit. Without it the field is
    /// disabled: muted and not editable.
    pub fn on_input(mut self, on_input: impl Fn(String) -> Message + 'a) -> Self {
        self.on_input = Some(Box::new(on_input));
        self
    }

    /// Sent on Enter.
    pub fn on_submit(mut self, message: Message) -> Self {
        self.on_submit = Some(message);
        self
    }

    /// The id a [`focus`] finds it by.
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Dots instead of the text (`password`).
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The text's inset from the well's edge (`pad`).
    pub fn pad(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }

    /// A ground other than the sunken well (`fill`); transparent for a field
    /// on a ground of its own.
    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Whether the accent ring shows while it has the keys (`ring`).
    pub fn ring(mut self, ring: bool) -> Self {
        self.ring = ring;
        self
    }

    /// Whether the line rim shows at rest (`rim`; the ring's setting unless
    /// given).
    pub fn rim(mut self, rim: bool) -> Self {
        self.rim = Some(rim);
        self
    }

    /// The text's colour, for a field on another ground (`ink`).
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// The placeholder's colour, and whether it is in italics, as a
    /// profile's note asks (`placeholder-color`, `placeholder-italic`).
    pub fn placeholder_style(mut self, ink: Option<Color>, italic: bool) -> Self {
        self.placeholder_ink = ink;
        self.italic = italic;
        self
    }

    /// A picture at the right end the text stays clear of, tinted `tint` or
    /// the secondary text colour (`trailing-icon`). A transparent tint keeps
    /// the room and draws nothing, for a caller that lays its own over it.
    pub fn trailing(mut self, icon: Icon, tint: Option<Color>) -> Self {
        self.trailing = Some((icon, tint));
        self
    }

    /// The well's height (`min-height`), a regular button's 36 unless given.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// The well's corner radius (`border-radius`), a row's 6 unless given.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Whether things may move: the ring and the ground fade over
    /// `Theme.quick`, or land at once while still.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    /// Told `true` when the field gets the keys and `false` when it loses
    /// them (`focused`), for a ground or a save that follows focus.
    pub fn on_focus(mut self, on_focus: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_focus = Some(Box::new(on_focus));
        self
    }

    /// The vertical padding that makes the well `height` tall.
    fn vertical(&self) -> f32 {
        ((self.height - self.size * LINE) / 2.0).max(0.0)
    }

    fn room(&self) -> f32 {
        if self.trailing.is_some() {
            TRAILING
        } else {
            0.0
        }
    }

    /// The well as an element: the input, its placeholder when that is in
    /// italics, the trailing picture, and the error line under it.
    pub fn view(self) -> Element<'a, Message> {
        let vertical = self.vertical();
        let padding = Padding {
            top: vertical,
            bottom: vertical,
            left: self.pad,
            right: self.pad + self.room(),
        };
        let enabled = self.on_input.is_some();
        // The placeholder is drawn over an empty input: iced's own can
        // neither elide nor take a second face.
        let overlay = self.value.is_empty();
        let ground = Ground {
            fill: self.fill,
            ring: self.ring,
            rim: self.rim.unwrap_or(self.ring),
            radius: self.radius,
        };
        let looks = Looks { ink: self.ink };
        let mut well: TextInput<'a, Message, Theme> = input("", self.value)
            .size(self.size)
            .line_height(LINE)
            .padding(padding)
            .secure(self.password)
            .style(move |theme: &Theme, status| looks.style(theme, status));
        if let Some(on_input) = self.on_input {
            well = well.on_input(on_input);
        }
        if let Some(message) = self.on_submit {
            well = well.on_submit(message);
        }
        if let Some(id) = self.id {
            well = well.id(id);
        }

        let well = Well::new(well.boxed(), ground, self.motion, self.on_focus);
        let mut layers: Vec<Element<'a, Message>> = vec![well.boxed()];
        if overlay {
            layers.push(parts::placeholder(
                self.placeholder,
                self.italic,
                self.size,
                padding,
                enabled,
                self.placeholder_ink,
            ));
        }
        if let Some((glyph, tint)) = self.trailing {
            layers.push(trailing(glyph, tint, self.pad));
        }
        // Always the same shape, the well as layer 0 of the stack in a
        // column's first child, so the input keeps its state as the value
        // and the error come and go; only later children change.
        let first: Element<'a, Message> = stack(layers).boxed();
        let mut rows = column![first].spacing(ERROR_GAP);
        if let Some(error) = self.error {
            let line = text(error)
                .size(typography::size::SMALL)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().danger),
                    selection: None,
                });
            rows = rows.push(line.boxed());
        }
        rows.boxed()
    }
}

/// Gives the keys to the field with this id.
pub fn focus<T: Send + 'static>(id: impl Into<Id>) -> iced::Task<T> {
    iced::widget::operation::focus(id.into())
}

/// The search field at the top of the sidebar, as "Find a conversation".
pub fn search<'a, Message: Clone + 'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
) -> TextInput<'a, Message, Theme> {
    input(placeholder, value)
        .on_input(on_input)
        .size(typography::size::SMALL)
        .padding([6.0, 8.0])
        .style(catalog::text_input::search)
}
