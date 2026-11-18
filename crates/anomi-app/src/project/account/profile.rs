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
/// arrived, as `0xRRGGBB`: the average of its opaque pixels, each counted
/// by how saturated it is, so a vivid avatar on a grey background gives its
/// vivid colour and a grey one its plain average (`dominant_color`,
/// `src/ui/profile.rs:979`).
pub fn dominant(state: &AppState, address: &str) -> Option<u32> {
    let picture = state.pictures.get(address)?;
    dominant_of(&picture.rgba)
}

fn dominant_of(rgba: &[u8]) -> Option<u32> {
    // About 1000 samples are enough for an average.
    let pixels = rgba.chunks_exact(4);
    let step = (pixels.len() / 1024).max(1);
    let (mut total, mut sum, mut count) = (0.0f64, [0.0f64; 3], 0u32);
    for pixel in pixels.step_by(step).filter(|pixel| pixel[3] >= 128) {
        let channels = [pixel[0], pixel[1], pixel[2]].map(f64::from);
        let high = channels.iter().copied().fold(0.0, f64::max);
        let low = channels.iter().copied().fold(255.0, f64::min);
        // Saturation, with a floor so that grey pictures still average.
        let weight = (high - low) / 255.0 + 0.02;
        for (sum, channel) in sum.iter_mut().zip(channels) {
            *sum += channel * weight;
        }
        total += weight;
        count += 1;
    }
    (count > 0).then(|| {
        sum.iter().fold(0u32, |color, sum| {
            (color << 8) | (sum / total).round().clamp(0.0, 255.0) as u32
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vivid_colour_stands_out_of_grey() {
        // Three grey pixels and a red one: the red one counts most.
        let pixels = [
            [128, 128, 128, 255],
            [128, 128, 128, 255],
            [128, 128, 128, 255],
            [255, 0, 0, 255],
        ]
        .concat();
        let color = dominant_of(&pixels).expect("opaque pixels");
        let [_, red, green, _] = color.to_be_bytes();
        assert!(red > 230 && green < 30, "{color:06x}");
        assert_eq!(dominant_of(&[0, 0, 0, 0]), None, "transparent only");
    }
}
