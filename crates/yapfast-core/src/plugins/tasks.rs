//! The application's background runtime, as the plugin bridge asks for work.
//!
//! The bridge's fetches, its catalog reads and the supervisor itself outlive the
//! call that asked for them, and none of them belongs on the thread that draws.
//! The runtime is the application's — [`crate::runtime::BackgroundRuntime`] — so
//! a session with no plugin enabled starts no threads at all: it is made on the
//! first request, as it always was.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use yapfast_plugins::bridge::tasks::{Spawn, Task, Work};

/// The application's runtime, made on first use.
#[derive(Default)]
pub struct Tasks {
    runtime: Mutex<Option<crate::runtime::BackgroundRuntime>>,
}

impl Tasks {
    /// The runtime's handle, starting the runtime the first time it is asked.
    fn handle(&self) -> anyhow::Result<tokio::runtime::Handle> {
        let mut slot = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(crate::runtime::BackgroundRuntime::new()?);
        }
        match slot.as_ref() {
            Some(runtime) => Ok(runtime.handle()),
            None => anyhow::bail!("the plugin system's runtime isn't there"),
        }
    }
}

impl Spawn for Tasks {
    fn spawn(&self, work: Work) -> anyhow::Result<Task> {
        Ok(Task::new(self.handle()?.spawn(work)))
    }

    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send + 'static>) -> anyhow::Result<Task> {
        Ok(Task::new(self.handle()?.spawn_blocking(work)))
    }

    fn wait(&self, task: &mut Task, limit: std::time::Duration) -> bool {
        match self.handle() {
            Ok(handle) => {
                handle.block_on(async { tokio::time::timeout(limit, task).await.is_ok() })
            }
            Err(_) => false,
        }
    }
}

/// A plain future, for the tests that wait on one.
#[allow(dead_code)]
pub(crate) fn spin<T: Future>(future: T) -> Pin<Box<T>> {
    Box::pin(future)
}
