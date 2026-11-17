//! The timeline's keys: while it has the keyboard, Up and Down step through
//! the messages, Page Up and Page Down five at a time, Home and End to the
//! first and the newest; Escape goes back to the composer; R replies, E
//! edits, Delete deletes, C copies, F forwards, `+` reacts and Enter opens
//! the menu of the message stepped to, Alt+Enter marks it unread. While its
//! deletion waits, Enter deletes and Escape keeps it. The Slint side:
//! `timeline-keys` (`ui/yapfast.slint:3654`-`:3700`), `step-message` and
//! `browsing` (`:745`-`:767`).
//!
//! The timeline takes the keyboard only when asked
//! ([`focus::TIMELINE`], from the composer's Alt+Up), never on a click, as
//! `focus-on-click: false`; a press elsewhere lets it go. A key it doesn't
//! use goes on: the window's scope sends letters and digits to the
//! composer. It tells the app when it takes the keyboard and lets it go
//! ([`Intent::Browsing`]), for the keyboard's message to show it; built
//! afresh, as after the skeleton stood in for the messages, it hasn't the
//! keyboard, and says so if the app thinks it has.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Id, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};
use anomi_vm::chat::{
    self,
    timeline::{Act, Intent, Timeline},
};
use anomi_vm::menu::{Align, GAP_BESIDE, Side, Spot};
use anomi_widgets::focus;

use super::row::ACTIVE;

/// The menu and the picker a key opens sit left of the message's right
/// edge, 16 px in, from 16 px over its top, as if from a 32 px button
/// (`root.anchor(...)`, `ui/yapfast.slint:3772`).
const POPUP_INSET: f32 = 16.0;
const POPUP_ANCHOR: f32 = 32.0;
/// Page Up and Page Down step this many messages.
const PAGE: usize = 5;

pub struct Keys<'a> {
    content: Element<'a, chat::Intent>,
    timeline: &'a Timeline,
}

/// `content`, the timeline's messages, with their keys.
pub fn keys<'a>(content: impl View<chat::Intent> + 'a, timeline: &'a Timeline) -> Keys<'a> {
    Keys {
        content: content.boxed(),
        timeline,
    }
}

struct State {
    focused: bool,
    /// Whether the app was last told it has the keyboard.
    told: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// What a key asks.
enum Answer {
    Ask(Intent),
    /// The message's menu or the picker, beside it.
    Beside(u64, fn(Spot) -> Act),
    /// Taken, for nothing: Alt+Enter on a message that can't be marked.
    Swallow,
}

impl Keys<'_> {
    /// What `key` asks of the timeline as it stands, if anything.
    fn answer(&self, key: &Key, modified: &Key, modifiers: Modifiers) -> Option<Answer> {
        let timeline = self.timeline;
        let rows = &timeline.rows;
        let last = rows.len().checked_sub(1)?;
        let keys = &timeline.keys;
        let active = keys.active.and_then(|key| timeline.index_of(key));
        let named = match key.as_ref() {
            Key::Named(named) => Some(named),
            _ => None,
        };
        if keys.confirming && active.is_some() {
            match named {
                Some(Named::Enter) => return act(keys.active, Act::ConfirmDelete),
                Some(Named::Escape) => return act(keys.active, Act::CancelDelete),
                _ => {}
            }
        }
        let step = |to: usize| Some(Answer::Ask(Intent::Step(rows[to.min(last)].key)));
        match named {
            Some(Named::ArrowUp) => return step(active.map_or(last, |at| at.saturating_sub(1))),
            Some(Named::ArrowDown) => return step(active.map_or(last, |at| at + 1)),
            Some(Named::PageUp) => return step(active.map_or(0, |at| at.saturating_sub(PAGE))),
            Some(Named::PageDown) => return step(active.map_or(PAGE - 1, |at| at + PAGE)),
            Some(Named::Home) => return step(0),
            Some(Named::End) => return step(last),
            Some(Named::Escape) => return Some(Answer::Ask(Intent::Leave)),
            _ => {}
        }
        let message = keys.active.filter(|_| active.is_some())?;
        let may = keys.may;
        // A message deleted or still being sent takes none of them.
        if !may.menu {
            return None;
        }
        match named {
            Some(Named::Enter) if modifiers.alt() => {
                return if may.mark_unread {
                    act(Some(message), Act::MarkUnread)
                } else {
                    Some(Answer::Swallow)
                };
            }
            Some(Named::Enter | Named::ContextMenu) => {
                return Some(Answer::Beside(message, Act::Menu));
            }
            Some(Named::F10) if modifiers.shift() => {
                return Some(Answer::Beside(message, Act::Menu));
            }
            Some(Named::Delete | Named::Backspace) if may.delete => {
                return act(Some(message), Act::Delete);
            }
            _ => {}
        }
        let plain = !modifiers.control() && !modifiers.alt() && !modifiers.logo();
        let letter = |wanted: &str| plain && is(key, wanted);
        if plain && (is(key, "=") || is(modified, "+")) && may.react {
            Some(Answer::Beside(message, Act::React))
        } else if letter("r") && may.reply {
            act(Some(message), Act::Reply)
        } else if letter("e") && may.edit {
            act(Some(message), Act::Edit)
        } else if letter("c") && may.copy {
            act(Some(message), Act::Copy)
        } else if letter("f") && may.forward {
            act(Some(message), Act::Forward)
        } else {
            None
        }
    }
}

