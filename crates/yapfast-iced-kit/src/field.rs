//! The composer's text field, and the picker's search field: a multi-line
//! field that grows with its text up to ten lines, then scrolls to keep the
//! caret in view. Enter sends and Shift+Enter breaks the line; with
//! suggestions over it, Up and Down move through them, Tab or Enter takes
//! one and Escape closes them. Mouse and keyboard selection (a click with
//! Shift held extends it), Ctrl+A, C, X and V, and undo (Ctrl+Z,
//! Ctrl+Shift+Z or Ctrl+Y) by typing runs. Text selected with the pointer
//! becomes the primary selection, which a middle click pastes at the
//! pointer (where the desktop has one, as X11 and Wayland do).
//!
//! iced's `text_editor` has none of the composer's keys, no undo, and draws
//! an input method's preedit as a box over the text at the caret (iced_winit
//! paints it over whatever is there), so this is a widget of its own on
//! iced's editor (`graphics::text::Editor`, cosmic-text's): the preedit goes
//! into the text at the caret, underlined, and comes out again before the
//! committed text goes in, so the draft, undo and the core never see it.
//!
//! The header's search box draws its filters as Discord's does: runs of the
//! text as pills, keys in semibold ([`Field::pills`]), and some removed
//! whole by Backspace or Delete at their edge or inside ([`Field::atoms`]).

use std::{
    cell::RefCell,
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{
    Border, Color, Element, Font, Length, Padding, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, clipboard,
        graphics::text::{Editor, cosmic_text},
        input_method::{self, InputMethod},
        layout, mouse, renderer,
        text::{
            self, LineHeight, Wrapping,
            editor::{Action, Cursor, Edit, Editor as _, Motion, Position, Selection},
            highlighter::{self, Highlighter, PlainText},
        },
        widget::{Id, Tree, operation, tree},
    },
    keyboard::{self, key},
    window,
};

use crate::fonts::{REGULAR, SEMIBOLD};

type Renderer = iced::Renderer;

/// What the field tells the app.
#[derive(Clone, Debug)]
pub enum Event {
    /// The text changed: the whole of it and the caret's byte offset.
    Edited(String, usize),
    /// The caret moved without an edit, to this byte offset.
    Moved(usize),
    /// Enter, with nothing over the composer to take it.
    Send,
    /// Up or Down with suggestions shown.
    Step(i32),
    /// Tab or Enter with suggestions shown.
    Choose,
    /// Escape: closes suggestions, else what the app has open.
    Escape,
    /// Ctrl+E, as Discord's: the emoji picker.
    Emoji,
    /// A right click, at this place in the window.
    Menu(Point),
    /// Up in an empty composer: edit the last message.
    UpFromEmpty,
    /// Alt+Up: into the messages, to step through them.
    UpToMessages,
    /// Ctrl+V in a field that takes files (`Field::pastes`): the app looks
    /// at the clipboard first, and pastes text itself.
    Paste,
    /// Ctrl+G, as Discord's: the GIF picker.
    Gifs,
    /// The field took the keys, or lost them.
    Focus(bool),
    /// Tab, or with `true` Shift+Tab, in a field that passes them on
    /// (`Field::tabs`): to the next or the last of an app command's options.
    Tab(bool),
    /// Backspace in an empty field that passes Tab on.
    BackFromEmpty,
}

/// How a run of typing is grouped for undo.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Insert,
    Delete,
    /// Pastes, cuts, line breaks, a picked mention: each its own step.
    Other,
}

/// What undo goes back to.
struct Snapshot {
    text: String,
    caret: usize,
}

struct Inner {
    editor: Editor,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last edit's kind and time; a new kind, a space after a word, or a
    /// pause of a second starts a new step.
    last: Option<(Kind, Instant)>,
    /// The input method's text, put into the editor at the caret: its first
    /// and last place, on one line.
    preedit: Option<(Position, Position)>,
    /// The height last laid out, for what opens over the composer.
    height: f32,
    /// The text has runs in semibold, which a layout without pills undoes.
    styled: bool,
    /// How far a one-line field's text is scrolled left, to keep the caret
    /// in view.
    scroll_x: f32,
}

/// The composer's text and its history, which the app keeps per channel
/// (in the core's draft) and the widget edits in place.
pub struct Draft(RefCell<Inner>);

/// The most undo steps kept: a draft's, so a few KiB at most.
const STEPS: usize = 100;

impl Default for Draft {
    fn default() -> Self {
        Self(RefCell::new(Inner {
            editor: Editor::with_text(""),
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            preedit: None,
            height: 48.0,
            styled: false,
            scroll_x: 0.0,
        }))
    }
}

fn whole(editor: &Editor) -> String {
    let mut text = String::new();
    for (index, line) in editor.buffer().lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(line.text());
    }
    text
}

/// A position's byte offset in the whole text.
fn offset(editor: &Editor, position: Position) -> usize {
    let lines = &editor.buffer().lines;
    lines[..position.line.min(lines.len())]
        .iter()
        .map(|line| line.text().len() + 1)
        .sum::<usize>()
        + position.column
}

/// The position of a byte offset in the whole text.
fn position(text: &str, at: usize) -> Position {
    let at = at.min(text.len());
    let before = &text[..at];
    let line = before.matches('\n').count();
    let column = before.rfind('\n').map_or(at, |newline| at - newline - 1);
    Position { line, column }
}

impl Draft {
    /// The text, without any preedit.
    pub fn text(&self) -> String {
        whole(&self.0.borrow().editor)
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().editor.is_empty()
    }

    /// The field's height as last laid out.
    pub fn height(&self) -> f32 {
        self.0.borrow().height
    }

    pub fn caret(&self) -> usize {
        let inner = self.0.borrow();
        offset(&inner.editor, inner.editor.cursor().position)
    }

    /// The selected text.
    pub fn selection(&self) -> Option<String> {
        self.0.borrow().editor.copy()
    }

    /// Takes a new text, as when another channel opens: its history goes.
    pub fn reset(&self, text: &str) {
        let mut inner = self.0.borrow_mut();
        inner.undo.clear();
        inner.redo.clear();
        inner.last = None;
        inner.preedit = None;
        inner.editor = Editor::with_text(text);
        inner.editor.perform(Action::Move(Motion::DocumentEnd));
    }

    /// Replaces the text, as picking a mention or an emoji does, with the
    /// caret at `caret`: one undo step.
    pub fn replace(&self, text: &str, caret: usize) {
        let mut inner = self.0.borrow_mut();
        inner.remember(Kind::Other);
        inner.editor = Editor::with_text(text);
        let at = position(text, caret);
        inner.editor.move_to(Cursor {
            position: at,
            selection: None,
        });
    }

