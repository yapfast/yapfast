//! Samples of deleting a server or handing it over for tests and shots,
//! after `ownership_prompt` (`src/ui/server_settings.rs:348`): task X12's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::{pictures, shell};
use crate::kind::Avatar;
use crate::settings::ownership::{Kind, Ownership};
use crate::shell::Layer;
use crate::text::{Builder, Style, Text};

fn bold() -> Style {
    Style {
        bold: true,
        ..Style::default()
    }
}

/// A translated sentence with `names` in bold, one per `{}`.
fn sentence(words: &str, names: &[&str]) -> Text {
    let mut builder = Builder::new();
    builder.template(words, Style::default(), |builder, place| {
        builder.text(names.get(place).copied().unwrap_or_default(), bold());
    });
    builder.finish()
}

fn person(initial: &str, name: &str) -> Avatar {
    pictures::pictured(
        Avatar {
            initial: initial.into(),
            ..Avatar::default()
        },
        name,
    )
}

/// Delete Server, the name not typed yet.
pub fn delete() -> Ownership {
    Ownership {
        kind: Kind::Delete,
        title: tr!("Delete '{}'", "Server"),
        text: sentence(
            tr!("Are you sure you want to delete {}? This action cannot be undone."),
            &["Server"],
        ),
        by_name: true,
        ..Ownership::default()
    }
}

/// The name typed wrongly.
pub fn delete_wrong() -> Ownership {
    Ownership {
        typed: "Serve".into(),
        wrong: true,
        ..delete()
    }
}

/// The name typed right.
pub fn delete_typed() -> Ownership {
    Ownership {
        typed: "Server".into(),
        ..delete()
    }
}

/// Deleting, sent and waiting for Discord.
pub fn deleting() -> Ownership {
    Ownership {
        sending: true,
        ..delete_typed()
    }
}

/// Discord refused.
pub fn delete_refused() -> Ownership {
    Ownership {
        error: "Couldn't delete the server: Two-factor authentication is required.".into(),
        ..delete_typed()
    }
}

/// Transfer Ownership to Bob, the acknowledgement not ticked.
pub fn transfer() -> Ownership {
    Ownership {
        kind: Kind::Transfer,
        title: tr!("Transfer Ownership").into(),
        text: sentence(
            tr!("This will transfer ownership of {} to {}. This cannot be undone!"),
            &["Server", "bob"],
        ),
        owner: person("A", "Alice"),
        new_owner: person("B", "Bob"),
        acknowledge: sentence(
            tr!(
                "I acknowledge that by transferring ownership of this server to {}, it officially belongs to them."
            ),
            &["bob"],
        ),
        ..Ownership::default()
    }
}

/// Ticked, so the button works.
pub fn transfer_agreed() -> Ownership {
    Ownership {
        agreed: true,
        may_confirm: true,
        ..transfer()
    }
}

/// A Verified or Partnered server: a warning, and nothing may be ticked.
pub fn transfer_verified() -> Ownership {
    Ownership {
        verified: true,
        ..transfer()
    }
}

/// Sent and waiting.
pub fn transferring() -> Ownership {
    Ownership {
        sending: true,
        may_confirm: false,
        ..transfer_agreed()
    }
}

/// Discord refused.
pub fn transfer_refused() -> Ownership {
    Ownership {
        error: "Couldn't transfer the server: that member can't take over its subscriptions."
            .into(),
        ..transfer_agreed()
    }
}

/// The emailed code's step, nothing typed.
pub fn code() -> Ownership {
    Ownership {
        code: true,
        ..transfer_agreed()
    }
}

/// The code typed.
pub fn code_typed() -> Ownership {
    Ownership {
        typed: "123456".into(),
        ..code()
    }
}

/// Another code was asked for, and sent.
pub fn code_resent() -> Ownership {
    Ownership {
        resent: true,
        ..code()
    }
}

/// The code was refused.
pub fn code_refused() -> Ownership {
    Ownership {
        typed: "000000".into(),
        error: "Invalid verification code.".into(),
        ..code()
    }
}

/// The server's settings with `ownership` asked over them.
pub fn asked(ownership: Ownership) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.ownership = ownership;
    shell::open(vm, Layer::Ownership)
}
