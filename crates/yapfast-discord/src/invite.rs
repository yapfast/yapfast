//! Invites to a server: the codes in links people send or paste, what an
//! invite shows before joining, and what joining by one answers. Discord's
//! client resolves an invite with `GET /invites/{code}` and joins with
//! `POST /invites/{code}`; see https://docs.discord.food/resources/invite.

use serde_json::{Value, json};

use crate::ready;

/// The hosts whose `/invite/{code}` links Discord's client takes as
/// invites, besides discord.gg's `/{code}`.
const INVITE_HOSTS: [&str; 5] = [
    "discord.com",
    "discordapp.com",
    "canary.discord.com",
    "ptb.discord.com",
    "canary.discordapp.com",
];

/// The code of an invite link in a message, as Discord's client finds
/// them: `discord.gg/{code}`, or `/invite/{code}` on discord.com and
/// discordapp.com, over http or https, with or without `www.`. A query
/// such as `?event=` is left off, as joining leaves it off.
pub fn link_code(link: &str) -> Option<&str> {
    let rest = link
        .strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    let path = if host.eq_ignore_ascii_case("discord.gg") {
        path
    } else if INVITE_HOSTS
        .iter()
        .any(|known| host.eq_ignore_ascii_case(known))
    {
        path.strip_prefix("invite/")?
    } else {
        return None;
    };
    let code = path.split(['?', '#']).next()?;
    // Discord's client allows one `/` or `.` after the code.
    let code = code
        .strip_suffix('/')
        .or_else(|| code.strip_suffix('.'))
        .unwrap_or(code);
    valid(code).then_some(code)
}

/// The code in what someone typed or pasted into Join a Server: a link, as
/// [`link_code`] reads it, or a bare code such as a server's vanity one.
/// Discord's dialog takes the last part after a `/`.
pub fn typed_code(typed: &str) -> Option<&str> {
    let typed = typed.trim();
    if let Some(code) = link_code(typed) {
        return Some(code);
    }
    let code = typed.rsplit('/').next()?;
    let code = code.split(['?', '#']).next()?;
    valid(code).then_some(code)
}

/// Discord's invite codes and vanity codes are letters, digits and `-`.
pub(crate) fn valid(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// What an invite shows before joining, from `GET /invites/{code}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub code: String,
    pub guild: String,
    pub name: String,
    /// The address of the server's icon, if it has one.
    pub icon: Option<String>,
    /// The channel joining lands in, its name and its type.
    pub channel: String,
    pub channel_name: String,
    pub channel_type: u64,
    /// How many members the server has, and how many are online, when
    /// Discord counts them.
    pub members: Option<u64>,
    pub online: Option<u64>,
}

impl Preview {
    /// The preview of an invite object; none for one that isn't to a
    /// server's channel, such as a friend or group DM invite, which
    /// Yapfast doesn't join.
    pub fn of(invite: &Value) -> Option<Self> {
        let guild = &invite["guild"];
        let channel = &invite["channel"];
        let id = guild["id"].as_str()?;
        Some(Self {
            code: invite["code"].as_str()?.to_owned(),
            guild: id.to_owned(),
            name: guild["name"].as_str().unwrap_or_default().to_owned(),
            icon: guild["icon"]
                .as_str()
                .map(|hash| ready::server_icon(id, hash)),
            channel: channel["id"].as_str()?.to_owned(),
            channel_name: channel["name"].as_str().unwrap_or_default().to_owned(),
            channel_type: channel["type"].as_u64().unwrap_or_default(),
            members: invite["approximate_member_count"].as_u64(),
            online: invite["approximate_presence_count"].as_u64(),
        })
    }

    /// Whether joining lands in a voice or stage channel.
    pub fn voice(&self) -> bool {
        matches!(self.channel_type, VOICE | STAGE)
    }

    /// The X-Context-Properties of joining by this invite from `from`, as
    /// Discord's client sends them: where it was joined, and the server
    /// and channel the invite is to.
    pub fn context(&self, from: JoinFrom) -> Value {
        json!({
            "location": from.location(),
            "location_guild_id": self.guild,
            "location_channel_id": self.channel,
            "location_channel_type": self.channel_type,
        })
    }
}

const VOICE: u64 = 2;
const STAGE: u64 = 13;

