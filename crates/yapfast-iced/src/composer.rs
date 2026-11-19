//! The composer under the timeline: the message request's answer in its
//! place, the reply's bar over it (with whether the reply mentions its
//! author), the field (`field::Field`) between the + and the emoji button,
//! suggestions over it (`suggest.rs`), and the hint under it. The draft is
//! the core's, per channel (`Action::SetDraft`), which tells the room this
//! account is typing; the field follows it when the core changes it, as
//! when a sent message empties it. A message of one's own is edited in its
//! row (`edit.rs`), in a field of its own beside the draft.

use iced::{
    Alignment, Element, Length, Padding, Task, keyboard,
    widget::{Id, Space, button, column, container, operation, row, text, tooltip},
};
use yapfast_core::{
    application::{Action, Compose},
    network::Network,
};

use crate::{
    app::{App, Message, NOBODY},
    controls::{self, lit_style},
    field::{self, Field, MenuEdit},
    fonts::BOLD,
    menus::{Menu, MenuAct, MenuRow},
    picker::{Tab, Target},
    pictures::Icon,
    popover::{Popover, Side},
    style::{corners, faded, rounded},
};

/// The composer's field and the inline edit's, for the app's focus tasks.
pub const COMPOSER: &str = "composer";
pub const EDIT: &str = "edit";
/// The reply bar's height.
pub const BAR: f32 = 36.0;
/// The composer's least height (`ClientStyle.composer-height`), the room
/// over and under its text, beside it, and under its buttons.
pub const HEIGHT: f32 = 58.0;
pub const PAD: f32 = 17.0;
pub const GUTTER: f32 = 8.0;
pub const BUTTON_INSET: f32 = 13.0;
/// The hint's line under the composer.
const HINT: f32 = 22.0;

/// Which field is written in: the room's draft, or a message's edit.
/// The composer, the edit in its row and the suggestions over them.
#[derive(Default)]
pub struct Composer {
    /// The composer's text and history: the core's draft for the open room.
    pub draft: crate::field::Draft,
    /// The draft the core last had, to tell its own changes from ours.
    pub draft_known: String,
    /// Whether the composer's field has the keys, which its outline shows.
    pub focused: bool,
    /// The message being edited in its row (the core's `edit`), and the
    /// edit's own field.
    pub editing: Option<String>,
    pub edit: crate::field::Draft,
    /// The suggestions over the field typed in last.
    pub suggest: crate::suggest::Suggest,
    /// The composer written in last, the channel's or the thread's.
    pub writing_last: Writing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Writing {
    #[default]
    Draft,
    Edit,
    /// The thread beside the channel's composer, the New Thread form's
    /// first message, and a new forum post's (`thread.rs`, `forum.rs`).
    Aside,
    NewThread,
    NewPost,
}

/// What the composer's parts send.
#[derive(Clone, Debug)]
pub enum Event {
    /// A field's keys and text.
    Field(Writing, field::Event),
    /// The clipboard's text, for a field's paste.
    Pasted(Writing, Option<String>),
    /// A suggestion clicked, by its place.
    Suggestion(usize),
    CancelReply,
    TogglePing,
    /// The message request's Accept (true) or Ignore.
    Answer(bool),
    /// The +.
    Plus,
    /// The edit's hint: cancel or save; its emoji button.
    CancelEdit,
    SaveEdit,
    EditEmoji,
    /// A message of one's own that didn't go: send it again, or not; an
    /// answer only this account sees, dismissed.
    Retry(String),
    CancelSend(String),
    Dismiss(String),
    /// Files to go with the message.
    Uploads(crate::uploads::Event),
    /// An app command's pill clicked: its option, or a file option's file;
    /// the file chosen.
    CommandOption(usize),
    CommandFile(usize),
    CommandFileChosen(usize, String),
    /// The commands' list scrolled to this far down.
    CommandsScrolled(f32),
}

/// What the composer shows for the open room, from the core's state.
pub struct Look {
    pub enabled: bool,
    /// Discord's one bar in place of the composer, where this account may
    /// not write.
    pub locked: bool,
    /// The line under it: why it can't send, or nothing; with Matrix,
    /// whether the room is end-to-end encrypted, under a lock if it is.
    pub hint: String,
    pub encrypted: bool,
    /// A message request's title, answered in place of the composer.
    pub request: Option<String>,
    pub attach: bool,
    /// The + opens a menu (threads or polls can start here).
    pub plus: bool,
}

/// What the reply bar over a composer shows: whom it replies to, the first
/// line's text and custom emoji, and whether it mentions them.
pub struct ReplyBar<'a> {
    pub author: &'a str,
    pub body: &'a str,
    pub emoji: &'a [yapfast_core::chat::CustomEmoji],
    pub ping: bool,
}