fn act(message: Option<u64>, act: Act) -> Option<Answer> {
    message.map(|message| Answer::Ask(Intent::Act(message, act)))
}

/// Whether `key` types `text`, whatever its case.
fn is(key: &Key, text: &str) -> bool {
    matches!(key.as_ref(), Key::Character(typed) if typed.eq_ignore_ascii_case(text))
}

/// Tells the app the timeline took the keyboard or let it go, once.
fn tell(state: &mut State, shell: &mut Shell<'_, chat::Intent>) {
    if state.focused != state.told {
        state.told = state.focused;
        shell.publish(Intent::Browsing(state.focused).into());
    }
}

/// Finds where the keyboard's message is.
struct Find {
    bounds: Option<Rectangle>,
}

impl Operation for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        if self.bounds.is_none() {
            operate(self);
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, _viewport: &Rectangle) {
        if id == Some(&ACTIVE) {
            self.bounds = Some(bounds);
        }
    }
}

/// Where a menu opens beside the message at `bounds`.
fn beside(bounds: Rectangle) -> Spot {
    Spot {
        anchor: Rectangle::new(
            Point::new(
                bounds.x + bounds.width - POPUP_INSET,
                bounds.y - POPUP_INSET,
            ),
            Size::new(0.0, POPUP_ANCHOR),
        ),
        side: Side::Left,
        align: Align::Start,
        gap: GAP_BESIDE,
    }
}

impl widget::Meta for Keys<'_> {}

impl widget::Widget<chat::Intent, Theme, Renderer> for Keys<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        // A fresh state hasn't the keyboard. The app may think it has, as
        // when the messages gave way to the skeleton while it did: told so
        // here, it hears the keyboard went on the first event.
        tree::State::new(State {
            focused: false,
            told: self.timeline.keys.browsing,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(Some(&focus::TIMELINE), layout.bounds(), state);
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let content = &mut self.content;
        operation.traverse(&mut |operation| {
            content.operate(child_tree, child, viewport, renderer, operation);
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, chat::Intent>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        // The keyboard came or went by an operation since the last event.
        tell(tree.state.downcast_mut::<State>(), shell);
        let focused = tree.state.downcast_ref::<State>().focused;
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            modifiers,
            ..
        }) = event
            && focused
            && !shell.is_event_captured()
            && let Some(answer) = self.answer(key, modified_key, *modifiers)
        {
            match answer {
                Answer::Ask(intent) => shell.publish(intent.into()),
                Answer::Beside(message, open) => {
                    let mut find = Find { bounds: None };
                    let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
                    self.content
                        .operate(child_tree, child, viewport, renderer, &mut find);
                    // The message kept in view is built; else beside the
                    // timeline's top.
                    let at = find.bounds.unwrap_or(Rectangle::new(
                        Point::new(bounds.x, bounds.y + POPUP_INSET),
                        Size::new(bounds.width, POPUP_ANCHOR),
                    ));
                    shell.publish(Intent::Act(message, open(beside(at))).into());
                }
                Answer::Swallow => {}
            }
            shell.capture_event();
        } else {
            let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.content
                .update(child_tree, event, child, cursor, renderer, shell, viewport);
        }
        let state = tree.state.downcast_mut::<State>();
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && !cursor.is_over(bounds)
        {
            state.focused = false;
        }
        tell(state, shell);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child_tree, renderer, theme, style, child, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child_tree, child, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, chat::Intent, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}
