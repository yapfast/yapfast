//! A single instance: a second start on the data folder brings the running
//! one's window forward, with a link it was handed, and ends
//! (`anomi_desktop::instance`, shared with the Slint app, so either hands a
//! start to the other). `launched` in `src/ui/shell.rs`.

use std::path::Path;

use anomi_desktop::instance::{self, Instance, Launch};
use iced::Task;

use crate::app::App;
use crate::cli::Start;
use crate::layout::Layout;
use crate::message::Message;

/// This start's hold on `root`, or none when another Anomi runs there and
/// took this start over: then nothing is left to do. The preview takes no
/// hold, as it touches no session.
pub fn claim(root: &Path, layout: &Layout, start: &Start) -> Option<Instance> {
    if start.preview {
        return Some(Instance::default());
    }
    match instance::claim(
        root,
        !layout.multiple_windows,
        start.background,
        start.link.clone(),
    ) {
        instance::Start::Run(instance) => Some(instance),
        instance::Start::Handed => None,
    }
}

/// Anomi was started again on this folder and the new start asked for the
/// window: it shows, taking the focus with that start's token, and the link
/// it brought opens once it can.
pub fn launched(app: &mut App, launch: Launch) -> Task<Message> {
    let Launch { token, link } = launch;
    tracing::info!("Anomi started again; showing the window");
    if let Some(link) = link {
        super::links::queue(app, link);
    }
    let shown = super::window::open(app, token);
    Task::batch([shown, super::links::open_waiting(app)])
}
