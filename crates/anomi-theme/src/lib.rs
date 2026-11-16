//! The design system of Anomi's iced interface: what `ui/theme.slint` and
//! `src/ui/theme.rs` are to the Slint one. Everything that draws takes its
//! colours, sizes, type and timing from here.
//!
//! - [`Palette`]: the 37 colour tokens a theme sets, and those derived from
//!   them; [`builtin`] holds the twelve themes Anomi comes with.
//! - [`Theme`]: the iced theme type of the whole interface. Every built-in
//!   widget's style catalog is implemented for it in [`catalog`], so a style
//!   closure reads `theme.palette()`.
//! - [`metrics`] and [`typography`]: sizes and type, as `ClientStyle` has them.
//! - [`motion`]: durations and easings, and the switch that stops them all.
//! - [`View`] and [`Element`]: the widget types every view returns.
//!
//! See docs/iced/README.md for how screens and widgets use it.

pub mod builtin;
pub mod catalog;
pub mod fonts;
pub mod metrics;
pub mod motion;
mod palette;
mod theme;
pub mod typography;

pub use motion::Motion;
pub use palette::{Palette, rgb, rgba};
pub use theme::Theme;

/// The renderer every view draws with: tiny-skia by default.
pub type Renderer = iced::Renderer;

/// A boxed widget of the interface, for a value whose widget type varies,
/// such as two branches of a `match`: `widget.boxed()` makes one.
pub type Element<'a, Message> = iced::Element<'a, Message, Theme, Renderer>;

/// What every view module imports: `use anomi_theme::prelude::*;`. It brings
/// the widget types and iced's `Widget` extension trait, whose `.boxed()` and
/// `.map()` every view calls.
pub mod prelude {
    pub use super::{Element, Motion, Theme, View};
    pub use iced::Widget as _;
}

/// A widget of the interface. Views return `impl View<Message> + '_`, which
/// is `iced::Widget<Message, Theme, Renderer>` without naming the theme and
/// renderer each time.
pub trait View<Message>: iced::Widget<Message, Theme, Renderer> {}

impl<Message, W> View<Message> for W where W: iced::Widget<Message, Theme, Renderer> {}
