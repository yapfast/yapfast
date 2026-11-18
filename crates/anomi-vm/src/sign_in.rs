//! The sign-in screen and linking Discord: what it shows and what it asks
//! for. The Slint side: `SignInScreen`, `VerifyBanner` (`ui/sign-in.slint:8`,
//! `:119`) and `DiscordLinkPanel` (`ui/discord.slint:29`), filled by
//! `show_workspace` (`src/ui/refresh.rs:960`) and answered by
//! `src/ui/callbacks.rs:1308`, `:2685`, `:2704`. Task N09
//! (docs/iced/tasks/shell.md).
//!
//! The text of every field is held here, as the Slint window's in-out
//! properties were: the app keeps what was typed, projects it back, and
//! empties the password once a sign-in starts (`on_login`). Their `Debug`
//! leaves the password, the token and the recovery key out, so no log line
//! holds them.

use std::fmt;
use std::rc::Rc;

use iced::widget::qr_code;

/// The sign-in screen and linking Discord: what its screen draws. The window
/// holds it as `vm.sign_in`.
#[derive(Clone, PartialEq)]
pub struct SignIn {
    /// The app's name beside its icon at the top left (`BuildInfo.product`).
    pub product: String,
    /// A sign-in or a token check is under way: fields and buttons are off
    /// (`busy`).
    pub busy: bool,
    /// What the last attempt said: red, or grey while `busy` (`notice`).
    pub notice: String,
    /// A saved session may be tried again (`restore-available`).
    pub restore_available: bool,
    /// A build with Matrix signs in with a password; one without links
    /// Discord (`matrix-build`).
    pub matrix_build: bool,
    /// Where linking Discord has got to.
    pub link: Link,
    /// The token field is open under the link's buttons (`token-open`).
    pub token_open: bool,
    /// What was pasted in it (`token`).
    pub token: String,
    /// The graphics' notice over the card, with Dismiss (`graphics-notice`).
    pub graphics_notice: Option<String>,
    /// The Matrix sign-in's fields (`homeserver`, `username`, `password`).
    pub homeserver: String,
    pub username: String,
    pub password: String,
    /// The banner over the conversation of an unverified device.
    pub verify: Verify,
}

impl Default for SignIn {
    fn default() -> Self {
        Self {
            product: "Anomi".into(),
            busy: false,
            notice: String::new(),
            restore_available: false,
            matrix_build: false,
            link: Link::default(),
            token_open: false,
            token: String::new(),
            graphics_notice: None,
            homeserver: String::new(),
            username: String::new(),
            password: String::new(),
            verify: Verify::default(),
        }
    }
}

/// Where linking Discord has got to (`phase`, `ui/discord.slint:24`: 0 to 6).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Link {
    /// Not linking: the warning and the buttons (0).
    #[default]
    Idle,
    /// Getting a code (1).
    Starting,
    /// The code to scan, or none when it could not be drawn (2).
    Code(Option<Code>),
    /// The phone scanned it, as this name (3).
    Scanned(String),
    /// Linking failed, saying why (4).
    Failed(String),
    /// Discord's captcha waits in the browser (5).
    Captcha,
    /// Discord checks a pasted token (6).
    Checking,
}

/// The QR code Discord's phone app scans, made once from the address the
/// core hands over (`qr_code`, `src/ui/pictures.rs:13`) and shared by every
/// copy of the view-model.
#[derive(Clone, Debug)]
pub struct Code {
    address: String,
    data: Rc<qr_code::Data>,
    modules: usize,
}

impl Code {
    /// The code carrying `address`, or `None` when it does not fit one.
    pub fn new(address: &str) -> Option<Self> {
        Some(Self {
            address: address.to_owned(),
            data: Rc::new(qr_code::Data::new(address.as_bytes()).ok()?),
            // iced's `Data` keeps its width private; the same encoder says it.
            modules: qrcode::QrCode::new(address.as_bytes()).ok()?.width(),
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// Modules along one side of the code, without any quiet zone.
    pub fn modules(&self) -> usize {
        self.modules
    }

    pub fn data(&self) -> &qr_code::Data {
        &self.data
    }
}

impl PartialEq for Code {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

/// The banner above the conversation until this device is verified
/// (`VerifyBanner`, `ui/sign-in.slint:119`).
#[derive(Clone, Default, PartialEq)]
pub struct Verify {
    /// The banner is shown: the app sets it from `!trusted && !preview`
    /// (`ui/yapfast.slint:3464`); the conversation places `verify_banner`
    /// while it is set.
    pub unverified: bool,
    /// A recovery or a check is under way (`busy`).
    pub busy: bool,
    /// The recovery key as typed (`recovery`).
    pub recovery: String,
}

/// The sign-in screen and linking Discord: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The fields were edited: the whole text.
    Homeserver(String),
    Username(String),
    Password(String),
    /// Sign in with Matrix (`login`).
    Login,
    /// Try the saved session again (`restore`).
    Restore,
    /// Open the local preview (`explore-preview`).
    ExplorePreview,
    /// Show the code to scan (`link-discord`).
    LinkDiscord,
    /// Open the token field; the app focuses `sign_in::TOKEN`.
    OpenToken,
    /// The token field was edited.
    Token(String),
    /// Link with this token (`link-discord-token`).
    LinkToken(String),
    /// Open the captcha's page in the browser again (`open-discord-captcha`).
    OpenCaptcha,
    /// Put the graphics' notice away.
    DismissNotice,
    /// The recovery key was edited.
    Recovery(String),
    /// Verify with the recovery key (`recover`).
    Recover,
    /// Ask whether another client verified this device (`refresh-trust`).
    RefreshTrust,
}

/// A secret as a log line shows it: whether one was typed, never what.
pub(crate) struct Hidden<'a>(pub &'a str);

impl fmt::Debug for Hidden<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_empty() {
            "\"\""
        } else {
            "<hidden>"
        })
    }
}

