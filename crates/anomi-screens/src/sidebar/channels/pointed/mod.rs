//! A row of the channel list that knows the pointer: the ground, behaviour
//! and keys of `ChannelRow` (`ui/rail.slint:397`), shared with
//! `CategoryHeading` (`:840`) and the people under a voice channel
//! (`:1487`, `PersonTouch` at `:799`). iced's button gives its style the
//! pointer but not its content, and these rows change their content with
//! it, so this widget holds the pointer and hands what it decides down:
//!
//! - **Ground.** The active fill while chosen, the hover fill under the
//!   pointer, easing over `QUICK`; an accent ring of 2 px while it has the
//!   keyboard's focus; the live colour round it while someone dragged would
//!   drop on it (`move-aimed`, `:1475`); the unread mark at the sidebar's
//!   edge, left of the row (`:535`).
//! - **Ink.** The body is drawn in the row's ink, which text that names no
//!   colour of its own takes (`catalog::text::inherit`): muted at rest,
//!   secondary under the pointer, the text colour chosen or unread, faint
//!   when locked (`:517`).
//! - **Strength.** A muted channel, or this account on another client, is
//!   half as strong until pointed at or chosen; a row someone dragged can't
//!   drop among, 0.3 (`:596`, `:1452`, `:1497`). iced draws no layer at an
//!   opacity, so the body's theme fades instead, and pictures in it stay
//!   whole.
//! - **What shows under the pointer.** A trailing part, laid out at the
//!   right while pointed at or chosen (a voice channel's chat button,
//!   `:703`; a heading's "+", fading in, `:922`); a card beside the row
//!   (who is in a voice channel, `VoiceCardView`, `:741`), until a click, a
//!   right click, a drag or a scroll hides it until the pointer leaves
//!   (`card-dismissed`, `:456`).
//! - **A heading's chevron**, drawn after its name, turning a quarter while
//!   its category is folded (`:871`).
//! - **Asking.** A click (on letting go: a drag never clicks) and Space or
//!   Enter while focused press it; a right click, the Menu key or
//!   Shift+F10 open its menu, at the pointer or under the row's start, in
//!   the window's coordinates. A press a part of it took (a button in it)
//!   is that part's.
//!
//! Nothing asks for a frame but an animation under way, and none while the
//! interface holds still.

mod card;
mod paint;
mod state;
mod widget;

#[cfg(test)]
mod tests;

use iced::{Point, Rectangle};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics};
use anomi_vm::icon::Icon;

/// Space at the row's ends (`padding-left`, `padding-right: 8px`), and a
/// leading icon's size and the space after it (`:595`, `:612`).
const PAD: f32 = 8.0;
const LEADING: f32 = metrics::ICON_ROW;
const LEADING_GAP: f32 = metrics::SPACE_S;
/// The heading's chevron: 12 px, 4 px after its name (`:871`, `:866`).
const CHEVRON: f32 = 12.0;
const CHEVRON_GAP: f32 = 4.0;
/// A fading trailing icon: 16 px, in the middle of its part (`:927`).
const FADING: f32 = 16.0;
/// The unread mark: 4 by 8, round on its right (`:537`).
const MARK: (f32, f32) = (4.0, 8.0);
/// The focus ring's and the drop outline's width (`:511`, `:1476`).
const RING: f32 = 2.0;

/// What fills the row's ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    /// The active fill while chosen, the hover fill under the pointer.
    #[default]
    Row,
    /// The hover fill under the pointer only: someone in a voice channel.
    Hover,
    /// None: a category's heading.
    None,
}

/// The colour text in the body takes, unless it names its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ink {
    /// A channel's name: `ChannelRow`'s `ink`.
    #[default]
    Channel,
    /// A heading's: muted, secondary under the pointer.
    Heading,
    /// What the row is drawn in already.
    Inherit,
}

/// How a row looks in its states.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub fill: Fill,
    pub ink: Ink,
    pub selected: bool,
    pub unread: bool,
    pub locked: bool,
    /// The ground's corners.
    pub radius: f32,
    /// The focus ring's corners; none for a row that takes no focus.
    pub ring: Option<f32>,
    /// How strong the body is at rest, until pointed at or chosen.
    pub rest: f32,
    /// How strong the whole row is: 0.3 while a drag can't drop among it.
    pub dim: f32,
    /// The unread mark, this far left of the row; none for a row without.
    pub mark: Option<f32>,
    /// Outlined: someone dragged would drop here.
    pub aimed: bool,
    /// An icon over the trailing part, which is then only its click
    /// target: it fades in with the pointer rather than showing at once,
    /// in the muted colour, the text colour while the pointer is on the
    /// part (a heading's "+", `ui/rail.slint:924`-`:928`). The row draws
    /// it, as the leading icon: drawn in the part, it wouldn't fade.
    pub fading: Option<Icon>,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            fill: Fill::Row,
            ink: Ink::Channel,
            selected: false,
            unread: false,
            locked: false,
            radius: metrics::RADIUS_ROW,
            ring: Some(metrics::RADIUS_ROW),
            rest: 1.0,
            dim: 1.0,
            mark: None,
            aimed: false,
            fading: None,
        }
    }
}

