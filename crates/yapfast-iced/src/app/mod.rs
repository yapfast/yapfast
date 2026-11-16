//! The window's state and what each message does to it. Its messages are
//! in `message`, what's open over it in `layers`, what it listens to in
//! `subscription`, and the layers the view stacks in `view`.

use std::{sync::Arc, time::Duration};

use iced::{Point, Size, Task, keyboard, widget::operation};
use yapfast_core::application::Action;

use crate::{
    after,
    client::Client,
    mfa,
    signin::{self, SignIn},
    sync::Scroll,
    theme::Theme,
    themes::Themes,
    video,
};

pub(crate) mod layers;
mod message;
mod screens;
mod settle;
mod subscription;
mod view;
mod window;

pub use window::{Keep, Window};

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
    /// What lives only as long as the window, torn down with it.
    pub win: Window,
    /// What's under the pointer, part by part.
    pub pointed: Pointed,
    /// What the window keeps in `layout.json`: open folders, collapsed
    /// categories.
    pub layout: crate::layout::Layout,
    /// The thread beside's width as its edge was pressed, while it's dragged.
    pub aside_drag: Option<f32>,
    /// The minute now, as the clocks that show minutes read it, and the
    /// timers to the next minute and the next midnight, while armed.
    pub minute: u64,
    /// What the conversation's header shows of the open room.
    pub header: crate::header::Header,
    /// The channel's timeline: its rows, where it's scrolled, the selection.
    pub timeline: crate::timeline::Timeline,
    /// The server rail, the sidebar beside it and the member list.
    pub rail: crate::rail::Rail,
    pub sidebar: crate::sidebar::Sidebar,
    pub roster: crate::members::Roster,
    pub selected: Option<String>,
    /// What the rail, the sidebar, the header and the history's edge were
    /// made from last ([`crate::sync::lists_key`]); none to make them again.
    pub lists_shown: Option<u64>,
    /// A server is open, and the open room is in one.
    pub server: bool,
    pub in_server: bool,
    /// The account's card, and whether the pointer is on its names.
    pub account: crate::account::Account,
    pub account_pointed: bool,
    /// The composer, the edit in its row and the suggestions over them.
    pub composer: crate::composer::Composer,
    /// Files waiting to go and dragged over the window.
    pub uploads: crate::uploads::Uploads,
    /// The keys held with a click: Shift puts a GIF in the composer.
    pub modifiers: keyboard::Modifiers,
    /// The message added to one being forwarded.
    pub forward: crate::forward::Forward,
    /// The confirmation's focused button: Delete (true) or Cancel.
    pub confirm_focus: bool,
    /// Whether a batch of pictures to ask for is due; what was asked for
    /// is the core's to know (`Application::picture_requested`).
    pub asking: bool,
    pub reporting: bool,
    /// The message the keyboard is stepping on (`browse.rs`), by item.
    pub keyed: Option<String>,
    /// What the timeline's voice messages and videos show of their playing.
    pub playback: crate::playback::Playback,
    /// A masked link clicked, while the question before it shows.
    pub leaving: Option<String>,
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
    /// What the screens' updates asked of the root, and whether they
    /// changed what the window shows: `settle` takes them after each
    /// message.
    pub requests: Vec<crate::ctx::Request>,
    pub dirty: bool,
    /// Whether the message being handled asked for the keys to go
    /// somewhere, for [`App::focus_backstop`].
    focus_asked: bool,
    /// The popups open, and when each opened, for their motion.
    opening: layers::Opening,
    /// Whether Settings tried a camera, and how many times it started or
    /// stopped, when the videos were last placed for it.
    videos_for: (bool, u32),
    /// A server's settings' own state while they're open.
    pub server_screen: crate::slot::Slot<crate::server_settings::Screen>,
    /// What's chosen or typed in the invite, join and channel dialogs.
    pub server_dialogs: crate::server_dialogs::Dialogs,
    /// What's chosen in an event's, gift's or authorization's dialog.
    pub link_dialogs: crate::link_dialogs::LinkDialogs,
    /// Whether Discord heard nobody is using Yapfast.
    pub away: crate::away::Away,
    pub channel_screen: crate::slot::Slot<crate::channel_settings::Screen>,
    /// The account's popover, and the custom status's dialog's draft.
    pub status: crate::status::Status,
    /// The quick switcher's places and the one Enter goes to.
    pub quick: crate::switcher::Quick,
    pub build: crate::settings::Build,
    /// The keybinds' watcher, and whether the key being handled ends a
    /// push to talk keybind, which types nothing during a call.
    pub keybinds: crate::keybinds::Keybinds,
    pub talk_pressed: bool,
    /// The size the picture viewer opened at.
    pub viewer: crate::image_viewer::Viewed,
    /// The thread beside the channel, while it shows, the last message it
    /// had read, and the side composers' fields (`thread.rs`).
    pub aside: Option<Box<crate::thread::Aside>>,
    pub seen_aside: Option<String>,
    pub drafts: crate::thread::Drafts,
    pub popovers: crate::popovers::Popovers,
    pub modal: crate::modal::Shown,
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
    pub mfa: mfa::Mfa,
}