/// A reply's bar over a composer: its author, first line, custom emoji, and
/// whether it mentions them.
pub fn reply_bar_info(compose: Option<&Compose>) -> Option<ReplyBar<'_>> {
    let Compose::Reply {
        author,
        body,
        emoji,
        ping,
        ..
    } = compose?;
    Some(ReplyBar {
        author,
        body: body.lines().next().unwrap_or_default(),
        emoji,
        ping: *ping,
    })
}

/// The composer box's top in the window, where what opens over it sits.
/// The top of the composer's buttons, which stay on its last line:
/// the expression picker opens 8 px over them.
pub fn composer_buttons_top(app: &App) -> f32 {
    app.window.height - composer_foot(app) - BUTTON_INSET - 32.0
}

/// The room under the composer's box: its hint, or the gutter.
fn composer_foot(app: &App) -> f32 {
    if composer_look(app.ctx()).hint.is_empty() {
        GUTTER
    } else {
        HINT
    }
}

pub fn composer_top(app: &App) -> f32 {
    let foot = composer_foot(app);
    let tray = if app.client.state().staged().is_empty() {
        0.0
    } else {
        crate::uploads::TRAY
    };
    app.window.height - foot - app.composer.draft.height().max(HEIGHT) - tray
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Field(which, event) => field_event(app, which, event),
        Event::Pasted(which, pasted) => {
            if pasted.is_some() {
                let draft = writing(app.ctx(), which);
                draft.act(MenuEdit::Paste, pasted);
                let (text, caret) = (draft.text(), draft.caret());
                edited(app, which, text, caret);
            }
            Task::none()
        }
        Event::Suggestion(index) => {
            // The keys go back to the field the suggestion was for: a
            // side composer's, the form's, the edit's or the channel's.
            let which = app.composer.suggest.field;
            app.composer.suggest.set_index(index);
            Task::batch([
                crate::suggest::choose_suggestion(app, which),
                operation::focus(crate::thread::field_id(which)),
            ])
        }
        Event::CancelReply => {
            app.client.dispatch(Action::CancelCompose);
            Task::batch([app.sync(), refocus(app)])
        }
        Event::TogglePing => {
            app.client
                .dispatch(Action::ToggleReplyPing { aside: false });
            Task::batch([app.sync(), refocus(app)])
        }
        Event::Answer(accept) => {
            app.client.dispatch(Action::AnswerRequest(accept));
            Task::batch([app.sync(), refocus(app)])
        }
        Event::Plus => plus(app),
        Event::CancelEdit => {
            app.client.dispatch(Action::CancelEdit);
            Task::batch([app.sync(), operation::focus(COMPOSER)])
        }
        Event::SaveEdit => crate::edit::save_edit(app),
        Event::EditEmoji => crate::picker::open_picker(app, Target::Edit, Tab::Emoji),
        Event::Retry(item) => {
            app.client.dispatch(Action::Retry(item));
            app.sync()
        }
        Event::CancelSend(item) => {
            app.client.dispatch(Action::CancelSend(item));
            app.sync()
        }
        Event::Dismiss(item) => {
            app.client.dispatch(Action::DismissEphemeral(item));
            app.sync()
        }
        Event::Uploads(event) => crate::uploads::update(app, event),
        Event::CommandOption(index) => {
            Task::batch([crate::commands::focus_option(app, index), refocus(app)])
        }
        Event::CommandFile(index) => crate::commands::command_file(index),
        Event::CommandFileChosen(index, value) => {
            app.client
                .dispatch(Action::SetCommandOption { index, value });
            app.sync()
        }
        Event::CommandsScrolled(offset) => {
            app.composer.suggest.command_offset = offset;
            Task::none()
        }
    }
}

/// The keys go back to the field being written in: the edit while one
/// is open, else the composer; not while a server's or a channel's
/// settings cover it, as after a member's profile from Members.
pub fn refocus(app: &App) -> Task<Message> {
    // A screen or dialog over the window keeps the keys itself.
    if app.server_screen.is_open()
        || app.channel_screen.is_open()
        || crate::server_dialogs::is_open(app.client.state())
        || crate::link_dialogs::is_open(app.client.state())
    {
        return Task::none();
    }
    // The picture viewer and a video full screen keep them from the
    // fields under them.
    if app.client.state().viewer.is_some() || app.playback.clip_full {
        return operation::focus(crate::app::NOBODY);
    }
    // A popover's field, while it's open.
    if app.client.state().thread_list().is_some() {
        return operation::focus(crate::header_popovers::THREADS_SEARCH);
    }
    if crate::header_popovers::popover_open(app) {
        return operation::focus(crate::header_popovers::PEOPLE_FIELD);
    }
    operation::focus(if app.composer.editing.is_some() {
        EDIT
    } else if writing_aside(app) {
        crate::thread::COMPOSER
    } else if app.client.state().new_post().is_some() {
        // A forum has no composer: its New Post's body has the keys.
        crate::forum::BODY
    } else {
        COMPOSER
    })
}

