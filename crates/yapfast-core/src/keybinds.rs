//! Keybinds' keys and what they do, as the window keeps, shows and records
//! them: a [`Shortcut`] of up to [`MOST_KEYS`] keys, the [`Action`] it
//! does, the [`Binding`]s `layout.json` keeps, the [`Recorder`] that takes
//! a new one from the keys pressed, and the [`Status`] of where they work.
//! The root crate's watcher watches them in every app.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The most keys one keybind holds together.
pub const MOST_KEYS: usize = 4;

/// Mouse buttons share the saved scan-code field, tagged above Windows' 16-bit
/// scan codes so keyboard bindings keep their meaning.
#[cfg(windows)]
pub const MOUSE_4: u32 = 0xFFFF_0004;
#[cfg(windows)]
pub const MOUSE_5: u32 = 0xFFFF_0005;

/// Ctrl, Shift, Alt and Super: each side's scan code, as winit gives them,
/// and the XDG shortcuts specification's name for it. In a combination
/// either side holds it.
#[cfg(target_os = "linux")]
const MODIFIERS: [(u32, u32, &str); 4] = [
    (29, 97, "CTRL"),
    (42, 54, "SHIFT"),
    (56, 100, "ALT"),
    (125, 126, "LOGO"),
];
#[cfg(windows)]
const MODIFIERS: [(u32, u32, &str); 4] = [
    (0x1D, 0xE01D, "CTRL"),
    (0x2A, 0x36, "SHIFT"),
    (0x38, 0xE038, "ALT"),
    (0xE05B, 0xE05C, "LOGO"),
];
#[cfg(not(any(target_os = "linux", windows)))]
const MODIFIERS: [(u32, u32, &str); 0] = [];

/// The modifier `code` is a side of, if it's one.
pub fn modifier(code: u32) -> Option<(u32, u32, &'static str)> {
    MODIFIERS
        .iter()
        .copied()
        .find(|&(left, right, _)| code == left || code == right)
}

/// A key recorded in settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// winit's scan code for it: evdev's on Linux, which X11 reports 8
    /// higher, and set 1's on Windows, with 0xE000 for an extended key.
    pub scancode: u32,
    /// As settings show it, such as "V" or "Right Ctrl".
    pub name: String,
    /// Its XKB keysym name, such as "v" or "Control_R", from which the
    /// GlobalShortcuts portal gets the shortcut Yapfast would like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
}

/// The keys of a keybind, held together: one to [`MOST_KEYS`], modifiers
/// first.
///
/// Saved as the last key, with the others before it in `with`. A single key
/// is saved as before keybinds had more, and a build from then reads a
/// combination as its last key rather than failing on the file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Stored", into = "Stored")]
pub struct Shortcut {
    keys: Vec<Key>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(flatten)]
    last: Key,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    with: Vec<Key>,
}

impl From<Stored> for Shortcut {
    fn from(stored: Stored) -> Self {
        let mut keys = stored.with;
        keys.push(stored.last);
        Self::new(keys).expect("one key at least")
    }
}

impl From<Shortcut> for Stored {
    fn from(shortcut: Shortcut) -> Self {
        let mut with = shortcut.keys;
        let last = with.pop().expect("one key at least");
        Self { last, with }
    }
}

impl Shortcut {
    /// These keys held together, each once, modifiers first; none for none.
    pub fn new(keys: Vec<Key>) -> Option<Self> {
        let mut unique: Vec<Key> = Vec::with_capacity(keys.len().min(MOST_KEYS));
        for key in keys {
            if unique.len() < MOST_KEYS && unique.iter().all(|seen| seen.scancode != key.scancode) {
                unique.push(key);
            }
        }
        unique.sort_by_key(|key| modifier(key.scancode).is_none());
        (!unique.is_empty()).then_some(Self { keys: unique })
    }

    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// As settings show it, such as "Ctrl + Shift + M": in a combination a
    /// modifier goes without its side, as either side holds it.
    pub fn name(&self) -> String {
        names(&self.keys)
    }

    /// The shortcut to offer the portal in the XDG shortcuts specification's
    /// words, such as "CTRL+SHIFT+m": modifiers and then one other key. None
    /// when the keys can't be put that way.
    #[cfg(target_os = "linux")]
    pub fn trigger(&self) -> Option<String> {
        let [modifiers @ .., last] = self.keys.as_slice() else {
            return None;
        };
        let mut trigger = String::new();
        for key in modifiers {
            trigger.push_str(modifier(key.scancode)?.2);
            trigger.push('+');
        }
        trigger.push_str(last.trigger.as_deref()?);
        Some(trigger)
    }
}

