//! Messages that come later.

use std::time::Duration;

use iced::Task;

/// `message` after `delay`, as a task on the backend's runtime.
pub fn after<M: Send + 'static>(delay: Duration, message: M) -> Task<M> {
    // Made when first polled, so it needs the runtime only then.
    Task::perform(async move { tokio::time::sleep(delay).await }, move |()| {
        message
    })
}