/// A key nothing took: one that writes goes on the end of the composer
/// written in last (the thread beside's or the channel's), which takes the
/// keys, as Discord's window sends typing anywhere to its composer; Tab
/// only gives it the keys. Ctrl+V pastes there as the composer's own does,
/// at its caret, as Discord's window hands a paste with no field focused
/// to its composer (`GLOBAL_CLIPBOARD_PASTE`). Not under a card or a
/// popup, nor push to talk's own key in a call; stepping through the
/// messages, only letters, digits and Ctrl+V leave them.
pub fn type_anywhere(
    app: &mut App,
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Task<Message> {
    let typed = field::Typed::of(key, modifiers);
    let tab = *key == keyboard::Key::Named(keyboard::key::Named::Tab)
        && !modifiers.command()
        && !modifiers.alt();
    // The key as the layout has it unmodified, so Shift or Caps Lock too.
    let pastes =
        modifiers.command() && !modifiers.alt() && key.as_ref() == keyboard::Key::Character("v");
    if (typed.is_none() && !tab && !pastes) || composer_covered(app) {
        return Task::none();
    }
    if app.keyed.is_some() {
        let letters = matches!(&typed, Some(field::Typed::Text(text))
            if text.chars().all(char::is_alphanumeric));
        if !letters && !pastes {
            return Task::none();
        }
        crate::timeline::keys::leave_messages(app);
    }
    // Holding the key to talk shouldn't also write it: only that key, by
    // where it is, while it sends.
    let voice = &app.client.state().voice;
    if app.talk_pressed && voice.active() && !voice.muted {
        return Task::none();
    }
    let (which, field) = if writing_aside(app) {
        let Some(aside) = &app.aside else {
            return Task::none();
        };
        if !app.client.state().can_write_in(&aside.room) {
            return Task::none();
        }
        (Writing::Aside, crate::thread::COMPOSER)
    } else {
        if !composer_look(app.ctx()).enabled || app.composer.editing.is_some() {
            return Task::none();
        }
        (Writing::Draft, COMPOSER)
    };
    if pastes {
        return Task::batch([paste(app, which), operation::focus(field)]);
    }
    if let Some(typed) = typed {
        let draft = writing(app.ctx(), which);
        draft.type_at_end(&typed);
        let (text, caret) = (draft.text(), draft.caret());
        edited(app, which, text, caret);
    }
    operation::focus(field)
}

/// Something over the window that keys typed with nothing focused
/// aren't for: a card, a popup, the picker or a full view.
fn composer_covered(app: &App) -> bool {
    let state = app.client.state();
    app.profiles.card.is_some()
        || app.profiles.shown.is_some()
        || app.cards.server.is_some()
        || app.cards.expression.is_some()
        || state.role.is_some()
        || app.win.picker.is_open()
        || app.call.popup.is_some()
        || app.playback.clip_full
        || crate::call::stage::full_shown(app)
        || app.inbox.is_some()
        || state.select_menu.is_some()
        || state.forwarding.is_some()
        || state.viewer.is_some()
}

/// The thread beside the channel's composer was written in last, and
/// shows: keys typed anywhere go to it, and Escape cancels its reply.
pub fn writing_aside(app: &App) -> bool {
    app.composer.writing_last == Writing::Aside
        && app.aside.is_some()
        && crate::thread::aside_shown(app)
}

fn field_event(app: &mut App, which: Writing, event: field::Event) -> Task<Message> {
    // The keys come back to the composer written in last: the thread's
    // or the channel's.
    if matches!(which, Writing::Draft | Writing::Aside) {
        app.composer.writing_last = which;
    }
    match event {
        field::Event::Edited(text, caret) => {
            edited(app, which, text, caret);
            Task::none()
        }
        field::Event::Moved(caret) => {
            let text = writing(app.ctx(), which).text();
            crate::suggest::caret_moved(app, which, &text, caret);
            Task::none()
        }
        field::Event::Send => match which {
            Writing::Draft => send(app),
            Writing::Edit => crate::edit::save_edit(app),
            Writing::Aside => crate::thread::update(app, crate::thread::Event::Send),
            Writing::NewThread => crate::thread::update(app, crate::thread::Event::Create),
            Writing::NewPost => crate::forum::update(app, crate::forum::Event::Create),
        },
        field::Event::Step(step) => {
            let count = crate::suggest::suggestion_count(app);
            app.composer.suggest.step(step, count);
            if which == Writing::Draft && crate::commands::command_list(app.ctx()).is_some() {
                let lit = app.composer.suggest.lit(count);
                return crate::commands::keep_lit_in_view(app, lit, count);
            }
            Task::none()
        }
        field::Event::Choose => crate::suggest::choose_suggestion(app, which),
        field::Event::Escape => {
            if crate::suggest::dismiss_suggestions(app) {
                return Task::none();
            }
            // With no list over it, Escape stops writing a command and
            // empties the composer.
            if which == Writing::Draft && crate::commands::close_command(app, false) {
                return app.sync();
            }
            // Under a server's or a channel's settings, Escape is
            // theirs: it comes to them as a key.
            if app.server_screen.is_open() || app.channel_screen.is_open() {
                return Task::none();
            }
            // Escape in the thread's composer cancels its reply first.
            if which == Writing::Aside && app.client.state().aside_compose().is_some() {
                return crate::thread::update(app, crate::thread::Event::CancelReply);
            }
            if matches!(
                which,
                Writing::Aside | Writing::NewThread | Writing::NewPost
            ) {
                return app.escape();
            }
            // Escape in the edit's field ends the edit.
            if which == Writing::Edit {
                app.client.dispatch(Action::CancelEdit);
                return Task::batch([app.sync(), operation::focus(COMPOSER)]);
            }
            app.escape()
        }
        field::Event::Emoji => match which {
            Writing::Draft => crate::picker::open_picker(app, Target::Composer, Tab::Emoji),
            Writing::Edit => crate::picker::open_picker(app, Target::Edit, Tab::Emoji),
            _ => Task::none(),
        },
        // Ctrl+G opens the GIFs, as Discord's; again, it closes them.
        field::Event::Gifs if which == Writing::Draft && app.client.state().can_pick_gifs() => {
            crate::picker::open_picker(app, Target::Composer, Tab::Gifs)
        }
        field::Event::Gifs => Task::none(),
        field::Event::Menu(at) => {
            let row = MenuRow::new;
            let mut rows = Vec::new();
            if writing(app.ctx(), which).selection().is_some() {
                rows.push(row(
                    MenuAct::Field(which, MenuEdit::Cut),
                    "Cut",
                    Icon::Scissors,
                ));
                rows.push(row(
                    MenuAct::Field(which, MenuEdit::Copy),
                    "Copy",
                    Icon::Copy,
                ));
            }
            rows.push(row(
                MenuAct::Field(which, MenuEdit::Paste),
                "Paste",
                Icon::Paste,
            ));
            rows.push(row(
                MenuAct::Field(which, MenuEdit::SelectAll),
                "Select All",
                Icon::TextSelect,
            ));
            app.win.menu = Some(Menu::new(at, rows).width(crate::menus::FIELD_WIDTH));
            operation::focus(NOBODY)
        }
        // Up in an empty composer edits the last message of one's own: the
        // core's rule, which leaves Up to a command being written.
        field::Event::UpFromEmpty => {
            if which != Writing::Draft {
                return Task::none();
            }
            app.client.dispatch(Action::EditLast);
            crate::edit::take_edit(app)
        }
        field::Event::UpToMessages => crate::timeline::keys::step_into_messages(app),
        field::Event::Paste => paste(app, which),
        field::Event::Focus(focused) => {
            if which == Writing::Draft {
                app.composer.focused = focused;
            }
            if focused {
                crate::timeline::keys::leave_messages(app);
            }
            Task::none()
        }
        field::Event::Tab(back) => crate::commands::command_tab(app, back),
        field::Event::BackFromEmpty => {
            if crate::commands::close_command(app, true) {
                return app.sync();
            }
            Task::none()
        }
    }
}

/// Ctrl+V and the menu's Paste: in the composer, files copied in a file
/// manager or a picture go with the message, and otherwise the text is
/// pasted; the edit's field takes text alone. The clipboard is read for
/// files even with the message full, so the core says why they can't go;
/// where files never go (the preview, a channel not to write in), it's
/// the text that is pasted.
fn paste(app: &App, which: Writing) -> Task<Message> {
    use crate::uploads::Event as Uploads;
    let state = app.client.state();
    let full =
        |staged: &[yapfast_core::chat::Staged]| staged.len() >= yapfast_core::chat::MAX_FILES;
    let files_go = match which {
        Writing::Draft => state.can_attach() || full(state.staged()),
        Writing::Aside => state.aside_can_attach() || full(state.aside_staged()),
        _ => false,
    };
    if files_go && let Some(read) = crate::desktop::desktop().paste_reader() {
        return Task::perform(
            async move { tokio::task::spawn_blocking(read).await.ok().flatten() },
            move |files| {
                Message::Write(Event::Uploads(if which == Writing::Aside {
                    Uploads::ClipboardAside(files)
                } else {
                    Uploads::Clipboard(files)
                }))
            },
        );
    }
    iced::clipboard::read().map(move |text| Message::Write(Event::Pasted(which, text)))
}

/// The + opens Discord's menu where threads or polls can start, and the
/// file dialog elsewhere.
fn plus(app: &mut App) -> Task<Message> {
    let look = composer_look(app.ctx());
    if !look.plus {
        return crate::uploads::attach(app);
    }
    let mut rows = vec![MenuRow {
        disabled: !look.attach,
        subtitle: None,
        check: crate::controls::Check::None,
        ..MenuRow::new(MenuAct::Upload, "Upload a File", Icon::File)
    }];
    if app.client.app.can_start_threads() {
        rows.push(MenuRow {
            disabled: false,
            subtitle: None,
            check: crate::controls::Check::None,
            ..MenuRow::new(MenuAct::NewThread, "Create Thread", Icon::Thread)
        });
    }
    if app.client.app.can_create_polls() {
        rows.push(MenuRow {
            disabled: false,
            subtitle: None,
            check: crate::controls::Check::None,
            ..MenuRow::new(MenuAct::NewPoll, "Create Poll", Icon::ChartBar)
        });
    }
    let at = iced::Point::new(
        crate::conversation::chat_left(app) + GUTTER + 10.0,
        composer_top(app) - 8.0,
    );
    app.win.menu = Some(Menu::new(at, rows).anchor(crate::widgets::Anchor::Bottom));
    operation::focus(NOBODY)
}

/// Enter: the draft sent. The core empties the draft once the message is
/// sent; `sync` follows it.
fn send(app: &mut App) -> Task<Message> {
    if app.client.state().selected.is_none() {
        return Task::none();
    }
    if let Some(task) = crate::commands::command_enter(app) {
        return task;
    }
    let commanding = app.client.state().command_draft().is_some();
    if !commanding {
        app.client
            .dispatch(Action::SetDraft(app.composer.draft.text()));
    }
    // Sending goes to the newest message, as Discord does, so the sent
    // one shows; from a window of older messages, once the newest have
    // replaced it.
    let mut to_end = Task::none();
    if app.client.state().can_send() {
        if app.client.state().detached {
            app.timeline.latest_due = true;
        } else {
            to_end = crate::timeline::scroll::go_to_end(app);
        }
    }
    app.client.dispatch(Action::Send);
    app.composer.suggest.clear(&mut app.client);
    Task::batch([to_end, app.sync()])
}

/// A field's menu chose Cut, Copy, Paste or Select All.
pub fn field_menu(app: &mut App, which: Writing, edit: MenuEdit) -> Task<Message> {
    app.win.menu = None;
    let focus = operation::focus(crate::thread::field_id(which));
    if edit == MenuEdit::Paste {
        return Task::batch([focus, paste(app, which)]);
    }
    let draft = writing(app.ctx(), which);
    let copied = draft.act(edit, None);
    if edit == MenuEdit::Cut {
        let (text, caret) = (draft.text(), draft.caret());
        edited(app, which, text, caret);
    }
    match copied {
        Some(copied) => Task::batch([focus, iced::clipboard::write(copied)]),
        None => focus,
    }
}

/// A field's text changed: the core's draft or edit follows, and so do the
/// suggestions. A draft changed tells the room this account is typing (the
/// core's).
pub fn edited(app: &mut App, which: Writing, text: String, caret: usize) {
    keep_text(app, which, text.clone());
    // The core may have kept something else: the field shows that.
    let kept = writing(app.ctx(), which).text();
    let caret = if kept == text {
        caret
    } else {
        writing(app.ctx(), which).caret()
    };
    crate::suggest::caret_moved(app, which, &kept, caret);
}

/// The core's draft or edit takes a field's text.
fn keep_text(app: &mut App, which: Writing, text: String) {
    match which {
        Writing::Edit => app.client.dispatch(Action::SetEditText(text)),
        Writing::Draft if app.client.state().selected.is_some() => {
            app.client.dispatch(Action::SetDraft(text.clone()));
            app.composer.draft_known = app.client.state().draft().to_owned();
            // The core took the text as something else, as a command's
            // name and a space picks it: the field shows what it kept.
            if app.composer.draft_known != text {
                app.composer.draft.reset(&app.composer.draft_known);
            }
        }
        Writing::Draft => {}
        Writing::Aside => {
            app.client.dispatch(Action::SetAsideDraft(text));
            app.drafts.aside_known = app.client.state().aside_draft().to_owned();
        }
        Writing::NewThread => {
            app.client.dispatch(Action::WriteNewThread {
                name: None,
                body: Some(text.clone()),
            });
            app.drafts.first_known = text;
        }
        Writing::NewPost => crate::forum::forum_body(app, text),
    }
}

/// Puts `text` in a field with the caret at `caret`, and in the core's
/// draft or edit with it.
pub fn replace_text(app: &mut App, which: Writing, text: String, caret: usize) {
    writing(app.ctx(), which).replace(&text, caret);
    keep_text(app, which, text);
}

/// An emoji's text goes in at the field's caret.
pub fn insert(app: &mut App, which: Writing, inserted: &str) {
    let draft = writing(app.ctx(), which);
    let text = draft.text();
    let at = draft.caret().min(text.len());
    let text = format!("{}{inserted}{}", &text[..at], &text[at..]);
    let caret = at + inserted.len();
    replace_text(app, which, text.clone(), caret);
    crate::suggest::caret_moved(app, which, &text, caret);
}

/// A composer button, kept [`BUTTON_INSET`] off the box's foot, so it stays
/// on the last line as the field grows.
fn lift<'a>(button: Element<'a, Message>) -> Element<'a, Message> {
    container(button)
        .padding(Padding::ZERO.bottom(BUTTON_INSET))
        .into()
}

