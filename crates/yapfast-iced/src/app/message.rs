//! The messages the window's parts send, and the backend's events in
//! batches.

use std::sync::{Arc, Mutex};

use iced::{Point, Size, keyboard, widget::scrollable::Viewport};
use yapfast_core::chat::{ChatMessage, Event};

use crate::{
    mfa,
    picker::{self, Target},
    select::Select,
    signin,
};

/// The open timeline's messages, as the core shares them.
pub type Messages = Arc<Vec<Arc<ChatMessage>>>;

/// Events the backend sent, taken out once by `update`: the core's events
/// aren't `Clone`, which a message must be.
pub type Batch = Arc<Mutex<Vec<Event>>>;

/// The most backend events applied together before the window takes them
/// in, as the Slint client batches them.
const EVENT_BATCH: usize = 64;

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
    /// The minute turned, for the clocks that show minutes: busy voice
    /// channels', open polls', the activity cards'.
    Minute,
    /// The day turned: today's message times become yesterday's.
    Day,
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
    /// A right click on a message, by its item, where the pointer is.
    RowMenu(Arc<str>, Point),
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
    /// A key nothing in the window took; as the window hands it on, with
    /// where it is on the keyboard, which tells push to talk's key.
    Key(keyboard::Key, keyboard::Modifiers),
    Pressed(keyboard::Key, keyboard::Modifiers, keyboard::key::Physical),
    /// Shift, Ctrl, Alt or the logo key went down or up.
    Modifiers(keyboard::Modifiers),
    /// Fallback fonts the shaper wanted are in (`fonts::load_wanted`): the
    /// view is built and its text laid out again.
    FontsLoaded,
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
    /// message by its item, the attachment's index.
    Reveal(Arc<str>, usize),
    /// A long message's Show more or Show less, by its item.
    Unfold(Arc<str>),
    /// A blocked person's message, shown when asked for, by its item.
    ShowBlocked(Arc<str>),
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
    PointRow(Arc<str>, bool),
    /// A member's line was clicked, by their id.
    OpenMember(String),
    /// A message's avatar or author was clicked, by the message's item.
    OpenAuthor(Arc<str>),
    CloseProfile,
    /// Something done on a role's or a mutual server's card (`info_cards.rs`).
    Info(crate::info_cards::Event),
    /// Something done on a profile: the card, the full one, the DM's panel.
    Profile(crate::profile::Event),
    /// Something done on the friends list, the requests or Add Friend.
    Friends(crate::friends::Event),
    /// Something done in the Reactions window.
    Reactions(crate::reactions::Event),
    /// New video frames the view shows differently: a source's first, or
    /// a message's video's, with how far it's got (milliseconds).
    Frames(Option<u64>),
    /// Something done to the call or in it (`call/`).
    Call(crate::call::Event),
    /// Something done in Settings (`settings/`).
    Settings(crate::settings::Event),
    /// The update check's, which runs with Settings closed.
    Updates(crate::updates::Event),
    /// The keybinds' watcher told where they work, or of a toggling one's
    /// press, by the watch's count.
    Keybinds(u64, crate::keybinds::Told),
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
    ZoomStep(yapfast_core::zoom::Step),
    /// The window's own title bar (`window_bar.rs`).
    Bar(crate::window_bar::Event),
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
