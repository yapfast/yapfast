//! Streams in windows of their own, which keep going while the main window is
//! closed to the tray: each takes its pictures on its own timer, and the
//! stream shows here and not in the call.

use yapfast_core::frames::Frames;

use super::*;

/// How often a window looks for a new picture, as `stream-tick` does.
const TICK: Duration = Duration::from_millis(33);

/// A stream's window, by whose stream it is in [`Adapter::pop_outs`].
pub(super) struct PopOut {
    window: StreamWindow,
    /// The stream's pictures, which a move to another server replaces.
    frames: Rc<RefCell<Option<Arc<Frames>>>>,
    /// Stops with the window.
    _ticks: slint::Timer,
}

impl PopOut {
    /// Opens a window on `watching`, which starts on `frame`, the last picture
    /// the call showed: a still screen may send no other for a while.
    fn open(
        adapter: &Rc<RefCell<Adapter>>,
        watching: &voice::Watching,
        frame: Option<slint::Image>,
    ) -> Result<Self, slint::PlatformError> {
        title_bar::frame_next_window();
        let window = StreamWindow::new()?;
        window.set_window_title(format!("{}'s screen", watching.name).into());
        if let Some(frame) = frame {
            window.set_frame(frame);
        }
        window.set_status(watch_status(watching));
        let user = watching.user.clone();
        window.on_stop(move || {
            shell::act(Action::Voice(voice::Command::StopWatching(user.clone())));
        });
        let (state, user) = (Rc::downgrade(adapter), watching.user.clone());
        window.window().on_close_requested(move || {
            let (state, user) = (state.clone(), user.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(adapter) = state.upgrade() {
                    Adapter::pop_in(&adapter, &user);
                }
            });
            slint::CloseRequestResponse::HideWindow
        });
        let frames = Rc::new(RefCell::new(watching.frames.clone()));
        let ticks = slint::Timer::default();
        let (weak, source) = (window.as_weak(), frames.clone());
        ticks.start(slint::TimerMode::Repeated, TICK, move || {
            if let Some(window) = weak.upgrade()
                && let Some(frame) = source
                    .borrow()
                    .as_ref()
                    .and_then(|frames| frames.take(stream_image))
            {
                window.set_frame(frame);
            }
        });
        window.show()?;
        Ok(Self {
            window,
            frames,
            _ticks: ticks,
        })
    }

    /// Follows the stream, which may have moved to another server.
    fn show(&self, watching: &voice::Watching) {
        self.window.set_status(watch_status(watching));
        self.frames.borrow_mut().clone_from(&watching.frames);
    }

    /// Closes the window, with the picture it showed.
    fn close(self) -> slint::Image {
        let _ = self.window.hide();
        self.window.get_frame()
    }
}

impl Adapter {
    /// Moves someone's stream to a window of its own, or back to the call.
    pub(super) fn toggle_pop_out(adapter: &Rc<RefCell<Self>>, user: &str) {
        if adapter.borrow().pop_outs.contains_key(user) {
            Self::pop_in(adapter, user);
            return;
        }
        let mut state = adapter.borrow_mut();
        let Some(watching) = state
            .app
            .state
            .voice
            .watching
            .iter()
            .find(|watching| watching.user == user)
            .cloned()
        else {
            return;
        };
        let frame = state
            .stream_row(user)
            .and_then(|row| state.models.stream_frames.row_data(row));
        match PopOut::open(adapter, &watching, frame) {
            Ok(opened) => {
                state.pop_outs.insert(user.to_owned(), opened);
            }
            Err(error) => tracing::warn!(%error, "could not open the stream's window"),
        }
        state.show_streams_in_window();
    }

    /// Closes a stream's window; the stream goes on in the call.
    pub(super) fn pop_in(adapter: &Rc<RefCell<Self>>, user: &str) {
        let mut state = adapter.borrow_mut();
        if let Some(popped) = state.pop_outs.remove(user) {
            let frame = popped.close();
            if let Some(row) = state.stream_row(user) {
                state.models.stream_frames.set_row_data(row, frame);
            }
            state.show_streams_in_window();
        }
    }

    /// Keeps the windows on the streams being watched: one whose stream
    /// stopped closes, even with the main window closed.
    pub(super) fn show_pop_outs(&mut self) {
        if self.pop_outs.is_empty() {
            return;
        }
        let watching = &self.app.state.voice.watching;
        self.pop_outs.retain(|user, popped| {
            match watching.iter().find(|watching| &watching.user == user) {
                Some(watching) => {
                    popped.show(watching);
                    true
                }
                None => {
                    let _ = popped.window.hide();
                    false
                }
            }
        });
    }

    fn show_streams_in_window(&mut self) {
        if let Some(window) = self.window.upgrade() {
            self.show_streams(&window);
            self.show_stage(&window);
        }
    }
}
