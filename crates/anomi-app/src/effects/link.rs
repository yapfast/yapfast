//! Linking Discord: `LinkDiscord`, `LinkDiscordToken` and `StopDiscordLink`
//! (`execute` and `link_discord`, `src/ui.rs:3068`, `:3133`). The work runs
//! on the core's runtime (`discord_link::start`); its progress comes to the
//! window as [`Message::Linking`], followed by an abortable task that stops
//! with the run. Each run is numbered, so the last words of one replaced
//! (a new code after one expired, a token after the code) are dropped.

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::message::Message;

/// The linking under way, if any: dropping it stops the work and the
/// following of its progress.
#[derive(Default)]
pub struct Linking {
    /// The run whose progress counts.
    run: u64,
    #[cfg(feature = "discord")]
    current: Option<Current>,
}

#[cfg(feature = "discord")]
struct Current {
    _work: yapfast_core::runtime::BackgroundTask<()>,
    _follow: iced::task::Handle,
}

impl Linking {
    /// The run whose progress counts now.
    pub fn run(&self) -> u64 {
        self.run
    }

    /// Stops the linking under way; what it said meanwhile is dropped.
    fn stop(&mut self) {
        self.run += 1;
        #[cfg(feature = "discord")]
        {
            self.current = None;
        }
    }
}

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::LinkDiscord => start(app, None),
        Effect::LinkDiscordToken(token) => start(app, Some(token)),
        Effect::StopDiscordLink => {
            app.linking.stop();
            Task::none()
        }
        effect => super::waiting(&effect),
    }
}

/// Starts linking, by QR code or with a pasted token, in place of any
/// linking under way.
#[cfg(feature = "discord")]
fn start(app: &mut App, token: Option<String>) -> Task<Message> {
    app.linking.stop();
    let run = app.linking.run;
    let (work, progress) = yapfast_core::discord_link::start(&app.runtime, token);
    let steps = iced::futures::stream::unfold(progress, |mut progress| async move {
        let step = progress.recv().await?;
        Some((step, progress))
    });
    let (follow, handle) = Task::run(steps, move |step| Message::Linking(run, step)).abortable();
    app.linking.current = Some(Current {
        _work: work,
        _follow: handle.abort_on_drop(),
    });
    follow
}

#[cfg(not(feature = "discord"))]
fn start(_app: &mut App, _token: Option<String>) -> Task<Message> {
    Task::none()
}

/// A step of the linking run `run`, unless another replaced it: the core
/// takes it, and the sign-in shows where it got to.
#[cfg(feature = "discord")]
pub fn progress(
    app: &mut App,
    run: u64,
    step: yapfast_core::discord_link::Progress,
) -> Task<Message> {
    if run != app.linking.run {
        return Task::none();
    }
    let effects = app.core.linking(step);
    let task = super::run(app, effects);
    app.refresh();
    task
}
