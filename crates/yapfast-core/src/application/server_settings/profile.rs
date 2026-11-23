//! Server Profile: the server's name, icon and description, as Discord's
//! client shows them on that page and saves them with `PATCH
//! /guilds/{id}/profile`.

use super::*;
use crate::{
    application::profile_editor::PictureEdit,
    chat::{ServerProfile, ServerSettingsChange, ServerTrait},
};

/// The longest name and description Discord takes, in characters, and the
/// shortest name.
pub const NAME_LIMIT: usize = 100;
pub const NAME_LEAST: usize = 2;
pub const DESCRIPTION_LIMIT: usize = 300;
/// How many traits a profile has room for, and the longest one Discord's
/// client lets be written.
pub const TRAITS: usize = 5;
pub const TRAIT_LIMIT: usize = 24;

/// A text of a settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsText {
    Name,
    Description,
    /// The Banner's colour, as a hex string such as "#ff73fa".
    BrandColor,
    /// A trait's words, by its place.
    Trait(usize),
}

/// Server Profile, with its draft.
#[derive(Clone, Debug)]
pub struct ProfilePage {
    /// What Discord has.
    pub original: ServerProfile,
    pub name: String,
    pub description: String,
    pub icon: PictureEdit,
    pub brand_color: Option<String>,
    /// Each of the five traits' words, empty for none; a trait keeps the
    /// emoji it had.
    pub traits: [String; TRAITS],
}

impl ProfilePage {
    pub(super) fn new(original: ServerProfile) -> Self {
        let traits = std::array::from_fn(|index| {
            original
                .traits
                .get(index)
                .map(|found| found.label.clone())
                .unwrap_or_default()
        });
        Self {
            name: original.name.clone(),
            description: original.description.clone(),
            icon: PictureEdit::Keep,
            brand_color: original.brand_color.clone(),
            traits,
            original,
        }
    }

    pub(super) fn write(&mut self, field: SettingsText, text: String) {
        match field {
            SettingsText::Name => self.name = text,
            SettingsText::Description => self.description = text,
            SettingsText::BrandColor => self.brand_color = Some(text),
            SettingsText::Trait(index) => {
                if let Some(written) = self.traits.get_mut(index) {
                    *written = text.chars().take(TRAIT_LIMIT).collect();
                }
            }
        }
    }

    pub(super) fn undo(&mut self) {
        *self = Self::new(std::mem::take(&mut self.original));
    }

    /// Takes a name or icon changed elsewhere, unless the draft changed it.
    #[cfg(feature = "discord")]
    pub(super) fn follow(&mut self, name: String, icon: Option<String>) {
        if self.name == self.original.name {
            self.name.clone_from(&name);
        }
        self.original.name = name;
        self.original.icon = icon;
    }

    /// The icon as it would be saved: its address, a new one's preview, or
    /// neither.
    pub fn icon(&self) -> (Option<&str>, Option<&Arc<Picture>>) {
        match &self.icon {
            PictureEdit::Keep => (self.original.icon.as_deref(), None),
            PictureEdit::Reset => (None, None),
            PictureEdit::New(picked) => (None, Some(&picked.preview)),
        }
    }

    /// Why the draft can't be saved as it is: a name too short or long, or
    /// a description too long, as Discord's client says it.
    pub fn problem(&self) -> Option<String> {
        let name = self.name.trim().chars().count();
        if !(NAME_LEAST..=NAME_LIMIT).contains(&name) {
            return Some(format!(
                "The server name must be between {NAME_LEAST} and {NAME_LIMIT} characters."
            ));
        }
        let description = self.description.trim().chars().count();
        (description > DESCRIPTION_LIMIT)
            .then(|| format!("The description can be up to {DESCRIPTION_LIMIT} characters."))
    }

