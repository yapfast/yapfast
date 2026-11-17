//! The soundboard: what it shows and what it asks for. Task V12
//! (docs/iced/tasks/voice.md). The Slint side: `Soundboard`, `SoundCell` and
//! `SoundRow` (`ui/voice.slint:1699`, `:1646`), fed by `sound_rows`
//! (`src/ui/refresh.rs:6188`) and `show_soundboard` (`:5013`).
//!
//! It is Discord's soundboard in a call: the favourites, the call's
//! server's sounds, others' with Nitro, then Discord's own, three to a row
//! under a heading each. Settings reuses the same card to choose an
//! entrance sound, with its own `title` and no `extras`.

use crate::kind::Expected;

/// How many sounds a row holds.
pub const PER_ROW: usize = 3;

/// A sound's emoji: a Unicode one, a custom one's picture, or none.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Emoji {
    #[default]
    None,
    /// A Unicode emoji, drawn as Twemoji.
    Glyph(String),
    /// A custom emoji: its room is kept while its picture is on its way.
    Custom(Expected),
}

impl Emoji {
    /// It takes room before the name.
    pub fn shown(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A sound to play (`SoundCell`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub emoji: Emoji,
    /// Among the account's favourites: its star stays shown.
    pub favorite: bool,
    /// The hear button's tooltip, "Preview {name}", made where the sound is
    /// projected: a tooltip borrows its text.
    pub hear: String,
}

/// A heading, or a row of up to [`PER_ROW`] sounds (`SoundRow`).
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Heading(String),
    Sounds(Vec<Sound>),
}

/// A row's key: its section's heading, and its first sound if it has sounds.
fn key(heading: &str, first: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    heading.hash(&mut hasher);
    first.hash(&mut hasher);
    hasher.finish()
}

/// The soundboard: what its screen draws. The window holds it as
/// `vm.voice.soundboard`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Soundboard {
    /// Its title when it isn't "Soundboard" (empty): the entrance sounds' own
    /// in Settings.
    pub title: String,
    /// Why no sound can be played now, such as being deafened; empty when
    /// they can. The sounds then show dimmed and a click does nothing, but a
    /// favourite can still change.
    pub note: String,
    pub rows: Vec<Row>,
    /// Each row's identity, one per row, stable as rows come and go above it:
    /// a heading's text, or its section's heading with its first sound's id.
    /// [`Self::section`] fills it.
    pub keys: Vec<u64>,
    /// Settings' picker of an entrance sound: no hearing a sound first and
    /// no star, where a call's soundboard offers both on hover.
    pub chooser: bool,
    /// It opens under what opened it, from its right end, rather than above
    /// the call's button: where the shell grows it from.
    pub drops: bool,
}

impl Soundboard {
    /// A heading (written in capitals, as the Slint rows are) and its sounds
    /// three to a row.
    pub fn section(&mut self, heading: &str, sounds: impl IntoIterator<Item = Sound>) {
        let heading = heading.to_uppercase();
        self.keys.push(key(&heading, None));
        let mut sounds = sounds.into_iter().peekable();
        self.rows.push(Row::Heading(heading.clone()));
        while sounds.peek().is_some() {
            let line: Vec<Sound> = sounds.by_ref().take(PER_ROW).collect();
            self.keys
                .push(key(&heading, line.first().map(|s| s.id.as_str())));
            self.rows.push(Row::Sounds(line));
        }
    }

    /// Sounds can be played.
    pub fn playable(&self) -> bool {
        self.note.is_empty()
    }

    /// Its rows are still on their way.
    pub fn loading(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The soundboard: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Plays a sound for everyone in the call (or, in Settings, chooses it).
    Play(String),
    /// Plays it for the account alone, to hear it first.
    Preview(String),
    /// Stars a sound, or takes its star away.
    Favorite(String, bool),
}
