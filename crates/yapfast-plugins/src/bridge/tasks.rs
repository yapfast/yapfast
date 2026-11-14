//! Where the bridge's background work runs.
//!
//! A fetch, a catalog read and the supervisor itself outlive the call that asked
//! for them, and the application owns the threads they run on: it hands the
//! bridge a [`Spawn`] when it opens one, and this crate names what runs. Nothing
//! here creates a runtime, so the plugin system has no thread pool of its own.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::task::{JoinError, JoinHandle};

/// Work for a runtime to run: one task, boxed because a trait can't be generic
/// over the future it takes.
pub type Work = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// The application's background runtime, as the bridge asks it for work.
///
/// Both methods can fail, because the runtime behind them may not have started:
/// the caller says why and goes on without plugins rather than panicking.
pub trait Spawn: Send + Sync {
    /// Runs `work` in the background: a fetch, a catalog read or the supervisor.
    fn spawn(&self, work: Work) -> anyhow::Result<Task>;

    /// Runs `work` on the runtime's blocking pool, which is where the git and
    /// disk work of a fetch belongs.
    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send + 'static>) -> anyhow::Result<Task>;

    /// Waits for `task` to end, up to `limit`, on the thread that called this:
    /// how the bridge stops the helper before the process ends. True when the
    /// work ended in time.
    fn wait(&self, task: &mut Task, limit: std::time::Duration) -> bool;
}

/// One piece of that work, cancelled when it is dropped.
///
/// The bridge holds the tasks it started so that dropping one cancels it: a
/// fetch for a page that closed asks nobody for anything afterwards.
pub struct Task(JoinHandle<()>);

impl Task {
    /// Takes the runtime's handle on a task it started.
    pub fn new(handle: JoinHandle<()>) -> Self {
        Self(handle)
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Future for Task {
    type Output = Result<(), JoinError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(context)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    /// A runtime of the test's own, standing in for the application's.
    struct Runtime(tokio::runtime::Runtime);

    impl Runtime {
        fn new() -> Self {
            Self(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("a runtime"),
            )
        }
    }

    impl Spawn for Runtime {
        fn spawn(&self, work: Work) -> anyhow::Result<Task> {
            Ok(Task::new(self.0.spawn(work)))
        }

        fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send + 'static>) -> anyhow::Result<Task> {
            Ok(Task::new(self.0.spawn_blocking(work)))
        }

        fn wait(&self, task: &mut Task, limit: std::time::Duration) -> bool {
            self.0
                .block_on(async { tokio::time::timeout(limit, task).await.is_ok() })
        }
    }

    #[test]
    fn a_dropped_task_is_cancelled() {
        let runtime = Runtime::new();
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let task = runtime
            .spawn(Box::pin(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                flag.store(true, Ordering::Relaxed);
            }))
            .expect("a task");
        drop(task);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!ran.load(Ordering::Relaxed), "a dropped task never runs on");
    }
}
