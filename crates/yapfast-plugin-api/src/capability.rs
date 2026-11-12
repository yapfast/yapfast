//! What a plugin may do, and the hosts it may reach.
//!
//! A manifest declares these, the review dialog shows them, and both the parent
//! and the helper refuse anything not declared. Nothing here names a person or
//! a token: a capability opens a group of calls, and the calls themselves are
//! checked again where they run.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One ability a manifest may declare.
///
/// The names are public API: they are written in manifests, stored in
/// `grants.json`, and shown at install.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Its own key-value store.
    #[serde(rename = "storage")]
    Storage,
    /// Message events and history queries.
    #[serde(rename = "messages.read")]
    MessagesRead,
    /// Change or cancel outgoing messages.
    #[serde(rename = "messages.intercept")]
    MessagesIntercept,
    /// Send, within limits.
    #[serde(rename = "messages.send")]
    MessagesSend,
    /// Send without a recent user gesture. Reviewed as exceptional.
    #[serde(rename = "messages.send.background")]
    MessagesSendBackground,
    /// Keep deleted messages visible.
    #[serde(rename = "messages.retain")]
    MessagesRetain,
    /// Hide messages from view.
    #[serde(rename = "messages.filter")]
    MessagesFilter,
    /// Profiles, members, friends, presence.
    #[serde(rename = "people.read")]
    PeopleRead,
    /// Servers, channels, roles, permissions.
    #[serde(rename = "servers.read")]
    ServersRead,
    /// Actions the person already has permission for, on their servers.
    #[serde(rename = "servers.manage")]
    ServersManage,
    /// Status, custom status, activity.
    #[serde(rename = "presence.write")]
    PresenceWrite,
    /// Mute, mark as read.
    #[serde(rename = "notifications.manage")]
    NotificationsManage,
    /// Stop the typing indicator being sent.
    #[serde(rename = "typing.intercept")]
    TypingIntercept,
    /// Rename or strip files before upload.
    #[serde(rename = "uploads.intercept")]
    UploadsIntercept,
    /// Bytes of attachments, avatars and emoji.
    #[serde(rename = "media.read")]
    MediaRead,
    /// Save through the native dialog.
    #[serde(rename = "files.save")]
    FilesSave,
    /// Voice state and who is speaking.
    #[serde(rename = "voice.read")]
    VoiceRead,
    /// Play a sound.
    #[serde(rename = "audio.play")]
    AudioPlay,
    /// Speak text through the OS.
    #[serde(rename = "audio.speak")]
    AudioSpeak,
    /// Record from the microphone.
    #[serde(rename = "microphone")]
    Microphone,
    /// OS notifications.
    #[serde(rename = "notifications")]
    Notifications,
    /// Write to the clipboard on a gesture.
    #[serde(rename = "clipboard.write")]
    ClipboardWrite,
    /// Change, hide or replace named parts of the interface.
    #[serde(rename = "ui.modify")]
    UiModify,
    /// HTTPS to the hosts the manifest lists, and to no other.
    ///
    /// The hosts themselves are checked against [`Permissions::network`]; this
    /// capability only says the plugin means to use the network at all.
    #[serde(rename = "network")]
    Network,
    /// One exact loopback port, which the manifest names.
    #[serde(rename = "network.local")]
    NetworkLocal,
    /// Open other applications through listed URI schemes.
    #[serde(rename = "open.app")]
    OpenApp,
}

impl Capability {
    /// Every capability, in the order the review dialog lists them.
    ///
    /// The list stays whole even for a capability nothing checks: a manifest an
    /// older version wrote still parses, and the declared list is filtered where
    /// it is read, so a stored name reads back as itself. See
    /// [`Capability::is_implemented`].
    pub const ALL: [Capability; 26] = [
        Capability::Storage,
        Capability::MessagesRead,
        Capability::MessagesIntercept,
        Capability::MessagesSend,
        Capability::MessagesSendBackground,
        Capability::MessagesRetain,
        Capability::MessagesFilter,
        Capability::PeopleRead,
        Capability::ServersRead,
        Capability::ServersManage,
        Capability::PresenceWrite,
        Capability::NotificationsManage,
        Capability::TypingIntercept,
        Capability::UploadsIntercept,
        Capability::MediaRead,
        Capability::FilesSave,
        Capability::VoiceRead,
        Capability::AudioPlay,
        Capability::AudioSpeak,
        Capability::Microphone,
        Capability::Notifications,
        Capability::ClipboardWrite,
        Capability::UiModify,
        Capability::Network,
        Capability::NetworkLocal,
        Capability::OpenApp,
    ];

