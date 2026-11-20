use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    runtime::{Builder, Runtime},
    task::{JoinError, JoinHandle},
};

/// Shared network workers, separate from the client's UI executor.
pub struct BackgroundRuntime {
    runtime: Runtime,
}

impl BackgroundRuntime {
    pub fn new() -> std::io::Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("yapfast-network")
            .enable_all()
            .build()?;
        Ok(Self { runtime })
    }

    pub fn spawn<F>(&self, future: F) -> BackgroundTask<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        BackgroundTask {
            handle: self.runtime.spawn(future),
        }
    }

    /// Runs blocking work, such as a platform call, on Tokio's blocking pool.
    pub fn spawn_blocking<F, T>(&self, work: F) -> BackgroundTask<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        BackgroundTask {
            handle: self.runtime.spawn_blocking(work),
        }
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// The runtime's handle, for work another crate spawns here.
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }
}

/// Dropping a task handle cancels its Tokio work.
pub struct BackgroundTask<T> {
    handle: JoinHandle<T>,
}

impl<T> Future for BackgroundTask<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx)
    }
}

impl<T> Drop for BackgroundTask<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[test]
    fn background_work_can_use_tokio_and_return_a_value() {
        let runtime = BackgroundRuntime::new().unwrap();
        let task = runtime.spawn(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            42
        });
        assert_eq!(runtime.block_on(task).unwrap(), 42);
    }

    #[test]
    fn dropping_a_task_releases_its_background_resources() {
        struct NotifyOnDrop(Option<oneshot::Sender<()>>);
        impl Drop for NotifyOnDrop {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }

        let runtime = BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let (started_tx, started_rx) = oneshot::channel();
            let (stopped_tx, stopped_rx) = oneshot::channel();
            let task = runtime.spawn(async move {
                let _guard = NotifyOnDrop(Some(stopped_tx));
                started_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            });
            tokio::time::timeout(Duration::from_secs(2), started_rx)
                .await
                .unwrap()
                .unwrap();
            drop(task);
            tokio::time::timeout(Duration::from_secs(2), stopped_rx)
                .await
                .unwrap()
                .unwrap();
        });
    }
}
