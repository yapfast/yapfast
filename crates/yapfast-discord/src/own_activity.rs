//! The game this account shares: what a game says over Rich Presence, or
//! else the game detection finds. The account's `show_current_game` setting
//! turns both on and off; the custom status stays apart from them.

use crate::{api::Api, service::Snapshot};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::watch;

type Activity = Option<Arc<Value>>;

/// The game the account shows it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    pub name: String,
    /// When it started, in Unix milliseconds, when the game says.
    pub started_ms: Option<u64>,
}

impl Playing {
    /// The game an activity names, if it names one.
    pub(crate) fn of(activity: &Value) -> Option<Self> {
        let name = activity["name"].as_str()?.trim();
        (!name.is_empty()).then(|| Self {
            name: name.to_owned(),
            started_ms: activity
                .pointer("/timestamps/start")
                .and_then(Value::as_u64),
        })
    }
}

/// Runs detection and Rich Presence while `shown` holds, and gives the
/// activity they make, until the receiver goes.
pub(crate) fn start(
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    mut shown: watch::Receiver<bool>,
) -> watch::Receiver<Activity> {
    #[cfg(not(target_os = "linux"))]
    let _ = snapshots;
    let (activity, receiver) = watch::channel(None);
    tokio::spawn(async move {
        loop {
            if *shown.borrow_and_update() {
                let detected = crate::game_detection::start(api.clone());
                #[cfg(target_os = "linux")]
                let rpc = crate::rpc::start(api.clone(), snapshots.clone());
                #[cfg(not(target_os = "linux"))]
                let rpc = watch::channel(None).1;
                tokio::select! {
                    () = combine(rpc, detected, activity.clone()) => return,
                    () = hidden(&mut shown) => {}
                }
                activity.send_replace(None);
            } else {
                tokio::select! {
                    () = activity.closed() => return,
                    result = shown.changed() => if result.is_err() { return; },
                }
            }
        }
    });
    receiver
}

/// Waits for sharing to stop. Once the session is gone, the activity's
/// receiver goes with it, which ends [`combine`] instead.
async fn hidden(shown: &mut watch::Receiver<bool>) {
    while shown.changed().await.is_ok() {
        if !*shown.borrow_and_update() {
            return;
        }
    }
    std::future::pending().await
}

async fn combine(
    mut rpc: watch::Receiver<Activity>,
    mut detected: watch::Receiver<Activity>,
    shown: watch::Sender<Activity>,
) {
    let mut rpc_open = true;
    let mut detection_open = true;
    loop {
        let next = rpc_open
            .then(|| rpc.borrow().clone())
            .flatten()
            .or_else(|| detection_open.then(|| detected.borrow().clone()).flatten());
        shown.send_if_modified(|old| {
            if *old == next {
                false
            } else {
                *old = next;
                true
            }
        });
        tokio::select! {
            () = shown.closed() => return,
            result = rpc.changed(), if rpc_open => rpc_open = result.is_ok(),
            result = detected.changed(), if detection_open => detection_open = result.is_ok(),
        }
    }
}

/// Puts what this session shows in `presence`, in the order Discord's client
/// sends it: the custom status, the game, then the music.
pub(crate) fn merge(presence: &mut Value, game: Option<&Value>, music: Option<&Value>) {
    if let Some(activities) = presence["activities"].as_array_mut() {
        activities.retain(|value| value["type"] == 4);
        activities.extend(game.cloned());
        activities.extend(music.cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn rpc_overrides_detection_and_disconnect_restores_it() {
        let (rpc, rpc_rx) = watch::channel(None);
        let (detected, detected_rx) = watch::channel(None);
        let (shown, mut receiver) = watch::channel(None);
        let worker = tokio::spawn(combine(rpc_rx, detected_rx, shown));
        let game = Arc::new(json!({"type":0,"name":"Game"}));
        detected.send_replace(Some(game.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&game));
        let rich = Arc::new(json!({"type":0,"name":"Game","details":"In match"}));
        rpc.send_replace(Some(rich.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&rich));
        // A source that dies without clearing must not leave a stale activity.
        drop(rpc);
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&game));
        detected.send_replace(None);
        receiver.changed().await.unwrap();
        assert!(receiver.borrow_and_update().is_none());
        drop(receiver);
        worker.await.unwrap();
        assert!(detected.is_closed());
    }

    #[test]
    fn names_the_game_an_activity_plays() {
        assert_eq!(
            Playing::of(&json!({"type":0,"name":" Game ","timestamps":{"start":5}})),
            Some(Playing {
                name: "Game".into(),
                started_ms: Some(5)
            })
        );
        assert_eq!(Playing::of(&json!({"type":0,"name":" "})), None);
    }
}