    /// A key typed while nothing had the keys, at the end of the text, as
    /// the field would take it with its caret there.
    pub fn type_at_end(&self, typed: &Typed) {
        let mut inner = self.0.borrow_mut();
        inner.editor.perform(Action::Move(Motion::DocumentEnd));
        let (kind, edits) = match typed {
            Typed::Text(text) => (Kind::Insert, text.chars().map(Edit::Insert).collect()),
            Typed::Backspace => (Kind::Delete, vec![Edit::Backspace]),
            Typed::Delete => (Kind::Delete, vec![Edit::Delete]),
        };
        for edit in edits {
            inner.remember(kind);
            inner.editor.perform(Action::Edit(edit));
        }
    }

    /// Edits as the composer's menu does: cut, copy, paste or select all.
    pub fn act(&self, edit: MenuEdit, clipboard_text: Option<String>) -> Option<String> {
        let mut inner = self.0.borrow_mut();
        match edit {
            MenuEdit::SelectAll => inner.editor.perform(Action::SelectAll),
            MenuEdit::Copy => return inner.editor.copy(),
            MenuEdit::Cut => {
                let copied = inner.editor.copy();
                if copied.is_some() {
                    inner.remember(Kind::Other);
                    inner.editor.perform(Action::Edit(Edit::Delete));
                }
                return copied;
            }
            MenuEdit::Paste => {
                if let Some(text) = clipboard_text {
                    inner.remember(Kind::Other);
                    inner
                        .editor
                        .perform(Action::Edit(Edit::Paste(Arc::new(text))));
                }
            }
        }
        None
    }
}

/// A key that writes, typed with nothing holding the keys (`App::key`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    Text(String),
    Backspace,
    Delete,
}

impl Typed {
    /// What a key nothing took writes, if it writes: text without Ctrl
    /// (AltGr's is), Space, Backspace or Delete. Enter isn't one: Discord's
    /// sends the draft, which a stray press after a menu closes shouldn't.
    pub fn of(key: &key::Key, modifiers: keyboard::Modifiers) -> Option<Self> {
        if (modifiers.command() && !modifiers.alt()) || modifiers.logo() {
            return None;
        }
        match key.as_ref() {
            key::Key::Character(text) if text.chars().all(|c| !c.is_control()) => {
                Some(Self::Text(text.to_owned()))
            }
            key::Key::Named(key::Named::Space) => Some(Self::Text(" ".to_owned())),
            key::Key::Named(key::Named::Backspace) => Some(Self::Backspace),
            key::Key::Named(key::Named::Delete) => Some(Self::Delete),
            _ => None,
        }
    }
}

/// What the composer's own menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEdit {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl Inner {
    /// Keeps the text for undo before an edit of `kind`, unless it goes on
    /// the run of typing before it.
    fn remember(&mut self, kind: Kind) {
        let now = Instant::now();
        let continues = self.last.is_some_and(|(last, at)| {
            last == kind && kind != Kind::Other && now.duration_since(at) < Duration::from_secs(1)
        });
        self.last = Some((kind, now));
        self.redo.clear();
        if continues {
            return;
        }
        let snapshot = Snapshot {
            text: whole(&self.editor),
            caret: offset(&self.editor, self.editor.cursor().position),
        };
        if self
            .undo
            .last()
            .is_some_and(|last| last.text == snapshot.text)
        {
            return;
        }
        if self.undo.len() == STEPS {
            self.undo.remove(0);
        }
        self.undo.push(snapshot);
    }

    fn restore(&mut self, back: bool) -> bool {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(snapshot) = from.pop() else {
            return false;
        };
        to.push(Snapshot {
            text: whole(&self.editor),
            caret: offset(&self.editor, self.editor.cursor().position),
        });
        self.editor = Editor::with_text(&snapshot.text);
        let at = position(&snapshot.text, snapshot.caret);
        self.editor.move_to(Cursor {
            position: at,
            selection: None,
        });
        self.last = None;
        true
    }

    /// Takes the preedit out of the text, leaving the caret where it was.
    fn drop_preedit(&mut self) {
        if let Some((start, end)) = self.preedit.take() {
            self.editor.move_to(Cursor {
                position: end,
                selection: Some(start),
            });
            self.editor.perform(Action::Edit(Edit::Delete));
        }
    }
}

// ---------------------------------------------------------------------------
// The widget

pub struct Field<'a, M> {
    draft: &'a Draft,
    id: Id,
    placeholder: String,
    /// Mention suggestions show: the arrows, Tab, Enter and Escape go to them.
    suggesting: bool,
    font: Font,
    size: f32,
    line: f32,
    /// Room above and below the text, and either side.
    pad: Padding,
    lines: f32,
    /// A search field: one line, no line breaks.
    single: bool,
    /// Enter breaks the line rather than sending, as in a plain text box.
    breaks: bool,
    /// Ctrl+V asks the app (`Event::Paste`).
    pastes: bool,
    /// Tab and Backspace at its start go to the app.
    tabs: bool,
    /// Takes no input, its text greyed.
    enabled: bool,
    /// Runs drawn as pills, and their inks.
    pills: Vec<Pill>,
    pill_inks: Option<PillInks>,
    /// Runs Backspace and Delete take whole.
    atoms: Vec<Range<usize>>,
    colors: Colors,
    /// The line round it while it has the keys, and its corners.
    ring: Option<Color>,
    radius: f32,
    on: fn(Event) -> M,
}

/// A run of the text, by byte offsets, drawn as a pill: a filter's key, in
/// semibold, or its answer. Runs that meet are one pill, as Discord's
/// search box joins a key and its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pill {
    pub range: Range<usize>,
    pub bold: bool,
}

/// A pill's ground and outline.
#[derive(Clone, Copy, Debug)]
pub struct PillInks {
    pub ground: Color,
    pub line: Color,
}

/// A pill's height, its corners' radius and the room either side of its
/// text, as Discord's search tokens have (`searchToken`).
const PILL_HEIGHT: f32 = 22.0;
const PILL_RADIUS: f32 = 4.0;
const PILL_PAD: f32 = 1.0;

/// The room a one-line field lays its text out in: more than any line.
const UNWRAPPED: f32 = 100_000.0;

#[derive(Clone, Copy)]
pub struct Colors {
    pub ground: Color,
    pub text: Color,
    pub placeholder: Color,
    pub selection: Color,
}

