//! Samples of a channel's settings frame and its overview for tests and
//! shots, after the fake Discord's data: task X13's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::icon::Icon;
use crate::settings::channel::{Channel, Kind, Page, Row};
use crate::settings::frame::Unsaved;
use crate::shell::Layer;

/// The sections of a channel the account may edit and delete.
fn rows(name: &str, kind: Kind) -> Vec<Row> {
    let mut rows = vec![Row::Heading(name.to_uppercase())];
    rows.push(Row::Page {
        text: tr!("Overview").to_string(),
        icon: Icon::Pencil,
        page: Page::Overview,
        danger: false,
    });
    rows.push(Row::Page {
        text: tr!("Permissions").to_string(),
        icon: Icon::Shield,
        page: Page::Permissions,
        danger: false,
    });
    rows.push(Row::Line);
    rows.push(Row::Page {
        text: if kind == Kind::Category {
            tr!("Delete Category").to_string()
        } else {
            tr!("Delete Channel").to_string()
        },
        icon: Icon::Trash,
        page: Page::Delete,
        danger: true,
    });
    rows
}

/// `#general` of the fake Discord, open on its overview: its topic, no
/// slowmode, no age restriction.
pub fn overview() -> Channel {
    Channel {
        rows: rows("#general", Kind::Text),
        page: Page::Overview,
        kind: Kind::Text,
        title: tr!("Overview").to_string(),
        close: tr!("Close {}", tr!("Channel settings")),
        name: "general".into(),
        topic: "Co-op nights, plans and links. Ask <@1400000000000000002> to join, keep \
                <#1400000000000000102> on topic, and read the **house rules** at \
                https://example.com/rules first."
            .into(),
        ..Channel::default()
    }
}

/// A voice channel's: its name alone.
pub fn voice() -> Channel {
    Channel {
        rows: rows("Lounge", Kind::Voice),
        kind: Kind::Voice,
        name: "Lounge".into(),
        ..overview()
    }
}

/// A category's: its name alone, its sections' last named for it.
pub fn category() -> Channel {
    Channel {
        rows: rows("Text Channels", Kind::Category),
        kind: Kind::Category,
        name: "Text Channels".into(),
        ..overview()
    }
}

/// A draft changed: a 5 m slowmode and age restriction, the bar up.
pub fn changed() -> Channel {
    Channel {
        slowmode: 7,
        nsfw: true,
        unsaved: Some(Unsaved::default()),
        ..overview()
    }
}

/// The draft on its way.
pub fn saving() -> Channel {
    Channel {
        saving: true,
        unsaved: Some(Unsaved {
            saving: true,
            ..Unsaved::default()
        }),
        ..changed()
    }
}

/// Discord still hasn't sent the channel.
pub fn loading() -> Channel {
    Channel {
        loading: true,
        topic: String::new(),
        ..overview()
    }
}

/// A draft Discord refused.
pub fn refused() -> Channel {
    Channel {
        error: "Missing Permissions".into(),
        unsaved: Some(Unsaved::default()),
        ..overview()
    }
}

/// The window with the channel's settings open on `channel`.
pub fn asked(channel: Channel) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ChannelSettings);
    vm.settings.channel = channel;
    vm
}