impl App {
    /// What views read besides their own state.
    /// The theme, icons and room the kit draws with.
    pub fn look(&self) -> crate::look::Look<'_> {
        crate::look::Look {
            theme: &self.theme,
            icons: &self.win.icons,
            window: self.window,
        }
    }

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
            win: Window::new(crate::pictures::Pictures::new(fitter)),
            pointed: Pointed::default(),
            layout,
            aside_drag: None,
            minute: yapfast_core::clock::now_ms() / 1000 / 60 * 60,
            header: Default::default(),
            timeline: crate::timeline::Timeline::new(
                height,
                width - crate::conversation::NAVIGATION,
            ),
            rail: crate::rail::Rail::new(height),
            sidebar: crate::sidebar::Sidebar::new(height),
            roster: crate::members::Roster::new(height),
            selected: None,
            lists_shown: None,
            server: false,
            in_server: false,
            account: Default::default(),
            account_pointed: false,
            composer: Default::default(),
            uploads: Default::default(),
            modifiers: keyboard::Modifiers::default(),
            forward: Default::default(),
            confirm_focus: true,
            asking: false,
            reporting: false,
            keyed: None,
            playback: Default::default(),
            leaving: None,
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
            requests: Vec::new(),
            dirty: false,
            focus_asked: false,
            opening: Default::default(),
            videos_for: (false, 0),
            server_screen: Default::default(),
            server_dialogs: Default::default(),
            link_dialogs: Default::default(),
            away: Default::default(),
            channel_screen: Default::default(),
            status: Default::default(),
            quick: Default::default(),
            build: crate::build(),
            keybinds: Default::default(),
            talk_pressed: false,
            viewer: Default::default(),
            aside: None,
            seen_aside: None,
            drafts: Default::default(),
            popovers: Default::default(),
            modal: Default::default(),
            searching: Default::default(),
            forum_post: Default::default(),
            theme,
            themes,
            sign_in: SignIn::default(),
            backend_events: None,
            linking: None,
            mfa: mfa::Mfa::default(),
        };
        app.fit_window();
        app.follow_popups();
        let talk = app.watch_keybinds();
        if app.layout.voice.noise_suppression != yapfast_core::noise::Suppressor::Off {
            app.call.noise_last = app.layout.voice.noise_suppression;
        }
        // Started on a saved account: see `SignIn::starting`.
        app.sign_in.starting = !app.workspace() && app.client.state().busy;
        let frames = Task::batch([
            video::follow(app.video.clone(), Message::Frames),
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
        // What's on top, only while the backstop's log is read.
        let top = tracing::enabled!(tracing::Level::DEBUG).then(|| self.layers().next());
        self.focus_asked = false;
        let task = self.handle_message(message);
        let task = Task::batch([task, self.settle()]);
        if let Some(top) = top {
            self.focus_backstop(top);
        }
        self.follow_popups();
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
            Message::SignIn(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.sign_in.update(event, &mut cx)
            }
            Message::Mfa(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.mfa.update(event, &mut cx)
            }
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
                self.timeline.latest_due = true;
                self.client.dispatch(Action::JumpToPresent);
                self.sync()
            }
            Message::JumpFaded(item) => {
                if self.timeline.jump_lit.as_ref() == Some(&item) {
                    self.timeline.jump_lit = None;
                }
                Task::none()
            }
            Message::Link(link, at) => self.follow_link(&link, at),
            Message::Members(viewport) => {
                self.roster.scroll = Scroll::of(viewport);
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
            Message::Forward(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.forward.update(event, &mut cx)
            }
            Message::Select(select) => {
                self.select(select);
                Task::none()
            }
            Message::SelectEnd => {
                self.select_end();
                Task::none()
            }
            Message::SelectClear => {
                self.timeline.selection = None;
                self.timeline.selecting = false;
                Task::none()
            }
            Message::RowMenu(item, at) => match self.row_of(&item) {
                Some(row) => self.open_menu(row, at),
                None => Task::none(),
            },
            Message::MenuLight(index) => self.menu_light(index),
            Message::SubLight(index) => {
                if let Some(menu) = &mut self.win.menu {
                    menu.sub_lit = Some(index);
                }
                Task::none()
            }
            Message::SubChoose(index) => self.choose_sub(index),
            Message::MenuChoose(index) => self.choose(index),
            Message::MenuScrolled(down) => {
                if let Some(menu) = &mut self.win.menu {
                    menu.scrolled = down;
                }
                Task::none()
            }
            Message::Act(act, item, at) => self.act(act, item, at, None),
            Message::CloseMenu => {
                self.win.menu = None;
                self.refocus()
            }
            Message::PersonMenu(user, at) => {
                self.open_person_menu(&user, at, crate::person::CONTEXT_MENU)
            }
            Message::Person(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.person.update(event, &mut cx)
            }
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
                self.roster.hidden = !self.roster.hidden;
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
                self.win.menu = None;
                self.client.dispatch(Action::React { item, key });
                Task::batch([self.sync(), self.refocus()])
            }
            Message::AskPictures => self.ask_now(),
            Message::Report => {
                self.reporting = false;
                self.report()
            }
            Message::Fitted(fitted) => {
                self.win.pictures.fitter.borrow_mut().made(fitted);
                Task::none()
            }
            Message::Reveal(item, attachment) => self.reveal(&item, attachment),
            Message::Unfold(item) => self.unfold(&item),
            Message::ShowBlocked(item) => {
                self.timeline
                    .revealed
                    .insert(crate::message::blocked_key(&item));
                self.ask()
            }
            Message::Poll(event) => self.poll_event(event),
            Message::Voice(event) => self.voice_event(event),
            Message::Clip(clip) => self.clip_event(clip),
            Message::Image(event) => self.image_event(event),
            Message::Thread(event) => self.thread_event(event),
            Message::Popover(event) => self.popover_event(event),
            Message::Modal(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.modal.update(event, &mut cx)
            }
            Message::Forum(event) => self.forum_event(event),
            Message::BrowseChannels(event) => self.browse_channels_event(event),
            Message::Search(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.searching.update(event, &mut cx)
            }
            Message::Focus(id) => operation::focus(id),
            Message::Leave(visit) => self.leave(visit),
            Message::BotSelect(event) => {
                let mut cx = crate::ctx::cx!(self);
                crate::bot_select::update(event, &mut cx)
            }
            Message::UseButton(item, custom_id) => {
                self.client.dispatch(Action::UseButton { item, custom_id });
                self.sync()
            }
            Message::Roster => {
                self.roster.waiting = false;
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
            Message::TagCard(event) => self.tag_card_event(event),
            Message::PopupGone(popup) => {
                self.popup_gone(popup);
                Task::none()
            }
            Message::Profile(event) => self.profile_event(event),
            Message::Friends(event) => self.friends_event(event),
            Message::Reactions(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.reactions.update(event, &mut cx)
            }
            Message::Frames(clip_time) => {
                self.frames_found(clip_time);
                Task::none()
            }
            Message::Call(event) => self.call_event(event),
            Message::Settings(event) => self.settings_event(event),
            Message::Updates(event) => self.updates_event(event),
            Message::Keybinds(count, told) => self.keybinds_told(count, told),
            Message::ServerSettings(event) => self.server_settings_event(event),
            Message::Resized(size) => {
                self.shell.frame = size;
                self.fit_zoom();
                let fitted = self.ctx().active_now_fits();
                self.fit_window();
                if self.ctx().active_now_fits() == fitted {
                    return self.read_window_state();
                }
                self.active_now_sync();
                Task::batch([self.read_window_state(), self.ask()])
            }
            Message::ServerDialog(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.server_dialogs.update(event, &mut cx)
            }
            Message::LinkDialog(event) => {
                let mut cx = crate::ctx::cx!(self);
                self.link_dialogs.update(event, &mut cx)
            }
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
