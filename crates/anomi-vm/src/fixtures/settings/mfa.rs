//! Samples of the two-factor prompt for tests and shots, after how
//! `show_mfa` words it (`src/ui/server_settings.rs:410`): task S11's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::menu::Row;
use crate::settings::mfa::{Intent, Method, Mfa, WARNING_WAIT};
use crate::shell::Layer;

/// The window with the prompt open over it.
pub fn asked(mfa: Mfa) -> Vm {
    let mut vm = shell::dm_home();
    vm.settings.mfa = mfa;
    shell::open(vm, Layer::Mfa)
}

/// The four ways, as `show_mfa` words them.
pub fn choices() -> Vec<Row<Intent>> {
    [
        (Method::App, tr!("Use your authenticator app")),
        (Method::Sms, tr!("Use a code sent to your phone")),
        (Method::Password, tr!("Use your password")),
        (Method::Backup, tr!("Use a backup code")),
    ]
    .into_iter()
    .map(|(method, text)| Row::new(text, Intent::Choose(method)))
    .collect()
}

/// Nothing chosen yet: the list of ways.
pub fn listing() -> Mfa {
    Mfa {
        id: 1,
        listing: true,
        choices: choices(),
        ..Mfa::default()
    }
}

/// The authenticator app's code, empty.
pub fn app() -> Mfa {
    Mfa {
        id: 1,
        text: tr!("Enter the 6-digit code from your authenticator app.").into(),
        field: tr!("6-digit code").into(),
        choices: choices(),
        ..Mfa::default()
    }
}

/// The app's code typed in full, so Confirm works.
pub fn app_ready() -> Mfa {
    Mfa {
        code: "123456".into(),
        ready: true,
        ..app()
    }
}

/// The code sent and waiting for Discord.
pub fn busy() -> Mfa {
    Mfa {
        busy: true,
        ready: false,
        ..app_ready()
    }
}

/// Discord refused the code.
pub fn refused() -> Mfa {
    Mfa {
        error: "Invalid two-factor code".into(),
        ..app_ready()
    }
}

/// A text's code, with the number it went to and Resend.
pub fn sms() -> Mfa {
    Mfa {
        text: tr!(
            "Enter the 6-digit code Discord sent to {}.",
            "+1 ••• ••• 4821"
        ),
        field: tr!("6-digit code").into(),
        resend: true,
        ..app()
    }
}

/// The text still on its way.
pub fn sms_sending() -> Mfa {
    Mfa {
        text: tr!("Discord is sending a code to your phone.").into(),
        ..app()
    }
}

/// The password.
pub fn password() -> Mfa {
    Mfa {
        text: tr!("Enter your password.").into(),
        field: tr!("Password").into(),
        password: true,
        code: "hunter2".into(),
        ready: true,
        ..app()
    }
}

/// A backup code.
pub fn backup() -> Mfa {
    Mfa {
        text: tr!("Enter one of your 8-digit backup codes. Each works only once.").into(),
        field: tr!("Backup code").into(),
        ..app()
    }
}

/// Discord's warning before a backup code, its button still waiting.
pub fn warning() -> Mfa {
    Mfa {
        warning: true,
        wait: WARNING_WAIT,
        ..backup()
    }
}

/// The warning once its wait is over.
pub fn warning_over() -> Mfa {
    Mfa {
        wait: 0,
        ..warning()
    }
}

/// The warning of an account with no other way: no "go back" line.
pub fn warning_alone() -> Mfa {
    Mfa {
        choices: choices().into_iter().take(1).collect(),
        wait: 1,
        ..warning()
    }
}

/// Only a passkey: Discord's words and no field, and no usable method, so
/// no choices.
pub fn passkey() -> Mfa {
    Mfa {
        id: 1,
        text: tr!(
            "Discord asks for a passkey or security key, which Yapfast can't use. Confirm in Discord's app, then try again."
        )
        .into(),
        ..Mfa::default()
    }
}
