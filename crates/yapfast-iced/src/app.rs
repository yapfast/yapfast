//! The window's state, the messages its parts send, and the layers the view
//! stacks: the window itself, then whatever is open over it (the profile
//! card, mention suggestions, the emoji picker, a menu, the modal), each
//! with a backdrop that takes a click outside.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use iced::{
    Element, Length, Point, Size, Subscription, Task,
    advanced::image::Handle,
    keyboard::{self, key},
    widget::{column, container, operation, row, scrollable::Viewport, stack},
};
use yapfast_core::{
    application::Action,
    chat::{ChatMember, ChatMessage, Event},
};

use crate::{
    after,
    call::Focus,
    client::Client,
    composer::COMPOSER,
    control::{self, Command},
    controls::outside,
    field::{self, Draft, MenuEdit},
    fonts,
    looks::MemberLine,
    menus::Menu,
    mfa,
    picker::{self, Picker, Target},
    pictures::{Converted, Mark},
    profile::Card,
    rail::RailItem,
    select::{Sel, Select},
    sidebar::SideRow,
    signin::{self, SignIn},
    sync::Scroll,
    theme::{Theme, Themes},
    video::{self, Fresh},
    widgets::{Anchor, Place, PlayKey, Playing},
};

/// The open timeline's messages, as the core shares them.
pub type Messages = Arc<Vec<Arc<ChatMessage>>>;

/// Events the backend sent, taken out once by `update`: the core's events
/// aren't `Clone`, which a message must be.
pub type Batch = Arc<Mutex<Vec<Event>>>;

/// The most backend events applied together before the window takes them
/// in, as the Slint client batches them.
const EVENT_BATCH: usize = 64;

/// Nothing has this id: focusing it takes the keys from every field, as a
/// menu or the modal does while it's open.
pub const NOBODY: &str = "nobody";

