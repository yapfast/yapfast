//! Discord's own links that Yapfast can open without a browser.

use url::Url;

/// Discord's web hosts besides `discord.gg` whose channel, invite and
/// profile paths name the same places in every client. A subdomain or a
/// look-alike host is not Discord for this purpose.
const WEB_HOSTS: [&str; 5] = [
    "discord.com",
    "discordapp.com",
    "canary.discord.com",
    "ptb.discord.com",
    "canary.discordapp.com",
];

/// A place in Discord named by one of Discord's links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// A server invite, by its code.
    Invite { code: String },
    /// Authorizing an application, by its link's query.
    Authorize { query: String },
    /// A gift, such as Nitro, by its code.
    Gift { code: String },
    /// A server's scheduled event, by the server and the event. A link to
    /// one occurrence of a recurring event names the whole series.
    Event { guild: String, event: String },
    /// Direct messages.
    DirectMessages,
    /// A server, without naming a channel.
    Server { guild: String },
    /// A server channel or DM; `@me` is the guild of a DM.
    Channel { guild: String, channel: String },
    /// A message in a server channel or DM.
    Message {
        guild: String,
        channel: String,
        message: String,
    },
    /// A person's profile.
    User { user: String },
}

/// The route a Discord link names, when Yapfast can open it itself.
///
/// This takes Discord's web links, `discord://-/...` app links, and the
/// same paths under `yapfast://` for a caller handing a link to Yapfast.
/// It deliberately leaves pages Yapfast has no screen for, such as the
/// shop and server templates, for their own handling or the browser.
pub fn parse(link: &str) -> Option<Route> {
    if link.chars().any(char::is_control) {
        return None;
    }
    let url = Url::parse(link).ok()?;
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return None;
    }
    let mut segments = segments(&url)?;
    if matches!(
        segments.as_slice(),
        ["oauth2", "authorize"] | ["api", "oauth2", "authorize"]
    ) && matches!(url.scheme(), "http" | "https" | "discord" | "yapfast")
    {
        if matches!(url.scheme(), "http" | "https") && !web_host(&url) {
            return None;
        }
        let query = url.query()?;
        return crate::oauth::Request::parse(query).map(|_| Route::Authorize {
            query: query.to_owned(),
        });
    }
    match url.scheme() {
        "http" | "https" => {
            let host = url.host_str()?;
            let host = host.strip_prefix("www.").unwrap_or(host);
            if host == "discord.gg" {
                return match segments.as_slice() {
                    [code] => invite(code),
                    _ => None,
                };
            }
            if host == "discord.gift" {
                return match segments.as_slice() {
                    [code] => gift(code),
                    _ => None,
                };
            }
            if !web_host(&url) {
                return None;
            }
            from_segments(&segments)
        }
        "discord" | "yapfast" => {
            match url.host_str() {
                Some("-") | None => {}
                Some(host) => segments.insert(0, host),
            }
            from_segments(&segments)
        }
        _ => None,
    }
}

/// Whether `link` is a web page on one of Discord's own hosts, such as
/// where Discord sends someone after authorizing an application.
pub fn on_discord(link: &str) -> bool {
    Url::parse(link).is_ok_and(|url| matches!(url.scheme(), "http" | "https") && web_host(&url))
}

/// Whether a web link's host is one of [`WEB_HOSTS`], with or without
/// `www.`.
fn web_host(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|host| WEB_HOSTS.contains(&host.strip_prefix("www.").unwrap_or(host)))
}

/// A link's path parts, with one trailing slash left off. An empty part
/// anywhere else makes it no route of Discord's.
fn segments(url: &Url) -> Option<Vec<&str>> {
    let mut segments: Vec<_> = url.path_segments()?.collect();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    (!segments.iter().any(|segment| segment.is_empty())).then_some(segments)
}