impl<'a, M: Clone> Field<'a, M> {
    pub fn new(
        draft: &'a Draft,
        id: Id,
        placeholder: String,
        on: fn(Event) -> M,
        colors: Colors,
    ) -> Self {
        Self {
            draft,
            id,
            placeholder,
            suggesting: false,
            font: REGULAR,
            size: 16.0,
            line: 24.0,
            pad: Padding {
                top: 12.0,
                right: 6.0,
                bottom: 12.0,
                left: 6.0,
            },
            lines: 10.0,
            single: false,
            breaks: false,
            pastes: false,
            tabs: false,
            enabled: true,
            pills: Vec::new(),
            pill_inks: None,
            atoms: Vec::new(),
            colors,
            ring: None,
            radius: 8.0,
            on,
        }
    }

    /// Ctrl+V asks the app, which may find files or a picture to attach.
    pub fn pastes(mut self) -> Self {
        self.pastes = true;
        self
    }

    /// Tab and Backspace at an empty field's start go to the app.
    pub fn tabs(mut self, tabs: bool) -> Self {
        self.tabs = tabs;
        self
    }

    /// Draws `pills` behind the text, in `inks`.
    pub fn pills(mut self, pills: Vec<Pill>, inks: PillInks) -> Self {
        self.pills = pills;
        self.pill_inks = Some(inks);
        self
    }

