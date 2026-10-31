//! Authorizing a Discord application from its link: adding a bot to a
//! server, installing an app, or letting a site sign someone in.
//! Discord's client previews an authorization with
//! `GET /oauth2/authorize` and makes it with `POST /oauth2/authorize`,
//! each with a query it builds from the fields it read from the link; see
//! https://docs.discord.food/topics/oauth2#preview-oauth2-authorization.

use serde_json::Value;
use url::form_urlencoded;

use crate::ready;

/// What an authorization link asks for, read from its query. Previewing
/// and authorizing send only these fields, as Discord's client rebuilds
/// its queries, so Discord is asked for exactly what the dialog shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The application being authorized, by its id.
    pub client: String,
    /// The scopes it asks for, such as `bot` and `identify`.
    pub scopes: Vec<String>,
    /// How Discord answers, such as `code`; a plain bot link has none.
    pub response_type: Option<String>,
    /// Where Discord sends the person afterwards, when the link names
    /// one of the application's addresses.
    pub redirect_uri: Option<String>,
    /// What the application passes through the flow untouched: its PKCE
    /// challenge and method, its state and its OpenID nonce.
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub state: Option<String>,
    pub nonce: Option<String>,
    /// The permissions a bot asks for in the server it's added to; only
    /// with the `bot` scope.
    pub permissions: Option<u64>,
    /// The server the link preselects, and whether it allows another.
    pub guild: Option<String>,
    pub guild_locked: bool,
    /// The application installs to the account rather than a server.
    pub user_install: bool,
}

/// The fields of a link that Yapfast reads. A link naming one twice could
/// show one value and send another, so it isn't taken.
const FIELDS: [&str; 12] = [
    "client_id",
    "scope",
    "response_type",
    "redirect_uri",
    "code_challenge",
    "code_challenge_method",
    "state",
    "nonce",
    "permissions",
    "guild_id",
    "disable_guild_select",
    "integration_type",
];

impl Request {
    /// The request in an authorization link's query; none without an
    /// application it names by a snowflake, or when the browser should
    /// take it: a link without scopes (an install link, whose scopes
    /// Discord's page reads from the application's settings), one asking
    /// for a webhook (which needs a channel picked), one with a scope
    /// [`scope_wants`] can't put in words, or one naming a field twice.
    pub fn parse(query: &str) -> Option<Self> {
        let fields: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
            .filter(|(name, _)| FIELDS.contains(&name.as_ref()))
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let twice = fields
            .iter()
            .enumerate()
            .any(|(at, (name, _))| fields[..at].iter().any(|(before, _)| before == name));
        if twice {
            return None;
        }
        let field = |name: &str| {
            fields
                .iter()
                .find(|(found, _)| found == name)
                .map(|(_, value)| value.clone())
        };
        let client = field("client_id")?;
        if !snowflake(&client) {
            return None;
        }
        let scopes: Vec<String> = field("scope")?
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if scopes.is_empty() || !scopes.iter().all(|scope| scope_wants(scope).is_some()) {
            return None;
        }
        let bot = scopes.iter().any(|scope| scope == "bot");
        let permissions = match field("permissions").filter(|bits| !bits.is_empty()) {
            Some(bits) => bot.then_some(bits.parse::<u64>().ok()?),
            None => None,
        };
        let guild = match field("guild_id") {
            Some(guild) if snowflake(&guild) => Some(guild),
            Some(_) => return None,
            None => None,
        };
        let user_install = match field("integration_type").as_deref() {
            None | Some("0") => false,
            Some("1") => true,
            Some(_) => return None,
        };
        Some(Self {
            client,
            scopes,
            response_type: field("response_type"),
            redirect_uri: field("redirect_uri"),
            code_challenge: field("code_challenge"),
            code_challenge_method: field("code_challenge_method"),
            state: field("state"),
            nonce: field("nonce"),
            permissions,
            guild_locked: guild.is_some()
                && field("disable_guild_select").as_deref() == Some("true"),
            guild,
            user_install,
        })
    }

