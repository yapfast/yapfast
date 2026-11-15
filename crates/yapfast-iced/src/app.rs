//! The window's state, the messages its parts send, and the layers the view
//! stacks: the window itself, then whatever is open over it (the profile
//! card, mention suggestions, the emoji picker, a menu, the modal), each
//! with a backdrop that takes a click outside.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use iced::{
    Element, Length, Point, Size, Subscription, Task,
    advanced::image::Handle,
    keyboard::{self, key},
    widget::{Space, Stack, column, container, operation, row, scrollable::Viewport, stack},
};
use yapfast_core::{
    application::Action,
    chat::{ChatMember, ChatMessage, Event},
};

use crate::{
    after,
    client::Client,
    control::{self, Command},
    controls::outside,
    field::{self, Draft},
    fonts,
    looks::MemberLine,
    menus::Menu,
    mfa,
    picker::{self, Picker, Target},
    profile::Card,
    rail::RailItem,
    select::{Sel, Select},
    sidebar::SideRow,
    signin::{self, SignIn},
    sync::Scroll,
    theme::{Theme, Themes},
    video::{self, Fresh},
    widgets::{Anchor, Place},
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

/// The zoom level's note, this far below the window's top or its bar.
const ZOOM_NOTE_TOP: f32 = 12.0;

#[derive(Clone)]
pub enum Message {
    Events(Batch),
    /// Whether someone is using the window.
    Away(crate::away::Event),
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
    /// Newer messages after a window of older ones.
    Later,
    /// The core's notice over the conversation, dismissed.
    DismissNotice,
    JumpUnread,
    JumpPresent,
    JumpFaded(String),
    /// A link in the timeline's text, and where the pointer is.
    Link(String, Point),
    Members(Viewport),
    /// The member list's Activity panel.
    MemberActivity(crate::members::Event),
    /// The server rail, the sidebar, the account's card.
    Rail(crate::rail::Event),
    Side(crate::sidebar::Event),
    /// A channel or category dragged in the sidebar.
    Arrange(crate::arrange::Event),
    Account(crate::account::Event),
    /// The minute turned, for the clocks of busy voice channels.
    Minute,
    /// The composer's and the inline edit's fields, bars and buttons.
    Write(crate::composer::Event),
    /// The expression picker: opened for the composer, the edit or a
    /// reaction on a tab, and what's done in it.
    OpenPicker(Target, crate::picker::Tab),
    Forward(crate::forward::Event),
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
    /// A menu taller than the window was scrolled this far down.
    MenuScrolled(f32),
    /// The open submenu's row under the pointer, or chosen.
    SubLight(usize),
    SubChoose(usize),
    CloseMenu,
    /// Someone's menu, at the pointer, by their id; the person dialog.
    PersonMenu(String, Point),
    Person(crate::person::Event),
    /// The delete confirmation's answers.
    ConfirmDelete,
    CancelDelete,
    /// A key nothing in the window took.
    Key(keyboard::Key, keyboard::Modifiers),
    /// Shift, Ctrl, Alt or the logo key went down or up.
    Modifiers(keyboard::Modifiers),
    /// Scripts are waiting for a fallback font (`fonts::load_wanted`).
    FontsWanted,
    /// The pointer came onto a reaction chip or left it, and its tooltip
    /// fell due.
    PointReaction(String, String, bool),
    ReactionDue(String, String),
    ToggleMembers,
    /// One of the menu's quick reactions.
    React(String, String),
    /// A button of a message's hover bar: what it does, to which message,
    /// and where what it opens goes.
    Act(crate::menus::MenuAct, String, Point),
    /// Nothing to do: a click a backdrop takes.
    Nothing,
    /// The header's bell: the open channel's notification choices.
    Bell,
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
    /// A blocked person's message, shown when asked for.
    ShowBlocked(usize),
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
    /// An app's select menu: opened, an option toggled, searched, closed.
    BotSelect(crate::bot_select::Event),
    /// The pointer came onto or left a member's row, a message, the card.
    PointMember(String, bool),
    PointRow(String, bool),
    /// A member's line was clicked, by its place in the list.
    OpenMember(usize),
    /// A message's avatar or author was clicked, by its place.
    OpenAuthor(usize),
    CloseProfile,
    /// Something done on a role's or a mutual server's card (`info_cards.rs`).
    Info(crate::info_cards::Event),
    /// Something done on a profile: the card, the full one, the DM's panel.
    Profile(crate::profile::Event),
    /// Something done on the friends list, the requests or Add Friend.
    Friends(crate::friends::Event),
    /// Something done in the Reactions window.
    Reactions(crate::reactions::Event),
    /// A second passed while a profile times an activity.
    Second,
    /// New video frames.
    Frames(Vec<Fresh>),
    /// Something done to the call or in it (`call/`).
    Call(crate::call::Event),
    /// Something done in Settings (`settings/`).
    Settings(crate::settings::Event),
    /// Something done in a server's settings (`server_settings/`).
    ServerSettings(crate::server_settings::Event),
    /// Something done in the invite, join, channel or leave dialog
    /// (`server_dialogs.rs`).
    ServerDialog(crate::server_dialogs::Event),
    LinkDialog(crate::link_dialogs::Event),
    ChannelSettings(crate::channel_settings::Event),
    Status(crate::status::Event),
    Switcher(crate::switcher::Event),
    /// The window took another size, or opened at one.
    Resized(Size),
    /// A message's picture large, or its attachment saved
    /// (`image_viewer.rs`).
    Image(crate::image_viewer::Event),
    /// The thread beside the channel, or the New Thread form (`thread.rs`).
    Thread(crate::thread::Event),
    Popover(crate::popovers::Event),
    Modal(crate::modal::Event),
    Forum(crate::forum::Event),
    BrowseChannels(crate::browse_channels::Event),
    Search(crate::search::Event),
    /// The keys go to the field with this id.
    Focus(&'static str),
    /// The window's frame, the tray, notifications and later starts
    /// (`shell.rs`).
    Shell(crate::shell::Event),
    /// A key pressed with Ctrl, which may zoom.
    Zoom(keyboard::Key, keyboard::key::Physical),
    /// Ctrl and the wheel.
    ZoomStep(crate::zoom::Step),
    /// The window's own title bar (`window_bar.rs`).
    Bar(crate::window_bar::Event),
}

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
    /// The Activity panel's cards over the member list, and whether it's
    /// opened past its first three.
    pub activity_cards: Vec<crate::members::ActivityCard>,
    pub activity_open: bool,
    /// The feed and its people the cards were made from, by address.
    pub activity_for: (usize, usize),
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
    pub pointed_reaction: Option<(String, String)>,
    pub hide_members: bool,
    /// Whether a batch of pictures to ask for is due; what was asked for
    /// is the core's to know (`Application::picture_requested`).
    pub asking: bool,
    pub reporting: bool,
    pub pointed_member: Option<String>,
    pub pointed_message: Option<String>,
    /// The message the keyboard is stepping on (`browse.rs`), by item.
    pub keyed: Option<String>,
    /// The voice message whose card the pointer is on, and the one paused
    /// while its waveform is held.
    pub pointed_voice: Option<String>,
    /// A voice message's volume is open over its mute, by its item, while
    /// the pointer is on the button or the slider (counted, as they
    /// overlap).
    pub voice_volume: Option<(String, u8)>,
    /// The video cell under the pointer, by `{item}/{index}`, and how far
    /// into the message's video playing its last frame is, in milliseconds.
    pub pointed_clip: Option<String>,
    pub clip_time: Option<u64>,
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
    /// The Reactions window's scroll and pointer.
    pub reactions: crate::reactions::Reactions,
    /// The person dialog's field and tick.
    pub person: crate::person::Dialog,
    /// Video: what [`video::frames`] looks at; the newest frame of each
    /// camera (by participant id), of the watched stream ("stream:" and
    /// the streamer's id), of this account's screen and of a message's
    /// video, with its size.
    pub video: Arc<video::Shared>,
    pub frames: HashMap<String, (Handle, (u32, u32))>,
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
    pub popover: Option<crate::status::Popover>,
    pub status_draft: Option<crate::status::Draft>,
    /// The quick switcher's place Enter goes to.
    pub switcher_chosen: usize,
    pub build: crate::settings::Build,
    /// Push to talk's watcher while it's the input mode.
    pub talk: crate::settings::voice::Talk,
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
        let shell = crate::shell::Shell {
            zoom: crate::zoom::saved(layout.zoom),
            own_title_bar: crate::window_bar::own(layout.system_title_bar),
            frame: Size::new(width, height),
            ..Default::default()
        };
        let mut app = Self {
            client,
            window: Size::new(width, height),
            shell,
            inbox: None,
            bar_drag: Default::default(),
            announce_updates: false,
            pictures: crate::pictures::Pictures::new(fitter),
            rail_home: Default::default(),
            rail: Vec::new(),
            rail_scroll: Scroll::new(height),
            pointed_rail: None,
            layout,
            side_header: Default::default(),
            side: Vec::new(),
            pointed_side: None,
            voice_card_off: false,
            side_places: Default::default(),
            arranging: None,
            moving_person: None,
            aside_drag: None,
            writing_last: Default::default(),
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
            activity_cards: Vec::new(),
            activity_open: false,
            activity_for: (0, 0),
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
            composer_focused: false,
            editing: None,
            edit: Draft::default(),
            suggest: Default::default(),
            thumbnails: HashMap::new(),
            drop: Default::default(),
            picker: None,
            modifiers: keyboard::Modifiers::default(),
            forward_note: String::new(),
            selection: None,
            selecting: false,
            menu: None,
            confirm_focus: true,
            pointed_reaction: None,
            hide_members: false,
            asking: false,
            reporting: false,
            pointed_member: None,
            pointed_message: None,
            keyed: None,
            pointed_voice: None,
            voice_volume: None,
            pointed_clip: None,
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
            reactions: Default::default(),
            person: Default::default(),
            video: Default::default(),
            frames: HashMap::new(),
            call: Default::default(),
            settings: None,
            server_screen: None,
            server_dialogs: Default::default(),
            link_dialogs: Default::default(),
            away: Default::default(),
            leave_shown: false,
            channel_screen: None,
            popover: None,
            status_draft: None,
            switcher_chosen: 0,
            build: crate::build(),
            talk: Default::default(),
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
        app.watch_talk_key();
        if app.layout.voice.noise_suppression != yapfast_core::noise::Suppressor::Off {
            app.call.noise_last = app.layout.voice.noise_suppression;
        }
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
        let away = app.watch_away();
        (app, Task::batch([sync, frames, focus, away]))
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
            Message::Minute => {
                self.minute = yapfast_core::clock::now_ms() / 1000 / 60 * 60;
                self.make_activity_cards();
                Task::none()
            }
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
            Message::RowMenu(index, at) => self.open_menu(index, at),
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
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
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
            Message::Reveal(row, attachment) => self.reveal(row, attachment),
            Message::Unfold(row) => self.unfold(row),
            Message::ShowBlocked(row) => {
                if let Some(message) = self.row_message(row) {
                    self.revealed
                        .insert(crate::message::blocked_key(&message.item));
                }
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
                point(&mut self.pointed_member, id, entered);
                self.ask()
            }
            Message::PointRow(item, entered) => {
                point(&mut self.pointed_message, item, entered);
                self.ask()
            }
            Message::OpenMember(index) => self.open_member(index),
            Message::OpenAuthor(index) => self.open_author(index),
            Message::CloseProfile => self.profile_event(crate::profile::Event::Close),
            Message::Info(event) => self.info_event(event),
            Message::Profile(event) => self.profile_event(event),
            Message::Friends(event) => self.friends_event(event),
            Message::Reactions(event) => self.reactions_event(event),
            Message::Second => Task::none(),
            Message::Frames(found) => {
                self.frames_found(found);
                Task::none()
            }
            Message::Call(event) => self.call_event(event),
            Message::Settings(event) => self.settings_event(event),
            Message::ServerSettings(event) => self.server_settings_event(event),
            Message::Resized(size) => {
                self.shell.frame = size;
                self.fit_window();
                self.read_window_state()
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

    /// A line of the test display's control file.
    fn control(&mut self, command: &str) -> Task<Message> {
        tracing::info!(command, "test window control");
        match control::parse(command) {
            Command::Stats => control::allocator_stats(),
            Command::Trim => control::trim(),
            Command::Quit => return self.quit(),
            Command::Close => return self.close_requested(),
            Command::Destroy => {
                // The window goes without its memory given back: a `trim`
                // after shows what closing frees.
                return self.drop_window();
            }
            Command::Open => return self.open_window(),
            Command::Hide => {
                if let Some(id) = self.shell.window {
                    return iced::window::set_mode(id, iced::window::Mode::Hidden);
                }
            }
            Command::Show => {
                if let Some(id) = self.shell.window {
                    return iced::window::set_mode(id, iced::window::Mode::Windowed);
                }
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
        // An app's form, unless its select menu is open over it.
        if self.client.state().interaction_modal.is_some()
            && self.client.state().select_menu.is_none()
        {
            return match named {
                Some(key::Named::Escape) => self.modal_event(crate::modal::Event::Close),
                _ => Task::none(),
            };
        }
        if self.client.state().viewer.is_some() && self.menu.is_none() {
            return self.viewer_key(named);
        }
        if self.client.state().switcher.is_some() {
            return self.switcher_key(named, modifiers.shift());
        }
        if self.client.state().new_dm.is_some() {
            return match named {
                Some(key::Named::Escape) => self.switcher_event(crate::switcher::Event::CloseNewDm),
                _ => Task::none(),
            };
        }
        if self.client.state().poll_draft.is_some() {
            return match named {
                Some(key::Named::Escape) if self.picker.is_some() => self.close_picker(),
                Some(key::Named::Escape) => self.switcher_event(crate::switcher::Event::ClosePoll),
                _ => Task::none(),
            };
        }
        if self.link_dialog_open() {
            return self.link_dialog_key(named);
        }
        if self.server_dialog_open() {
            return self.server_dialog_key(named);
        }
        if self.client.state().status_editor.is_some() {
            return match named {
                Some(key::Named::Escape) if self.picker.is_some() => self.close_picker(),
                Some(key::Named::Escape) => self.status_event(crate::status::Event::Cancel),
                _ => Task::none(),
            };
        }
        if self.popover.is_some() {
            return match named {
                Some(key::Named::Escape) => self.status_event(crate::status::Event::Close),
                _ => Task::none(),
            };
        }
        if self.popover_open() {
            return match named {
                Some(key::Named::Escape) => self.popover_event(crate::popovers::Event::Close),
                _ => Task::none(),
            };
        }
        if self.editor_open() {
            return match named {
                Some(key::Named::Escape) => self.editor_escape(),
                _ => Task::none(),
            };
        }
        if self.settings.is_some() {
            return match named {
                Some(key::Named::Escape) => self.settings_escape(),
                _ => Task::none(),
            };
        }
        if self.server_screen.is_some() {
            return match named {
                Some(key::Named::Escape) => self.server_settings_escape(),
                _ => Task::none(),
            };
        }
        if self.channel_screen.is_some() {
            return match named {
                Some(key::Named::Escape) => self.channel_settings_escape(),
                _ => Task::none(),
            };
        }
        if self.client.state().switch_voice.is_some() {
            return match named {
                Some(key::Named::Escape) => self.call_event(crate::call::Event::Switch(false)),
                Some(key::Named::Enter) => self.call_event(crate::call::Event::Switch(true)),
                _ => Task::none(),
            };
        }
        if self.client.state().person_dialog.is_some() {
            return self.person_key(named);
        }
        if self.client.state().reactions.is_some() {
            return match named {
                Some(key::Named::Escape) => self.reactions_event(crate::reactions::Event::Close),
                _ => Task::none(),
            };
        }
        if self.client.state().add_friend.is_some() {
            return match named {
                Some(key::Named::Escape) => self.friends_event(crate::friends::Event::CloseAdd),
                _ => Task::none(),
            };
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
        if self.keyed.is_some()
            && let Some(task) = self.browse_key(&key, modifiers)
        {
            return task;
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
        self.type_anywhere(&key, modifiers)
    }

    /// Escape where nothing else took it: the topmost thing open closes.
    pub fn escape(&mut self) -> Task<Message> {
        if self.inbox.is_some() {
            self.inbox = None;
            return Task::none();
        }
        if self.client.state().select_menu.is_some() {
            return self.select_event(crate::bot_select::Event::Close);
        }
        if self.client.state().forwarding.is_some() {
            return self.forward_event(crate::forward::Event::Close);
        }
        if self.menu.is_some() {
            self.menu = None;
            return self.refocus();
        }
        if self.client.state().switch_voice.is_some() {
            return self.call_event(crate::call::Event::Switch(false));
        }
        if self.call.popup.is_some() {
            self.close_popup();
            return self.refocus();
        }
        if self.clip_full {
            return self.clip_event(crate::playback::Clip::Full(false));
        }
        if self.full_shown() {
            return self.call_event(crate::call::Event::Full(false));
        }
        if self.picker.is_some() {
            return self.picker_event(picker::Event::Field(field::Event::Escape));
        }
        if self.cards.server.is_some() {
            return self.info_event(crate::info_cards::Event::CloseServer);
        }
        if self.cards.expression.is_some() {
            return self.info_event(crate::info_cards::Event::CloseExpression);
        }
        if self.card.is_some() || self.profiles.shown.is_some() {
            return self.update(Message::CloseProfile);
        }
        if self.client.state().role.is_some() {
            return self.info_event(crate::info_cards::Event::CloseRole);
        }
        if self.selection.is_some() {
            self.selection = None;
            return Task::none();
        }
        if self.forum_post.tags_menu {
            return self.forum_event(crate::forum::Event::TagsMenu(false));
        }
        // Then in Discord's order: the search, a reply or an edit.
        if self.search_shown() {
            return self.search_event(crate::search::Event::Clear);
        }
        if self.editing.is_some() {
            self.client.dispatch(Action::CancelEdit);
            return Task::batch([self.sync(), operation::focus(crate::composer::COMPOSER)]);
        }
        if self.client.state().compose().is_some() {
            self.client.dispatch(Action::CancelCompose);
            return self.sync();
        }
        // Then the thread beside's, as the Slint client's
        // `escape-composing`: its reply, or the New Thread form.
        if self.aside_shown() {
            if self.client.state().new_thread().is_some() {
                return self.thread_event(crate::thread::Event::CloseForm);
            }
            if self.client.state().aside_compose().is_some() {
                return self.thread_event(crate::thread::Event::CancelReply);
            }
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
            keyboard::Event::ModifiersChanged(modifiers) => Some(Message::Modifiers(modifiers)),
            _ => None,
        });
        let fonts = Subscription::run(fonts::wakes).map(|()| Message::FontsWanted);
        let drops = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(event) => crate::uploads::drops(&event),
            _ => None,
        });
        // What the overlays are placed by: the window's size as it is,
        // not as it was asked for.
        let sizes = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(
                iced::window::Event::Resized(size) | iced::window::Event::Opened { size, .. },
            ) => Some(Message::Resized(size)),
            _ => None,
        });
        // Ctrl with + or - zooms wherever the keys are.
        let zooms = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }) if modifiers.control() && !modifiers.alt() => Some(Message::Zoom(key, physical_key)),
            _ => None,
        });
        let mut subscriptions = vec![
            keys,
            fonts,
            drops,
            sizes,
            zooms,
            crate::window_bar::history_keys(),
            Self::shell_subscription(),
        ];
        // A look at the releases every six hours while the check is on.
        if self.layout.prefs.check_for_updates {
            subscriptions.push(iced::time::every(crate::settings::updates::EVERY).map(|_| {
                Message::Settings(crate::settings::Event::Updates(
                    crate::settings::updates::Event::Due,
                ))
            }));
        }
        if self.updates_installing() {
            subscriptions.push(iced::time::every(Duration::from_millis(200)).map(|_| {
                Message::Shell(crate::shell::Event::Heard(crate::desktop::Heard::Updates))
            }));
        }
        // Ctrl+K opens the quick switcher wherever the keys are, as the
        // Slint client's does.
        if self.workspace() {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character(c),
                    modifiers,
                    repeat: false,
                    ..
                }) if c.as_str() == "k"
                    && modifiers.control()
                    && !modifiers.alt()
                    && !modifiers.shift() =>
                {
                    Some(Message::Switcher(crate::switcher::Event::Toggle))
                }
                _ => None,
            }));
        }
        // A dialog's field keeps Escape to itself, only letting go of the
        // keys: while a dialog with one is open, Escape closes the dialog,
        // as the Slint client's do.
        let state = self.client.state();
        if state.add_friend.is_some()
            || state.person_dialog.is_some()
            || state.forwarding.is_some()
            || self.settings.is_some()
            || self.server_screen.is_some()
            || self.channel_screen.is_some()
            || state.profile_editor.is_some()
            || state.status_editor.is_some()
            || state.new_dm.is_some()
            || state.poll_draft.is_some()
            || state.switcher.is_some()
            || self.server_dialog_open()
            || self.link_dialog_open()
            || self.popover_open()
            || self.search_shown()
            || self.forum_post.tags_menu
            || state.interaction_modal.is_some()
        {
            subscriptions.push(iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: key @ keyboard::Key::Named(key::Named::Escape),
                    modifiers,
                    ..
                }) if status == iced::event::Status::Captured => Some(Message::Key(key, modifiers)),
                _ => None,
            }));
        }
        // Busy voice channels show how long their call has gone on, to the
        // minute.
        // Open polls, how long they have left.
        if self
            .side
            .iter()
            .any(|row| matches!(row, crate::sidebar::SideRow::Room(room) if room.started.is_some()))
            || crate::polls::open_poll(&self.messages, self.minute)
            || !self.activity_cards.is_empty()
            || self.active_now_minutes()
            || self.popover_playing()
        {
            subscriptions.push(iced::time::every(Duration::from_secs(15)).map(|_| Message::Minute));
        }
        // A shown activity's time, each second.
        if self.profile_ticks() || self.active_now_ticks() || self.invites_ticking() {
            subscriptions.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Second));
        }
        // The microphone's level while the input menu shows it, the call's
        // clock, and the corner picture's drag.
        if self.metering() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(50))
                    .map(|_| Message::Call(crate::call::Event::Meter)),
            );
        }
        // Settings: the gate's meter while it shows, the push-to-talk key
        // while it's recorded.
        if self.gate_metered() {
            subscriptions.push(
                iced::time::every(crate::settings::voice::METER_EVERY).map(|_| {
                    Message::Settings(crate::settings::Event::Voice(
                        crate::settings::voice::Event::Meter,
                    ))
                }),
            );
        }
        if self.recording_talk_key() {
            subscriptions.push(crate::settings::voice::record_keys());
        }
        if self.call.since.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_secs(1))
                    .map(|_| Message::Call(crate::call::Event::Second)),
            );
        }
        subscriptions.extend(self.arrange_subscription());
        subscriptions.extend(self.follow_subscription());
        subscriptions.push(self.away_subscription());
        subscriptions.extend(crate::settings::voice::window_talk_keys());
        if self.call.grab.is_some() {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::Call(crate::call::Event::DragTo(position)))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::Call(crate::call::Event::Drop)),
                _ => None,
            }));
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
        // Whatever this returns, what it no longer shows goes at its end.
        let _viewing = self.pictures.view();
        if !self.workspace() {
            return self.frame_layers(self.sign_in_view()).into();
        }
        // The window's own background is the theme's chat (`window_style`).
        // The rail and the sidebar on the rail's ground, the account's card
        // across the foot of both.
        let rail = self.theme.rail;
        let mut foot = column![];
        if let Some(dock) = self.dock_view() {
            foot = foot.push(dock);
        }
        let navigation = container(column![
            row![self.rail_view(), self.sidebar_view()].height(Length::Fill),
            foot.push(self.account_view()),
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
        // A role's card, under the card of someone on it.
        if let Some(role) = self.role_card_layers() {
            layers = layers.extend(role);
        }
        if let Some(card) = self.card_view() {
            layers = layers
                .push(outside(Message::CloseProfile, false))
                .push(card);
        }
        // The picker for a dialog's emoji opens over the dialog.
        let mut over_dialogs = None;
        if let Some(picker) = &self.picker
            && let Some(shown) = self.picker_view()
        {
            let size = picker.size();
            let place = match &picker.target {
                // Over the emoji button, its right edge level with the button's.
                Target::Composer => {
                    let right = self.chat_right() - 8.0 - 4.0;
                    let bottom = self.composer_buttons_top() - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                // Over the edit's field, at the timeline's right.
                Target::Edit => {
                    let right = self.chat_right() - 16.0;
                    let bottom = self.edit_top().unwrap_or(self.composer_top()) - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                Target::React(_, at) | Target::Status(at) | Target::Poll(_, at) => {
                    Place::new(shown, *at, 8.0).anchor(Anchor::Pointer)
                }
            };
            if matches!(picker.target, Target::Status(_) | Target::Poll(..)) {
                over_dialogs = Some(place);
            } else {
                layers = layers
                    .push(outside(Message::Picker(picker::Event::Close), false))
                    .push(place);
            }
        }
        for layer in self.call_layers() {
            layers = layers.push(layer);
        }
        if let Some(viewer) = self.clip_viewer() {
            layers = layers.push(viewer);
        }
        if let Some(popup) = self.call_popup_view() {
            layers = layers
                .push(outside(Message::Call(crate::call::Event::Close), false))
                .push(popup);
        }
        if let Some(settings) = self.settings_view() {
            layers = layers.push(settings);
        }
        if let Some(settings) = self.server_settings_view() {
            layers = layers.push(settings);
        }
        if let Some(settings) = self.channel_settings_view() {
            layers = layers.push(settings);
        }
        // Over the settings, as a member's from Members.
        if let Some(full) = self.full_profile_view() {
            layers = layers.push(full);
        }
        if let Some(editor) = self.profile_editor_view() {
            layers = layers.push(editor);
        }
        // A mutual server's, over the profile that lists it.
        if let Some(server) = self.server_card_layers() {
            layers = layers.extend(server);
        }
        if let Some(expression) = self.expression_card_layers() {
            layers = layers.extend(expression);
        }
        if let Some(popover) = self.popover_view() {
            layers = layers
                .push(outside(Message::Status(crate::status::Event::Close), false))
                .push(popover);
        }
        if let Some(dialog) = self.status_dialog_view() {
            layers = layers.push(dialog);
        }
        for dialog in [
            self.new_dm_view(),
            self.poll_draft_view(),
            self.switcher_view(),
            self.server_dialog_view(),
            self.link_dialog_view(),
        ]
        .into_iter()
        .flatten()
        {
            layers = layers.push(dialog);
        }
        if let Some(place) = over_dialogs {
            layers = layers
                .push(outside(Message::Picker(picker::Event::Close), false))
                .push(place);
        }
        if let Some(popover) = self.popover_layer() {
            layers = layers.push(popover);
        }
        if let Some(modal) = self.modal_view() {
            layers = layers.push(modal);
        }
        if let Some(viewer) = self.image_viewer() {
            layers = layers.push(viewer);
        }
        if let Some(menu) = self.menu_view() {
            layers = layers.push(outside(Message::CloseMenu, false)).push(menu);
        }
        if let Some(window) = self.reactions_view() {
            layers = layers.push(window);
        }
        if let Some(modal) = self.delete_view() {
            layers = layers.push(modal);
        }
        if let Some(dialog) = self.person_view() {
            layers = layers.push(dialog);
        }
        if let Some(dialog) = self.add_friend_view() {
            layers = layers.push(dialog);
        }
        if let Some(forward) = self.forward_view() {
            layers = layers.push(forward);
        }
        if let Some(leaving) = self.leaving_view() {
            layers = layers.push(leaving);
        }
        if let Some(drop) = self.drop_view() {
            layers = layers.push(drop);
        }
        if let Some(prompt) = self.mfa_view() {
            layers = layers.push(prompt);
        }
        let content = crate::widgets::RightPress::spot(
            layers.width(Length::Fill).height(Length::Fill),
            &self.pressed,
        );
        let content = crate::widgets::CtrlWheel::new(content, Message::ZoomStep);
        // A middle click scrolls the list under it towards the pointer.
        let theme = &self.theme;
        let arrow = |icon| self.icon(icon, 10, crate::theme::rgb(theme.secondary));
        let content = crate::widgets::Autoscroll::new(
            content,
            &[crate::timeline::TIMELINE, crate::thread::LIST],
            crate::widgets::AutoscrollLook {
                ground: theme.raised,
                edge: theme.line,
                dot: theme.secondary,
                up: arrow(crate::pictures::Icon::ChevronUp),
                down: arrow(crate::pictures::Icon::ChevronDown),
            },
        );
        self.frame_layers(content).into()
    }

    /// The window's content under the title bar, when Yapfast draws one,
    /// where it takes the window's place (`self.window` is its size), and
    /// over it the zoom level's note, the inbox, the bar and the edges that
    /// resize the window.
    fn frame_layers<'a>(&'a self, content: impl Into<Element<'a, Message>>) -> Stack<'a, Message> {
        let mut layers = stack![crate::widgets::Below::new(content, self.bar_height())];
        if let Some(note) = self.zoom_note_view() {
            layers = layers.push(note);
        }
        if let Some(inbox) = self.inbox_view() {
            layers = layers.push(inbox);
        }
        if let Some(bar) = self.window_bar() {
            layers = layers.push(column![bar, Space::new().height(Length::Fill)]);
        }
        if let Some(edges) = self.resize_edges() {
            layers = layers.push(edges);
        }
        layers
    }

    /// The zoom level, a moment after it changes, as the Slint client's.
    fn zoom_note_view(&self) -> Option<Element<'_, Message>> {
        self.shell.zoom_note?;
        let theme = &self.theme;
        let tip = theme.tooltip;
        let note = container(
            iced::widget::text(format!("{:.0}%", self.shell.zoom * 100.0))
                .size(15)
                .font(fonts::BOLD)
                .color(theme.text),
        )
        .padding([0, 14])
        .height(36)
        .align_y(iced::Alignment::Center)
        .style(move |_| iced::widget::container::Style {
            shadow: crate::style::raised_shadow(),
            ..crate::style::rounded(tip, 18.0)
        });
        Some(
            column![
                Space::new().height(self.bar_height() + ZOOM_NOTE_TOP),
                container(note).center_x(Length::Fill)
            ]
            .into(),
        )
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
        self.show_theme(theme);
        true
    }

    /// Shows the window in `theme` without keeping it, as the theme
    /// editor shows what's edited.
    pub fn show_theme(&mut self, theme: Theme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        self.pictures.marks.borrow_mut().clear();
        self.show_roster(false);
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