    /// Whether anything in this build looks at this capability.
    ///
    /// Two aren't looked at: `files.save` has no save dialog behind it and
    /// `microphone` no recording path, so a manifest may still name them — an
    /// older one does — and reading it leaves them out. The review dialog lists
    /// what a manifest declares, and a line promising a recording with nothing
    /// behind it is worse than a short list.
    ///
    /// `typing.intercept` and `uploads.intercept` stay: they are what
    /// `typing.pre_send` and `attachment.pre_upload` ask for, and the chain
    /// those hooks run is picked by them.
    pub fn is_implemented(self) -> bool {
        !matches!(self, Capability::FilesSave | Capability::Microphone)
    }

    /// The name written in a manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Storage => "storage",
            Capability::MessagesRead => "messages.read",
            Capability::MessagesIntercept => "messages.intercept",
            Capability::MessagesSend => "messages.send",
            Capability::MessagesSendBackground => "messages.send.background",
            Capability::MessagesRetain => "messages.retain",
            Capability::MessagesFilter => "messages.filter",
            Capability::PeopleRead => "people.read",
            Capability::ServersRead => "servers.read",
            Capability::ServersManage => "servers.manage",
            Capability::PresenceWrite => "presence.write",
            Capability::NotificationsManage => "notifications.manage",
            Capability::TypingIntercept => "typing.intercept",
            Capability::UploadsIntercept => "uploads.intercept",
            Capability::MediaRead => "media.read",
            Capability::FilesSave => "files.save",
            Capability::VoiceRead => "voice.read",
            Capability::AudioPlay => "audio.play",
            Capability::AudioSpeak => "audio.speak",
            Capability::Microphone => "microphone",
            Capability::Notifications => "notifications",
            Capability::ClipboardWrite => "clipboard.write",
            Capability::UiModify => "ui.modify",
            Capability::Network => "network",
            Capability::NetworkLocal => "network.local",
            Capability::OpenApp => "open.app",
        }
    }

    /// The line the consent dialog shows beside this capability.
    ///
    /// The four sentences in capitals are the ones the consent dialog shows in
    /// the danger tone; a caller decides how to draw them, not this crate.
    pub fn summary(self) -> &'static str {
        match self {
            Capability::Storage => "Keeps its own data",
            Capability::MessagesRead => "Reads your messages",
            Capability::MessagesIntercept => "Can change what you send",
            Capability::MessagesSend => "Can send messages as you",
            Capability::MessagesSendBackground => {
                "Sends messages automatically, without you doing anything. \
                 Automated sending can get your Discord account banned."
            }
            Capability::MessagesRetain => "Keeps messages after they're deleted",
            Capability::MessagesFilter => "Can hide messages from you",
            Capability::PeopleRead => "Reads people's profiles",
            Capability::ServersRead => "Reads server details",
            Capability::ServersManage => "Can change servers you manage",
            Capability::PresenceWrite => "Changes your status",
            Capability::NotificationsManage => "Can mute and mark things read",
            Capability::TypingIntercept => "Can hide that you're typing",
            Capability::UploadsIntercept => "Can change files you upload",
            Capability::MediaRead => "Reads attachments and pictures",
            Capability::FilesSave => "Saves files where you choose",
            Capability::VoiceRead => "Sees your calls",
            Capability::AudioPlay => "Plays sounds",
            Capability::AudioSpeak => "Speaks out loud",
            Capability::Microphone => "Records your microphone",
            Capability::Notifications => "Shows notifications",
            Capability::ClipboardWrite => "Writes to your clipboard",
            Capability::UiModify => "Can change Yapfast's interface",
            Capability::Network => "Connects to the sites it lists",
            Capability::NetworkLocal => "Connects to an app on this computer",
            Capability::OpenApp => "Opens other applications",
        }
    }

    /// Whether the consent dialog shows this in the danger tone.
    pub fn is_dangerous(self) -> bool {
        matches!(
            self,
            Capability::MessagesIntercept
                | Capability::MessagesSendBackground
                | Capability::Microphone
                | Capability::UiModify
        )
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A host a plugin may reach, as a manifest writes it.
///
/// `api.example.com` is exact. `*.cdn.example.com` matches `cdn.example.com`
/// and any one name below it, and nothing wider: a bare `*` is refused, since a
/// plugin that may reach anywhere says nothing useful at install.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HostPattern(Box<str>);

/// Why a string isn't a host pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostPatternError {
    /// `*` alone, or a `*` anywhere but the first label.
    BadWildcard,
    /// Not `name` or `name.name` and so on, of `a-z0-9-` labels.
    BadHost,
    /// Longer than [`HostPattern::MAX_LEN`].
    TooLong,
}

