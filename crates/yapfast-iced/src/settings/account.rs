//! Account tools in Yapfast's form kit: the username, email and password
//! forms, the profiles' editors, and the lists of the apps the account
//! authorized, the devices it is signed in on and the services it linked,
//! each with what can be done to its rows. All of it is carried out in
//! Yapfast; see docs/discord/account-settings.md.

use iced::{
    Alignment, Element, Length, Task,
    widget::{Id, column, row, text},
};
use yapfast_core::{
    account_tools::{AccountField, SERVICES},
    application::{Action, MenuChoice},
    i18n::{self, tr},
};

use super::{Page, Settings};
use crate::{
    app::{App, Message},
    controls::{Kind, action, field},
    ctx::Cx,
    fonts::BOLD,
    form,
};

const VALUE: &str = "account-value";
const CONFIRM: &str = "account-confirm";
const PASSWORD: &str = "account-password";

#[derive(Default)]
pub struct Account {
    pub tab: Tab,
    servers_open: bool,
    /// The username, email or password form that is open.
    form: Option<Form>,
}

/// An open form: what is typed in it.
struct Form {
    field: AccountField,
    value: String,
    confirm: String,
    password: String,
    /// Whether Save was pressed, so the end of the change is this form's.
    submitted: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Account,
    Profiles,
    Connections,
    AuthorizedApps,
    Devices,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Account => tr("Overview"),
            Self::Profiles => tr("Profiles"),
            Self::Connections => tr("Connections"),
            Self::AuthorizedApps => tr("Authorized Apps"),
            Self::Devices => tr("Devices"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Show(Tab),
    ToggleServers,
    EditServer(String),
    /// Opens a form for the username, email or password, or closes it.
    Form(AccountField),
    Value(String),
    Confirm(String),
    Password(String),
    Save,
    /// Reads the open tab's list again.
    Refresh,
    /// Takes an authorization back, by its id.
    Revoke(String),
    /// Signs devices out, by their session hashes.
    LogOut(Vec<String>),
    Unlink {
        service: String,
        id: String,
    },
    /// Shows a connected account on the profile or not, or as an activity
    /// or not.
    Visible {
        service: String,
        id: String,
        on: bool,
    },
    Activity {
        service: String,
        id: String,
        on: bool,
    },
    /// Opens the page that links an account of a service, in the browser.
    Link(String),
}

fn message(event: Event) -> Message {
    Message::Settings(super::Event::AccountPage(event))
}

/// Reads what a tab lists, as the tab opens.
fn load(tab: Tab, cx: &mut Cx<'_>) {
    match tab {
        Tab::AuthorizedApps => cx.dispatch(Action::LoadAuthorizedApps),
        Tab::Devices => cx.dispatch(Action::LoadDevices),
        Tab::Connections => cx.dispatch(Action::LoadConnections),
        Tab::Account | Tab::Profiles => {}
    }
}

impl Settings {
    pub(super) fn account_page_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Show(tab) => {
                self.account.tab = tab;
                self.account.servers_open = false;
                self.account.form = None;
                load(tab, cx);
                return iced::widget::operation::snap_to(
                    Id::new(super::PAGE),
                    iced::widget::scrollable::RelativeOffset::START,
                );
            }
            Event::ToggleServers => self.account.servers_open = !self.account.servers_open,
            Event::EditServer(server) => {
                self.account.servers_open = false;
                if cx
                    .state()
                    .servers
                    .iter()
                    .any(|known| known.id == server && known.discord && !known.unavailable)
                {
                    cx.dispatch(Action::EditServerProfile(server));
                }
            }
            Event::Form(field) => {
                let open = self
                    .account
                    .form
                    .as_ref()
                    .is_some_and(|form| form.field == field);
                self.account.form = (!open).then(|| Form {
                    field,
                    value: String::new(),
                    confirm: String::new(),
                    password: String::new(),
                    submitted: false,
                });
            }
            Event::Value(value) => {
                if let Some(form) = self.account.form.as_mut() {
                    form.value = value;
                }
            }
            Event::Confirm(value) => {
                if let Some(form) = self.account.form.as_mut() {
                    form.confirm = value;
                }
            }
            Event::Password(value) => {
                if let Some(form) = self.account.form.as_mut() {
                    form.password = value;
                }
            }
            Event::Save => {
                if let Some(form) = self.account.form.as_mut()
                    && form.ready()
                {
                    form.submitted = true;
                    // The password is asked for again after each try.
                    let password = std::mem::take(&mut form.password);
                    cx.dispatch(Action::ChangeAccount {
                        field: form.field,
                        value: form.value.clone(),
                        password,
                    });
                }
            }
            Event::Refresh => load(self.account.tab, cx),
            Event::Revoke(id) => cx.dispatch(Action::RevokeAuthorizedApp(id)),
            Event::LogOut(ids) => cx.dispatch(Action::LogOutDevices(ids)),
            Event::Unlink { service, id } => cx.dispatch(Action::DisconnectAccount { service, id }),
            Event::Visible { service, id, on } => cx.dispatch(Action::ShowConnection {
                service,
                id,
                visible: Some(on),
                activity: None,
            }),
            Event::Activity { service, id, on } => cx.dispatch(Action::ShowConnection {
                service,
                id,
                visible: None,
                activity: Some(on),
            }),
            Event::Link(service) => cx.dispatch(Action::LinkConnection(service)),
        }
        Task::none()
    }
}

