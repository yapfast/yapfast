//! Samples of making a channel or a category for tests and shots, after the
//! fake Discord's data (`show_channel_dialog`, `src/ui/refresh.rs:3149`):
//! task X15's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::settings::create_channel::{ChannelType, CreateChannel};
use crate::shell::Layer;

/// Making a text channel, as the server's menu opens it: the type segments,
/// an empty name.
pub fn channel() -> CreateChannel {
    CreateChannel {
        title: tr!("Create channel").into(),
        detail: tr!("In {}", "Server"),
        confirm: tr!("Create channel").into(),
        kinds: vec![tr!("Text").into(), tr!("Voice").into()],
        named: true,
        field: tr!("CHANNEL NAME").into(),
        placeholder: tr!("new-channel").into(),
        ..CreateChannel::default()
    }
}

/// The same with voice chosen and a name written.
pub fn voice() -> CreateChannel {
    CreateChannel {
        kind: ChannelType::Voice,
        name: "game-night".into(),
        ..channel()
    }
}

/// Making a category: no type, its own wording.
pub fn category() -> CreateChannel {
    CreateChannel {
        title: tr!("Create category").into(),
        confirm: tr!("Create category").into(),
        kinds: Vec::new(),
        field: tr!("CATEGORY NAME").into(),
        placeholder: tr!("New Category").into(),
        ..channel()
    }
}

/// Copying a channel: a name, no type.
pub fn duplicate() -> CreateChannel {
    CreateChannel {
        title: tr!("Duplicate channel").into(),
        detail: tr!(
            "A copy of {}, with its settings and permissions, in the same category.",
            "#general"
        ),
        kinds: Vec::new(),
        name: "general".into(),
        ..channel()
    }
}

/// Deleting a channel: no field, a red button.
pub fn delete() -> CreateChannel {
    CreateChannel {
        title: tr!("Delete channel").into(),
        detail: tr!(
            "Are you sure you want to delete {}? This cannot be undone.",
            "#general"
        ),
        confirm: tr!("Delete channel").into(),
        danger: true,
        kinds: Vec::new(),
        named: false,
        ..CreateChannel::default()
    }
}

/// It was sent and waits for Discord.
pub fn saving() -> CreateChannel {
    CreateChannel {
        saving: true,
        ..voice()
    }
}

/// Discord refused.
pub fn refused() -> CreateChannel {
    CreateChannel {
        error: "Missing Permissions".into(),
        ..voice()
    }
}

/// The server's window with `dialog` open over it.
pub fn asked(dialog: CreateChannel) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::CreateChannel);
    vm.settings.create_channel = dialog;
    vm
}
