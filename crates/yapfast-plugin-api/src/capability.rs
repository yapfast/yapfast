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
    /// Change the microphone, the speakers, their volumes and noise
    /// suppression.
    #[serde(rename = "voice.control")]
    VoiceControl,
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
    /// Send a file from the plugin's chosen folder with a message.
    #[serde(rename = "messages.attach")]
    MessagesAttach,
    /// Send without a keystroke, as an agent, at the rate the manifest names.
    /// Powerful: see [`Capability::is_powerful`].
    #[serde(rename = "messages.send.agent")]
    MessagesSendAgent,
    /// Read, or read and write, one folder the person picks. Powerful.
    #[serde(rename = "fs.folder")]
    FsFolder,
    /// Read, or read and write, the home folder or the whole disk. Powerful.
    #[serde(rename = "fs.full")]
    FsFull,
    /// HTTPS to any public host, not only the ones the manifest lists. Powerful.
    #[serde(rename = "net.any")]
    NetAny,
    /// Raw TCP or WebSocket connections out. Powerful.
    #[serde(rename = "net.socket")]
    NetSocket,
    /// Serve on this computer's loopback, behind a token. Powerful.
    #[serde(rename = "net.listen")]
    NetListen,
    /// Show an activity of its own on the account's presence: a name, two
    /// lines, timestamps, pictures and buttons, as Rich Presence does.
    #[serde(rename = "activity.set")]
    ActivitySet,
    /// Change the account's bio, pronouns and custom status, after the person
    /// says yes the first time.
    #[serde(rename = "profile.write")]
    ProfileWrite,
    /// Name actions the person can bind to keys in Settings → Keybinds. The
    /// plugin hears that its action ran, never which key, nor any other key.
    #[serde(rename = "keybinds")]
    Keybinds,
    /// Play its own sounds in place of Yapfast's for the events it names.
    #[serde(rename = "sounds.replace")]
    SoundsReplace,
    /// Search messages through Discord's search, as the search box does.
    #[serde(rename = "messages.search")]
    MessagesSearch,
    /// Read the drafts the person left unsent in the composer.
    #[serde(rename = "drafts.read")]
    DraftsRead,
    /// List the devices signed in to the account, read-only.
    #[serde(rename = "account.sessions")]
    AccountSessions,
    /// Read and control the linked Spotify account's playback.
    #[serde(rename = "spotify.control")]
    SpotifyControl,
    /// Offer to open a link the person clicked in an app, beside the leaving
    /// dialog's Visit site, for the link hosts the manifest lists.
    #[serde(rename = "links.offer")]
    LinksOffer,
    /// Add a few words to the end of the window's title.
    #[serde(rename = "window.title")]
    WindowTitle,
    /// See what the person searches in the quick switcher, and reorder or
    /// leave out what it found.
    #[serde(rename = "switcher.rank")]
    SwitcherRank,
    /// How long this start of Yapfast took, once, as `startup_timing`.
    #[serde(rename = "app.timing")]
    AppTiming,
    /// Friends added and removed, and friend requests, as they happen.
    #[serde(rename = "relationships.read")]
    RelationshipsRead,
    /// Show pictures from the HTTPS hosts the manifest lists in what it draws.
    #[serde(rename = "images.web")]
    ImagesWeb,
    /// The game Yapfast sees this account playing, as it starts and stops.
    #[serde(rename = "activity.read")]
    ActivityRead,
    /// Sign in to a service with the plugin's own app, in the browser, and
    /// use the account: the manifest's `oauth`.
    #[serde(rename = "oauth.login")]
    OAuthLogin,
    /// Draw a free-floating layer of its own pictures over the window.
    #[serde(rename = "overlay")]
    Overlay,
}

