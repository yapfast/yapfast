//! What the selection does with an event, before or after its content has
//! it, as `yapfast_selection.rs` and its hooks did: a left press anywhere
//! clears it and one in a row's text starts another (`pressed`, `press`;
//! `items/text.rs:480`); the pointer moving, the wheel or a frame carries
//! it on (`drag_to`, `items/flickable.rs:249`), scrolling the list at its
//! edges (both in `drag`); letting go ends it (`release`); Ctrl+C copies it
//! and Escape clears it (`copy`, `clear`; `window.rs:1476`); a right press
//! tells the menu opening there what it would copy (`copied_at`).

use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, Shell, mouse};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Modifiers};
use iced::{Event, Point, Rectangle, window};

use anomi_theme::Renderer;

use super::Selection;
use super::model::{Held, State};
use super::pass::Showing;

#[allow(clippy::too_many_arguments)]
pub fn update<Message>(
    selection: &mut Selection<'_, Message>,
    tree: &mut Tree,
    event: &Event,
    layout: Layout,
    cursor: mouse::Cursor,
    renderer: &Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
) {
    let bounds = layout.bounds();
    let Tree {
        state, children, ..
    } = tree;
    let state = state.downcast_mut::<State>();
    let (layout, content) = layout.iter_mut(children).next().expect("one");
    let pass = |selection: &mut Selection<'_, Message>,
                shell: &mut Shell<'_, Message>,
                content: &mut Tree,
                cursor: mouse::Cursor,
                event: &Event| {
        selection
            .content
            .update(content, event, layout, cursor, renderer, shell, viewport);
    };
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            // A press anywhere ends any before it, which may have lost the
            // pointer without letting go, and clears the selection.
            state.drag = None;
            if state.held.is_some() {
                state.drop_held();
                selection.wipe(content, layout, renderer);
                shell.request_redraw();
            }
            let shows = selection
                .content
                .mouse_interaction(content, layout, cursor, viewport, renderer);
            let started = match cursor.position_over(bounds) {
                Some(point) if !shell.is_event_captured() && !takes_press(shows) => {
                    selection.press(state, content, layout, renderer, point)
                }
                _ => false,
            };
            pass(selection, shell, content, cursor, event);
            if started {
                shell.capture_event();
                shell.request_redraw();
            }
        }
        Event::Mouse(mouse::Event::CursorMoved { position }) if state.drag.is_some() => {
            pass(selection, shell, content, cursor, event);
            selection.drag_to(state, content, layout, renderer, *position);
            if state.held.as_ref().is_some_and(|held| held.selected) {
                shell.capture_event();
            }
            shell.request_redraw();
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.drag.is_some() => {
            state.drag = None;
            let Some(held) = state.held.as_mut().filter(|held| held.selected) else {
                state.drop_held();
                pass(selection, shell, content, cursor, event);
                return;
            };
            // What the press was on doesn't take this for a click: a link
            // pressed opens only when letting go selected nothing.
            let left = Event::Mouse(mouse::Event::CursorLeft);
            pass(selection, shell, content, mouse::Cursor::Unavailable, &left);
            pass(selection, shell, content, mouse::Cursor::Unavailable, event);
            if let Some(on_select) = &selection.on_select
                && let Some(copied) = selection.copied(held)
            {
                shell.publish(on_select(copied));
            }
            shell.capture_event();
            shell.request_redraw();
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
            if let Some(point) = cursor.position_over(bounds)
                && selection.on_menu.is_some()
            {
                let copied = selection.copied_at(state, content, layout, renderer, point);
                if let Some(on_menu) = &selection.on_menu {
                    shell.publish(on_menu(copied));
                }
            }
            pass(selection, shell, content, cursor, event);
        }
        Event::Mouse(mouse::Event::WheelScrolled { .. }) if state.drag.is_some() => {
            pass(selection, shell, content, cursor, event);
            // The list moves under the pointer: the next frames follow it.
            shell.request_redraw();
        }
        Event::Window(window::Event::RedrawRequested(now)) if state.drag.is_some() => {
            pass(selection, shell, content, cursor, event);
            selection.scroll_at_edge(state, content, layout, renderer, *now, shell);
            // Selects anew only where the rows moved under the pointer.
            if let Some(pointer) = state.drag.as_ref().map(|drag| drag.pointer) {
                selection.drag_to(state, content, layout, renderer, pointer);
            }
        }
        Event::Window(window::Event::Unfocused) => {
            if let Some(drag) = &mut state.drag {
                drag.frame = None;
            }
            pass(selection, shell, content, cursor, event);
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => {
            // A key ends an autoscroll, and Escape does nothing else then.
            let autoscrolling = autoscrolls(
                selection
                    .content
                    .mouse_interaction(content, layout, cursor, viewport, renderer),
            );
            pass(selection, shell, content, cursor, event);
            if is_copy(key, *physical_key, *modifiers)
                && !shell.is_event_captured()
                && let Some(copied) = state.held.as_mut().and_then(|held| selection.copied(held))
            {
                match &selection.on_copy {
                    Some(on_copy) => {
                        shell.write_clipboard(copied.clone());
                        shell.publish(on_copy(copied));
                    }
                    None => shell.write_clipboard(copied),
                }
                shell.capture_event();
            }
            // Escape clears it, and still goes on.
            if matches!(key, Key::Named(Named::Escape))
                && !autoscrolling
                && state.drag.is_none()
                && state.held.is_some()
            {
                state.drop_held();
                selection.wipe(content, layout, renderer);
                shell.request_redraw();
            }
        }
        _ => pass(selection, shell, content, cursor, event),
    }
}