impl Look {
    /// The ground's colour, `lit` of the way under the pointer.
    pub fn fill(&self, palette: &Palette, lit: f32) -> iced::Color {
        let fill = match self.fill {
            Fill::Row if self.selected => palette.active,
            Fill::Row | Fill::Hover => palette.hover.scale_alpha(lit),
            Fill::None => iced::Color::TRANSPARENT,
        };
        fill.scale_alpha(self.dim)
    }

    /// The body's ink, before its strength: `inherited` for [`Ink::Inherit`].
    pub fn ink(&self, palette: &Palette, lit: f32, inherited: iced::Color) -> iced::Color {
        match self.ink {
            Ink::Inherit => inherited,
            Ink::Channel if self.locked => {
                if self.selected {
                    palette.muted
                } else {
                    palette.faint
                }
            }
            Ink::Channel if self.selected || self.unread => palette.text,
            Ink::Channel | Ink::Heading => blend(palette.muted, palette.secondary, lit),
        }
    }

    /// How strong the body is, `lit` of the way under the pointer.
    pub fn strength(&self, lit: f32) -> f32 {
        let rest = if self.selected { 1.0 } else { self.rest };
        self.dim * (rest + (1.0 - rest) * lit)
    }
}

/// `from` blended `amount` of the way to `to`.
pub fn blend(from: iced::Color, to: iced::Color, amount: f32) -> iced::Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    iced::Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

type Press<'a, M> = Box<dyn Fn(Rectangle) -> M + 'a>;
type Menu<'a, M> = Box<dyn Fn(Point) -> M + 'a>;
/// A colour read from the theme at draw time.
pub type Tint = fn(&Palette, &Look) -> iced::Color;

/// A row `height` tall, its body laid out in it; see the module.
pub struct Pointed<'a, M> {
    body: Element<'a, M>,
    leading: Option<(Icon, Tint)>,
    trailing: Option<Element<'a, M>>,
    card: Option<Element<'a, M>>,
    look: Look,
    height: f32,
    top: f32,
    chevron: Option<bool>,
    on_press: Option<Press<'a, M>>,
    on_menu: Option<Menu<'a, M>>,
    motion: Motion,
}

pub fn pointed<'a, M: 'a>(
    body: impl View<M> + 'a,
    height: f32,
    look: Look,
    motion: Motion,
) -> Pointed<'a, M> {
    Pointed {
        body: body.boxed(),
        leading: None,
        trailing: None,
        card: None,
        look,
        height,
        top: 0.0,
        chevron: None,
        on_press: None,
        on_menu: None,
        motion,
    }
}

impl<'a, M: 'a> Pointed<'a, M> {
    /// An icon at the row's start, 8 px in, 20 px, before the body; drawn
    /// by the row in `tint`, as strong as the body. The renderer drops an
    /// svg's colour alpha (`tiny_skia/src/vector.rs`), so an icon in the
    /// body wouldn't fade with it.
    pub fn leading(mut self, icon: Icon, tint: Tint) -> Self {
        self.leading = Some((icon, tint));
        self
    }

    /// Shown at the right while pointed at, or chosen; with
    /// [`Look::fading`], laid out always and drawn as that icon.
    pub fn trailing(mut self, trailing: impl View<M> + 'a) -> Self {
        self.trailing = Some(trailing.boxed());
        self
    }

    /// A card beside the row while the pointer is on it.
    pub fn card(mut self, card: impl View<M> + 'a) -> Self {
        self.card = Some(card.boxed());
        self
    }

    /// Room above the body: a heading keeps 17 px over its name.
    pub fn top(mut self, top: f32) -> Self {
        self.top = top;
        self
    }

    /// A chevron after the body, turned while `folded`. The body is laid
    /// out as wide as it is, then.
    pub fn chevron(mut self, folded: bool) -> Self {
        self.chevron = Some(folded);
        self
    }

    /// What a click, Space or Enter sends.
    pub fn on_press(self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_press_at(move |_| message.clone())
    }

    /// What a click sends, given the row's bounds in the window.
    pub fn on_press_at(mut self, message: impl Fn(Rectangle) -> M + 'a) -> Self {
        self.on_press = Some(Box::new(message));
        self
    }

    /// What a right click, the Menu key or Shift+F10 send, given where in
    /// the window the menu opens.
    pub fn on_menu(mut self, message: impl Fn(Point) -> M + 'a) -> Self {
        self.on_menu = Some(Box::new(message));
        self
    }
}
