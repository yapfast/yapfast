//! Quests of playing: while a game runs that an accepted quest asks the
//! account to play, Discord counts the minutes the client says so, with a
//! heartbeat about every minute, as its own client does for the game it
//! detects. The game comes from [`crate::own_activity`], which is what the
//! account shows it plays, so a game the person hides is not counted.

use crate::{
    api::Api,
    quests::{Status, Task},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// How often Discord hears the game is still played.
const HEARTBEAT: Duration = Duration::from_secs(60);
/// How long to wait before looking again for a quest of the game, when the
/// account has none accepted: it may accept one while the game runs.
const LOOK_AGAIN: Duration = Duration::from_secs(180);
/// How many heartbeats in a row may fail before counting stops until the
/// game changes.
const FAILURES: u32 = 3;

/// Counts the playing of whatever game `activity` names, until it goes.
pub(crate) fn start(api: Arc<Api>, mut activity: watch::Receiver<Option<Arc<Value>>>) {
    tokio::spawn(async move {
        loop {
            let game = application(&activity.borrow_and_update());
            match game {
                Some(application) => {
                    tokio::select! {
                        () = count(&api, &application) => {}
                        changed = activity.changed() => {
                            if changed.is_err() {
                                return;
                            }
                        }
                    }
                }
                None => {
                    if activity.changed().await.is_err() {
                        return;
                    }
                }
            }
        }
    });
}

/// The Discord application of the game an activity names, if it has one:
/// the games a quest can ask for are those Discord knows.
fn application(activity: &Option<Arc<Value>>) -> Option<String> {
    let id = activity.as_ref()?["application_id"].as_str()?;
    (!id.is_empty()).then(|| id.to_owned())
}

/// Sends heartbeats for the account's accepted quest of playing `application`
/// for as long as it runs, and looks for the quest again while it has
/// none. Ends only when it is dropped, or after failing too often.
async fn count(api: &Api, application: &str) {
    let mut failed = 0;
    loop {
        let Some((id, target)) = find(api, application).await else {
            tokio::time::sleep(LOOK_AGAIN).await;
            continue;
        };
        loop {
            match api.quest_heartbeat(&id, application).await {
                Ok(answer) => {
                    failed = 0;
                    let (status, _) = crate::quests::standing(&answer, target);
                    if matches!(status, Status::Completed | Status::Claimed) {
                        tracing::info!(quest = %id, "a quest of playing is done");
                        break;
                    }
                }
                Err(error) => {
                    failed += 1;
                    tracing::debug!(%error, quest = %id, "could not count a quest's playing");
                    if failed >= FAILURES {
                        return;
                    }
                }
            }
            tokio::time::sleep(HEARTBEAT).await;
        }
        // Another quest of the same game may follow.
        tokio::time::sleep(LOOK_AGAIN).await;
    }
}

/// An accepted quest of playing `application`, by its id and the seconds it
/// asks for.
async fn find(api: &Api, application: &str) -> Option<(String, u64)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())?;
    let listed = api
        .quests()
        .await
        .inspect_err(|error| tracing::debug!(%error, "could not list quests to count playing"))
        .ok()?;
    crate::quests::parse(&listed, now)
        .into_iter()
        .find(|quest| {
            quest.task == Task::Play
                && quest.status == Status::Accepted
                && quest.application.as_deref() == Some(application)
        })
        .map(|quest| (quest.id, quest.target))
}
