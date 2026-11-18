//! What the field does with an event: iced's editing state takes the
//! pointer, the keys and the input method, the keys going first through the
//! field's own ([`super::keys`]); then the field tells the app what changed
//! (`edited`, `caret`, `got-focus`, `ui/composer-field.slint:179`-`:182`).

use std::sync::Arc;
use std::time::Instant;

use iced::advanced::clipboard::{self, Content, Kind};
use iced::advanced::input_method;
use iced::advanced::text::editor::{Action, Edit, Editor as _, Update};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::{Shell, mouse};
use iced::{Event, Padding, Point, Rectangle, Vector, keyboard, window};

use super::keys::{self, Act, Can, Context};
use super::menu::{Choice, Open};
use super::state::State;
use super::{Composer, INSET, Suggestion};

impl<M: Clone> Composer<'_, M> {
    /// Handles one event over the field laid out at `bounds`, after its
    /// pills had it.
    pub(super) fn handle(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, M>,
    ) {
        if !self.enabled() {
            // A field turned off while it had the keys lets them go.
            if state.keys.is_focused() {
                Focusable::unfocus(&mut state.keys);
            }
            state.menu = None;
            self.tell_focus(state, shell);
            return;
        }
        // The focus a window's operation moved, before what this event does.
        self.tell_focus(state, shell);
        // A press anywhere ends a composition, as the caret may move.
        if state.preedit.is_some() && matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_)))
        {
            state.drop_preedit();
            shell.invalidate_layout();
        }
        if state.paste_wanted {
            state.paste_wanted = false;
            self.read_clipboard(state, shell);
        }
        match event {
            Event::Clipboard(clipboard::Event::Read(read)) => {
                // Only what this field asked for, and never by iced's own
                // pasting, which would take any read while focused.
                if std::mem::take(&mut state.awaiting)
                    && let Ok(content) = read
                    && let Content::Text(text) = content.as_ref()
                {
                    let text = Arc::new(text.clone());
                    self.perform(state, Action::Edit(Edit::Paste(text)), shell);
                }
            }
            Event::Clipboard(_) => {}
            Event::InputMethod(event) if state.keys.is_focused() => {
                self.compose(state, event, shell);
            }
            // While an input method composes, the keys are its own.
            Event::Keyboard(keyboard::Event::KeyPressed { .. }) if state.composing() => {
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(at) = cursor.position_in(bounds) {
                    if !state.keys.is_focused() {
                        Focusable::focus(&mut state.keys);
                    }
                    let selection = state.has_selection();
                    let at = Vector::new(at.x, at.y);
                    state.menu = Some(Open::new(at, selection, self.motion, Instant::now()));
                    shell.capture_event();
                    shell.request_redraw();
                    shell.invalidate_overlay();
                } else if state.keys.is_focused() {
                    // A right press elsewhere lets the keys go, as a left
                    // one does, so another field's menu takes them alone;
                    // the menu closes and the app hears it below.
                    Focusable::unfocus(&mut state.keys);
                }
            }
            // Nothing to scroll: the wheel is the list's above.
            Event::Mouse(mouse::Event::WheelScrolled { .. }) if !state.overflows => {}
            _ => {
                let padding = Padding {
                    top: state.pills + self.pad,
                    left: INSET,
                    right: INSET,
                    bottom: self.pad,
                };
                let context = self.context(state);
                let update = {
                    let editor = state.editor.borrow();
                    state
                        .keys
                        .update(&*editor, event, bounds, padding, cursor, |press| {
                            keys::bind(press, &context)
                        })
                };
                if let Some(update) = update {
                    self.apply(state, update, shell);
                }
            }
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            let origin = Point::new(bounds.x + INSET, bounds.y + state.pills + self.pad);
            let editor = state.editor.borrow();
            shell.request_input_method(&state.keys.input_method(&*editor, origin));
        }
        if !state.keys.is_focused() && state.menu.take().is_some() {
            shell.invalidate_overlay();
        }
        self.settle(state, shell);
    }

    /// What an input method sends: what it composes goes in the text at the
    /// caret, underlined, and comes out again before what it commits goes
    /// in as typing does (upstream Yapfast's field: iced_winit would draw it
    /// as a box over the text). iced's editing state never sees it, so the
    /// input method asks for no box.
    fn compose(&self, state: &mut State, event: &input_method::Event, shell: &mut Shell<'_, M>) {
        match event {
            input_method::Event::Opened => {}
            input_method::Event::Closed => state.drop_preedit(),
            input_method::Event::Preedit(content, selection) => {
                state.drop_preedit();
                if !content.is_empty() {
                    // A selection gives way to it, as to typing.
                    if state.has_selection() {
                        self.perform(state, Action::Edit(Edit::Delete), shell);
                    }
                    state.compose(content, selection.as_ref());
                }
            }
            input_method::Event::Commit(content) => {
                state.drop_preedit();
                let text = Arc::new(content.clone());
                self.perform(state, Action::Edit(Edit::Paste(text)), shell);
            }
        }
        shell.capture_event();
        // The composition changes no text the app knows, so nothing else
        // lays the editor out again before it's drawn.
        shell.invalidate_layout();
        shell.request_redraw();
    }

    /// Tells the app what changed of the focus and the caret.
    pub(super) fn settle(&self, state: &mut State, shell: &mut Shell<'_, M>) {
        self.tell_focus(state, shell);
        self.tell_caret(state, shell);
    }

    /// What the keys need to know.
    fn context(&self, state: &State) -> Context {
        Context {
            suggesting: self.suggesting,
            picking: self.picking,
            command: self.command,
            empty: state.text.is_empty(),
            composing: state.composing(),
            can: Can {
                suggest: self.on_suggestion.is_some(),
                command: self.on_command.is_some(),
                send: self.on_send.is_some(),
                up: self.on_up.is_some(),
                browse: self.on_browse.is_some(),
                paste: self.on_paste.is_some(),
                expressions: self.on_expressions.is_some(),
            },
        }
    }

    fn apply(&self, state: &mut State, update: Update<Act>, shell: &mut Shell<'_, M>) {
        match update {
            Update::Action(action) => self.perform(state, action, shell),
            Update::Custom(act) => {
                self.act(state, act, shell);
                shell.capture_event();
            }
            Update::Copy(text) => {
                shell.write_clipboard(Content::Text(text));
                shell.capture_event();
            }
            Update::Paste => {
                self.read_clipboard(state, shell);
                shell.capture_event();
            }
            Update::RedrawAt(at) => {
                if self.blinks() {
                    shell.request_redraw_at(at);
                }
            }
            Update::Focus | Update::Unfocus | Update::InputMethod => shell.request_redraw(),
            Update::Release => {}
            Update::Sequence(updates) => {
                for update in updates {
                    self.apply(state, update, shell);
                }
            }
        }
    }

    /// Does `action` to the text, and tells the app of an edit.
    fn perform(&self, state: &mut State, action: Action, shell: &mut Shell<'_, M>) {
        let edit = action.is_edit();
        let action = match action {
            // Windows' line ends, as a clipboard may hold them.
            Action::Edit(Edit::Paste(text)) if text.contains('\r') => {
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                Action::Edit(Edit::Paste(Arc::new(text)))
            }
            action => action,
        };
        state.editor.get_mut().perform(action);
        shell.request_redraw();
        shell.capture_event();
        if edit && state.take_text() {
            // It may have grown or shrunk by a line.
            shell.invalidate_layout();
            let caret = state.caret_offset();
            state.caret = caret;
            if let Some(on_edit) = &self.on_edit {
                shell.publish(on_edit(state.text.clone(), caret));
            }
        }
    }

    /// Hands the app the message of `act`. One that may set the text marks
    /// the state, so the next view's text replaces the field's.
    fn act(&self, state: &mut State, act: Act, shell: &mut Shell<'_, M>) {
        let sets_text = matches!(
            act,
            Act::Send | Act::Suggest(Suggestion::Choose) | Act::Command(_)
        );
        let message = match act {
            Act::Send => self.on_send.clone(),
            Act::Up => self.on_up.clone(),
            Act::Browse => self.on_browse.clone(),
            Act::Paste => self.on_paste.clone(),
            Act::Expressions(which) => self.on_expressions.as_ref().map(|asked| asked(which)),
            Act::Suggest(key) => self.on_suggestion.as_ref().map(|asked| asked(key)),
            Act::Command(key) => self.on_command.as_ref().map(|asked| asked(key)),
        };
        if let Some(message) = message {
            state.handed |= sets_text;
            shell.publish(message);
        }
    }

    /// A row of the edit menu chosen (`chose(id)`, `:203`): the field keeps
    /// the keys.
    pub(super) fn choose(&self, state: &mut State, choice: Choice, shell: &mut Shell<'_, M>) {
        state.menu = None;
        shell.invalidate_overlay();
        shell.request_redraw();
        if !state.keys.is_focused() {
            Focusable::focus(&mut state.keys);
        }
        match choice {
            Choice::Cut => {
                let copied = state.editor.get_mut().copy();
                if let Some(text) = copied {
                    shell.write_clipboard(Content::Text(text));
                    self.perform(state, Action::Edit(Edit::Backspace), shell);
                }
            }
            Choice::Copy => {
                if let Some(text) = state.editor.get_mut().copy() {
                    shell.write_clipboard(Content::Text(text));
                }
            }
            Choice::Paste => match &self.on_paste {
                Some(paste) => shell.publish(paste.clone()),
                None => self.read_clipboard(state, shell),
            },
            Choice::SelectAll => self.perform(state, Action::SelectAll, shell),
            Choice::Close => {}
        }
        self.settle(state, shell);
    }

    /// Asks the clipboard for text, which pastes when it comes.
    fn read_clipboard(&self, state: &mut State, shell: &mut Shell<'_, M>) {
        state.awaiting = true;
        shell.read_clipboard(Kind::Text);
    }

    /// Tells the app the field took the keys or lost them.
    fn tell_focus(&self, state: &mut State, shell: &mut Shell<'_, M>) {
        let focused = state.keys.is_focused();
        if focused == state.focused {
            return;
        }
        state.focused = focused;
        shell.request_redraw();
        if let Some(on_focus) = &self.on_focus {
            shell.publish(on_focus(focused));
        }
    }

    /// Tells the app the caret moved, when it did.
    fn tell_caret(&self, state: &mut State, shell: &mut Shell<'_, M>) {
        // Inside a composition the caret is in text the app doesn't know.
        if state.composing() {
            return;
        }
        let caret = state.caret_offset();
        if caret == state.caret {
            return;
        }
        state.caret = caret;
        if let Some(on_caret) = &self.on_caret {
            shell.publish(on_caret(caret));
        }
    }
}