/// Where someone joined a server by an invite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinFrom {
    /// Join a Server, after typing or pasting the invite.
    Dialog,
    /// The dialog a clicked invite link opens.
    Link,
}

impl JoinFrom {
    fn location(self) -> &'static str {
        match self {
            Self::Dialog => "Join Guild",
            Self::Link => "Desktop Invite Modal",
        }
    }
}

/// What joining by an invite answered, from `POST /invites/{code}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Joined {
    pub guild: String,
    pub channel: String,
    /// Whether the server asks new members to fill in its form, such as
    /// accepting its rules, before they may talk.
    pub verification_form: bool,
}

impl Joined {
    pub fn of(answer: &Value, preview: &Preview) -> Self {
        let text = |value: &Value, or: &str| value.as_str().unwrap_or(or).to_owned();
        Self {
            guild: text(&answer["guild"]["id"], &preview.guild),
            channel: text(&answer["channel"]["id"], &preview.channel),
            verification_form: answer["show_verification_form"] == true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_codes_in_invite_links_as_discords_client_does() {
        assert_eq!(
            link_code("https://discord.gg/Q3fUExJPy5"),
            Some("Q3fUExJPy5")
        );
        assert_eq!(link_code("http://www.discord.gg/abc-123/"), Some("abc-123"));
        assert_eq!(
            link_code("https://discord.com/invite/yapfast"),
            Some("yapfast")
        );
        assert_eq!(
            link_code("https://ptb.discord.com/invite/abc."),
            Some("abc")
        );
        assert_eq!(
            link_code("https://discordapp.com/invite/abc?event=1"),
            Some("abc")
        );
        assert_eq!(link_code("https://discord.gg/abc#top"), Some("abc"));
        assert_eq!(link_code("https://discord.com/channels/1/2"), None);
        assert_eq!(link_code("https://discord.gg/"), None);
        assert_eq!(link_code("https://discord.gg/a/b"), None);
        assert_eq!(link_code("https://example.com/invite/abc"), None);
        assert_eq!(link_code("https://discord.gg.example.com/abc"), None);
        assert_eq!(link_code("discord.gg/abc"), None);
    }

    #[test]
    fn takes_a_typed_link_or_code() {
        assert_eq!(typed_code("  https://discord.gg/abc \n"), Some("abc"));
        assert_eq!(typed_code("abc-123"), Some("abc-123"));
        assert_eq!(typed_code("discord.gg/abc"), Some("abc"));
        assert_eq!(
            typed_code("https://discord.com/invite/abc?event=2"),
            Some("abc")
        );
        assert_eq!(typed_code(""), None);
        assert_eq!(typed_code("two words"), None);
        assert_eq!(typed_code("https://discord.gg/"), None);
    }

    #[test]
    fn previews_an_invite_to_a_servers_channel() {
        let invite = json!({
            "code": "abc",
            "guild": {"id": "10", "name": "Builders", "icon": "hash"},
            "channel": {"id": "11", "name": "general", "type": 0},
            "approximate_member_count": 42,
            "approximate_presence_count": 7,
        });
        let preview = Preview::of(&invite).unwrap();
        assert_eq!(preview.name, "Builders");
        assert!(
            preview
                .icon
                .as_ref()
                .unwrap()
                .contains("/icons/10/hash.png")
        );
        assert_eq!((preview.members, preview.online), (Some(42), Some(7)));
        assert!(!preview.voice());
        // A friend invite has no server.
        assert_eq!(Preview::of(&json!({"code": "f", "type": 2})), None);
    }

    #[test]
    fn joining_names_the_invites_server_and_channel() {
        let invite = json!({
            "code": "abc",
            "guild": {"id": "10", "name": "Builders"},
            "channel": {"id": "11", "name": "voice", "type": 2},
        });
        let preview = Preview::of(&invite).unwrap();
        assert!(preview.voice());
        assert_eq!(
            preview.context(JoinFrom::Dialog),
            json!({
                "location": "Join Guild",
                "location_guild_id": "10",
                "location_channel_id": "11",
                "location_channel_type": 2,
            })
        );
        assert_eq!(
            preview.context(JoinFrom::Link)["location"],
            "Desktop Invite Modal"
        );
        let joined = Joined::of(&json!({"show_verification_form": true}), &preview);
        assert_eq!(
            (joined.guild.as_str(), joined.channel.as_str()),
            ("10", "11")
        );
        assert!(joined.verification_form);
    }
}