impl fmt::Display for HostPatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostPatternError::BadWildcard => {
                f.write_str("a host may only start with `*.`; a bare `*` is refused")
            }
            HostPatternError::BadHost => {
                f.write_str("a host is dot-separated labels of a-z, 0-9 and hyphen")
            }
            HostPatternError::TooLong => {
                write!(f, "a host is at most {} bytes", HostPattern::MAX_LEN)
            }
        }
    }
}

impl std::error::Error for HostPatternError {}

impl HostPattern {
    /// The longest pattern accepted, in bytes.
    pub const MAX_LEN: usize = 253;

    /// Checks and takes a pattern.
    pub fn parse(text: &str) -> Result<Self, HostPatternError> {
        if text.len() > Self::MAX_LEN {
            return Err(HostPatternError::TooLong);
        }
        let (wildcard, host) = match text.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None if text.contains('*') => return Err(HostPatternError::BadWildcard),
            None => (false, text),
        };
        if host.is_empty() || host.contains("..") || host.starts_with('.') || host.ends_with('.') {
            return Err(HostPatternError::BadHost);
        }
        if host.split('.').count() < 2 {
            return Err(HostPatternError::BadHost);
        }
        if !host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        {
            return Err(HostPatternError::BadHost);
        }
        if host
            .split('.')
            .any(|label| label.is_empty() || label.starts_with('-') || label.ends_with('-'))
        {
            return Err(HostPatternError::BadHost);
        }
        let text = if wildcard {
            format!("*.{host}")
        } else {
            host.to_owned()
        };
        Ok(HostPattern(text.into()))
    }

    /// Whether this pattern covers `host`, which is matched in lowercase.
    pub fn matches(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        match self.0.strip_prefix("*.") {
            Some(base) => host == base || host.ends_with(&format!(".{base}")),
            None => host == self.0.as_ref(),
        }
    }

    /// The pattern as written in the manifest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for HostPattern {
    type Err = HostPatternError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<String> for HostPattern {
    type Error = HostPatternError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<HostPattern> for String {
    fn from(pattern: HostPattern) -> String {
        pattern.0.into()
    }
}

use std::str::FromStr;

/// A manifest's `permissions` object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Permissions {
    /// Every capability the plugin means to use.
    ///
    /// Read through [`declared`], which leaves out the ones nothing checks; see
    /// [`Capability::is_implemented`].
    #[serde(default, deserialize_with = "declared")]
    pub capabilities: Vec<Capability>,
    /// The hosts it may reach over HTTPS.
    #[serde(default)]
    pub network: Vec<HostPattern>,
    /// The one loopback port `network.local` opens, if it is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    /// URI schemes `open.app` may open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schemes: Vec<String>,
}

impl Permissions {
    /// Whether the plugin declared `capability`.
    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// The `network` capability, or a `network` host, is declared.
    pub fn reaches_network(&self) -> bool {
        self.allows(Capability::Network) || !self.network.is_empty()
    }