fn names(keys: &[Key]) -> String {
    let combination = keys.len() > 1;
    let mut name = String::new();
    for key in keys {
        if !name.is_empty() {
            name.push_str(" + ");
        }
        let sided = key.name.as_str();
        let unsided = sided
            .strip_prefix("Left ")
            .or_else(|| sided.strip_prefix("Right "));
        match unsided {
            Some(unsided) if combination && modifier(key.scancode).is_some() => {
                name.push_str(unsided);
            }
            _ => name.push_str(sided),
        }
    }
    name
}

/// What a keybind does, as Discord's actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    PushToTalk,
    PushToMute,
    ToggleMute,
    ToggleDeafen,
    /// Switches between push to talk and voice activity.
    ToggleVoiceMode,
    /// Does nothing, as a keybind just added; an action from a later
    /// build reads as this, so it's last.
    #[default]
    #[serde(other)]
    Unassigned,
}

impl Action {
    /// The actions settings offer, in Discord's order.
    pub const OFFERED: [Action; 6] = [
        Action::Unassigned,
        Action::PushToTalk,
        Action::PushToMute,
        Action::ToggleMute,
        Action::ToggleDeafen,
        Action::ToggleVoiceMode,
    ];

    /// Its name, as Discord's settings give it; the portal shows it.
    pub fn label(self) -> &'static str {
        match self {
            Action::Unassigned => "Unassigned",
            Action::PushToTalk => "Push to Talk (Normal)",
            Action::PushToMute => "Push to Mute",
            Action::ToggleMute => "Toggle Mute",
            Action::ToggleDeafen => "Toggle Deafen",
            Action::ToggleVoiceMode => "Toggle VAD",
        }
    }

    /// What it does, as Discord's settings say under it.
    pub fn description(self) -> &'static str {
        match self {
            Action::Unassigned => {
                "This action doesn't do anything! Please choose one from the drop down."
            }
            Action::PushToTalk => {
                "Hold to temporarily enable your microphone while in Push to Talk mode."
            }
            Action::PushToMute => {
                "Hold to temporarily disable your microphone while in Voice Activity mode."
            }
            Action::ToggleMute => "Toggle your microphone's transmission on and off.",
            Action::ToggleDeafen => {
                "Toggle your speaker's playback on and off. Also disables your microphone while deafened."
            }
            Action::ToggleVoiceMode => {
                "Toggle to switch between Push To Talk and Voice Activity mode."
            }
        }
    }

    /// It lasts while its keys are held, rather than happening once a press.
    pub fn holds(self) -> bool {
        matches!(self, Action::PushToTalk | Action::PushToMute)
    }

    /// Its place among those that hold.
    pub fn hold_slot(self) -> Option<usize> {
        match self {
            Action::PushToTalk => Some(0),
            Action::PushToMute => Some(1),
            _ => None,
        }
    }
}

/// A keybind in settings, as `layout.json` keeps them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// Its own number, which the desktop's shortcut for it is named after,
    /// so the desktop keeps what it allowed when others go.
    pub id: u32,
    #[serde(default)]
    pub action: Action,
    /// None until recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Shortcut>,
}

impl Binding {
    /// Its shortcut's name for the portal.
    pub fn shortcut_id(&self) -> String {
        format!("keybind-{}", self.id)
    }
}

/// A keybind to watch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watched {
    /// The portal's name for it, which stays the same from run to run.
    pub id: String,
    pub action: Action,
    pub keys: Shortcut,
}

/// The shortcut id of the Voice page's push-to-talk key, as before there
/// were other keybinds, so the desktop keeps what it allowed for it.
pub const TALK_SHORTCUT: &str = "push-to-talk";

/// Records a keybind from the keys pressed while settings record one: each
/// key pressed joins it, and it's done once they're all up again.
#[derive(Default)]
pub struct Recorder {
    keys: Vec<Key>,
    /// Keys down now, of those pressed since recording began.
    down: Vec<u32>,
}

impl Recorder {
    /// A key went down or up; the keybind once every key is up again.
    pub fn key(&mut self, key: Key, pressed: bool) -> Option<Shortcut> {
        if pressed {
            if !self.down.contains(&key.scancode) {
                self.down.push(key.scancode);
            }
            if self.keys.len() < MOST_KEYS && self.keys.iter().all(|k| k.scancode != key.scancode) {
                self.keys.push(key);
            }
            return None;
        }
        // A key held from before recording began coming up changes nothing.
        let at = self.down.iter().position(|&code| code == key.scancode)?;
        self.down.swap_remove(at);
        if self.down.is_empty() {
            self.finish()
        } else {
            None
        }
    }

