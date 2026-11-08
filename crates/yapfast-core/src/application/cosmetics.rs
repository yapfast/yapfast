//! Which of the cosmetics people wear show, as Settings → Appearance →
//! Profiles says. What's off is never asked for: the pictures the core
//! downloads, and the rows the window draws, go through these first, so an
//! effect, decoration or tag badge that's off is neither fetched nor
//! decoded. Nameplates have their own switch, `AppState::hide_nameplates`.

/// The profile switches. Off is the cheaper side of each, which fetches and
/// decodes nothing of it; see "Profile switches" in `docs/discord/people.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cosmetics {
    /// Profile effects play over cards: a request for what the effect is
    /// made of, then its pictures decoded frame by frame.
    pub effects: bool,
    /// Avatar decorations show.
    pub decorations: bool,
    /// Decorations and nameplates play their animated pictures. Off, only
    /// their stills are fetched.
    pub moving: bool,
    /// Display names show in the font and colours their person picked.
    pub name_styles: bool,
    /// Server tags show beside names, with their badges.
    pub server_tags: bool,
}

impl Default for Cosmetics {
    /// Everything on, as Discord's client draws a profile; each switch turns
    /// its cosmetic off.
    fn default() -> Self {
        Self {
            effects: true,
            decorations: true,
            moving: true,
            name_styles: true,
            server_tags: true,
        }
    }
}

impl Cosmetics {
    /// The address of a decoration around an avatar, if decorations show.
    pub fn decoration(self, url: Option<&str>) -> Option<&str> {
        url.filter(|_| self.decorations)
    }

    /// A server tag beside a name, if tags show.
    pub fn tag<T>(self, tag: Option<&T>) -> Option<&T> {
        tag.filter(|_| self.server_tags)
    }

    /// A display name's style, if styles show.
    pub fn style<T>(self, style: Option<T>) -> Option<T> {
        style.filter(|_| self.name_styles)
    }

    /// A profile's effect, if effects play.
    pub fn effect<T>(self, effect: Option<T>) -> Option<T> {
        effect.filter(|_| self.effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_leaves_out_each_cosmetic() {
        let off = Cosmetics {
            effects: false,
            decorations: false,
            moving: false,
            name_styles: false,
            server_tags: false,
        };
        assert_eq!(off.decoration(Some("https://cdn/static")), None);
        assert_eq!(off.tag(Some(&"TAG")), None);
        assert_eq!(off.style(Some(1)), None);
        assert_eq!(off.effect(Some("sku")), None);
    }

    #[test]
    fn on_shows_each_as_discord_gives_it() {
        let on = Cosmetics {
            effects: true,
            decorations: true,
            moving: true,
            name_styles: true,
            server_tags: true,
        };
        assert_eq!(on.decoration(Some("url")), Some("url"));
        assert_eq!(on.tag(Some(&"TAG")), Some(&"TAG"));
        assert_eq!(on.style(Some(1)), Some(1));
        assert_eq!(on.effect(Some("sku")), Some("sku"));
        // Every switch starts on.
        assert_eq!(Cosmetics::default(), on);
    }
}