    /// Backspace with the caret inside one of `atoms` or at its end, or
    /// Delete inside or at its start, removes it whole.
    pub fn atoms(mut self, atoms: Vec<Range<usize>>) -> Self {
        self.atoms = atoms;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A line of `ring` round it while it has the keys.
    pub fn ring(mut self, ring: Color) -> Self {
        self.ring = Some(ring);
        self
    }

    pub fn single(mut self) -> Self {
        self.single = true;
        self.lines = 1.0;
        self
    }

    /// Enter breaks the line, as Shift+Enter does, and nothing sends.
    pub fn breaks(mut self) -> Self {
        self.breaks = true;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn suggesting(mut self, suggesting: bool) -> Self {
        self.suggesting = suggesting;
        self
    }

    pub fn padding(mut self, pad: Padding) -> Self {
        self.pad = pad;
        self
    }

    pub fn lines(mut self, lines: f32) -> Self {
        self.lines = lines;
        self
    }

    /// Its corners' radius; 8 px without.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Each line's height, in pixels; 24 without.
    pub fn line_height(mut self, line: f32) -> Self {
        self.line = line;
        self
    }
}

#[derive(Default)]
struct State {
    focus: Option<Focus>,
    /// Whether the app was last told the field has the keys.
    told: bool,
    last_click: Option<mouse::Click>,
    dragging: bool,
    /// A left or middle press began in the field: its release copies the
    /// selection to the primary selection, or pastes from it.
    pressed: Option<mouse::Button>,
    /// The keys held, for Shift with a click.
    modifiers: keyboard::Modifiers,
}

struct Focus {
    updated_at: Instant,
    now: Instant,
    window_focused: bool,
    /// Which taking of the keys this was, of all the fields'.
    taken: u64,
}

thread_local! {
    /// How many times a field has taken the keys. The one that took them
    /// last has them: a right or middle press on another field, which the
    /// first never hears as a press of its own, takes them from it.
    static TAKEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Takes the keys for something that isn't a field (`widgets::Selectable`),
/// so the field that had them lets them go: which taking it was.
pub fn take_keys() -> u64 {
    TAKEN.with(|taken| {
        taken.set(taken.get() + 1);
        taken.get()
    })
}

/// Whether nothing has taken the keys since `taken` did.
pub fn holds_keys(taken: u64) -> bool {
    TAKEN.with(|now| now.get() == taken)
}

const BLINK: u128 = 500;

/// What a paste puts in a field: in a one-line field, its line breaks as
/// spaces.
fn pasted_into(pasted: String, single: bool) -> String {
    if single && pasted.contains(['\n', '\r']) {
        pasted.replace("\r\n", " ").replace(['\n', '\r'], " ")
    } else {
        pasted
    }
}

impl Focus {
    fn now() -> Self {
        let now = Instant::now();
        Self {
            updated_at: now,
            now,
            window_focused: true,
            taken: TAKEN.with(|taken| {
                taken.set(taken.get() + 1);
                taken.get()
            }),
        }
    }

    /// Whether no field has taken the keys since.
    fn holds(&self) -> bool {
        TAKEN.with(|taken| taken.get() == self.taken)
    }

    fn caret_shown(&self) -> bool {
        self.window_focused && ((self.now - self.updated_at).as_millis() / BLINK).is_multiple_of(2)
    }
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
        self.dragging = false;
    }
}

impl<M: Clone> Field<'_, M> {
    fn text_bounds(&self, layout: Layout<'_>) -> Rectangle {
        layout.bounds().shrink(self.pad)
    }

    /// Publishes the text and caret after an edit.
    fn edited(&self, inner: &Inner, shell: &mut Shell<'_, M>) {
        let text = whole(&inner.editor);
        let caret = offset(&inner.editor, inner.editor.cursor().position);
        shell.publish((self.on)(Event::Edited(text, caret)));
    }

    fn moved(&self, inner: &Inner, shell: &mut Shell<'_, M>) {
        let caret = offset(&inner.editor, inner.editor.cursor().position);
        shell.publish((self.on)(Event::Moved(caret)));
    }

    /// Scrolls a one-line field's text so the caret shows in `room`.
    fn follow_caret(&self, inner: &mut Inner, room: f32) {
        if !self.single {
            return;
        }
        let room = room.max(1.0);
        let mut scroll = inner.scroll_x;
        if let Some(caret) = self.caret(inner) {
            if caret.x < scroll {
                scroll = caret.x;
            } else if caret.x + 1.0 > scroll + room {
                scroll = caret.x + 1.0 - room;
            }
        }
        let most = (inner.editor.min_bounds().width + 1.0 - room).max(0.0);
        inner.scroll_x = scroll.clamp(0.0, most);
    }

    /// Where the text's corner is drawn: the text's room, less the scroll.
    fn origin(&self, inner: &Inner, text_bounds: Rectangle) -> Point {
        Point::new(text_bounds.x - inner.scroll_x, text_bounds.y)
    }

    /// Removes the atom Backspace (`back`) or Delete takes at the caret,
    /// with nothing selected: whether there was one.
    fn take_atom(&self, inner: &mut Inner, back: bool) -> bool {
        if inner.editor.copy().is_some() {
            return false;
        }
        let text = whole(&inner.editor);
        let caret = offset(&inner.editor, inner.editor.cursor().position);
        let Some(atom) = self.atoms.iter().find(|atom| {
            fits(&text, atom)
                && if back {
                    atom.start < caret && caret <= atom.end
                } else {
                    atom.start <= caret && caret < atom.end
                }
        }) else {
            return false;
        };
        inner.remember(Kind::Other);
        inner.editor.move_to(Cursor {
            position: position(&text, atom.end),
            selection: Some(position(&text, atom.start)),
        });
        inner.editor.perform(Action::Edit(Edit::Delete));
        true
    }

    /// The pills' boxes: runs that meet joined, each line of a run its own.
    fn pill_boxes(&self, inner: &Inner, origin: Point) -> Vec<Rectangle> {
        if inner.preedit.is_some() {
            return Vec::new();
        }
        let text = whole(&inner.editor);
        let mut runs: Vec<Range<usize>> = Vec::new();
        for pill in self.pills.iter().filter(|pill| fits(&text, &pill.range)) {
            match runs.last_mut() {
                Some(last) if last.end == pill.range.start => last.end = pill.range.end,
                _ => runs.push(pill.range.clone()),
            }
        }
        let mut boxes = Vec::new();
        for range in runs {
            let (start, end) = (position(&text, range.start), position(&text, range.end));
            let from = cosmic_text::Cursor::new(start.line, start.column);
            let to = cosmic_text::Cursor::new(end.line, end.column);
            for run in inner
                .editor
                .buffer()
                .layout_runs()
                .filter(|run| (start.line..=end.line).contains(&run.line_i))
            {
                if let Some((x, width)) = run.highlight(from, to)
                    && width > 0.0
                {
                    let y = run.line_top + (self.line - PILL_HEIGHT) / 2.0;
                    boxes.push(Rectangle::new(
                        Point::new(origin.x + x - PILL_PAD, origin.y + y),
                        Size::new(width + 2.0 * PILL_PAD, PILL_HEIGHT),
                    ));
                }
            }
        }
        boxes
    }

    /// The caret's place, from the text's corner.
    fn caret(&self, inner: &Inner) -> Option<Point> {
        match inner.editor.selection() {
            Selection::Caret(point) => Some(point),
            Selection::Range(ranges) => ranges
                .first()
                .map(|range| Point::new(range.x + range.width, range.y)),
        }
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Field<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        // As many lines as it shows, and no taller than its room.
        let most = (self.lines * self.line)
            .min(limits.max().height - self.pad.y())
            .max(self.line);
        let mut inner = self.draft.0.borrow_mut();
        // A one-line field's text runs on past its end, scrolled to the
        // caret, as a search box's does.
        let (room, wrapping) = if self.single {
            (UNWRAPPED, Wrapping::None)
        } else {
            ((width - self.pad.x()).max(1.0), Wrapping::WordOrGlyph)
        };
        inner.editor.update(
            Size::new(room, most),
            self.font,
            self.size.into(),
            LineHeight::Absolute(self.line.into()),
            wrapping,
            &mut PlainText::new(&()),
        );
        // Keys in semibold, or back to regular once there are none; not
        // while an input method composes, which moves the text.
        let bold: Vec<Range<usize>> = if inner.preedit.is_none() {
            let text = whole(&inner.editor);
            self.pills
                .iter()
                .filter(|pill| pill.bold && fits(&text, &pill.range))
                .map(|pill| pill.range.clone())
                .collect()
        } else {
            Vec::new()
        };
        if !bold.is_empty() || inner.styled {
            inner.styled = !bold.is_empty();
            inner
                .editor
                .highlight(self.font, &mut Bold::new(&bold), |_| highlighter::Format {
                    color: None,
                    font: Some(SEMIBOLD),
                });
        }
        let content = inner.editor.min_bounds().height.clamp(self.line, most);
        self.follow_caret(&mut inner, width - self.pad.x());
        inner.height = content + self.pad.y();
        layout::Node::new(Size::new(width, content + self.pad.y()))
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(Some(&self.id), layout.bounds(), state);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if !self.enabled || state.focus.as_ref().is_some_and(|focus| !focus.holds()) {
            state.focus = None;
            state.dragging = false;
        }
        // Focus moves by the app's tasks as well as by presses: the next
        // event after it tells the app.
        if state.focus.is_some() != state.told {
            state.told = state.focus.is_some();
            shell.publish((self.on)(Event::Focus(state.told)));
        }
        if !self.enabled {
            return;
        }
        let bounds = layout.bounds();
        let text_bounds = self.text_bounds(layout);
        match event {
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            iced::Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window_focused = false;
                }
            }
            iced::Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window_focused = true;
                    focus.updated_at = Instant::now();
                    shell.request_redraw();
                }
            }
            iced::Event::Window(window::Event::RedrawRequested(now)) => {
                // The caret's blink, the only thing the window does at rest.
                if let Some(focus) = &mut state.focus
                    && focus.window_focused
                {
                    focus.now = *now;
                    let left = BLINK - (focus.now - focus.updated_at).as_millis() % BLINK;
                    shell.request_redraw_at(focus.now + Duration::from_millis(left as u64));
                    let inner = self.draft.0.borrow();
                    if let Some(caret) = self.caret(&inner) {
                        // The input method's window goes at the caret.
                        let at = caret + (self.origin(&inner, text_bounds) - Point::ORIGIN);
                        shell.request_input_method(&InputMethod::<&str>::Enabled {
                            cursor: Rectangle::new(at, Size::new(1.0, self.line)),
                            purpose: input_method::Purpose::Normal,
                            // Drawn here, in the text; not iced_winit's box.
                            preedit: None,
                        });
                    }
                }
                return;
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_in(text_bounds.expand(self.pad)) else {
                    state.focus = None;
                    state.dragging = false;
                    state.pressed = None;
                    return;
                };
                let mut inner = self.draft.0.borrow_mut();
                let at = at - Vector::new(self.pad.left - inner.scroll_x, self.pad.top);
                let click = mouse::Click::new(at, mouse::Button::Left, state.last_click);
                inner.drop_preedit();
                inner.editor.perform(match click.kind() {
                    // With Shift held, the selection runs from where it
                    // was anchored (or the caret) to the click.
                    mouse::click::Kind::Single if state.modifiers.shift() => Action::Drag(at),
                    mouse::click::Kind::Single => Action::Click(at),
                    mouse::click::Kind::Double => Action::SelectWord,
                    mouse::click::Kind::Triple => Action::SelectLine,
                });
                state.last_click = Some(click);
                state.dragging = click.kind() == mouse::click::Kind::Single;
                state.pressed = Some(mouse::Button::Left);
                state.focus = Some(Focus::now());
                self.moved(&inner, shell);
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(at) = cursor.position_over(bounds) {
                    state.focus = Some(Focus::now());
                    shell.publish((self.on)(Event::Menu(at)));
                    shell.capture_event();
                }
                return;
            }
            // A middle press takes the keys, and no autoscroll starts
            // under it; its release pastes the primary selection there.
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if cursor.is_over(bounds) {
                    state.focus = Some(Focus::now());
                    state.pressed = Some(mouse::Button::Middle);
                    shell.capture_event();
                    shell.request_redraw();
                }
                return;
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.pressed.take() != Some(mouse::Button::Middle) {
                    return;
                }
                let Some(at) = cursor.position_in(text_bounds.expand(self.pad)) else {
                    return;
                };
                let mut inner = self.draft.0.borrow_mut();
                let at = at - Vector::new(self.pad.left - inner.scroll_x, self.pad.top);
                inner.drop_preedit();
                inner.editor.perform(Action::Click(at));
                match clipboard.read(clipboard::Kind::Primary) {
                    Some(pasted) => {
                        let pasted = pasted_into(pasted, self.single);
                        inner.remember(Kind::Other);
                        inner
                            .editor
                            .perform(Action::Edit(Edit::Paste(Arc::new(pasted))));
                        self.edited(&inner, shell);
                    }
                    None => self.moved(&inner, shell),
                }
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            // Text selected with the pointer becomes the primary
            // selection, as the desktop's other fields do.
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                if state.pressed.take() == Some(mouse::Button::Left)
                    && let Some(selection) = self.draft.0.borrow().editor.copy()
                {
                    clipboard.write(clipboard::Kind::Primary, selection);
                }
                return;
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(at) = cursor.position() {
                    let mut inner = self.draft.0.borrow_mut();
                    let at = at - (self.origin(&inner, text_bounds) - Point::ORIGIN);
                    inner.editor.perform(Action::Drag(at));
                    self.moved(&inner, shell);
                    shell.request_redraw();
                }
                return;
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let mut inner = self.draft.0.borrow_mut();
                let height = inner.editor.min_bounds().height;
                if height + 1.0 >= self.lines * self.line {
                    let lines = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y * 3.0,
                        mouse::ScrollDelta::Pixels { y, .. } => -y / self.line,
                    };
                    inner.editor.perform(Action::Scroll {
                        lines: lines.round() as i32,
                    });
                    shell.capture_event();
                    shell.request_redraw();
                }
                return;
            }
            _ => {}
        }
        let Some(focus) = &mut state.focus else {
            return;
        };
        match event {
            iced::Event::InputMethod(event) => {
                let mut inner = self.draft.0.borrow_mut();
                match event {
                    input_method::Event::Opened => {}
                    input_method::Event::Closed => inner.drop_preedit(),
                    input_method::Event::Preedit(content, selection) => {
                        inner.drop_preedit();
                        if !content.is_empty() {
                            if inner.editor.copy().is_some() {
                                inner.remember(Kind::Delete);
                                inner.editor.perform(Action::Edit(Edit::Delete));
                                self.edited(&inner, shell);
                            }
                            let start = inner.editor.cursor().position;
                            inner
                                .editor
                                .perform(Action::Edit(Edit::Paste(Arc::new(content.clone()))));
                            let end = inner.editor.cursor().position;
                            if let Some(selection) = selection {
                                let column = start.column + selection.end.min(content.len());
                                inner.editor.move_to(Cursor {
                                    position: Position { column, ..start },
                                    selection: None,
                                });
                            }
                            inner.preedit = Some((start, end));
                        }
                    }
                    input_method::Event::Commit(content) => {
                        inner.drop_preedit();
                        inner.remember(Kind::Other);
                        inner
                            .editor
                            .perform(Action::Edit(Edit::Paste(Arc::new(content.clone()))));
                        self.edited(&inner, shell);
                    }
                }
                focus.updated_at = Instant::now();
                shell.capture_event();
                // A preedit publishes nothing, so nothing else lays the
                // text out again before it's drawn (iced's editor needs its
                // lines laid out to place the caret).
                shell.invalidate_layout();
                shell.request_redraw();
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                let mut inner = self.draft.0.borrow_mut();
                // The input method has the keys while it composes.
                if inner.preedit.is_some() {
                    shell.capture_event();
                    return;
                }
                let command = modifiers.command();
                let named = match modified_key {
                    keyboard::Key::Named(named) => Some(*named),
                    _ => None,
                };
                let publish = |shell: &mut Shell<'_, M>, event| {
                    shell.publish((self.on)(event));
                    shell.capture_event();
                };
                if self.suggesting {
                    match named {
                        Some(key::Named::ArrowUp) => return publish(shell, Event::Step(-1)),
                        Some(key::Named::ArrowDown) => return publish(shell, Event::Step(1)),
                        Some(key::Named::Enter | key::Named::Tab) if !modifiers.shift() => {
                            return publish(shell, Event::Choose);
                        }
                        Some(key::Named::Escape) => return publish(shell, Event::Escape),
                        _ => {}
                    }
                }
                let latin = key.to_latin(*physical_key);
                let edit = |inner: &mut Inner, kind: Kind, action: Action| {
                    inner.remember(kind);
                    inner.editor.perform(action);
                };
                match (named, latin) {
                    (Some(key::Named::Enter), _) => {
                        if (modifiers.shift() || self.breaks) && !self.single {
                            edit(&mut inner, Kind::Other, Action::Edit(Edit::Enter));
                            self.edited(&inner, shell);
                        } else if !command && !modifiers.alt() {
                            return publish(shell, Event::Send);
                        }
                    }
                    (Some(key::Named::Escape), _) => return publish(shell, Event::Escape),
                    (Some(key::Named::Tab), _) if self.tabs => {
                        return publish(shell, Event::Tab(modifiers.shift()));
                    }
                    (Some(key::Named::Tab), _) => return,
                    (Some(key::Named::Backspace), _) if self.tabs && inner.editor.is_empty() => {
                        return publish(shell, Event::BackFromEmpty);
                    }
                    (Some(key::Named::ArrowUp), _) if modifiers.alt() && !self.single => {
                        return publish(shell, Event::UpToMessages);
                    }
                    (_, Some('v')) if command && !modifiers.alt() && self.pastes => {
                        return publish(shell, Event::Paste);
                    }
                    (_, Some('g')) if command && !modifiers.shift() && !self.single => {
                        return publish(shell, Event::Gifs);
                    }
                    (_, Some('c')) if command => {
                        // Nothing selected here: the timeline's selection may be.
                        let Some(selection) = inner.editor.copy() else {
                            return;
                        };
                        clipboard.write(clipboard::Kind::Standard, selection);
                    }
                    (_, Some('x')) if command => {
                        if let Some(selection) = inner.editor.copy() {
                            clipboard.write(clipboard::Kind::Standard, selection);
                            edit(&mut inner, Kind::Other, Action::Edit(Edit::Delete));
                            self.edited(&inner, shell);
                        }
                    }
                    (_, Some('v')) if command && !modifiers.alt() => {
                        if let Some(pasted) = clipboard.read(clipboard::Kind::Standard) {
                            let pasted = pasted_into(pasted, self.single);
                            edit(
                                &mut inner,
                                Kind::Other,
                                Action::Edit(Edit::Paste(Arc::new(pasted))),
                            );
                            self.edited(&inner, shell);
                        }
                    }
                    (_, Some('a')) if command => {
                        inner.editor.perform(Action::SelectAll);
                        self.moved(&inner, shell);
                    }
                    (_, Some('z')) if command => {
                        if inner.restore(!modifiers.shift()) {
                            self.edited(&inner, shell);
                        }
                    }
                    (_, Some('y')) if command => {
                        if inner.restore(false) {
                            self.edited(&inner, shell);
                        }
                    }
                    (_, Some('e')) if command && !modifiers.shift() => {
                        return publish(shell, Event::Emoji);
                    }
                    (Some(key::Named::Backspace), _)
                        if !command && self.take_atom(&mut inner, true) =>
                    {
                        self.edited(&inner, shell);
                    }
                    (Some(key::Named::Delete), _)
                        if !command && self.take_atom(&mut inner, false) =>
                    {
                        self.edited(&inner, shell);
                    }
                    (Some(key::Named::Backspace), _) => {
                        if command && inner.editor.copy().is_none() {
                            inner.editor.perform(Action::Select(Motion::WordLeft));
                        }
                        edit(&mut inner, Kind::Delete, Action::Edit(Edit::Backspace));
                        self.edited(&inner, shell);
                    }
                    (Some(key::Named::Delete), _) => {
                        if command && inner.editor.copy().is_none() {
                            inner.editor.perform(Action::Select(Motion::WordRight));
                        }
                        edit(&mut inner, Kind::Delete, Action::Edit(Edit::Delete));
                        self.edited(&inner, shell);
                    }
                    (Some(key::Named::ArrowUp), _)
                        if inner.editor.is_empty() && !modifiers.shift() && !command =>
                    {
                        return publish(shell, Event::UpFromEmpty);
                    }
                    (Some(named), _) if motion(named).is_some() => {
                        let motion = motion(named).expect("checked");
                        let motion = if command { motion.widen() } else { motion };
                        inner.editor.perform(if modifiers.shift() {
                            Action::Select(motion)
                        } else {
                            Action::Move(motion)
                        });
                        self.moved(&inner, shell);
                    }
                    _ => {
                        let Some(text) = text.as_ref().filter(|_| !command || modifiers.alt())
                        else {
                            return;
                        };
                        let typed: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
                        if typed.is_empty() {
                            return;
                        }
                        // A space after a word ends a step, as a word is undone whole.
                        let kind = Kind::Insert;
                        if typed.iter().any(|c| c.is_whitespace()) {
                            inner.last = None;
                        }
                        for c in typed {
                            edit(&mut inner, kind, Action::Edit(Edit::Insert(c)));
                        }
                        self.edited(&inner, shell);
                    }
                }
                focus.updated_at = Instant::now();
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let text_bounds = self.text_bounds(layout);
        let border = match self.ring.filter(|_| state.focus.is_some()) {
            Some(ring) => crate::style::edge(ring, self.radius),
            None => Border::default().rounded(self.radius),
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border,
                ..Default::default()
            },
            self.colors.ground,
        );
        let mut inner = self.draft.0.borrow_mut();
        self.follow_caret(&mut inner, text_bounds.width);
        let inner = inner;
        if inner.editor.is_empty() && inner.preedit.is_none() {
            renderer.fill_text(
                text::Text {
                    content: self.placeholder.clone(),
                    bounds: text_bounds.size(),
                    size: self.size.into(),
                    line_height: LineHeight::Absolute(self.line.into()),
                    font: self.font,
                    align_x: text::Alignment::Default,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: Wrapping::None,
                },
                text_bounds.position(),
                self.colors.placeholder,
                text_bounds,
            );
        }
        let origin = self.origin(&inner, text_bounds);
        let translation = origin - Point::ORIGIN;
        if let Some(inks) = self.pill_inks {
            for pill in self.pill_boxes(&inner, origin) {
                if let Some(pill) = bounds.intersection(&pill) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: pill,
                            border: Border {
                                color: inks.line,
                                width: 1.0,
                                radius: PILL_RADIUS.into(),
                            },
                            ..Default::default()
                        },
                        inks.ground,
                    );
                }
            }
        }
        // The selection under the text.
        if state.focus.is_some()
            && let Selection::Range(ranges) = inner.editor.selection()
        {
            for range in ranges
                .into_iter()
                .filter_map(|range| text_bounds.intersection(&(range + translation)))
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: range,
                        ..Default::default()
                    },
                    self.colors.selection,
                );
            }
        }
        if !inner.editor.is_empty() {
            renderer.fill_editor(&inner.editor, origin, self.colors.text, text_bounds);
        }
        // The preedit, underlined where it sits in the text.
        if let Some((start, end)) = inner.preedit {
            let from = cosmic_text::Cursor::new(start.line, start.column);
            let to = cosmic_text::Cursor::new(end.line, end.column);
            for run in inner
                .editor
                .buffer()
                .layout_runs()
                .filter(|run| run.line_i == start.line)
            {
                if let Some((x, width)) = run.highlight(from, to) {
                    let line = Rectangle::new(
                        Point::new(origin.x + x, origin.y + run.line_y + 3.0),
                        Size::new(width, 1.0),
                    );
                    if let Some(line) = text_bounds.expand(4.0).intersection(&line) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: line,
                                ..Default::default()
                            },
                            self.colors.text,
                        );
                    }
                }
            }
        }
        if let Some(focus) = &state.focus
            && focus.caret_shown()
            && let Selection::Caret(point) = inner.editor.selection()
        {
            let caret = Rectangle::new(point + translation, Size::new(1.0, self.line));
            if let Some(caret) = text_bounds.expand(1.0).intersection(&caret) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: caret,
                        ..Default::default()
                    },
                    self.colors.text,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Whether `range` is a run of `text`, on its characters' edges.
