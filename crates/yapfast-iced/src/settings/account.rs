//! Account tools in Yapfast's form kit. Profiles use the existing editor;
//! security, connected accounts and payments stay in Discord's own flows.
//! Browser actions name their destination rather than promising a settings
//! deep link. The official sources are in docs/discord/account-settings.md.

use iced::{Element, Task, widget::column};
use yapfast_core::application::{Action, MenuChoice};

use super::{Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::Cx,
    form,
};

#[derive(Default)]
pub struct Account {
    pub tab: Tab,
    servers_open: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Account,
    Profiles,
    Connections,
    AuthorizedApps,
    Devices,
    Billing,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Account => "Overview",
            Self::Profiles => "Profiles",
            Self::Connections => "Connections",
            Self::AuthorizedApps => "Authorized Apps",
            Self::Devices => "Devices",
            Self::Billing => "Nitro & Billing",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Show(Tab),
    Open(Workflow),
    ToggleServers,
    EditServer(String),
}

/// Fixed public destinations, with no account identifiers or login data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workflow {
    Discord,
    UsernameHelp,
    EmailHelp,
    PhoneHelp,
    PasswordHelp,
    MfaHelp,
    ConnectionsHelp,
    AuthorizedAppsHelp,
    LoginHelp,
    Nitro,
    BillingHelp,
}

impl Workflow {
    pub fn label(self) -> &'static str {
        match self {
            Self::Discord => "Open Discord in browser",
            Self::UsernameHelp => "Username help",
            Self::EmailHelp => "Email help",
            Self::PhoneHelp => "Phone number help",
            Self::PasswordHelp => "Password help",
            Self::MfaHelp => "Authentication help",
            Self::ConnectionsHelp => "Connections help",
            Self::AuthorizedAppsHelp => "App access help",
            Self::LoginHelp => "Login help",
            Self::Nitro => "Explore Nitro on discord.com",
            Self::BillingHelp => "Subscriptions and billing help",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Discord => "https://discord.com/app",
            Self::UsernameHelp => {
                "https://support.discord.com/hc/en-us/articles/12620128861463-New-Usernames-Display-Names"
            }
            Self::EmailHelp => {
                "https://support.discord.com/hc/en-us/articles/4423385681175-How-to-Change-your-Discord-Account-s-Email"
            }
            Self::PhoneHelp => {
                "https://support.discord.com/hc/en-us/articles/4413460214807-How-to-Remove-a-Phone-Number-From-a-Discord-Account"
            }
            Self::PasswordHelp => {
                "https://support.discord.com/hc/en-us/articles/218410947-How-to-Reset-or-Change-Your-Forgotten-Password-on-Discord"
            }
            Self::MfaHelp => {
                "https://support.discord.com/hc/en-us/articles/219576828-Setting-up-Multi-Factor-Authentication"
            }
            Self::ConnectionsHelp => {
                "https://support.discord.com/hc/en-us/articles/32330173689623-Account-Connections-on-Discord-FAQ"
            }
            Self::AuthorizedAppsHelp => {
                "https://support.discord.com/hc/en-us/articles/24160905919511-My-Discord-Account-was-Hacked-or-Compromised"
            }
            Self::LoginHelp => {
                "https://support.discord.com/hc/en-us/articles/360039213771-QR-Code-Login-FAQ"
            }
            Self::Nitro => "https://discord.com/nitro",
            Self::BillingHelp => {
                "https://support.discord.com/hc/en-us/articles/360017693772-Managing-Subscriptions-and-Billing-FAQ"
            }
        }
    }
}

fn message(event: Event) -> Message {
    Message::Settings(super::Event::AccountPage(event))
}

impl Settings {
    pub(super) fn account_page_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Show(tab) => {
                self.account.tab = tab;
                self.account.servers_open = false;
                return iced::widget::operation::snap_to(
                    iced::widget::Id::new(super::PAGE),
                    iced::widget::scrollable::RelativeOffset::START,
                );
            }
            Event::Open(workflow) => crate::browser::open(workflow.url()),
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
        }
        Task::none()
    }
}

