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

/// The games the last scan took the running programs for, each by the name
/// that would show after Playing, and not those the blocked list leaves out.
/// Empty while nothing is scanning.
struct Detected {
    generation: u64,
    names: Vec<String>,
}

static DETECTED: std::sync::Mutex<Detected> = std::sync::Mutex::new(Detected {
    generation: 0,
    names: Vec::new(),
});

pub(super) struct Detection(u64);

impl Detection {
    pub(super) fn start() -> Self {
        let mut detected = DETECTED.lock().unwrap_or_else(|error| error.into_inner());
        detected.generation = detected.generation.wrapping_add(1);
        detected.names.clear();
        Self(detected.generation)
    }

    pub(super) fn publish(&self, names: Vec<String>) {
        let mut detected = DETECTED.lock().unwrap_or_else(|error| error.into_inner());
        if detected.generation == self.0 && detected.names != names {
            detected.names = names;
        }
    }
}

impl Drop for Detection {
    fn drop(&mut self) {
        self.publish(Vec::new());
    }
}

/// Keeps the games the last scan found, if they changed. Detection does this;
/// the window's tests may too.
pub fn set_detected(names: Vec<String>) {
    DETECTED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .names = names;
}

/// The games the last scan took the running programs for, by their names.
pub fn detected() -> Vec<String> {
    DETECTED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .names
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_current_detector_can_replace_or_clear_the_menu() {
        let old = Detection::start();
        let new = Detection::start();
        new.publish(vec!["Current game".into()]);
        old.publish(vec!["Old game".into()]);
        drop(old);
        assert_eq!(detected(), ["Current game"]);
        drop(new);
        assert!(detected().is_empty());
    }
}
