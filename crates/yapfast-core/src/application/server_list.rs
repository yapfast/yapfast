//! The rail's server list rearranged by a drag: servers in another order,
//! moved into or out of folders, a folder made. Discord's settings keep the
//! list (`PreloadedUserSettings.guild_folders`), written whole: each folder
//! with its id, name and colour as they were, a server outside any as a
//! folder of one without an id, a new folder with an id made as Discord's
//! client makes one. The list shows at once; if Discord refuses it, the
//! order it had comes back with a notice. A list changed on another device
//! replaces it, as Discord's settings always do.

use super::*;
use crate::chat::ServerGroup;
#[cfg(feature = "discord")]
use crate::chat::{NEW_FOLDER, ServerListEntry};

impl Application {
    pub(super) fn arrange_servers(&mut self, groups: Vec<ServerGroup>) -> Vec<Effect> {
        if self.state.preview || self.state.discord_user.is_none() {
            return vec![];
        }
        #[cfg(feature = "discord")]
        {
            let Some(entries) = self.server_list(&groups) else {
                return vec![];
            };
            self.discord_index.arrange(&entries);
            let mut effects = self.show_lists();
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ArrangeServers(entries),
            )));
            effects
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = groups;
            vec![]
        }
    }

    /// The Discord server list `groups` make, by Discord's ids, or `None`
    /// when it's the list as it is. Discord's servers it leaves out keep
    /// their places after it; others' are no part of it.
    #[cfg(feature = "discord")]
    fn server_list(&self, groups: &[ServerGroup]) -> Option<Vec<ServerListEntry>> {
        let guild = |id: &str| {
            self.state
                .servers
                .iter()
                .find(|server| server.discord && server.id == id)
                .and_then(|server| server.id.strip_prefix(discord::PREFIX))
                .map(str::to_owned)
        };
        let folders: HashMap<&str, &chat::ServerFolder> = self
            .state
            .servers
            .iter()
            .filter_map(|server| server.folder.as_ref())
            .map(|folder| (folder.id.as_str(), folder))
            .collect();
        let taken: Vec<&str> = folders
            .keys()
            .filter_map(|id| id.strip_prefix(discord::FOLDER_PREFIX))
            .collect();
        let alone = |guild: String| ServerListEntry {
            servers: vec![guild],
            ..ServerListEntry::default()
        };
        let mut entries: Vec<ServerListEntry> = Vec::new();
        for group in groups {
            let servers: Vec<String> = group.servers.iter().filter_map(|id| guild(id)).collect();
            let folder = group.folder.as_deref();
            let kept = folder.and_then(|id| folders.get(id));
            match (folder, kept) {
                (Some(NEW_FOLDER), _) if !servers.is_empty() => entries.push(ServerListEntry {
                    id: yapfast_discord::settings::new_folder_id(&taken),
                    servers,
                    ..ServerListEntry::default()
                }),
                (_, Some(folder)) if !servers.is_empty() => entries.push(ServerListEntry {
                    id: folder
                        .id
                        .strip_prefix(discord::FOLDER_PREFIX)
                        .unwrap_or_default()
                        .to_owned(),
                    name: folder.name.clone(),
                    color: folder.color,
                    servers,
                }),
                _ => entries.extend(servers.into_iter().map(alone)),
            }
        }
        let listed: HashSet<&str> = entries
            .iter()
            .flat_map(|entry| entry.servers.iter().map(String::as_str))
            .collect();
        let left: Vec<String> = self
            .state
            .servers
            .iter()
            .filter(|server| server.discord)
            .filter_map(|server| server.id.strip_prefix(discord::PREFIX))
            .filter(|guild| !listed.contains(guild))
            .map(str::to_owned)
            .collect();
        entries.extend(left.into_iter().map(alone));
        (entries != self.current_server_list()).then_some(entries)
    }

    /// The Discord server list as the rail shows it now.
    #[cfg(feature = "discord")]
    fn current_server_list(&self) -> Vec<ServerListEntry> {
        let mut entries: Vec<ServerListEntry> = Vec::new();
        for server in self.state.servers.iter().filter(|server| server.discord) {
            let Some(guild) = server.id.strip_prefix(discord::PREFIX) else {
                continue;
            };
            match (&server.folder, entries.last_mut()) {
                (Some(folder), Some(last))
                    if !last.id.is_empty()
                        && folder.id.strip_prefix(discord::FOLDER_PREFIX) == Some(&last.id) =>
                {
                    last.servers.push(guild.to_owned());
                }
                (folder, _) => entries.push(ServerListEntry {
                    id: folder
                        .as_ref()
                        .and_then(|folder| folder.id.strip_prefix(discord::FOLDER_PREFIX))
                        .unwrap_or_default()
                        .to_owned(),
                    name: folder.as_ref().and_then(|folder| folder.name.clone()),
                    color: folder.as_ref().and_then(|folder| folder.color),
                    servers: vec![guild.to_owned()],
                }),
            }
        }
        entries
    }

    /// Discord took the list, or refused it: then the order it had comes
    /// back, from what Discord last sent, with a notice.
    pub(super) fn servers_arranged(&mut self, result: Result<(), String>) -> Vec<Effect> {
        let Err(error) = result else {
            return vec![];
        };
        let effects = self.list_discord();
        // After the lists, which may close a conversation and its notice.
        self.state.notice = Some(yapfast_lang::tr!("Couldn't move the server: {}", error));
        effects
    }
}
