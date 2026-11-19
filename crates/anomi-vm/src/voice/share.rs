//! The share menu: what it shows and what it asks for. Task V10
//! (docs/iced/tasks/voice.md). The Slint side is `ShareMenu` and `ShareScreen`
//! (`ui/voice.slint:1414`, `:1359`), the window's `share-menu` popup and its
//! picks (`ui/yapfast.slint:2115`-`:2180`), and the handlers
//! `list_share_sources`, `stream_sound_changed` and `go_live`
//! (`src/ui/callbacks.rs:3108`-`:3302`).
//!
//! The menu lists what can be shared, in the order the Slint picks count
//! them: the screens, then the windows, then (where the system's dialog picks
//! one) the dialog, then the cameras. The picks themselves (which source, the
//! resolution, the frame rate, the quality, the sound) live here, so a click
//! changes them in the app and the menu is projected again; "Go Live" and
//! "Switch" carry them to the core.

use iced::widget::image;

use crate::menu::Row;

/// A screen in the menu, and once it's been glanced at, a picture of it
/// (`ShareScreen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub title: String,
    /// `None` until the picture comes.
    pub picture: Option<image::Handle>,
}

/// The size a stream goes out at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Resolution {
    P720,
    #[default]
    P1080,
    /// As the source is.
    Source,
}

impl Resolution {
    pub const ALL: [Self; 3] = [Self::P720, Self::P1080, Self::Source];
}

/// The frame rates offered.
pub const FRAME_RATES: [u32; 3] = [15, 30, 60];

/// The sound shared: a choice changes it at once, live or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sound {
    /// Every application's.
    #[default]
    All,
    None,
    /// The application whose window `Share::sound_apps` lists at that index.
    App(usize),
}

/// The share menu: what its screen draws. The window holds it as
/// `vm.voice.share`.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub screens: Vec<Screen>,
    pub windows: Vec<String>,
    pub cameras: Vec<String>,
    /// The system's dialog picks a screen or window: with cameras listed, a
    /// choice of it comes before them; with nothing listed it is the only
    /// way.
    pub dialog: bool,
    /// Live: the button switches what's shared, and a choice of sound,
    /// resolution, frame rate or encoding changes the stream at once.
    pub switching: bool,
    /// What's picked to share: an index in screens, windows, the dialog and
    /// cameras, counted in that order. `None` while live, until something to
    /// switch to is picked.
    pub source: Option<usize>,
    pub resolution: Resolution,
    pub fps: u32,
    /// The encoder's slower preset: a sharper picture for more CPU.
    pub high_quality: bool,
    pub sound: Sound,
    /// On Windows, the windows whose application's sound alone can be
    /// shared; elsewhere none, and no such choice.
    pub sound_apps: Vec<String>,
    /// The list of applications under "One application" is open.
    pub apps_open: bool,
    /// The choices as menu rows, made by `anomi_voice::share::built` (the
    /// text is translated there): a row's view borrows its row, so the rows
    /// live here, beside what they were made from. The projection and a shot
    /// call it after setting the other fields.
    pub rows: Rows,
}

/// The menu's choices as rows (`MenuItem`), each sending its [`Intent`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    pub windows: Vec<Row<Intent>>,
    /// "Screen or window", picked in the system's dialog.
    pub dialog: Option<Row<Intent>>,
    pub cameras: Vec<Row<Intent>>,
    /// All applications, No sound, and One application where there is one.
    pub sound: Vec<Row<Intent>>,
    /// Each application whose sound can be shared alone.
    pub apps: Vec<Row<Intent>>,
    pub resolutions: Vec<Row<Intent>>,
    pub rates: Vec<Row<Intent>>,
    pub quality: Option<Row<Intent>>,
}

impl Default for Share {
    fn default() -> Self {
        Self {
            screens: Vec::new(),
            windows: Vec::new(),
            cameras: Vec::new(),
            dialog: false,
            switching: false,
            source: Some(0),
            resolution: Resolution::default(),
            fps: 30,
            high_quality: false,
            sound: Sound::default(),
            sound_apps: Vec::new(),
            apps_open: false,
            rows: Rows::default(),
        }
    }
}

impl Share {
    /// Where the dialog's choice sits among the sources.
    pub fn dialog_index(&self) -> usize {
        self.screens.len() + self.windows.len()
    }

    /// Where the first camera sits.
    pub fn first_camera(&self) -> usize {
        self.dialog_index() + usize::from(self.dialog)
    }

    /// The dialog is a choice of its own, listed before the cameras.
    pub fn asks_dialog(&self) -> bool {
        self.dialog && !self.cameras.is_empty()
    }

    /// Screens, windows or cameras are listed.
    pub fn lists(&self) -> bool {
        self.screens.len() + self.windows.len() + self.cameras.len() > 0
    }

    /// The menu is the wider one, for the screens' pictures.
    pub fn wide(&self) -> bool {
        !self.screens.is_empty()
    }

    /// What "Go Live" or "Switch" sends, or `None` with nothing to send.
    pub fn go(&self) -> Option<Intent> {
        let source = self
            .source
            .or(if self.switching { None } else { Some(0) })?;
        Some(if self.switching {
            Intent::Switch(source)
        } else {
            Intent::GoLive {
                source,
                resolution: self.resolution,
                fps: self.fps,
                high_quality: self.high_quality,
            }
        })
    }

    /// "Go Live" always goes; "Switch" waits for something to switch to.
    pub fn can_go(&self) -> bool {
        !self.switching || self.source.is_some()
    }
}

/// The share menu: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Pick what to share, by its index among the sources.
    Source(usize),
    Resolution(Resolution),
    Fps(u32),
    HighQuality(bool),
    /// Choose the sound; the app applies it at once, as `stream_sound_changed`
    /// does, and keeps the choice.
    Sound(Sound),
    /// Open or close the list of applications.
    Apps(bool),
    /// Start the stream: the source, the quality and the encoder's preset
    /// (`go_live`); the menu closes.
    GoLive {
        source: usize,
        resolution: Resolution,
        fps: u32,
        high_quality: bool,
    },
    /// While live, share what's picked instead (`switch_live`); the menu
    /// closes.
    Switch(usize),
}
