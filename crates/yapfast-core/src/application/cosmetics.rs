//! Which of a profile's cosmetics show, as Settings → Appearance → Profiles
//! says. What's off is never asked for: the profile's pictures go through
//! these before anything is fetched or shown, and a profile is fetched
//! without the collectibles left out.

use std::borrow::Cow;

use crate::chat::{EffectLayer, NameStyle, Profile, ProfileFrame, ServerTag};

/// The profile switches. Off is the cheaper side of each; see "Profile
/// switches" in `docs/discord/people.md` for why each starts as it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cosmetics {
    /// Profile effects play over cards: a request for what the effect is
    /// made of, then moving pictures decoded frame by frame.
    pub effects: bool,
    /// The avatar's decoration and the card's profile frame show.
    pub decorations: bool,
    /// Avatars, banners and decorations move, and styled names animate.
    /// Off, the still picture is fetched in place of the moving one.
    pub moving: bool,
    /// Display names show in the font and colours picked.
    pub name_styles: bool,
    /// The server tag shows beside the name, with its badge.
    pub server_tags: bool,
}

impl Default for Cosmetics {
    fn default() -> Self {
        Self {
            effects: false,
            decorations: true,
            moving: false,
            name_styles: true,
            server_tags: true,
        }
    }
}

impl Cosmetics {
    /// The collectibles a profile fetched now asks Discord about.
    pub fn collectibles(self) -> yapfast_discord::profile::Collectibles {
        yapfast_discord::profile::Collectibles {
            effect: self.effects,
            frame: self.decorations,
        }
    }

    /// The address to fetch a profile's avatar or banner at: a moving one's
    /// still, a PNG of its first frame, unless they may move.
    pub fn picture(self, url: &str) -> Cow<'_, str> {
        let moving =
            (url.contains("/avatars/") || url.contains("/banners/")) && url.contains(".gif?");
        if moving && !self.moving {
            Cow::Owned(url.replacen(".gif?", ".png?", 1))
        } else {
            Cow::Borrowed(url)
        }
    }

    pub fn avatar(self, profile: &Profile) -> Option<Cow<'_, str>> {
        profile.avatar.as_deref().map(|url| self.picture(url))
    }

    pub fn banner(self, profile: &Profile) -> Option<Cow<'_, str>> {
        profile.banner.as_deref().map(|url| self.picture(url))
    }

    /// The avatar's decoration, if it shows; held to one frame unless it may
    /// move, as Discord's CDN gives it without `passthrough`.
    pub fn decoration(self, profile: &Profile) -> Option<Cow<'_, str>> {
        let url = profile.decoration.as_deref().filter(|_| self.decorations)?;
        Some(if self.moving {
            Cow::Borrowed(url)
        } else {
            Cow::Owned(url.replacen("passthrough=true", "passthrough=false", 1))
        })
    }

    pub fn effect(self, profile: &Profile) -> &[EffectLayer] {
        if self.effects { &profile.effect } else { &[] }
    }

    pub fn frame(self, profile: &Profile) -> Option<&ProfileFrame> {
        profile.frame.as_ref().filter(|_| self.decorations)
    }

    pub fn tag(self, profile: &Profile) -> Option<&ServerTag> {
        profile.tag.as_ref().filter(|_| self.server_tags)
    }

    pub fn name_style(self, profile: &Profile) -> Option<&NameStyle> {
        profile.name_style.as_ref().filter(|_| self.name_styles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dressed() -> Profile {
        Profile {
            avatar: Some("https://cdn/avatars/7/a_1.gif?size=128".into()),
            banner: Some("https://cdn/banners/7/a_2.gif?size=600".into()),
            decoration: Some(
                "https://cdn/avatar-decoration-presets/a_3.png?size=240&passthrough=true".into(),
            ),
            effect: vec![EffectLayer {
                src: "https://cdn/effect.png".into(),
                ..Default::default()
            }],
            frame: Some(ProfileFrame::default()),
            tag: Some(ServerTag::default()),
            name_style: Some(NameStyle::default()),
            ..Default::default()
        }
    }

    #[test]
    fn off_leaves_out_each_cosmetic_and_holds_pictures_still() {
        let profile = dressed();
        let off = Cosmetics {
            effects: false,
            decorations: false,
            moving: false,
            name_styles: false,
            server_tags: false,
        };
        assert_eq!(
            off.avatar(&profile).as_deref(),
            Some("https://cdn/avatars/7/a_1.png?size=128")
        );
        assert_eq!(
            off.banner(&profile).as_deref(),
            Some("https://cdn/banners/7/a_2.png?size=600")
        );
        assert_eq!(off.decoration(&profile), None);
        assert!(off.effect(&profile).is_empty());
        assert_eq!(off.frame(&profile), None);
        assert_eq!(off.tag(&profile), None);
        assert_eq!(off.name_style(&profile), None);
        assert_eq!(
            off.collectibles(),
            yapfast_discord::profile::Collectibles::default()
        );
        // A decoration that shows but may not move is its still.
        let still = Cosmetics {
            decorations: true,
            ..off
        };
        assert!(
            still
                .decoration(&profile)
                .unwrap()
                .ends_with("passthrough=false")
        );
    }

    #[test]
    fn on_shows_each_as_discord_gives_it() {
        let profile = dressed();
        let on = Cosmetics {
            effects: true,
            decorations: true,
            moving: true,
            name_styles: true,
            server_tags: true,
        };
        assert_eq!(on.avatar(&profile).as_deref(), profile.avatar.as_deref());
        assert_eq!(on.banner(&profile).as_deref(), profile.banner.as_deref());
        assert_eq!(
            on.decoration(&profile).as_deref(),
            profile.decoration.as_deref()
        );
        assert_eq!(on.effect(&profile).len(), 1);
        assert!(on.frame(&profile).is_some() && on.tag(&profile).is_some());
        assert!(on.collectibles().effect && on.collectibles().frame);
        // Emoji and stickers aren't a profile's to hold still.
        let emoji = "https://cdn/emojis/9.gif?size=48";
        assert_eq!(Cosmetics::default().picture(emoji), emoji);
    }
}