impl Form {
    /// Whether it can be sent: what is asked for is typed, and a new
    /// password is typed twice the same.
    fn ready(&self) -> bool {
        !self.value.trim().is_empty()
            && !self.password.is_empty()
            && (self.field != AccountField::Password || self.confirm == self.value)
    }
}

/// The pictures the open tab shows: the authorized apps' icons.
pub fn pictures(app: &App) -> Vec<String> {
    let Some(settings) = app.win.settings.get() else {
        return Vec::new();
    };
    if settings.section() != super::Section::Account || settings.account.tab != Tab::AuthorizedApps
    {
        return Vec::new();
    }
    app.client
        .state()
        .account_tools
        .apps
        .items
        .iter()
        .filter_map(|app| app.icon.clone())
        .collect()
}

/// What a scope lets an app do, in words.
fn scope_words(scope: &str) -> String {
    match scope {
        "identify" => tr("See your username and avatar").to_owned(),
        "email" => tr("See your email address").to_owned(),
        "guilds" => tr("See the servers you are in").to_owned(),
        "guilds.join" => tr("Join servers for you").to_owned(),
        "guilds.members.read" => tr("Read your member info in servers").to_owned(),
        "connections" => tr("See your connected accounts").to_owned(),
        "bot" => tr("Add a bot to servers").to_owned(),
        "applications.commands" => tr("Use its commands in servers").to_owned(),
        "relationships.read" => tr("See your friends").to_owned(),
        "rpc" | "rpc.notifications.read" | "rpc.voice.read" | "rpc.voice.write" => {
            tr("Control your Discord client").to_owned()
        }
        "messages.read" => tr("Read your messages").to_owned(),
        "activities.read" => tr("See what you are playing").to_owned(),
        "activities.write" => tr("Update your activity").to_owned(),
        other => other.to_owned(),
    }
}

impl<'a> Page<'a> {
    pub(super) fn account_hub(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tabs = [
            &[Tab::Account, Tab::Profiles, Tab::Connections][..],
            &[Tab::AuthorizedApps, Tab::Devices][..],
        ];
        let navigation = form::group(tabs.map(|tabs| {
            form::buttons(tabs.iter().map(|&tab| {
                action(
                    theme,
                    tab.label(),
                    if self.s.account.tab == tab {
                        Kind::Primary
                    } else {
                        Kind::Plain
                    },
                    Some(message(Event::Show(tab))),
                )
                .wide(form::BOX)
                .into()
            }))
        }));
        let body = match self.s.account.tab {
            Tab::Account => form::page([self.account_page(), self.account_security()]),
            Tab::Profiles => self.account_profiles(),
            Tab::Connections => self.account_connections(),
            Tab::AuthorizedApps => self.account_apps(),
            Tab::Devices => self.account_devices(),
        };
        form::page([navigation, form::rule(theme), body])
    }

