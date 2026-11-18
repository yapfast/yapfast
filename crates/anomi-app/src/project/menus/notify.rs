//! A Discord channel's, category's and server's notification choices: the
//! header's bell, the rows of their menus that say how they notify, and
//! how long a mute lasts (`Notify`, `notification_rows`, `mute_times`,
//! `server_change` in `src/ui/menus.rs`).

use yapfast_core::application::{Application, MUTE_TIMES, MenuChoice};
use yapfast_core::chat::{Muting, NotificationChange, Notifications, NotifyLevel};
use yapfast_core::discord;
use yapfast_lang::{tr, trn};

use anomi_vm::icon::Icon;
use anomi_vm::menu::Row;

use super::Make;
use super::label::choice_id;

/// The ids of the notification levels a channel's or category's choices
/// offer.
const LEVELS: [(&str, Option<NotifyLevel>); 4] = [
    ("level-default", None),
    ("level-all", Some(NotifyLevel::All)),
    ("level-mentions", Some(NotifyLevel::Mentions)),
    ("level-nothing", Some(NotifyLevel::Nothing)),
];

pub(super) const SUPPRESS_EVERYONE: &str = "suppress-everyone";
pub(super) const SUPPRESS_ROLES: &str = "suppress-roles";

/// A Discord server channel's or category's notification choices, which
/// its menu and the header's bell build alike: muting it for a time, or
/// unmuting it, and which of its messages notify.
pub(crate) struct Notify {
    category: bool,
    /// A channel listed under a category, whose default is the category's.
    in_category: bool,
    settings: Notifications,
}

impl Notify {
    /// Nothing for a place that isn't a Discord server channel or category.
    pub(crate) fn of(core: &Application, place: &str) -> Option<Self> {
        let settings = core.notifications(place)?;
        discord::parse_room(place)?;
        Some(Self {
            category: !core.state.rooms.iter().any(|room| room.id == place),
            in_category: core.category_of(place).is_some(),
            settings,
        })
    }

    /// The bell's menu: muting, then the levels.
    pub(super) fn bell(&self, make: &Make<'_>, twelve_hour: bool) -> Vec<Row> {
        let mut rows = vec![self.mute_row(make, twelve_hour)];
        let mut levels = self.levels(make);
        if let Some(first) = levels.first_mut() {
            first.separated = true;
        }
        rows.extend(levels);
        rows
    }

    /// Mute for a time, in a submenu, or while muted, unmute, saying till
    /// when. Clicked itself, it mutes until it's turned back on, as
    /// Discord's does; while muted it has no submenu, and a click unmutes.
    pub(super) fn mute_row(&self, make: &Make<'_>, twelve_hour: bool) -> Row {
        let muted = self.settings.muted;
        let text = match (muted, self.category) {
            (true, true) => tr!("Unmute category"),
            (true, false) => tr!("Unmute channel"),
            (false, true) => tr!("Mute category"),
            (false, false) => tr!("Mute channel"),
        };
        let subtitle = self
            .settings
            .muted_until
            .map(|until| muted_until(until / 1000, twelve_hour))
            .unwrap_or_default();
        if muted {
            make.row(text, choice_id(MenuChoice::ChannelMute))
                .icon(Icon::Bell)
                .subtitle(subtitle)
        } else {
            make.row(text, &mute_id(Muting::Always))
                .icon(Icon::BellOff)
                .subtitle(subtitle)
                .submenu(mute_times(make))
        }
    }

    /// The levels, the one chosen checked; following the category's or the
    /// server's says which that is.
    fn levels(&self, make: &Make<'_>) -> Vec<Row> {
        LEVELS
            .into_iter()
            .map(|(id, level)| {
                let text = match level {
                    None if self.in_category => tr!("Use category default"),
                    None => tr!("Use server default"),
                    Some(level) => level_name(level),
                };
                let row = make.row(text, id).checked(self.settings.level == level);
                match level {
                    None => row.subtitle(level_name(self.settings.inherited)),
                    Some(_) => row,
                }
            })
            .collect()
    }

    /// Dresses a channel's or category's own rows in its menu: its mute,
    /// and its notification settings with the level that applies.
    pub(super) fn dress(&self, make: &Make<'_>, row: &mut Row, choice: MenuChoice, twelve: bool) {
        match choice {
            MenuChoice::ChannelMute => {
                let mut mute = self.mute_row(make, twelve);
                mute.separated = row.separated;
                mute.plain = row.plain;
                // In a plain menu it has no icon, as the rows around it.
                if row.plain {
                    mute.icon = None;
                }
                *row = mute;
            }
            MenuChoice::ChannelNotifications => {
                row.subtitle =
                    level_name(self.settings.level.unwrap_or(self.settings.inherited)).to_owned();
                row.intent = None;
                row.submenu = self.levels(make);
            }
            _ => {}
        }
    }

