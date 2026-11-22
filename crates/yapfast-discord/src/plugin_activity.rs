//! An activity a plugin shows on this account's presence, beside the game and
//! the song the session already shares.
//!
//! The shape is Discord's Activity Object (docs.discord.food,
//! resources/presence#activity-object): a name, a type, `details`, `state`,
//! timestamps in milliseconds, `assets` and `buttons` with their addresses in
//! `metadata.button_urls`, the convention Discord's own clients send. Pictures
//! need an application: a web address goes through
//! `POST /applications/{id}/external-assets`, Discord's media proxy, and comes
//! back as an `mp:` key, as the Rich Presence bridge does for a game (see
//! `rpc.rs`); an application's own asset name is looked up in its assets.
//! How often it goes out is the gateway's: five presence updates in twenty
//! seconds, coalesced to the latest (`gateway.rs`).

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use crate::api::Api;

/// What a plugin asked to show, already checked against Discord's lengths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginActivity {
    /// Discord's activity type: 0 playing, 2 listening, 3 watching,
    /// 5 competing.
    pub kind: u8,
    pub name: String,
    pub details: Option<String>,
    pub state: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub application_id: Option<String>,
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    /// Labels and the pages they open, at most two.
    pub buttons: Vec<(String, String)>,
    /// A streaming activity's stream.
    pub url: Option<String>,
    /// Its party's id, and how many are in it of how many.
    pub party_id: Option<String>,
    pub party_size: Option<(u32, u32)>,
}

impl PluginActivity {
    /// The activity as the gateway takes it, with pictures already turned
    /// into keys Discord shows.
    pub fn to_gateway(&self, large: Option<&str>, small: Option<&str>) -> Value {
        let mut out = json!({ "name": self.name, "type": self.kind });
        if let Some(id) = &self.application_id {
            out["application_id"] = json!(id);
        }
        if let Some(url) = &self.url {
            out["url"] = json!(url);
        }
        if self.party_id.is_some() || self.party_size.is_some() {
            let mut party = json!({});
            if let Some(id) = &self.party_id {
                party["id"] = json!(id);
            }
            if let Some((now, most)) = self.party_size {
                party["size"] = json!([now, most]);
            }
            out["party"] = party;
        }
        for (key, value) in [("details", &self.details), ("state", &self.state)] {
            if let Some(value) = value {
                out[key] = json!(value);
            }
        }
        if self.start_ms.is_some() || self.end_ms.is_some() {
            let mut timestamps = json!({});
            if let Some(start) = self.start_ms {
                timestamps["start"] = json!(start);
            }
            if let Some(end) = self.end_ms {
                timestamps["end"] = json!(end);
            }
            out["timestamps"] = timestamps;
        }
        let mut assets = json!({});
        for (key, value) in [
            ("large_image", large),
            ("small_image", small),
            ("large_text", self.large_text.as_deref()),
            ("small_text", self.small_text.as_deref()),
        ] {
            if let Some(value) = value {
                assets[key] = json!(value);
            }
        }
        if assets.as_object().is_some_and(|assets| !assets.is_empty()) {
            out["assets"] = assets;
        }
        if !self.buttons.is_empty() {
            out["buttons"] = json!(
                self.buttons
                    .iter()
                    .map(|(label, _)| label)
                    .collect::<Vec<_>>()
            );
            out["metadata"] = json!({ "button_urls": self.buttons.iter().map(|(_, url)| url).collect::<Vec<_>>() });
        }
        out
    }

    /// The activity with its pictures resolved through its application.
    pub(crate) async fn resolve(&self, api: &Api) -> Result<Value> {
        let images = [self.large_image.as_deref(), self.small_image.as_deref()];
        if images.iter().all(Option::is_none) {
            return Ok(self.to_gateway(None, None));
        }
        let application = self
            .application_id
            .as_deref()
            .context("an activity's pictures need an application")?;
        let addresses: Vec<&str> = images
            .iter()
            .flatten()
            .copied()
            .filter(|image| image.starts_with("https://"))
            .collect();
        let mut proxied = std::collections::HashMap::new();
        if !addresses.is_empty() {
            let answer = api.rpc_external_assets(application, &addresses).await?;
            for asset in answer.as_array().context("invalid external assets")? {
                if let (Some(url), Some(path)) =
                    (asset["url"].as_str(), asset["external_asset_path"].as_str())
                {
                    proxied.insert(url.to_owned(), format!("mp:{path}"));
                }
            }
        }
        let named = images
            .iter()
            .flatten()
            .any(|image| !image.starts_with("https://") && !snowflake(image));
        let mut keys = std::collections::HashMap::new();
        if named {
            let answer = api.rpc_assets(application).await?;
            for asset in answer.as_array().context("invalid application assets")? {
                if let (Some(name), Some(id)) = (asset["name"].as_str(), asset["id"].as_str()) {
                    keys.insert(name.to_owned(), id.to_owned());
                }
            }
        }
        let key = |image: Option<&str>| -> Result<Option<String>> {
            let Some(image) = image else {
                return Ok(None);
            };
            if image.starts_with("https://") {
                return proxied
                    .get(image)
                    .cloned()
                    .map(Some)
                    .context("Discord didn't take the picture's address");
            }
            if snowflake(image) {
                return Ok(Some(image.to_owned()));
            }
            keys.get(image)
                .cloned()
                .map(Some)
                .with_context(|| format!("the application has no asset named {image}"))
        };
        let (large, small) = (key(images[0])?, key(images[1])?);
        let out = self.to_gateway(large.as_deref(), small.as_deref());
        ensure!(
            serde_json::to_vec(&out)?.len() <= 3072,
            "the activity is too large for a presence"
        );
        Ok(out)
    }
}

fn snowflake(text: &str) -> bool {
    !text.is_empty() && text.len() <= 20 && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_activity_is_written_as_discords_clients_write_one() {
        let activity = PluginActivity {
            kind: 2,
            name: "Pomodoro".into(),
            details: Some("Focus".into()),
            start_ms: Some(5),
            buttons: vec![("Site".into(), "https://example.com".into())],
            ..PluginActivity::default()
        };
        let shown = activity.to_gateway(None, None);
        assert_eq!(shown["type"], 2);
        assert_eq!(shown["name"], "Pomodoro");
        assert_eq!(shown["details"], "Focus");
        assert_eq!(shown["timestamps"]["start"], 5);
        assert!(shown.get("assets").is_none());
        assert_eq!(shown["buttons"], json!(["Site"]));
        assert!(shown.get("party").is_none());
        let streaming = PluginActivity {
            kind: 1,
            url: Some("https://twitch.tv/a".into()),
            party_size: Some((1, 4)),
            ..PluginActivity::default()
        }
        .to_gateway(None, None);
        assert_eq!(streaming["url"], "https://twitch.tv/a");
        assert_eq!(streaming["party"]["size"], json!([1, 4]));
        assert_eq!(
            shown["metadata"]["button_urls"],
            json!(["https://example.com"])
        );
        let pictured = activity.to_gateway(Some("mp:external/x"), None);
        assert_eq!(pictured["assets"]["large_image"], "mp:external/x");
    }
}