impl fmt::Debug for SignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every field named, so a new one is a choice to show or hide.
        let Self {
            product,
            busy,
            notice,
            restore_available,
            matrix_build,
            link,
            token_open,
            token,
            graphics_notice,
            homeserver,
            username,
            password,
            verify,
        } = self;
        f.debug_struct("SignIn")
            .field("product", product)
            .field("busy", busy)
            .field("notice", notice)
            .field("restore_available", restore_available)
            .field("matrix_build", matrix_build)
            .field("link", link)
            .field("token_open", token_open)
            .field("token", &Hidden(token))
            .field("graphics_notice", graphics_notice)
            .field("homeserver", homeserver)
            .field("username", username)
            .field("password", &Hidden(password))
            .field("verify", verify)
            .finish()
    }
}

impl fmt::Debug for Verify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            unverified,
            busy,
            recovery,
        } = self;
        f.debug_struct("Verify")
            .field("unverified", unverified)
            .field("busy", busy)
            .field("recovery", &Hidden(recovery))
            .finish()
    }
}

impl fmt::Debug for Intent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn hidden(f: &mut fmt::Formatter<'_>, name: &str, secret: &str) -> fmt::Result {
            f.debug_tuple(name).field(&Hidden(secret)).finish()
        }
        match self {
            Self::Password(secret) => hidden(f, "Password", secret),
            Self::Token(secret) => hidden(f, "Token", secret),
            Self::LinkToken(secret) => hidden(f, "LinkToken", secret),
            Self::Recovery(secret) => hidden(f, "Recovery", secret),
            Self::Homeserver(text) => f.debug_tuple("Homeserver").field(text).finish(),
            Self::Username(text) => f.debug_tuple("Username").field(text).finish(),
            Self::Login => f.write_str("Login"),
            Self::Restore => f.write_str("Restore"),
            Self::ExplorePreview => f.write_str("ExplorePreview"),
            Self::LinkDiscord => f.write_str("LinkDiscord"),
            Self::OpenToken => f.write_str("OpenToken"),
            Self::OpenCaptcha => f.write_str("OpenCaptcha"),
            Self::DismissNotice => f.write_str("DismissNotice"),
            Self::Recover => f.write_str("Recover"),
            Self::RefreshTrust => f.write_str("RefreshTrust"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Intent, SignIn, Verify};
    use crate::settings::mfa::{self, Mfa};

    const SECRET: &str = "mfa.hunter2-secret";

    #[test]
    fn no_secret_reaches_a_log_line() {
        let draft = SignIn {
            token: SECRET.into(),
            password: SECRET.into(),
            username: "sam".into(),
            verify: Verify {
                recovery: SECRET.into(),
                ..Verify::default()
            },
            ..SignIn::default()
        };
        let prompt = Mfa {
            code: SECRET.into(),
            ..Mfa::default()
        };
        let lines = [
            format!("{draft:?}"),
            format!("{prompt:?}"),
            format!("{:?}", Intent::Password(SECRET.into())),
            format!("{:?}", Intent::Token(SECRET.into())),
            format!("{:?}", Intent::LinkToken(SECRET.into())),
            format!("{:?}", Intent::Recovery(SECRET.into())),
            format!("{:?}", mfa::Intent::Write(SECRET.into())),
            format!("{:?}", crate::Intent::SignIn(Intent::Token(SECRET.into()))),
        ];
        for line in &lines {
            assert!(!line.contains("hunter2"), "{line}");
            assert!(line.contains("<hidden>"), "{line}");
        }
        assert!(lines[0].contains("\"sam\""), "the rest shows");
    }
}
