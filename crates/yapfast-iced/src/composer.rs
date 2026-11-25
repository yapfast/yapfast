//! The composer under the timeline: the message request's answer in its
//! place, the reply's bar over it (with whether the reply mentions its
//! author), the field (`field::Field`) between the + and the emoji button,
//! suggestions over it (`suggest.rs`), and the strip over it all
//! (`typing.rs`). The draft is the core's, per channel
//! (`Action::SetDraft`), which tells the room this account is typing; the
//! field follows it when the core changes it, as when a sent message
//! empties it. A message of one's own is edited in its row (`edit.rs`), in
//! a field of its own beside the draft.

pub mod tools;

use iced::{
    Alignment, Element, Length, Padding, Task, keyboard,
    widget::{Id, Space, button, column, container, operation, row, text, tooltip},
};
use yapfast_core::{
    application::{Action, Compose},
    network::Network,
    themes::package::art::{KeySlot, MAX_KEY_TRAVEL},
};

use crate::{
    app::{App, Message, NOBODY},
    controls::{self, lit_style},
    field::{self, Field, MenuEdit},
    fonts::{BOLD, MEDIUM},
    menus::{Menu, MenuAct, MenuRow},
    picker::{Tab, Target},
    pictures::Icon,
    popover::{Popover, Side},
    style::{corners, faded, rounded},
    themes::ComposerKey,
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
/// The room inside the box before its first button.
pub const INSET: f32 = 10.0;

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
    /// Gift and Apps open Yapfast's menus of native actions and browser links.
    Tools(tools::Kind),
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
    /// The line on the strip over it: why it can't send, or nothing; with
    /// Matrix, whether the room is end-to-end encrypted, under a lock if
    /// it is.
    pub hint: String,
    /// The hint only informs, as the line on encryption does: someone
    /// typing takes its place.
    pub note: bool,
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
    app.window.height - GUTTER - BUTTON_INSET - 32.0
}

