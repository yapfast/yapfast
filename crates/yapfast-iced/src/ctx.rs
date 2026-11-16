//! What a screen reads and does besides its own state.
//!
//! A view reads a [`Ctx`]: the look, the pictures made to draw, the core's
//! state and the root's, borrowed together and passed by value, so a
//! closure kept in an element can hold it. `App::ctx` makes one.
//!
//! An update works through a [`Cx`]: it dispatches to the core, changes
//! what `layout.json` keeps, and asks the root for anything outside the
//! screen with a [`Request`]. It never syncs: after each message the root
//! runs one pass (`App::settle`) that takes the requests, works out the
//! theme shown, syncs while anything changed, moves the focus and places
//! the videos. `cx!(self)` makes one from the App's fields.

use std::sync::Arc;

use yapfast_core::application::{Action, AppState, Application};

use crate::{
    account::Account, client::Client, keybinds::Keybinds, layout::Layout, look::Look,
    pictures::Pictures, settings::Build, theme::Theme, themes::Themes, video,
};

#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub look: Look<'a>,
    /// The pictures made to draw, and the moving ones playing.
    pub pictures: &'a Pictures,
    /// The core, and its state.
    pub app: &'a Application,
    pub state: &'a AppState,
    /// Whether moving pictures play (`App::plays`).
    pub plays: bool,
    /// What `layout.json` keeps, and the themes it offers.
    pub layout: &'a Layout,
    pub themes: &'a Themes,
    /// The account's card, as the rail's foot shows it.
    pub account: &'a Account,
    /// Which build this is.
    pub build: &'a Build,
    /// The window's video sources, and its pixels to the display's.
    pub video: &'a Arc<video::Shared>,
    pub scale: f32,
    /// The keybinds' watcher and what it said of where they work.
    pub keybinds: &'a Keybinds,
    /// Yapfast draws the window's title bar itself.
    pub own_title_bar: bool,
    /// The desktop has a tray to close the window to.
    pub tray: bool,
    /// Linking Discord's form, which the sign-in screen and Settings'
    /// Discord page show.
    pub sign_in: &'a crate::signin::SignIn,
}

/// What a screen asks of the root, after its update: the whole list of
/// what screens may ask of each other, in one place.
#[derive(Clone, Debug)]
pub enum Request {
    /// The screen that asked closes, its own close run.
    CloseSelf,
    /// The menu open, if any, closes.
    CloseMenus,
    /// The call's popups close, and the signal's pointer is let go.
    ClosePopups,
    /// Where the keys go next; the last asked wins.
    Focus(Focus),
    /// A preference changed that others show: each screen hears of it.
    Changed(Pref),
    /// The window's title bar: the system's, or Yapfast's own.
    TitleBar(bool),
    /// A look at the releases, telling of a newer one if `true`.
    LookForUpdates(bool),
}

/// Where the keys go.
#[derive(Clone, Debug)]
pub enum Focus {
    /// Where they go with nothing open: the composer, or nothing.
    Default,
    /// Nowhere, so no field takes them (`app::NOBODY`).
    Nobody,
}

/// A preference others show, changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pref {
    /// Message times read on another clock.
    Clock,
    /// The timeline's rows look otherwise: text size, link previews,
    /// collapsing long messages.
    Rows,
    /// The member list's rows look otherwise: boost gems.
    Roster,
    /// Push to talk's key, the input mode or the Keybinds page's: the
    /// keybinds are watched again.
    Keybinds,
    /// Going idle after a while, or not.
    Idle,
    /// The theme chosen, or the one a screen previews: the window shows
    /// it again.
    Theme,
    /// Noise suppression chosen: the call's switch puts back the last one
    /// that wasn't off.
    Noise,
}

/// What an update works through: the core, what `layout.json` keeps, the
/// themes, and the requests it makes of the root.
pub struct Cx<'a> {
    pub client: &'a mut Client,
    pub layout: &'a mut Layout,
    pub themes: &'a mut Themes,
    /// The theme shown, to read.
    pub theme: &'a Theme,
    /// The window's pixels to the display's.
    pub scale: f32,
    pub build: &'a Build,
    pub requests: &'a mut Vec<Request>,
    /// Something changed that the window shows: sync after the update.
    pub dirty: &'a mut bool,
}

impl Cx<'_> {
    pub fn state(&self) -> &AppState {
        self.client.state()
    }

    /// Hands `action` to the core; the window syncs after the update.
    pub fn dispatch(&mut self, action: Action) {
        self.client.dispatch(action);
        *self.dirty = true;
    }

    /// Something the window shows changed without a dispatch.
    pub fn touch(&mut self) {
        *self.dirty = true;
    }

    pub fn request(&mut self, request: Request) {
        self.requests.push(request);
    }

    /// Keeps `value` as `key` in `layout.json`.
    pub fn keep(&self, key: &str, value: impl serde::Serialize) {
        crate::layout::store(self.client.root(), key, value);
    }
}

/// A [`Cx`] over `$app`'s fields, borrowed apart so the screens beside
/// them can be borrowed too.
macro_rules! cx {
    ($app:expr) => {{
        let scale = $app.device_scale();
        $crate::ctx::Cx {
            client: &mut $app.client,
            layout: &mut $app.layout,
            themes: &mut $app.themes,
            theme: &$app.theme,
            scale,
            build: &$app.build,
            requests: &mut $app.requests,
            dirty: &mut $app.dirty,
        }
    }};
}
pub(crate) use cx;
