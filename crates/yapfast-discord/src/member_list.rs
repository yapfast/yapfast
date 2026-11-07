//! A channel's member list, as Discord's sidebar shows it: the people who can
//! read the channel, under their highest hoisted role, then online, then
//! offline. Discord sends the rows a client subscribed to in
//! GUILD_MEMBER_LIST_UPDATE events, and keeps them current with operations
//! on those rows.

use std::{collections::HashMap, sync::Arc, time::Instant};

use serde_json::Value;

use crate::{
    activity::{Activities, activities},
    collectibles::{Nameplate, Style},
    message::{self, CustomEmoji, Names, Tag},
    ready::Server,
};

/// The rows Yapfast asks for, group headings included.
pub const ROWS: usize = 100;

/// One person in the list, ready to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedMember {
    pub user_id: String,
    pub name: String,
    pub username: Option<String>,
    /// The address of the avatar Discord shows for them on this server.
    pub avatar: String,
    /// The decoration they wear around their avatar, and the nameplate
    /// behind their row.
    pub decoration: Option<Arc<str>>,
    pub nameplate: Option<Nameplate>,
    /// How they draw their name, when they have a display name style.
    pub style: Option<Style>,
    /// The heading they're listed under, with its size, such as `Mods — 2`.
    pub group: String,
    /// The address of the picture of the role their group is, and of the
    /// highest of their roles that has one.
    pub group_icon: Option<Arc<str>>,
    pub role_icon: Option<Arc<str>>,
    /// The server tag they wear by their name.
    pub tag: Option<Tag>,
    pub presence: Presence,
    /// The RGB colour of their highest coloured role.
    pub color: Option<u32>,
    /// Their custom status, else what they're doing, as Discord shows it.
    pub activity: Option<String>,
    /// What they're doing, in full, as their profile shows it.
    pub activities: Activities,
    /// Their roles' ids, shared with [`Names`]; empty outside a server.
    pub roles: Arc<[String]>,
    /// When their timeout ends, in Unix milliseconds, while they're timed
    /// out as far as Yapfast knows.
    pub timed_out_until: Option<u64>,
}

/// Whether someone is around, as their status dot shows.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Presence {
    Online,
    /// Online, and only on a phone.
    Mobile,
    Idle,
    DoNotDisturb,
    /// Offline or invisible.
    #[default]
    Offline,
}

impl Presence {
    /// Reads a presence object: its status, and from `client_status` whether
    /// they're online only on a phone, which Discord marks with a phone.
    pub fn parse(presence: &Value) -> Self {
        let clients = &presence["client_status"];
        match presence["status"].as_str() {
            Some("online")
                if clients["mobile"] == "online"
                    && clients["desktop"] != "online"
                    && clients["web"] != "online" =>
            {
                Self::Mobile
            }
            Some("online") => Self::Online,
            Some("idle") => Self::Idle,
            Some("dnd") => Self::DoNotDisturb,
            _ => Self::Offline,
        }
    }
}

