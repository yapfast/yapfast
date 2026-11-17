//! Who is in the fake's Lounge: Bob streaming, Carol muted, Dave muted and
//! deafened (`testing/fake_discord.ts:2134`), and Alice once she joins.

use crate::fixtures::avatar;
use crate::fixtures::pictures::{picture, pictured};
use crate::sidebar::channels::VoicePerson;

fn person(user: &str, name: &str, tint: u32) -> VoicePerson {
    VoicePerson {
        id: user.into(),
        user: user.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        movable: true,
        ..VoicePerson::default()
    }
}

/// This account, in the call.
pub fn alice() -> VoicePerson {
    VoicePerson {
        local: true,
        ..person("1400000000000000001", "Alice", 0x3a9357)
    }
}

pub fn bob() -> VoicePerson {
    VoicePerson {
        live: true,
        ..person("1400000000000000002", "Bob", 0xeb459e)
    }
}

/// Carol set another avatar for the server (`testing/fake_discord.ts:959`).
pub fn carol() -> VoicePerson {
    let mut carol = VoicePerson {
        muted: true,
        ..person("1400000000000000003", "Carol", 0x1abc9c)
    };
    carol.avatar.picture = Some(picture(
        "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
    ));
    carol
}

/// Erin, joining: not connected yet, her camera on.
pub fn erin() -> VoicePerson {
    VoicePerson {
        video: true,
        ..person("1400000000000000005", "Erin", 0xf47b67)
    }
}

/// Dave has no avatar of his own: Discord's default for his id.
pub fn dave() -> VoicePerson {
    let mut dave = VoicePerson {
        muted: true,
        deafened: true,
        ..person("1400000000000000004", "Dave", 0x5865f2)
    };
    dave.avatar.picture = Some(picture("/embed/avatars/2.png"));
    dave
}
