//! The window's state and what each message does to it. Its messages are
//! in `message`, what's open over it in `layers`, what it listens to in
//! `subscription`, and the layers the view stacks in `view`.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{Point, Size, Task, keyboard, widget::operation};
use yapfast_core::{application::Action, chat::ChatMember};

use crate::{
    after,
    client::Client,
    field::Draft,
    looks::MemberLine,
    menus::Menu,
    mfa,
    picker::Picker,
    profile::Card,
    rail::RailItem,
    select::Sel,
    sidebar::SideRow,
    signin::{self, SignIn},
    sync::Scroll,
    theme::{Theme, Themes},
    video,
};

mod layers;
mod message;
mod subscription;
mod view;

pub use self::message::{Message, Messages, batches};

/// What's under the pointer in each part of the window that shows it, by
/// id: set on entering, cleared on leaving ([`point`]).
#[derive(Default)]
pub struct Pointed {
    /// The rail's tile.
    pub rail: Option<String>,
    /// The sidebar's row.
    pub side: Option<String>,
    /// A reaction chip, by its message's item and the reaction's key.
    pub reaction: Option<(String, String)>,
    /// A member list's row.
    pub member: Option<String>,
    /// The timeline's message, by item.
    pub message: Option<Arc<str>>,
    /// A voice message's card, by item.
    pub voice: Option<String>,
    /// A video cell, by `{item}/{index}`.
    pub clip: Option<String>,
}

/// Nothing has this id: focusing it takes the keys from every field, as a
/// menu or the modal does while it's open.
pub const NOBODY: &str = "nobody";

