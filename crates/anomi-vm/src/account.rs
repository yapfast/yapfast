//! The account card at the bottom of the sidebar (who is signed in, and the
//! microphone, headphones and settings buttons) and the popover it opens
//! above itself (the account's profile card, with the status submenu).
//! The Slint side: the account card of `ui/yapfast.slint:3014`,
//! `AccountPopover` (`ui/yapfast/account-popover.slint`), filled by
//! `show_account` in `src/ui/refresh.rs`. Task N06's; the screen is
//! `anomi_screens::account`.

use iced::Color;
use iced::widget::image;

use crate::kind::{Avatar, NameStyle, Nameplate, Presence, Status, Tag};
use crate::menu::Spot;
use crate::people::badges::Badges;

/// The card, and what its popover shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub name: String,
    pub avatar: Avatar,
    /// The decoration round the avatar (`AccountLook.decoration-url`).
    pub decoration: Option<image::Handle>,
    /// Their custom status or what they're doing, else their presence, as
    /// one line: the custom emoji's picture, and for a game the small icon
    /// before it (`account-activity-icon`); `bullet` puts a dot after the
    /// icon when a custom status follows it.
    pub status: Status,
    pub bullet: bool,
    /// Their username, which the card shows in place of the status while
    /// the pointer is on it. Empty keeps the status.
    pub username: String,
    /// Their id, which the popover copies. Empty leaves "Copy User ID" out.
    pub user_id: String,
    /// The nameplate behind the card, hidden when nameplates are off.
    pub plate: Nameplate,
    /// A Discord account is linked: the avatar and name open the popover.
    pub linked: bool,
    /// Discord's own status for the dot; none draws the sync's dot instead:
    /// the accent in the local preview, green online, grey offline.
    pub presence: Presence,
    pub preview: bool,
    pub online: bool,
    /// Speaking in the call, unmuted: a ring round the avatar.
    pub speaking: bool,
    /// The card's buttons as they show: the call's while in one, else how
    /// the next call is joined.
    pub muted: bool,
    pub deafened: bool,
    pub in_call: bool,
    /// The call is ready for them (`voice-ready`); outside one the buttons
    /// always work.
    pub ready: bool,
    /// The input and output menus are open: their carets turn over.
    pub input_open: bool,
    pub output_open: bool,
    /// Offline: a button to reconnect, unless one is already going.
    pub busy: bool,
    /// A call's dock sits over the card: its top corners go square.
    pub docked: bool,
    pub popover: Popover,
}

impl Account {
    /// The reconnect button shows: signed in for real and not online.
    pub fn offline(&self) -> bool {
        !self.preview && !self.online
    }
}

/// The popover above the card: Discord's own card of the account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Popover {
    pub choice: Choice,
    /// The four statuses show below the current one.
    pub expanded: bool,
    /// The custom status, when one is set: the card shows it with its emoji
    /// (never the game's icon), and the button changes its words.
    pub custom: Option<Status>,
    pub game: Option<Game>,
    /// The account's own profile, once it came.
    pub profile: Profile,
}

/// The status they chose, which the dot and the row show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Online,
    Idle,
    DoNotDisturb,
    Invisible,
}

impl Choice {
    pub const ALL: [Self; 4] = [
        Self::Online,
        Self::Idle,
        Self::DoNotDisturb,
        Self::Invisible,
    ];

    /// What its dot shows: an invisible account looks offline.
    pub fn presence(self) -> Presence {
        match self {
            Self::Online => Presence::Online,
            Self::Idle => Presence::Idle,
            Self::DoNotDisturb => Presence::DoNotDisturb,
            Self::Invisible => Presence::Offline,
        }
    }
}

/// The game they play, and for how long ("2 hours"), empty before it is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub duration: String,
}

/// What of the account's profile the card draws: the banner, the colours of
/// its theme, its styled name, its server tag and its badges. All empty
/// until the profile comes, and then the card is plain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub banner: Option<image::Handle>,
    /// The banner's colour when there is no picture.
    pub accent: Option<Color>,
    /// The profile's theme, a Nitro setting: its two colours.
    pub theme: Option<[Color; 2]>,
    pub name_style: Option<NameStyle>,
    pub tag: Option<Tag>,
    pub badges: Badges,
}

/// What the card and its popover ask for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    ToggleMute,
    ToggleDeafen,
    /// The card's cog and the popover's "User Settings".
    Settings,
    /// Open the popover above the card (`open-status-menu`). The app asks
    /// for the account's profile as it opens (`account-opened`).
    Popover(Spot),
    /// The carets beside the microphone and the headphones: their device
    /// menus, from the button.
    InputMenu(Spot),
    OutputMenu(Spot),
    /// The card's reconnect button.
    Reconnect,
    /// Fold or unfold the four statuses.
    ToggleStatuses,
    Choose(Choice),
    CustomStatus,
    EditProfile,
    CopyId,
    OpenLink(String),
    /// The game's duration is due again: every 20 seconds while the
    /// popover is open and the window is in front, and as it comes back.
    GameDurationDue,
}
