//! Discord's "Multi-Factor Authentication" prompt: for a request Discord
//! holds back until this account confirms, with a code from an
//! authenticator app or a text, the password, or a backup code; then the
//! request goes again. Cancelling fails the request. The core keeps the
//! prompts (`AppState::mfa`); this is their view.

use std::time::Duration;

use iced::{
    Alignment, Element, Length, Task,
    widget::{Space, column, operation, row, text},
};
use yapfast_core::{
    application::{Action, Application},
    chat::MfaMethod,
    i18n::tr,
};

use crate::{
    after,
    app::Message,
    controls::{Item, Kind, Light, action, field},
    ctx::{Ctx, Cx},
    dialog::dialog,
    fonts::BOLD,
    pictures::Icon,
};

/// The prompt's field, for focus and the tests.
pub const CODE: &str = "mfa-code";

/// How long Discord's warning before a backup code holds its button back.
const WAIT: u32 = 10;

/// What's done in the prompt.
#[derive(Clone, Debug)]
pub enum Event {
    /// Use a method, or with none list them ("Verify with something else").
    Choose(Option<MfaMethod>),
    Write(String),
    Resend,
    Confirm,
    Cancel,
    /// A second of the warning's wait went by, for the prompt by its id.
    Tick(u64),
}

/// The warning's wait: which prompt's warning it counts for, and the
/// seconds left.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wait {
    pub prompt: Option<u64>,
    pub left: u32,
}

/// What the window keeps of the core's prompts: the one shown, by its id
/// and method, and its warning's wait.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mfa {
    shown: Option<(u64, Option<MfaMethod>)>,
    pub wait: Wait,
}

impl Mfa {
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let action = match event {
            Event::Choose(method) => Action::ChooseMfa(method),
            Event::Write(code) => Action::WriteMfa(code),
            Event::Resend => Action::ResendMfa,
            Event::Confirm => Action::ConfirmMfa,
            Event::Cancel => Action::CancelMfa,
            Event::Tick(id) => {
                if self.wait.prompt == Some(id) && self.wait.left > 0 {
                    self.wait.left -= 1;
                    if self.wait.left > 0 {
                        return after(Duration::from_secs(1), Message::Mfa(Event::Tick(id)));
                    }
                }
                return Task::none();
            }
        };
        cx.dispatch(action);
        Task::none()
    }

    /// Takes in a prompt shown or changed: its field takes the keys, and
    /// the warning before a backup code starts its ten seconds, each time
    /// it shows. Nothing counts while no warning shows.
    pub fn sync(&mut self, app: &Application) -> Task<Message> {
        let Some(prompt) = app.mfa_prompt() else {
            self.wait = Wait::default();
            self.shown = None;
            return Task::none();
        };
        let mut tasks = Vec::new();
        let shown = (prompt.id, prompt.method);
        if self.shown != Some(shown) {
            self.shown = Some(shown);
            tasks.push(if prompt.method.is_some() && !prompt.warning() {
                operation::focus(CODE)
            } else {
                operation::focus(crate::app::NOBODY)
            });
        }
        if prompt.warning() {
            if self.wait.prompt != Some(prompt.id) {
                self.wait = Wait {
                    prompt: Some(prompt.id),
                    left: WAIT,
                };
                tasks.push(after(
                    Duration::from_secs(1),
                    Message::Mfa(Event::Tick(prompt.id)),
                ));
            }
        } else {
            self.wait = Wait::default();
        }
        Task::batch(tasks)
    }
}

