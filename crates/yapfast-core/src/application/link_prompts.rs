//! What the dialogs a clicked Discord link opens say: a scheduled event's,
//! a gift's and an application's authorization, each from its dialog's
//! state ([`EventDialog`], [`GiftDialog`], [`OAuthDialog`]), in Discord's
//! wording.

use super::{AppState, ROLE_PERMISSIONS};
use crate::{chat::GiftPreview, links};

/// A scheduled event's dialog: when and where it is, what it says, and how
/// changing interest in it goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventPrompt {
    pub title: &'static str,
    /// The event, once found: its name, when it is, and where.
    pub found: bool,
    pub name: String,
    pub when: String,
    pub detail: String,
    /// What the event says about itself.
    pub description: String,
    /// What the main button does, such as "Interested"; empty for none.
    pub confirm: &'static str,
    pub busy: bool,
    /// How it's going, such as "This event is going on now.".
    pub status: &'static str,
    pub error: String,
}

/// A gift's dialog: what it grants, who sent it, and how redeeming goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GiftPrompt {
    pub title: &'static str,
    pub found: bool,
    pub name: String,
    pub detail: String,
    /// What the main button does, such as "Redeem Gift"; empty for none.
    pub confirm: &'static str,
    pub busy: bool,
    pub status: &'static str,
    pub error: String,
}

/// An application's authorization: what it asks for, in words, the
/// servers it can be added to, and how authorizing goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OAuthPrompt {
    pub title: String,
    pub found: bool,
    pub name: String,
    pub icon: Option<String>,
    /// What it says about itself, or the bot it adds.
    pub detail: String,
    /// What it asks for, a line each, and what of that can take over the
    /// account or a server, such as Administrator, in red above it.
    pub wants: String,
    pub warning: String,
    /// The servers it can be added to, by id and name, and which is
    /// chosen; offered to choose from when `guild_picker`.
    pub guilds: Vec<(String, String)>,
    pub guild_chosen: usize,
    pub guild_picker: bool,
    /// The one server a link names, which can't be changed.
    pub guild_name: String,
    /// Where Discord sends the person next when Yapfast won't open it,
    /// such as an app's own address, to copy.
    pub address: String,
    /// What the main button does, "Authorize"; empty for none.
    pub confirm: &'static str,
    pub busy: bool,
    pub status: &'static str,
    pub error: String,
}

/// Discord's Administrator permission, which grants every other.
pub const ADMINISTRATOR: u64 = 1 << 3;

impl AppState {
    /// The scheduled event a clicked event link opened, with its times in
    /// the 12-hour clock or not.
    pub fn event_prompt(&self, twelve_hour: bool) -> Option<EventPrompt> {
        let dialog = self.scheduled_event.as_ref()?;
        let preview = dialog.preview.as_ref();
        let when = preview
            .filter(|preview| preview.starts_at > 0)
            .map(|preview| {
                crate::clock::event(
                    preview.starts_at,
                    preview.ends_at,
                    crate::clock::now_ms() / 1000,
                    twelve_hour,
                )
            });
        let detail = preview.map(|preview| {
            let mut parts = Vec::new();
            if let Some(location) = &preview.location {
                parts.push(location.clone());
            } else if let Some(room) = &preview.room
                && let Some(found) = self.rooms.iter().find(|known| &known.id == room)
            {
                parts.push(format!("#{}", found.name));
            }
            if let Some(creator) = &preview.creator {
                parts.push(format!("by {creator}"));
            }
            if let Some(count) = preview.interested_count {
                parts.push(format!("{count} interested"));
            }
            parts.join(" · ")
        });
        let confirm = match preview {
            Some(preview) if !preview.over && preview.interested => "Remove interest",
            Some(preview) if !preview.over => "Interested",
            _ => "",
        };
        let status = if dialog.resolving {
            "Looking up the event..."
        } else if dialog.marking {
            "Saving..."
        } else if preview.is_some_and(|preview| preview.over) {
            "This event has ended."
        } else if preview.is_some_and(|preview| preview.active) {
            "This event is going on now."
        } else if preview.is_some_and(|preview| preview.interested) {
            "You're interested in this event."
        } else {
            ""
        };
        Some(EventPrompt {
            title: "Event",
            found: preview.is_some(),
            name: preview.map_or_else(String::new, |preview| preview.name.clone()),
            when: when.unwrap_or_default(),
            detail: detail.unwrap_or_default(),
            description: preview
                .and_then(|preview| preview.description.clone())
                .unwrap_or_default(),
            confirm,
            busy: dialog.resolving || dialog.marking,
            status,
            error: dialog.error.clone().unwrap_or_default(),
        })
    }

