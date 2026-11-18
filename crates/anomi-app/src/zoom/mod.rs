//! Zooming the whole window, as browsers do: Ctrl with + or -, or with the
//! wheel, steps through the levels of `anomi_vm::shell::zoom`, and Ctrl+0
//! returns to 100%. The app's half of `src/zoom.rs` and `watch_window`
//! (`src/ui.rs:391`): [`keys`] catches what asks, the level becomes the
//! window's scale factor (`App::scale_factor`, which iced multiplies with the
//! display's), the layout keeps it, and the window shows the level for a
//! moment ([`NOTE`]).
//!
//! Pictures are copied at the display's scale times the level, which iced's
//! `Rescaled` leaves out: this is the one caller of
//! `anomi_widgets::picture::set_scale`, as the window opens, rescales and
//! zooms, before the next layout. The backend is told the density the
//! pictures it fetches need when it changes (`zoom_window`).

mod keys;

pub use keys::keys;

use std::time::Duration;

use iced::{Size, Task, window};
use yapfast_core::application::Action;
use yapfast_core::chat::Density;

use anomi_vm::shell::zoom::{self, LEVELS, Step};

use crate::app::{App, later};
use crate::message::Message;

/// How long the level shows after it changes.
const NOTE: Duration = Duration::from_millis(1200);

pub struct Zoom {
    /// One of the levels; 100% unless changed.
    level: f32,
    /// The display's own scale factor, which the level multiplies.
    base: f32,
    /// The note's number, so only the last change's wait takes it away.
    note: u64,
    /// The density last told to the core.
    density: Option<Density>,
}

impl Zoom {
    /// The saved level, no further in than a window of `size` (in the
    /// display's logical pixels) lays out in.
    pub fn new(saved: Option<f32>, size: Size) -> Self {
        let level = zoom::saved(saved)
            .min(zoom::most(size.width, size.height, 1.0))
            .max(LEVELS[0]);
        Self {
            level,
            base: 1.0,
            note: 0,
            density: None,
        }
    }

    /// The window's scale factor over the display's.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// The display's scale factor; 1 until a window says it.
    pub fn base(&self) -> f32 {
        self.base
    }

    /// The display's scale factor, once a window said it.
    pub fn base_known(&self) -> Option<f32> {
        self.density.is_some().then_some(self.base)
    }
}

/// A step asked: the window's size first, to know how far in it may go.
pub fn step(app: &App, step: Step) -> Task<Message> {
    app.window.map_or_else(Task::none, |id| {
        window::size(id).map(move |size| Message::Zoomed(step, size))
    })
}

/// The step, at the window's size in its logical pixels.
pub fn zoomed(app: &mut App, step: Step, size: Size) -> Task<Message> {
    let zoom = &mut app.zoom;
    // The size iced gives is the display's pixels over both scales.
    let scale = zoom.base * zoom.level;
    let most = zoom::most(size.width * scale, size.height * scale, zoom.base);
    let next = zoom::stepped(zoom.level, step, most);
    zoom.note += 1;
    app.vm.shell.zoom = Some(zoom::note(next));
    let note = later(NOTE, Message::ZoomNoteGone(app.zoom.note));
    if next == app.zoom.level {
        return note;
    }
    app.zoom.level = next;
    app.layout.edit(|saved| saved.zoom = Some(next));
    Task::batch([note, rescale(app)])
}

/// The note's moment ended, unless a later change showed it again.
pub fn note_gone(app: &mut App, note: u64) {
    if note == app.zoom.note {
        app.vm.shell.zoom = None;
    }
}

/// The display's scale factor, as the window opens and as it changes.
pub fn rescaled(app: &mut App, base: f32) -> Task<Message> {
    if base.is_finite() && base > 0.0 {
        app.zoom.base = base;
    }
    rescale(app)
}

/// Pictures are copied at the new scale, and the backend fetches them at
/// its density.
fn rescale(app: &mut App) -> Task<Message> {
    let scale = app.zoom.base * app.zoom.level;
    let was = anomi_widgets::picture::scale();
    anomi_widgets::picture::set_scale(scale);
    // The video's pictures come at the new number of pixels (I09).
    let video = if was == scale {
        Task::none()
    } else {
        crate::media::rescaled(app)
    };
    let density = Density::of_scale(scale);
    if app.zoom.density.replace(density) == Some(density) {
        return video;
    }
    Task::batch([video, app.dispatch(Action::PreviewDensity(density))])
}

#[cfg(test)]
mod tests {
    use iced::Size;

    use anomi_vm::shell::zoom::Step;

    use crate::app::App;

    #[test]
    fn a_step_zooms_shows_the_level_and_is_kept() {
        let folder = std::env::temp_dir().join(format!("anomi-zoom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut app = App::for_tests(&folder);
        assert_eq!(app.zoom.level(), 1.0);
        drop(super::zoomed(&mut app, Step::In, Size::new(1400.0, 860.0)));
        assert_eq!(app.zoom.level(), 1.05);
        assert_eq!(app.vm.shell.zoom.as_deref(), Some("105%"));
        assert_eq!(app.layout.zoom, Some(1.05));
        let first = app.zoom.note;
        // A window too small to zoom further in shows the level it stays at.
        drop(super::zoomed(&mut app, Step::In, Size::new(640.0, 560.0)));
        assert_eq!(app.zoom.level(), 1.05);
        super::note_gone(&mut app, first);
        assert!(app.vm.shell.zoom.is_some(), "a later change shows it still");
        let last = app.zoom.note;
        super::note_gone(&mut app, last);
        assert_eq!(app.vm.shell.zoom, None);
        drop(super::zoomed(
            &mut app,
            Step::Reset,
            Size::new(1400.0, 860.0),
        ));
        assert_eq!(app.zoom.level(), 1.0);
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_saved_level_fits_the_window() {
        assert_eq!(
            super::Zoom::new(Some(1.2), Size::new(1280.0, 800.0)).level(),
            1.2
        );
        assert_eq!(
            super::Zoom::new(Some(1.13), Size::new(1280.0, 800.0)).level(),
            1.0
        );
        let small = super::Zoom::new(Some(1.5), Size::new(700.0, 560.0)).level();
        assert_eq!(small, 1.0, "no further in than the window lays out in");
    }
}