    /// The query of `GET /oauth2/authorize`, as Discord's client builds it.
    pub fn preview_query(&self) -> String {
        self.query(true)
    }

    /// The query of `POST /oauth2/authorize`, which carries the
    /// integration type in its body instead.
    pub fn authorize_query(&self) -> String {
        self.query(false)
    }

    fn query(&self, integration_type: bool) -> String {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("client_id", &self.client);
        let optional = [
            ("response_type", &self.response_type),
            ("redirect_uri", &self.redirect_uri),
            ("code_challenge", &self.code_challenge),
            ("code_challenge_method", &self.code_challenge_method),
        ];
        for (name, value) in optional {
            if let Some(value) = value {
                query.append_pair(name, value);
            }
        }
        query.append_pair("scope", &self.scopes.join(" "));
        for (name, value) in [("state", &self.state), ("nonce", &self.nonce)] {
            if let Some(value) = value {
                query.append_pair(name, value);
            }
        }
        if integration_type {
            query.append_pair(
                "integration_type",
                if self.user_install { "1" } else { "0" },
            );
        }
        query.finish()
    }

    /// Whether the link adds a bot to a server.
    pub fn bot(&self) -> bool {
        self.scopes.iter().any(|scope| scope == "bot")
    }

    /// Whether authorizing picks a server: installing a bot or an app's
    /// commands to one, rather than to the account.
    pub fn guild_install(&self) -> bool {
        !self.user_install
            && self
                .scopes
                .iter()
                .any(|scope| matches!(scope.as_str(), "bot" | "applications.commands"))
    }

    /// Whether the link asks to control the person's Discord client
    /// through RPC, which reaches nearly everything the account can do.
    pub fn controls_client(&self) -> bool {
        self.scopes.iter().any(|scope| scope == "rpc")
    }
}

/// What a scope lets an application do, in the words of Discord's
/// authorization page; none for a scope Yapfast leaves to that page, such
/// as `webhook.incoming`, which needs a channel picked.
pub fn scope_wants(scope: &str) -> Option<&'static str> {
    Some(match scope {
        "bot" => "Add a bot to a server",
        "applications.commands" => "Create commands",
        "applications.commands.update" => "Update commands",
        "identify" => "Access your username, avatar, and banner",
        "identify.premium" => "Access your Nitro status",
        "openid" => "Access your profile information",
        "email" => "Access your email address",
        "connections" => "Access your third-party connections",
        "role_connections.write" => "Update your connection and metadata for this application",
        "guilds" => "Know what servers you're in",
        "guilds.join" => "Join servers for you",
        "guilds.members.read" => {
            "Read your member info (nickname, avatar, roles, etc...) for servers you belong to"
        }
        "guilds.channels.read" => {
            "Read the channel list for servers you belong to and some channel info (name, type, sort order, etc...)"
        }
        "gdm.join" => "Join group DMs for you",
        "messages.read" => "Read all messages",
        "dm_channels.read" => "See information about the DMs and GDMs you are in",
        "dm_channels.messages.read" => "Read your direct messages",
        "dm_channels.messages.write" => "Send messages in your DMs",
        "relationships.read" => "See your friends, pending friend requests, and blocked users",
        "relationships.write" => "Manage your friends and blocked users",
        "presences.read" => "See who is online",
        "presences.write" => "Set your online status and current activity",
        "activities.read" => "Access your Active Now activities",
        "activities.write" => "Update your current activity",
        "activities.invites.write" => "Send activity invites",
        "voice" => {
            "Connect to voice on your behalf and see others who are in any voice channel or call"
        }
        "gateway.connect" => "Start a gateway session on your behalf",
        "rpc" => "Interface with your Discord client",
        "rpc.notifications.read" => "Listen to notifications over your Discord client",
        "rpc.voice.read" => "Read your voice settings and listen for voice events",
        "rpc.voice.write" => "Alter your voice settings and use soundboard",
        "rpc.video.read" => "Read your video state",
        "rpc.video.write" => "Alter your video settings and start video",
        "rpc.screenshare.read" => "Read your screenshare state",
        "rpc.screenshare.write" => "Alter your screenshare settings and start screenshare",
        "rpc.activities.write" => "Set your current activity",
        _ => return None,
    })
}

