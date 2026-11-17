//! Samples of the composer's frame for tests and shots, after the fake
//! Discord's data: task C14's. Each `Vm` is the whole window with the
//! composer filled as the golden screenshot shows it; the timeline above it
//! is the timeline's own sample.

use iced::Rectangle;
use yapfast_lang::tr;

use crate::Vm;
use crate::chat::composer::{Composer, Expression, Hint, More, Reply, Staged};
use crate::menu::{Menu, Spot};

use super::super::{pictures, shell};

/// The golden window's plus button: 32 px, 10 px into the composer.
pub const PLUS: Rectangle = Rectangle {
    x: 394.0,
    y: 806.0,
    width: 32.0,
    height: 32.0,
};

/// A DM's composer: the plus takes files alone, and the GIFs, stickers and
/// emoji beside the field.
pub fn dm() -> Composer {
    Composer {
        room: 1,
        enabled: true,
        placeholder: tr!("Message @{}", "Person 5"),
        more: More {
            attach: true,
            ..More::default()
        },
        gifs: true,
        stickers: true,
        ..Composer::default()
    }
}

/// A server channel's: the plus opens Discord's menu.
pub fn channel() -> Composer {
    Composer {
        room: 2,
        placeholder: tr!("Message #{}", "general"),
        more: More {
            attach: true,
            threads: true,
            polls: true,
        },
        ..dm()
    }
}

/// With a reply to the status bot being written, the pings on.
pub fn replying() -> Composer {
    Composer {
        placeholder: tr!("Write a reply").into(),
        reply: Some(Reply {
            label: tr!("Replying to {}: {}", "Status Bot", "Deploy v2.4 finished."),
            ping: true,
        }),
        ..channel()
    }
}

/// Replying, with `icon.png` waiting and a mention being written.
pub fn staged_and_mentioning() -> Composer {
    Composer {
        draft: "hello @Bo".into(),
        focused: true,
        staged: vec![staged("icon.png", &tr!("{} KB", 7), true, false)],
        can_spoil: true,
        suggesting: true,
        ..replying()
    }
}

/// A file waiting, with its buttons named after it as the app names them.
pub fn staged(name: &str, size: &str, picture: bool, spoiler: bool) -> Staged {
    Staged {
        name: name.into(),
        size: size.into(),
        picture: picture.then(|| pictures::sunset(108, 86)),
        spoiler,
        remove_label: tr!("Remove {}", name),
        spoil_label: if spoiler {
            tr!("Stop covering {}", name)
        } else {
            tr!("Mark {} as a spoiler", name)
        },
    }
}

/// A message request's bar in place of the field.
pub fn request() -> Composer {
    Composer {
        request: Some(tr!("{} wants to message you", "Bob")),
        ..dm()
    }
}

/// The one disabled bar where nothing may be sent.
pub fn locked() -> Composer {
    Composer {
        enabled: false,
        locked: Some(tr!("You do not have permission to send messages in this channel.").into()),
        ..channel()
    }
}

/// Someone typing, a hint with its lock, and the sticker picker open.
pub fn typing() -> Composer {
    Composer {
        typing: tr!("{} is typing...", "Bob"),
        hint: Hint {
            text: tr!("End-to-end encrypted. Enter to send, Shift+Enter for a new line.").into(),
            lock: true,
        },
        open: Some(Expression::Stickers),
        ..channel()
    }
}

/// `dm/person_chat.png`: the DM's composer, empty.
pub fn person_chat() -> Vm {
    let mut vm = shell::dm_home();
    vm.chat.composer = Composer {
        focused: true,
        ..dm()
    };
    vm
}

/// A server channel with `composer` in it.
pub fn in_channel(composer: Composer) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.composer = composer;
    vm
}

/// `composer/reply.png`.
pub fn reply() -> Vm {
    in_channel(Composer {
        focused: true,
        ..replying()
    })
}

/// `composer/reply_attachment_mention.png`: the suggestions over it are
/// C15's, filled on top of this.
pub fn reply_attachment_mention() -> Vm {
    in_channel(staged_and_mentioning())
}

/// `menu/composer_plus.png`: the plus's menu over the reply with its file.
pub fn plus_menu() -> Vm {
    let mut vm = in_channel(Composer {
        draft: String::new(),
        suggesting: false,
        focused: false,
        ..staged_and_mentioning()
    });
    let rows = vm.chat.composer.more.rows();
    vm.menu = Menu::new(
        1,
        Spot {
            gap: 8.0,
            ..Spot::above(PLUS)
        },
        rows,
    );
    vm
}
