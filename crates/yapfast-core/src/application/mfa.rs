//! The prompt Discord's client shows when Discord holds a request back for
//! two-factor authentication: Discord offers its methods, the person
//! confirms with one, and the request goes again. Several requests held at
//! once each get a prompt, one after another. See "Two-factor
//! authentication" in docs/discord/server-settings.md.

use super::*;
use crate::chat::MfaMethod;

/// A prompt for a request Discord holds back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MfaPrompt {
    /// The prompt's id, as the Discord service knows it.
    pub id: u64,
    /// The methods Discord offers, in its order.
    pub methods: Vec<MfaMethod>,
    /// The method being used; none while choosing another, as Discord's
    /// "Verify with something else" lists them.
    pub method: Option<MfaMethod>,
    /// The code, password or backup code as written.
    pub code: String,
    /// Where Discord texted a code, once it did.
    pub phone: Option<String>,
    /// Waiting for Discord to take the confirmation.
    pub busy: bool,
    pub error: Option<String>,
    /// A backup code was chosen past Discord's warning, "Slow down! Are you
    /// sure?", which shows first each time it's chosen.
    pub warned: bool,
}

impl MfaPrompt {
    /// Whether Discord's warning before a backup code shows.
    pub fn warning(&self) -> bool {
        self.method == Some(MfaMethod::Backup) && !self.warned
    }

    /// The methods Yapfast can confirm with: all but a passkey, which needs
    /// Discord's app.
    pub fn usable(&self) -> impl Iterator<Item = MfaMethod> + '_ {
        self.methods
            .iter()
            .copied()
            .filter(|method| *method != MfaMethod::Passkey)
    }

    /// The code as Discord takes it: a backup code without its dashes, the
    /// rest as written without spaces at its ends.
    pub fn answer(&self) -> String {
        match self.method {
            Some(MfaMethod::Backup) => self.code.trim().replace('-', ""),
            Some(MfaMethod::Password) => self.code.clone(),
            _ => self.code.trim().replace(' ', ""),
        }
    }

    /// Whether the code is one Discord could take, as Discord's client
    /// checks before sending: six digits from an app or a text, eight to
    /// eleven characters of a backup code, any password.
    pub fn ready(&self) -> bool {
        let answer = self.answer();
        !self.busy
            && match self.method {
                Some(MfaMethod::App | MfaMethod::Sms) => {
                    answer.len() == 6 && answer.bytes().all(|byte| byte.is_ascii_digit())
                }
                Some(MfaMethod::Backup) => (8..=11).contains(&answer.chars().count()),
                Some(MfaMethod::Password) => !answer.is_empty(),
                Some(MfaMethod::Passkey) | None => false,
            }
    }
}

impl Application {
    /// The prompt showing: the first of those waiting.
    pub fn mfa_prompt(&self) -> Option<&MfaPrompt> {
        self.state.mfa.first()
    }

    /// Discord held a request back: a prompt opens, at the first method
    /// Yapfast can use. A text goes out as Discord's client sends one when
    /// it starts there.
    pub(super) fn mfa_required(&mut self, id: u64, methods: Vec<MfaMethod>) -> Vec<Effect> {
        let mut prompt = MfaPrompt {
            id,
            methods,
            method: None,
            code: String::new(),
            phone: None,
            busy: false,
            error: None,
            warned: false,
        };
        let first = prompt.usable().next();
        prompt.method = first;
        let text = prompt.method == Some(MfaMethod::Sms);
        self.state.mfa.push(prompt);
        if text && self.state.mfa.len() == 1 {
            return vec![Effect::Backend(Command::Discord(DiscordCommand::TextMfa(
                id,
            )))];
        }
        vec![]
    }

    /// Uses another method, or with none lists them.
    pub(super) fn choose_mfa(&mut self, method: Option<MfaMethod>) -> Vec<Effect> {
        let Some(prompt) = self.state.mfa.first_mut().filter(|prompt| !prompt.busy) else {
            return vec![];
        };
        if method.is_some_and(|method| !prompt.usable().any(|usable| usable == method)) {
            return vec![];
        }
        prompt.method = method;
        prompt.code.clear();
        prompt.error = None;
        prompt.warned = false;
        if method == Some(MfaMethod::Sms) && prompt.phone.is_none() {
            return vec![Effect::Backend(Command::Discord(DiscordCommand::TextMfa(
                prompt.id,
            )))];
        }
        vec![]
    }

    pub(super) fn write_mfa(&mut self, code: String) {
        if let Some(prompt) = self.state.mfa.first_mut() {
            prompt.code = code;
        }
    }

    /// Has Discord text the code again.
    pub(super) fn resend_mfa(&mut self) -> Vec<Effect> {
        let Some(prompt) = self
            .state
            .mfa
            .first_mut()
            .filter(|prompt| prompt.method == Some(MfaMethod::Sms) && !prompt.busy)
        else {
            return vec![];
        };
        prompt.error = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::TextMfa(
            prompt.id,
        )))]
    }

    /// Confirms with the code, or past the warning before a backup code.
    pub(super) fn confirm_mfa(&mut self) -> Vec<Effect> {
        if let Some(prompt) = self
            .state
            .mfa
            .first_mut()
            .filter(|prompt| prompt.warning() && !prompt.busy)
        {
            prompt.warned = true;
            return vec![];
        }
        let Some(prompt) = self.state.mfa.first_mut().filter(|prompt| prompt.ready()) else {
            return vec![];
        };
        let Some(method) = prompt.method else {
            return vec![];
        };
        prompt.busy = true;
        prompt.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::FinishMfa {
                id: prompt.id,
                method,
                code: prompt.answer(),
            },
        ))]
    }

    /// Closes the prompt; the request waiting on it fails, and the next
    /// prompt shows.
    pub(super) fn cancel_mfa(&mut self) -> Vec<Effect> {
        if self.state.mfa.is_empty() {
            return vec![];
        }
        let prompt = self.state.mfa.remove(0);
        let mut effects = vec![Effect::Backend(Command::Discord(
            DiscordCommand::CancelMfa(prompt.id),
        ))];
        effects.extend(self.next_mfa());
        effects
    }

    pub(super) fn mfa_finished(&mut self, id: u64, result: Result<(), String>) -> Vec<Effect> {
        let Some(index) = self.state.mfa.iter().position(|prompt| prompt.id == id) else {
            return vec![];
        };
        match result {
            // The request goes again; another prompt comes if Discord
            // still holds it back.
            Ok(()) => {
                self.state.mfa.remove(index);
                if index == 0 {
                    return self.next_mfa();
                }
            }
            Err(error) => {
                let prompt = &mut self.state.mfa[index];
                prompt.busy = false;
                prompt.error = Some(error);
            }
        }
        vec![]
    }

    pub(super) fn mfa_texted(&mut self, id: u64, result: Result<String, String>) {
        let Some(prompt) = self.state.mfa.iter_mut().find(|prompt| prompt.id == id) else {
            return;
        };
        match result {
            Ok(phone) => prompt.phone = Some(phone),
            Err(error) => prompt.error = Some(error),
        }
    }

    /// The prompt that shows once another closed: a text goes out for it
    /// if it starts there.
    fn next_mfa(&mut self) -> Vec<Effect> {
        match self.state.mfa.first() {
            Some(prompt) if prompt.method == Some(MfaMethod::Sms) && prompt.phone.is_none() => {
                vec![Effect::Backend(Command::Discord(DiscordCommand::TextMfa(
                    prompt.id,
                )))]
            }
            _ => vec![],
        }
    }
}