/// A server an application could be added to, from the preview's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guild {
    pub id: String,
    pub name: String,
    /// The address of the server's icon, if it has one.
    pub icon: Option<String>,
    /// This account may add applications to the server.
    pub can_add: bool,
}

/// What an authorization shows before it's made, from
/// `GET /oauth2/authorize`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The application, what it says about itself, and its icon's
    /// address, when it has one.
    pub name: String,
    pub about: Option<String>,
    pub icon: Option<String>,
    /// The bot the link would add, by the name Discord shows for it,
    /// when it adds one.
    pub bot: Option<String>,
    /// This account authorized the application for these scopes before.
    pub authorized: bool,
    /// Where Discord sends the person after authorizing, when the flow
    /// answers with a redirect.
    pub redirect_uri: Option<String>,
    /// The servers it could be added to, for a server install.
    pub guilds: Vec<Guild>,
}

impl Preview {
    /// The preview of an authorization answer, for `request`.
    pub fn of(answer: &Value, request: &Request) -> Option<Self> {
        let application = &answer["application"];
        let name = application["name"].as_str()?;
        let bot = &answer["bot"];
        Some(Self {
            name: name.to_owned(),
            about: application["description"].as_str().map(str::to_owned),
            icon: application["icon"]
                .as_str()
                .map(|hash| ready::application_icon(&request.client, hash)),
            bot: bot["global_name"]
                .as_str()
                .or_else(|| bot["username"].as_str())
                .map(str::to_owned),
            authorized: answer["authorized"] == true,
            redirect_uri: answer["redirect_uri"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| request.redirect_uri.clone()),
            guilds: answer["guilds"]
                .as_array()
                .map(|guilds| guilds.iter().filter_map(Guild::of).collect())
                .unwrap_or_default(),
        })
    }
}

impl Guild {
    fn of(guild: &Value) -> Option<Self> {
        use ready::permission::{ADMINISTRATOR, MANAGE_GUILD};
        let id = guild["id"].as_str()?;
        let permissions: u64 = guild["permissions"]
            .as_str()
            .and_then(|bits| bits.parse().ok())
            .unwrap_or_default();
        Some(Self {
            id: id.to_owned(),
            name: guild["name"].as_str().unwrap_or_default().to_owned(),
            icon: guild["icon"]
                .as_str()
                .map(|hash| ready::server_icon(id, hash)),
            can_add: permissions & (ADMINISTRATOR | MANAGE_GUILD) != 0,
        })
    }
}

