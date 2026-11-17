//! The sign-in screen and linking Discord: what it shows and what it asks
//! for. The Slint side: `SignInScreen`, `VerifyBanner` (`ui/sign-in.slint:8`,
//! `:119`) and `DiscordLinkPanel` (`ui/discord.slint:29`), filled by
//! `show_workspace` (`src/ui/refresh.rs:960`) and answered by
//! `src/ui/callbacks.rs:1308`, `:2685`, `:2704`. Task N09
//! (docs/iced/tasks/shell.md).
//!
//! The text of every field is held here, as the Slint window's in-out
//! properties were: the app keeps what was typed, projects it back, and
//! empties the password once a sign-in starts (`on_login`).

use std::rc::Rc;

use iced::widget::qr_code;

/// The sign-in screen and linking Discord: what its screen draws. The window
/// holds it as `vm.sign_in`.
#[derive(Clone, Debug, PartialEq)]
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
}

impl Code {
    /// The code carrying `address`, or `None` when it does not fit one.
    pub fn new(address: &str) -> Option<Self> {
        Some(Self {
            address: address.to_owned(),
            data: Rc::new(qr_code::Data::new(address.as_bytes()).ok()?),
        })
    }

    pub fn address(&self) -> &str {
        &self.address
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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
