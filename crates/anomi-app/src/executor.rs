//! The executor of iced's futures: Tokio with two workers.
//!
//! iced's own executors size themselves to the CPU, a thread a core. The
//! network runs on the core's `BackgroundRuntime`, which keeps its runtime
//! to itself, so iced's tasks get this small one; waiting on the core's
//! receivers works from any runtime.

use iced::executor::Executor;

pub struct Small(tokio::runtime::Runtime);

impl Executor for Small {
    fn new() -> Result<Self, iced::futures::io::Error> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("anomi-ui")
            .enable_time()
            .build()
            .map(Small)
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        drop(self.0.spawn(future));
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.enter();
        f()
    }
}
