//! The image viewer: what it shows and what it asks for. The Slint side:
//! `ImageViewer` and `ViewerMenu` (`ui/messages.slint:2610`, `:2563`), fed by
//! `show_viewer` (`src/ui/refresh.rs:5279`) and answered at
//! `src/ui/callbacks.rs:1146`, `:1159`.
//!
//! It opens as `Layer::Viewer` over the window. The picture is the full image
//! once it arrives and the preview until then; `size` is the full image's
//! (or the preview's) in pixels, which the view fits into the window without
//! enlarging it. The viewer's own state, the zoom, lives here too, as a
//! view-model's does: the app's `update` changes it. Its context menu is the
//! window's (`vm.menu`): the app fills it on [`Intent::Menu`].

use iced::widget::image;

use crate::menu::Spot;

/// The image viewer: what its screen draws. The window holds it as
/// `vm.chat.viewer`.
#[derive(Clone, Debug, PartialEq)]
pub struct Viewer {
    /// The message it shows a picture of, and which of its attachments.
    pub item: String,
    pub index: usize,
    /// The file's name, in the bar.
    pub name: String,
    /// The full image once it arrived, the preview until then; none while
    /// there is neither.
    pub picture: Option<image::Handle>,
    /// The image's size in pixels (the preview's until the full one is
    /// known), as the view lays it out. `(0, 0)` while unknown.
    pub size: (u32, u32),
    /// Which of the message's images this is and how many it has, for "2 / 3";
    /// none for a message's only image.
    pub place: Option<(usize, usize)>,
    /// While the full image loads, or why it can't show.
    pub note: Note,
    /// It's a link's preview, not a file of the message: it can't be saved.
    pub preview: bool,
    /// Its address on the web, which its menu copies and opens.
    pub link: Option<String>,
    /// How far in, as a multiple of the size that fits the window: 1 is
    /// the fit, the least there is.
    pub zoom: f32,
    /// What the Save button's tooltip says, "Save sunset.png" in the user's
    /// language (`@tr("Save {}", viewer-name)`): the app makes it with the
    /// name.
    pub save_tip: String,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            item: String::new(),
            index: 0,
            name: String::new(),
            picture: None,
            size: (0, 0),
            place: None,
            note: Note::None,
            preview: false,
            link: None,
            zoom: 1.0,
            save_tip: String::new(),
        }
    }
}

/// What a pill at the bottom says about the picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Note {
    #[default]
    None,
    /// The full image is on its way.
    Loading,
    /// Why it can't show.
    Failed(String),
}

/// The least and most the zoom takes, and one step of it.
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 8.0;
pub const ZOOM_STEP: f32 = 1.25;
/// What a click on the picture zooms to from the fit.
pub const ZOOM_CLICK: f32 = 2.0;

/// A change of the zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    /// Back to the fit.
    Reset,
    /// A click on the picture: in from the fit, back to it otherwise.
    Toggle,
}

impl Zoom {
    /// The zoom after this change, kept between [`ZOOM_MIN`] and [`ZOOM_MAX`].
    pub fn apply(self, zoom: f32) -> f32 {
        let next = match self {
            Self::In => zoom * ZOOM_STEP,
            Self::Out => zoom / ZOOM_STEP,
            Self::Reset => ZOOM_MIN,
            Self::Toggle if zoom > ZOOM_MIN => ZOOM_MIN,
            Self::Toggle => ZOOM_CLICK,
        };
        next.clamp(ZOOM_MIN, ZOOM_MAX)
    }
}

impl Viewer {
    /// Whether there is more than one picture to step through.
    pub fn steps(&self) -> bool {
        self.place.is_some()
    }

    /// "2 / 3", as the bar shows it.
    pub fn position(&self) -> Option<String> {
        self.place
            .map(|(place, count)| format!("{place} / {count}"))
    }

    /// Whether a right click on the picture opens its menu: always for a
    /// message's file, for a link's preview only with an address to offer.
    pub fn has_menu(&self) -> bool {
        !self.preview || self.link.is_some()
    }
}

/// The image viewer: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The close button, Escape, a click outside the picture.
    Close,
    /// The side buttons and the arrow keys: -1 the previous image, 1 the next.
    Step(isize),
    /// Save the file (`save-attachment`).
    Save { item: String, index: usize },
    /// Put the image on the clipboard (`copy-attachment`).
    CopyImage { item: String, index: usize },
    /// Put its address on the clipboard.
    CopyLink(String),
    /// Open its address in the browser.
    OpenLink(String),
    /// A right click on the picture, with the pointer in the window: the app
    /// opens the window's context menu there (`vm.menu`), with the rows of
    /// `anomi_chat::viewer::menu::rows`.
    Menu(Spot),
    /// The wheel, `+`, `-`, `0` and a click on the picture.
    Zoom(Zoom),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_and_out_step_by_a_quarter() {
        assert_eq!(Zoom::In.apply(2.0), 2.0 * ZOOM_STEP);
        assert_eq!(Zoom::Out.apply(2.0), 2.0 / ZOOM_STEP);
    }

    #[test]
    fn the_zoom_stays_between_the_fit_and_eight() {
        assert_eq!(Zoom::Out.apply(ZOOM_MIN), ZOOM_MIN);
        assert_eq!(Zoom::Out.apply(1.1), ZOOM_MIN);
        assert_eq!(Zoom::In.apply(ZOOM_MAX), ZOOM_MAX);
        assert_eq!(Zoom::In.apply(7.5), ZOOM_MAX);
    }

    #[test]
    fn reset_goes_back_to_the_fit() {
        assert_eq!(Zoom::Reset.apply(5.0), ZOOM_MIN);
        assert_eq!(Zoom::Reset.apply(ZOOM_MIN), ZOOM_MIN);
    }

    #[test]
    fn a_click_zooms_in_from_the_fit_and_back_from_anywhere_above() {
        assert_eq!(Zoom::Toggle.apply(ZOOM_MIN), ZOOM_CLICK);
        assert_eq!(Zoom::Toggle.apply(ZOOM_CLICK), ZOOM_MIN);
        assert_eq!(Zoom::Toggle.apply(3.7), ZOOM_MIN);
    }
}
