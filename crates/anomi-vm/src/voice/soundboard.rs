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
}

/// A heading, or a row of up to [`PER_ROW`] sounds (`SoundRow`).
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Heading(String),
    Sounds(Vec<Sound>),
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
    pub fn section(&mut self, heading: &str, sounds: &[Sound]) {
        self.rows.push(Row::Heading(heading.to_uppercase()));
        self.rows.extend(
            sounds
                .chunks(PER_ROW)
                .map(|chunk| Row::Sounds(chunk.to_vec())),
        );
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
