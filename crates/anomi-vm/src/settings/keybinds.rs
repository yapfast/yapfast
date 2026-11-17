//! The Keybinds settings page: what it shows and what it asks for. Task S09
//! (docs/iced/tasks/settings.md). The Slint side: `KeybindSettings` and
//! `KeyRecorder` (`ui/settings.slint:756`, `:713`), set from
//! `src/ui/keybinds.rs`.

/// What a keybind does, in the order of `crate::keybinds::Action::OFFERED`
/// (the page's list of actions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Unassigned,
    PushToTalk,
    PushToMute,
    ToggleMute,
    ToggleDeafen,
    ToggleVoiceMode,
    ToggleGoLive,
    LeaveCall,
}

impl Action {
    /// Every action the page offers, in its list's order.
    pub const ALL: [Action; 8] = [
        Action::Unassigned,
        Action::PushToTalk,
        Action::PushToMute,
        Action::ToggleMute,
        Action::ToggleDeafen,
        Action::ToggleVoiceMode,
        Action::ToggleGoLive,
        Action::LeaveCall,
    ];

    /// Its place in [`Action::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }
}

/// One keybind: its action, its keys' name (empty while none), and what the
/// desktop did with it while the desktop runs keybinds (empty otherwise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bind {
    pub action: Action,
    pub keys: String,
    pub desktop: String,
}

/// The Keybinds settings page: what its screen draws. The window holds it as
/// `vm.settings.keybinds`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keybinds {
    pub rows: Vec<Bind>,
    /// Where keybinds work, empty while unknown (the page then says they
    /// work in any app).
    pub note: String,
    /// The row whose keys are recorded now, and the keys pressed so far. The
    /// Voice page's push-to-talk key records through its own
    /// `talk_recording`; the app stops one when the other starts.
    pub recording: Option<usize>,
    pub recorded: String,
}

/// The Keybinds settings page: what a click or a key on it asks for. The app
/// saves the keybinds and watches them again, and records the keys of the
/// window's key events while `recording` is set (`Escape` pressed first
/// stops). Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// "Add a Keybind": a row with no action and no keys.
    Add,
    /// The row's remove button.
    Remove(usize),
    /// An action was chosen for a row, by its place in [`Action::ALL`].
    Action(usize, usize),
    /// Start (true) or stop (false) recording a row's keys.
    Record(usize, bool),
}

impl Keybinds {
    /// What an intent of the page changes in it, for the app's update and for
    /// the simulator's tests; saving and watching are the app's.
    pub fn apply(&mut self, intent: &Intent) {
        match *intent {
            Intent::Add => self.rows.push(Bind::default()),
            Intent::Remove(row) => {
                if row < self.rows.len() {
                    self.rows.remove(row);
                    self.recording = None;
                    self.recorded.clear();
                }
            }
            Intent::Action(row, action) => {
                if let (Some(bind), Some(&action)) =
                    (self.rows.get_mut(row), Action::ALL.get(action))
                {
                    bind.action = action;
                }
            }
            Intent::Record(row, on) => {
                self.recorded.clear();
                self.recording = (on && row < self.rows.len()).then_some(row);
            }
        }
    }

    /// Whether the row's keys are being recorded.
    pub fn records(&self, row: usize) -> bool {
        self.recording == Some(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_added_chosen_recorded_and_removed() {
        let mut page = Keybinds::default();
        page.apply(&Intent::Add);
        page.apply(&Intent::Add);
        page.apply(&Intent::Action(1, 4));
        page.apply(&Intent::Action(1, 99));
        assert_eq!(page.rows[1].action, Action::ToggleDeafen);
        page.apply(&Intent::Record(1, true));
        assert!(page.records(1) && !page.records(0));
        page.apply(&Intent::Record(5, true));
        assert_eq!(page.recording, None);
        page.apply(&Intent::Record(0, true));
        page.apply(&Intent::Remove(0));
        assert_eq!((page.rows.len(), page.recording), (1, None));
    }
}
