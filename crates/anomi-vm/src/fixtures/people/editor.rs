//! Samples of the profile editor for tests and shots, after the fake
//! Discord's data: task P14's. Alice's own profile as `profile/editor.png`
//! shows it: the pink ring, a banner colour, the theme open to set.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::account;
use crate::fixtures::people::profile;
use crate::fixtures::shell;
use crate::kind::{Expected, Presence};
use crate::people::editor::{Choice, Editor, May};
use crate::shell::Layer;

/// The tan the golden shot's banner has.
pub const TAN: Color = Color::from_rgb(
    0xb8 as f32 / 255.0,
    0xa5 as f32 / 255.0,
    0x87 as f32 / 255.0,
);

/// Alice's main profile, loaded, nothing changed.
pub fn main() -> Editor {
    let alice = account::account();
    let mut card = profile::own();
    card.presence = Presence::Online;
    card.accent = Some(TAN);
    card.banner = Expected::None;
    card.thought = Some(alice.status);
    card.decoration = alice.decoration;
    card.created = "Jul 30, 2025".into();
    card.bio = None;
    Editor {
        card,
        title: tr!("Edit profile").into(),
        detail: tr!("Your profile").into(),
        main: true,
        name_placeholder: "alice".into(),
        bio_placeholder: tr!("Tell everyone a bit about yourself").into(),
        may: May {
            name: true,
            avatar: true,
            banner: false,
            bio: true,
            look: true,
            colour: true,
        },
        bio_left: 190,
        colour: "#b8a587".into(),
        side_open: true,
        ..Editor::default()
    }
}

/// With Nitro: the theme, the banner and the name style open.
pub fn nitro() -> Editor {
    let mut editor = main();
    editor.may.banner = true;
    editor.may.look = true;
    editor.banner_reset = true;
    editor
}

/// Something changed: the bar asks to save it.
pub fn changed() -> Editor {
    let mut editor = main();
    editor.name = "Alice W".into();
    editor.card.name = "Alice W".into();
    editor.changed = true;
    editor
}

/// A server's profile: a nickname, Nitro's offer over the rest.
pub fn server() -> Editor {
    let mut editor = main();
    editor.main = false;
    editor.title = tr!("Edit server profile").into();
    editor.detail = tr!("Your profile in {}", "Server");
    editor.offer = true;
    editor.banner_shown = true;
    editor.may = May {
        name: true,
        ..May::default()
    };
    editor.bio_placeholder = tr!("Tell this server a bit about yourself").into();
    editor.card.joined = None;
    editor
}

/// The servers whose tags this account may wear.
pub fn tags() -> Vec<Choice> {
    vec![Choice {
        id: "g1".into(),
        name: "Server".into(),
        icon: None,
        tag: "SRV".into(),
    }]
}

/// The window with the editor open on `editor`.
pub fn open(editor: Editor) -> Vm {
    let mut vm = shell::server_home();
    vm.people.editor = editor;
    shell::open(vm, Layer::ProfileEditor)
}

/// `profile/editor.png`.
pub fn shot() -> Vm {
    open(main())
}
