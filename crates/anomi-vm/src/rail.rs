//! The server rail: the DM home, the DMs with unread messages, then the
//! servers and their folders, and joining one. The Slint side:
//! `ui/rail.slint` (`ServerRail`, `RailItem`, `FolderItem`) and `rail`,
//! `rail_rows` and `server_row` in `src/ui/refresh.rs`.
//!
//! Servers are one list in the order the account keeps them. A server in a
//! folder names it in [`Server::folder`], and servers of one folder follow
//! each other, as the core lists them; the view puts the folder's tile
//! before the first of them, and shows the rest below it while it is
//! [`Folder::open`]. A folder's own [`Tile`] sums up its servers, as the
//! Slint rows did, since the counts are numbers the app has.

use iced::Color;

use crate::kind::Avatar;
use crate::menu::Spot;

#[derive(Clone, Debug, PartialEq)]
pub struct Rail {
    /// The DM home's tile: selected while DMs show.
    pub home: Tile,
    /// Unread DMs, above the servers: each opens its conversation.
    pub dms: Vec<Dm>,
    pub servers: Vec<Server>,
    /// The folders `servers` name, in any order.
    pub folders: Vec<Folder>,
    /// The "+" at the foot offers to join a Discord server.
    pub can_join: bool,
}

impl Default for Rail {
    fn default() -> Self {
        Self {
            home: Tile::default(),
            dms: Vec::new(),
            servers: Vec::new(),
            folders: Vec::new(),
            can_join: true,
        }
    }
}

/// What every rail item shows beside its picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub selected: bool,
    pub unread: bool,
    /// Mentions waiting, such as "3" or "99+"; empty for none.
    pub badge: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    /// The core's server id.
    pub id: String,
    /// Its name, for the tooltip.
    pub name: String,
    /// What the tooltip says when it isn't the name: "{} (unavailable in a
    /// Discord outage)". The app makes it, in the language.
    pub tip: String,
    /// Up to three initials, shown until its icon arrives.
    pub avatar: Avatar,
    pub tile: Tile,
    /// This account is in a call in one of its voice channels.
    pub call: bool,
    /// A Discord server, which shows Discord's mark while the setting for
    /// marks is on (the app reads it).
    pub discord: bool,
    /// Out of reach in a Discord outage, shown as last known, faded.
    pub unavailable: bool,
    /// The id of the folder it is in.
    pub folder: Option<String>,
}

/// A folder of servers.
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    /// The folder's own id.
    pub id: String,
    /// Its name for the tooltip: its own, or the first three of its
    /// servers' names joined by commas (the app makes it).
    pub name: String,
    /// Its colour, Discord's blurple where it has none.
    pub tint: Color,
    /// Its servers show below it.
    pub open: bool,
    /// One of its servers is selected, one is unread, and their mentions:
    /// all shown while it is closed.
    pub tile: Tile,
    /// In a call in one of its servers, shown while it is closed.
    pub call: bool,
    /// Its servers are Discord's, so it shows Discord's mark while the
    /// setting for marks is on (the app reads it).
    pub discord: bool,
}

/// Discord's colour for a folder that wasn't given one.
pub const FOLDER_COLOR: Color = Color::from_rgb(
    0x58 as f32 / 255.0,
    0x65 as f32 / 255.0,
    0xf2 as f32 / 255.0,
);

impl Default for Folder {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            tint: FOLDER_COLOR,
            open: false,
            tile: Tile::default(),
            call: false,
            discord: false,
        }
    }
}

/// An unread DM above the servers, as in Discord.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dm {
    /// Its room's id.
    pub id: String,
    pub name: String,
    /// The person's avatar or the group's icon, or initials.
    pub avatar: Avatar,
    /// Its unread messages, such as "3" or "99+".
    pub badge: String,
    /// A Discord DM.
    pub discord: bool,
}

/// What the rail asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Show the DMs.
    Home,
    /// Show a server, by id.
    Server(String),
    /// Open or close a folder, by id (`on_toggle_folder`).
    Folder(String),
    /// Open an unread DM, by its room's id.
    Dm(String),
    /// A menu for a server, folder or DM, by id, where it was asked for
    /// (`open-server-menu`).
    Menu(String, Spot),
    /// "Join a server".
    Join,
    /// The rows in view and a few beyond, as the list reports them: the ids
    /// of their servers, folders and DMs, whose pictures the app asks for.
    Seen(Vec<String>),
}
