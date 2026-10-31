//! Authorizing a Discord application from a clicked authorization
//! link: adding a bot to a server, installing an app, or letting a site
//! sign this account in. What it asks for shows first, and authorizing
//! waits for the click — even when Discord would skip asking because
//! the account authorized it before.

use super::*;
use crate::chat::OAuthPreview;

/// The authorization dialog, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OAuthDialog {
    /// The query of the link that opened it, which authorizing replays.
    pub query: Option<String>,
    /// What the authorization shows, once Discord answered.
    pub preview: Option<OAuthPreview>,
    /// The server picked for a server install, by its Discord id.
    pub guild: Option<String>,
    pub resolving: bool,
    pub authorizing: bool,
    /// Authorized, with nowhere else for Discord to send the person, or
    /// with an address Yapfast doesn't open.
    pub done: bool,
    /// Where Discord sends the person next, when that isn't a web page,
    /// such as an app's own `myapp://` address: Yapfast shows it to copy
    /// and never opens it, as it opens no such link in a message.
    pub next: Option<String>,
    pub error: Option<String>,
}

impl Application {
    /// Opens the dialog for a clicked authorization link, by its query,
    /// and looks the authorization up.
    pub(super) fn open_oauth_link(&mut self, query: String) -> Vec<Effect> {
        if !self.state.discord_online && !self.state.preview {
            return vec![];
        }
        self.state.oauth = Some(OAuthDialog {
            query: Some(query.clone()),
            resolving: true,
            ..OAuthDialog::default()
        });
        if self.state.preview {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ResolveOAuth { query },
        ))]
    }

    pub(super) fn oauth_resolved(
        &mut self,
        query: String,
        result: Result<OAuthPreview, String>,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .oauth
            .as_mut()
            .filter(|dialog| dialog.resolving && dialog.query.as_ref() == Some(&query))
        else {
            return vec![];
        };
        dialog.resolving = false;
        match result {
            Ok(preview) => {
                dialog.guild = preview.wanted_guild.clone().filter(|wanted| {
                    preview
                        .guilds
                        .iter()
                        .any(|guild| &guild.id == wanted && guild.can_add)
                });
                if dialog.guild.is_none() {
                    dialog.guild = preview
                        .guilds
                        .iter()
                        .find(|guild| guild.can_add)
                        .map(|guild| guild.id.clone());
                }
                dialog.preview = Some(preview);
            }
            Err(error) => dialog.error = Some(error),
        }
        vec![]
    }

    /// Picks the server a server install goes to, when the link allows
    /// picking and this account may add applications there.
    pub(super) fn choose_oauth_guild(&mut self, guild: String) -> Vec<Effect> {
        if let Some(dialog) = self.state.oauth.as_mut()
            && let Some(preview) = dialog.preview.as_ref()
            && !preview.guild_locked
            && preview
                .guilds
                .iter()
                .any(|found| found.id == guild && found.can_add)
        {
            dialog.guild = Some(guild);
        }
        vec![]
    }

    /// Authorizes what the dialog shows, in the server picked.
    pub(super) fn authorize_oauth(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .oauth
            .as_mut()
            .filter(|dialog| !dialog.authorizing && !dialog.done)
        else {
            return vec![];
        };
        let Some(preview) = dialog.preview.clone() else {
            return vec![];
        };
        if self.state.preview {
            return vec![];
        }
        if preview.guild_install && dialog.guild.is_none() {
            dialog.error =
                Some("Pick a server you manage first, or ask its admin to add it.".into());
            return vec![];
        }
        dialog.authorizing = true;
        dialog.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::AuthorizeOAuth {
                query: preview.query.clone(),
                guild: dialog.guild.clone().filter(|_| preview.guild_install),
            },
        ))]
    }

    pub(super) fn oauth_authorized(
        &mut self,
        query: String,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .oauth
            .as_mut()
            .filter(|dialog| dialog.authorizing && dialog.query.as_ref() == Some(&query))
        else {
            return vec![];
        };
        dialog.authorizing = false;
        match result {
            Ok(url) if on_discord(&url) => {
                dialog.done = true;
                vec![]
            }
            // The application's own address, with its code: the browser
            // takes the person there, as Discord's page would.
            Ok(url) if crate::links::openable(&url) => {
                self.state.oauth = None;
                vec![Effect::OpenPage(url)]
            }
            // Any other address could start another program or open a
            // file, which a link from a message never does.
            Ok(url) => {
                dialog.done = true;
                dialog.next = Some(url);
                vec![]
            }
            Err(error) => {
                dialog.error = Some(error);
                vec![]
            }
        }
    }
}

/// Whether an address is a page on one of Discord's own web hosts.
#[cfg(feature = "discord")]
fn on_discord(url: &str) -> bool {
    yapfast_discord::link::on_discord(url)
}

#[cfg(not(feature = "discord"))]
fn on_discord(_url: &str) -> bool {
    false
}
