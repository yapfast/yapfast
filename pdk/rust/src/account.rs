//! The account's own calls: sending files the plugin made, its activity on the
//! profile, the profile itself, the reads of search, drafts and signed-in
//! devices, and the linked Spotify.
//!
//! Each needs its capability in the manifest (`messages.attach`,
//! `activity.set`, `profile.write`, `messages.search`, `drafts.read`,
//! `account.sessions`, `spotify.control`), and each is answered later: the
//! call gives a [`Pending`], and its answer arrives in a later event batch,
//! which [`Context::answer`](crate::plugin::Context::answer) finds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Error;
use crate::Result;
use crate::call::{self, HostReturn, Pending, method};
use crate::json;

/// Makes a call that is answered later, and gives the [`Pending`] to wait on.
fn ask(method: &str, payload: impl Serialize) -> Result<Pending> {
    match call::send(method, payload)? {
        HostReturn::Pending { tag } => Ok(Pending::from_tag(tag)),
        other => call::value(method, other).and_then(|value| {
            Err(Error::msg(format!(
                "`{method}` answered at once, with {value}, where an answer later was expected"
            )))
        }),
    }
}

/// One file a plugin sends: bytes it made, or one of its assets by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spoiler: bool,
}

impl Attachment {
    /// A file of bytes the plugin made, such as a GIF from [`crate::image`].
    pub fn bytes(name: impl Into<String>, bytes: &[u8]) -> Self {
        Attachment {
            name: name.into(),
            bytes_base64: Some(crate::wire::base64::encode(bytes)),
            ..Attachment::default()
        }
    }

    /// One of the package's own assets, by its manifest name.
    pub fn asset(name: impl Into<String>, asset: impl Into<String>) -> Self {
        Attachment {
            name: name.into(),
            asset: Some(asset.into()),
            ..Attachment::default()
        }
    }
}

/// Sends `body` in `room` with files the plugin made: `messages.attach`, on
/// the same rails as any send (a click of the person's behind it, or
/// `messages.send.background`, and five in ten seconds). Every message a
/// plugin sends says so under it.
pub fn attach(room: &str, body: &str, attachments: &[Attachment]) -> Result<Pending> {
    ask(
        method::MESSAGES_ATTACH,
        json!({ "room": room, "body": body, "attachments": attachments }),
    )
}

/// An activity for the account's presence, as `activity.set` takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// `playing`, `listening`, `watching` or `competing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    /// The Discord application its pictures go through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
    /// Up to two `{ label, url }`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<ActivityButton>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityButton {
    pub label: String,
    pub url: String,
}

/// Shows an activity of the plugin's own on the account: `activity.set`.
pub fn set_activity(activity: &Activity) -> Result<Pending> {
    ask(method::ACTIVITY_SET, activity)
}

/// Takes the plugin's activity away.
pub fn clear_activity() -> Result<Pending> {
    ask(method::ACTIVITY_CLEAR, Value::Null)
}

/// Changes the profile: `profile.write`, which asks the person the first
/// time. `None` leaves a field as it is.
pub fn set_profile(
    bio: Option<&str>,
    pronouns: Option<&str>,
    custom_status: Option<&str>,
) -> Result<Pending> {
    let mut payload = json!({});
    if let Some(bio) = bio {
        payload["bio"] = json!(bio);
    }
    if let Some(pronouns) = pronouns {
        payload["pronouns"] = json!(pronouns);
    }
    if let Some(text) = custom_status {
        payload["custom_status"] = json!({ "text": text });
    }
    ask(method::PROFILE_SET, payload)
}

/// Searches a conversation or a server as the search box does:
/// `messages.search`. The answer is `{ total, hits: [...], indexing }`.
pub fn search(place: &str, query: &str, offset: u32) -> Result<Pending> {
    ask(
        method::MESSAGES_SEARCH,
        json!({ "place": place, "query": query, "offset": offset }),
    )
}

/// The drafts the person left unsent: `drafts.read`, `[{ room, body }]`.
pub fn drafts() -> Result<Pending> {
    ask(method::DRAFTS_LIST, Value::Null)
}

/// The devices signed in to the account, read-only: `account.sessions`.
pub fn sessions() -> Result<Pending> {
    ask(method::ACCOUNT_SESSIONS, Value::Null)
}

/// Who Discord ranks closest to the account, read-only, highest first:
/// `relationships.read`. `{ users: [{ person, affinity }] }`, with v2's
/// probabilities and ranks when `detailed`.
pub fn affinities(detailed: bool) -> Result<Pending> {
    ask(
        method::RELATIONSHIPS_AFFINITIES,
        json!({ "of": "users", "detailed": detailed }),
    )
}

/// The servers Discord ranks the account uses most, read-only:
/// `{ servers: [{ server, affinity }] }`.
pub fn server_affinities() -> Result<Pending> {
    ask(method::RELATIONSHIPS_AFFINITIES, json!({ "of": "servers" }))
}

/// The linked Spotify: `spotify.control`. Yapfast makes the requests with the
/// token Discord lends and the plugin never sees it. Control needs Spotify
/// Premium; a free account's calls fail with a sentence saying so.
pub mod spotify {
    use super::*;

    pub fn now() -> Result<Pending> {
        ask(method::SPOTIFY_NOW, Value::Null)
    }
    pub fn play() -> Result<Pending> {
        ask(method::SPOTIFY_PLAY, Value::Null)
    }
    pub fn pause() -> Result<Pending> {
        ask(method::SPOTIFY_PAUSE, Value::Null)
    }
    pub fn next() -> Result<Pending> {
        ask(method::SPOTIFY_NEXT, Value::Null)
    }
    pub fn previous() -> Result<Pending> {
        ask(method::SPOTIFY_PREVIOUS, Value::Null)
    }
    pub fn seek(position_ms: u64) -> Result<Pending> {
        ask(method::SPOTIFY_SEEK, json!({ "position_ms": position_ms }))
    }
    /// 0 to 100.
    pub fn volume(percent: u8) -> Result<Pending> {
        ask(method::SPOTIFY_VOLUME, json!({ "percent": percent }))
    }
    pub fn shuffle(on: bool) -> Result<Pending> {
        ask(method::SPOTIFY_SHUFFLE, json!({ "on": on }))
    }
    /// `off`, `track` or `context`.
    pub fn repeat(mode: &str) -> Result<Pending> {
        ask(method::SPOTIFY_REPEAT, json!({ "mode": mode }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_is_the_shape_the_app_reads() {
        let made = Attachment::bytes("pet.gif", b"GIF89a");
        let written = serde_json::to_value(&made).unwrap();
        assert_eq!(
            written,
            json!({ "name": "pet.gif", "bytes_base64": "R0lGODlh" })
        );
        let asset = serde_json::to_value(Attachment::asset("logo.png", "logo")).unwrap();
        assert_eq!(asset, json!({ "name": "logo.png", "asset": "logo" }));
    }

    #[test]
    fn an_activity_leaves_out_what_it_doesnt_say() {
        let activity = Activity {
            name: "Pomodoro".into(),
            ..Activity::default()
        };
        assert_eq!(
            serde_json::to_value(&activity).unwrap(),
            json!({ "name": "Pomodoro" })
        );
    }
}
