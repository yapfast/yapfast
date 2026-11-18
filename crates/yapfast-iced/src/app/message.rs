//! The messages the window's parts send, and the backend's events in
//! batches.

use std::sync::{Arc, Mutex};

use iced::{Point, keyboard};
use yapfast_core::chat::{ChatMessage, Event};

use crate::{
    mfa,
    picker::{self, Target},
    piece::Select,
    signin,
};

/// The open timeline's messages, as the core shares them.
pub type Messages = Arc<Vec<Arc<ChatMessage>>>;

/// Events the backend sent, taken out once by `update`: the core's events
/// aren't `Clone`, which a message must be.
pub type Batch = Arc<Mutex<Vec<Event>>>;

/// The most backend events applied together before the window takes them
/// in.
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
    /// The timeline: its scrolling, its rows and what's done on them
    /// (`timeline/`).
    Timeline(crate::timeline::Event),
    /// The core's notice over the conversation, dismissed.
    DismissNotice,
    /// What a person did with something a plugin drew: a slot's cell, a
    /// notice's close button or a menu row.
    #[cfg(feature = "plugins")]
    Plugin(yapfast_core::plugins::Input),
    /// A link in the timeline's text, and where the pointer is.
    Link(String, Point),
    /// The member list and its Activity panel (`members.rs`).
    Members(crate::members::Event),
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
    /// The open menu, its submenu and a message's hover bar (`menus.rs`).
    Menu(crate::menus::Event),
    /// Someone's menu, at the pointer, by their id; the person dialog.
    PersonMenu(String, Point),
    Person(crate::person_menu::Event),
    /// The delete confirmation's answers, and the masked link question's
    /// (`dialogs.rs`).
    Dialog(crate::dialogs::Event),
    /// A key nothing in the window took; as the window hands it on, with
    /// where it is on the keyboard, which tells push to talk's key.
    Key(keyboard::Key, keyboard::Modifiers),
    Pressed(keyboard::Key, keyboard::Modifiers, keyboard::key::Physical),
    /// Shift, Ctrl, Alt or the logo key went down or up.
    Modifiers(keyboard::Modifiers),
    /// Fallback fonts the shaper wanted are in (`fonts::load_wanted`): the
    /// view is built and its text laid out again.
    FontsLoaded,
    /// Nothing to do: a click a backdrop takes.
    Nothing,
    /// The header's bell: the open channel's notification choices.
    Bell,
    AskPictures,
    Report,
    /// A copy of a picture at the size it shows at was made.
    Fitted(crate::pictures::Fitted),
    /// A poll's card.
    Poll(crate::polls::Event),
    /// A voice message's card.
    Voice(crate::playback::Event),
    /// A video's cell asked for something.
    Clip(crate::playback::Clip),
    /// An app's select menu: opened, an option toggled, searched, closed.
    BotSelect(crate::bot_select::Event),
    /// Something done on a role's or a mutual server's card (`info_cards.rs`).
    Info(crate::info_cards::Event),
    /// Something done on a server tag's card (`tag_card.rs`).
    TagCard(crate::tag_card::Event),
    /// A popup closed has been drawn going (`widgets::motion::leave`).
    PopupGone(crate::app::layers::Popup),
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
    /// A message's picture large, or its attachment saved
    /// (`image_viewer.rs`).
    Image(crate::image_viewer::Event),
    /// The thread beside the channel, or the New Thread form (`thread.rs`).
    Thread(crate::thread::Event),
    Popover(crate::header_popovers::Event),
    Modal(crate::bot_modal::Event),
    Forum(crate::forum::Event),
    BrowseChannels(crate::browse_channels::Event),
    Search(crate::search::Event),
    /// The keys go to the field with this id.
    Focus(&'static str),
    /// The window's frame and size, the tray, notifications, later starts
    /// and the zoom (`shell.rs`).
    Shell(crate::shell::Event),
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
