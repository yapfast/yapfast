//! The preview's devices and camera (`src/ui.rs:2997`-`:3030`): the
//! system's audio devices listed on the core's blocking pool, as listing
//! starts the platform's audio service, which can block, then handed to
//! the core (`Application::preview_devices`); and the preview call's
//! camera filming into the feed the core gives, or stopped.

use iced::Task;
use yapfast_core::application::Effect;
use yapfast_lang::tr;

use crate::app::App;
use crate::intent::voice::Due;
use crate::message::{Message, Once};

/// The preview call's camera, while it films.
#[derive(Default)]
pub struct Camera(Option<yapfast_core::camera::Camera>);

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::ListDevices => {
            let listing = app.runtime.spawn_blocking(|| {
                yapfast_core::voice::list_devices().map_err(|error| error.to_string())
            });
            Task::perform(listing, |listed| {
                let listed =
                    listed.unwrap_or_else(|_| Err(tr!("Could not list audio devices.").into()));
                Message::Voice(Due::Devices(Once::new(listed)))
            })
        }
        Effect::PreviewCamera(filming) => {
            app.shown.voice.camera.0 = None;
            if let Some((camera, feed)) = filming {
                let sink = Box::new(feed.mirrored_sink());
                match yapfast_core::camera::Camera::open(camera.as_deref(), sink) {
                    Ok(camera) => app.shown.voice.camera.0 = Some(camera),
                    Err(error) => {
                        app.core.state.voice.error = Some(error.to_string());
                        app.show(crate::project::Reach::VOICE);
                    }
                }
            }
            Task::none()
        }
        effect => super::waiting(&effect),
    }
}

/// The devices listed for the preview reach the core.
pub fn listed(app: &mut App, listed: Once<Result<yapfast_core::voice::Devices, String>>) {
    if let Some(listed) = listed.take() {
        app.core.preview_devices(listed);
        app.refresh();
    }
}
