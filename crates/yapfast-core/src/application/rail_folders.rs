//! What the rail offers around #320's drag of Discord servers and folders:
//! Undo for a while after a drop, which puts the server list back as it
//! was, and a folder's Folder Settings, its name and colour. Both send the
//! whole list as Discord's settings keep it.

use super::*;
use crate::chat::RailEntry;

/// Folder Settings, from a rail folder's menu, while it's open: the folder,
/// by its rail id, and the name and colour it had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderSettings {
    pub folder: String,
    pub name: String,
    pub color: Option<u32>,
}

/// The colours Folder Settings offers, as Discord's: its default first,
/// then the role colours, light and dark.
pub const FOLDER_COLORS: [u32; 21] = [
    0x5865f2, 0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c,
    0x95a5a6, 0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300,
    0x992d22, 0x979c9f, 0x546e7a,
];

impl Application {
    /// The rail's Discord servers as Discord's settings keep them: each
    /// folder with its servers, in its first server's place, and the rest
    /// one by one.
    pub fn rail_entries(&self) -> Vec<RailEntry> {
        let mut entries: Vec<RailEntry> = Vec::new();
        for server in self.state.servers.iter().filter(|server| server.discord) {
            match (&server.folder, entries.last_mut()) {
                (Some(folder), Some(last)) if last.folder.as_ref() == Some(folder) => {
                    last.servers.push(server.id.clone());
                }
                (folder, _) => entries.push(RailEntry {
                    folder: folder.clone(),
                    servers: vec![server.id.clone()],
                }),
            }
        }
        entries
    }

    /// Keeps the server list as it is now, for Undo to put back once the
    /// drop about to go out lands.
    pub(super) fn keep_for_undo(&mut self) {
        self.state.rail_undo = Some(self.rail_entries());
    }

    /// Puts the server list back as it was before the last drop, while the
    /// line offering it shows.
    pub(super) fn undo_rail_move(&mut self) -> Vec<Effect> {
        let Some(entries) = self.state.rail_undo.take() else {
            return vec![];
        };
        if self.state.preview || self.state.discord_user.is_none() {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ArrangeServers { entries },
        ))]
    }

    /// Opens Folder Settings for a rail folder, by its id.
    pub(super) fn open_folder_settings(&mut self, folder: String) -> Vec<Effect> {
        if self.state.preview || self.state.discord_user.is_none() {
            return vec![];
        }
        let Some(found) = self
            .rail_entries()
            .into_iter()
            .filter_map(|entry| entry.folder)
            .find(|known| known.id == folder)
        else {
            return vec![];
        };
        self.state.folder_settings = Some(FolderSettings {
            folder,
            name: found.name.unwrap_or_default(),
            color: found.color,
        });
        vec![]
    }

    /// Saves Folder Settings' name, none when blank, and colour, none for
    /// the default, in the server list Discord keeps, and closes it.
    pub(super) fn save_folder_settings(&mut self, name: String, color: Option<u32>) -> Vec<Effect> {
        let Some(settings) = self.state.folder_settings.take() else {
            return vec![];
        };
        let name = Some(name.trim().to_owned()).filter(|name| !name.is_empty());
        let mut entries = self.rail_entries();
        let Some(folder) = entries
            .iter_mut()
            .filter_map(|entry| entry.folder.as_mut())
            .find(|folder| folder.id == settings.folder)
        else {
            return vec![];
        };
        if folder.name == name && folder.color == color {
            return vec![];
        }
        folder.name = name;
        folder.color = color;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ArrangeServers { entries },
        ))]
    }
}
