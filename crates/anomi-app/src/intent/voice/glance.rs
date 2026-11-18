//! The share menu's pictures of the screens (`Worker::glance`,
//! `src/ui/callbacks.rs:3166`): a stream worker captures each screen once,
//! at the card's size in the display's pixels, and the pictures come one by
//! one into the menu's cards. The worker runs only while the menu shows: it
//! starts as the menu opens, ends once the pictures are in, and goes when
//! the menu closes, so capturing the screens takes memory only meanwhile.

use std::sync::Arc;

use iced::Task;
use iced::futures::stream;
use iced::widget::image;
use yapfast_core::runtime::BackgroundRuntime;
use yapfast_core::stream_worker::{Report, Source, Worker};

use super::Due;
use crate::app::App;
use crate::message::Message;

/// A card's picture, in the view's pixels (Slint's 160 by 90).
const WIDTH: f32 = 160.0;
const HEIGHT: f32 = 90.0;

/// A picture of the screen listed as `id`, for the listing numbered so.
#[derive(Clone, Debug)]
pub struct Glanced {
    pub asked: u64,
    pub id: u64,
    pub picture: image::Handle,
}

/// The worker, told to stop as it goes. Stopping waits for it up to two
/// seconds, so it goes on a thread of its own, never the window's.
struct Glancing(Option<Worker>);

impl Drop for Glancing {
    fn drop(&mut self) {
        if let Some(worker) = self.0.take() {
            let _ = std::thread::Builder::new()
                .name("anomi-glance-stop".into())
                .spawn(move || drop(worker));
        }
    }
}

type Reports = tokio::sync::mpsc::UnboundedReceiver<Report>;

enum Glance {
    /// The worker starts as the task first runs, off the window's thread.
    Start(Arc<BackgroundRuntime>, Vec<u64>, u32, u32),
    Running(Glancing, Reports),
}

/// Pictures of `sources`' screens for the menu of listing `asked`; the
/// task's handle is kept while the menu shows, and dropping it stops the
/// worker.
pub fn start(
    app: &App,
    asked: u64,
    sources: &[Source],
) -> Option<(Task<Message>, iced::task::Handle)> {
    let ids: Vec<u64> = (sources.iter())
        .filter_map(|source| match source {
            Source::Display(id) => Some(*id),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return None;
    }
    let scale = anomi_widgets::picture::scale();
    let (width, height) = (
        (WIDTH * scale).ceil() as u32,
        (HEIGHT * scale).ceil() as u32,
    );
    let first = Glance::Start(app.runtime.clone(), ids, width, height);
    let pictures = stream::unfold(first, |glance| async move {
        let (worker, mut reports) = match glance {
            Glance::Start(runtime, ids, width, height) => {
                let started = runtime.spawn_blocking(move || Worker::glance(ids, width, height));
                match started.await {
                    Ok(Ok((worker, reports))) => (Glancing(Some(worker)), reports),
                    Ok(Err(error)) => {
                        tracing::warn!("no pictures of the screens to share: {error:#}");
                        return None;
                    }
                    Err(_) => return None,
                }
            }
            Glance::Running(worker, reports) => (worker, reports),
        };
        while let Some(report) = reports.recv().await {
            match report {
                Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                } => {
                    let picture = image::Handle::from_rgba(width, height, rgba);
                    return Some(((id, picture), Glance::Running(worker, reports)));
                }
                Report::Ended { error } => {
                    if let Some(error) = error {
                        tracing::warn!("no pictures of the screens to share: {error}");
                    }
                    return None;
                }
                _ => {}
            }
        }
        None
    });
    let (task, handle) = Task::run(pictures, move |(id, picture)| {
        Message::Voice(Due::Glanced(Glanced { asked, id, picture }))
    })
    .abortable();
    Some((task, handle.abort_on_drop()))
}

/// A picture came: it fills its screen's card, while the menu of its
/// listing shows.
pub fn glanced(app: &mut App, glanced: Glanced) -> Task<Message> {
    let shown = &app.shown.voice.share;
    if !shown.glancing(glanced.asked) {
        return Task::none();
    }
    let at = (shown.sources.iter()).position(|source| *source == Source::Display(glanced.id));
    if let Some(screen) = at.and_then(|at| app.vm.voice.share.screens.get_mut(at)) {
        screen.picture = Some(glanced.picture);
    }
    Task::none()
}
