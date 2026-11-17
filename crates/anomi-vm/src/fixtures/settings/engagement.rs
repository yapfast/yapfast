//! Samples of a server's Engagement settings for tests and shots, after the
//! fake Discord's data: task X02's.

use crate::Vm;
use crate::fixtures::settings::server::server;
use crate::fixtures::shell;
use crate::settings::engagement::Engagement;
use crate::settings::server::Section;
use crate::shell::Layer;
use yapfast_lang::tr;

/// The fake's server before anything is set: its text and voice channels,
/// no system channel, so the switches wait.
pub fn engagement() -> Engagement {
    let none = |words: &str, channels: &[&str]| {
        std::iter::once(words)
            .chain(channels.iter().copied())
            .map(String::from)
            .collect()
    };
    Engagement {
        system_channels: none(
            tr!("No System Messages"),
            &["general", "memes", "announcements"],
        ),
        afk_channels: none(tr!("No Inactive Channel"), &["Lounge", "Gaming"]),
        afk_timeout: 1,
        ..Engagement::default()
    }
}

/// As the golden screenshot has it: system messages in #general with every
/// switch on, only @mentions by default, no inactive channel.
pub fn with_system_channel() -> Engagement {
    Engagement {
        system_channel: 1,
        welcome: true,
        welcome_sticker: true,
        boosted: true,
        setup_tips: true,
        notifications: 1,
        ..engagement()
    }
}

/// The window with the server's settings open on Engagement.
fn window(page: Engagement) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = crate::settings::server::Server {
        section: Section::Engagement,
        ..server()
    };
    vm.settings.engagement = page;
    vm
}

/// The page as the golden screenshot has it.
pub fn page() -> Vm {
    window(with_system_channel())
}

/// The page with nothing set: the switches and the timeout wait.
pub fn unset() -> Vm {
    window(engagement())
}
