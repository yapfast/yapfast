//! A profile as its card draws it: the banner, its colour or the profile's
//! theme, the styled name, the server tag and the badges (`profile_data`,
//! `src/ui/profile.rs:588`). The account's popover shows its own; the
//! profile cards (I16) can take the same parts.

use yapfast_core::application::{AppState, Cosmetics};
use yapfast_core::chat::{Profile as CoreProfile, ProfileBadge};

use anomi_vm::account::Profile;
use anomi_vm::kind::{NameStyle, Tag};
use anomi_vm::people::badges::{Badge, Badges, Level};

use crate::pictures;
use crate::project::rows;

/// The parts of `own` the account's popover draws.
pub fn profile(state: &AppState, own: &CoreProfile) -> Profile {
    let cosmetics = state.cosmetics;
    Profile {
        banner: pictures::find(state, own.banner.as_deref()),
        accent: accent(state, own).map(rows::rgb),
        theme: own
            .theme
            .map(|[primary, secondary]| [rows::rgb(primary), rows::rgb(secondary)]),
        name_style: cosmetics
            .style(own.style.as_deref())
            .and_then(|style| NameStyle::new(style.font, style.effect, style.colors())),
        tag: cosmetics.tag(own.tag.as_ref()).map(|tag| Tag {
            text: tag.text.to_string(),
            badge: rows::expected(state, tag.badge.as_deref()),
            ..Tag::default()
        }),
        badges: badges(state, &own.badges),
    }
}

/// The banner's colour: the profile's own, or with no banner, colour or
/// theme, its avatar's dominant colour, as Discord tints it.
pub fn accent(state: &AppState, profile: &CoreProfile) -> Option<u32> {
    profile.accent.or_else(|| {
        (profile.banner.is_none() && profile.theme.is_none())
            .then(|| dominant(state, profile.avatar.as_deref()?))
            .flatten()
    })
}

/// A profile's badges, with the level of those that level up; the level's
/// name in capitals, as the card writes it.
pub fn badges(state: &AppState, badges: &[ProfileBadge]) -> Badges {
    Badges {
        badges: badges
            .iter()
            .map(|badge| Badge {
                icon: rows::expected(state, Some(&badge.icon)),
                description: badge.description.clone(),
                link: badge.link.clone().unwrap_or_default(),
                level: badge.level.as_ref().map(|level| Level {
                    kind: level.kind.clone().unwrap_or_default(),
                    title: level.title.to_uppercase(),
                    progress: level.progress.clone(),
                    picture: rows::expected(state, Some(&level.picture)),
                }),
            })
            .collect(),
    }
}

/// Each picture [`profile`] draws: the banner, the avatar its colour may
/// come from, the tag's badge and the badges'.
pub fn addresses(cosmetics: Cosmetics, profile: &CoreProfile, each: &mut impl FnMut(&str)) {
    let tag = cosmetics
        .tag(profile.tag.as_ref())
        .and_then(|tag| tag.badge.as_deref());
    for address in [profile.banner.as_deref(), profile.avatar.as_deref(), tag]
        .into_iter()
        .flatten()
    {
        each(address);
    }
    for badge in &profile.badges {
        each(&badge.icon);
        if let Some(level) = &badge.level {
            each(&level.picture);
        }
    }
}

/// The colour that stands out most in the picture at `address`, once it
/// arrived, as `0xRRGGBB` ([`yapfast_core::dominant_colour::dominant`],
/// as the Slint app's `avatar_color` takes it).
pub fn dominant(state: &AppState, address: &str) -> Option<u32> {
    let picture = state.pictures.get(address)?;
    yapfast_core::dominant_colour::dominant(
        &picture.rgba,
        yapfast_core::dominant_colour::Order::Rgba,
    )
}
