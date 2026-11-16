//! What many rows share: an avatar from a name and a picture address, a
//! presence, the line under a DM. Ported from `src/ui.rs` and
//! `src/ui/profile.rs` of the Slint app.

use iced::Color;
use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatRoom, Presence as CorePresence};
use yapfast_core::network::Network;
use yapfast_lang::trn;

use anomi_vm::kind::{Avatar, Presence};

/// Avatar tints: mid tones that keep a white initial legible.
const TINTS: [(u8, u8, u8); 8] = [
    (0xc2, 0x55, 0x4d),
    (0xb8, 0x6e, 0x22),
    (0x3a, 0x93, 0x57),
    (0x2b, 0x8a, 0x98),
    (0x4a, 0x74, 0xc9),
    (0x7a, 0x5f, 0xc9),
    (0xb3, 0x4f, 0x8c),
    (0x64, 0x6b, 0x7a),
];

/// The same name always gets the same tint (FNV-1a of its lower case).
pub fn tint(key: &str) -> Color {
    let hash = key
        .to_lowercase()
        .bytes()
        .fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
    let (red, green, blue) = TINTS[(hash >> 20) as usize % TINTS.len()];
    Color::from_rgb8(red, green, blue)
}

/// A name's first letter, in capitals.
pub fn initial(text: &str) -> String {
    text.chars().next().unwrap_or('?').to_uppercase().collect()
}

/// A server's initials: the first letter of up to three words.
pub fn initials(name: &str) -> String {
    let mut words = name
        .split_whitespace()
        .filter_map(|word| word.chars().next());
    let mut text: String = words
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_else(|| "?".into());
    text.extend(words.take(2).flat_map(char::to_uppercase));
    text
}

/// An avatar: the picture at `address` once it arrived, else the initial on
/// the name's tint.
pub fn avatar(state: &AppState, name: &str, tint_key: &str, address: Option<&str>) -> Avatar {
    Avatar {
        initial: initial(name),
        tint: tint(tint_key),
        picture: crate::pictures::find(state, address),
    }
}

pub fn presence(presence: Option<CorePresence>) -> Presence {
    match presence {
        None => Presence::None,
        Some(CorePresence::Online) => Presence::Online,
        Some(CorePresence::Mobile) => Presence::Mobile,
        Some(CorePresence::Idle) => Presence::Idle,
        Some(CorePresence::DoNotDisturb) => Presence::DoNotDisturb,
        Some(CorePresence::Offline) => Presence::Offline,
    }
}

/// The line under a Discord DM: the person's status or what they're doing,
/// or a group's size. `dm_detail` in `src/ui.rs`.
pub fn dm_detail(room: &ChatRoom) -> String {
    if !room.direct || room.network != Network::Discord {
        return String::new();
    }
    if room.person.is_none() {
        return trn!("{n} member", "{n} members", room.members);
    }
    let words = room.activity.as_deref().map_or("", |activity| {
        yapfast_core::discord::status_parts(activity).1
    });
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_three_words_at_most() {
        assert_eq!(initials("Co-op crew"), "CC");
        assert_eq!(initials("a b c d"), "ABC");
        assert_eq!(initials(""), "?");
        assert_eq!(initial("élan"), "É");
    }

    #[test]
    fn a_name_keeps_its_tint() {
        assert_eq!(tint("Bob"), tint("bob"));
    }
}