/// What the composer shows for the open room.
pub fn composer_look(ctx: crate::ctx::Ctx<'_>) -> Look {
    let state = ctx.state;
    let room = state.selected_room();
    let discord = room.is_some_and(|room| room.network == Network::Discord);
    let enabled = state.room_ready
        && !state.busy
        && state.shows_text()
        && room.is_some_and(|room| {
            if discord {
                state.writable(room)
            } else {
                state.preview || room.encrypted
            }
        });
    let timed_out = room
        .and_then(|room| room.timed_out_until)
        .filter(|_| discord)
        .map(|until| {
            format!(
                "You're timed out in this server until {}.",
                crate::place_menus::until(until / 1000, ctx.layout.prefs.twelve_hour_clock)
            )
        });
    let request = room.filter(|room| room.request).map(|room| {
        if room.category == yapfast_core::discord::SPAM {
            format!("Discord thinks {} may be spam", room.name)
        } else {
            format!("{} wants to message you", room.name)
        }
    });
    let locked = timed_out.is_none()
        && !state.preview
        && discord
        && room.is_some_and(|room| !room.request && (room.read_only || room.locked));
    // Without Matrix everything is Discord, so the lines on encryption
    // say nothing new; what stops sending still shows.
    let discord_only = !cfg!(feature = "matrix");
    let hint = if room.is_none() {
        "Choose a DM or server channel to write a message."
    } else if state.preview {
        ""
    } else if request.is_some() {
        if discord_only {
            ""
        } else {
            "Discord · not end-to-end encrypted."
        }
    } else if let Some(timed_out) = &timed_out {
        timed_out
    } else if locked {
        ""
    } else if discord && !state.discord_online {
        "Discord is disconnected. Yapfast is reconnecting."
    } else if discord_only {
        ""
    } else if discord {
        "Discord · not end-to-end encrypted. Enter to send, Shift+Enter for a new line."
    } else if !state.trusted {
        "Sending is locked until this device is verified."
    } else if room.is_some_and(|room| room.encrypted) {
        "End-to-end encrypted. Enter to send, Shift+Enter for a new line."
    } else {
        "Unencrypted room. Reading only in this build."
    }
    .to_owned();
    Look {
        enabled,
        locked,
        encrypted: room.is_some_and(|room| room.encrypted),
        hint,
        request,
        attach: state.can_attach(),
        plus: ctx.app.can_start_threads() || ctx.app.can_create_polls(),
    }
}

