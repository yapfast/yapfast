//! Mod View: Discord's panel about one member of a server, for its
//! moderators, in place of the member list beside the channel. It shows
//! their signals, what they sent there, their moderation permissions,
//! roles and account, with Message, Kick, Ban, Time out and Copy ID over
//! them. See "Mod View" in docs/discord/servers.md.

use super::*;
use crate::chat::{AuditLog, AuditQuery, ListedMember, MemberChange, ModViewPart, ServerRole};
use server_settings::Known;

#[cfg(feature = "discord")]
use crate::chat::Permission;
#[cfg(feature = "discord")]
use yapfast_discord::ready::permission;

/// The permissions Mod View lists as a moderator's, as Discord's client
/// picks them: those its Community setup warns about, those moderators need
/// two-factor authentication for, and managing threads and webhooks; by bit.
pub const MOD_PERMISSIONS: [u32; 17] = [
    3, 1, 2, 5, 4, 28, 13, 30, 43, 33, 44, 40, 17, 53, 29, 34, 41,
];

/// The permissions Discord asks moderators to have two-factor
/// authentication for, which Mod View marks "Elevated"; by bit.
pub const ELEVATED_PERMISSIONS: [u32; 11] = [1, 2, 3, 4, 5, 13, 28, 29, 30, 34, 41];

/// A Discord account made less than this long ago, in milliseconds, gets
/// Yapfast's "New account" mark in Mod View.
pub const NEW_ACCOUNT_MS: u64 = 7 * 86_400_000;

/// Mod View, while it's open.
#[derive(Clone, Debug, Default)]
pub struct ModView {
    /// The channel it opened beside, by room id.
    pub place: String,
    /// The server, by its server id, and the member, by user id.
    pub server: String,
    pub user: String,
    pub panel: ModPanel,
    /// The member as Members' search finds them; `None` while it's coming.
    pub member: Option<Result<Option<ListedMember>, String>>,
    /// How many messages, links and media they sent; `None` while coming.
    pub activity: Option<Result<[u64; 3], String>>,
    /// Their Moderator Action Log: what moderators did to them.
    pub log: AuditLogPage,
}

/// What Mod View shows: the member, every permission they hold, or what
/// moderators did to them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModPanel {
    #[default]
    Info,
    Permissions,
    ActionLog,
}

/// Something done in Mod View.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModViewInput {
    /// Open it on a member, from their menu in a server channel.
    Open {
        place: String,
        user: String,
    },
    Close,
    Panel(ModPanel),
    /// Ask Discord again, as its Refresh does.
    Refresh,
    /// Search their messages, or those with links, in the search panel.
    Search {
        links: bool,
    },
    /// Something done on the Moderator Action Log.
    Log(AuditInput),
    /// Time them out for one of [`TIMEOUT_TIMES`] at once, without the
    /// dialog: a Yapfast addition.
    QuickTimeout(usize),
}

/// A permission a member holds, and what gives it to them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Granted {
    /// Its bit in Discord's permissions.
    pub bit: u32,
    /// The roles that give it, highest first, @everyone last, by name.
    pub roles: Vec<String>,
    /// They own the server, which gives every permission.
    pub owner: bool,
}

/// What Mod View shows of its member, from what Yapfast holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModViewMember {
    pub name: String,
    pub username: Option<String>,
    pub avatar: Option<String>,
    /// When they made their account and joined the server, in Unix
    /// milliseconds.
    pub created: u64,
    pub joined: Option<u64>,
    pub timed_out_until: Option<u64>,
    /// Their roles, highest first, each with whether this account may take
    /// it away; the roles it may give them.
    pub roles: Vec<(ServerRole, bool)>,
    pub addable: Vec<ServerRole>,
    /// Each permission they hold in the server, in the order of a role's
    /// Permissions tab.
    pub granted: Vec<Granted>,
    /// What this account may do to them.
    pub me: bool,
    pub may_timeout: bool,
    pub may_kick: bool,
    pub may_ban: bool,
    pub may_audit: bool,
    /// Who made the invite they joined by, by name, when known.
    pub inviter: Option<String>,
}