/// A Discord snowflake, as [`crate::link`] reads one.
fn snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_what_an_authorization_link_asks_for() {
        let request = Request::parse(
            "client_id=157730590492196864&scope=bot%20applications.commands&permissions=2048&guild_id=10&disable_guild_select=true",
        )
        .unwrap();
        assert_eq!(request.client, "157730590492196864");
        assert_eq!(request.scopes, ["bot", "applications.commands"]);
        assert_eq!(request.permissions, Some(2048));
        assert_eq!(request.guild.as_deref(), Some("10"));
        assert!(request.guild_locked && request.bot() && request.guild_install());

        let sign_in = Request::parse(
            "response_type=code&client_id=157730590492196864&scope=identify&redirect_uri=https%3A%2F%2Fnicememe.website",
        )
        .unwrap();
        assert!(!sign_in.bot() && !sign_in.guild_install());
        assert_eq!(
            sign_in.redirect_uri.as_deref(),
            Some("https://nicememe.website")
        );

        assert!(Request::parse("scope=bot").is_none());
        assert!(Request::parse("client_id=not-an-app&scope=bot").is_none());
    }

    #[test]
    fn leaves_to_the_browser_what_it_cant_show_as_sent() {
        let app = "client_id=157730590492196864";
        for query in [
            // An install link: Discord's page reads the scopes from the
            // application's settings.
            app.to_owned(),
            format!("{app}&scope="),
            // A webhook needs a channel picked.
            format!("{app}&scope=webhook.incoming"),
            // A scope Yapfast can't put in words.
            format!("{app}&scope=identify%20applications.store.update"),
            // A field named twice shows one value and could send both.
            format!("{app}&scope=identify&scope=bot%20guilds.join"),
            format!("{app}&client_id=10&scope=identify"),
            format!("{app}&scope=bot&permissions=8&permissions=2048"),
            format!(
                "{app}&scope=identify&redirect_uri=https%3A%2F%2Fa&redirect_uri=myapp%3A%2F%2Fb"
            ),
            // Values Discord would read otherwise than shown.
            format!("{app}&scope=bot&permissions=lots"),
            format!("{app}&scope=bot&guild_id=@me"),
            format!("{app}&scope=applications.commands&integration_type=2"),
        ] {
            assert_eq!(Request::parse(&query), None, "{query}");
        }
        // A field it doesn't read may come twice; it isn't sent.
        assert!(Request::parse(&format!("{app}&scope=bot&utm=a&utm=b")).is_some());
    }

    #[test]
    fn sends_only_what_it_read() {
        let request = Request::parse(
            "client_id=157730590492196864&scope=identify+guilds&response_type=code&redirect_uri=https%3A%2F%2Fnicememe.website%2Fcb&state=x%26y&code_challenge=abc&code_challenge_method=S256&nonce=n&prompt=none&extra=1",
        )
        .unwrap();
        assert_eq!(
            request.preview_query(),
            "client_id=157730590492196864&response_type=code&redirect_uri=https%3A%2F%2Fnicememe.website%2Fcb&code_challenge=abc&code_challenge_method=S256&scope=identify+guilds&state=x%26y&nonce=n&integration_type=0"
        );
        assert_eq!(
            request.authorize_query(),
            "client_id=157730590492196864&response_type=code&redirect_uri=https%3A%2F%2Fnicememe.website%2Fcb&code_challenge=abc&code_challenge_method=S256&scope=identify+guilds&state=x%26y&nonce=n"
        );
        // A bot's permissions and server go in the body, and only a bot
        // asks for permissions.
        let bot =
            Request::parse("client_id=157730590492196864&scope=bot&permissions=8&guild_id=10")
                .unwrap();
        assert_eq!(
            bot.authorize_query(),
            "client_id=157730590492196864&scope=bot"
        );
        assert_eq!(bot.permissions, Some(8));
        assert!(!bot.guild_locked);
        let commands = Request::parse(
            "client_id=157730590492196864&scope=applications.commands&permissions=8",
        )
        .unwrap();
        assert_eq!(commands.permissions, None);
        // A lock without a server locks nothing.
        let unlocked =
            Request::parse("client_id=157730590492196864&scope=bot&disable_guild_select=true")
                .unwrap();
        assert!(!unlocked.guild_locked);
    }

    #[test]
    fn reads_what_an_authorization_shows_first() {
        let request =
            Request::parse("client_id=157730590492196864&scope=bot&permissions=2048").unwrap();
        let preview = Preview::of(
            &json!({
                "application": {"id": "157730590492196864", "name": "Alien App", "description": "Beams messages up.", "icon": null},
                "bot": {"id": "157730590492196864", "username": "alienapp", "global_name": "Alien App"},
                "authorized": false,
                "guilds": [
                    {"id": "10", "name": "Server", "icon": null, "mfa_level": 0, "permissions": "32"},
                    {"id": "20", "name": "Elsewhere", "icon": null, "mfa_level": 0, "permissions": "1048576"},
                ],
            }),
            &request,
        )
        .unwrap();
        assert_eq!(preview.name, "Alien App");
        assert_eq!(preview.bot.as_deref(), Some("Alien App"));
        assert!(!preview.authorized);
        assert!(preview.guilds[0].can_add);
        assert!(!preview.guilds[1].can_add);
    }
}
