//! An account on another service on a profile, such as Steam, and the mark
//! its tile shows (`ProfileConnectionRow`, `ui/people.slint:415`; the marks
//! as `connection_glyph` picks them, `src/ui/profile.rs:1158`).

use crate::icon::Icon;

/// One of their accounts on another service.
#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    /// The service, as Discord names it ("steam", "github").
    pub kind: String,
    /// Its mark: [`connection_icon`] of the kind.
    pub icon: Icon,
    /// Their name there.
    pub name: String,
    /// Where it opens; empty when it doesn't.
    pub url: String,
}

/// The mark of a service, by Discord's name for it; a link for one
/// without a mark of its own.
pub fn connection_icon(kind: &str) -> Icon {
    match kind {
        "domain" => Icon::Globe,
        "battlenet" => Icon::Battledotnet,
        "bluesky" => Icon::Bluesky,
        "bungie" => Icon::Bungie,
        "crunchyroll" => Icon::Crunchyroll,
        "ebay" => Icon::Ebay,
        "epicgames" => Icon::Epicgames,
        "facebook" => Icon::Facebook,
        "github" => Icon::Github,
        "instagram" => Icon::Instagram,
        "leagueoflegends" => Icon::Leagueoflegends,
        "mastodon" => Icon::Mastodon,
        "paypal" => Icon::Paypal,
        "playstation" | "playstation-stg" => Icon::Playstation,
        "reddit" => Icon::Reddit,
        "riotgames" => Icon::Riotgames,
        "roblox" => Icon::Roblox,
        "samsung" => Icon::Samsung,
        "soundcloud" => Icon::Soundcloud,
        "spotify" => Icon::Spotify,
        "steam" => Icon::Steam,
        "tiktok" => Icon::Tiktok,
        "twitch" => Icon::Twitch,
        "twitter" | "x" => Icon::XLogo,
        "youtube" => Icon::Youtube,
        _ => Icon::Link,
    }
}