    /// What a list shows before its rows: that it is loading, why it
    /// failed with a button to read it again, or that it is empty.
    fn listing_state(
        self,
        loading: bool,
        loaded: bool,
        error: Option<&'a str>,
        empty: bool,
        none: &'static str,
    ) -> Option<Element<'a, Message>> {
        // `none` is already in the reader's language.
        let theme = self.cx.look.theme;
        if let Some(error) = error {
            return Some(form::group([
                form::note_in(error, theme.danger),
                action(
                    theme,
                    tr("Try again"),
                    Kind::Plain,
                    (!loading).then_some(message(Event::Refresh)),
                )
                .into(),
            ]));
        }
        if loading && !loaded {
            return Some(form::note(theme, tr("Loading...")));
        }
        if !loaded {
            // Nothing is read while Discord is not connected.
            return Some(form::note(
                theme,
                tr("Discord is not connected, so this list can't be read."),
            ));
        }
        empty.then(|| form::note(theme, none))
    }

    /// Why the last revoking, signing out or unlinking was refused.
    fn tool_error(self) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        self.cx
            .state
            .account_tools
            .error
            .as_deref()
            .map(|error| form::note_in(error, theme.danger))
    }

    fn account_security(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let heading = form::heading(theme, tr("ACCOUNT DETAILS"));
        if !self.linked() {
            return form::page([
                heading,
                form::note(theme, tr("Link a Discord account to change its details.")),
            ]);
        }
        let mut parts = vec![heading];
        for (field, title, words) in [
            (
                AccountField::Username,
                tr("Username"),
                tr("What people add you by."),
            ),
            (
                AccountField::Email,
                tr("Email"),
                tr("Where Discord writes to you."),
            ),
            (
                AccountField::Password,
                tr("Password"),
                tr("Changing it signs your other devices out."),
            ),
        ] {
            parts.push(self.detail_card(field, title, words));
        }
        form::page(parts)
    }

    /// One of the username, email and password: its change button, and its
    /// form while open.
    fn detail_card(
        self,
        field_: AccountField,
        title: &'static str,
        words: &'static str,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let open = self
            .s
            .account
            .form
            .as_ref()
            .filter(|form| form.field == field_);
        let head = row![
            column![
                text(title).size(16).font(BOLD).color(theme.text),
                form::note(theme, words)
            ]
            .spacing(2)
            .width(Length::Fill),
            action(
                theme,
                if open.is_some() {
                    tr("Cancel")
                } else {
                    tr("Change")
                },
                Kind::Plain,
                Some(message(Event::Form(field_))),
            )
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let mut parts = vec![head.into()];
        if let Some(form) = open {
            parts.push(self.detail_form(form));
        }
        form::card(theme, form::page(parts))
    }

    fn detail_form(self, form: &'a Form) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let change = &self.cx.state.account_tools.change;
        let busy = change.busy == Some(form.field);
        if form.submitted && !busy && change.error.is_none() && change.done == Some(form.field) {
            return form::note(
                theme,
                match form.field {
                    AccountField::Username => tr("Your username was changed."),
                    AccountField::Email => {
                        tr("Your email was changed. Discord asks you to confirm it by mail.")
                    }
                    AccountField::Password => {
                        tr("Your password was changed. Other devices are signed out.")
                    }
                },
            );
        }
        let secure = form.field == AccountField::Password;
        let value = field(
            theme,
            VALUE,
            match form.field {
                AccountField::Username => tr("New username"),
                AccountField::Email => tr("New email address"),
                AccountField::Password => tr("New password"),
            },
            &form.value,
            secure,
            !busy,
        )
        .on_input(|value| message(Event::Value(value)));
        let mut parts: Vec<Element<'a, Message>> = vec![value.into()];
        if secure {
            parts.push(
                field(
                    theme,
                    CONFIRM,
                    tr("Confirm new password"),
                    &form.confirm,
                    true,
                    !busy,
                )
                .on_input(|value| message(Event::Confirm(value)))
                .into(),
            );
            if !form.confirm.is_empty() && form.confirm != form.value {
                parts.push(form::note_in(
                    tr("The new passwords are not the same."),
                    theme.danger,
                ));
            }
        }
        parts.push(
            field(
                theme,
                PASSWORD,
                tr("Current password"),
                &form.password,
                true,
                !busy,
            )
            .on_input(|value| message(Event::Password(value)))
            .into(),
        );
        if busy {
            parts.push(form::note(theme, tr("Saving...")));
        } else if let Some(error) = change.error.as_deref().filter(|_| form.submitted) {
            parts.push(form::note_in(error, theme.danger));
        }
        parts.push(
            action(
                theme,
                tr("Save"),
                Kind::Primary,
                (form.ready() && change.busy.is_none()).then_some(message(Event::Save)),
            )
            .into(),
        );
        form::group(parts)
    }

    fn account_connections(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let title = form::title(theme, tr("Connected accounts"));
        if !self.linked() {
            return form::page([
                title,
                form::note(theme, tr("Link a Discord account to see its connections.")),
            ]);
        }
        let tools = &self.cx.state.account_tools;
        let list = &tools.connections;
        let mut parts = vec![
            title,
            form::note(
                theme,
                tr(
                    "Accounts of other services you linked to Discord. They can show on your profile, and some can show what you do as an activity.",
                ),
            ),
        ];
        parts.extend(self.listing_state(
            list.loading,
            list.loaded,
            list.error.as_deref(),
            list.items.is_empty(),
            tr("No accounts are linked."),
        ));
        parts.extend(self.tool_error());
        for connection in &list.items {
            let busy = tools.busy.contains(&connection.key());
            let (service, id) = (connection.service.clone(), connection.id.clone());
            let mut body: Vec<Element<'a, Message>> = vec![
                row![
                    column![
                        text(connection.label.as_str())
                            .size(16)
                            .font(BOLD)
                            .color(theme.text),
                        form::note(
                            theme,
                            if connection.revoked {
                                i18n::fill(
                                    tr("{0} (access was taken back, link it again)"),
                                    &[&connection.name],
                                )
                            } else if connection.verified {
                                i18n::fill(tr("{0} (verified)"), &[&connection.name])
                            } else {
                                connection.name.clone()
                            }
                        ),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    action(
                        theme,
                        tr("Disconnect"),
                        Kind::Danger,
                        (!busy).then(|| message(Event::Unlink {
                            service: service.clone(),
                            id: id.clone(),
                        })),
                    )
                ]
                .spacing(16)
                .align_y(Alignment::Center)
                .into(),
            ];
            body.push(
                form::switch(
                    tr("Display on profile"),
                    connection.visible,
                    (!busy).then(|| {
                        message(Event::Visible {
                            service: service.clone(),
                            id: id.clone(),
                            on: !connection.visible,
                        })
                    }),
                )
                .view(self.cx.look),
            );
            body.push(
                form::switch(
                    tr("Display as activity status"),
                    connection.show_activity,
                    (!busy).then(|| {
                        message(Event::Activity {
                            service: service.clone(),
                            id: id.clone(),
                            on: !connection.show_activity,
                        })
                    }),
                )
                .view(self.cx.look),
            );
            parts.push(form::card(theme, form::page(body)));
        }
        parts.push(form::heading(theme, tr("ADD A CONNECTION")));
        parts.push(form::note(
            theme,
            tr("The service's own sign-in page opens in your browser. When you are done there, read the list again."),
        ));
        for services in SERVICES.chunks(4) {
            parts.push(form::buttons(services.iter().map(|&(service, name)| {
                let busy = tools.busy.contains(&format!("link/{service}"));
                action(
                    theme,
                    name,
                    Kind::Plain,
                    (!busy).then(|| message(Event::Link(service.to_owned()))),
                )
                .into()
            })));
        }
        parts.push(
            action(
                theme,
                tr("Read the list again"),
                Kind::Plain,
                (!list.loading).then_some(message(Event::Refresh)),
            )
            .into(),
        );
        form::page(parts)
    }

    fn account_apps(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let title = form::title(theme, tr("App access"));
        if !self.linked() {
            return form::page([
                title,
                form::note(theme, tr("Link a Discord account to see its apps.")),
            ]);
        }
        let tools = &self.cx.state.account_tools;
        let list = &tools.apps;
        let mut parts = vec![
            title,
            form::note(
                theme,
                tr(
                    "Apps you let into your Discord account. Revoking one ends its access, and it has to ask you again.",
                ),
            ),
        ];
        parts.extend(self.listing_state(
            list.loading,
            list.loaded,
            list.error.as_deref(),
            list.items.is_empty(),
            tr("No apps are authorized."),
        ));
        parts.extend(self.tool_error());
        for app in &list.items {
            let busy = tools.busy.contains(&app.id);
            let scopes = app
                .scopes
                .iter()
                .map(|scope| scope_words(scope))
                .collect::<Vec<_>>()
                .join(", ");
            let mut words = column![
                text(app.name.as_str())
                    .size(16)
                    .font(BOLD)
                    .color(theme.text)
            ]
            .spacing(2)
            .width(Length::Fill);
            if !app.description.is_empty() {
                words = words.push(form::note(theme, app.description.as_str()));
            }
            if !scopes.is_empty() {
                words = words.push(form::note(theme, i18n::fill(tr("Can: {0}"), &[&scopes])));
            }
            parts.push(form::card(
                theme,
                row![
                    self.cx.face(crate::pictures::Face::new(
                        app.icon.as_deref(),
                        &app.name,
                        40.0
                    )),
                    words,
                    action(
                        theme,
                        tr("Revoke"),
                        Kind::Danger,
                        (!busy).then(|| message(Event::Revoke(app.id.clone()))),
                    )
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            ));
        }
        form::page(parts)
    }

    fn account_devices(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let title = form::title(theme, tr("Signed-in devices"));
        if !self.linked() {
            return form::page([
                title,
                form::note(theme, tr("Link a Discord account to see its devices.")),
            ]);
        }
        let tools = &self.cx.state.account_tools;
        let list = &tools.devices;
        let mut parts = vec![
            title,
            form::note(
                theme,
                tr("Where your account is signed in. Sign out a device you do not know."),
            ),
        ];
        parts.extend(self.listing_state(
            list.loading,
            list.loaded,
            list.error.as_deref(),
            list.items.is_empty(),
            tr("No devices are signed in."),
        ));
        parts.extend(self.tool_error());
        let others: Vec<String> = list
            .items
            .iter()
            .filter(|device| !device.current)
            .map(|device| device.id.clone())
            .collect();
        if others.len() > 1 {
            parts.push(
                action(
                    theme,
                    tr("Sign out all other devices"),
                    Kind::Danger,
                    others
                        .iter()
                        .all(|id| !tools.busy.contains(id))
                        .then(|| message(Event::LogOut(others.clone()))),
                )
                .into(),
            );
        }
        let now = yapfast_core::clock::now_ms();
        for device in &list.items {
            let busy = tools.busy.contains(&device.id);
            let name = match (device.platform.is_empty(), device.os.is_empty()) {
                (false, false) => format!("{}, {}", device.platform, device.os),
                (false, true) => device.platform.clone(),
                (true, false) => device.os.clone(),
                (true, true) => tr("Unknown device").to_owned(),
            };
            let mut details = Vec::new();
            if device.current {
                details.push(tr("This device").to_owned());
            }
            if !device.location.is_empty() {
                details.push(device.location.clone());
            }
            if device.last_used > 0 {
                details.push(crate::header_popovers::ago(device.last_used * 1000, now));
            }
            let mut words = column![text(name).size(16).font(BOLD).color(theme.text)]
                .spacing(2)
                .width(Length::Fill);
            if !details.is_empty() {
                words = words.push(form::note(theme, details.join(" - ")));
            }
            let mut line = row![words].spacing(16).align_y(Alignment::Center);
            if !device.current {
                line = line.push(action(
                    theme,
                    tr("Sign out"),
                    Kind::Danger,
                    (!busy).then(|| message(Event::LogOut(vec![device.id.clone()]))),
                ));
            }
            parts.push(form::card(theme, line));
        }
        form::page(parts)
    }

    fn account_profiles(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let title = form::title(theme, tr("Your profiles"));
        if self.cx.state.discord_user.is_none() {
            return form::page([
                title,
                form::note(theme, tr("Link a Discord account to edit its profile.")),
            ]);
        }
        let main = form::card(
            theme,
            form::page([
                form::heading(theme, tr("Main profile")),
                form::note(
                    theme,
                    tr("Edit your display name, pronouns, bio and pictures in Yapfast."),
                ),
                action(
                    theme,
                    tr("Edit main profile"),
                    Kind::Plain,
                    Some(Message::Settings(super::Event::EditProfile)),
                )
                .into(),
            ]),
        );
        let mut servers = vec![form::heading(theme, tr("Server profiles"))];
        // Only build the server choices while the chooser is open. Names
        // are borrowed from the core, with no second list kept in settings.
        if self.s.account.servers_open {
            let choices: Vec<_> = self
                .cx
                .state
                .servers
                .iter()
                .filter(|server| {
                    server.discord
                        && !server.unavailable
                        && self
                            .cx
                            .app
                            .menu_choices(&server.id)
                            .contains(&MenuChoice::EditServerProfile)
                })
                .map(|server| {
                    action(
                        theme,
                        server.name.as_str(),
                        Kind::Plain,
                        Some(message(Event::EditServer(server.id.clone()))),
                    )
                    .into()
                })
                .collect();
            if choices.is_empty() {
                servers.push(form::note(theme, tr("No server profiles are available.")));
            } else {
                servers.push(column(choices).spacing(form::TIGHT).into());
            }
        }
        servers.push(
            action(
                theme,
                if self.s.account.servers_open {
                    tr("Close server list")
                } else {
                    tr("Choose a server")
                },
                Kind::Plain,
                Some(message(Event::ToggleServers)),
            )
            .into(),
        );
        form::page([title, main, form::card(theme, form::page(servers))])
    }
}