impl Capability {
    /// Every capability, in the order the review dialog lists them.
    ///
    /// The list stays whole even for a capability nothing checks: a manifest an
    /// older version wrote still parses, and the declared list is filtered where
    /// it is read, so a stored name reads back as itself. See
    /// [`Capability::is_implemented`].
    pub const ALL: [Capability; 51] = [
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
        Capability::VoiceControl,
        Capability::AudioPlay,
        Capability::AudioSpeak,
        Capability::Microphone,
        Capability::Notifications,
        Capability::ClipboardWrite,
        Capability::UiModify,
        Capability::Network,
        Capability::NetworkLocal,
        Capability::OpenApp,
        Capability::MessagesAttach,
        Capability::MessagesSendAgent,
        Capability::FsFolder,
        Capability::FsFull,
        Capability::NetAny,
        Capability::NetSocket,
        Capability::NetListen,
        Capability::ActivitySet,
        Capability::ProfileWrite,
        Capability::Keybinds,
        Capability::SoundsReplace,
        Capability::MessagesSearch,
        Capability::DraftsRead,
        Capability::AccountSessions,
        Capability::SpotifyControl,
        Capability::LinksOffer,
        Capability::WindowTitle,
        Capability::SwitcherRank,
        Capability::AppTiming,
        Capability::RelationshipsRead,
        Capability::ImagesWeb,
        Capability::ActivityRead,
        Capability::OAuthLogin,
        Capability::Overlay,
    ];

    /// Whether anything in this build looks at this capability.
    ///
    /// One isn't looked at: `microphone` has no path behind it yet — the
    /// design is `docs/plugins/audio-processing.md` — so a manifest may still
    /// name it and reading it leaves it out. The review dialog lists what a
    /// manifest declares, and a line promising a recording with nothing behind
    /// it is worse than a short list.
    ///
    /// `typing.intercept` and `uploads.intercept` stay: they are what
    /// `typing.pre_send` and `attachment.pre_upload` ask for, and the chain
    /// those hooks run is picked by them.
    pub fn is_implemented(self) -> bool {
        true
    }

    /// Whether this is one of the powerful tier: abilities past the sandbox's
    /// usual walls, which the person turns on in Settings → Plugins → Advanced
    /// before any plugin may use one, confirms in a red install dialog, and
    /// allows again the first time each is used (`docs/plugins/permissions.md`).
    pub fn is_powerful(self) -> bool {
        matches!(
            self,
            Capability::MessagesSendAgent
                | Capability::FsFolder
                | Capability::FsFull
                | Capability::NetAny
                | Capability::NetSocket
                | Capability::NetListen
        )
    }

