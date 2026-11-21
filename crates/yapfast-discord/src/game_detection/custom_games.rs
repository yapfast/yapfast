//! The games this device names itself, which the desktop sets from its
//! settings. They're the device's, not an account's, so every session shares
//! them, and a change reaches the detection running.
use std::sync::OnceLock;
use tokio::sync::watch;

/// An executable, by file name or full path, and the game to show for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomGame {
    pub executable: String,
    pub name: String,
}

fn channel() -> &'static watch::Sender<Vec<CustomGame>> {
    static CHANNEL: OnceLock<watch::Sender<Vec<CustomGame>>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(Vec::new()).0)
}

pub fn set(games: Vec<CustomGame>) {
    channel().send_if_modified(|old| {
        if *old == games {
            false
        } else {
            *old = games;
            true
        }
    });
}

pub(crate) fn subscribe() -> watch::Receiver<Vec<CustomGame>> {
    channel().subscribe()
}

fn blocked_channel() -> &'static watch::Sender<Vec<String>> {
    static CHANNEL: OnceLock<watch::Sender<Vec<String>>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(Vec::new()).0)
}

/// The programs and games this device never shows as the game being played:
/// each an executable, by file name or full path, or the name that would show
/// after Playing. They're the device's, like the custom games, and a change
/// reaches the detection running.
pub fn set_blocked(blocked: Vec<String>) {
    blocked_channel().send_if_modified(|old| {
        if *old == blocked {
            false
        } else {
            *old = blocked;
            true
        }
    });
}

pub(crate) fn subscribe_blocked() -> watch::Receiver<Vec<String>> {
    blocked_channel().subscribe()
}
