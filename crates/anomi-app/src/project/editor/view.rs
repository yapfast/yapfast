//! The profile as the editor's card wears it: the texts as written over
//! what the profile falls back to, the pictures chosen, the theme, the name
//! style and the server tag chosen, made into a `ProfileView` and drawn by
//! the popout's own [`card::card`], so the preview can't drift from what
//! others see (`show_profile_editor`, `src/ui/refresh.rs:3036`).

use yapfast_core::application::{AppState, Application, ProfileEditor, ProfileText, ProfileView};
use yapfast_core::chat::{Profile, ProfilePicture, ServerTag};

use anomi_vm::kind::Expected;
use anomi_vm::people::profile::Card;

use crate::pictures;
use crate::project::people::card;
use crate::project::rows;

/// The card the editor shows, its pictures looked up through
/// `pictures::find`, so that the caller may note them.
pub fn card(core: &Application, editor: &ProfileEditor) -> Card {
    let state = &core.state;
    let view = view(state, editor);
    let mut made = card::card(core, &view);
    let (_, avatar) = editor.picture(ProfilePicture::Avatar);
    let (_, banner) = editor.picture(ProfilePicture::Banner);
    if let Some(avatar) = avatar {
        made.avatar.picture = pictures::shown(avatar);
        // With no banner, colour or theme, the banner takes the new
        // avatar's colour, as it took the old one's.
        if view.profile.accent.is_none()
            && view.profile.banner.is_none()
            && view.profile.theme.is_none()
        {
            let dominant = yapfast_core::dominant_colour::dominant(
                &avatar.rgba,
                yapfast_core::dominant_colour::Order::Rgba,
            );
            made.accent = dominant.map(rows::rgb);
        }
    }
    if let Some(banner) = banner {
        made.banner = pictures::shown(banner).map_or(Expected::Waiting, Expected::Ready);
    }
    made
}

/// What the card shows, as Discord's preview has it.
fn view(state: &AppState, editor: &ProfileEditor) -> ProfileView {
    let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
    let kept = editor.profile.as_ref();
    let or_fallback = |written: &str, fallback: Option<&String>| {
        let written = written.trim();
        if written.is_empty() {
            fallback.cloned().unwrap_or_default()
        } else {
            written.to_owned()
        }
    };
    let (avatar, _) = editor.picture(ProfilePicture::Avatar);
    let (banner, _) = editor.picture(ProfilePicture::Banner);
    let kept_accent = kept.and_then(|profile| profile.accent);
    // The colour as written while it's one; Discord's while it isn't.
    let accent = if editor.may_write(ProfileText::Colour) {
        editor.accent().unwrap_or(kept_accent)
    } else {
        kept_accent
    };
    ProfileView {
        profile: Profile {
            id: fallback.map(|own| own.id.clone()).unwrap_or_default(),
            name: or_fallback(&editor.name, fallback.map(|own| &own.name)),
            handle: fallback.map(|own| own.handle.clone()).unwrap_or_default(),
            avatar: avatar
                .map(str::to_owned)
                .or_else(|| fallback.and_then(|own| own.avatar.clone())),
            banner: banner
                .map(str::to_owned)
                .or_else(|| fallback.and_then(|own| own.banner.clone())),
            accent: accent.or_else(|| fallback.and_then(|own| own.accent)),
            pronouns: or_fallback(&editor.pronouns, fallback.map(|own| &own.pronouns)),
            // Its times as the profile shows them, as Discord's preview does.
            bio: yapfast_core::clock::render_timestamps(&or_fallback(
                &editor.bio,
                fallback.map(|own| &own.bio),
            )),
            created: fallback.and_then(|own| own.created.clone()),
            joined: fallback.and_then(|own| own.joined.clone()),
            server_name: fallback
                .map(|own| own.server_name.clone())
                .unwrap_or_default(),
            server_icon: fallback.and_then(|own| own.server_icon.clone()),
            theme: editor.theme().ok().flatten(),
            style: editor.name_style().map(std::sync::Arc::new),
            tag: shown_tag(state, editor),
            // What the editor doesn't change comes from the profile it
            // read: its decoration, effect, badges, connections and what
            // it's doing.
            ..fallback.map(worn).unwrap_or_default()
        },
        ..ProfileView::default()
    }
}

/// What the profile wears that the editor doesn't change.
fn worn(own: &Profile) -> Profile {
    Profile {
        decoration: own.decoration.clone(),
        effect: own.effect.clone(),
        badges: own.badges.clone(),
        connections: own.connections.clone(),
        presence: own.presence,
        activity: own.activity.clone(),
        activities: own.activities.clone(),
        ..Profile::default()
    }
}

/// The server tag it would wear: the one chosen, else the profile's.
pub fn shown_tag(state: &AppState, editor: &ProfileEditor) -> Option<ServerTag> {
    match &editor.chosen_tag {
        None => editor
            .profile
            .as_ref()
            .and_then(|profile| profile.fallback.tag.clone()),
        Some(None) => None,
        Some(Some(chosen)) => state
            .servers
            .iter()
            .find(|server| server_id(&server.id) == chosen)
            .and_then(|server| server.tag.clone()),
    }
}

/// A Discord server's own id, from Yapfast's (`discord_server_id`).
pub fn server_id(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}