/// An icon button in `tint`, greyed without a message.
pub fn composer_button(
    ctx: crate::ctx::Ctx<'_>,
    icon: Icon,
    label: &'static str,
    glyph: u32,
    tint: iced::Color,
    message: Option<Message>,
) -> Element<'static, Message> {
    let tint = if message.is_some() {
        tint
    } else {
        faded(tint, 0.45)
    };
    let face = ctx.look.glyph(icon, glyph, tint).boxed(32);
    let face = button(face)
        .padding(0)
        .width(32)
        .height(32)
        .on_press_maybe(message)
        .style(lit_style(None, ctx.look.theme.wash(0.1), 6.0));
    tooltip(
        face,
        controls::tip(ctx.look.theme, label),
        tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let look = composer_look(ctx);
    let mut compose = column![];
    if let Some(title) = &look.request {
        compose = compose.push(request_bar(ctx, title));
    } else {
        if let Some(reply) = reply_bar_info(ctx.state.compose()) {
            compose = compose.push(reply_bar(ctx, reply));
        }
        compose = compose.push(if look.locked {
            locked_bar(ctx)
        } else {
            composer_box(ctx, &look)
        });
    }
    let foot: Element<'a, Message> = if look.hint.is_empty() {
        Space::new().height(GUTTER).into()
    } else {
        let muted = ctx.look.theme.muted;
        let mut line = row![].spacing(6).align_y(Alignment::Center);
        if look.encrypted {
            line = line.push(ctx.look.glyph(Icon::Lock, 12, muted));
        }
        line = line.push(text(look.hint).size(13).color(muted));
        container(line)
            .height(HINT)
            .padding(Padding::ZERO.left(4))
            .align_y(Alignment::Center)
            .into()
    };
    container(compose.push(foot))
        .padding(Padding::ZERO.left(GUTTER).right(GUTTER))
        .into()
}

