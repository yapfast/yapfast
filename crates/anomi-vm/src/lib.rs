//! What the screens of the iced interface show, and what they ask for.
//!
//! Each area of the window has a module here: its view-model, the plain data
//! its screen draws, and its `Intent`, what a click or a keystroke on it asks.
//! `anomi-app` fills the view-models from the core's `AppState` (as
//! `src/ui/refresh.rs` fills the Slint window) and turns intents into the
//! core's `Action`s. Nothing here knows the core, so the screens type-check
//! without it.
//!
//! An area's module owns its types; a type two areas share goes in [`kind`].
//! The larger areas group their modules in folders ([`sidebar`], [`places`],
//! [`chat`], [`people`], [`voice`], [`settings`]), each with the fields and
//! variants of its modules wired by task F01, so a task fills only its own
//! module. See docs/iced/README.md.

/// Lets a view's `.map(Intent::from)` lift a part's intent into its parent's
/// and on into the window's: `parts!(Intent in Chat: Timeline(timeline::Intent))`
/// implements `From<timeline::Intent>` for this module's `Intent` (as
/// `Intent::Timeline`) and for [`Intent`] (as `Intent::Chat(..)`). Without
/// `in`, it implements only the first.
macro_rules! parts {
    ($whole:ident: $($variant:ident($part:ty)),* $(,)?) => {
        $(impl From<$part> for $whole {
            fn from(intent: $part) -> Self {
                Self::$variant(intent)
            }
        })*
    };
    ($whole:ident in $top:ident: $($variant:ident($part:ty)),* $(,)?) => {
        $crate::parts!($whole: $($variant($part)),*);
        $(impl From<$part> for $crate::Intent {
            fn from(intent: $part) -> Self {
                Self::$top($whole::$variant(intent))
            }
        })*
    };
}
pub(crate) use parts;

pub mod account;
pub mod browse;
pub mod chat;
pub mod chrome;
pub mod conversation;
pub mod header;
pub mod inbox;
pub mod kind;
pub mod menu;
pub mod new_dm;
pub mod people;
pub mod places;
pub mod rail;
pub mod settings;
pub mod shell;
pub mod sidebar;
pub mod sign_in;
pub mod switcher;
pub mod text;
pub mod voice;

#[cfg(feature = "fixtures")]
pub mod fixtures;

/// Everything the main window shows, by area. A field's type is its area
/// module's; it derives `Clone`, `Debug` and `Default`, which every area's
/// type keeps.
#[derive(Clone, Debug, Default)]
pub struct Vm {
    pub shell: shell::Shell,
    pub chrome: chrome::Chrome,
    pub menu: menu::Menu,
    pub rail: rail::Rail,
    pub sidebar: sidebar::Sidebar,
    pub account: account::Account,
    pub conversation: conversation::Conversation,
    pub header: header::Header,
    pub inbox: inbox::Inbox,
    pub sign_in: sign_in::SignIn,
    pub switcher: switcher::Switcher,
    pub new_dm: new_dm::NewDm,
    pub places: places::Places,
    pub browse: browse::Browse,
    pub chat: chat::Chat,
    pub people: people::People,
    pub voice: voice::Voice,
    pub settings: settings::Settings,
}

/// What a screen asks for, by area. `anomi-app` routes each to the area's
/// handler, which changes the view-model's own state or dispatches a core
/// `Action`. Every area's `Intent` converts into it with `From`, so a parent
/// maps a part's view with `.map(Intent::from)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Shell(shell::Intent),
    Chrome(chrome::Intent),
    Menu(menu::Intent),
    Rail(rail::Intent),
    Sidebar(sidebar::Intent),
    Account(account::Intent),
    Header(header::Intent),
    Inbox(inbox::Intent),
    SignIn(sign_in::Intent),
    Switcher(switcher::Intent),
    NewDm(new_dm::Intent),
    Places(places::Intent),
    Browse(browse::Intent),
    Chat(chat::Intent),
    People(people::Intent),
    Voice(voice::Intent),
    Settings(settings::Intent),
}

parts!(Intent:
    Shell(shell::Intent),
    Chrome(chrome::Intent),
    Menu(menu::Intent),
    Rail(rail::Intent),
    Sidebar(sidebar::Intent),
    Account(account::Intent),
    Header(header::Intent),
    Inbox(inbox::Intent),
    SignIn(sign_in::Intent),
    Switcher(switcher::Intent),
    NewDm(new_dm::Intent),
    Places(places::Intent),
    Browse(browse::Intent),
    Chat(chat::Intent),
    People(people::Intent),
    Voice(voice::Intent),
    Settings(settings::Intent),
);