    /// Whether the person is asked, the first time a plugin uses it, before
    /// it does anything: the powerful tier, and `profile.write`, which changes
    /// what everyone sees on the account. Only the powerful tier also waits
    /// for the switch in Advanced.
    pub fn asks_first(self) -> bool {
        self.is_powerful() || matches!(self, Capability::ProfileWrite | Capability::Microphone)
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
            Capability::VoiceControl => "voice.control",
            Capability::AudioPlay => "audio.play",
            Capability::AudioSpeak => "audio.speak",
            Capability::Microphone => "microphone",
            Capability::Notifications => "notifications",
            Capability::ClipboardWrite => "clipboard.write",
            Capability::UiModify => "ui.modify",
            Capability::Network => "network",
            Capability::NetworkLocal => "network.local",
            Capability::OpenApp => "open.app",
            Capability::MessagesAttach => "messages.attach",
            Capability::MessagesSendAgent => "messages.send.agent",
            Capability::FsFolder => "fs.folder",
            Capability::FsFull => "fs.full",
            Capability::NetAny => "net.any",
            Capability::NetSocket => "net.socket",
            Capability::NetListen => "net.listen",
            Capability::ActivitySet => "activity.set",
            Capability::ProfileWrite => "profile.write",
            Capability::Keybinds => "keybinds",
            Capability::SoundsReplace => "sounds.replace",
            Capability::MessagesSearch => "messages.search",
            Capability::DraftsRead => "drafts.read",
            Capability::AccountSessions => "account.sessions",
            Capability::SpotifyControl => "spotify.control",
            Capability::LinksOffer => "links.offer",
            Capability::WindowTitle => "window.title",
            Capability::SwitcherRank => "switcher.rank",
            Capability::AppTiming => "app.timing",
            Capability::RelationshipsRead => "relationships.read",
            Capability::ImagesWeb => "images.web",
            Capability::ActivityRead => "activity.read",
            Capability::OAuthLogin => "oauth.login",
            Capability::Overlay => "overlay",
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
            Capability::UploadsIntercept => {
                "Can read and change files you upload, or send them somewhere else"
            }
            Capability::MediaRead => "Reads attachments and pictures",
            Capability::FilesSave => "Saves files where you choose",
            Capability::VoiceRead => "Sees your calls",
            Capability::VoiceControl => "Changes your microphone, speakers and their volumes",
            Capability::AudioPlay => "Plays sounds",
            Capability::AudioSpeak => "Speaks out loud",
            Capability::Microphone => {
                "Records your microphone when it asks, once you say yes, while a red bar says so"
            }
            Capability::Notifications => "Shows notifications",
            Capability::ClipboardWrite => "Writes to your clipboard",
            Capability::UiModify => "Can change Yapfast's interface",
            Capability::Network => "Connects to the sites it lists",
            Capability::NetworkLocal => "Connects to an app on this computer",
            Capability::OpenApp => "Opens other applications",
            Capability::MessagesAttach => "Sends files it makes with the messages it sends",
            Capability::MessagesSendAgent => {
                "Sends messages as you without you pressing a key, as an agent. Automated \
                 sending can get your Discord account banned."
            }
            Capability::FsFolder => "Can read the files in a folder you choose",
            Capability::FsFull => "Can read every file in your home folder",
            Capability::NetAny => "Can send your data to any website",
            Capability::NetSocket => "Can open raw connections to any server",
            Capability::NetListen => "Lets apps on this computer connect to it",
            Capability::ActivitySet => "Shows an activity of its own on your profile",
            Capability::ProfileWrite => {
                "Can change your bio, pronouns and custom status, once you say yes"
            }
            Capability::Keybinds => "Offers actions you can bind to keys",
            Capability::SoundsReplace => "Replaces Yapfast's sounds with its own",
            Capability::MessagesSearch => "Searches your messages",
            Capability::DraftsRead => {
                "Reads what you type before you send it, and the drafts you left"
            }
            Capability::AccountSessions => "Sees the devices signed in to your account",
            Capability::SpotifyControl => "Sees and controls what your Spotify plays",
            Capability::LinksOffer => "Offers to open links you click in other apps",
            Capability::WindowTitle => "Adds a few words to the window's title",
            Capability::SwitcherRank => {
                "Sees what you search in the quick switcher, and can reorder it"
            }
            Capability::AppTiming => "Reads how long Yapfast took to start",
            Capability::RelationshipsRead => {
                "Sees friends being added and removed, friend requests, and who Discord ranks you closest to"
            }
            Capability::ImagesWeb => "Shows pictures from the sites it lists",
            Capability::ActivityRead => "Sees which game you're playing, even when you hide it",
            Capability::Overlay => "Draws moving pictures over Yapfast's window",
            Capability::OAuthLogin => {
                "Asks you to sign in to another site in your browser, and uses that account"
            }
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
        ) || self.is_powerful()
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
    /// What `fs.folder` and `fs.full` may do: read, or read and write; and for
    /// `fs.full`, the home folder or the whole disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<FileAccess>,
    /// The loopback port `net.listen` serves on; none lets the system pick one,
    /// which the plugin's page shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen_port: Option<u16>,
    /// How many messages a minute `messages.send.agent` may send without a
    /// keystroke, 1 to [`Permissions::MAX_AGENT_PER_MINUTE`]; six when unsaid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_per_minute: Option<u8>,
    /// The hosts whose links `links.offer` is asked about: a link the person
    /// clicks to any other host never reaches the plugin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_hosts: Vec<HostPattern>,
    /// Whether `net.socket` may reach this computer's loopback too. Without it
    /// a connection goes to public addresses only, as `http.fetch` does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub socket_local: bool,
}

