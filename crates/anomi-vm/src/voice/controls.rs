//! The call controls: what they show and what they ask for. Task V05
//! (docs/iced/tasks/voice.md). The Slint side is `CallControls`
//! (`ui/voice.slint:1121`), its properties filled where the stage is
//! (`ui/yapfast.slint:3537`) and its callbacks answered at
//! `src/ui/callbacks.rs:3073` (mute), `:3087` (deafen), `:3591` (share) and
//! `:3749` (camera).
//!
//! The pill floats at the foot of the stage; the projection hands it the
//! booleans below and the app answers each [`Intent`] with the voice
//! command, or, with no call yet, with the choice to join muted or deaf.

use crate::menu::Spot;

/// The soundboard and the share menu open this far left of their button
/// (`root.menu-x - 6px`, `ui/yapfast.slint:2135`, `:2148`).
pub const MENU_SHIFT: f32 = 6.0;

/// The call controls: what its screen draws. The window holds it as
/// `vm.voice.controls`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Controls {
    pub muted: bool,
    pub deafened: bool,
    /// Connected or reconnecting: the sound, camera, share and soundboard
    /// buttons work (`voice-ready`).
    pub ready: bool,
    /// Leaving: the hang-up waits, faded.
    pub leaving: bool,
    pub can_share: bool,
    pub sharing: bool,
    /// The call carries camera video.
    pub can_film: bool,
    pub camera_on: bool,
    /// The camera is on its way, until its first picture.
    pub camera_starting: bool,
    /// No camera to film with: the core listed none. The camera button
    /// looks unavailable and says so; a click still asks
    /// [`Intent::ToggleCamera`], which the app answers.
    pub camera_unavailable: bool,
    /// The call has Discord's soundboard.
    pub can_soundboard: bool,
}

/// The call controls: what a click or a key on them asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    ToggleMute,
    ToggleDeafen,
    /// Turn the camera on, or off (a click while it starts cancels it).
    ToggleCamera,
    /// Stop sharing the screen; starting is from [`Intent::OpenShareMenu`].
    StopSharing,
    /// Sharing starts from a menu opened at the button.
    OpenShareMenu(Spot),
    /// The soundboard, opened above its button.
    OpenSoundboard(Spot),
    Leave,
}
