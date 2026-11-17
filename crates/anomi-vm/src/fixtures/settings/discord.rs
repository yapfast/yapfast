//! Samples of the Discord settings page for tests and shots, after the fake
//! Discord's data: task S08's.

use crate::Vm;
use crate::fixtures::avatar;
use crate::fixtures::pictures::pictured;
use crate::fixtures::settings::frame::settings;
use crate::settings::discord::{Blocked, Discord};
use crate::settings::frame::Page;

/// The page as the golden screenshot has it: linked, the four switches at
/// the fake's defaults, the fake's Dave blocked.
pub fn discord() -> Discord {
    Discord {
        sync_reads: true,
        show_idle: true,
        blocked: vec![Blocked {
            id: "1400000000000000004".into(),
            name: "Dave".into(),
            username: "dave".into(),
            avatar: avatar("D", 0x5865f2),
        }],
        ..Discord::default()
    }
}

/// The page with nobody blocked: no list.
pub fn nobody_blocked() -> Discord {
    Discord {
        blocked: Vec::new(),
        ..discord()
    }
}

/// The window with the settings open on Discord.
pub fn page() -> Vm {
    with(discord())
}

/// The window with Discord as `page` has it.
pub fn with(page: Discord) -> Vm {
    let mut vm = settings(Page::Discord);
    vm.settings.discord = page;
    vm
}

/// `count` blocked accounts, the first as the fake's people.
pub fn blocked(count: usize) -> Vec<Blocked> {
    (0..count)
        .map(|n| {
            let name = match n {
                0 => "Erin".to_owned(),
                1 => "Carol".to_owned(),
                n => format!("Person {n}"),
            };
            let initial = name.chars().next().unwrap_or('?').to_string();
            Blocked {
                id: format!("140000000000000010{n}"),
                username: if n == 1 {
                    String::new()
                } else {
                    name.to_lowercase().replace(' ', "")
                },
                avatar: pictured(avatar(&initial, 0x5865f2), &name),
                name,
            }
        })
        .collect()
}

/// Seven blocked, five shown, a Matrix build's Unlink above.
pub fn discord_blocked() -> Discord {
    Discord {
        matrix_build: true,
        blocked: blocked(7),
        ..discord()
    }
}

/// No account linked: the page offers to link one.
pub fn discord_unlinked() -> Discord {
    Discord {
        linked: false,
        ..discord()
    }
}

/// The window with the blocked accounts.
pub fn with_blocked() -> Vm {
    with(discord_blocked())
}

/// The window with no account linked.
pub fn unlinked() -> Vm {
    with(discord_unlinked())
}
