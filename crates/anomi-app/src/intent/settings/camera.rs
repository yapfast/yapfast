//! Voice & video's camera test (`test-camera`, `start_camera_test`,
//! `src/ui/callbacks.rs:3828`, `:4157`; `camera-filming`,
//! `ui/yapfast.slint:2042`): the camera films for the preview only while the
//! page shows in front with the test on. Its frames reach the preview's
//! `Live` through the media task (`crate::media`), each a redraw of its
//! rectangle and no view; behind another window, on another page or with
//! Settings closed the camera closes, and the test films again as the page
//! shows again.

use yapfast_core::camera::Camera;
use yapfast_core::picture::Feed;

use anomi_vm::kind::Live;
use anomi_vm::settings::frame::Page;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::media::Source;

/// The preview's size, in the view's pixels (`400 x 225`).
const PREVIEW: (f32, f32) = (400.0, 225.0);

/// The camera filming for the test, or that it couldn't, for the device
/// chosen then.
pub struct Filming {
    _camera: Option<Camera>,
    device: Option<String>,
}

/// After each update: films while the test shows, and stops otherwise.
pub(super) fn follow(app: &mut App) {
    let page = &app.vm.settings.voice;
    let wanted = app.vm.shell.is_open(Layer::Settings)
        && app.vm.settings.frame.page == Page::Voice
        && page.camera_testing
        && app.motion.front
        && !app.hidden;
    let device = &app.core.state.voice.camera;
    let kept = &mut app.shown.settings.kept;
    match (&kept.camera, wanted) {
        (Some(filming), true) if filming.device == *device => return,
        (None, false) => return,
        _ => {}
    }
    kept.camera = None;
    crate::media::follow_test(&mut app.shown.media, None);
    let page = &mut app.vm.settings.voice;
    if !wanted {
        if !page.camera_testing {
            page.camera_note.clear();
        }
        return;
    }
    let feed = Feed::new();
    let scale = anomi_widgets::picture::scale();
    let pixels = |side: f32| (side * scale).round() as u32;
    feed.show_at(Some((pixels(PREVIEW.0), pixels(PREVIEW.1))));
    let camera = match Camera::open(device.as_deref(), Box::new(feed.sink())) {
        Ok(camera) => {
            let live = Live::default();
            page.camera_preview = live.clone();
            page.camera_note.clear();
            crate::media::follow_test(&mut app.shown.media, Some(Source::Feed(feed, live)));
            Some(camera)
        }
        Err(error) => {
            tracing::warn!(%error, "could not start the camera's test");
            page.camera_note = error.to_string();
            None
        }
    };
    app.shown.settings.kept.camera = Some(Filming {
        _camera: camera,
        device: device.clone(),
    });
}
