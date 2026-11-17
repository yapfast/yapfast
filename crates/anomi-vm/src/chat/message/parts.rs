//! What a message holds besides its text: its author, the line over it,
//! the notice it may be, and the lines under it.

use iced::Color;
use iced::widget::image;

use crate::icon::Icon;
use crate::kind::{Avatar, NameStyle, Tag};

/// Who wrote a message, as its first line shows them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Author {
    /// Their name in the conversation: a server's nickname, or their own.
    pub name: String,
    pub avatar: Avatar,
    /// The decoration around their picture, when the profile switches show
    /// it (`decoration-url`).
    pub decoration: Option<image::Handle>,
    /// Their role's colour in a server; none draws the name in the text's.
    pub color: Option<Color>,
    /// How their name is drawn; in a server, its face alone.
    pub style: Option<NameStyle>,
    /// The server tag they wear by their name.
    pub tag: Option<Tag>,
    /// A Discord bot or webhook: "APP" after the name.
    pub bot: bool,
    /// Someone the app can show: their picture and name open their card
    /// and their menu. A blocked person isn't (`sender` empty).
    pub known: bool,
}

/// The line over a message.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Header {
    #[default]
    None,
    /// The message it replies to, quoted.
    Reply(Box<Reply>),
    /// An app's answer: who used which command (`has-command`).
    Command(Box<Command>),
}

impl Header {
    /// Whether a line stands over the message.
    pub fn shown(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// The message a message replies to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    /// Its author, quoted as "@Name"; empty when unknown, as for a message
    /// that's gone.
    pub author: String,
    pub avatar: Avatar,
    /// Their role's colour; none quotes them in the secondary text's.
    pub color: Option<Color>,
    /// What it said, on one line.
    pub body: String,
    /// It's loaded or can be: a click jumps to it (`reply-item`).
    pub jumps: bool,
}

/// The command an app's answer answers: "Ann used /ping on @Bob".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Command {
    /// Who used it.
    pub user: String,
    pub avatar: Avatar,
    pub color: Option<Color>,
    /// Someone the app can show: their name opens their card.
    pub known: bool,
    /// The command as Discord names it, "/ping".
    pub name: String,
    /// It's a slash command and the composer takes text here: a click
    /// writes it there (`command-slash`, `can-use-commands`).
    pub usable: bool,
    /// Whom it was used on, for a command on a person.
    pub target: Option<String>,
}

/// What a notice tells of, by the icon it shows in place of the picture,
/// as Discord's client marks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Pin,
    Join,
    Thread,
    Call,
    Poll,
}

impl Notice {
    pub fn icon(self) -> Icon {
        match self {
            Self::Pin => Icon::Pin,
            Self::Join => Icon::UserPlus,
            Self::Thread => Icon::Thread,
            Self::Call => Icon::Phone,
            Self::Poll => Icon::ChartBar,
        }
    }

    /// Someone joined or called: its icon is live green, not muted.
    pub fn live(self) -> bool {
        matches!(self, Self::Join | Self::Call)
    }
}

/// A message on its way, or one that didn't go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delivery {
    /// What's happening to it: "Sending…", "Uploading 40%", "Failed to
    /// send".
    pub text: String,
    pub failed: bool,
    /// Retry and Cancel beside the text.
    pub retry: bool,
    pub cancel: bool,
    /// Still going out with no files to upload: no line under it
    /// (`sending`).
    pub quiet: bool,
}

/// An app's answer still to come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thinking {
    /// "{author} is thinking…", until Discord updates it.
    Waiting,
    /// After 15 minutes: "The application did not respond", as Discord's
    /// client gives up (`timed-out`).
    TimedOut,
}

/// The thread a message started or tells of, as its chip shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    pub name: String,
    /// How many messages it has, as "12 Messages"; empty when unknown.
    pub detail: String,
}
