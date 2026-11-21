//! Joining a Discord server by an invite: Join a Server, from Add a Server,
//! takes a pasted link or code, and a clicked
//! invite link opens the same dialog with it. Either looks the invite up
//! and shows its server before anyone joins; joining waits for the click.

use super::*;
use crate::chat::{InvitePreview, JoinedServer, ScreeningForm, ScreeningKind, ScreeningReview};

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
    /// The server, joined, asks new members to fill in its form first.
    pub form: Option<ScreeningDialog>,
    pub error: Option<String>,
}

/// What a person answers to a question of a server's form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningAnswer {
    /// Agreeing to the rules, or taking it back.
    Agree(bool),
    /// The words of a text, whole.
    Text(String),
    /// A choice, by its place.
    Choose(usize),
}

/// The longest answer of a text, as Discord takes a paragraph.
const ANSWER_LIMIT: usize = 1000;

/// A joined server's form, while the dialog shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreeningDialog {
    pub server: String,
    /// The code of the invite the server was joined by.
    pub code: String,
    /// The form with its answers, once Discord sent it.
    pub form: Option<ScreeningForm>,
    pub loading: bool,
    pub submitting: bool,
    /// The form went in, and the server's moderators decide.
    pub waiting: bool,
    pub error: Option<String>,
}

impl Application {
    /// Opens Join a Server, empty; nothing without a linked Discord account.
    pub(super) fn open_join(&mut self) -> Vec<Effect> {
        if (self.state.discord_online || self.state.preview) && !self.creating_server() {
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
        if (!self.state.discord_online && !self.state.preview) || self.creating_server() {
            return vec![];
        }
        self.state.server_creation = None;
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
        join.joining = true;
        join.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::JoinServer {
                preview: Box::new(preview),
                from_link: join.from_link,
            },
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
        self.state.server_creation = None;
        if joined.form {
            // The server lists once Discord sends it, and the dialog stays
            // to show the server's form.
            join.form = Some(ScreeningDialog {
                server: joined.server.clone(),
                code: code.clone(),
                loading: true,
                ..ScreeningDialog::default()
            });
            self.pending_open = Some(joined.room);
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::LoadScreening {
                    server: joined.server,
                    code,
                },
            ))];
        }
        self.open_joined(&joined.server, joined.room)
    }

    fn screening(&mut self) -> Option<&mut ScreeningDialog> {
        self.state.join.as_mut()?.form.as_mut()
    }

    pub(super) fn screening_loaded(
        &mut self,
        server: String,
        result: Result<ScreeningForm, String>,
    ) -> Vec<Effect> {
        let waiting = self
            .screening()
            .is_some_and(|dialog| dialog.server == server && dialog.loading);
        if !waiting {
            // A server joined from Discovery asks for nothing until its form
            // is read, and one without a form answers with an error, which
            // is no news. A form it has is shown as a joined invite's.
            if self.state.join.is_none()
                && self
                    .state
                    .discover
                    .joined
                    .iter()
                    .any(|id| discord::server_id(id) == server)
                && let Ok(form) = result
                && !form.fields.is_empty()
            {
                self.state.join = Some(JoinDialog {
                    form: Some(ScreeningDialog {
                        server,
                        form: Some(form),
                        ..ScreeningDialog::default()
                    }),
                    ..JoinDialog::default()
                });
            }
            return vec![];
        }
        let Some(dialog) = self.screening() else {
            return vec![];
        };
        dialog.loading = false;
        match result {
            Ok(form) => dialog.form = Some(form),
            Err(error) => dialog.error = Some(error),
        }
        vec![]
    }

    /// Answers a question of the form; one that doesn't fit its question is
    /// left out.
    pub(super) fn answer_screening(
        &mut self,
        index: usize,
        answer: ScreeningAnswer,
    ) -> Vec<Effect> {
        let Some(field) = self
            .screening()
            .filter(|dialog| !dialog.submitting && !dialog.waiting)
            .and_then(|dialog| dialog.form.as_mut())
            .and_then(|form| form.fields.get_mut(index))
        else {
            return vec![];
        };
        match (field.kind, answer) {
            (ScreeningKind::Terms, ScreeningAnswer::Agree(agreed)) => field.agreed = agreed,
            (ScreeningKind::Text | ScreeningKind::Paragraph, ScreeningAnswer::Text(text)) => {
                field.text = text.chars().take(ANSWER_LIMIT).collect();
            }
            (ScreeningKind::Choice, ScreeningAnswer::Choose(choice))
                if choice < field.values.len() =>
            {
                field.choice = Some(choice);
            }
            _ => {}
        }
        vec![]
    }

    /// Hands the form in once every question that must be answered is, or
    /// reads it again when it didn't load.
    pub(super) fn submit_screening(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .screening()
            .filter(|dialog| !dialog.loading && !dialog.submitting && !dialog.waiting)
        else {
            return vec![];
        };
        let server = dialog.server.clone();
        let Some(form) = dialog.form.clone() else {
            dialog.loading = true;
            dialog.error = None;
            let code = dialog.code.clone();
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::LoadScreening { server, code },
            ))];
        };
        if !form.complete() {
            return vec![];
        }
        dialog.submitting = true;
        dialog.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SubmitScreening {
                server,
                form: Box::new(form),
            },
        ))]
    }

    pub(super) fn screening_submitted(
        &mut self,
        server: String,
        result: Result<ScreeningReview, String>,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .screening()
            .filter(|dialog| dialog.server == server && dialog.submitting)
        else {
            return vec![];
        };
        dialog.submitting = false;
        match result {
            Ok(ScreeningReview::Approved) => {
                // The server's channel opens as it lists, as for any join.
                self.state.join = None;
            }
            Ok(ScreeningReview::Waiting) => dialog.waiting = true,
            Ok(ScreeningReview::Rejected) => {
                dialog.error = Some("The server turned your answers down.".into());
            }
            Err(error) => dialog.error = Some(error),
        }
        vec![]
    }

    /// Closes the dialog and opens the invite's channel, now or once it
    /// lists, or else its server.
    fn open_joined(&mut self, server: &str, room: String) -> Vec<Effect> {
        self.state.join = None;
        self.state.server_creation = None;
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
