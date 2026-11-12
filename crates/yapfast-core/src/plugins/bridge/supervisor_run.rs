use super::*;

/// Runs the helper, restarts it after a crash, and gives up when it keeps
/// crashing.
///
/// A process is started only when there is a command for it: with nothing to do,
/// this task holds a channel and no process at all.
pub(super) async fn supervise(
    shared: Arc<Shared>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Request>,
    data_dir: PathBuf,
) {
    let mut restarts = Restarts::new();
    loop {
        let first = match commands.recv().await {
            Some(command) => command,
            // The bridge is gone, which only happens as the application exits.
            None => return,
        };
        if matches!(first, Request::Shutdown) {
            return;
        }
        let mut session = match Session::start(&data_dir) {
            Ok(session) => session,
            Err(error) => {
                shared.give_up(format!("{error:#}"));
                stand_down(&shared, &mut commands).await;
                return;
            }
        };
        shared.set_helper_pid(Some(session.child.id()));
        session.queue(first);
        let ending = session.run(&shared, &mut commands).await;
        // A send that was waiting on this process has to go out as the person
        // wrote it: the chain it was waiting for no longer exists. A request
        // waiting on a consent dialog is the other way round, since there is no
        // person to answer in this process's name any more.
        session.fail_open(&shared);
        shared.release_held("the plugin helper stopped");
        drop(session);
        shared.set_helper_pid(None);
        match ending {
            Ending::Asked => return,
            Ending::Idle => continue,
            Ending::Gone(reason) => match restarts.crashed(Instant::now()) {
                spawn::Next::After(wait) => {
                    tracing::warn!(
                        %reason,
                        wait_ms = wait.as_millis() as u64,
                        "the plugin helper stopped; starting it again"
                    );
                    tokio::time::sleep(wait).await;
                }
                spawn::Next::Off(reason) => {
                    shared.give_up(reason);
                    stand_down(&shared, &mut commands).await;
                    return;
                }
            },
        }
    }
}