    /// A sentence about a combination worth calling out, if there is one.
    ///
    /// `messages.read` with any host is the real threat a plugin poses: it can
    /// send what it reads to a server it names. Listing that as two unrelated
    /// lines in the dialog hides it, so the dialog says this instead.
    pub fn risky_combination(&self) -> Option<String> {
        let reads = self.allows(Capability::MessagesRead)
            || self.allows(Capability::MessagesSend)
            || self.allows(Capability::MessagesIntercept);
        let reaches = self.reaches_network() || self.allows(Capability::NetworkLocal);
        if !(reads && reaches) {
            return None;
        }
        let hosts: Vec<&str> = self.network.iter().map(HostPattern::as_str).collect();
        let where_to = if hosts.is_empty() {
            "a server it names".to_owned()
        } else if hosts.len() == 1 {
            hosts[0].to_owned()
        } else {
            let last = hosts[hosts.len() - 1];
            format!("{} and {last}", hosts[..hosts.len() - 1].join(", "))
        };
        Some(format!(
            "This plugin can read your messages and connect to {where_to}, so it could send them there."
        ))
    }
}

/// Reads the capabilities a manifest declares, leaving out the ones this build
/// doesn't check.
///
/// Dropped rather than refused: a plugin that lists what it could use is still
/// one Yapfast can run, and the capability it named asks for nothing that
/// happens.
fn declared<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Capability>, D::Error> {
    let mut declared: Vec<Capability> = Deserialize::deserialize(deserializer)?;
    declared.retain(|capability| capability.is_implemented());
    Ok(declared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_through_json() {
        for capability in Capability::ALL {
            let json = serde_json::to_string(&capability).unwrap();
            assert_eq!(json, format!("\"{}\"", capability.as_str()));
            assert_eq!(
                serde_json::from_str::<Capability>(&json).unwrap(),
                capability
            );
        }
    }

    #[test]
    fn a_capability_nothing_checks_is_left_out_of_what_is_read() {
        let permissions: Permissions = serde_json::from_str(
            r#"{ "capabilities": ["messages.read", "files.save", "microphone", "typing.intercept"] }"#,
        )
        .expect("a manifest's permissions");
        assert_eq!(
            permissions.capabilities,
            [Capability::MessagesRead, Capability::TypingIntercept],
            "the typing hook is real, the save dialog and the microphone aren't"
        );
        assert!(
            !Capability::FilesSave.is_implemented() && !Capability::Microphone.is_implemented(),
            "the save dialog and the recording path aren't there yet"
        );
        assert!(
            Capability::TypingIntercept.is_implemented()
                && Capability::UploadsIntercept.is_implemented(),
            "the typing and upload hooks are"
        );
    }

    #[test]
    fn hosts_match_one_level_below_a_wildcard() {
        let pattern = HostPattern::parse("*.cdn.example.com").unwrap();
        assert!(pattern.matches("cdn.example.com"));
        assert!(pattern.matches("a.cdn.example.com"));
        assert!(!pattern.matches("example.com"));
        assert!(!pattern.matches("evilcdn.example.com"));
        assert!(!pattern.matches("cdn.example.com.evil.test"));

        let exact = HostPattern::parse("api.example.com").unwrap();
        assert!(exact.matches("API.example.com"));
        assert!(!exact.matches("x.api.example.com"));
    }

    #[test]
    fn a_bare_wildcard_is_refused() {
        assert_eq!(
            HostPattern::parse("*").unwrap_err(),
            HostPatternError::BadWildcard
        );
        assert_eq!(
            HostPattern::parse("*.com").unwrap_err(),
            HostPatternError::BadHost
        );
    }

    #[test]
    fn reading_messages_and_reaching_out_is_called_out() {
        let permissions = Permissions {
            capabilities: vec![Capability::MessagesRead, Capability::Network],
            network: vec![HostPattern::parse("api.example.com").unwrap()],
            ..Permissions::default()
        };
        let sentence = permissions.risky_combination().unwrap();
        assert!(sentence.contains("api.example.com"));
        assert!(sentence.contains("read your messages"));
    }

    #[test]
    fn a_single_read_or_reach_is_not() {
        let quiet = Permissions {
            capabilities: vec![Capability::MessagesRead],
            ..Permissions::default()
        };
        assert!(quiet.risky_combination().is_none());
    }
}
