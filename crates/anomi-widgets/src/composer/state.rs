//! What the field keeps between views: the editor with its own copy of the
//! text, iced's editing state (focus, clicks, the input method's
//! composition, the caret's blink), and what it last told the app.

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::text::editor::{self, Action, Cursor, Editor as _, Selection};

use super::marks::Marker;
use super::menu::Open;
use super::offset;

/// The editor iced's renderer draws: cosmic-text's buffer with its caret.
pub(super) type Field = iced::advanced::graphics::text::Editor;

pub(super) struct State {
    /// In a cell so drawing can colour the marks, as iced's own editor does.
    pub editor: RefCell<Field>,
    /// Focus, the last click, a drag, the composition, the blink.
    pub keys: editor::State,
    pub marker: RefCell<Marker>,
    /// Whose draft the editor holds.
    pub key: u64,
    /// The text the editor holds, kept beside it so each view compares it
    /// with the view-model's without asking the editor to join its lines.
    pub text: String,
    /// Counts the changes of `text`, so the marks know when to find their
    /// lines again.
    pub version: u64,
    /// The view-model's text as the last view gave it, hashed: when it
    /// changes, the app has set a text of its own.
    pub seen: u64,
    /// The field handed the app a message that may set its text (a send, a
    /// suggestion chosen, a command's key) since the last view: the next
    /// view's text is the app's answer, even when it equals the last one's,
    /// as when the typing and the Enter that sent it came in one batch.
    pub handed: bool,
    /// The caret's byte offset last reported, and whether the field had the
    /// keys when last said.
    pub caret: usize,
    pub focused: bool,
    /// The caret moved where the view may not show it: the next layout
    /// scrolls to it.
    pub reveal: bool,
    /// The field asked the clipboard for text, and pastes what comes.
    pub awaiting: bool,
    /// The app asked for a paste ([`super::paste_text`]): the next event
    /// asks the clipboard.
    pub paste_wanted: bool,
    /// The pills' height with their gap, the height of the text's part, and
    /// whether the text is taller than that part, as last laid out.
    pub pills: f32,
    pub visible: f32,
    pub overflows: bool,
    /// The edit menu, while it's open.
    pub menu: Option<Open>,
    /// The time of the frame last drawn, for the menu's way in.
    pub now: Instant,
}

impl State {
    pub fn new(text: &str, key: u64) -> Self {
        let mut state = Self {
            editor: RefCell::new(Field::with_text(text)),
            keys: editor::State::new(),
            marker: RefCell::new(Marker::default()),
            key,
            text: text.to_owned(),
            version: 0,
            seen: hash(text),
            handed: false,
            caret: 0,
            focused: false,
            reveal: true,
            awaiting: false,
            paste_wanted: false,
            pills: 0.0,
            visible: 0.0,
            overflows: false,
            menu: None,
            now: Instant::now(),
        };
        state.caret_to_end();
        state
    }

    /// Takes the view-model's `text` for `key`, as each view gives them.
    /// Another key starts that draft afresh. A text the app set replaces
    /// the field's, the caret kept where it was as far as the text allows;
    /// so does the answer to a message [`Self::handed`]. A text the
    /// view-model held already leaves the field's alone otherwise, which
    /// may be ahead of it by an edit the app hasn't answered yet.
    pub fn sync(&mut self, text: &str, key: u64) {
        let seen = hash(text);
        if key != self.key {
            self.key = key;
            self.replace(text);
            self.caret_to_end();
            // Told at the next event, so the app finds the suggestions of
            // the draft it comes back to.
            self.caret = usize::MAX;
            self.menu = None;
        } else if text != self.text && (seen != self.seen || self.handed) {
            let caret = self.caret_offset();
            self.replace(text);
            self.place(caret);
            // The app set it: nothing to tell it.
            self.caret = self.caret_offset();
        }
        self.seen = seen;
        self.handed = false;
    }

    /// The editor with `text` and nothing to undo: what was done to the
    /// last text can't be undone over this one. The layout that follows
    /// every view lays it out.
    fn replace(&mut self, text: &str) {
        *self.editor.get_mut() = Field::with_text(text);
        self.marker.get_mut().forget();
        self.text = text.to_owned();
        self.version += 1;
    }

    fn caret_to_end(&mut self) {
        let editor = self.editor.get_mut();
        let end = offset::end_of(editor);
        editor.move_to(Cursor {
            position: end,
            selection: None,
        });
        self.caret = offset::offset_of(editor, end);
        self.reveal = true;
    }

    /// Puts the caret at a byte offset, its selection gone, and scrolls to
    /// it at the next layout.
    pub fn place(&mut self, at: usize) {
        let editor = self.editor.get_mut();
        let position = offset::position_of(editor, at);
        editor.move_to(Cursor {
            position,
            selection: None,
        });
        self.reveal = true;
    }

    /// The caret's byte offset in the text: the end a selection moves.
    pub fn caret_offset(&self) -> usize {
        let editor = self.editor.borrow();
        offset::offset_of(&editor, editor.cursor().position)
    }

    /// After an edit: takes the editor's text, and says whether it changed.
    pub fn take_text(&mut self) -> bool {
        let text = self.editor.get_mut().text();
        if text == self.text {
            return false;
        }
        self.text = text;
        self.version += 1;
        true
    }

    /// Something is selected.
    pub fn has_selection(&self) -> bool {
        self.editor.borrow().cursor().selection.is_some()
    }

    /// Whether an input method is composing text at the caret, whose keys
    /// are its own.
    pub fn composing(&self) -> bool {
        let editor = self.editor.borrow();
        matches!(
            self.keys.input_method(&*editor, iced::Point::ORIGIN),
            iced::advanced::InputMethod::Enabled {
                preedit: Some(preedit),
                ..
            } if !preedit.content.is_empty()
        )
    }

    /// Scrolls the text so the caret's line shows in the part `visible`
    /// tall, as the Slint field's flickable follows `cursor-position-changed`.
    pub fn show_caret(&mut self, visible: f32) {
        let editor = self.editor.get_mut();
        let height = editor.line_height().to_absolute(editor.text_size()).0;
        let Some(selection) = editor.selection() else {
            return;
        };
        let top = match selection {
            Selection::Caret(point) => point.y,
            Selection::Range(regions) => {
                let Some(first) = regions.first() else {
                    return;
                };
                first.y
            }
        };
        if height <= 0.0 {
            return;
        }
        let lines = if top < 0.0 {
            (top / height).floor()
        } else if top + height > visible + 0.5 {
            ((top + height - visible) / height).ceil()
        } else {
            return;
        };
        editor.perform(Action::Scroll {
            lines: lines as i32,
        });
    }
}

/// A text's hash, to tell whether the view-model's changed without keeping
/// a second copy of it.
fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
