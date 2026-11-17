//! The selection driven headless through iced's simulator, over a chat of
//! rows in the virtual list as the timeline draws them: a styled name
//! (whole), a time (a text that selects), the message paragraph, and
//! pictures. The
//! Slint app's tests ported: `a_selection_runs_across_messages_with_their_
//! names_and_times`, `a_selection_copies_the_pictures_it_runs_through`,
//! `a_selection_keeps_the_messages_it_scrolls_past` and
//! `styled_messages_select_by_dragging_and_copy_as_plain_text`
//! (`src/ui/tests.rs`).

mod clicks;
mod copying;
mod guard;
mod shown;

use std::borrow::Cow;
use std::time::{Duration, Instant};

use iced::advanced::widget::Id;
use iced::keyboard::key::{Code, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::widget::{Column, container, row, text};
use iced::{Event, Fill, Point, Rectangle, Settings, Size, mouse, window};
use iced_test::Simulator;

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, fonts, typography};
use anomi_vm::text::{Link, Source, Text, markdown};

use super::{picture, selection, whole};
use crate::list::list;
use crate::name::styled_name;
use crate::paragraph::paragraph;

/// The window the tests draw in.
pub const SIZE: Size = Size::new(600.0, 400.0);
pub const LIST: &str = "timeline";
pub const SELECTION: &str = "selection";

#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    Copied(String),
    Selected(String),
    Menu(Option<String>),
    Open(Link),
}

pub struct Message {
    pub key: u64,
    pub author: String,
    pub time: String,
    pub body: Text,
    pub pictures: usize,
}

/// Messages, each starting a group: its author and time over its text.
pub struct Chat {
    pub messages: Vec<Message>,
}

impl Chat {
    /// Messages by `author` saying `body` (Markdown), keyed and timed by
    /// their place from `first`.
    pub fn new(first: u64, lines: &[(&str, &str)]) -> Self {
        let messages = lines
            .iter()
            .zip(first..)
            .map(|((author, body), key)| Message {
                key,
                author: (*author).into(),
                time: format!("19:{:02}", key % 60),
                body: markdown(Source::new(body)),
                pictures: 0,
            })
            .collect();
        Self { messages }
    }

    /// `count` messages by Sam, "Message N." each.
    pub fn many(count: u64) -> Self {
        let bodies: Vec<String> = (0..count).map(|at| format!("Message {at}.")).collect();
        let lines: Vec<(&str, &str)> = bodies.iter().map(|body| ("Sam", body.as_str())).collect();
        Self::new(0, &lines)
    }

    /// The window: the list from its end, in a selection that copies each
    /// row whole as `copied` reads it.
    pub fn view(&self, motion: Motion) -> Element<'_, Asked> {
        let count = self.messages.len();
        let key = |at: usize| self.messages[at].key;
        selection(
            list(count, key, move |at| self.row(at), motion)
                .id(Id::new(LIST))
                .from_end(),
        )
        .id(Id::new(SELECTION))
        .rows(count, key, |at| Some(self.copied(at)))
        .on_copy(Asked::Copied)
        .on_select(Asked::Selected)
        .on_menu(Asked::Menu)
        .motion(motion)
        .boxed()
    }

    fn row(&self, at: usize) -> Element<'_, Asked> {
        let message = &self.messages[at];
        let header = row![
            whole(styled_name(&message.author, None).size(15.0)),
            text(message.time.as_str()).size(12).selectable(true),
        ]
        .spacing(8);
        let mut items: Vec<Element<'_, Asked>> = vec![
            header.boxed(),
            paragraph(&message.body).on_link(Asked::Open).boxed(),
        ];
        items.extend(
            (0..message.pictures)
                .map(|_| picture(container(text("")).width(40).height(30)).boxed()),
        );
        let items = Column::with_children(items).spacing(2);
        super::row(at, message.key, container(items).padding(8).width(Fill)).boxed()
    }

    /// A message as a selection covering it copies it (`message_text`,
    /// `src/ui.rs:924`).
    pub fn copied(&self, at: usize) -> String {
        let message = &self.messages[at];
        let mut lines = vec![
            format!("{} {}", message.author, message.time),
            message.body.read(),
        ];
        lines.extend((0..message.pictures).map(|_| "[Picture]".to_owned()));
        lines.join("\n")
    }
}

pub fn settings() -> Settings {
    fonts::install();
    Settings {
        fonts: fonts::ALL.iter().map(|font| Cow::Borrowed(*font)).collect(),
        font: typography::UI,
        ..Settings::default()
    }
}

pub fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

pub type Ui<'a> = Simulator<'a, Asked, Theme, Renderer>;

pub fn simulate(view: Element<'_, Asked>) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view)
}

/// Where the text reading `words` is.
pub fn bounds(ui: &mut Ui<'_>, words: &str) -> Rectangle {
    ui.find(words)
        .unwrap_or_else(|_| panic!("{words:?} shows"))
        .bounds()
}

/// The pointer moves to `point`, as the window tells it.
pub fn move_to(ui: &mut Ui<'_>, point: Point) {
    ui.point_at(point);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: point })]);
}

pub fn press(ui: &mut Ui<'_>, button: mouse::Button) {
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(button))]);
}

pub fn release(ui: &mut Ui<'_>, button: mouse::Button) {
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(button))]);
}

/// Presses at `from`, drags through `through` and lets go there.
pub fn drag(ui: &mut Ui<'_>, from: Point, through: &[Point]) {
    move_to(ui, from);
    press(ui, mouse::Button::Left);
    for point in through {
        move_to(ui, *point);
    }
    release(ui, mouse::Button::Left);
}

/// Clicks at `at`.
pub fn click(ui: &mut Ui<'_>, at: Point) {
    move_to(ui, at);
    press(ui, mouse::Button::Left);
    release(ui, mouse::Button::Left);
}

/// A key pressed with Ctrl held.
pub fn ctrl(letter: &str, code: Code) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(letter.into()),
        modified_key: Key::Character(letter.into()),
        physical_key: Physical::Code(code),
        location: Location::Standard,
        modifiers: Modifiers::CTRL,
        repeat: false,
        text: None,
    })
}

/// Ctrl+C, and what it copied.
pub fn copy(ui: &mut Ui<'_>) -> Option<String> {
    ui.simulate([ctrl("c", Code::KeyC)]);
    ui.drain().find_map(|asked| match asked {
        Asked::Copied(copied) => Some(copied),
        _ => None,
    })
}

/// Frames `step` apart, `count` of them, from `start`.
pub fn frames(ui: &mut Ui<'_>, start: Instant, step: Duration, count: u32) {
    for at in 0..count {
        ui.simulate([Event::Window(window::Event::RedrawRequested(
            start + step * at,
        ))]);
    }
}

/// Past the end of a text at `bounds`, on its last line, clear of the
/// list's scrollbar.
pub fn end_of(bounds: Rectangle) -> Point {
    Point::new(
        bounds.x + bounds.width - 40.0,
        bounds.y + bounds.height - 6.0,
    )
}