    /// What saving would change; none when nothing does.
    pub fn change(&self) -> Option<ServerSettingsChange> {
        let text = |written: &str, kept: &str| {
            let written = written.trim();
            (written != kept).then(|| written.to_owned())
        };
        let name = text(&self.name, &self.original.name);
        let description = text(&self.description, &self.original.description);
        let icon = match &self.icon {
            PictureEdit::Keep => None,
            PictureEdit::Reset => self.original.icon.is_some().then_some(None),
            PictureEdit::New(picked) => Some(Some(picked.clone())),
        };
        let brand_color = self
            .brand_color
            .clone()
            .filter(|color| Some(color) != self.original.brand_color.as_ref());
        // The traits written, in order, each with the emoji it had there.
        let traits: Vec<ServerTrait> = self
            .traits
            .iter()
            .enumerate()
            .filter(|(_, label)| !label.trim().is_empty())
            .map(|(index, label)| ServerTrait {
                emoji: self
                    .original
                    .traits
                    .get(index)
                    .and_then(|found| found.emoji.clone()),
                label: label.trim().to_owned(),
            })
            .collect();
        let traits = (traits != self.original.traits).then_some(traits);
        (name.is_some()
            || description.is_some()
            || icon.is_some()
            || brand_color.is_some()
            || traits.is_some())
        .then_some(ServerSettingsChange::Profile {
            name,
            description,
            icon,
            brand_color,
            traits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> ProfilePage {
        ProfilePage::new(ServerProfile {
            name: "Crew".into(),
            description: String::new(),
            icon: Some("https://cdn/icon.png".into()),
            brand_color: None,
            traits: vec![ServerTrait {
                emoji: Some("\u{1F3AE}".into()),
                label: "Games".into(),
            }],
        })
    }

    #[test]
    fn the_banner_colour_and_traits_change_only_when_written() {
        let mut page = page();
        page.write(SettingsText::Trait(0), "Games".into());
        assert!(page.change().is_none());
        page.write(SettingsText::BrandColor, "#ff73fa".into());
        page.write(SettingsText::Trait(1), "x".repeat(40));
        let Some(ServerSettingsChange::Profile {
            brand_color,
            traits,
            name,
            ..
        }) = page.change()
        else {
            panic!("a change");
        };
        assert_eq!((brand_color.as_deref(), name), (Some("#ff73fa"), None));
        let traits = traits.unwrap();
        assert_eq!(
            traits[0].emoji.as_deref(),
            Some("\u{1F3AE}"),
            "keeps its emoji"
        );
        assert_eq!(traits[1].label.chars().count(), TRAIT_LIMIT);
    }

    #[test]
    fn a_page_changes_only_what_the_draft_does() {
        let mut page = page();
        assert!(page.change().is_none());
        page.write(SettingsText::Name, " Crew ".into());
        assert!(page.change().is_none(), "spaces at the ends don't count");
        page.write(SettingsText::Description, "Hi".into());
        page.icon = PictureEdit::Reset;
        let Some(ServerSettingsChange::Profile {
            name,
            description,
            icon,
            ..
        }) = page.change()
        else {
            panic!("a change");
        };
        assert_eq!((name, description.as_deref()), (None, Some("Hi")));
        assert!(matches!(icon, Some(None)));
        page.undo();
        assert!(page.change().is_none());
        assert_eq!(page.description, "");
    }

    #[test]
    fn a_name_discord_refuses_holds_the_save_back() {
        let mut page = page();
        page.write(SettingsText::Name, "C".into());
        assert!(page.problem().is_some());
        page.write(SettingsText::Name, "C".repeat(NAME_LIMIT + 1));
        assert!(page.problem().is_some());
        page.write(SettingsText::Name, "Co".into());
        assert!(page.problem().is_none());
    }

    #[test]
    #[cfg(feature = "discord")]
    fn a_change_elsewhere_follows_unless_the_draft_changed_it() {
        let mut page = page();
        page.follow("Crew 2".into(), None);
        assert_eq!(page.name, "Crew 2");
        assert!(page.change().is_none());
        page.write(SettingsText::Name, "Mine".into());
        page.follow("Crew 3".into(), None);
        assert_eq!(page.name, "Mine");
        assert_eq!(page.original.name, "Crew 3");
    }
}
