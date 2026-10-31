//! Authorizing a Discord application from its link: adding a bot to a
//! server, installing an app, or letting a site sign someone in.
//! Discord's client previews an authorization with
//! `GET /oauth2/authorize` and makes it with `POST /oauth2/authorize`,
//! both with the link's query; see
//! https://docs.discord.food/topics/oauth2#preview-oauth2-authorization.

use serde_json::Value;

use crate::ready;

/// What an authorization link asks for, read from its query. The query
/// itself is kept as the link wrote it, so previewing and authorizing
/// ask Discord for exactly what the link asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The link's query, as it was written.
    pub query: String,
    /// The application being authorized, by its id.
    pub client: String,
    /// The scopes it asks for, such as `bot` and `identify`.
    pub scopes: Vec<String>,
    /// How Discord answers, such as `code`; a plain bot link has none.
    pub response_type: Option<String>,
    /// Where Discord sends the person afterwards, when the link names
    /// one of the application's addresses.
    pub redirect_uri: Option<String>,
    /// The permissions a bot asks for in the server it's added to.
    pub permissions: Option<u64>,
    /// The server the link preselects, and whether it allows another.
    pub guild: Option<String>,
    pub guild_locked: bool,
    /// The application installs to the account rather than a server.
    pub user_install: bool,
}

impl Request {
    /// The request in an authorization link's query; none without an
    /// application it names by a snowflake.
    pub fn parse(query: &str) -> Option<Self> {
        let fields: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
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
        Some(Self {
            query: query.to_owned(),
            client,
            scopes: field("scope")
                .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
            response_type: field("response_type"),
            redirect_uri: field("redirect_uri"),
            permissions: field("permissions").and_then(|bits| bits.parse().ok()),
            guild: field("guild_id").filter(|guild| snowflake(guild)),
            guild_locked: field("disable_guild_select").as_deref() == Some("true"),
            user_install: field("integration_type").as_deref() == Some("1"),
        })
    }

    /// Whether the link adds a bot to a server.
    pub fn bot(&self) -> bool {
        self.scopes.iter().any(|scope| scope == "bot")
    }

    /// Whether authorizing picks a server: installing a bot, an app's
    /// commands or a webhook to one, rather than to the account.
    pub fn guild_install(&self) -> bool {
        !self.user_install
            && self.scopes.iter().any(|scope| {
                matches!(
                    scope.as_str(),
                    "bot" | "applications.commands" | "webhook.incoming"
                )
            })
    }
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