fn fits(text: &str, range: &Range<usize>) -> bool {
    range.start < range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

/// Marks the pills' keys for the editor's semibold, from the first line on:
/// runs by byte offsets in the whole text, cut to each line.
struct Bold {
    runs: Vec<Range<usize>>,
    line: usize,
    /// Where the next line starts in the whole text.
    start: usize,
}

impl Highlighter for Bold {
    type Settings = Vec<Range<usize>>;
    type Highlight = ();
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, ())>;

    fn new(runs: &Self::Settings) -> Self {
        Self {
            runs: runs.clone(),
            line: 0,
            start: 0,
        }
    }

    fn update(&mut self, runs: &Self::Settings) {
        *self = Self::new(runs);
    }

    fn change_line(&mut self, _line: usize) {
        self.line = 0;
        self.start = 0;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let (start, end) = (self.start, self.start + line.len());
        self.line += 1;
        self.start = end + 1;
        self.runs
            .iter()
            .filter(|run| run.start < end && run.end > start)
            .map(|run| (run.start.max(start) - start..run.end.min(end) - start, ()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

fn motion(key: key::Named) -> Option<Motion> {
    Some(match key {
        key::Named::ArrowLeft => Motion::Left,
        key::Named::ArrowRight => Motion::Right,
        key::Named::ArrowUp => Motion::Up,
        key::Named::ArrowDown => Motion::Down,
        key::Named::Home => Motion::Home,
        key::Named::End => Motion::End,
        key::Named::PageUp => Motion::PageUp,
        key::Named::PageDown => Motion::PageDown,
        _ => return None,
    })
}

impl<'a, M: Clone + 'a> From<Field<'a, M>> for Element<'a, M> {
    fn from(field: Field<'a, M>) -> Self {
        Element::new(field)
    }
}

#[cfg(test)]
mod paste_tests {
    use super::pasted_into;

    #[test]
    fn a_one_line_field_takes_a_paste_on_one_line() {
        assert_eq!(pasted_into("foo\nbar\r\nbaz".into(), true), "foo bar baz");
        assert_eq!(pasted_into("foo\nbar".into(), false), "foo\nbar");
    }
}

#[cfg(test)]
mod pointer_tests {
    use iced::{
        Color, Event, Font, Pixels, Point, Size,
        advanced::{Clipboard, clipboard, renderer::Headless as _, widget::Id},
        keyboard, mouse,
        widget::container,
    };
    use iced_test::runtime::{UserInterface, user_interface::Cache};

    use super::{Colors, Draft, Field, Pill, PillInks};
    use crate::testing::{Message, install_fonts};

    /// The desktop's clipboards, as a field reads and writes them.
    #[derive(Default)]
    struct Clipboards {
        standard: Option<String>,
        primary: Option<String>,
    }

    impl Clipboard for Clipboards {
        fn read(&self, kind: clipboard::Kind) -> Option<String> {
            match kind {
                clipboard::Kind::Standard => self.standard.clone(),
                clipboard::Kind::Primary => self.primary.clone(),
            }
        }

        fn write(&mut self, kind: clipboard::Kind, contents: String) {
            match kind {
                clipboard::Kind::Standard => self.standard = Some(contents),
                clipboard::Kind::Primary => self.primary = Some(contents),
            }
        }
    }

    /// A field holding `draft` in a 400 by 100 window, and `events` given
    /// to it with the pointer at each `Point` before its events.
    fn run(draft: &Draft, clipboards: &mut Clipboards, steps: &[(Point, Vec<Event>)]) {
        run_fields(&[draft], clipboards, steps);
    }

    /// The same of a field for each of `drafts`, one under the other, each
    /// 50 px of the window.
    fn run_fields(drafts: &[&Draft], clipboards: &mut Clipboards, steps: &[(Point, Vec<Event>)]) {
        install_fonts();
        let mut renderer = iced_test::futures::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            Pixels(16.0),
            None,
        ))
        .expect("a headless renderer");
        let colors = Colors {
            ground: Color::BLACK,
            text: Color::WHITE,
            placeholder: Color::WHITE,
            selection: Color::WHITE,
        };
        let fields = drafts.iter().map(|draft| {
            let field = Field::new(
                draft,
                Id::new("field"),
                String::new(),
                |_| Message::Nothing,
                colors,
            );
            container(field).width(400).height(50).into()
        });
        let mut ui = UserInterface::build(
            iced::widget::column(fields),
            Size::new(400.0, 100.0),
            Cache::default(),
            &mut renderer,
        );
        let mut messages = Vec::new();
        for (at, events) in steps {
            let cursor = mouse::Cursor::Available(*at);
            let mut all = vec![Event::Mouse(mouse::Event::CursorMoved { position: *at })];
            all.extend(events.iter().cloned());
            let _ = ui.update(&all, cursor, &mut renderer, clipboards, &mut messages);
        }
    }

    /// A key press that writes `letter`, or a named key's.
    fn key(key: keyboard::Key, text: Option<&str>) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            repeat: false,
            text: text.map(Into::into),
        })
    }

    #[test]
    fn backspace_and_delete_take_an_atom_whole_and_pills_keep_the_text() {
        install_fonts();
        let mut renderer = iced_test::futures::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            Pixels(16.0),
            None,
        ))
        .expect("a headless renderer");
        let draft = Draft::default();
        draft.reset("hi has: image");
        let colors = Colors {
            ground: Color::BLACK,
            text: Color::WHITE,
            placeholder: Color::WHITE,
            selection: Color::WHITE,
        };
        let backspace = keyboard::Key::Named(keyboard::key::Named::Backspace);
        let delete = keyboard::Key::Named(keyboard::key::Named::Delete);
        let home = keyboard::Key::Named(keyboard::key::Named::Home);
        let right = keyboard::Key::Named(keyboard::key::Named::ArrowRight);
        let left = mouse::Button::Left;
        let click = || [press(left), release(left)];
        // Each step's UI is new, so a click gives it the keys again; the
        // last starts over and deletes from inside the key.
        let steps: [(Option<&str>, Vec<Event>); 3] = [
            (
                None,
                [click().as_slice(), &[key(backspace.clone(), None)]].concat(),
            ),
            (None, [click().as_slice(), &[key(backspace, None)]].concat()),
            (
                Some("hi has: image"),
                [
                    click().as_slice(),
                    &[
                        key(home, None),
                        key(right.clone(), None),
                        key(right.clone(), None),
                        key(right, None),
                        key(delete, None),
                    ],
                ]
                .concat(),
            ),
        ];
        let texts = ["hi has:", "hi ", "hi  image"];
        let mut clipboards = Clipboards::default();
        for ((reset, events), want) in steps.iter().zip(texts) {
            if let Some(text) = reset {
                draft.reset(text);
            }
            let text = draft.text();
            // The app's pills and atoms for the text as it is.
            let atoms: Vec<_> = [3..7, 7..13]
                .into_iter()
                .filter(|atom| atom.end <= text.len())
                .collect();
            let pills = atoms
                .iter()
                .map(|range| Pill {
                    range: range.clone(),
                    bold: range.start == 3,
                })
                .collect();
            let field = Field::new(
                &draft,
                Id::new("field"),
                String::new(),
                |_| Message::Nothing,
                colors,
            )
            .single()
            .pills(
                pills,
                PillInks {
                    ground: Color::BLACK,
                    line: Color::WHITE,
                },
            )
            .atoms(atoms);
            let mut ui = UserInterface::build(
                container(field).width(400).height(50),
                Size::new(400.0, 100.0),
                Cache::default(),
                &mut renderer,
            );
            let cursor = mouse::Cursor::Available(END);
            let mut all = vec![Event::Mouse(mouse::Event::CursorMoved { position: END })];
            all.extend(events.iter().cloned());
            let _ = ui.update(
                &all,
                cursor,
                &mut renderer,
                &mut clipboards,
                &mut Vec::new(),
            );
            assert_eq!(draft.text(), want);
        }
    }

    #[test]
    fn a_one_line_field_scrolls_sideways_to_the_caret() {
        install_fonts();
        let mut renderer = iced_test::futures::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            Pixels(16.0),
            None,
        ))
        .expect("a headless renderer");
        let draft = Draft::default();
        draft.reset(&"word ".repeat(40));
        let colors = Colors {
            ground: Color::BLACK,
            text: Color::WHITE,
            placeholder: Color::WHITE,
            selection: Color::WHITE,
        };
        let left = mouse::Button::Left;
        let mut scrolls = Vec::new();
        for named in [keyboard::key::Named::End, keyboard::key::Named::Home] {
            let field = Field::new(
                &draft,
                Id::new("field"),
                String::new(),
                |_| Message::Nothing,
                colors,
            )
            .single();
            let mut ui = UserInterface::build(
                container(field).width(400).height(50),
                Size::new(400.0, 100.0),
                Cache::default(),
                &mut renderer,
            );
            let events = [
                Event::Mouse(mouse::Event::CursorMoved { position: START }),
                press(left),
                release(left),
                key(keyboard::Key::Named(named), None),
            ];
            let _ = ui.update(
                &events,
                mouse::Cursor::Available(START),
                &mut renderer,
                &mut Clipboards::default(),
                &mut Vec::new(),
            );
            // Laid out again, as the app's view is after the message.
            let field = Field::new(
                &draft,
                Id::new("field"),
                String::new(),
                |_| Message::Nothing,
                colors,
            )
            .single();
            let _ = UserInterface::build(
                container(field).width(400).height(50),
                Size::new(400.0, 100.0),
                ui.into_cache(),
                &mut renderer,
            );
            scrolls.push(draft.0.borrow().scroll_x);
        }
        assert!(scrolls[0] > 1000.0, "at the end: {scrolls:?}");
        assert_eq!(scrolls[1], 0.0, "back at the start");
    }

    fn press(button: mouse::Button) -> Event {
        Event::Mouse(mouse::Event::ButtonPressed(button))
    }

    fn release(button: mouse::Button) -> Event {
        Event::Mouse(mouse::Event::ButtonReleased(button))
    }

    fn shift(held: bool) -> Event {
        Event::Keyboard(keyboard::Event::ModifiersChanged(if held {
            keyboard::Modifiers::SHIFT
        } else {
            keyboard::Modifiers::empty()
        }))
    }

    /// The text's first line, from its start to well past its end.
    const START: Point = Point::new(7.0, 24.0);
    const END: Point = Point::new(380.0, 24.0);

    #[test]
    fn a_shift_click_extends_the_selection_which_becomes_the_primary_one() {
        let draft = Draft::default();
        draft.reset("hello world");
        let mut clipboards = Clipboards::default();
        let left = mouse::Button::Left;
        run(
            &draft,
            &mut clipboards,
            &[
                (START, vec![press(left), release(left)]),
                (
                    END,
                    vec![shift(true), press(left), release(left), shift(false)],
                ),
            ],
        );
        assert_eq!(draft.selection().as_deref(), Some("hello world"));
        assert_eq!(clipboards.primary.as_deref(), Some("hello world"));
        assert_eq!(clipboards.standard, None, "the clipboard is left alone");
    }

    #[test]
    fn a_middle_click_pastes_the_primary_selection_at_the_pointer() {
        let draft = Draft::default();
        draft.reset("world");
        let mut clipboards = Clipboards {
            primary: Some("hello ".into()),
            standard: Some("not this".into()),
        };
        let middle = mouse::Button::Middle;
        run(
            &draft,
            &mut clipboards,
            &[(START, vec![press(middle), release(middle)])],
        );
        assert_eq!(draft.text(), "hello world");
    }

    #[test]
    fn a_middle_or_right_press_on_another_field_takes_the_keys_from_the_first() {
        let key = |letter: &str| {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(letter.into()),
                modified_key: keyboard::Key::Character(letter.into()),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                repeat: false,
                text: Some(letter.into()),
            })
        };
        let second = Point::new(START.x, START.y + 50.0);
        let left = mouse::Button::Left;
        for button in [mouse::Button::Middle, mouse::Button::Right] {
            let (above, below) = (Draft::default(), Draft::default());
            run_fields(
                &[&above, &below],
                &mut Clipboards::default(),
                &[
                    (START, vec![press(left), release(left), key("a")]),
                    (second, vec![press(button), release(button)]),
                    (second, vec![key("b")]),
                ],
            );
            assert_eq!(above.text(), "a", "{button:?}");
            assert_eq!(below.text(), "b", "{button:?}");
        }
    }
}
