//! My Account's tabs: the apps the account authorized, the devices it is
//! signed in on and the services it linked are read when their tab opens,
//! and each can be taken back, signed out or unlinked; the username, email
//! and password change with the current password. Nothing is sent without a
//! click, and a click that can't be answered, as in preview, does nothing.

use super::*;
use crate::account_tools::{AccountField, AuthorizedApp, Connection, Device, key};

impl Application {
    /// Whether Discord can be asked: a linked account that is connected.
    fn tools_ready(&self) -> bool {
        !self.state.preview && self.state.discord_online
    }

    /// Starts something about `key`, unless it already goes.
    fn begin_tool(&mut self, key: &str) -> bool {
        let ready = self.tools_ready();
        let tools = &mut self.state.account_tools;
        if !ready || tools.busy.iter().any(|busy| busy == key) {
            return false;
        }
        tools.busy.push(key.to_owned());
        tools.error = None;
        true
    }

    fn end_tool(&mut self, key: &str, result: Result<(), String>) -> bool {
        let tools = &mut self.state.account_tools;
        tools.busy.retain(|busy| busy != key);
        match result {
            Ok(()) => true,
            Err(error) => {
                tools.error = Some(error);
                false
            }
        }
    }

    pub(super) fn load_authorized_apps(&mut self) -> Vec<Effect> {
        if !self.tools_ready() || self.state.account_tools.apps.loading {
            return vec![];
        }
        let apps = &mut self.state.account_tools.apps;
        apps.loading = true;
        apps.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadAuthorizedApps,
        ))]
    }

    pub(super) fn authorized_apps_loaded(
        &mut self,
        result: Result<Vec<AuthorizedApp>, String>,
    ) -> Vec<Effect> {
        self.state.account_tools.apps.finish(result);
        vec![]
    }

    pub(super) fn revoke_authorized_app(&mut self, id: String) -> Vec<Effect> {
        if !self.begin_tool(&id) {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::RevokeAuthorizedApp { id },
        ))]
    }

    pub(super) fn authorized_app_revoked(
        &mut self,
        id: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        if self.end_tool(&id, result) {
            self.state
                .account_tools
                .apps
                .items
                .retain(|app| app.id != id);
        }
        vec![]
    }

    pub(super) fn load_devices(&mut self) -> Vec<Effect> {
        if !self.tools_ready() || self.state.account_tools.devices.loading {
            return vec![];
        }
        let devices = &mut self.state.account_tools.devices;
        devices.loading = true;
        devices.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadDevices,
        ))]
    }

    pub(super) fn devices_loaded(&mut self, result: Result<Vec<Device>, String>) -> Vec<Effect> {
        self.state.account_tools.devices.finish(result);
        vec![]
    }

    /// Signs these devices out; those already going aren't asked for again.
    pub(super) fn log_out_devices(&mut self, ids: Vec<String>) -> Vec<Effect> {
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| !id.is_empty())
            .filter(|id| self.begin_tool(id))
            .collect();
        if ids.is_empty() {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LogOutDevices { ids },
        ))]
    }

    pub(super) fn devices_logged_out(
        &mut self,
        ids: Vec<String>,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let mut done = false;
        for id in &ids {
            done = self.end_tool(id, result.clone());
        }
        if done {
            self.state
                .account_tools
                .devices
                .items
                .retain(|device| !ids.contains(&device.id));
        }
        vec![]
    }

    pub(super) fn load_connections(&mut self) -> Vec<Effect> {
        if !self.tools_ready() || self.state.account_tools.connections.loading {
            return vec![];
        }
        let connections = &mut self.state.account_tools.connections;
        connections.loading = true;
        connections.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadConnections,
        ))]
    }

    pub(super) fn connections_loaded(
        &mut self,
        result: Result<Vec<Connection>, String>,
    ) -> Vec<Effect> {
        self.state.account_tools.connections.finish(result);
        vec![]
    }

    pub(super) fn disconnect_account(&mut self, service: String, id: String) -> Vec<Effect> {
        if !self.begin_tool(&key(&service, &id)) {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::DisconnectAccount { service, id },
        ))]
    }

    pub(super) fn account_disconnected(
        &mut self,
        service: String,
        id: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        if self.end_tool(&key(&service, &id), result) {
            self.state
                .account_tools
                .connections
                .items
                .retain(|connection| !(connection.service == service && connection.id == id));
        }
        vec![]
    }

    pub(super) fn show_connection(
        &mut self,
        service: String,
        id: String,
        visible: Option<bool>,
        activity: Option<bool>,
    ) -> Vec<Effect> {
        if (visible.is_none() && activity.is_none()) || !self.begin_tool(&key(&service, &id)) {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ShowConnection {
                service,
                id,
                visible,
                activity,
            },
        ))]
    }

    pub(super) fn connection_shown(
        &mut self,
        service: String,
        id: String,
        visible: Option<bool>,
        activity: Option<bool>,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        if self.end_tool(&key(&service, &id), result)
            && let Some(connection) = self
                .state
                .account_tools
                .connections
                .items
                .iter_mut()
                .find(|connection| connection.service == service && connection.id == id)
        {
            if let Some(visible) = visible {
                connection.visible = visible;
            }
            if let Some(activity) = activity {
                connection.show_activity = activity;
            }
        }
        vec![]
    }

    /// Asks for the page of a service that links an account of it; the
    /// service's own sign-in can only be shown by its page.
    pub(super) fn link_connection(&mut self, service: String) -> Vec<Effect> {
        if !self.begin_tool(&format!("link/{service}")) {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LinkConnection { service },
        ))]
    }

    pub(super) fn connection_link(
        &mut self,
        service: String,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        let tools = &mut self.state.account_tools;
        tools.busy.retain(|busy| *busy != format!("link/{service}"));
        match result {
            // Only a secure page is opened, whatever Discord answered.
            Ok(page) if page.starts_with("https://") => vec![Effect::OpenPage(page)],
            Ok(_) => {
                tools.error = Some("Discord gave no page to sign in on.".into());
                vec![]
            }
            Err(error) => {
                tools.error = Some(error);
                vec![]
            }
        }
    }

    /// Changes the username, email or password, with the account's current
    /// password; an empty value or password is not sent.
    pub(super) fn change_account(
        &mut self,
        field: AccountField,
        value: String,
        password: String,
    ) -> Vec<Effect> {
        if !self.tools_ready()
            || self.state.account_tools.change.busy.is_some()
            || value.trim().is_empty()
            || password.is_empty()
        {
            return vec![];
        }
        let change = &mut self.state.account_tools.change;
        change.busy = Some(field);
        change.error = None;
        change.done = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ChangeAccount {
                field,
                value: value.trim().to_owned(),
                password,
            },
        ))]
    }

    pub(super) fn account_changed(
        &mut self,
        field: AccountField,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let change = &mut self.state.account_tools.change;
        change.busy = None;
        match result {
            Ok(()) => change.done = Some(field),
            Err(error) => change.error = Some(error),
        }
        vec![]
    }
}