/// The box with the field and its buttons, and the suggestions over it.
fn composer_box<'a>(ctx: crate::ctx::Ctx<'a>, look: &Look) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let header = ctx.header;
    let replying = ctx.state.compose().is_some();
    // Discord's words: `Message @sam` in a DM, `Message #general` in a
    // channel, and a group's or thread's own name as it is.
    let name = &header.name;
    let placeholder = if replying {
        "Write a reply".to_owned()
    } else {
        match (header.direct, header.one_to_one, header.thread) {
            (true, true, _) => format!("Message @{name}"),
            (false, _, false) => format!("Message #{name}"),
            _ => format!("Message {name}"),
        }
    };
    let commanding = ctx.state.command_draft().is_some();
    let placeholder = crate::commands::command_placeholder(ctx).unwrap_or(placeholder);
    let field = Field::new(
        &ctx.composer.draft,
        Id::new(COMPOSER),
        placeholder,
        |event| Message::Write(Event::Field(Writing::Draft, event)),
        field::Colors {
            ground: iced::Color::TRANSPARENT,
            text: theme.text,
            placeholder: theme.placeholder,
            selection: theme.selection,
        },
    )
    .padding(Padding {
        // Under the pills, 10 px from them.
        top: if commanding { 10.0 } else { PAD },
        right: 6.0,
        bottom: PAD,
        left: 6.0,
    })
    .enabled(look.enabled)
    .pastes()
    .tabs(commanding)
    .suggesting(crate::suggest::suggesting(ctx, Writing::Draft));
    // Always in a column after the same kind of widget, so the field
    // keeps its state (the keys) as the pills come and go.
    let pills = crate::commands::command_pills(ctx);
    let field = column![pills, field];
    // A tab's button is lit while its tab shows, the emoji's also while
    // the emoji picker alone is open over the composer.
    let shown = ctx
        .picker
        .filter(|picker| picker.target == Target::Composer)
        .map(|picker| picker.tab);
    let tint = |tab: Tab| {
        if shown == Some(tab) {
            theme.text
        } else {
            theme.secondary
        }
    };
    let plus = composer_button(
        ctx,
        Icon::Add,
        if look.plus {
            "More options"
        } else {
            "Attach files"
        },
        27,
        theme.secondary,
        (look.attach || look.plus).then_some(Message::Write(Event::Plus)),
    );
    let opens = |tab: Tab| {
        look.enabled
            .then_some(Message::OpenPicker(Target::Composer, tab))
    };
    // As Discord's: the GIFs, the stickers, then the emoji, in one
    // picker, in a Discord room.
    let mut buttons = row![].spacing(6).align_y(Alignment::End);
    let state = ctx.state;
    if state.can_pick_gifs() {
        buttons = buttons.push(lift(composer_button(
            ctx,
            Icon::Gif,
            "Open GIF picker",
            22,
            tint(Tab::Gifs),
            opens(Tab::Gifs),
        )));
    }
    if state.can_pick_stickers() {
        buttons = buttons.push(lift(composer_button(
            ctx,
            Icon::Sticker,
            "Open sticker picker",
            22,
            tint(Tab::Stickers),
            opens(Tab::Stickers),
        )));
    }
    let emoji = buttons.push(lift(composer_button(
        ctx,
        Icon::Smile,
        "Add an emoji",
        22,
        tint(Tab::Emoji),
        opens(Tab::Emoji),
    )));
    let line = row![lift(plus), field, emoji]
        .spacing(6)
        .align_y(Alignment::End)
        .padding(Padding::ZERO.left(10).right(8));
    let ground = if look.enabled {
        theme.composer
    } else {
        theme.field
    };
    // Discord's outline: faint, and a notch lighter with the keys.
    let edge = if ctx.composer.focused {
        faded(theme.muted, 0.55)
    } else {
        theme.outline_field
    };
    let radius = if replying {
        corners([0.0, 0.0, 10.0, 10.0])
    } else {
        10.0.into()
    };
    let mut inside = column![];
    if let Some(tray) = crate::uploads::tray(ctx) {
        inside = inside.push(tray);
    }
    let frame = container(inside.push(line))
        .width(Length::Fill)
        .style(move |_| crate::style::outlined(ground, edge, radius));
    Popover::new(
        frame,
        crate::suggest::suggestions_view(ctx, Writing::Draft),
        Side::Above,
    )
    .into()
}

