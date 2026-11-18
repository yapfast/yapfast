//! What a change can reach on screen, so that a backend event shows again
//! only the areas it can have changed: who's typing, several times a second
//! in a busy channel, reaches the composer and nothing else, and who speaks
//! in a call the call's own views. Ported from `Reach` in
//! `src/ui/refresh.rs` of the Slint app.
//!
//! [`Reach::of`] says what an event can reach; an event whose effects ask
//! anything but a sound reaches everything (`Adapter::apply`, `src/ui.rs`).
//! [`Reach::after`] says what an action can reach, as `Adapter::action`
//! (`src/ui.rs:3228`) narrowed it: a keystroke in a field the core keeps
//! reaches that field's area alone. Each area's line in [`super::refresh`]
//! names the reaches that can change what it shows.

use std::ops::BitOr;

use yapfast_core::application::{Action, ChannelInput, PollEdit, RoleInput};
use yapfast_core::chat::Event;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reach(u16);

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
    /// A keystroke in a composer or in a message being edited: its draft and
    /// the suggestions it opens (I11, and I10's inline edit).
    pub const COMPOSER: Self = Self(1 << 7);
    /// A bot's form was filled in or a select's choice changed, or the poll
    /// being made was written in (I08).
    pub const FORM: Self = Self(1 << 8);
    /// A keystroke in the profile editor (I17).
    pub const EDITOR: Self = Self(1 << 9);
    /// A keystroke in a server's or a channel's settings (I20), or in the
    /// two-factor prompt (I05).
    pub const SETTINGS: Self = Self(1 << 10);
    /// A keystroke in the quick switcher or in the new DM's field: that
    /// dialog's places or people (I06).
    pub const PLACES: Self = Self(1 << 11);
    /// Whatever the core's state can change. The clocks' bits are not in
    /// it: rows are built with the time they're built at, and only a
    /// clock's tick, or the window coming to the front, says it again.
    pub const EVERYTHING: Self = Self(0xfff);
    /// A minute turned for a clock that shows it: a busy voice channel's
    /// call, the game's time played (`crate::clocks`).
    pub const MINUTE: Self = Self(1 << 12);
    /// The timeline's words that change by themselves: midnight turned
    /// "Today" to "Yesterday", or an app's answer gave up.
    pub const DAY: Self = Self(1 << 13);
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

    /// What an action can change on screen once the core took it, as
    /// `Adapter::action` (`src/ui.rs:3228`) narrowed it. Anything not named
    /// here reaches everything: a wider reach only compares more keys.
    pub fn after(action: &Action) -> Self {
        match action {
            // Pictures asked for change nothing until they come, and the
            // messages in view only what the backend sends back.
            Action::LoadPicturesFirst(_)
            | Action::KeepEmojiPictures(_)
            | Action::CacheEmojiPictures
            | Action::VisibleMessages { .. }
            | Action::WantedPictures(_) => Self::NOTHING,
            // Discord hears whether someone uses the window; the Activity
            // panel's feed it asks again for shows as it comes.
            Action::Inactive { .. } => Self::NOTHING,
            // A picker's search, page and the pictures it keeps: the picker
            // alone (I12).
            Action::OpenGifs
            | Action::CloseGifs
            | Action::SearchGifs(_)
            | Action::ShowGifs(_)
            | Action::SearchGifCategory(_)
            | Action::GifsShown { .. }
            | Action::OpenStickers
            | Action::CloseStickers
            | Action::SearchStickers(_)
            | Action::StickersShown(_) => Self::PICKER,
            Action::SetDraft(_) | Action::SetEditText(_) | Action::MentionQuery(_) => {
                Self::COMPOSER
            }
            // A keystroke in the poll being made reaches its dialog alone,
            // as one in a bot's form does (I08).
            Action::SetModalField { .. }
            | Action::SearchSelect(_)
            | Action::ToggleSelect(_)
            | Action::RemoveModalFile { .. }
            | Action::EditPoll(PollEdit::Question(_) | PollEdit::Answer { .. }) => Self::FORM,
            Action::WriteProfile { .. } => Self::EDITOR,
            Action::SearchSwitcher(_) | Action::NewDmQuery(_) => Self::PLACES,
            Action::WriteServerSetting { .. }
            | Action::WriteMfa(_)
            | Action::Roles(
                RoleInput::Name(_)
                | RoleInput::Search(_)
                | RoleInput::PermissionSearch(_)
                | RoleInput::MemberSearch(_)
                | RoleInput::FindCandidates(_),
            )
            | Action::ChannelSettings(
                ChannelInput::Name(_) | ChannelInput::Topic(_) | ChannelInput::FindMembers(_),
            ) => Self::SETTINGS,
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
    use yapfast_core::application::{Action, Navigation};
    use yapfast_core::chat::Event;

    #[test]
    fn a_keystroke_reaches_its_field_alone_and_the_rest_everything() {
        let draft = Reach::after(&Action::SetDraft("hi".into()));
        assert_eq!(draft, Reach::COMPOSER);
        assert!(!draft.touches(Reach::STATE));
        assert_eq!(
            Reach::after(&Action::WriteMfa("123".into())),
            Reach::SETTINGS
        );
        assert_eq!(Reach::after(&Action::CacheEmojiPictures), Reach::NOTHING);
        let home = Action::Navigate(Navigation::DirectMessages);
        assert_eq!(Reach::after(&home), Reach::EVERYTHING);
        assert!(Reach::EVERYTHING.touches(Reach::SETTINGS | Reach::COMPOSER));
    }

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