/// The prompt over the dimmed window, while the core has one.
pub fn view<'a>(mfa: &'a Mfa, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let prompt = cx.app.mfa_prompt()?;
    let theme = cx.look.theme;
    let choices: Vec<MfaMethod> = prompt.usable().collect();
    let listing = prompt.method.is_none() && !choices.is_empty();
    let warning = prompt.warning();
    let (words, placeholder) = match prompt.method {
        Some(MfaMethod::App) => (
            tr("Enter the 6-digit code from your authenticator app.").to_owned(),
            "6-digit code",
        ),
        Some(MfaMethod::Sms) => (
            match &prompt.phone {
                Some(phone) => format!("Enter the 6-digit code Discord sent to {phone}."),
                None => tr("Discord is sending a code to your phone.").to_owned(),
            },
            "6-digit code",
        ),
        Some(MfaMethod::Password) => (tr("Enter your password.").to_owned(), tr("Password")),
        Some(MfaMethod::Backup) => (
            tr("Enter one of your 8-digit backup codes. Each works only once.").to_owned(),
            tr("Backup code"),
        ),
        Some(MfaMethod::Passkey) | None => (
            tr(
                "Discord asks for a passkey or security key, which Yapfast can't use. Confirm \
             in Discord's app, then try again.",
            )
            .to_owned(),
            "",
        ),
    };
    let note = |words: String| text(words).size(13).color(theme.muted).width(Length::Fill);
    let close = cx
        .look
        .icon_button(Icon::X, tr("Cancel"), Message::Mfa(Event::Cancel));
    let mut body = column![
        row![
            text(tr("Multi-Factor Authentication"))
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            close
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(12)
    .padding(20);
    if listing {
        let mut list = column![note(tr("Choose how to confirm it's you.").into())].spacing(2);
        for method in &choices {
            let label = match method {
                MfaMethod::App => tr("Use your authenticator app"),
                MfaMethod::Sms => tr("Use a code sent to your phone"),
                MfaMethod::Password => tr("Use your password"),
                MfaMethod::Backup => tr("Use a backup code"),
                MfaMethod::Passkey => tr("Use a passkey or security key"),
            };
            list = list.push(cx.look.menu_item(
                Item {
                    label: label.into(),
                    icon: None,
                    danger: false,
                    slot: true,
                    disabled: false,
                    subtitle: None,
                    check: crate::controls::Check::None,
                },
                Light::Pointer,
                Message::Mfa(Event::Choose(Some(*method))),
            ));
        }
        body = body.push(list);
    }
    if warning {
        body = body.push(
            text(tr("Slow down! Are you sure?"))
                .size(16)
                .font(BOLD)
                .color(theme.text),
        );
        body = body.push(note(
            tr(
                "Backup codes can only be used one time each and are for emergency access to \
             your account!",
            )
            .into(),
        ));
        if choices.len() > 1 {
            body = body.push(note(
                tr(
                    "If you have access to any of your other methods, select ‘Verify with \
                 something else’ to go back.",
                )
                .into(),
            ));
        }
        let left = mfa.wait.left;
        body = body.push(note(if left > 0 {
            format!(
                "If you do not have access to any other methods, the button below will \
                 activate in {left} seconds."
            )
        } else {
            tr(
                "If you do not have access to any other methods, use the other button below \
             to proceed.",
            )
            .to_owned()
        }));
        body = body.push(
            action(
                theme,
                tr("I’m sure, use a backup code"),
                Kind::Primary,
                (left == 0).then_some(Message::Mfa(Event::Confirm)),
            )
            .wide(36.0),
        );
    }
    let asks = !listing && !warning;
    if asks {
        body = body.push(note(words));
        if !placeholder.is_empty() {
            let mut code = field(
                theme,
                CODE,
                placeholder,
                &prompt.code,
                prompt.method == Some(MfaMethod::Password),
                !prompt.busy,
            )
            .size(15)
            .padding([8, 10]);
            if !prompt.busy {
                code = code.on_input(|code| Message::Mfa(Event::Write(code)));
                if prompt.ready() {
                    code = code.on_submit(Message::Mfa(Event::Confirm));
                }
            }
            body = body.push(code);
        }
    }
    if let Some(error) = &prompt.error {
        body = body.push(crate::form::error(theme, error.clone()).width(Length::Fill));
    }
    let open = !prompt.busy;
    let mut buttons = row![].spacing(8).align_y(Alignment::Center);
    if !listing && choices.len() > 1 {
        buttons = buttons.push(action(
            theme,
            tr("Verify with something else"),
            Kind::Ghost,
            open.then_some(Message::Mfa(Event::Choose(None))),
        ));
    }
    if !listing && prompt.method == Some(MfaMethod::Sms) && prompt.phone.is_some() {
        buttons = buttons.push(action(
            theme,
            tr("Resend Code"),
            Kind::Ghost,
            open.then_some(Message::Mfa(Event::Resend)),
        ));
    }
    buttons = buttons.push(Space::new().width(Length::Fill));
    buttons = buttons.push(action(
        theme,
        tr("Cancel"),
        Kind::Ghost,
        Some(Message::Mfa(Event::Cancel)),
    ));
    if asks && !placeholder.is_empty() {
        buttons = buttons.push(action(
            theme,
            if prompt.busy {
                tr("Confirming...")
            } else {
                tr("Confirm")
            },
            Kind::Primary,
            prompt.ready().then_some(Message::Mfa(Event::Confirm)),
        ));
    }
    body = body.push(buttons);
    let width = (cx.look.window.width - 48.0).min(440.0);
    Some(dialog(theme, body.into(), width, Message::Nothing))
}