impl Application {
    /// Whether this account may open Mod View in the Discord server a place
    /// is in, as Discord's client lets moderators: with Administrator,
    /// Manage Server, Kick, Ban or Time Out Members, Manage Roles or Manage
    /// Nicknames.
    pub fn may_mod_view(&self, place: &str) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.discord_place(place) {
            let server = discord::server_id(&server.id);
            return [
                Permission::ManageServer,
                Permission::Kick,
                Permission::Ban,
                Permission::Moderate,
                Permission::ManageRoles,
                Permission::ManageNicknames,
            ]
            .into_iter()
            .any(|permission| self.may(&server, permission));
        }
        let _ = place;
        false
    }

    pub(super) fn mod_view_input(&mut self, input: ModViewInput) -> Vec<Effect> {
        match input {
            ModViewInput::Open { place, user } => self.open_mod_view(place, user),
            ModViewInput::Close => {
                self.state.mod_view = None;
                vec![]
            }
            ModViewInput::Panel(panel) => {
                if let Some(view) = self.state.mod_view.as_mut() {
                    view.panel = panel;
                }
                vec![]
            }
            ModViewInput::Refresh => match self.state.mod_view.take() {
                Some(view) => self.open_mod_view(view.place, view.user),
                None => vec![],
            },
            ModViewInput::Search { links } => self.search_mod_view(links),
            ModViewInput::Log(input) => self.mod_view_log_input(input),
            ModViewInput::QuickTimeout(index) => {
                let Some(view) = self.state.mod_view.as_ref() else {
                    return vec![];
                };
                let (place, user) = (view.place.clone(), view.user.clone());
                let seconds = TIMEOUT_TIMES[index.min(TIMEOUT_TIMES.len() - 1)];
                let change = MemberChange::Timeout {
                    until_ms: Some(crate::clock::now_ms() + u64::from(seconds) * 1000),
                    reason: String::new(),
                };
                self.change_member(place, user, change)
            }
        }
    }

    /// Opens Mod View on a member and asks Discord for what Yapfast
    /// doesn't hold: how they joined and their signals, their counts of
    /// messages, and, with View Audit Log, what moderators did to them.
    fn open_mod_view(&mut self, place: String, user: String) -> Vec<Effect> {
        if !self.may_mod_view(&place) || self.state.preview {
            return vec![];
        }
        let Some(guild) = discord::parse_room(&place).map(|(guild, _)| guild.to_owned()) else {
            return vec![];
        };
        let server = discord::server_id(&guild);
        let audit = self.may(&server, chat::Permission::ViewAuditLog);
        let (mut log, query) = AuditLogPage::about(user.clone());
        if !audit {
            log.asking = None;
            log.failed = true;
        }
        self.state.mod_view = Some(ModView {
            place,
            server: server.clone(),
            user: user.clone(),
            panel: ModPanel::Info,
            member: None,
            activity: None,
            log,
        });
        let mut effects = vec![Effect::Backend(Command::Discord(DiscordCommand::ModView {
            server: server.clone(),
            user,
        }))];
        if audit {
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::AuditLog { server, query },
            )));
        }
        effects
    }

    /// Searches the member's messages in the server, or those with links,
    /// as Mod View's counts open them.
    fn search_mod_view(&mut self, links: bool) -> Vec<Effect> {
        let Some(view) = self.state.mod_view.as_ref() else {
            return vec![];
        };
        #[cfg(feature = "discord")]
        let username = self.discord.as_ref().and_then(|snapshot| {
            snapshot
                .names
                .read()
                .username(&view.user)
                .map(str::to_owned)
        });
        #[cfg(not(feature = "discord"))]
        let username: Option<String> = None;
        let Some(username) = username.or_else(|| {
            let found = view.member.as_ref()?.as_ref().ok()?.as_ref()?;
            Some(
                found
                    .person
                    .username
                    .clone()
                    .unwrap_or_else(|| found.person.name.clone()),
            )
        }) else {
            return vec![];
        };
        let quoted = if username.contains(' ') {
            format!("\"{username}\"")
        } else {
            username
        };
        let query = if links {
            format!("from:{quoted} has:link")
        } else {
            format!("from:{quoted}")
        };
        self.search(query)
    }

    /// Something done on the Moderator Action Log.
    fn mod_view_log_input(&mut self, input: AuditInput) -> Vec<Effect> {
        let Some(view) = self.state.mod_view.as_mut() else {
            return vec![];
        };
        let query = match input {
            AuditInput::More => view.log.more(),
            AuditInput::Toggle(row) => {
                view.log.toggle(row);
                None
            }
            // It has no filters.
            AuditInput::User(_) | AuditInput::Action(_) => None,
        };
        query
            .map(|query| {
                Effect::Backend(Command::Discord(DiscordCommand::AuditLog {
                    server: view.server.clone(),
                    query,
                }))
            })
            .into_iter()
            .collect()
    }

    /// A page of the Moderator Action Log, unless Mod View moved on.
    pub(super) fn mod_view_log(
        &mut self,
        server: String,
        query: AuditQuery,
        result: Result<AuditLog, String>,
    ) {
        let asked =
            |view: &ModView| view.server == server && view.log.asking.as_ref() == Some(&query);
        let Some(view) = self.state.mod_view.as_mut().filter(|view| asked(view)) else {
            return;
        };
        // Taken out while it's put into words, which reads the server.
        let mut log = std::mem::take(&mut view.log);
        match result {
            Ok(page) => {
                let known = Known {
                    app: self,
                    server: &server,
                };
                log.take(&query, page, &known);
            }
            Err(_) => log.refused(),
        }
        if let Some(view) = self.state.mod_view.as_mut() {
            view.log = log;
        }
    }

    /// What Discord sent for Mod View, if it's still on that member.
    pub(super) fn mod_view_loaded(&mut self, server: String, user: String, part: ModViewPart) {
        let Some(view) = self
            .state
            .mod_view
            .as_mut()
            .filter(|view| view.server == server && view.user == user)
        else {
            return;
        };
        match part {
            ModViewPart::Member(member) => view.member = Some(member),
            ModViewPart::Activity(counts) => view.activity = Some(counts),
        }
    }

    /// Mod View closes when a channel of another server opens, as
    /// Discord's sidebar belongs to its server.
    pub(super) fn keep_mod_view(&mut self, room: &str) {
        let same = |view: &ModView| {
            discord::parse_room(room)
                .is_some_and(|(guild, _)| discord::server_id(guild) == view.server)
        };
        if self.state.mod_view.as_ref().is_some_and(|view| !same(view)) {
            self.state.mod_view = None;
        }
    }

    /// What Mod View shows of its member from what Yapfast holds: the
    /// service's names, the server's roles, and Members' search.
    pub fn mod_view_member(&self) -> Option<ModViewMember> {
        let view = self.state.mod_view.as_ref()?;
        #[cfg(feature = "discord")]
        {
            let place = view.place.as_str();
            let user = view.user.as_str();
            let (server, _, me) = self.discord_place(place)?;
            let found = view
                .member
                .as_ref()
                .and_then(|member| member.as_ref().ok())
                .and_then(Option::as_ref);
            let (name, username, avatar, timed_out_until, inviter) = match &self.discord {
                Some(snapshot) => {
                    let names = snapshot.names.read();
                    let owned = |name: Option<&str>| name.map(str::to_owned);
                    let inviter = found
                        .and_then(|found| found.inviter.as_deref())
                        .and_then(|inviter| owned(names.name(&server.id, inviter)));
                    (
                        owned(names.name(&server.id, user)),
                        owned(names.username(user)),
                        Some(names.avatar(Some(&server.id), user)),
                        names.timed_out_until(&server.id, user),
                        inviter,
                    )
                }
                None => (None, None, None, None, None),
            };
            let person = self.state.person(user);
            let name = name
                .or_else(|| found.map(|found| found.person.name.clone()))
                .or_else(|| person.as_ref().map(|person| person.name.clone()))
                .unwrap_or_else(|| user.to_owned());
            let username =
                username.or_else(|| found.and_then(|found| found.person.username.clone()));
            let timed_out_until = timed_out_until
                .or_else(|| found.map(|found| found.timed_out_until))
                .filter(|until| *until > crate::clock::now_ms());
            let held = self.member_roles(place, user);
            // Theirs, each with whether it may be taken away, and those it
            // may give them.
            let (roles, addable): (Vec<_>, Vec<_>) = self
                .server_roles(place)
                .into_iter()
                .map(|role| {
                    let grantable = self.may_grant(place, &role.id);
                    (role, grantable)
                })
                .filter(|(role, grantable)| *grantable || held.contains(&role.id))
                .partition(|(role, _)| held.contains(&role.id));
            let addable = addable.into_iter().map(|(role, _)| role).collect();
            let is_me = user == me;
            let server_place = discord::server_id(&server.id);
            let may = |permission| {
                !is_me
                    && self.may(&server_place, permission)
                    && self.outranks(&server_place, user, &held)
            };
            Some(ModViewMember {
                name,
                username,
                avatar: avatar.or_else(|| found.map(|found| found.person.avatar.clone())),
                created: created(user),
                joined: found
                    .map(|found| found.joined)
                    .filter(|joined| *joined != 0),
                timed_out_until,
                roles,
                addable,
                granted: granted(server, user, &held),
                me: is_me,
                may_timeout: may(Permission::Moderate) && !self.is_admin(place, user),
                may_kick: may(Permission::Kick),
                may_ban: may(Permission::Ban),
                may_audit: self.may(&server_place, Permission::ViewAuditLog),
                inviter,
            })
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = view;
            None
        }
    }
}