    /// The gift a clicked gift link opened.
    pub fn gift_prompt(&self) -> Option<GiftPrompt> {
        let gift = self.gift.as_ref()?;
        let preview = gift.preview.as_ref();
        let detail = preview.map(|preview| {
            let mut parts = Vec::new();
            if let Some(plan) = &preview.plan {
                parts.push(plan.clone());
            }
            if let Some(gifter) = &preview.gifter {
                parts.push(format!("from {gifter}"));
            }
            parts.join(" · ")
        });
        let redeeming = self.redeeming_gifts.contains(&gift.code);
        let redeemable = !gift.redeemed && preview.is_some_and(GiftPreview::redeemable);
        let in_discord = !gift.redeemed && preview.is_some_and(|preview| preview.in_discord);
        let status = if gift.redeemed {
            "Redeemed. Enjoy!"
        } else if redeeming {
            "Redeeming..."
        } else if gift.resolving {
            "Looking up the gift..."
        } else if preview.is_some_and(|preview| preview.redeemed) {
            "You've already redeemed this gift."
        } else if preview.is_some_and(|preview| preview.spent) {
            "This gift has already been redeemed."
        } else if in_discord {
            "This gift needs a payment method, which Yapfast can't take, so it can't be redeemed here."
        } else {
            ""
        };
        Some(GiftPrompt {
            title: "You've been sent a gift",
            found: preview.is_some(),
            name: preview.map_or_else(String::new, |preview| preview.name.clone()),
            detail: detail.unwrap_or_default(),
            confirm: if redeemable { "Redeem Gift" } else { "" },
            busy: gift.resolving || redeeming,
            status,
            error: gift.error.clone().unwrap_or_default(),
        })
    }