    /// The keys so far, as settings show them while recording.
    pub fn shown(&self) -> String {
        let mut keys = self.keys.clone();
        keys.sort_by_key(|key| modifier(key.scancode).is_none());
        names(&keys)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The keys pressed so far, as when the window loses the focus while
    /// they're held; none if none were.
    pub fn finish(&mut self) -> Option<Shortcut> {
        self.down.clear();
        Shortcut::new(std::mem::take(&mut self.keys))
    }
}

/// Where keybinds work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    /// Asking the desktop for them.
    #[default]
    Asking,
    /// In every app, as Yapfast hears every key.
    Everywhere,
    /// In every app, as the desktop runs them: it may give them other keys,
    /// or none until it's asked to allow them; see [`Status::given`].
    Desktop,
    /// Only while Yapfast's window has the focus, for this reason, which
    /// the note goes on from.
    Focused(&'static str),
}

/// Where keybinds work, and what the desktop gave each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub reach: Reach,
    /// Each keybind's shortcut in the desktop's words, by its id, while the
    /// desktop runs them; empty for one it hasn't allowed yet.
    pub given: HashMap<String, String>,
}

impl Status {
    /// What the Voice page says under the push-to-talk key.
    pub fn talk_note(&self) -> String {
        match &self.reach {
            Reach::Asking => {
                "Asking your desktop to let Yapfast use the key in every app...".into()
            }
            Reach::Everywhere => "Hold the key in any app to talk.".into(),
            Reach::Desktop => match self.given.get(TALK_SHORTCUT) {
                Some(shortcut) if !shortcut.is_empty() => format!(
                    "Your desktop gave push to talk the shortcut {shortcut}. Hold it in any app to talk; change it in your system's global shortcuts settings."
                ),
                _ => "Your desktop hasn't allowed this shortcut yet. Approve it in your system's global shortcuts settings.".into(),
            },
            Reach::Focused(why) => format!(
                "{why}, so push to talk works only while Yapfast's window is in front."
            ),
        }
    }

    /// What the Keybinds page says above the keybinds.
    pub fn page_note(&self) -> String {
        match &self.reach {
            Reach::Asking => {
                "Asking your desktop to let Yapfast use these keybinds in every app...".into()
            }
            Reach::Everywhere => "Keybinds work in any app.".into(),
            Reach::Desktop => "Your desktop runs these keybinds in every app: it may ask you to allow each, or give one other keys. Change them in your system's global shortcuts settings.".into(),
            Reach::Focused(why) => {
                format!("{why}, so keybinds work only while Yapfast's window is in front.")
            }
        }
    }

