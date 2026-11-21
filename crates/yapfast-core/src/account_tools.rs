//! What Settings > My Account lists and changes: the apps the account
//! authorized, the devices it is signed in on, the services it linked, and
//! the username, email and password forms. Read when a tab opens. See
//! "Account settings" in docs/discord/account-settings.md.

/// A list read from Discord when its tab opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
}

impl<T> Default for Listing<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            loading: false,
            loaded: false,
            error: None,
        }
    }
}

impl<T> Listing<T> {
    /// Takes what Discord answered to the read.
    pub fn finish(&mut self, result: Result<Vec<T>, String>) {
        self.loading = false;
        match result {
            Ok(items) => {
                self.items = items;
                self.loaded = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

/// An app this account let into it; revoking takes its authorization back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorizedApp {
    /// The authorization's id, not the app's.
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// What it may do, as Discord names the scopes.
    pub scopes: Vec<String>,
}

/// A device the account is signed in on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// The hash of its session.
    pub id: String,
    pub os: String,
    pub platform: String,
    pub location: String,
    /// When it last used the account, in Unix seconds; 0 when unknown.
    pub last_used: u64,
    /// Whether it is the device Yapfast runs on.
    pub current: bool,
}

/// A service the account linked, and how it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Connection {
    pub service: String,
    pub label: String,
    pub id: String,
    pub name: String,
    pub verified: bool,
    pub visible: bool,
    pub show_activity: bool,
    pub revoked: bool,
}

impl Connection {
    /// What a request about this account is kept by while it goes.
    pub fn key(&self) -> String {
        key(&self.service, &self.id)
    }
}

/// The services Yapfast offers to link an account of, by Discord's id for
/// them and their name: the page that links one is asked of Discord and
/// opened in the browser.
pub const SERVICES: [(&str, &str); 12] = [
    ("spotify", "Spotify"),
    ("twitch", "Twitch"),
    ("youtube", "YouTube"),
    ("steam", "Steam"),
    ("github", "GitHub"),
    ("reddit", "Reddit"),
    ("twitter", "X (Twitter)"),
    ("xbox", "Xbox"),
    ("battlenet", "Battle.net"),
    ("epicgames", "Epic Games"),
    ("facebook", "Facebook"),
    ("tiktok", "TikTok"),
];

/// What a request about a connected account is kept by while it goes.
pub fn key(service: &str, id: &str) -> String {
    format!("{service}/{id}")
}

/// What My Account changes with the account's password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountField {
    Username,
    Email,
    Password,
}

/// The change under way, or the last one's end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountChange {
    /// What Discord is changing now.
    pub busy: Option<AccountField>,
    /// Why Discord didn't change it.
    pub error: Option<String>,
    /// What Discord changed last, which the form says.
    pub done: Option<AccountField>,
}

/// The lists and forms of My Account's tabs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountTools {
    pub apps: Listing<AuthorizedApp>,
    pub devices: Listing<Device>,
    pub connections: Listing<Connection>,
    /// What is being revoked, signed out or unlinked, until Discord answers:
    /// an authorization's id, a session's hash or a connection's [`key`].
    pub busy: Vec<String>,
    /// Why the last revoking, signing out or unlinking was refused.
    pub error: Option<String>,
    pub change: AccountChange,
}