impl<'a> Page<'a> {
    pub(super) fn account_hub(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tabs = [
            [Tab::Account, Tab::Profiles, Tab::Connections],
            [Tab::AuthorizedApps, Tab::Devices, Tab::Billing],
        ];
        let navigation = form::group(tabs.map(|tabs| {
            form::buttons(tabs.map(|tab| {
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
            Tab::Account => form::page([
                self.account_page(),
                self.account_workflows(),
            ]),
            Tab::Profiles => self.account_profiles(),
            Tab::Connections => form::page([
                form::title(theme, "Connected accounts"),
                self.discord_workflow(
                    "Link services such as Spotify, Xbox or PlayStation in Discord.",
                    "User Settings > Connections",
                    Workflow::ConnectionsHelp,
                ),
            ]),
            Tab::AuthorizedApps => form::page([
                form::title(theme, "App access"),
                self.discord_workflow(
                    "Review the apps allowed to access your Discord account and their permissions in Discord.",
                    "User Settings > Authorized Apps",
                    Workflow::AuthorizedAppsHelp,
                ),
            ]),
            Tab::Devices => form::page([
                form::title(theme, "Signed-in devices"),
                self.discord_workflow(
                    "Review your signed-in devices in Discord.",
                    "User Settings > Devices",
                    Workflow::LoginHelp,
                ),
            ]),
            Tab::Billing => self.account_billing(),
        };
        form::page([navigation, form::rule(theme), body])
    }

    fn workflow_button(self, workflow: Workflow) -> Element<'a, Message> {
        action(
            self.cx.look.theme,
            workflow.label(),
            Kind::Plain,
            Some(message(Event::Open(workflow))),
        )
        .into()
    }

    fn discord_workflow(
        self,
        description: &'static str,
        path: &'static str,
        help: Workflow,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        form::card(
            theme,
            form::page([
                form::note(theme, description),
                form::heading(theme, path),
                form::note(
                    theme,
                    "Opens Discord's web app. Choose the settings page there.",
                ),
                form::group([
                    self.workflow_button(Workflow::Discord),
                    self.workflow_button(help),
                ]),
            ]),
        )
    }

    fn account_workflows(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        form::page([
            form::heading(theme, "Account details and security"),
            self.discord_workflow(
                "Change your username, email, phone number, password or authentication in Discord.",
                "User Settings > My Account",
                Workflow::UsernameHelp,
            ),
            form::group([
                self.workflow_button(Workflow::EmailHelp),
                self.workflow_button(Workflow::PhoneHelp),
                self.workflow_button(Workflow::PasswordHelp),
                self.workflow_button(Workflow::MfaHelp),
            ]),
        ])
    }

    fn account_profiles(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let title = form::title(theme, "Your profiles");
        if self.cx.state.discord_user.is_none() {
            return form::page([
                title,
                form::note(theme, "Link a Discord account to edit its profile."),
            ]);
        }
        let main = form::card(
            theme,
            form::page([
                form::heading(theme, "Main profile"),
                form::note(
                    theme,
                    "Edit your display name, pronouns, bio and pictures in Yapfast.",
                ),
                action(
                    theme,
                    "Edit main profile",
                    Kind::Plain,
                    Some(Message::Settings(super::Event::EditProfile)),
                )
                .into(),
            ]),
        );
        let mut servers = vec![form::heading(theme, "Server profiles")];
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
                servers.push(form::note(theme, "No server profiles are available."));
            } else {
                servers.push(column(choices).spacing(form::TIGHT).into());
            }
        }
        servers.push(
            action(
                theme,
                if self.s.account.servers_open {
                    "Close server list"
                } else {
                    "Choose a server"
                },
                Kind::Plain,
                Some(message(Event::ToggleServers)),
            )
            .into(),
        );
        form::page([title, main, form::card(theme, form::page(servers))])
    }

    fn account_billing(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        form::page([
            form::title(theme, "Nitro and payments"),
            self.workflow_button(Workflow::Nitro),
            self.discord_workflow(
                "Manage Nitro and Boost subscriptions, renewals and plan changes in Discord.",
                "User Settings > Subscriptions",
                Workflow::BillingHelp,
            ),
            form::card(
                theme,
                form::group([
                    form::heading(theme, "User Settings > Billing"),
                    form::note(
                        theme,
                        "Payment methods, transaction history, receipts and invoices are in Discord's Billing page.",
                    ),
                ]),
            ),
        ])
    }
}