pub fn composer_top(app: &App) -> f32 {
    let foot = GUTTER;
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
        Event::Tools(kind) => tools::open(app, kind),
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
    typed: Option<field::Typed>,
) -> Task<Message> {
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

/// Ctrl+V and the menu's Paste: in the channel's composer, the thread
/// beside's, a new forum post's message and the New Thread form's, files
/// copied in a file manager or a picture go with the message, and
/// otherwise the text is pasted; the edit's field takes text alone. The
/// clipboard is read for files even with the message full, so the core
/// says why they can't go; where files never go (the preview, a channel
/// not to write in), it's the text that is pasted.
fn paste(app: &App, which: Writing) -> Task<Message> {
    let state = app.client.state();
    let full =
        |staged: &[yapfast_core::chat::Staged]| staged.len() >= yapfast_core::chat::MAX_FILES;
    let files_go = match which {
        // A forum's files go with its post, as dropped ones do.
        Writing::Draft | Writing::NewPost => state.can_attach() || full(state.staged()),
        Writing::Aside => state.aside_can_attach() || full(state.aside_staged()),
        Writing::NewThread => {
            state.can_attach_first() || state.new_thread().is_some_and(|form| full(&form.files))
        }
        Writing::Edit => false,
    };
    if files_go && let Some(read) = crate::desktop::desktop().paste_reader() {
        return Task::perform(
            async move { tokio::task::spawn_blocking(read).await.ok().flatten() },
            move |files| {
                Message::Write(Event::Uploads(crate::uploads::Event::Clipboard(
                    which, files,
                )))
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
        crate::conversation::chat_left(app)
            + app.theme.spacing().chat_padding.unwrap_or(GUTTER)
            + INSET,
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
    // replaced it. A message the slowmode's wait holds back isn't sent,
    // so nothing moves.
    let mut to_end = Task::none();
    let state = app.client.state();
    let held = state
        .selected
        .as_ref()
        .is_some_and(|room| state.slowmode_wait(room, yapfast_core::clock::now_ms()) > 0);
    if state.can_send() && !held {
        if app.client.state().detached {
            app.timeline.latest_due = true;
        } else {
            to_end = crate::timeline::scroll::go_to_end(app);
        }
    }
    app.client.dispatch(Action::Send);
    app.cue(yapfast_core::themes::media::SoundSlot::Send);
    app.themes.particle_event(
        &app.theme.id,
        yapfast_core::themes::package::art::Trigger::MessageSent,
        None,
    );
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
    // Each with whether it only informs, as the lines on encryption do,
    // rather than saying why this account can't send.
    let (hint, note) = if room.is_none() {
        ("Choose a DM or server channel to write a message.", false)
    } else if state.preview {
        ("", false)
    } else if request.is_some() {
        if discord_only {
            ("", false)
        } else {
            ("Discord · not end-to-end encrypted.", true)
        }
    } else if let Some(timed_out) = &timed_out {
        (timed_out.as_str(), false)
    } else if locked {
        ("", false)
    } else if discord && !state.discord_online {
        ("Discord is disconnected. Yapfast is reconnecting.", false)
    } else if discord_only {
        ("", false)
    } else if discord {
        (
            "Discord · not end-to-end encrypted. Enter to send, Shift+Enter for a new line.",
            true,
        )
    } else if !state.trusted {
        ("Sending is locked until this device is verified.", false)
    } else if room.is_some_and(|room| room.encrypted) {
        (
            "End-to-end encrypted. Enter to send, Shift+Enter for a new line.",
            true,
        )
    } else {
        ("Unencrypted room. Reading only in this build.", false)
    };
    let hint = hint.to_owned();
    Look {
        enabled,
        locked,
        encrypted: room.is_some_and(|room| room.encrypted),
        hint,
        note,
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

/// The composer in two: the typing strip, and the rest under it (the
/// bars, the box and the foot), each in the composer's gutters. A theme's
/// composer ground lies under the rest alone, so the strip, as Discord's,
/// has no ground of its own.
pub fn parts<'a>(ctx: crate::ctx::Ctx<'a>) -> (Element<'a, Message>, Element<'a, Message>) {
    let look = composer_look(ctx);
    // The strip over it all, as Discord's: who else is typing, or why
    // this account can't send, and the slowmode's note.
    let hint = (!look.hint.is_empty()).then(|| {
        let ink = ctx.look.theme.secondary;
        let mut line = row![].spacing(6).align_y(Alignment::Center);
        if look.encrypted {
            line = line.push(ctx.look.glyph(Icon::Lock, 12, ink));
        }
        line = line.push(
            text(look.hint.clone())
                .size(crate::typing::SIZE)
                .font(MEDIUM)
                .color(ink)
                .wrapping(text::Wrapping::None),
        );
        container(line)
            .padding(Padding::ZERO.left(crate::typing::EDGE))
            .width(Length::Fill)
            .clip(true)
            .into()
    });
    let gutter = ctx.look.theme.spacing().chat_padding.unwrap_or(GUTTER);
    let strip = container(crate::typing::strip(ctx, hint, look.note))
        .padding(Padding::ZERO.left(gutter).right(gutter))
        .into();
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
    let rest = container(compose.push(Space::new().height(gutter)))
        .padding(Padding::ZERO.left(gutter).right(gutter))
        .into();
    (strip, rest)
}

/// [`composer_button`] on a theme's key for `slot` (Bubblegum's candy
/// keys), when the theme draws one: the key lies under the glyph, rises a
/// pixel under the pointer and goes down while pressed, its pressed
/// picture shown and the glyph moved down its travel. Without a key it's
/// the plain button.
fn keyed_button(
    ctx: crate::ctx::Ctx<'_>,
    slot: KeySlot,
    icon: Icon,
    label: &'static str,
    glyph: u32,
    tint: iced::Color,
    message: Option<Message>,
) -> Element<'static, Message> {
    let Some(key) = ctx.themes.composer_key(&ctx.look.theme.id, slot).cloned() else {
        return composer_button(ctx, icon, label, glyph, tint, message);
    };
    let tint = if message.is_some() {
        tint
    } else {
        faded(tint, 0.45)
    };
    let face = ctx.look.glyph(icon, glyph, tint).boxed(32);
    // The key is the button's face: no hover wash over the candy.
    let face = button(face)
        .padding(0)
        .width(32)
        .height(32)
        .on_press_maybe(message)
        .style(|_, _| button::Style::default());
    let hover = if ctx.look.theme.motion().hover == yapfast_iced_kit::theme::Hover::None {
        0.0
    } else {
        -1.0
    };
    let face = crate::widgets::Wrap::new(face, KeyCap { key, hover });
    tooltip(
        face,
        controls::tip(ctx.look.theme, label),
        tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

/// Draws a [`ComposerKey`] under a button and moves both with the pointer.
struct KeyCap {
    key: ComposerKey,
    /// Logical pixels the key and glyph move under the pointer (up is
    /// negative).
    hover: f32,
}

#[derive(Default)]
struct KeyState {
    pointed: bool,
    pressed: bool,
}

impl crate::widgets::Hooks<Message> for KeyCap {
    type State = KeyState;

    fn draw_viewport(&self, child: iced::Rectangle, viewport: iced::Rectangle) -> iced::Rectangle {
        // The key, centred on the button and as big as it's drawn, and
        // the few pixels it and the glyph move either way.
        let size = self.key.size;
        let over = ((size.width.max(size.height) - child.width.min(child.height)) / 2.0).max(0.0);
        child
            .expand(over + MAX_KEY_TRAVEL + 2.0)
            .intersection(&viewport)
            .unwrap_or(viewport)
    }

    fn update(
        &mut self,
        state: &mut KeyState,
        event: &iced::Event,
        at: crate::widgets::At,
        shell: &mut iced::advanced::Shell<'_, Message>,
        child: &mut crate::widgets::Inner<'_, '_, Message>,
    ) {
        child.update(shell);
        let iced::Event::Mouse(event) = event else {
            return;
        };
        let pointed = at.cursor.is_over(at.child);
        let pressed = match event {
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left) => pointed,
            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)
            | iced::mouse::Event::CursorLeft => false,
            _ => state.pressed && pointed,
        };
        if (pointed, pressed) != (state.pointed, state.pressed) {
            state.pointed = pointed;
            state.pressed = pressed;
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        state: &KeyState,
        renderer: &mut crate::widgets::Renderer,
        at: crate::widgets::At,
        child: &mut dyn FnMut(&mut crate::widgets::Renderer),
    ) {
        use iced::advanced::{Renderer as _, image::Renderer as _};
        let lifted = if state.pointed { self.hover } else { 0.0 };
        let (picture, key_offset, glyph_offset) = match (&self.key.down, state.pressed) {
            (Some(down), true) => (down, 0.0, self.key.travel),
            (None, true) => (&self.key.up, self.key.travel, self.key.travel),
            (_, false) => (&self.key.up, lifted, lifted),
        };
        let centre = at.child.center();
        let size = self.key.size;
        renderer.draw_image(
            iced::advanced::image::Image {
                handle: picture.clone(),
                filter_method: iced::advanced::image::FilterMethod::Linear,
                rotation: iced::Radians(0.0),
                border_radius: Default::default(),
                opacity: 1.0,
                snap: false,
            },
            iced::Rectangle::new(
                iced::Point::new(
                    centre.x - size.width / 2.0,
                    centre.y - size.height / 2.0 + key_offset,
                ),
                size,
            ),
            at.viewport,
        );
        if glyph_offset == 0.0 {
            child(renderer);
        } else {
            renderer.with_transformation(iced::Transformation::translate(0.0, glyph_offset), child);
        }
    }
}

/// The box with the field and its buttons, and the suggestions over it.
fn composer_box<'a>(ctx: crate::ctx::Ctx<'a>, look: &Look) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let header = ctx.header;
    let replying = ctx.state.compose().is_some();
    // Discord's words: `Message @sam` in a DM, `Message #general` in a
    // channel, a quoted thread name, and a group's own name as it is.
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
    let placeholder = if replying {
        placeholder
    } else {
        ctx.themes.label(
            &theme.id,
            yapfast_core::themes::chrome::Label::Composer,
            placeholder,
            &[("channel", name.as_str())],
        )
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
            placeholder: theme.token_colour("placeholder", theme.placeholder),
            selection: theme.token_colour("selection", theme.selection),
        },
    )
    .padding(Padding {
        // Under the pills, 10 px from them.
        top: if commanding { 10.0 } else { PAD },
        right: 6.0,
        bottom: PAD,
        left: 12.0,
    })
    .enabled(look.enabled)
    .font(theme.font("body", crate::fonts::REGULAR))
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
    let plus = keyed_button(
        ctx,
        KeySlot::Plus,
        Icon::Add,
        if look.plus {
            "More options"
        } else {
            "Attach files"
        },
        20,
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
    if tools::available(ctx) {
        buttons = buttons
            .push(lift(tools::button(ctx, tools::Kind::Gift)))
            .push(lift(tools::button(ctx, tools::Kind::Apps)));
    }
    // A theme can draw a key under each button (Bubblegum's candy keys);
    // the preview, which has no GIFs or stickers, still shows a button
    // with a key of its own, greyed, so no key is left without its glyph.
    let keyed = |slot: KeySlot| state.preview && ctx.themes.composer_key(&theme.id, slot).is_some();
    if state.can_pick_gifs() || keyed(KeySlot::Gif) {
        buttons = buttons.push(lift(keyed_button(
            ctx,
            KeySlot::Gif,
            Icon::Gif,
            "Open GIF picker",
            22,
            tint(Tab::Gifs),
            opens(Tab::Gifs).filter(|_| state.can_pick_gifs()),
        )));
    }
    if state.can_pick_stickers() || keyed(KeySlot::Sticker) {
        buttons = buttons.push(lift(keyed_button(
            ctx,
            KeySlot::Sticker,
            Icon::Sticker,
            "Open sticker picker",
            22,
            tint(Tab::Stickers),
            opens(Tab::Stickers).filter(|_| state.can_pick_stickers()),
        )));
    }
    let emoji = buttons.push(lift(keyed_button(
        ctx,
        KeySlot::Emoji,
        Icon::Smile,
        "Add an emoji",
        22,
        tint(Tab::Emoji),
        opens(Tab::Emoji),
    )));
    // A theme's prompt before the words, such as `$`.
    let prompt = ctx
        .themes
        .chrome(&theme.id)
        .and_then(|chrome| chrome.prompt.as_deref())
        .filter(|prompt| !prompt.is_empty())
        .map(|prompt| {
            container(
                text(prompt.to_owned())
                    .font(crate::theme_chrome::mono(ctx))
                    .size(theme.type_size("body", 15.0))
                    .color(theme.muted),
            )
            .padding(Padding::ZERO.bottom(12))
        });
    let mut line = row![lift(plus)];
    if let Some(prompt) = prompt {
        line = line.push(prompt);
    }
    let line = line
        .push(field)
        .push(emoji)
        .spacing(6)
        .align_y(Alignment::End)
        .padding(Padding::ZERO.left(INSET).right(8));
    let ground = if look.enabled {
        theme.token_colour("composer", theme.composer)
    } else {
        theme.field
    };
    // Discord's outline: faint, and a notch lighter with the keys.
    let edge = if ctx.composer.focused {
        theme.token_colour("composer-focus", faded(theme.muted, 0.55))
    } else {
        theme.outline_field
    };
    let surface_radius = theme.surface_radius("composer", 10.0);
    let radius = if replying {
        corners([0.0, 0.0, surface_radius, surface_radius])
    } else {
        surface_radius.into()
    };
    let mut border = theme.surface_border("composer", 10.0, 1.0, edge);
    border.radius = radius;
    let outline_fallback = border;
    if theme.outline("composer").is_some() {
        border.width = 0.0;
    }
    let shadow = theme.surface_shadow("composer", iced::Shadow::default());
    let mut inside = column![];
    if let Some(tray) = crate::uploads::tray(ctx) {
        inside = inside.push(tray);
    }
    // A theme's composer art lies over the field's ground, under its
    // words and buttons, as in Slint; a glass composer's field is the
    // glass (the blur under the theme's ground) rather than a solid ground.
    let surface = yapfast_core::themes::package::art::ArtSurface::Composer;
    let art = crate::theme_art::art_layers(ctx, surface, surface_radius);
    let glass = crate::theme_art::glass_under(ctx, surface, surface_radius);
    let solid = glass.is_none();
    let frame: Element<'a, Message> = if art.is_some() || glass.is_some() {
        let mut stack = iced::widget::Stack::with_children([container(inside.push(line))
            .width(Length::Fill)
            .into()]);
        if let Some(art) = art {
            stack = stack.push_under(art);
        }
        if let Some(glass) = glass {
            stack = stack.push_under(glass);
        }
        stack
            .push_under(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| container::Style {
                        background: solid.then(|| ground.into()),
                        border,
                        shadow,
                        ..Default::default()
                    }),
            )
            .into()
    } else {
        container(inside.push(line))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border,
                shadow,
                ..Default::default()
            })
            .into()
    };
    let frame = crate::theme_art::outlined(
        frame,
        ctx,
        yapfast_core::themes::layout::BorderPlace::Composer,
        "composer",
        10.0,
        outline_fallback.width,
        outline_fallback.color,
    );
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
                    size: theme.type_size("small", 14.0),
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