fn from_segments(segments: &[&str]) -> Option<Route> {
    match segments {
        ["invite", code] => invite(code),
        ["gifts", code] => gift(code),
        // Discord's client adds the occurrence of a recurring event as a
        // third id; Yapfast shows the series.
        ["events", guild, event, occurrence @ ..]
            if snowflake(guild)
                && snowflake(event)
                && occurrence.len() <= 1
                && occurrence.iter().all(|id| snowflake(id)) =>
        {
            Some(Route::Event {
                guild: (*guild).to_owned(),
                event: (*event).to_owned(),
            })
        }
        ["channels", "@me"] => Some(Route::DirectMessages),
        ["channels", guild] if snowflake(guild) => Some(Route::Server {
            guild: (*guild).to_owned(),
        }),
        ["channels", guild, channel] if place(guild) && snowflake(channel) => {
            Some(Route::Channel {
                guild: (*guild).to_owned(),
                channel: (*channel).to_owned(),
            })
        }
        ["channels", guild, channel, message]
            if place(guild) && snowflake(channel) && snowflake(message) =>
        {
            Some(Route::Message {
                guild: (*guild).to_owned(),
                channel: (*channel).to_owned(),
                message: (*message).to_owned(),
            })
        }
        ["users", user] if snowflake(user) => Some(Route::User {
            user: (*user).to_owned(),
        }),
        _ => None,
    }
}

fn invite(code: &str) -> Option<Route> {
    let code = code.strip_suffix('.').unwrap_or(code);
    crate::invite::valid(code).then(|| Route::Invite {
        code: code.to_owned(),
    })
}

fn gift(code: &str) -> Option<Route> {
    let code = code.strip_suffix('.').unwrap_or(code);
    crate::gift::code(code).map(|code| Route::Gift { code })
}

/// The guild part of a channel link: a server's snowflake, or `@me` for
/// a DM. Discord's other pseudo-guilds name screens Yapfast has no
/// equivalent for.
fn place(guild: &str) -> bool {
    guild == "@me" || snowflake(guild)
}

