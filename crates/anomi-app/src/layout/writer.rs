//! The thread that writes `layout.json`, off the window's: it sleeps until
//! handed a file, writes each in the order handed, and says when it has
//! written everything handed before a wait.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender, SyncSender};

enum Job {
    /// The file's bytes, for the folder it goes in.
    Write(PathBuf, Vec<u8>),
    /// Everything before is written.
    Settled(SyncSender<()>),
}

/// The writer, once a write started it; none if it couldn't start.
static WRITER: OnceLock<Option<Sender<Job>>> = OnceLock::new();

/// The writer, started with the first write.
fn writer() -> Option<&'static Sender<Job>> {
    WRITER
        .get_or_init(|| {
            let (sender, jobs) = mpsc::channel::<Job>();
            let started = std::thread::Builder::new()
                .name("anomi-layout".into())
                .stack_size(256 * 1024)
                .spawn(move || {
                    for job in jobs {
                        match job {
                            Job::Write(folder, bytes) => store(&folder, &bytes),
                            Job::Settled(told) => drop(told.send(())),
                        }
                    }
                });
            match started {
                Ok(_) => Some(sender),
                Err(error) => {
                    tracing::warn!(%error, "could not start the layout's writer");
                    None
                }
            }
        })
        .as_ref()
}

fn store(folder: &std::path::Path, bytes: &[u8]) {
    if let Err(error) = yapfast_core::layout_file::write(folder, bytes) {
        tracing::warn!(%error, "could not store the window layout");
    }
}

/// Writes `bytes` as the layout in `folder`, on the writer's thread; on
/// this one if the writer could not start.
pub(super) fn write(folder: PathBuf, bytes: Vec<u8>) {
    match writer() {
        Some(writer) => {
            if let Err(mpsc::SendError(Job::Write(folder, bytes))) =
                writer.send(Job::Write(folder, bytes))
            {
                store(&folder, &bytes);
            }
        }
        None => store(&folder, &bytes),
    }
}

/// Waits until every write handed over is in its file.
pub(super) fn settle() {
    let Some(writer) = WRITER.get().and_then(Option::as_ref) else {
        return;
    };
    let (told, settled) = mpsc::sync_channel(1);
    if writer.send(Job::Settled(told)).is_ok() {
        let _ = settled.recv();
    }
}