    /// The application a clicked authorization link opened.
    pub fn oauth_prompt(&self) -> Option<OAuthPrompt> {
        let dialog = self.oauth.as_ref()?;
        let preview = dialog.preview.as_ref();
        let addable: Vec<(String, String)> = preview
            .map(|preview| {
                preview
                    .guilds
                    .iter()
                    .filter(|guild| guild.can_add)
                    .map(|guild| (guild.id.clone(), guild.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let guild_chosen = addable
            .iter()
            .position(|(id, _)| Some(id) == dialog.guild.as_ref())
            .unwrap_or_default();
        let detail = preview.and_then(|preview| {
            preview
                .about
                .clone()
                .filter(|about| !about.is_empty())
                .or_else(|| preview.bot.clone())
        });
        let wants = preview.map(|preview| {
            let mut lines: Vec<String> = preview
                .wants
                .iter()
                .map(|wants| format!("- {wants}"))
                .collect();
            if let Some(bits) = preview.permissions.filter(|bits| *bits != 0) {
                lines.push(format!("- Bot permissions: {}", permission_names(bits)));
            }
            match preview.redirect.as_deref() {
                Some(url) if links::openable(url) => {
                    let host = url
                        .split_once("://")
                        .map(|(_, rest)| rest)
                        .and_then(|rest| rest.split(['/', '?', '#']).next())
                        .unwrap_or_default();
                    lines.push(format!("Afterward, Discord sends you to {host}."));
                }
                Some(url) => lines.push(format!(
                    "Afterward, Discord hands its answer to {url}, which Yapfast won't open."
                )),
                None => {}
            }
            lines.join("\n")
        });
        let mut warnings = Vec::new();
        if preview.is_some_and(|preview| preview.controls_client) {
            warnings.push(
                "It asks to control your Discord client, which can do nearly anything \
                 your account can. Authorize it only if you trust it with your account.",
            );
        }
        if preview
            .and_then(|preview| preview.permissions)
            .is_some_and(|bits| bits & ADMINISTRATOR != 0)
        {
            warnings.push(
                "Its bot asks for Administrator: every permission in the server, \
                 in every channel, whatever its roles allow.",
            );
        }
        let locked = preview.is_some_and(|preview| preview.guild_install && preview.guild_locked);
        let guild_picker =
            preview.is_some_and(|preview| preview.guild_install && !locked && !addable.is_empty());
        // A link that names its one server shows it, as it can't be changed.
        let locked_to = preview
            .filter(|_| locked)
            .zip(dialog.guild.as_ref())
            .and_then(|(preview, chosen)| preview.guilds.iter().find(|guild| &guild.id == chosen))
            .map(|guild| guild.name.clone());
        let blocked =
            preview.is_some_and(|preview| preview.guild_install && dialog.guild.is_none());
        let status = if dialog.resolving {
            "Looking up the application..."
        } else if dialog.authorizing {
            "Authorizing..."
        } else if dialog.done && dialog.next.is_some() {
            "Authorized. Discord sends you on to this address, which isn't a web page, \
             so Yapfast doesn't open it. Copy it if the app asks for it."
        } else if dialog.done {
            "Authorized."
        } else if !locked
            && preview.is_some_and(|preview| preview.guild_install && addable.is_empty())
        {
            "You don't manage any server it can be added to."
        } else if preview.is_some_and(|preview| preview.authorized) {
            "You've authorized this before. Authorizing again is fine."
        } else {
            ""
        };
        Some(OAuthPrompt {
            title: match preview {
                Some(preview) => format!("Authorize {}", preview.name),
                None => "Authorize".to_owned(),
            },
            found: preview.is_some(),
            name: preview.map_or_else(String::new, |preview| preview.name.clone()),
            icon: preview.and_then(|preview| preview.icon.clone()),
            detail: detail.unwrap_or_default(),
            wants: wants.unwrap_or_default(),
            warning: warnings.join("\n"),
            guilds: addable,
            guild_chosen,
            guild_picker,
            guild_name: locked_to.unwrap_or_default(),
            address: dialog.next.clone().unwrap_or_default(),
            confirm: if preview.is_some() && !dialog.done && !blocked {
                "Authorize"
            } else {
                ""
            },
            busy: dialog.resolving || dialog.authorizing,
            status,
            error: dialog.error.clone().unwrap_or_default(),
        })
    }
}

/// A bot's asked-for permissions, by the titles of a role's switches,
/// and how many more it asks for that no switch shows.
pub fn permission_names(bits: u64) -> String {
    let mut named = 0u64;
    let mut names: Vec<&str> = Vec::new();
    for permission in ROLE_PERMISSIONS
        .iter()
        .flat_map(|group| group.permissions.iter())
    {
        let bit = 1u64 << permission.bit;
        if bits & bit != 0 && named & bit == 0 {
            named |= bit;
            names.push(permission.title);
        }
    }
    let more = (bits & !named).count_ones();
    let more = match (more, names.is_empty()) {
        (0, _) => None,
        (1, true) => Some("1 permission Yapfast can't name".to_owned()),
        (count, true) => Some(format!("{count} permissions Yapfast can't name")),
        (1, false) => Some("1 more permission".to_owned()),
        (count, false) => Some(format!("{count} more permissions")),
    };
    match (names.pop(), more) {
        (Some(last), Some(more)) => {
            names.push(last);
            format!("{} and {more}", names.join(", "))
        }
        (Some(last), None) if names.is_empty() => last.to_owned(),
        (Some(last), None) => format!("{} and {last}", names.join(", ")),
        (None, more) => more.unwrap_or_default(),
    }
}