impl<Message> Selection<'_, Message> {
    /// The selection as copied, if it covers part of the row at `point`.
    fn copied_at(
        &mut self,
        state: &mut State,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        point: Point,
    ) -> Option<String> {
        let held = state.held.as_mut().filter(|held| held.selected)?;
        if let Some(rows) = &self.rows {
            rows.find(held);
        }
        let (first, last) = held.ends();
        // Only the rows' places matter: rows showing it aren't gone into.
        let showing = Showing::behind(state.version);
        let walked = self.walk(content, layout, renderer, None, showing);
        let row = walked.rows.iter().find(|row| row.bounds.contains(point))?;
        if !(first.row..=last.row).contains(&row.index) {
            return None;
        }
        held.copied(self.rows.as_ref())
    }

    /// `held` as copied, its ends found again where rows went in or out.
    fn copied(&self, held: &mut Held) -> Option<String> {
        if let Some(rows) = &self.rows {
            rows.find(held);
        }
        held.copied(self.rows.as_ref())
    }
}

/// Whether the content takes a press where it shows `interaction` itself,
/// so none starts a selection there: a list's scrollbar shows the arrow
/// over its strip, an autoscroll its arrows, which the press ends (in Slint
/// it took the press before any text saw it). Texts show the caret, links
/// the hand, a whole text nothing.
fn takes_press(interaction: mouse::Interaction) -> bool {
    !matches!(
        interaction,
        mouse::Interaction::None | mouse::Interaction::Text | mouse::Interaction::Pointer
    )
}

/// Whether the content shows an autoscroll going on (`list::autoscroll`).
fn autoscrolls(interaction: mouse::Interaction) -> bool {
    matches!(
        interaction,
        mouse::Interaction::ResizingVertically | mouse::Interaction::AllScroll
    )
}

/// Whether a key copies: Ctrl+C, by the layout's letter, or in a layout
/// without Latin letters by the key in its place, as Slint's
/// `latin_on_same_key` lets Ctrl+С copy on a Russian keyboard; or a
/// keyboard's own Copy key.
fn is_copy(key: &Key, physical: Physical, modifiers: Modifiers) -> bool {
    if matches!(key, Key::Named(Named::Copy)) {
        return true;
    }
    if !modifiers.control() || modifiers.alt() || modifiers.shift() || modifiers.logo() {
        return false;
    }
    match key {
        Key::Character(text) if text.chars().all(|ch| ch.is_ascii_alphabetic()) => {
            text.eq_ignore_ascii_case("c")
        }
        _ => physical == Code::KeyC,
    }
}
