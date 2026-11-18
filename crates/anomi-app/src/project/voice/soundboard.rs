//! The call's soundboard (`show_soundboard`, `src/ui/refresh.rs:4803`):
//! its sounds in their groups, three a row, each with its emoji, and why
//! none can be played now. Made as it opens, and again while it shows when
//! the sounds or their pictures change.

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::voice::SoundGroup;
use yapfast_lang::tr;

use anomi_vm::voice::soundboard::{Emoji, Sound, Soundboard};

use crate::pictures::Requests;
use crate::project::rows;

/// What the soundboard was made from: its sounds, the pictures and why it
/// couldn't play.
pub type Key = (usize, u64, &'static str);

/// Why no sound can be played now, as Discord says it.
fn note(state: &AppState) -> &'static str {
    let voice = &state.voice;
    let server_muted =
        (voice.participants.iter()).any(|person| person.local && person.server_muted);
    if voice.deafened {
        tr!("Soundboard disabled when deafened")
    } else if server_muted {
        tr!("Soundboard disabled when server muted")
    } else {
        ""
    }
}

/// What [`soundboard`] reads.
pub fn key(state: &AppState) -> Key {
    let groups = state
        .voice
        .soundboard
        .as_ref()
        .map_or(0, |groups| Arc::as_ptr(groups).cast::<u8>().addr());
    (groups, state.picture_changes, note(state))
}

/// The soundboard of the call, its custom emoji wanted.
pub fn soundboard(state: &AppState, board: &mut Soundboard, requests: &mut Requests) {
    let mut made = Soundboard {
        note: note(state).into(),
        ..Soundboard::default()
    };
    let groups: &[SoundGroup] = state.voice.soundboard.as_deref().unwrap_or_default();
    for group in groups {
        let sounds = group.sounds.iter().map(|sound| {
            let emoji = match sound.picture.as_deref() {
                Some(picture) => {
                    requests.want(state, [picture]);
                    Emoji::Custom(rows::expected(state, Some(picture)))
                }
                None if !sound.emoji.is_empty() => Emoji::Glyph(sound.emoji.clone()),
                None => Emoji::None,
            };
            Sound {
                id: sound.id.clone(),
                name: sound.name.clone(),
                emoji,
                favorite: sound.favorite,
                hear: tr!("Preview {}", sound.name),
            }
        });
        made.section(&group.name, sounds);
    }
    if *board != made {
        *board = made;
    }
}