/// The bar over the field while replying: who to, the mention's toggle,
/// and the cancel.
fn reply_bar<'a>(ctx: crate::ctx::Ctx<'a>, reply: ReplyBar<'a>) -> Element<'a, Message> {
    reply_bar_for(
        ctx,
        reply,
        Message::Write(Event::TogglePing),
        Message::Write(Event::CancelReply),
    )
}

/// A reply's bar over a composer: who to, the mention's toggle
/// (`toggle`) and the cancel (`cancel`).
pub fn reply_bar_for<'a>(
    ctx: crate::ctx::Ctx<'a>,
    reply: ReplyBar<'a>,
    toggle: Message,
    cancel: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let ground = theme.sidebar;
    let ink = if reply.ping {
        theme.accent
    } else {
        theme.muted
    };
    let toggle = button(
        text(if reply.ping { "@ On" } else { "@ Off" })
            .size(13)
            .font(BOLD)
            .color(ink),
    )
    .padding([0, 8])
    .height(28)
    .on_press(toggle)
    .style(lit_style(None, theme.hover, 6.0));
    let toggle = tooltip(
        container(toggle).height(28).align_y(Alignment::Center),
        controls::tip(theme, "Mention the person you reply to"),
        tooltip::Position::Top,
    )
    .gap(6);
    container(
        row![
            ctx.look.glyph(Icon::Reply, 16, theme.muted),
            container(ctx.inline_emoji_line(
                &["Replying to ", reply.author, ": "],
                reply.body,
                reply.emoji,
                crate::markdown::Look {
                    size: 14.0,
                    line_height: 20.0,
                    ink: theme.secondary,
                },
                16,
                false,
            ))
            .width(Length::Fill)
            .clip(true),
            toggle,
            ctx.look
                .icon_button(Icon::X, "Cancel reply", cancel)
                .size(28.0, 16),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.left(14).right(4))
    .height(BAR)
    .clip(true)
    .align_y(Alignment::Center)
    .style(move |_| rounded(ground, corners([10.0, 10.0, 0.0, 0.0])))
    .into()
}