    /// What the desktop did with keybind `id`, while it runs them.
    pub fn given_note(&self, id: &str) -> Option<String> {
        if self.reach != Reach::Desktop {
            return None;
        }
        Some(match self.given.get(id) {
            Some(shortcut) if !shortcut.is_empty() => {
                format!("Your desktop gave it the shortcut {shortcut}.")
            }
            _ => "Your desktop hasn't allowed it yet.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scancode: u32, name: &str) -> Key {
        Key {
            scancode,
            name: name.into(),
            trigger: Some(name.to_lowercase()),
        }
    }

    const V: u32 = 47;
    const M: u32 = 50;
    const D: u32 = 32;
    #[cfg(target_os = "linux")]
    const LEFT_CTRL: u32 = 29;
    #[cfg(target_os = "linux")]
    const RIGHT_CTRL: u32 = 97;
    #[cfg(target_os = "linux")]
    const SHIFT: u32 = 42;
    #[cfg(windows)]
    const LEFT_CTRL: u32 = 0x1D;

    #[test]
    fn recording_takes_the_keys_held_together_once_they_come_up() {
        let mut recorder = Recorder::default();
        // A key held from before recording began coming up changes nothing.
        assert_eq!(recorder.key(key(28, "Enter"), false), None);
        assert_eq!(recorder.key(key(M, "M"), true), None);
        #[cfg(any(target_os = "linux", windows))]
        {
            assert_eq!(recorder.key(key(LEFT_CTRL, "Left Ctrl"), true), None);
            assert_eq!(recorder.key(key(M, "M"), true), None, "a repeat");
            assert_eq!(recorder.shown(), "Ctrl + M", "either side holds it");
            assert_eq!(recorder.key(key(M, "M"), false), None);
            let shortcut = recorder.key(key(LEFT_CTRL, "Left Ctrl"), false).unwrap();
            assert_eq!(shortcut.name(), "Ctrl + M", "modifiers first");
            let alone = Shortcut::new(vec![key(LEFT_CTRL, "Left Ctrl")]).unwrap();
            assert_eq!(alone.name(), "Left Ctrl", "a key alone is its side");
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        assert!(recorder.key(key(M, "M"), false).is_some());
        assert!(recorder.is_empty());
    }

    #[test]
    fn recording_keeps_four_keys_at_most() {
        let mut recorder = Recorder::default();
        for code in 1..=6 {
            recorder.key(key(code, &code.to_string()), true);
        }
        for code in 1..6 {
            assert_eq!(recorder.key(key(code, &code.to_string()), false), None);
        }
        let shortcut = recorder.key(key(6, "6"), false).unwrap();
        assert_eq!(shortcut.keys().len(), MOST_KEYS);
    }

    #[test]
    fn a_single_key_saves_as_before_and_a_combination_as_its_last_key_with_others() {
        let single = Shortcut::new(vec![key(V, "V")]).unwrap();
        let saved = serde_json::to_value(&single).unwrap();
        assert_eq!(
            saved,
            serde_json::json!({ "scancode": V, "name": "V", "trigger": "v" })
        );
        let old: Shortcut =
            serde_json::from_value(serde_json::json!({ "scancode": 47, "name": "V" })).unwrap();
        assert_eq!(old.keys()[0].scancode, 47);
        assert_eq!(old.keys()[0].trigger, None);

        let both = Shortcut::new(vec![key(M, "M"), key(D, "D")]).unwrap();
        let saved = serde_json::to_value(&both).unwrap();
        assert_eq!(saved["scancode"], D);
        assert_eq!(saved["with"][0]["scancode"], M);
        let read: Shortcut = serde_json::from_value(saved).unwrap();
        assert_eq!(read, both);
    }

    #[test]
    fn an_action_from_a_later_build_reads_as_unassigned() {
        let binding: Binding = serde_json::from_value(serde_json::json!({
            "id": 4, "action": "toggle_go_live", "keys": { "scancode": 47, "name": "V" }
        }))
        .unwrap();
        assert_eq!(binding.action, Action::Unassigned);
        assert_eq!(binding.shortcut_id(), "keybind-4");
        let unrecorded: Binding = serde_json::from_value(serde_json::json!({ "id": 1 })).unwrap();
        assert_eq!(unrecorded.keys, None);
        assert_eq!(
            serde_json::to_value(Binding {
                id: 2,
                action: Action::ToggleDeafen,
                keys: None
            })
            .unwrap(),
            serde_json::json!({ "id": 2, "action": "toggle_deafen" })
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_portal_is_offered_modifiers_then_one_key() {
        let shortcut = |keys: Vec<Key>| Shortcut::new(keys).unwrap().trigger();
        assert_eq!(shortcut(vec![key(V, "V")]), Some("v".into()));
        let ctrl = Key {
            scancode: RIGHT_CTRL,
            name: "Right Ctrl".into(),
            trigger: Some("Control_R".into()),
        };
        assert_eq!(shortcut(vec![ctrl.clone()]), Some("Control_R".into()));
        assert_eq!(
            shortcut(vec![key(M, "M"), ctrl, key(SHIFT, "Shift")]),
            Some("CTRL+SHIFT+m".into())
        );
        assert_eq!(shortcut(vec![key(M, "M"), key(V, "V")]), None);
    }

    #[test]
    fn notes_say_where_keybinds_work() {
        let mut status = Status {
            reach: Reach::Desktop,
            given: HashMap::from([
                (TALK_SHORTCUT.to_owned(), "Ctrl+V".to_owned()),
                ("keybind-2".to_owned(), String::new()),
            ]),
        };
        assert!(status.talk_note().contains("the shortcut Ctrl+V."));
        assert_eq!(
            status.given_note("keybind-2").as_deref(),
            Some("Your desktop hasn't allowed it yet.")
        );
        status.reach = Reach::Focused("Your desktop doesn't offer Yapfast global shortcuts");
        assert_eq!(status.given_note("keybind-2"), None);
        assert_eq!(
            status.page_note(),
            "Your desktop doesn't offer Yapfast global shortcuts, so keybinds work only while Yapfast's window is in front."
        );
    }
}