#[derive(Clone)]
pub enum Message {
    Events(Batch),
    Stopped,
    /// Linking a Discord account went a step further.
    Linking(yapfast_core::discord_link::Progress),
    /// The sign-in screen and the verify banner; Discord's two-factor
    /// prompt.
    SignIn(signin::Event),
    Mfa(mfa::Event),
    /// A command from the test display's control file.
    Control(String),
    /// The lists' viewports, as their scrollables report them.
    Timeline(Viewport),
    /// The timeline's rows built took other heights than they were given.
    Measured(Vec<(u64, f32)>),
    /// The history's Earlier messages button; the jump bars; a jump's
    /// tint fading.
    Earlier,
    JumpUnread,
    JumpPresent,
    JumpFaded(String),
    /// A link in the timeline's text, and where the pointer is.
    Link(String, Point),
    Members(Viewport),
    /// The server rail, the sidebar, the account's card.
    Rail(crate::rail::Event),
    Side(crate::sidebar::Event),
    Account(crate::account::Event),
    /// The minute turned, for the clocks of busy voice channels.
    Minute,
    /// The composer's text, keys and menu.
    Composer(field::Event),
    ComposerMenu(MenuEdit),
    Pasted(Option<String>),
    /// A mention suggestion clicked, by its place.
    Mention(usize),
    CancelReply,
    /// The edit bar's cancel button.
    CancelEdit,
    /// The emoji picker: opened for the composer or a reaction, and what's
    /// done in it.
    OpenPicker(Target),
    Picker(picker::Event),
    /// Text in the timeline: a press or drag on it, the button let go, a
    /// press on nothing.
    Select(Select),
    SelectEnd,
    SelectClear,
    /// A right click on a message, by its place, where the pointer is.
    RowMenu(usize, Point),
    /// The menu's row under the pointer or the keys, by its place.
    MenuLight(usize),
    MenuChoose(usize),
    CloseMenu,
    /// The delete confirmation's answers.
    ConfirmDelete,
    CancelDelete,
    /// A key nothing in the window took.
    Key(keyboard::Key, keyboard::Modifiers),
    /// Scripts are waiting for a fallback font (`fonts::load_wanted`).
    FontsWanted,
    /// The pointer came onto a reaction chip or left it, and its tooltip
    /// fell due.
    PointReaction(String, String, bool),
    ReactionDue(String, String),
    ToggleMembers,
    /// One of the menu's quick reactions.
    React(String, &'static str),
    /// A button of a message's hover bar: what it does, to which message,
    /// and where what it opens goes.
    Act(crate::menus::MenuAct, String, Point),
    /// Nothing to do: a click a backdrop takes.
    Nothing,
    AskPictures,
    Report,
    /// The held member list's changes are due.
    Roster,
    /// A copy of a picture at the size it shows at was made.
    Fitted(crate::pictures::Fitted),
    /// A spoiler's cover over a message's attachment was clicked: the
    /// message by its place, the attachment's index.
    Reveal(usize, usize),
    /// A long message's Show more or Show less, by its place.
    Unfold(usize),
    /// A poll's card.
    Poll(crate::polls::Event),
    /// A voice message's card.
    Voice(crate::playback::Event),
    /// A video's cell asked for something.
    Clip(crate::playback::Clip),
    /// The question before a masked link: visit it, or go back.
    Leave(bool),
    /// An app's button that asks the app, on a message by item, by the
    /// button's id.
    UseButton(String, String),
    /// The pointer came onto or left a member's row, a message, the card.
    PointMember(String, bool),
    PointRow(String, bool),
    PointCard(bool),
    /// A member's line was clicked, by its place in the list.
    OpenMember(usize),
    /// A message's avatar or author was clicked, by its place.
    OpenAuthor(usize),
    CloseProfile,
    /// New video frames; the call's Join button; watch someone's stream,
    /// or stop; a tile clicked (shown large), or beside the tiles (none);
    /// leave the call.
    Frames(Vec<Fresh>),
    JoinVoice(String),
    Watch(String),
    StopWatching,
    FocusTile(Option<usize>),
    LeaveCall,
}

pub struct App {
    pub client: Client,
    pub window: Size,
    /// Previews, and avatars that come at the size they're drawn at,
    /// sharing the core's pixels.
    pub previews: RefCell<Converted<Handle>>,
    /// Copies of other pictures, at each size they're drawn at (width,
    /// height, cut to cover).
    pub scaled: RefCell<HashMap<(u32, u32, bool), Converted<Handle>>>,
    /// Dots and icons, by what they are, their size and colour.
    pub marks: RefCell<HashMap<Mark, Handle>>,
    /// Emoji for the menu's quick reactions and the reactions' tooltips.
    pub emoji: RefCell<HashMap<(&'static str, u32), Option<Handle>>>,
    /// Moving pictures playing, and those the view being built shows: the
    /// others' decoders go after each view.
    pub playing: RefCell<HashMap<PlayKey, Playing>>,
    pub moving: RefCell<HashSet<PlayKey>>,
    /// Copies of pictures at the sizes messages show them at.
    pub fitter: RefCell<crate::pictures::Fitter>,
    /// The rail's DM home and rows, its viewport, the tile under the
    /// pointer.
    pub rail_home: crate::rail::Home,
    pub rail: Vec<RailItem>,
    pub rail_scroll: Scroll,
    pub pointed_rail: Option<String>,
    /// What the window keeps in `layout.json`: open folders, collapsed
    /// categories.
    pub layout: crate::layout::Layout,
    /// The sidebar's header and rows, the row under the pointer, what the
    /// DMs' search holds, and the minute the voice clocks show.
    pub side_header: crate::sidebar::SideHeader,
    pub side: Vec<SideRow>,
    pub pointed_side: Option<String>,
    pub filter: String,
    pub minute: u64,
    /// What the conversation's header shows of the open room.
    pub header: crate::header::Header,
    pub selected: Option<String>,
    pub messages: Messages,
    /// Each row's height and, one longer, where each row starts; what each
    /// row shows beyond its message; the history's edge above the first.
    pub heights: Vec<f32>,
    pub tops: Vec<f32>,
    /// Heights the timeline's last layout found, not yet taken in.
    pub found: crate::widgets::Found,
    pub looks: Vec<crate::timeline::RowLook>,
    pub edge: crate::timeline::Edge,
    /// Spoilers clicked, as `item/index`, while Yapfast runs.
    pub revealed: HashSet<String>,
    /// The core's last jump and jump to the first unread message that the
    /// timeline went to, and the message tinted for it.
    pub jumped: Option<(String, u64)>,
    pub unread_jumped: u64,
    pub jump_lit: Option<String>,
    /// The "New" divider's message and the last read, as the looks were
    /// worked out for.
    pub marks_for: (Option<String>, Option<String>),
    /// What the rail, the sidebar, the header and the history's edge were
    /// made from last ([`crate::sync::lists_key`]); none to make them again.
    pub lists_shown: Option<u64>,
    /// The history's edge changed since the rows' heights were worked out.
    pub edge_changed: bool,
    /// The member list as shown, which changes at most once per
    /// `ROSTER_PAUSE`, and its lines.
    pub members: Option<Arc<Vec<ChatMember>>>,
    pub lines: Vec<MemberLine>,
    pub roster_shown: Option<Instant>,
    pub roster_waiting: bool,
    /// A server is open, and the open room is in one.
    pub server: bool,
    pub in_server: bool,
    /// The account's card, and whether the pointer is on its names.
    pub account: crate::account::Account,
    pub account_pointed: bool,
    /// The timeline's viewport: its distance from the end, which the
    /// scrollable (anchored to the bottom) keeps as rows come.
    pub from_end: f32,
    pub timeline_height: f32,
    pub width: f32,
    pub at_end: bool,
    /// Sent from a window of older messages, or Jump to present: the view
    /// goes to the newest once they've replaced the window.
    pub latest_due: bool,
    pub side_scroll: Scroll,
    pub member_scroll: Scroll,
    /// The composer's text and history: the core's draft for the open room.
    pub draft: Draft,
    /// The draft the core last had, to tell its own changes from ours.
    pub draft_known: String,
    /// The message being edited in the composer (the core's `edit`).
    pub editing: Option<String>,
    /// Where the `@` word being typed starts, the suggestion lit, and the
    /// `@` whose suggestions Escape closed.
    pub mention_at: Option<usize>,
    pub mention_index: usize,
    pub mention_dismissed: Option<usize>,
    pub picker: Option<Picker>,
    /// The timeline's selection, where it started and where it goes to, and
    /// whether the button is still down.
    pub selection: Option<(Sel, Sel)>,
    pub selecting: bool,
    pub menu: Option<Menu>,
    /// The confirmation's focused button: Delete (true) or Cancel.
    pub confirm_focus: bool,
    pub pointed_reaction: Option<(String, String)>,
    pub hide_members: bool,
    /// Pictures asked for and not here yet, and whether a batch is due.
    pub asked: HashSet<String>,
    pub asking: bool,
    pub reporting: bool,
    pub pointed_member: Option<String>,
    pub pointed_message: Option<String>,
    /// The voice message whose card the pointer is on, and the one paused
    /// while its waveform is held.
    pub pointed_voice: Option<String>,
    /// The video cell under the pointer, by `{item}/{index}`, and how far
    /// into the message's video playing its last frame is, in milliseconds.
    pub pointed_clip: Option<String>,
    pub clip_time: Option<u64>,
    pub voice_held: Option<String>,
    /// A masked link clicked, while the question before it shows.
    pub leaving: Option<String>,
    pub card: Option<Card>,
    /// Video: what [`video::frames`] looks at; the newest frame of each
    /// camera (by participant id) and of the watched stream ("stream:" and
    /// the streamer's id), with its size; the tile shown large, and whose
    /// stream is watched, as the Slint client keeps them.
    pub video: Arc<video::Shared>,
    pub frames: HashMap<String, (Handle, (u32, u32))>,
    pub focus: Option<Focus>,
    pub watched: Option<String>,
    /// The colours every part of the window paints with, and the themes
    /// the data folder offers.
    pub theme: Theme,
    pub themes: Themes,
    /// The sign-in screen's fields; the listeners of the backend's events
    /// and of linking's progress, which stop when dropped.
    pub sign_in: SignIn,
    pub backend_events: Option<iced::task::Handle>,
    pub linking: Option<iced::task::Handle>,
    /// The two-factor prompt shown, by its id and method, and its warning's
    /// wait.
    pub mfa_shown: Option<(u64, Option<yapfast_core::chat::MfaMethod>)>,
    pub wait: mfa::Wait,
}

impl App {
    pub fn new(client: Client, size: (u32, u32)) -> (Self, Task<Message>) {
        let (width, height) = (size.0 as f32, size.1 as f32);
        let layout = crate::layout::Layout::read(client.root());
        let themes = Themes::read(&layout);
        let theme = themes.shown();
        let (fitter, fitted) = crate::pictures::Fitter::new();
        let mut app = Self {
            client,
            window: Size::new(width, height),
            previews: Default::default(),
            scaled: Default::default(),
            marks: Default::default(),
            emoji: Default::default(),
            playing: Default::default(),
            moving: Default::default(),
            fitter: RefCell::new(fitter),
            rail_home: Default::default(),
            rail: Vec::new(),
            rail_scroll: Scroll::new(height),
            pointed_rail: None,
            layout,
            side_header: Default::default(),
            side: Vec::new(),
            pointed_side: None,
            filter: String::new(),
            minute: yapfast_core::clock::now_ms() / 1000 / 60 * 60,
            header: Default::default(),
            selected: None,
            messages: Default::default(),
            heights: Vec::new(),
            tops: vec![0.0],
            found: Default::default(),
            looks: Vec::new(),
            edge: Default::default(),
            revealed: HashSet::new(),
            jumped: None,
            unread_jumped: 0,
            jump_lit: None,
            marks_for: (None, None),
            lists_shown: None,
            edge_changed: false,
            members: None,
            lines: Vec::new(),
            roster_shown: None,
            roster_waiting: false,
            server: false,
            in_server: false,
            account: Default::default(),
            account_pointed: false,
            from_end: 0.0,
            timeline_height: height,
            width: width - crate::rail::WIDTH - crate::sidebar::WIDTH,
            at_end: true,
            latest_due: false,
            side_scroll: Scroll::new(height),
            member_scroll: Scroll::new(height),
            draft: Draft::default(),
            draft_known: String::new(),
            editing: None,
            mention_at: None,
            mention_index: 0,
            mention_dismissed: None,
            picker: None,
            selection: None,
            selecting: false,
            menu: None,
            confirm_focus: true,
            pointed_reaction: None,
            hide_members: false,
            asked: HashSet::new(),
            asking: false,
            reporting: false,
            pointed_member: None,
            pointed_message: None,
            pointed_voice: None,
            pointed_clip: None,
            clip_time: None,
            voice_held: None,
            leaving: None,
            card: None,
            video: Default::default(),
            frames: HashMap::new(),
            focus: None,
            watched: None,
            theme,
            themes,
            sign_in: SignIn::default(),
            backend_events: None,
            linking: None,
            mfa_shown: None,
            wait: mfa::Wait::default(),
        };
        // Started on a saved account: see `SignIn::starting`.
        app.sign_in.starting = !app.workspace() && app.client.state().busy;
        let frames = Task::batch([
            Task::run(video::frames(app.video.clone()), Message::Frames),
            Task::run(fitted, Message::Fitted),
        ]);
        let sync = app.sync();
        // The sign-in screen's first field takes the keys.
        let focus = if app.workspace() {
            Task::none()
        } else if signin::MATRIX {
            operation::focus(signin::USERNAME)
        } else {
            Task::none()
        };
        (app, Task::batch([sync, frames, focus]))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        self.take_found();
        match message {
            Message::Events(batch) => {
                let events = std::mem::take(&mut *batch.lock().expect("event batch"));
                for event in events {
                    self.client.apply(event);
                }
                self.sync()
            }
            Message::Stopped => {
                self.client.stopped();
                self.backend_events = None;
                self.sync()
            }
            Message::Linking(step) => {
                self.client.linking(step);
                self.sync()
            }
            Message::SignIn(event) => self.sign_in_event(event),
            Message::Mfa(event) => self.mfa_event(event),
            Message::Control(command) => self.control(&command),
            Message::Timeline(viewport) => self.timeline_scrolled(viewport),
            Message::Measured(heights) => self.measured(heights),
            Message::Earlier => {
                self.client.dispatch(Action::Earlier);
                self.sync()
            }
            Message::JumpUnread => {
                self.client.dispatch(Action::JumpToUnread);
                self.sync()
            }
            Message::JumpPresent => {
                self.latest_due = true;
                self.client.dispatch(Action::JumpToPresent);
                self.sync()
            }
            Message::JumpFaded(item) => {
                if self.jump_lit.as_ref() == Some(&item) {
                    self.jump_lit = None;
                }
                Task::none()
            }
            Message::Link(link, at) => self.follow_link(&link, at),
            Message::Members(viewport) => {
                self.member_scroll = Scroll::of(viewport);
                self.ask()
            }
            Message::Rail(event) => self.rail_event(event),
            Message::Side(event) => self.sidebar_event(event),
            Message::Account(event) => self.account_event(event),
            Message::Minute => {
                self.minute = yapfast_core::clock::now_ms() / 1000 / 60 * 60;
                Task::none()
            }
            Message::Composer(event) => self.composer_event(event),
            Message::ComposerMenu(edit) => self.composer_menu(edit),
            Message::Pasted(text) => {
                if text.is_some() {
                    self.draft.act(MenuEdit::Paste, text);
                    self.edited(self.draft.text(), self.draft.caret());
                }
                Task::none()
            }
            Message::Mention(index) => {
                self.mention_index = index;
                Task::batch([self.choose_mention(), operation::focus(COMPOSER)])
            }
            Message::CancelEdit => {
                self.client.dispatch(Action::CancelEdit);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Message::CancelReply => {
                self.client.dispatch(Action::CancelCompose);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Message::OpenPicker(target) => self.open_picker(target),
            Message::Picker(event) => self.picker_event(event),
            Message::Select(select) => {
                self.select(select);
                Task::none()
            }
            Message::SelectEnd => {
                self.select_end();
                Task::none()
            }
            Message::SelectClear => {
                self.selection = None;
                self.selecting = false;
                Task::none()
            }
            Message::RowMenu(index, at) => self.open_menu(index, at),
            Message::MenuLight(index) => {
                if let Some(menu) = &mut self.menu {
                    menu.lit = Some(index);
                }
                Task::none()
            }
            Message::MenuChoose(index) => self.choose(index),
            Message::Act(act, item, at) => self.act(act, item, at, None),
            Message::CloseMenu => {
                self.menu = None;
                operation::focus(COMPOSER)
            }
            Message::ConfirmDelete => {
                self.client.dispatch(Action::ConfirmDelete);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Message::CancelDelete => {
                self.client.dispatch(Action::CancelDelete);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Message::Key(key, modifiers) => self.key(key, modifiers),
            Message::FontsWanted => {
                fonts::load_wanted();
                Task::none()
            }
            Message::PointReaction(item, key, entered) => self.point_reaction(item, key, entered),
            Message::ReactionDue(item, key) => {
                if self
                    .pointed_reaction
                    .as_ref()
                    .is_some_and(|(i, k)| *i == item && *k == key)
                {
                    self.client
                        .dispatch(Action::HoverReaction(Some((item, key))));
                    return self.sync();
                }
                Task::none()
            }
            Message::ToggleMembers => {
                self.hide_members = !self.hide_members;
                Task::none()
            }
            Message::Nothing => Task::none(),
            Message::React(item, key) => {
                self.menu = None;
                self.client.dispatch(Action::React {
                    item,
                    key: key.to_owned(),
                });
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Message::AskPictures => self.ask_now(),
            Message::Report => {
                self.reporting = false;
                self.report()
            }
            Message::Fitted(fitted) => {
                self.fitter.borrow_mut().made(fitted);
                Task::none()
            }
            Message::Reveal(row, attachment) => self.reveal(row, attachment),
            Message::Unfold(row) => self.unfold(row),
            Message::Poll(event) => self.poll_event(event),
            Message::Voice(event) => self.voice_event(event),
            Message::Clip(clip) => self.clip_event(clip),
            Message::Leave(visit) => self.leave(visit),
            Message::UseButton(item, custom_id) => {
                self.client.dispatch(Action::UseButton { item, custom_id });
                self.sync()
            }
            Message::Roster => {
                self.roster_waiting = false;
                self.show_roster(true);
                self.ask()
            }
            Message::PointMember(id, entered) => {
                point(&mut self.pointed_member, id, entered);
                self.ask()
            }
            Message::PointRow(item, entered) => {
                point(&mut self.pointed_message, item, entered);
                self.ask()
            }
            Message::PointCard(pointed) => {
                if let Some(card) = &mut self.card {
                    card.pointed = pointed;
                }
                self.ask()
            }
            Message::OpenMember(index) => self.open_member(index),
            Message::OpenAuthor(index) => self.open_author(index),
            Message::CloseProfile => {
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                self.sync()
            }
            Message::Frames(found) => {
                self.frames_found(found);
                Task::none()
            }
            Message::JoinVoice(id) => {
                self.client.dispatch(Action::RequestJoinVoice(id));
                self.sync()
            }
            Message::Watch(user) => {
                self.client
                    .dispatch(Action::Voice(yapfast_core::voice::Command::Watch(user)));
                self.sync()
            }
            Message::StopWatching => {
                self.client
                    .dispatch(Action::Voice(yapfast_core::voice::Command::StopWatching));
                self.sync()
            }
            Message::FocusTile(slot) => {
                self.focus_tile(slot);
                Task::none()
            }
            Message::LeaveCall => {
                self.client
                    .dispatch(Action::Voice(yapfast_core::voice::Command::Leave));
                self.sync()
            }
        }
    }

    /// A line of the test display's control file.
    fn control(&mut self, command: &str) -> Task<Message> {
        tracing::info!(command, "test window control");
        match control::parse(command) {
            Command::Stats => control::allocator_stats(),
            Command::Trim => control::trim(),
            Command::Quit => return iced::exit(),
            Command::Hide => {
                return iced::window::latest()
                    .and_then(|id| iced::window::set_mode(id, iced::window::Mode::Hidden));
            }
            Command::Show => {
                return iced::window::latest()
                    .and_then(|id| iced::window::set_mode(id, iced::window::Mode::Windowed));
            }
            Command::Act(action) => {
                self.client.dispatch(action);
                return self.sync();
            }
            Command::Theme(id) => {
                if !self.choose_theme(&id) {
                    tracing::warn!(id, "no such theme");
                }
            }
            Command::Later => {
                tracing::info!(command, "the iced client has no tray yet");
            }
            Command::Unknown => tracing::warn!(command, "unknown window control"),
        }
        Task::none()
    }

    /// Keys no field took: the modal's and the menu's, Ctrl+C for the
    /// timeline's selection, and Escape.
    fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        let named = match &key {
            keyboard::Key::Named(named) => Some(*named),
            _ => None,
        };
        if self.client.app.mfa_prompt().is_some() {
            return match named {
                Some(key::Named::Escape) => self.mfa_event(mfa::Event::Cancel),
                _ => Task::none(),
            };
        }
        if self.client.state().confirm_delete.is_some() {
            return self.confirm_key(named);
        }
        if self.leaving.is_some() {
            return match named {
                Some(key::Named::Escape) => self.leave(false),
                Some(key::Named::Enter) => self.leave(true),
                _ => Task::none(),
            };
        }
        if self.menu.is_some() {
            return self.menu_key(named);
        }
        if modifiers.command()
            && key.as_ref() == keyboard::Key::Character("c")
            && let Some(copied) = self.copied()
        {
            return iced::clipboard::write(copied);
        }
        if named == Some(key::Named::Escape) {
            return self.escape();
        }
        Task::none()
    }

    /// Escape where nothing else took it: the topmost thing open closes.
    pub fn escape(&mut self) -> Task<Message> {
        if self.menu.is_some() {
            self.menu = None;
            return operation::focus(COMPOSER);
        }
        if self.picker.is_some() {
            return self.close_picker();
        }
        if self.card.is_some() {
            return self.update(Message::CloseProfile);
        }
        if self.selection.is_some() {
            self.selection = None;
            return Task::none();
        }
        if self.editing.is_some() {
            self.client.dispatch(Action::CancelEdit);
            return Task::batch([self.sync(), operation::focus(COMPOSER)]);
        }
        if self.client.state().compose().is_some() {
            self.client.dispatch(Action::CancelCompose);
            return self.sync();
        }
        Task::none()
    }

    /// The pointer came onto a reaction chip or left it: who reacted is
    /// asked for after half a second on it, as Discord's client waits.
    fn point_reaction(&mut self, item: String, key: String, entered: bool) -> Task<Message> {
        if entered {
            self.pointed_reaction = Some((item.clone(), key.clone()));
            return after(Duration::from_millis(500), Message::ReactionDue(item, key));
        }
        if self
            .pointed_reaction
            .as_ref()
            .is_some_and(|(i, k)| *i == item && *k == key)
        {
            self.pointed_reaction = None;
            if self.client.state().hovered_reaction.is_some() {
                self.client.dispatch(Action::HoverReaction(None));
            }
        }
        Task::none()
    }

    /// Keys nothing in the window took, fonts the shaper wants, and the
    /// test display's commands.
    pub fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Key(key, modifiers))
            }
            _ => None,
        });
        let fonts = Subscription::run(fonts::wakes).map(|()| Message::FontsWanted);
        let mut subscriptions = vec![keys, fonts];
        // Busy voice channels show how long their call has gone on, to the
        // minute.
        // Open polls, how long they have left.
        if self
            .side
            .iter()
            .any(|row| matches!(row, crate::sidebar::SideRow::Room(room) if room.started.is_some()))
            || crate::polls::open_poll(&self.messages, self.minute)
        {
            subscriptions.push(iced::time::every(Duration::from_secs(15)).map(|_| Message::Minute));
        }
        if let Some(path) = control::path() {
            subscriptions.push(
                Subscription::run_with(path, |path| control::commands(path.clone()))
                    .map(Message::Control),
            );
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self) -> Element<'_, Message> {
        self.moving.borrow_mut().clear();
        self.fitter.borrow_mut().begin();
        if !self.workspace() {
            return self.sign_in_view();
        }
        // The window's own background is the theme's chat (`window_style`).
        // The rail and the sidebar on the rail's ground, the account's card
        // across the foot of both.
        let rail = self.theme.rail;
        let navigation = container(column![
            row![self.rail_view(), self.sidebar_view()].height(Length::Fill),
            self.account_view(),
        ])
        .width(crate::rail::WIDTH + crate::sidebar::WIDTH)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(rail));
        let base: Element<'_, Message> = row![navigation, self.conversation()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        // Always a stack, so the window's widget state stays as overlays
        // come and go: the card, the suggestions, the picker, a menu, the
        // modal, each over the last.
        let mut layers = stack![base];
        if let Some(card) = self.card_view() {
            layers = layers
                .push(outside(Message::CloseProfile, false))
                .push(card);
        }
        if let Some(mentions) = self.mentions_view() {
            layers = layers.push(mentions);
        }
        if let Some(picker) = &self.picker {
            let size = picker.size();
            let place = match &picker.target {
                // Over the emoji button, its right edge level with the button's.
                Target::Composer => {
                    let right = self.chat_right() - 8.0 - 4.0;
                    let bottom = self.window.height - 8.0 - self.draft.height() - 8.0;
                    Place::new(
                        picker.view(&self.theme),
                        Point::new(right - size.width, bottom),
                        8.0,
                    )
                    .anchor(Anchor::Bottom)
                }
                Target::React(_, at) => {
                    Place::new(picker.view(&self.theme), *at, 8.0).anchor(Anchor::Pointer)
                }
            };
            layers = layers
                .push(outside(
                    Message::Picker(picker::Event::Field(field::Event::Escape)),
                    false,
                ))
                .push(place);
        }
        if let Some(menu) = self.menu_view() {
            layers = layers.push(outside(Message::CloseMenu, false)).push(menu);
        }
        if let Some(modal) = self.delete_view() {
            layers = layers.push(modal);
        }
        if let Some(leaving) = self.leaving_view() {
            layers = layers.push(leaving);
        }
        if let Some(prompt) = self.mfa_view() {
            layers = layers.push(prompt);
        }
        // Decoders of moving pictures no longer shown go, and copies of
        // pictures no longer shown.
        self.fitter.borrow_mut().end();
        let moving = self.moving.borrow();
        self.playing
            .borrow_mut()
            .retain(|key, _| moving.contains(key));
        layers.width(Length::Fill).height(Length::Fill).into()
    }

