//! A user-confirmed server creation from the add-server dialog.

use super::*;
use crate::{chat::ServerLayout, i18n::tr};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerCreation {
    pub layout: Option<ServerLayout>,
    pub(super) request: Option<u64>,
    pub error: Option<String>,
}

impl ServerCreation {
    pub fn creating(&self) -> bool {
        self.request.is_some()
    }
}

pub fn valid_server_name(name: &str) -> bool {
    (2..=100).contains(&name.trim().chars().count())
}

impl Application {
    pub(super) fn creating_server(&self) -> bool {
        self.state
            .server_creation
            .as_ref()
            .is_some_and(ServerCreation::creating)
    }

    pub(super) fn open_add_server(&mut self) -> Vec<Effect> {
        if (self.state.discord_online || self.state.preview) && self.state.server_creation.is_none()
        {
            self.state.join = None;
            self.state.server_creation = Some(ServerCreation::default());
        }
        vec![]
    }

    pub(super) fn choose_server_layout(&mut self, layout: Option<ServerLayout>) -> Vec<Effect> {
        if let Some(dialog) = &mut self.state.server_creation
            && !dialog.creating()
            && self.state.join.is_none()
        {
            dialog.layout = layout;
            dialog.error = None;
        }
        vec![]
    }

    pub(super) fn close_add_server(&mut self) -> Vec<Effect> {
        if !self.creating_server() {
            self.state.server_creation = None;
        }
        vec![]
    }

    pub(super) fn create_server(&mut self, name: String, icon: Option<PathBuf>) -> Vec<Effect> {
        let Some(dialog) = &mut self.state.server_creation else {
            return vec![];
        };
        if dialog.creating() || self.state.join.is_some() {
            return vec![];
        }
        let Some(layout) = dialog.layout else {
            return vec![];
        };
        if !valid_server_name(&name) {
            dialog.error = Some(tr("Enter a server name between 2 and 100 characters.").into());
            return vec![];
        }
        if !self.state.discord_online || self.state.preview {
            dialog.error = Some(tr("Connect your Discord account to create a server.").into());
            return vec![];
        }
        self.server_creation_request = self.server_creation_request.wrapping_add(1);
        let request = self.server_creation_request;
        dialog.request = Some(request);
        dialog.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::CreateServer {
                request,
                name: name.trim().into(),
                layout,
                icon,
            },
        ))]
    }

    pub(super) fn server_created(&mut self, request: u64, result: Result<String, String>) {
        let Some(dialog) = self
            .state
            .server_creation
            .as_mut()
            .filter(|dialog| dialog.request == Some(request))
        else {
            return;
        };
        match result {
            Ok(server) => {
                self.state.server_creation = None;
                self.pending_server = Some(server);
            }
            Err(error) => {
                dialog.request = None;
                dialog.error = Some(error);
            }
        }
    }
}
