//! What a change can reach on screen, so that a backend event shows again
//! only the areas it can have changed: who's typing, several times a second
//! in a busy channel, reaches the composer and nothing else, and who speaks
//! in a call the call's own views. Ported from `Reach` in
//! `src/ui/refresh.rs` of the Slint app.
//!
//! [`Reach::of`] says what an event can reach; an event whose effects ask
//! anything but a sound reaches everything (`Adapter::apply`, `src/ui.rs`),
//! and so does an action. Each area's line in [`super::refresh`] names the
//! reaches that can change what it shows.

use std::ops::BitOr;

use yapfast_core::chat::Event;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reach(u8);

impl Reach {
    pub const NOTHING: Self = Self(0);
    /// Anything else in the core's state: every area compares its key.
    pub const STATE: Self = Self(1);
    /// Who's typing, which shows only above the composers.
    pub const TYPING: Self = Self(1 << 1);
    /// The call, with who's speaking: the call's own views, and the places
    /// that list who's in a voice channel.
    pub const VOICE: Self = Self(1 << 2);
    /// Pictures arrived, which can show anywhere: shown together with those
    /// that follow them closely (`App::show_events`).
    pub const PICTURES: Self = Self(1 << 3);
    /// A custom emoji's picture: the emoji picker shows it while open,
    /// anything else that shows custom emoji as a picture.
    pub const EMOJI: Self = Self(1 << 4);
    /// A GIF's or a sticker's picture, which its picker alone shows.
    pub const PICKER: Self = Self(1 << 5);
    /// The video or the voice message playing changed.
    pub const VIDEO: Self = Self(1 << 6);
    pub const EVERYTHING: Self = Self(0x7f);
    /// What arrives as pictures and may wait a moment for more.
    pub const ANY_PICTURE: Self = Self(Self::PICTURES.0 | Self::EMOJI.0 | Self::PICKER.0);

    /// What a backend event can change on screen.
    pub fn of(event: &Event) -> Self {
        match event {
            Event::Typing { .. } => Self::TYPING,
            Event::Voice(_) | Event::StreamPreview { .. } => Self::VOICE,
            Event::Picture { url, .. } if url.contains("/emojis/") => Self::EMOJI,
            Event::Picture { .. } => Self::PICTURES,
            Event::GifPicture { .. } | Event::StickerPicture { .. } => Self::PICKER,
            Event::Video(_) | Event::VoiceMessage(_) => Self::VIDEO,
            _ => Self::EVERYTHING,
        }
    }

    /// Whether this reaches any of `other`.
    pub fn touches(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// This without `other`.
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for Reach {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Reach;
    use yapfast_core::chat::Event;

    #[test]
    fn typing_reaches_the_composer_alone() {
        let typing = Reach::of(&Event::Typing {
            room: "discord:1".into(),
            names: Vec::new(),
        });
        assert_eq!(typing, Reach::TYPING);
        assert!(!typing.touches(Reach::STATE | Reach::PICTURES));
        assert!(Reach::EVERYTHING.touches(typing));
        assert_eq!(
            (typing | Reach::EMOJI).without(Reach::ANY_PICTURE),
            Reach::TYPING
        );
    }
}