    pub fn window_style(&self, _: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: self.theme.chat,
            text_color: self.theme.text,
        }
    }

    pub fn theme(&self) -> iced::Theme {
        self.theme.iced()
    }

    /// Shows the window in another theme, built in or the person's own, and
    /// keeps the choice, as Appearance does. Returns whether there's such a
    /// theme. Pictures stay as they are; only what the client drew in the
    /// old colours (icons, dots, the card's name) is drawn again, and the
    /// member list's styled names take the theme's palette.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(theme) = self.themes.find(id) else {
            return false;
        };
        self.themes.choose(self.client.root(), &theme.id);
        if theme == self.theme {
            return true;
        }
        self.theme = theme;
        self.marks.borrow_mut().clear();
        self.show_roster(false);
        true
    }
}

/// Keeps the thing the pointer is on: set on entering, cleared on leaving
/// only if it's still the one left (the next one's entering may come first).
pub fn point(pointed: &mut Option<String>, id: String, entered: bool) {
    if entered {
        *pointed = Some(id);
    } else if pointed.as_ref() == Some(&id) {
        *pointed = None;
    }
}

/// A row's key in a virtualized list, from its id.
pub fn key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// The backend's events, up to [`EVENT_BATCH`] at a time; iced applies all
/// the batches queued before it next draws.
pub fn batches(
    events: tokio::sync::mpsc::UnboundedReceiver<Event>,
) -> impl iced::futures::Stream<Item = Batch> + Send + 'static {
    iced::futures::stream::unfold(events, |mut events| async move {
        let first = events.recv().await?;
        let mut batch = vec![first];
        while batch.len() < EVENT_BATCH {
            match events.try_recv() {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        Some((Arc::new(Mutex::new(batch)), events))
    })
}