/// Discord's bar in place of the composer where this account may not
/// write.
fn locked_bar<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (ground, edge) = (theme.field, theme.outline_field);
    container(
        text("You do not have permission to send messages in this channel.")
            .size(16)
            .color(theme.muted)
            .wrapping(text::Wrapping::None),
    )
    .padding(Padding::ZERO.left(16).right(16))
    .width(Length::Fill)
    .height(HEIGHT)
    .clip(true)
    .align_y(Alignment::Center)
    .style(move |_| crate::style::outlined(ground, edge, 10.0))
    .into()
}

/// A message request's answer.
fn request_bar<'a>(ctx: crate::ctx::Ctx<'a>, title: &str) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let ground = theme.field;
    let words = column![
        text(title.to_owned())
            .size(15)
            .font(BOLD)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
        text("Accept to reply. Ignoring removes it without telling them.")
            .size(14)
            .color(theme.secondary)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(2)
    .width(Length::Fill)
    .clip(true);
    container(
        row![
            words,
            controls::action(
                theme,
                "Ignore",
                controls::Kind::Plain,
                Some(Message::Write(Event::Answer(false)))
            ),
            controls::action(
                theme,
                "Accept",
                controls::Kind::Primary,
                Some(Message::Write(Event::Answer(true)))
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: 12.0,
        right: 12.0,
        bottom: 12.0,
        left: 16.0,
    })
    .width(Length::Fill)
    .style(move |_| rounded(ground, 10))
    .into()
}

/// The draft's or the edit's text field.
pub fn writing<'a>(ctx: crate::ctx::Ctx<'a>, which: Writing) -> &'a field::Draft {
    match which {
        Writing::Draft => &ctx.composer.draft,
        Writing::Edit => &ctx.composer.edit,
        Writing::Aside => &ctx.drafts.aside,
        Writing::NewThread => &ctx.drafts.first,
        Writing::NewPost => &ctx.forum_post.body,
    }
}
