//! The message paragraph: a [`Text`] of `anomi_vm::text` drawn as Discord's
//! client draws a message's body, an embed's description or a notice.
//!
//! - Each block is a paragraph of its own at its level's size and colour:
//!   a heading larger in the heading colour, subtext and quoted text dimmer,
//!   fenced code in the system's monospace font in a box of the panel's
//!   colour, quoted blocks behind a bar rounded where the quote starts and
//!   ends, each with the space above it the text gives.
//! - Spans are bold, italic, underlined, struck through or inline code on
//!   its capsule; links in the link colour, underlined only under the
//!   pointer; mentions in their colour on a pill of it; hidden spoilers
//!   under an opaque cover; emoji as Twemoji's pictures through its font;
//!   a server's emoji as its picture, playing while it may, in the place a
//!   glyph of Twemoji's keeps for it (`picture::Player`).
//! - At most so many lines a block ([`Paragraph::max_lines`]), or folded to
//!   a few lines when long, with what unfolds it under it ([`Fold`]).
//! - What is under the pointer: a click on a link, a mention, an emoji or a
//!   spoiler sends [`Paragraph::on_link`], a right press
//!   [`Paragraph::on_right_press`] without taking the press from the menu
//!   around it. Text selects as iced's selectable text does, and copies with
//!   its emoji as typed.
//!
//! Ports `TextBlockView` and `TextBlocks` (`ui/messages.slint:2041`,
//! `:2092`), the `StyledText` of a message's body (`MessageBody`,
//! `ui/messages.slint:2948`), and what the `sharedparley` patches in
//! `vendor/i-slint-core` drew: mention pills, spoiler covers, the link under
//! the pointer, inline code's capsule, emoji past the line by their size.
//! Task F08.

mod capsule;
mod decorate;
mod draw;
mod select;
mod shape;
mod state;
#[cfg(test)]
mod tests;
mod widget;

use iced::{Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, typography};
use anomi_vm::text::{Link, Text};

/// A [`Text`] drawn as a message's body: see [`paragraph`].
pub struct Paragraph<'a, Message> {
    text: &'a Text,
    size: f32,
    color: Ink<'a>,
    heading: Option<Ink<'a>>,
    framed: bool,
    max_lines: Option<usize>,
    fold: Option<Fold>,
    toggle: Option<Element<'a, Message>>,
    motion: Motion,
    width: Length,
    selectable: bool,
    on_link: Option<Box<dyn Fn(Link) -> Message + 'a>>,
    on_right_press: Option<Box<dyn Fn(Link) -> Message + 'a>>,
}

/// A colour read from the theme as it draws, or one of the caller's own, as
/// an author's role colour: boxing a closure that captures nothing allocates
/// nothing.
type Ink<'a> = Box<dyn Fn(&Palette) -> Color + 'a>;

/// A long body folded to its first lines, as long messages fold with
/// "Collapse long messages" on (`MessageBody`, `ui/messages.slint:2956`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fold {
    /// The lines it shows while folded.
    pub lines: u16,
    /// It folds only once it's longer than these, so unfolding never
    /// uncovers only a line or two.
    pub beyond: u16,
    /// Shown whole, as Show more was clicked.
    pub open: bool,
}

impl Fold {
    /// Twelve lines once longer than fifteen, as Yapfast folds messages.
    pub const MESSAGE: Self = Self {
        lines: 12,
        beyond: 15,
        open: false,
    };
}

/// `text` at the message size in the text colour, its links opening
/// nothing until [`Paragraph::on_link`] says what they send.
pub fn paragraph<Message>(text: &Text) -> Paragraph<'_, Message> {
    Paragraph {
        text,
        size: typography::size::BODY,
        color: Box::new(|palette: &Palette| palette.text),
        heading: None,
        framed: false,
        max_lines: None,
        fold: None,
        toggle: None,
        motion: Motion::default(),
        width: Length::Fill,
        selectable: true,
        on_link: None,
        on_right_press: None,
    }
}

impl<'a, Message> Paragraph<'a, Message> {
    /// The size of its text, in logical pixels; a message of emoji alone
    /// draws 2.2 times it, as Discord's large emoji are 48 pixels where
    /// text's are 22.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The colour of its text, as `Theme.muted` for a message on its way or
    /// `Theme.secondary` for an embed's description.
    pub fn color(mut self, color: impl Fn(&Palette) -> Color + 'a) -> Self {
        self.color = Box::new(color);
        self
    }

    /// The colour of its headings, when not its text's: the strongest of
    /// its text, as in Discord's client.
    pub fn heading_color(mut self, color: impl Fn(&Palette) -> Color + 'a) -> Self {
        self.heading = Some(Box::new(color));
        self
    }

    /// Inside a card's container, where text and subtext are smaller, as
    /// in Discord's client; headings are not.
    pub fn framed(mut self, framed: bool) -> Self {
        self.framed = framed;
        self
    }

    /// Each block shows at most `lines` lines, as a found message's eight.
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = (lines > 0).then_some(lines);
        self
    }

    /// Folds it while long, `toggle` under it while it's long: what shows
    /// it whole or folds it again, 3 pixels below.
    pub fn fold(mut self, fold: Fold, toggle: impl View<Message> + 'a) -> Self {
        self.fold = Some(fold);
        self.toggle = Some(toggle.boxed());
        self
    }

    /// Lets its server's emoji play while `motion` plays.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Whether a drag over it selects its text; it does unless told.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// What a click on a link, a mention, an emoji or a covered spoiler sends.
    pub fn on_link(mut self, on_link: impl Fn(Link) -> Message + 'a) -> Self {
        self.on_link = Some(Box::new(on_link));
        self
    }

    /// What a right press on one sends, before the menu around it opens:
    /// the link its menu offers to copy or open.
    pub fn on_right_press(mut self, on_right_press: impl Fn(Link) -> Message + 'a) -> Self {
        self.on_right_press = Some(Box::new(on_right_press));
        self
    }

    /// The size its text draws at: 2.2 times for emoji alone.
    fn text_size(&self) -> f32 {
        if self.text.jumbo() {
            self.size * JUMBO
        } else {
            self.size
        }
    }
}

/// How much larger a message of emoji alone draws (`MessageBody`,
/// `ui/messages.slint:2973`).
const JUMBO: f32 = 2.2;

/// A line of text is 1.3 times its size, as DM Sans lays it out and as
/// `MessageBody` measures a fold (`ui/messages.slint:2959`).
const LINE: f32 = 1.3;

/// The space between a folded body and what unfolds it
/// (`ui/messages.slint:2994`).
const TOGGLE_GAP: f32 = 3.0;
