//! The cards about places a person's things open: a role's (who has it, a
//! role mentioned in a message: `show_role`, `src/ui/refresh.rs:2679`), a
//! server tag's (`tag_card`, `src/ui/profile.rs:1019`) and a mutual
//! server's, which the intent makes from the profile's row.

use yapfast_core::application::AppState;
use yapfast_lang::{tr, trn};

use anomi_vm::kind::{Expected, Tag};
use anomi_vm::people::cards::{Cards, Member, Role, ServerTag};
use anomi_vm::shell::{Layer, Shell};

use crate::project::rows;

/// The window's title bar and the room left under it, which a role's card
/// keeps clear of (`limit: root.height - root.title-height - 16px`).
const ABOVE: f32 = 32.0 + 16.0;

/// A tag whose card is open: the server it's from, and how it showed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ShownTag {
    pub server: String,
    pub text: String,
    pub badge: Expected,
}

/// The role's card while the core shows the role; the tag's while its card
/// is open.
pub(super) fn show(
    state: &AppState,
    height: f32,
    tag: Option<&ShownTag>,
    shell: &mut Shell,
    cards: &mut Cards,
) {
    match &state.role {
        Some(role) => {
            let made = Role {
                name: role.name.clone(),
                color: role.color.map(rows::rgb),
                note: match &role.members {
                    None => tr!("Loading..."),
                    Some(members) if members.is_empty() => {
                        tr!("Yapfast hasn't seen anyone with this role yet.")
                    }
                    Some(_) => "",
                }
                .to_owned(),
                members: role
                    .members
                    .iter()
                    .flatten()
                    .map(|member| Member {
                        id: member.id.clone(),
                        name: member.name.clone(),
                        avatar: rows::avatar(
                            state,
                            &member.name,
                            &member.name,
                            member.avatar.as_deref(),
                        ),
                        color: member.color.map(rows::rgb),
                    })
                    .collect(),
                limit: if height > 0.0 {
                    height - ABOVE
                } else {
                    f32::INFINITY
                },
            };
            if cards.role.as_ref() != Some(&made) {
                cards.role = Some(made);
            }
            if !shell.is_open(Layer::RoleCard) {
                shell.open(Layer::RoleCard);
            }
        }
        None => shell.close(Layer::RoleCard),
    }
    if let Some(tag) = tag.filter(|_| shell.is_open(Layer::Card)) {
        let made = tag_card(state, tag);
        if cards.tag.as_ref() != Some(&made) {
            cards.tag = Some(made);
        }
    }
    // A mutual server's pictures, as they come.
    let listed = state.profile.as_ref().zip(cards.server.as_mut());
    if let Some((view, card)) = listed
        && let Some(server) = view
            .profile
            .servers
            .iter()
            .find(|server| server.id == card.id)
    {
        let (icon, banner) = (
            rows::expected(state, server.icon.as_deref()),
            rows::expected(state, server.banner.as_deref()),
        );
        if card.icon != icon || card.banner != banner {
            (card.icon, card.banner) = (icon, banner);
        }
    }
}

/// The pictures the role's card shows.
pub(super) fn wanted(state: &AppState) -> Vec<&str> {
    state
        .role
        .iter()
        .flat_map(|role| role.members.iter().flatten())
        .filter_map(|member| member.avatar.as_deref())
        .collect()
}

/// What a server tag's card shows, as Discord's client does: the tag, and
/// the server's name and icon when this account is in it, while it loads;
/// then the server's profile, or a word on why there's none.
fn tag_card(state: &AppState, tag: &ShownTag) -> ServerTag {
    let joined = state
        .servers
        .iter()
        .find(|server| server.id == tag.server && !server.unavailable);
    let answer = state
        .tag_server
        .as_ref()
        .filter(|(server, _)| *server == tag.server)
        .map(|(_, answer)| answer);
    let profile = answer.and_then(|answer| answer.as_ref().ok()?.as_ref());
    let (private, failed) = (
        matches!(answer, Some(Ok(None))),
        matches!(answer, Some(Err(_))),
    );
    let known = joined.map(|server| server.name.as_str());
    let name = match (profile, private, failed) {
        (Some(profile), ..) => profile.name.as_str(),
        (None, true, _) => known.unwrap_or(tr!("Private Server")),
        (None, _, true) => known.unwrap_or(tr!("Unknown Server")),
        _ => known.unwrap_or_default(),
    };
    let icon = profile
        .and_then(|profile| profile.icon.as_deref())
        .or(joined.and_then(|server| server.icon.as_deref()));
    let grouped = yapfast_core::emoji::card::grouped;
    ServerTag {
        server: tag.server.clone(),
        name: name.to_owned(),
        initial: if name.is_empty() {
            tag.text
                .chars()
                .next()
                .map(String::from)
                .unwrap_or_default()
        } else {
            rows::initial(name)
        },
        icon: rows::expected(state, icon),
        tag: Tag {
            text: tag.text.clone(),
            badge: tag.badge.clone(),
            server: tag.server.clone(),
        },
        online: profile
            .and_then(|profile| profile.online)
            .map(|count| tr!("{} Online", grouped(count)))
            .unwrap_or_default(),
        members: profile
            .and_then(|profile| profile.members)
            .map(|count| trn!("{} Member", "{} Members", count, grouped(count)))
            .unwrap_or_default(),
        established: profile
            .map(|profile| yapfast_core::clock::month(profile.created_ms / 1000))
            .filter(|month| !month.is_empty())
            .map(|month| tr!("Est. {}", month))
            .unwrap_or_default(),
        description: profile.map_or_else(String::new, |profile| profile.description.clone()),
        note: match (private, failed) {
            (true, _) => tr!("The server has limited who can see this profile."),
            (_, true) => tr!("Unable to load server profile"),
            _ => "",
        }
        .to_owned(),
        loading: answer.is_none(),
        limited: private,
        failed,
        joined: joined.is_some(),
    }
}

/// The icon a tag's card shows, to ask for.
pub(super) fn tag_wanted<'a>(state: &'a AppState, tag: &ShownTag) -> Option<&'a str> {
    let answer = state
        .tag_server
        .as_ref()
        .filter(|(server, _)| *server == tag.server)?;
    answer.1.as_ref().ok()?.as_ref()?.icon.as_deref()
}