    /// What choosing a row of its choices, by its id, changes.
    pub(crate) fn change(&self, id: &str) -> Option<NotificationChange> {
        if let Some((_, level)) = LEVELS.into_iter().find(|(level, _)| *level == id) {
            Some(NotificationChange::Level(level))
        } else if let Some(muting) = muting(id) {
            Some(NotificationChange::Mute(muting))
        } else if id == choice_id(MenuChoice::ChannelMute) && self.settings.muted {
            Some(NotificationChange::Mute(Muting::Off))
        } else {
            None
        }
    }
}

pub(super) fn level_name(level: NotifyLevel) -> &'static str {
    match level {
        NotifyLevel::All => tr!("All messages"),
        NotifyLevel::Mentions => tr!("Only @mentions"),
        NotifyLevel::Nothing => tr!("Nothing"),
    }
}

/// The levels a server's notification submenu offers, the one that applies
/// checked, then its switches.
pub(super) fn server_rows(core: &Application, make: &Make<'_>, server: &str) -> Vec<Row> {
    let Some(notifications) = core.server_notifications(server) else {
        return Vec::new();
    };
    let own = notifications.own;
    let current = own.level.unwrap_or(own.inherited);
    // A server has no default above it.
    let levels = LEVELS.into_iter().filter_map(|(id, level)| {
        let level = level?;
        Some(make.row(level_name(level), id).checked(level == current))
    });
    levels
        .chain([
            make.row(tr!("Suppress @everyone and @here"), SUPPRESS_EVERYONE)
                .checkbox(notifications.suppress_everyone)
                .separated(),
            make.row(tr!("Suppress all role @mentions"), SUPPRESS_ROLES)
                .checkbox(notifications.suppress_roles),
        ])
        .collect()
}

/// The change to a Discord server's notification settings a choice from
/// its menu makes: a mute, a level or a switch, each switch the opposite
/// of what Discord last said.
pub(crate) fn server_change(
    core: &Application,
    server: &str,
    id: &str,
) -> Option<NotificationChange> {
    if let Some(muting) = muting(id) {
        return Some(NotificationChange::Mute(muting));
    }
    if let Some((_, Some(level))) = LEVELS.into_iter().find(|(level, _)| *level == id) {
        return Some(NotificationChange::Level(Some(level)));
    }
    let known = core.server_notifications(server)?;
    match id {
        SUPPRESS_EVERYONE => Some(NotificationChange::SuppressEveryone(
            !known.suppress_everyone,
        )),
        SUPPRESS_ROLES => Some(NotificationChange::SuppressRoles(!known.suppress_roles)),
        _ if id == choice_id(MenuChoice::HideMuted) => {
            Some(NotificationChange::HideMuted(!known.hide_muted))
        }
        _ if id == choice_id(MenuChoice::UnmuteServer) => {
            Some(NotificationChange::Mute(Muting::Off))
        }
        _ => None,
    }
}

/// When a mute ends, as its Unmute row says it.
pub(super) fn muted_until(seconds: u64, twelve_hour: bool) -> String {
    tr!("Muted until {}", until(seconds, twelve_hour))
}

/// When something ends, a Unix time: the time today, or the day too.
pub(super) fn until(seconds: u64, twelve_hour: bool) -> String {
    use yapfast_core::clock;
    let now = clock::now_ms() / 1000;
    let time = clock::time_on(seconds, twelve_hour);
    let on = clock::day(seconds);
    if on == clock::day(now) {
        time
    } else {
        format!("{on}, {time}")
    }
}

/// How long a mute lasts, as its submenu offers.
pub(super) fn mute_times(make: &Make<'_>) -> Vec<Row> {
    MUTE_TIMES
        .into_iter()
        .map(|muting| {
            let text = match muting {
                Muting::Minutes(minutes) if minutes % 60 == 0 => {
                    trn!("For {n} hour", "For {n} hours", minutes / 60)
                }
                Muting::Minutes(minutes) => trn!("For {n} minute", "For {n} minutes", minutes),
                Muting::Always | Muting::Off => tr!("Until I turn it back on").to_owned(),
            };
            make.row(&text, &mute_id(muting))
        })
        .collect()
}

/// A mute's row id.
pub(crate) fn mute_id(muting: Muting) -> String {
    match muting {
        Muting::Minutes(minutes) => format!("mute-{minutes}"),
        Muting::Always => "mute-always".into(),
        Muting::Off => "unmute".into(),
    }
}

/// The mute a row's id names.
pub(crate) fn muting(id: &str) -> Option<Muting> {
    MUTE_TIMES.into_iter().find(|&muting| mute_id(muting) == id)
}
