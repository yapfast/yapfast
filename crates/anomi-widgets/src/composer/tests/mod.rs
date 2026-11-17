//! The composer field driven two ways: through iced's simulator for what a
//! person does (typing, keys, the menu), and through its `update` with a
//! headless shell (the [`Rig`]) for what it asks of the window and what it
//! keeps (its height, the caret's line, the blink).

mod anywhere;
mod gallery;
mod gallery_menu;
mod keys;
mod layout;
mod menu;
mod sync;
mod typing;

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::{self, Settings as Look};
use iced::advanced::shell::{Bus, Waker};
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Tree, Widget};
use iced::advanced::{Shell, mouse};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::window::{self, RedrawRequest};
use iced::{Event, Point, Rectangle, Settings, Size};
use iced_test::Simulator;

use anomi_theme::{Motion, Renderer, Theme};

use super::state::State;
use super::*;

/// What the field asked, each with the room it was asked for.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Asked {
    Edit(u64, String, usize),
    Caret(u64, usize),
    Send,
    Focus(bool),
    Paste,
    Expressions(Expressions),
    Up,
    Browse,
    Suggest(Suggestion),
    Command(CommandKey),
    /// The window's Escape, past the field.
    Escape,
}

pub(super) const FIELD: &str = "composer";
pub(super) const WIDTH: f32 = 600.0;

pub(super) fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

/// The field for room `key` showing `text`, with every message given.
pub(super) fn field(text: &str, key: u64) -> Composer<'_, Asked> {
    composer(text, front())
        .key(key)
        .id(Id::new(FIELD))
        .placeholder("Message #general")
        .on_edit(move |text, caret| Asked::Edit(key, text, caret))
        .on_caret(move |caret| Asked::Caret(key, caret))
        .on_send(Asked::Send)
        .on_focus(Asked::Focus)
        .on_paste(Asked::Paste)
        .on_expressions(Asked::Expressions)
        .on_up(Asked::Up)
        .on_browse(Asked::Browse)
        .on_suggestion(Asked::Suggest)
        .on_command(Asked::Command)
}

pub(super) type Ui<'a> = Simulator<'a, Asked, Theme, Renderer>;

pub(super) fn simulate<'a>(field: Composer<'a, Asked>) -> Ui<'a> {
    Simulator::with_size(Settings::default(), Size::new(WIDTH, 400.0), field)
}

/// Gives the field the keys, as the window's focus does.
pub(super) fn focus(ui: &mut Ui<'_>) {
    ui.operate(&mut focusable::focus::<()>(Id::new(FIELD)));
}

/// What was asked since the last look.
pub(super) fn asked(ui: &mut Ui<'_>) -> Vec<Asked> {
    ui.drain().collect()
}

/// Only the edits asked since the last look.
pub(super) fn edits(ui: &mut Ui<'_>) -> Vec<Asked> {
    asked(ui)
        .into_iter()
        .filter(|asked| matches!(asked, Asked::Edit(..)))
        .collect()
}

/// A key pressed with `modifiers` held.
pub(super) fn press(key: Key, code: Code, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(code),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

/// A named key with `modifiers` held, with the text it would type.
pub(super) fn named(named: Named, modifiers: Modifiers) -> Event {
    let text = match named {
        Named::Tab => Some("\t"),
        Named::Enter => Some("\r"),
        Named::Backspace => Some("\u{8}"),
        Named::Delete => Some("\u{7f}"),
        _ => None,
    };
    press(Key::Named(named), Code::Fn, modifiers, text)
}

/// Ctrl and a letter, in its place on the keyboard.
pub(super) fn ctrl(letter: &str, code: Code) -> Event {
    press(Key::Character(letter.into()), code, Modifiers::CTRL, None)
}

/// The field driven through its own `update`, its state at hand.
pub(super) struct Rig {
    pub tree: Tree,
    renderer: Renderer,
    pub cursor: mouse::Cursor,
}

/// What the field asked of the window for one event.
#[derive(Debug)]
pub(super) struct Reply {
    pub asked: Vec<Asked>,
    pub redraw: RedrawRequest,
    pub captured: bool,
    pub reads: usize,
}

impl Rig {
    pub fn new(field: &mut Composer<'_, Asked>) -> Self {
        let mut tree = Tree::new(&*field);
        tree.diff(field);
        let mut rig = Self {
            tree,
            renderer: Renderer::new(Look::default()),
            cursor: mouse::Cursor::Unavailable,
        };
        rig.lay_out(field);
        rig
    }

    /// Takes the next view, as the window rebuilds after a message.
    pub fn rebuild(&mut self, field: &mut Composer<'_, Asked>) {
        self.tree.diff(field);
        self.lay_out(field);
    }

    pub fn lay_out(&mut self, field: &mut Composer<'_, Asked>) {
        let limits = Limits::new(Size::ZERO, Size::new(WIDTH, f32::INFINITY));
        field.layout(&mut self.tree, &self.renderer, &limits);
    }

    pub fn size(&self) -> Size {
        self.tree.size
    }

    pub fn state(&self) -> &State {
        self.tree.state.downcast_ref::<State>()
    }

    pub fn state_mut(&mut self) -> &mut State {
        self.tree.state.downcast_mut::<State>()
    }

    pub fn focus(&mut self) {
        iced::advanced::widget::operation::Focusable::focus(&mut self.state_mut().keys);
    }

    pub fn send(&mut self, field: &mut Composer<'_, Asked>, event: Event) -> Reply {
        let mut bus = Bus::<Asked>::new();
        let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(WIDTH, 1000.0));
        let layout = Layout::new(self.tree.size);
        field.update(
            &mut self.tree,
            &event,
            layout,
            self.cursor,
            &self.renderer,
            &mut shell,
            &viewport,
        );
        let redraw = shell.redraw_request();
        let captured = shell.is_event_captured();
        let reads = shell.clipboard_mut().reads.len();
        drop(shell);
        Reply {
            asked: bus.drain().map(|(asked, _)| asked).collect(),
            redraw,
            captured,
            reads,
        }
    }

    pub fn type_text(&mut self, field: &mut Composer<'_, Asked>, text: &str) -> Vec<Asked> {
        text.chars()
            .flat_map(|letter| {
                let letter = letter.to_string();
                let key = Key::Character(letter.as_str().into());
                self.send(
                    field,
                    press(key, Code::Fn, Modifiers::empty(), Some(&letter)),
                )
                .asked
            })
            .collect()
    }

    pub fn frame(&mut self, field: &mut Composer<'_, Asked>, now: std::time::Instant) -> Reply {
        self.send(field, Event::Window(window::Event::RedrawRequested(now)))
    }

    pub fn draw(&mut self, field: &Composer<'_, Asked>, theme: &Theme) {
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(WIDTH, 1000.0));
        field.draw(
            &self.tree,
            &mut self.renderer,
            theme,
            &renderer::Style::default(),
            Layout::new(self.tree.size),
            self.cursor,
            &viewport,
        );
    }
}