/// What the list says about one member besides their name.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Person {
    /// Shared with [`Names`] while it knows them.
    roles: Arc<[String]>,
    presence: Presence,
    activity: Option<String>,
    /// Not saved: a game is over by the next start, and the line above
    /// says enough until Discord sends the presence again.
    #[serde(skip)]
    activities: Activities,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Row {
    Group(String),
    Member(String),
    /// A row Discord hasn't sent.
    Unknown,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MemberList {
    guild: String,
    /// The channel it shows for, of those sharing it.
    channel: String,
    /// Who can read its channels, as Discord's id for the list; see
    /// [`Server::readers`]. Channels with the same readers share a list.
    readers: String,
    /// When Discord last sent anything for it; not saved, so a saved list
    /// is asked for again.
    #[serde(skip)]
    heard: Option<Instant>,
    /// Whether Discord has sent the list yet.
    #[serde(default)]
    synced: bool,
    rows: Vec<Row>,
    /// Each group's size, which may exceed the rows Yapfast asked for.
    counts: HashMap<String, u64>,
    /// How many people can read the channel.
    count: Option<u64>,
    /// The listed members' roles and presence, by user id.
    people: HashMap<String, Person>,
}

impl MemberList {
    pub fn new(guild: String, channel: String, readers: String) -> Self {
        Self {
            guild,
            channel,
            readers,
            heard: None,
            synced: false,
            rows: Vec::new(),
            counts: HashMap::new(),
            count: None,
            people: HashMap::new(),
        }
    }

    pub fn guild(&self) -> &str {
        &self.guild
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Shows the list for another channel with the same readers.
    pub fn show_for(&mut self, channel: &str) {
        channel.clone_into(&mut self.channel);
    }

    pub fn readers(&self) -> &str {
        &self.readers
    }

    /// Whether Discord sent anything for it within `within`.
    pub fn heard_within(&self, within: std::time::Duration) -> bool {
        self.heard.is_some_and(|heard| heard.elapsed() < within)
    }

    /// When it stops being fresh after `within`.
    pub fn stale_at(&self, within: std::time::Duration) -> Option<Instant> {
        self.heard.map(|heard| heard + within)
    }

    /// How many people can read the channel, once Discord has said.
    pub fn count(&self) -> Option<u64> {
        self.count
    }

    /// Whether it lists `user`.
    pub fn lists(&self, user: &str) -> bool {
        self.people.contains_key(user)
    }

    /// Whether Discord has sent the list yet.
    pub fn synced(&self) -> bool {
        self.synced
    }

    /// Applies one GUILD_MEMBER_LIST_UPDATE, learning the members it carries.
    /// Returns whether this list changed. Updates to other lists, such as the
    /// one of a channel shown before, are left out.
    pub fn apply(&mut self, data: &Value, names: &mut Names) -> bool {
        if data["guild_id"] != self.guild.as_str() || data["id"] != self.readers.as_str() {
            return false;
        }
        let ops = data["ops"].as_array().map_or(&[][..], Vec::as_slice);
        // Operations on rows Yapfast doesn't have yet mean nothing.
        self.synced |= ops.iter().any(|op| op["op"] == "SYNC");
        if !self.synced {
            return false;
        }
        if let Some(count) = data["member_count"].as_u64() {
            self.count = Some(count);
        }
        if let Some(groups) = data["groups"].as_array() {
            self.counts = groups
                .iter()
                .filter_map(|group| {
                    Some((group["id"].as_str()?.to_owned(), group["count"].as_u64()?))
                })
                .collect();
        }
        for op in ops {
            let index = op["index"].as_u64().map(|index| index as usize);
            let range = op["range"].as_array().and_then(|range| {
                Some((
                    range.first()?.as_u64()? as usize,
                    range.get(1)?.as_u64()? as usize,
                ))
            });
            match (op["op"].as_str(), index, range) {
                (Some("SYNC"), _, Some((start, end))) => {
                    let items: Vec<Row> = op["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|item| row(&self.guild, item, names, &mut self.people))
                        .collect();
                    // Fewer items than asked for: the list ends there.
                    let ended = items.len() <= end.saturating_sub(start);
                    if self.rows.len() < start {
                        self.rows.resize(start, Row::Unknown);
                    }
                    let after = start + items.len();
                    let tail = if ended || self.rows.len() <= after {
                        Vec::new()
                    } else {
                        self.rows.split_off(after)
                    };
                    self.rows.truncate(start);
                    self.rows.extend(items);
                    self.rows.extend(tail);
                }
                (Some("INSERT"), Some(index), _) => {
                    let row = row(&self.guild, &op["item"], names, &mut self.people);
                    self.rows.insert(index.min(self.rows.len()), row);
                }
                (Some("UPDATE"), Some(index), _) => {
                    let row = row(&self.guild, &op["item"], names, &mut self.people);
                    if let Some(slot) = self.rows.get_mut(index) {
                        *slot = row;
                    }
                }
                (Some("DELETE"), Some(index), _) if index < self.rows.len() => {
                    self.rows.remove(index);
                }
                // INVALIDATE: Discord stops sending these rows, as when Yapfast
                // subscribes to another channel's list. They stay as last
                // sent, shown when the channel is again, until a sync.
                _ => {}
            }
        }
        self.rows.truncate(ROWS);
        self.heard = Some(Instant::now());
        let rows = &self.rows;
        self.people.retain(|user, _| {
            rows.iter()
                .any(|row| matches!(row, Row::Member(listed) if listed == user))
        });
        true
    }

    /// Applies a PRESENCE_UPDATE to a listed member; returns whether it changed.
    pub fn presence(&mut self, data: &Value) -> bool {
        if data["guild_id"] != self.guild.as_str() {
            return false;
        }
        let Some(person) = data["user"]["id"]
            .as_str()
            .and_then(|user| self.people.get_mut(user))
        else {
            return false;
        };
        let presence = Presence::parse(data);
        let activity = activity(&data["activities"]);
        let activities = activities(&data["activities"]);
        let changed = person.presence != presence
            || person.activity != activity
            || person.activities != activities;
        person.presence = presence;
        person.activity = activity;
        person.activities = activities;
        changed
    }

    /// The members in the rows Yapfast has, in Discord's order.
    pub fn members(&self, server: Option<&Server>, names: &Names) -> Vec<ListedMember> {
        let mut members = Vec::new();
        let mut group: Option<(&str, String)> = None;
        for row in &self.rows {
            match row {
                Row::Group(id) => group = Some((id, self.heading(id, server))),
                Row::Member(user) => {
                    let mut person = self.people.get(user).cloned().unwrap_or_default();
                    // GUILD_MEMBER_UPDATE changes them in Names, without
                    // a list operation for the row.
                    if let Some(roles) = names.shared_roles(&self.guild, user) {
                        person.roles = roles;
                    }
                    members.push(ListedMember {
                        user_id: user.clone(),
                        name: names
                            .name(&self.guild, user)
                            .unwrap_or("Discord user")
                            .to_owned(),
                        username: names.username(user).map(str::to_owned),
                        avatar: names.avatar(Some(&self.guild), user),
                        decoration: names.decoration(Some(&self.guild), user),
                        nameplate: names.nameplate(Some(&self.guild), user),
                        style: names.style(Some(&self.guild), user),
                        group: group
                            .as_ref()
                            .map(|(_, heading)| heading.clone())
                            .unwrap_or_default(),
                        group_icon: group
                            .as_ref()
                            .and_then(|(id, _)| server?.group_role_icon(id)),
                        role_icon: server.and_then(|server| server.role_icon(&person.roles)),
                        tag: names.server_tag(user).cloned(),
                        presence: person.presence,
                        color: server.and_then(|server| server.color(&person.roles)),
                        activity: person.activity,
                        activities: person.activities,
                        roles: person.roles,
                        timed_out_until: names.timed_out_until(&self.guild, user),
                    });
                }
                Row::Unknown => {}
            }
        }
        members
    }

    fn heading(&self, id: &str, server: Option<&Server>) -> String {
        let name = match id {
            "online" => "Online",
            "offline" => "Offline",
            role => server
                .and_then(|server| server.role_names.get(role))
                .map_or("Members", String::as_str),
        };
        match self.counts.get(id) {
            Some(count) => format!("{name} — {count}"),
            None => name.to_owned(),
        }
    }
}

/// One row from an item: a group heading, or a member, whose name, roles and
/// presence it learns.
fn row(guild: &str, item: &Value, names: &mut Names, people: &mut HashMap<String, Person>) -> Row {
    if let Some(group) = item["group"]["id"].as_str() {
        return Row::Group(group.to_owned());
    }
    let member = &item["member"];
    let Some(user) = member["user"]["id"].as_str() else {
        return Row::Unknown;
    };
    names.learn_member(guild, member);
    let presence = &member["presence"];
    people.insert(
        user.to_owned(),
        Person {
            roles: names.shared_roles(guild, user).unwrap_or_default(),
            presence: Presence::parse(presence),
            activity: activity(&presence["activities"]),
            activities: activities(&presence["activities"]),
        },
    );
    Row::Member(user.to_owned())
}

/// What Discord shows under a member's name: their custom status, else what
/// they're playing, streaming, listening to, watching or competing in. A
/// custom emoji starts the status in Discord's markup, `<:name:id>`; see
/// [`status_emoji`].
pub fn activity(activities: &Value) -> Option<String> {
    const CUSTOM: u64 = 4;
    if let Some(custom) = activities
        .as_array()?
        .iter()
        .find(|activity| activity["type"] == CUSTOM)
    {
        let emoji = &custom["emoji"];
        let emoji = match (emoji["name"].as_str(), emoji["id"].as_str()) {
            (name, Some(id)) => Some(format!(
                "<{}:{}:{id}>",
                if emoji["animated"] == true { "a" } else { "" },
                name.unwrap_or("emoji"),
            )),
            (Some(name), None) => Some(name.to_owned()),
            (None, None) => None,
        };
        let text: Vec<&str> = [emoji.as_deref(), custom["state"].as_str()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        if !text.is_empty() {
            return Some(text.join(" "));
        }
    }
    crate::activity::activities(activities)
        .iter()
        .find_map(crate::activity::Activity::line)
}

/// The custom emoji an [`activity`] starts with, and the words after it.
pub fn status_emoji(activity: &str) -> Option<(CustomEmoji, &str)> {
    let end = activity.strip_prefix('<')?.find('>')? + 2;
    let emoji = message::custom_emoji(&activity[..end]).pop()?;
    Some((emoji, activity[end..].trim_start()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn member(id: &str, name: &str) -> Value {
        json!({"member": {"user": {"id": id, "username": name}, "roles": [], "nick": null}})
    }

    #[test]
    fn shows_custom_status_before_activities() {
        assert_eq!(
            activity(&json!([
                {"type": 0, "name": "Factorio"},
                {"type": 4, "name": "Custom Status", "state": "brb", "emoji": {"name": "☕", "id": null}},
            ])),
            Some("☕ brb".into())
        );
        assert_eq!(
            activity(&json!([{"type": 4, "state": "hi", "emoji": {"name": "pog", "id": "5"}}])),
            Some("<:pog:5> hi".into())
        );
        assert_eq!(
            activity(
                &json!([{"type": 4, "emoji": {"name": "party", "id": "6", "animated": true}}])
            ),
            Some("<a:party:6>".into()),
            "a custom emoji alone is a status"
        );
        assert_eq!(
            activity(&json!([{"type": 2, "name": "Spotify"}])),
            Some("Listening to Spotify".into())
        );
        assert_eq!(activity(&json!([])), None);
    }

    #[test]
    fn takes_the_custom_emoji_off_a_status() {
        let (emoji, words) = status_emoji("<:pog:5> back soon").expect("an emoji");
        assert_eq!(
            (emoji.name.as_str(), emoji.id.as_str(), words),
            ("pog", "5", "back soon")
        );
        let (emoji, words) = status_emoji("<a:party:6>").expect("an emoji");
        assert!(emoji.animated);
        assert_eq!(words, "");
        assert_eq!(status_emoji("☕ brb"), None);
        assert_eq!(status_emoji("<3 you"), None);
    }

    #[test]
    fn marks_who_is_online_only_on_a_phone() {
        let parse = |status: &str, clients: Value| {
            Presence::parse(&json!({"status": status, "client_status": clients}))
        };
        assert_eq!(
            parse("online", json!({"mobile": "online"})),
            Presence::Mobile
        );
        assert_eq!(
            parse("online", json!({"mobile": "online", "desktop": "idle"})),
            Presence::Mobile,
            "the phone keeps them online"
        );
        assert_eq!(
            parse("online", json!({"mobile": "online", "desktop": "online"})),
            Presence::Online
        );
        assert_eq!(
            parse("online", json!({"mobile": "online", "web": "online"})),
            Presence::Online
        );
        assert_eq!(parse("idle", json!({"mobile": "idle"})), Presence::Idle);
        assert_eq!(
            Presence::parse(&json!({"status": "online"})),
            Presence::Online
        );
    }

    #[test]
    fn follows_syncs_and_row_operations() {
        let mut names = Names::default();
        let mut list = MemberList::new("10".into(), "11".into(), "everyone".into());
        let server = Server {
            id: "10".into(),
            role_names: HashMap::from([("20".into(), "Mods".into())]).into(),
            role_colors: HashMap::from([("20".into(), (1, 0x3498db))]).into(),
            ..Default::default()
        };
        assert!(
            !list.apply(
                &json!({"guild_id": "10", "id": "123", "ops": [{"op": "SYNC", "range": [0, 99], "items": [member("9", "x")]}]}),
                &mut names,
            ),
            "another channel's list, as one shown before, is left out"
        );
        assert!(
            !list.apply(
                &json!({"guild_id": "10", "id": "everyone", "ops": [{"op": "UPDATE", "index": 0, "item": member("9", "x")}]}),
                &mut names,
            ),
            "an update before the sync means nothing"
        );
        assert!(!list.synced());
        assert!(list.apply(
            &json!({
                "guild_id": "10", "id": "everyone", "member_count": 3,
                "groups": [{"id": "20", "count": 1}, {"id": "online", "count": 1}, {"id": "offline", "count": 1}],
                "ops": [{"op": "SYNC", "range": [0, 99], "items": [
                    {"group": {"id": "20", "count": 1}},
                    {"member": {"user": {"id": "1", "username": "ann"}, "roles": ["20"],
                                "presence": {"status": "dnd", "activities": [{"type": 0, "name": "Chess"}]}}},
                    {"group": {"id": "online", "count": 1}}, member("2", "bob"),
                    {"group": {"id": "offline", "count": 1}}, member("3", "cat"),
                ]}],
            }),
            &mut names,
        ));
        assert_eq!(list.count(), Some(3));
        let members = list.members(Some(&server), &names);
        let shown: Vec<_> = members
            .iter()
            .map(|member| (member.name.as_str(), member.group.as_str(), member.presence))
            .collect();
        assert_eq!(
            shown,
            [
                ("ann", "Mods — 1", Presence::DoNotDisturb),
                ("bob", "Online — 1", Presence::Offline),
                ("cat", "Offline — 1", Presence::Offline)
            ]
        );
        assert_eq!(members[0].color, Some(0x3498db));
        assert_eq!(members[0].activity.as_deref(), Some("Playing Chess"));
        assert_eq!(members[1].color, None);
        assert!(list.presence(
            &json!({"guild_id": "10", "user": {"id": "2"}, "status": "idle", "activities": []})
        ));
        assert_eq!(
            list.members(Some(&server), &names)[1].presence,
            Presence::Idle
        );
        assert!(members[0].avatar.contains("/embed/avatars/"));

        // Cat comes online, above Bob; the empty offline group goes.
        assert!(list.apply(
            &json!({
                "guild_id": "10", "id": "everyone",
                "groups": [{"id": "20", "count": 1}, {"id": "online", "count": 2}],
                "ops": [
                    {"op": "DELETE", "index": 5},
                    {"op": "DELETE", "index": 4},
                    {"op": "INSERT", "index": 3, "item": member("3", "cat")},
                    {"op": "UPDATE", "index": 2, "item": {"group": {"id": "online", "count": 2}}},
                ],
            }),
            &mut names,
        ));
        let names_shown: Vec<_> = list
            .members(Some(&server), &names)
            .into_iter()
            .map(|member| (member.name, member.group))
            .collect();
        assert_eq!(
            names_shown,
            [
                ("ann".to_owned(), "Mods — 1".to_owned()),
                ("cat".to_owned(), "Online — 2".to_owned()),
                ("bob".to_owned(), "Online — 2".to_owned())
            ]
        );
        // Yapfast subscribes to another channel's list: Discord stops sending
        // this one, whose rows stay to show it again.
        assert!(list.apply(
            &json!({"guild_id": "10", "id": "everyone", "ops": [{"op": "INVALIDATE", "range": [0, 99]}]}),
            &mut names,
        ));
        let kept = list.members(Some(&server), &names);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[1].group, "Online — 2", "the groups' sizes stay");
    }
}
