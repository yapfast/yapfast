//! The two-factor prompt from the core's first waiting one (`show_mfa`,
//! `src/ui/server_settings.rs:410`): `Layer::Mfa` is open while there is
//! one, with its words in the language. Its field takes the keys as the
//! prompt or its way changes, the warning before a backup code starts its
//! ten seconds each time it shows, and the ways are built again only when
//! Discord offers others.

use std::borrow::Cow;

use yapfast_core::application::{Application, MfaPrompt};
use yapfast_core::chat::MfaMethod;
use yapfast_lang::tr;

use anomi_vm::menu::Row;
use anomi_vm::settings::mfa::{Intent, Method, Mfa, WARNING_WAIT};
use anomi_vm::shell::{Layer, Shell};

/// What the prompt showed last.
#[derive(Default)]
pub(super) struct Shown {
    /// The prompt and its way, so the field takes the keys as either changes.
    asked: Option<(u64, Option<MfaMethod>)>,
    /// The prompt whose warning showed, for its wait to start once.
    warned: Option<u64>,
    /// The ways the choices were built from.
    methods: Option<Vec<MfaMethod>>,
}

pub(super) fn show(
    core: &Application,
    shell: &mut Shell,
    mfa: &mut Mfa,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let Some(prompt) = core.mfa_prompt() else {
        if shell.is_open(Layer::Mfa) {
            // It plays its way out with what it showed, less the code.
            shell.close(Layer::Mfa);
            super::forget(&mut mfa.code);
        }
        shown.asked = None;
        shown.warned = None;
        return;
    };
    if !shell.is_open(Layer::Mfa) {
        shell.open(Layer::Mfa);
    }
    if shown.methods.as_ref() != Some(&prompt.methods) {
        mfa.choices = prompt.usable().filter_map(choice).collect();
        shown.methods = Some(prompt.methods.clone());
    }
    let (text, field) = words(prompt);
    if mfa.text != text {
        mfa.text = text.into_owned();
    }
    if mfa.field != field {
        field.clone_into(&mut mfa.field);
    }
    mfa.id = prompt.id;
    mfa.listing = prompt.method.is_none() && !mfa.choices.is_empty();
    mfa.warning = prompt.warning();
    if !mfa.warning {
        shown.warned = None;
    } else if shown.warned != Some(prompt.id) {
        shown.warned = Some(prompt.id);
        mfa.wait = WARNING_WAIT;
    }
    mfa.password = prompt.method == Some(MfaMethod::Password);
    if mfa.code != prompt.code {
        mfa.code.clone_from(&prompt.code);
    }
    mfa.resend = prompt.method == Some(MfaMethod::Sms) && prompt.phone.is_some();
    mfa.busy = prompt.busy;
    mfa.ready = prompt.ready();
    let error = prompt.error.as_deref().unwrap_or_default();
    if mfa.error != error {
        error.clone_into(&mut mfa.error);
    }
    let asked = (prompt.id, prompt.method);
    if shown.asked != Some(asked) {
        shown.asked = Some(asked);
        if mfa.asks() {
            *focus = Some(anomi_settings::mfa::CODE);
        }
    }
}

/// A way to confirm as its row says it; none for a passkey, which
/// Yapfast can't use (`usable` leaves it out).
fn choice(method: MfaMethod) -> Option<Row<Intent>> {
    let (text, method) = match method {
        MfaMethod::App => (tr!("Use your authenticator app"), Method::App),
        MfaMethod::Sms => (tr!("Use a code sent to your phone"), Method::Sms),
        MfaMethod::Password => (tr!("Use your password"), Method::Password),
        MfaMethod::Backup => (tr!("Use a backup code"), Method::Backup),
        MfaMethod::Passkey => return None,
    };
    Some(Row::new(text, Intent::Choose(method)))
}

/// The core's way as the prompt's.
pub(crate) fn method(method: Method) -> MfaMethod {
    match method {
        Method::App => MfaMethod::App,
        Method::Sms => MfaMethod::Sms,
        Method::Password => MfaMethod::Password,
        Method::Backup => MfaMethod::Backup,
    }
}

/// What the prompt asks for, and its field's placeholder; none when
/// Discord offers only a passkey.
fn words(prompt: &MfaPrompt) -> (Cow<'static, str>, &'static str) {
    match prompt.method {
        Some(MfaMethod::App) => (
            tr!("Enter the 6-digit code from your authenticator app.").into(),
            tr!("6-digit code"),
        ),
        Some(MfaMethod::Sms) => (
            match &prompt.phone {
                Some(phone) => tr!("Enter the 6-digit code Discord sent to {}.", phone).into(),
                None => tr!("Discord is sending a code to your phone.").into(),
            },
            tr!("6-digit code"),
        ),
        Some(MfaMethod::Password) => (tr!("Enter your password.").into(), tr!("Password")),
        Some(MfaMethod::Backup) => (
            tr!("Enter one of your 8-digit backup codes. Each works only once.").into(),
            tr!("Backup code"),
        ),
        Some(MfaMethod::Passkey) | None => (
            tr!("Discord asks for a passkey or security key, which Yapfast can't use. Confirm in Discord's app, then try again.")
                .into(),
            "",
        ),
    }
}
