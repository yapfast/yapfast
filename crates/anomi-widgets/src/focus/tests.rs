//! The window's keys in the simulator, over a window of the shape the shell
//! gives it: a filter field, the timeline's keys, the composer and the
//! header's search under the layers, a dialog with a field and two buttons
//! over them on demand, and the scope around it all.

mod shortcuts;
mod tab;
mod typing;

use iced::advanced::widget::operation::{Focusable, focusable};
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::{Code, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::widget::{column, container, text_input};
use iced::{Event, Rectangle, Settings, Size};
use iced_test::Simulator;

use anomi_theme::{Element, Motion, Renderer, Theme};

use super::{ASIDE_COMPOSER, COMPOSER, SEARCH, TIMELINE, scope};
use crate::button::{Kind, action_button};
use crate::layer::{layer, layers};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Asked {
    Switcher,
    Back,
    Forward,
    LetGo,
    Escape,
    Close,
    Filter(String),
    Composer(String),
    Aside(String),
    Search(String),
    Name(String),
    Messages,
    Cancel,
    Save,
}

pub(super) type Ui<'a> = Simulator<'a, Asked, Theme, Renderer>;

/// What the window shows and which of its keys are on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Window {
    /// A dialog is open over it.
    pub dialog: bool,
    /// The thread beside the channel shows, and its composer was used last.
    pub aside: bool,
    /// Someone dragged waits to be let go.
    pub let_go: bool,
    /// The shortcuts, Escape and typing are given.
    pub keys: bool,
    /// A call hears the microphone while V is held.
    pub talk: bool,
}

/// The key that talks while held, when [`Window::talk`].
const TALK: [Physical; 1] = [Physical::Code(Code::KeyV)];

impl Default for Window {
    fn default() -> Self {
        Self {
            dialog: false,
            aside: false,
            let_go: false,
            keys: true,
            talk: false,
        }
    }
}

pub(super) fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

pub(super) fn window(shown: Window) -> Element<'static, Asked> {
    use iced::Widget as _;
    let mut base: Vec<Element<'static, Asked>> = vec![
        text_input("Filter", "")
            .id(Id::new("filter"))
            .on_input(Asked::Filter)
            .boxed(),
        action_button("Messages", Kind::Ghost, front())
            .id(TIMELINE)
            .on_press(Asked::Messages)
            .boxed(),
        text_input("Message", "Hello")
            .id(COMPOSER)
            .on_input(Asked::Composer)
            .boxed(),
        text_input("Search", "cats")
            .id(SEARCH)
            .on_input(Asked::Search)
            .boxed(),
    ];
    if shown.aside {
        base.push(
            text_input("Thread", "")
                .id(ASIDE_COMPOSER)
                .on_input(Asked::Aside)
                .boxed(),
        );
    }
    let dialog = shown.dialog.then(|| {
        let card = column![
            text_input("Name", "abc")
                .id(Id::new("name"))
                .on_input(Asked::Name),
            action_button("Cancel", Kind::Secondary, front())
                .id(Id::new("cancel"))
                .on_press(Asked::Cancel),
            action_button("Save", Kind::Primary, front())
                .id(Id::new("save"))
                .on_press(Asked::Save),
        ];
        layer("dialog", container(card).width(200).padding(20))
            .dialog()
            .on_close(Asked::Close)
    });
    let stack = layers(column(base), front()).push_maybe(dialog);
    let typing = if shown.aside {
        ASIDE_COMPOSER
    } else {
        COMPOSER
    };
    let keys = shown.keys;
    scope(stack)
        .on_switcher(keys.then_some(Asked::Switcher))
        .on_back(keys.then_some(Asked::Back))
        .on_forward(keys.then_some(Asked::Forward))
        .on_escape(keys.then_some(Asked::Escape))
        .on_let_go(shown.let_go.then_some(Asked::LetGo))
        .search(keys.then_some(SEARCH))
        .type_into(keys.then_some(typing))
        .browse(TIMELINE)
        .talk_keys(if shown.talk { &TALK } else { &[] })
        .boxed()
}

pub(super) fn simulate(view: Element<'_, Asked>) -> Ui<'_> {
    Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), view)
}

/// What was asked since the last look.
pub(super) fn asked(ui: &mut Ui<'_>) -> Vec<Asked> {
    ui.drain().collect()
}

/// Gives `id` the focus, as iced's `focus` does.
pub(super) fn focus(ui: &mut Ui<'_>, id: Id) {
    ui.operate(&mut focusable::focus::<()>(id));
}

/// The id of what has the focus, among what's in reach.
pub(super) fn focused(ui: &mut Ui<'_>) -> Option<Id> {
    struct Find(Option<Id>);
    impl Operation for Find {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
            if state.is_focused() {
                self.0 = id.cloned();
            }
        }
    }
    let mut find = Find(None);
    ui.operate(&mut find);
    find.0
}

/// A key pressed, in its place `code`, with `modifiers` held.
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

/// The same press, repeated as the key is held.
pub(super) fn held(event: Event) -> Event {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            modifiers,
            text,
            ..
        }) => Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            modifiers,
            text,
            repeat: true,
        }),
        other => other,
    }
}

/// A named key, with `modifiers` held.
pub(super) fn named(named: keyboard::key::Named, modifiers: Modifiers) -> Event {
    let text = match named {
        keyboard::key::Named::Tab => Some("\t"),
        keyboard::key::Named::Enter => Some("\r"),
        keyboard::key::Named::Backspace => Some("\u{8}"),
        keyboard::key::Named::Delete => Some("\u{7f}"),
        keyboard::key::Named::Space => Some(" "),
        _ => None,
    };
    press(Key::Named(named), Code::Fn, modifiers, text)
}