/// A manifest's `permissions.files`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileAccess {
    /// Whether the plugin may write too.
    #[serde(default)]
    pub write: bool,
    /// For `fs.full`: the home folder, or the whole disk.
    #[serde(default)]
    pub scope: FileScope,
}

/// How much `fs.full` reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileScope {
    #[default]
    Home,
    Disk,
}

impl Permissions {
    /// The most `agent_per_minute` may say.
    pub const MAX_AGENT_PER_MINUTE: u8 = 20;

    /// How many agent sends a minute this plugin may make.
    pub fn agent_per_minute(&self) -> u8 {
        self.agent_per_minute
            .unwrap_or(6)
            .clamp(1, Self::MAX_AGENT_PER_MINUTE)
    }

    /// Whether `fs.folder` or `fs.full` may write.
    pub fn writes_files(&self) -> bool {
        self.files.is_some_and(|files| files.write)
    }

    /// The powerful capabilities this declares, in the dialog's order.
    pub fn powerful(&self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|capability| capability.is_powerful() && self.allows(*capability))
            .collect()
    }

    /// The plain sentence the red install dialog shows for one powerful
    /// capability of this plugin, with what the manifest says it reaches.
    pub fn powerful_sentence(&self, capability: Capability) -> String {
        let write = self.writes_files();
        match capability {
            Capability::FsFolder if write => {
                "This plugin can read, change and delete every file in a folder you choose".into()
            }
            Capability::FsFolder => "This plugin can read every file in a folder you choose".into(),
            Capability::FsFull => {
                let what = match self.files.map(|files| files.scope).unwrap_or_default() {
                    FileScope::Home => "your home folder, Documents and Downloads included",
                    FileScope::Disk => "this computer's whole disk",
                };
                if write {
                    format!("This plugin can read, change and delete every file in {what}")
                } else {
                    format!("This plugin can read every file in {what}")
                }
            }
            Capability::NetAny => {
                "This plugin can send your data to any website, not only the ones it lists".into()
            }
            Capability::NetSocket if self.socket_local => {
                "This plugin can open raw connections to any server, and to apps on this computer"
                    .into()
            }
            Capability::NetSocket => "This plugin can open raw connections to any server".into(),
            Capability::NetListen => {
                "Apps on this computer that have its token can connect to this plugin, and do \
                 whatever it lets them"
                    .into()
            }
            Capability::MessagesSendAgent => format!(
                "This plugin can send messages as you without you pressing a key, up to {} a \
                 minute. Automated sending can get your Discord account banned.",
                self.agent_per_minute()
            ),
            Capability::ProfileWrite => {
                "This plugin can change your bio, your pronouns and your custom status, which \
                 everyone who opens your profile sees"
                    .into()
            }
            other => other.summary().to_owned(),
        }
    }

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
            [
                Capability::MessagesRead,
                Capability::FilesSave,
                Capability::Microphone,
                Capability::TypingIntercept
            ],
            "the microphone records now"
        );
        assert!(
            Capability::Microphone.is_implemented() && Capability::Microphone.asks_first(),
            "a recording is asked about the first time"
        );
        assert!(
            Capability::NetSocket.is_implemented(),
            "raw connections are built now"
        );
        assert!(
            Capability::TypingIntercept.is_implemented()
                && Capability::UploadsIntercept.is_implemented()
                && Capability::FilesSave.is_implemented()
                && Capability::VoiceRead.is_implemented(),
            "the typing and upload hooks, the save dialog and the voice reads are"
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
