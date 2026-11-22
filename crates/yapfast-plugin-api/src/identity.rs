//! How a plugin changes the way one person appears.
//!
//! §17.2 of the plan asks for one answer per (person, server) and not one per
//! message, because a channel shows the same author twenty times: the parent
//! keeps the answers in a cache, and every surface that draws that person asks
//! the cache rather than a plugin.
//!
//! The answer is display only. Nothing here changes a message, and nothing here
//! reaches Discord: a plugin that wants to know more about somebody reads it
//! with `people.get` or `people.member`.

use serde::{Deserialize, Serialize};

/// The longest display name, pronoun line or badge a plugin may answer with.
pub const MAX_TEXT: usize = 64;

/// The most badges one person may be given.
pub const MAX_BADGES: usize = 8;

/// The longest address an answer may name for a picture.
pub const MAX_URL: usize = 2048;

/// What `yf_identity` is given: one person, in one server.
///
/// It is the summary a plugin needs to decide, and no more: it carries no
/// token and no message. A plugin that wants the whole member reads it with
/// `people.member`, which is what `people.read` is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRequest {
    /// Who is shown, by id.
    pub person: String,
    /// The server they are shown in, when one holds them: a DM and a room no
    /// server holds have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// Their name, as Yapfast already shows it.
    pub name: String,
    /// Their Discord username, when the account has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Whether this is the account itself.
    pub own: bool,
}

impl IdentityRequest {
    /// The pair this request is kept under, which is what makes one answer
    /// serve every message: `(person, server)`.
    pub fn pair(&self) -> (String, String) {
        (self.person.clone(), self.server.clone().unwrap_or_default())
    }
}

/// How one person should look, as a plugin answers.
///
/// Every field is optional: a plugin that only changes the name answers only
/// the name, and the rest stays as Yapfast had it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// The name to show in place of theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The colour of their name, `0xRRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_colour: Option<u32>,
    /// The address of an avatar to show in place of theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The address of a banner to show behind their profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// The address of a decoration to draw around their avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoration: Option<String>,
    /// What to show in place of their pronouns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    /// Badges to show on their profile, in this order, in place of theirs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<String>,
}

impl Identity {
    /// Nothing here changes how the person looks, so there is nothing to cache.
    pub fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.name_colour.is_none()
            && self.avatar.is_none()
            && self.banner.is_none()
            && self.decoration.is_none()
            && self.pronouns.is_none()
            && self.badges.is_empty()
    }

    /// The answer with everything Yapfast won't draw dropped.
    ///
    /// A field is dropped rather than the whole answer refused: a plugin that
    /// gets one field wrong still changes the others, and the reason is a
    /// plugin's own to find in its log. A name, a pronoun line or a badge that
    /// is empty, over [`MAX_TEXT`], or on more than one line is not a name a
    /// row can draw; a picture address that isn't `https` isn't one Yapfast
    /// will ask for.
    pub fn sanitized(mut self) -> Self {
        self.display_name = self.display_name.and_then(text);
        self.pronouns = self.pronouns.and_then(text);
        self.badges = self
            .badges
            .into_iter()
            .filter_map(text)
            .take(MAX_BADGES)
            .collect();
        self.name_colour = self.name_colour.filter(|colour| *colour <= 0x00FF_FFFF);
        self.avatar = self.avatar.filter(|url| address(url));
        self.banner = self.banner.filter(|url| address(url));
        self.decoration = self.decoration.filter(|url| address(url));
        self
    }
}

/// One line of text a row can draw, or nothing.
fn text(field: String) -> Option<String> {
    let field = field.trim();
    if field.is_empty() || field.len() > MAX_TEXT || field.contains(['\n', '\r']) {
        return None;
    }
    Some(field.to_owned())
}

/// An address Yapfast will fetch, which is `https` and nothing else: a plugin
/// that answered `http`, a `file:` path or a `data:` blob would have Yapfast
/// read something it shouldn't.
fn address(url: &str) -> bool {
    url.len() <= MAX_URL
        && url
            .strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_answer_changes_nothing() {
        assert!(Identity::default().is_empty());
        assert!(
            !Identity {
                pronouns: Some("they".into()),
                ..Identity::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn what_a_row_cannot_draw_is_dropped_and_the_rest_kept() {
        let kept = Identity {
            display_name: Some("Ada".into()),
            pronouns: Some("x".repeat(MAX_TEXT + 1)),
            name_colour: Some(0x0100_0000),
            avatar: Some("http://example.test/a.png".into()),
            banner: Some("https://example.test/b.png".into()),
            decoration: Some("data:image/png;base64,AAAA".into()),
            badges: (0..MAX_BADGES + 4).map(|n| format!("b{n}")).collect(),
        }
        .sanitized();
        assert_eq!(kept.display_name.as_deref(), Some("Ada"));
        assert_eq!(kept.pronouns, None);
        assert_eq!(kept.name_colour, None);
        assert_eq!(kept.avatar, None);
        assert_eq!(kept.banner.as_deref(), Some("https://example.test/b.png"));
        assert_eq!(kept.decoration, None);
        assert_eq!(kept.badges.len(), MAX_BADGES);
    }

    #[test]
    fn a_name_on_two_lines_is_not_a_name() {
        let kept = Identity {
            display_name: Some("Ada\nLovelace".into()),
            ..Identity::default()
        }
        .sanitized();
        assert_eq!(kept.display_name, None);
    }

    #[test]
    fn a_request_is_kept_under_its_person_and_server() {
        let request = IdentityRequest {
            person: "person-1".into(),
            server: None,
            name: "Ada".into(),
            username: None,
            own: false,
        };
        assert_eq!(request.pair(), ("person-1".into(), String::new()));
    }
}