/// A Discord snowflake: decimal digits that fit in the 64 bits Discord
/// gives them. This accepts the short ids of tests as readily as today's
/// nineteen-digit ones.
fn snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_places_discords_links_name() {
        assert_eq!(
            parse("https://discord.com/channels/10/11/12"),
            Some(Route::Message {
                guild: "10".into(),
                channel: "11".into(),
                message: "12".into(),
            })
        );
        assert_eq!(
            parse("https://canary.discordapp.com/channels/10/11?s=1"),
            Some(Route::Channel {
                guild: "10".into(),
                channel: "11".into(),
            })
        );
        assert_eq!(
            parse("HTTPS://WWW.DISCORD.COM/channels/10/"),
            Some(Route::Server { guild: "10".into() })
        );
        assert_eq!(
            parse("discord://-/channels/@me/20/21#linked"),
            Some(Route::Message {
                guild: "@me".into(),
                channel: "20".into(),
                message: "21".into(),
            })
        );
        assert_eq!(
            parse("https://discord.com/channels/@me"),
            Some(Route::DirectMessages)
        );
        assert_eq!(
            parse("yapfast://channels/@me/20"),
            Some(Route::Channel {
                guild: "@me".into(),
                channel: "20".into(),
            })
        );
        assert_eq!(
            parse("https://discordapp.com/users/99"),
            Some(Route::User { user: "99".into() })
        );
        assert_eq!(
            parse("http://ptb.discord.com/invite/Ab-C123?event=44"),
            Some(Route::Invite {
                code: "Ab-C123".into(),
            })
        );
        assert_eq!(
            parse("discord://-/invite/vanity"),
            Some(Route::Invite {
                code: "vanity".into(),
            })
        );
        // On discord.gg the only path segment is an invite code, even one
        // called gift; a Nitro gift is on discord.gift instead.
        assert_eq!(
            parse("https://discord.gg/gift"),
            Some(Route::Invite {
                code: "gift".into(),
            })
        );
        assert_eq!(
            parse("https://discord.gift/AbCdEfGhIjKlMnOp"),
            Some(Route::Gift {
                code: "AbCdEfGhIjKlMnOp".into(),
            })
        );
        assert_eq!(
            parse("https://discord.com/gifts/AbCdEfGhIjKlMnOp"),
            Some(Route::Gift {
                code: "AbCdEfGhIjKlMnOp".into(),
            })
        );
        // Discord's client drops the dashes a code may be grouped with.
        assert_eq!(
            parse("https://discord.gift/AbCd-EfGh-IjKl-MnOp"),
            Some(Route::Gift {
                code: "AbCdEfGhIjKlMnOp".into(),
            })
        );
        assert_eq!(
            parse("discord://-/gifts/AbCdEfGhIjKlMnOp"),
            Some(Route::Gift {
                code: "AbCdEfGhIjKlMnOp".into(),
            })
        );
        assert_eq!(
            parse("https://discord.com/events/10/11"),
            Some(Route::Event {
                guild: "10".into(),
                event: "11".into(),
            })
        );
        assert_eq!(
            parse("https://discord.com/events/10/11/12"),
            Some(Route::Event {
                guild: "10".into(),
                event: "11".into(),
            }),
            "one occurrence of a recurring event"
        );
        assert_eq!(
            parse("https://discord.com/oauth2/authorize?client_id=157730590492196864&scope=bot"),
            Some(Route::Authorize {
                query: "client_id=157730590492196864&scope=bot".into(),
            })
        );
        assert!(matches!(
            parse(
                "https://discord.com/api/oauth2/authorize?client_id=157730590492196864&scope=bot"
            ),
            Some(Route::Authorize { .. })
        ));
    }

    #[test]
    fn leaves_other_pages_and_look_alikes_alone() {
        for link in [
            // Discord pages with no Yapfast screen behind this router.
            "https://discord.gift/gift",
            "https://discord.com/gifts/not_a_gift_code",
            "https://discord.gift/g-i-f-t",
            "https://discord.com/oauth2/authorize?scope=bot",
            "https://discord.com/oauth2/authorize?client_id=not-an-app&scope=bot",
            "https://discord.com.example.com/oauth2/authorize?client_id=157730590492196864&scope=bot",
            "https://discord.com/events/not-a-server/11",
            "https://discord.com/events/10",
            "https://discord.com/events/10/11/soon",
            "https://discord.new/aTemplateCode",
            "https://discord.com/shop",
            // Hosts that merely contain or resemble Discord's.
            "https://discord.com.example.com/channels/10/11",
            "https://www.discord.com.example.com/channels/10/11",
            "https://notdiscord.com/channels/10/11",
            "https://discord.com@evil.example/channels/10/11",
            "https://someone@discord.com/channels/10/11",
            "https://discord.com:4443/channels/10/11",
            // Places no route names, and ids that are not snowflakes.
            "https://discord.com/channels/@favorites",
            "https://discord.com/channels/10/channel-browser",
            "https://discord.com/channels/10/11/12/13",
            "https://discord.com/channels/10//12",
            "https://discord.com/users/not-a-user",
            "https://discord.com/users/123456789012345678901",
            "discord.gg/vanity",
            "javascript://discord.com/channels/10/11",
        ] {
            assert_eq!(parse(link), None, "{link}");
        }
    }

    #[test]
    fn tells_discords_own_pages_from_others() {
        for page in [
            "https://discord.com/oauth2/authorized",
            "https://www.discord.com/channels/@me",
            "http://ptb.discord.com/",
            "HTTPS://DISCORD.COM/app",
        ] {
            assert!(on_discord(page), "{page}");
        }
        for page in [
            "https://discord.com.example.com/",
            "https://discord.com@evil.example/",
            "https://notdiscord.com/",
            "discord://-/channels/@me",
            "myapp://discord.com/cb?code=x",
            "javascript:alert(1)//discord.com",
            "file:///discord.com",
            "discord.com",
        ] {
            assert!(!on_discord(page), "{page}");
        }
    }
}