/// When a Discord account was made, from its id, in Unix milliseconds.
pub fn created(user: &str) -> u64 {
    user.parse::<u64>()
        .map_or(0, |id| (id >> 22) + 1_420_070_400_000)
}

/// The permissions a member holding `held` has in a server, each with
/// what gives it, in the order of a role's Permissions tab, as Mod View's
/// All Granted Permissions lists them. A timeout doesn't take them away
/// here, as it doesn't in Discord's.
#[cfg(feature = "discord")]
fn granted(
    server: &yapfast_discord::marked::MarkedServer,
    user: &str,
    held: &[String],
) -> Vec<Granted> {
    let owner = server.owner_id.as_deref() == Some(user);
    let bits = |role: &str| server.roles.get(role).copied().unwrap_or_default();
    // Highest first, @everyone last.
    let mut roles: Vec<&String> = held
        .iter()
        .filter(|role| server.roles.contains_key(*role))
        .collect();
    roles.sort_by_key(|role| {
        std::cmp::Reverse(
            server
                .role_positions
                .get(*role)
                .copied()
                .unwrap_or_default(),
        )
    });
    roles.push(&server.id);
    let all = owner
        || roles
            .iter()
            .any(|role| bits(role) & permission::ADMINISTRATOR != 0);
    let base = roles.iter().fold(0, |base, role| base | bits(role));
    ROLE_PERMISSIONS
        .iter()
        .flat_map(|group| group.permissions)
        .filter(|info| all || base & (1u64 << info.bit) != 0)
        .map(|info| Granted {
            bit: info.bit,
            roles: roles
                .iter()
                .filter(|role| {
                    let bits = bits(role);
                    bits & permission::ADMINISTRATOR != 0 || bits & (1u64 << info.bit) != 0
                })
                .map(|role| {
                    if **role == server.id {
                        "@everyone".to_owned()
                    } else {
                        server.role_names.get(*role).cloned().unwrap_or_default()
                    }
                })
                .collect(),
            owner,
        })
        .collect()
}