pub struct App {
    pub client: Client,
    pub window: Size,
    /// The window itself, the tray and what goes on without the window.
    pub shell: crate::shell::Shell,
    /// The title bar's inbox, while open, and a press on it that may move
    /// the window (`window_bar.rs`).
    pub inbox: Option<crate::window_bar::Inbox>,
    pub bar_drag: crate::window_bar::Drag,
    /// A look at the releases nobody asked for is under way, whose newer
    /// release a notification tells of (`settings/updates.rs`).
    pub announce_updates: bool,
    /// The pictures made to draw, and the moving ones playing.
    pub pictures: crate::pictures::Pictures,
    /// What's under the pointer, part by part.
    pub pointed: Pointed,
    /// The rail's DM home and rows, and its viewport.
    pub rail_home: crate::rail::Home,
    pub rail: Vec<RailItem>,
    pub rail_scroll: Scroll,
    /// What the window keeps in `layout.json`: open folders, collapsed
    /// categories.
    pub layout: crate::layout::Layout,
    /// The sidebar's header and rows.
    pub side_header: crate::sidebar::SideHeader,
    pub side: Vec<SideRow>,
    /// The busy voice channel's card under the pointer was put away by a
    /// click or its menu, until the pointer leaves the row.
    pub voice_card_off: bool,
    /// The sidebar rows' places to drag a channel or category among, and
    /// the drag going on.
    pub side_places: crate::arrange::Places,
    pub arranging: Option<crate::arrange::Arranging>,
    /// Someone being dragged from a voice channel to another.
    pub moving_person: Option<crate::arrange::Moving>,
    /// The thread beside's width as its edge was pressed, while it's dragged.
    pub aside_drag: Option<f32>,
    /// The composer written in last, the channel's or the thread's.
    pub writing_last: crate::composer::Writing,
    pub filter: String,
    /// Where the rail went last, so the DM search's filter goes when it
    /// goes elsewhere, as the Slint client's.
    pub filtered_in: Option<yapfast_core::application::Navigation>,
    /// The minute now, as the clocks that show minutes read it, and the
    /// timers to the next minute and the next midnight, while armed.
    pub minute: u64,
    pub clocks: crate::timeline::Clocks,
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
    /// What the member lines' looks were made with.
    pub roster_made: Option<crate::looks::LooksMadeWith>,
    /// The Activity panel's cards over the member list, and whether it's
    /// opened past its first three.
    pub activity_cards: Vec<crate::members::ActivityCard>,
    pub activity_open: bool,
    /// The feed and its people the cards were made from, held so a new
    /// one is told by its address.
    pub activity_for: (
        Arc<[yapfast_core::chat::FeedEntry]>,
        Option<Arc<[yapfast_core::chat::FeedPerson]>>,
    ),
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
    /// Whether the composer's field has the keys, which its outline shows.
    pub composer_focused: bool,
    /// The message being edited in its row (the core's `edit`), and the
    /// edit's own field.
    pub editing: Option<String>,
    pub edit: Draft,
    /// The suggestions over the field typed in last.
    pub suggest: crate::suggest::Suggest,
    /// Previews of the staged pictures, by path (none while being made),
    /// and files dragged over the window.
    pub thumbnails: HashMap<std::path::PathBuf, Option<Arc<yapfast_core::chat::Picture>>>,
    /// The staged lists the previews were last made for, by place and
    /// length: the channel's and the thread's.
    pub staged_seen: [usize; 4],
    pub drop: crate::uploads::Drop,
    pub picker: Option<Picker>,
    /// The keys held with a click: Shift puts a GIF in the composer.
    pub modifiers: keyboard::Modifiers,
    /// The message added to one being forwarded.
    pub forward_note: String,
    /// The timeline's selection, where it started and where it goes to, and
    /// whether the button is still down.
    pub selection: Option<(Sel, Sel)>,
    pub selecting: bool,
    pub menu: Option<Menu>,
    /// The confirmation's focused button: Delete (true) or Cancel.
    pub confirm_focus: bool,
    pub hide_members: bool,
    /// Whether a batch of pictures to ask for is due; what was asked for
    /// is the core's to know (`Application::picture_requested`).
    pub asking: bool,
    pub reporting: bool,
    /// The message the keyboard is stepping on (`browse.rs`), by item.
    pub keyed: Option<String>,
    /// A voice message's volume is open over its mute, by its item, while
    /// the pointer is on the button or the slider (counted, as they
    /// overlap).
    pub voice_volume: Option<(String, u8)>,
    /// How far into the message's video playing its last frame is, in
    /// milliseconds.
    pub clip_time: Option<u64>,
    /// The voice message paused while its waveform is held.
    pub voice_held: Option<String>,
    /// A masked link clicked, while the question before it shows.
    pub leaving: Option<String>,
    pub card: Option<Card>,
    /// The profiles on screen.
    pub profiles: crate::profile::Profiles,
    /// Where in the window the last press was, for a menu its button opens.
    pub pressed: std::cell::Cell<Point>,
    /// The header's button last pressed, where it is: what it opens opens
    /// from it.
    pub header_spot: std::cell::Cell<iced::Rectangle>,
    /// Where the link last pressed in text is, or the sticker, from the
    /// press (`App::pressed_spot`): an emoji's or sticker's card opens
    /// under it.
    pub link_spot: std::cell::Cell<iced::Rectangle>,
    /// A role's or a mutual server's card, open where it was clicked.
    pub cards: crate::info_cards::Cards,
    /// The friends list's and the requests' own state, and Active Now's
    /// cards while the friends list shows.
    pub friends: crate::friends::Friends,
    pub active_now: Vec<crate::active_now::Card>,
    /// The core's people and lists changes Active Now's cards were made
    /// at, while they show.
    pub active_now_made: Option<(u64, u64)>,
    /// The Reactions window's scroll and pointer.
    pub reactions: crate::reactions::Reactions,
    /// The person dialog's field and tick.
    pub person: crate::person::Dialog,
    /// Video: what [`video::frames`] looks at, and the newest frame of
    /// each camera (by participant id), of the watched stream ("stream:"
    /// and the streamer's id), of this account's screen and of a message's
    /// video, with its size, which `video::Video` draws.
    pub video: Arc<video::Shared>,
    /// The call's own state: what's large, what's open, the corner.
    pub call: crate::call::Call,
    /// Settings while it's open, and which build this is, which it shows.
    pub settings: Option<Box<crate::settings::Settings>>,
    /// A server's settings' own state while they're open.
    pub server_screen: Option<Box<crate::server_settings::Screen>>,
    /// What's chosen or typed in the invite, join and channel dialogs, and
    /// whether the leave dialog showed at the last sync.
    pub server_dialogs: crate::server_dialogs::Dialogs,
    /// What's chosen in an event's, gift's or authorization's dialog.
    pub link_dialogs: crate::link_dialogs::LinkDialogs,
    /// Whether Discord heard nobody is using Yapfast.
    pub away: crate::away::Away,
    pub leave_shown: bool,
    pub channel_screen: Option<Box<crate::channel_settings::Screen>>,
    /// The account's popover, and the custom status's dialog's draft.
    pub account_popover: Option<crate::status::AccountPopover>,
    pub status_draft: Option<crate::status::Draft>,
    /// The quick switcher's places and the one Enter goes to.
    pub quick: crate::switcher::Quick,
    pub build: crate::settings::Build,
    /// Push to talk's watcher while it's the input mode, and whether the
    /// key being handled is its key, which types nothing during a call.
    pub talk: crate::settings::voice::Talk,
    pub talk_pressed: bool,
    /// A message's video full screen; the size its cell asked for its
    /// frames at; the pointer on its volume's button or slider.
    pub clip_full: bool,
    pub clip_cell: (u32, u32),
    pub clip_volume: u8,
    /// The size the picture viewer opened at, for its message's item and
    /// attachment, which the full picture coming keeps.
    pub viewer_size: Option<(String, usize, Size)>,
    /// The thread beside the channel, while it shows, the last message it
    /// had read, and the side composers' fields (`thread.rs`).
    pub aside: Option<Box<crate::thread::Aside>>,
    pub seen_aside: Option<String>,
    pub drafts: crate::thread::Drafts,
    pub popovers: crate::popovers::Popovers,
    pub modal: crate::modal::Shown,
    /// Browse Channels' search.
    pub browse_filter: String,
    pub searching: crate::search::Searching,
    /// The forum's form for a post, as typed (`forum.rs`).
    pub forum_post: crate::forum::Post,
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
        // No test reaches the speakers or the microphone.
        #[cfg(test)]
        yapfast_core::silence_audio();
        let (width, height) = (size.0 as f32, size.1 as f32);
        let layout = crate::layout::Layout::open(client.root(), &crate::build().version);
        let themes = Themes::read(&layout);
        let theme = themes.shown();
        let (fitter, fitted) = crate::pictures::Fitter::new();
        let mut client = client;
        for action in layout.actions() {
            client.dispatch(action);
        }
        // How calls send and play, as Voice's settings keep it; message
        // times on the clock Messages chose; the games named by hand.
        layout.voice.apply();
        layout.prefs.apply_clock();
        yapfast_core::discord::set_custom_games(&layout.prefs.game_executables);
        client.share_typing = layout.prefs.share_typing.unwrap_or(true);
        // The window opens at its size in the desktop's pixels whatever
        // the zoom, as the Slint client's, and its layout gets fewer.
        let zoom = yapfast_core::zoom::saved(layout.zoom);
        let shell = crate::shell::Shell {
            zoom,
            own_title_bar: crate::window_bar::own(layout.system_title_bar),
            frame: Size::new(width / zoom, height / zoom),
            ..Default::default()
        };
        let mut app = Self {
            client,
            window: Size::new(width, height),
            shell,
            inbox: None,
            bar_drag: Default::default(),
            announce_updates: false,
            pointed: Pointed::default(),
            pictures: crate::pictures::Pictures::new(fitter),
            rail_home: Default::default(),
            rail: Vec::new(),
            rail_scroll: Scroll::new(height),
            layout,
            side_header: Default::default(),
            side: Vec::new(),
            voice_card_off: false,
            side_places: Default::default(),
            arranging: None,
            moving_person: None,
            aside_drag: None,
            writing_last: Default::default(),
            filter: String::new(),
            filtered_in: None,
            minute: yapfast_core::clock::now_ms() / 1000 / 60 * 60,
            clocks: Default::default(),
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
            roster_made: None,
            activity_cards: Vec::new(),
            activity_open: false,
            activity_for: (Arc::from([]), None),
            roster_shown: None,
            roster_waiting: false,
            server: false,
            in_server: false,
            account: Default::default(),
            account_pointed: false,
            from_end: 0.0,
            timeline_height: height,
            width: width - crate::conversation::NAVIGATION,
            at_end: true,
            latest_due: false,
            side_scroll: Scroll::new(height),
            member_scroll: Scroll::new(height),
            draft: Draft::default(),
            draft_known: String::new(),
            composer_focused: false,
            editing: None,
            edit: Draft::default(),
            suggest: Default::default(),
            thumbnails: HashMap::new(),
            staged_seen: [0; 4],
            drop: Default::default(),
            picker: None,
            modifiers: keyboard::Modifiers::default(),
            forward_note: String::new(),
            selection: None,
            selecting: false,
            menu: None,
            confirm_focus: true,
            hide_members: false,
            asking: false,
            reporting: false,
            keyed: None,
            voice_volume: None,
            clip_time: None,
            voice_held: None,
            leaving: None,
            card: None,
            profiles: Default::default(),
            pressed: Default::default(),
            link_spot: Default::default(),
            header_spot: Default::default(),
            cards: Default::default(),
            friends: Default::default(),
            active_now: Vec::new(),
            active_now_made: None,
            reactions: Default::default(),
            person: Default::default(),
            video: Default::default(),
            call: Default::default(),
            settings: None,
            server_screen: None,
            server_dialogs: Default::default(),
            link_dialogs: Default::default(),
            away: Default::default(),
            leave_shown: false,
            channel_screen: None,
            account_popover: None,
            status_draft: None,
            quick: Default::default(),
            build: crate::build(),
            talk: Default::default(),
            talk_pressed: false,
            clip_full: false,
            clip_cell: (0, 0),
            clip_volume: 0,
            viewer_size: None,
            aside: None,
            seen_aside: None,
            drafts: Default::default(),
            popovers: Default::default(),
            modal: Default::default(),
            browse_filter: String::new(),
            searching: Default::default(),
            forum_post: Default::default(),
            theme,
            themes,
            sign_in: SignIn::default(),
            backend_events: None,
            linking: None,
            mfa_shown: None,
            wait: mfa::Wait::default(),
        };
        app.fit_window();
        let talk = app.watch_talk_key();
        if app.layout.voice.noise_suppression != yapfast_core::noise::Suppressor::Off {
            app.call.noise_last = app.layout.voice.noise_suppression;
        }
        // Started on a saved account: see `SignIn::starting`.
        app.sign_in.starting = !app.workspace() && app.client.state().busy;
        let frames = Task::batch([
            video::follow(app.video.clone()),
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
        let away = app.watch_away();
        (app, Task::batch([sync, frames, focus, away, talk]))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle_message(message);
        // The bar comes and goes with full screen, and the room under it
        // with it.
        self.fit_window();
        task
    }

    fn handle_message(&mut self, message: Message) -> Task<Message> {
        self.take_found();
        self.take_aside_found();
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
            Message::Later => {
                self.client.dispatch(Action::Later);
                self.sync()
            }
            Message::DismissNotice => self.dismiss_notice(),
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
            Message::MemberActivity(event) => self.member_activity(event),
            Message::Rail(event) => self.rail_event(event),
            Message::Side(event) => self.sidebar_event(event),
            Message::Arrange(event) => self.arrange_event(event),
            Message::Account(event) => self.account_event(event),
            Message::Minute => self.minute_turned(),
            Message::Day => self.day_turned(),
            Message::Write(event) => self.write_event(event),
            Message::OpenPicker(target, tab) => self.open_picker(target, tab),
            Message::Picker(event) => self.picker_event(event),
            Message::Forward(event) => self.forward_event(event),
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
            Message::RowMenu(item, at) => match self.row_of(&item) {
                Some(row) => self.open_menu(row, at),
                None => Task::none(),
            },
            Message::MenuLight(index) => self.menu_light(index),
            Message::SubLight(index) => {
                if let Some(menu) = &mut self.menu {
                    menu.sub_lit = Some(index);
                }
                Task::none()
            }
            Message::SubChoose(index) => self.choose_sub(index),
            Message::MenuChoose(index) => self.choose(index),
            Message::MenuScrolled(down) => {
                if let Some(menu) = &mut self.menu {
                    menu.scrolled = down;
                }
                Task::none()
            }
            Message::Act(act, item, at) => self.act(act, item, at, None),
            Message::CloseMenu => {
                self.menu = None;
                self.refocus()
            }
            Message::PersonMenu(user, at) => {
                self.open_person_menu(&user, at, crate::person::CONTEXT_MENU)
            }
            Message::Person(event) => self.person_event(event),
            Message::ConfirmDelete => {
                self.client.dispatch(Action::ConfirmDelete);
                Task::batch([self.sync(), self.refocus()])
            }
            Message::CancelDelete => {
                self.client.dispatch(Action::CancelDelete);
                Task::batch([self.sync(), self.refocus()])
            }
            Message::Key(key, modifiers) => self.key(key, modifiers),
            Message::Pressed(key, modifiers, physical) => {
                self.talk_pressed = self.is_talk_key(physical);
                let task = self.key(key, modifiers);
                self.talk_pressed = false;
                task
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::FontsLoaded => Task::none(),
            Message::PointReaction(item, key, entered) => self.point_reaction(item, key, entered),
            Message::ReactionDue(item, key) => {
                if self
                    .pointed
                    .reaction
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
                // Hidden, the list lets its lines and pictures go.
                Task::batch([self.sync(), self.ask()])
            }
            Message::Nothing => Task::none(),
            Message::Bell => {
                // Under the bell, 6 px down, as the Slint client's.
                let at = self.header_button().map_or(self.pressed.get(), |bell| {
                    Point::new(bell.x, bell.y + bell.height + 6.0)
                });
                self.open_bell(at)
            }
            Message::React(item, key) => {
                self.menu = None;
                self.client.dispatch(Action::React { item, key });
                Task::batch([self.sync(), self.refocus()])
            }
            Message::AskPictures => self.ask_now(),
            Message::Report => {
                self.reporting = false;
                self.report()
            }
            Message::Fitted(fitted) => {
                self.pictures.fitter.borrow_mut().made(fitted);
                Task::none()
            }
            Message::Reveal(item, attachment) => self.reveal(&item, attachment),
            Message::Unfold(item) => self.unfold(&item),
            Message::ShowBlocked(item) => {
                self.revealed.insert(crate::message::blocked_key(&item));
                self.ask()
            }
            Message::Poll(event) => self.poll_event(event),
            Message::Voice(event) => self.voice_event(event),
            Message::Clip(clip) => self.clip_event(clip),
            Message::Image(event) => self.image_event(event),
            Message::Thread(event) => self.thread_event(event),
            Message::Popover(event) => self.popover_event(event),
            Message::Modal(event) => self.modal_event(event),
            Message::Forum(event) => self.forum_event(event),
            Message::BrowseChannels(event) => self.browse_channels_event(event),
            Message::Search(event) => self.search_event(event),
            Message::Focus(id) => operation::focus(id),
            Message::Leave(visit) => self.leave(visit),
            Message::BotSelect(event) => self.select_event(event),
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
                point(&mut self.pointed.member, id, entered);
                self.ask()
            }
            Message::PointRow(item, entered) => {
                point(&mut self.pointed.message, item, entered);
                self.ask()
            }
            Message::OpenMember(id) => self.open_member(&id),
            Message::OpenAuthor(item) => match self.row_of(&item) {
                Some(row) => self.open_author(row),
                None => Task::none(),
            },
            Message::CloseProfile => self.profile_event(crate::profile::Event::Close),
            Message::Info(event) => self.info_event(event),
            Message::Profile(event) => self.profile_event(event),
            Message::Friends(event) => self.friends_event(event),
            Message::Reactions(event) => self.reactions_event(event),
            Message::Frames(clip_time) => {
                self.frames_found(clip_time);
                Task::none()
            }
            Message::Call(event) => self.call_event(event),
            Message::Settings(event) => self.settings_event(event),
            Message::ServerSettings(event) => self.server_settings_event(event),
            Message::Resized(size) => {
                self.shell.frame = size;
                self.fit_zoom();
                let fitted = self.active_now_fits();
                self.fit_window();
                if self.active_now_fits() == fitted {
                    return self.read_window_state();
                }
                self.active_now_sync();
                Task::batch([self.read_window_state(), self.ask()])
            }
            Message::ServerDialog(event) => self.server_dialog_event(event),
            Message::LinkDialog(event) => self.link_dialog_event(event),
            Message::Away(event) => self.away_event(event),
            Message::ChannelSettings(event) => self.channel_settings_event(event),
            Message::Status(event) => self.status_event(event),
            Message::Switcher(event) => self.switcher_event(event),
            Message::Shell(event) => self.shell_event(event),
            Message::Zoom(key, physical) => {
                self.zoom_key(&key, physical).unwrap_or_else(Task::none)
            }
            Message::ZoomStep(step) => self.zoom_step(step),
            Message::Bar(event) => self.bar_event(event),
        }
    }

    /// The pointer came onto a reaction chip or left it: who reacted is
    /// asked for after half a second on it, as Discord's client waits.
    fn point_reaction(&mut self, item: String, key: String, entered: bool) -> Task<Message> {
        if entered {
            self.pointed.reaction = Some((item.clone(), key.clone()));
            return after(Duration::from_millis(500), Message::ReactionDue(item, key));
        }
        if self
            .pointed
            .reaction
            .as_ref()
            .is_some_and(|(i, k)| *i == item && *k == key)
        {
            self.pointed.reaction = None;
            if self.client.state().hovered_reaction.is_some() {
                self.client.dispatch(Action::HoverReaction(None));
            }
        }
        Task::none()
    }
}

/// Keeps the thing the pointer is on: set on entering, cleared on leaving
/// only if it's still the one left (the next one's entering may come first).
pub fn point<T: PartialEq>(pointed: &mut Option<T>, id: T, entered: bool) {
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
