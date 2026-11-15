//! Joining a Discord server by an invite: Join a Server, from the "+" at the
//! foot of the server rail, takes a pasted link or code, and a clicked
//! invite link opens the same dialog with it. Either looks the invite up
//! and shows its server before anyone joins; joining waits for the click.

use super::*;
use crate::chat::{InvitePreview, JoinedServer};

/// The Join a Server dialog, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JoinDialog {
    /// The code of the invite looked up or shown.
    pub code: Option<String>,
    /// What the invite shows, once Discord answered.
    pub preview: Option<InvitePreview>,
    /// This account is in the server already, so the dialog opens it.
    pub member: bool,
    /// A clicked invite link opened the dialog, which then has no field.
    pub from_link: bool,
    pub resolving: bool,
    pub joining: bool,
    /// The page with the captcha Discord asked for before joining.
    pub captcha: Option<String>,
    /// The server, joined, asks new members to fill in its form first,
    /// which Yapfast leaves to Discord's web app at this address.
    pub form: Option<String>,
    pub error: Option<String>,
}

impl Application {
    /// Opens Join a Server, empty; nothing without a linked Discord account.
    pub(super) fn open_join(&mut self) -> Vec<Effect> {
        if self.state.discord_online || self.state.preview {
            self.state.join = Some(JoinDialog::default());
        }
        vec![]
    }

    /// Looks up the invite in what someone typed or pasted into Join a
    /// Server.
    pub(super) fn find_invite(&mut self, typed: String) -> Vec<Effect> {
        let Some(join) = self.state.join.as_mut().filter(|join| !join.joining) else {
            return vec![];
        };
        let Some(code) = typed_code(&typed) else {
            join.error = Some("Paste an invite link, such as https://discord.gg/yapfast.".into());
            return vec![];
        };
        *join = JoinDialog {
            code: Some(code.clone()),
            resolving: true,
            ..JoinDialog::default()
        };
        self.resolve(code, Some(typed))
    }

    /// Opens the dialog for a clicked invite link, by its code, and looks
    /// the invite up.
    pub(super) fn open_invite_link(&mut self, code: String) -> Vec<Effect> {
        if !self.state.discord_online && !self.state.preview {
            return vec![];
        }
        self.state.join = Some(JoinDialog {
            code: Some(code.clone()),
            from_link: true,
            resolving: true,
            ..JoinDialog::default()
        });
        self.resolve(code, None)
    }

    fn resolve(&self, code: String, typed: Option<String>) -> Vec<Effect> {
        if self.state.preview {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ResolveInvite { code, typed },
        ))]
    }

    pub(super) fn invite_resolved(
        &mut self,
        code: String,
        result: Result<InvitePreview, String>,
    ) -> Vec<Effect> {
        let Some(join) = self
            .state
            .join
            .as_mut()
            .filter(|join| join.resolving && join.code.as_ref() == Some(&code))
        else {
            return vec![];
        };
        join.resolving = false;
        let preview = match result {
            Ok(preview) => preview,
            Err(error) => {
                join.error = Some(error);
                return vec![];
            }
        };
        join.member = self
            .state
            .servers
            .iter()
            .any(|server| server.id == preview.server);
        // A clicked link to a server this account is in goes there, as in
        // Discord.
        if join.member && join.from_link {
            return self.open_joined(&preview.server, preview.room);
        }
        join.preview = Some(preview);
        vec![]
    }

    /// Joins by the invite the dialog shows, or opens its server for a
    /// member.
    pub(super) fn join_server(&mut self) -> Vec<Effect> {
        let Some(join) = self
            .state
            .join
            .as_mut()
            .filter(|join| !join.joining && join.form.is_none())
        else {
            return vec![];
        };
        let Some(preview) = join.preview.clone() else {
            return vec![];
        };
        if join.member {
            return self.open_joined(&preview.server, preview.room);
        }
        if self.state.preview {
            return vec![];
        }
        self.state.join = None;
        self.pending_open = Some(preview.room);
        vec![Effect::OpenPage(format!(
            "https://discord.gg/{}",
            preview.code
        ))]
    }

    pub(super) fn join_captcha(&mut self, code: String, page: String) -> Vec<Effect> {
        let Some(join) = self
            .state
            .join
            .as_mut()
            .filter(|join| join.joining && join.code.as_ref() == Some(&code))
        else {
            return vec![];
        };
        join.captcha = Some(page.clone());
        vec![Effect::OpenPage(page)]
    }

    /// Opens the captcha Discord asked for again, as linking's does.
    pub(super) fn open_join_captcha(&self) -> Vec<Effect> {
        match self
            .state
            .join
            .as_ref()
            .and_then(|join| join.captcha.clone())
        {
            Some(page) => vec![Effect::OpenPage(page)],
            None => vec![],
        }
    }

    pub(super) fn server_joined(
        &mut self,
        code: String,
        result: Result<JoinedServer, String>,
    ) -> Vec<Effect> {
        let Some(join) = self
            .state
            .join
            .as_mut()
            .filter(|join| join.joining && join.code.as_ref() == Some(&code))
        else {
            return vec![];
        };
        join.joining = false;
        join.captcha = None;
        let joined = match result {
            Ok(joined) => joined,
            Err(error) => {
                join.error = Some(error);
                return vec![];
            }
        };
        if joined.form {
            // The server lists once Discord sends it, and the dialog stays
            // to send the form to the web app.
            join.form = Some(match discord::parse_room(&joined.room) {
                Some((guild, channel)) => {
                    format!("https://discord.com/channels/{guild}/{channel}")
                }
                None => "https://discord.com/app".into(),
            });
            self.pending_open = Some(joined.room);
            return vec![];
        }
        self.open_joined(&joined.server, joined.room)
    }

    /// Opens the server's form in Discord's web app, and closes the dialog.
    pub(super) fn open_join_form(&mut self) -> Vec<Effect> {
        match self.state.join.take().and_then(|join| join.form) {
            Some(page) => vec![Effect::OpenPage(page)],
            None => vec![],
        }
    }

    /// Closes the dialog and opens the invite's channel, now or once it
    /// lists, or else its server.
    fn open_joined(&mut self, server: &str, room: String) -> Vec<Effect> {
        self.state.join = None;
        if self.state.rooms.iter().any(|listed| listed.id == room) {
            return self.dispatch(Action::Open(room));
        }
        if self.state.servers.iter().any(|listed| listed.id == server) {
            return self.dispatch(Action::Navigate(Navigation::Server(server.to_owned())));
        }
        self.pending_open = Some(room);
        vec![]
    }
}

/// The invite code in what someone typed, as Discord reads it.
fn typed_code(typed: &str) -> Option<String> {
    #[cfg(feature = "discord")]
    {
        yapfast_discord::invite::typed_code(typed).map(str::to_owned)
    }
    #[cfg(not(feature = "discord"))]
    {
        let _ = typed;
        None
    }
}
