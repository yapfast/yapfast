//! The device menus and the input level: what they show and what they ask
//! for. Task V09 (docs/iced/tasks/voice.md). The Slint side is
//! `QuickDeviceMenu` (`ui/controls.slint:1610`) and `InputLevel`
//! (`ui/voice.slint:1331`), placed by the window's `input-menu` and
//! `output-menu` popups (`ui/yapfast.slint:2073`, `:2101`); the glue is
//! `select_microphone`, `select_speaker`, `metering` and `meter_tick`
//! (`src/ui/callbacks.rs:3605`, `:3637`, `:3838`-`:3881`).
//!
//! One menu serves both: `Layer::Devices` opens it from the caret of the
//! microphone's or the headphones' toggle, and [`Devices::kind`] says which.
//! The input menu adds the live level and, in a Discord call, the noise
//! suppression choice.
//!
//! The rows are made once by [`Devices::new`] (the view lays out `menu::Row`s
//! by reference, so the view-model holds them); [`Devices::choose`] moves the
//! check. The level is the app's to store, only while this layer shows and
//! the window is in front, in a [`Level`] the meter reads as it draws: a
//! level that moved a segment asks a redraw, and builds no view.

use crate::icon::Icon;
use crate::kind::Level;
use crate::menu::Row;

/// Which device the menu is about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The microphone's: its level and noise suppression come with it.
    #[default]
    Input,
    /// The speakers' or headphones'.
    Output,
}

/// How a Discord call removes noise, in the order the menu offers them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Noise {
    #[default]
    DeepFilterNet,
    RNNoise,
    Off,
}

impl Noise {
    pub const ALL: [Self; 3] = [Self::DeepFilterNet, Self::RNNoise, Self::Off];
}

/// The words the menu's rows carry, translated by the projection (this crate
/// translates nothing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Labels {
    /// "Input Device" or "Output Device".
    pub title: String,
    /// What the list says with nothing in it: "No microphones found. ...".
    pub empty: String,
    /// "Voice Settings".
    pub settings: String,
    /// The head's second line with no device: "None found".
    pub none: String,
    /// The head's second line for the system's choice: "System default".
    pub default: String,
}

/// The device menus and the input level: what its screen draws. The window
/// holds it as `vm.voice.devices`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Devices {
    pub kind: Kind,
    /// The words kept after the rows are made: `empty` (read by the view),
    /// `none` and `default` (read by `choose`); the rest moved into the rows.
    pub labels: Labels,
    /// "Input Device" or "Output Device", under it the one in use; opens the
    /// list ([`Intent::ToggleList`]).
    pub head: Row<Intent>,
    /// One row per device, the one in use checked; each asks
    /// [`Intent::Select`].
    pub devices: Vec<Row<Intent>>,
    /// The list shows, in place under the head.
    pub listing: bool,
    /// "Voice Settings".
    pub settings: Row<Intent>,
    /// The volume of the device, from 0 to 200 (100 marked).
    pub volume: i32,
    /// The microphone's level from 0 to 1, stored while the menu shows.
    pub level: Level,
    /// The microphone is muted: the level still shows, faded.
    pub muted: bool,
    /// Why the level can't be shown, if it can't.
    pub note: String,
    /// The noise suppression in use; offered only in the input menu of a
    /// linked Discord account, so `None` hides the choice.
    pub noise: Option<Noise>,
}

impl Devices {
    /// What the list says when there is nothing in it.
    pub fn empty(&self) -> &str {
        &self.labels.empty
    }

    /// A menu for `names`, the one at `current` in use (`None`: the system's
    /// default). Volume 100, the list closed.
    pub fn new(kind: Kind, mut labels: Labels, names: &[String], current: Option<usize>) -> Self {
        let icon = match kind {
            Kind::Input => Icon::Mic,
            Kind::Output => Icon::Headphones,
        };
        let mut menu = Self {
            kind,
            head: Row::new(std::mem::take(&mut labels.title), Intent::ToggleList).icon(icon),
            devices: names
                .iter()
                .enumerate()
                .map(|(index, name)| Row::new(name.as_str(), Intent::Select(index)))
                .collect(),
            settings: Row::new(std::mem::take(&mut labels.settings), Intent::Settings)
                .icon(Icon::Settings),
            volume: 100,
            labels,
            ..Self::default()
        };
        menu.choose(current);
        menu
    }

    /// The device at `index` is in use: it is checked, the head names it and
    /// the list closes, as choosing one does.
    pub fn choose(&mut self, index: Option<usize>) {
        let current = index.filter(|&index| index < self.devices.len());
        for (at, row) in self.devices.iter_mut().enumerate() {
            row.checked = Some(at) == current;
        }
        self.head.subtitle = match current {
            _ if self.devices.is_empty() => self.labels.none.clone(),
            Some(index) => self.devices[index].text.clone(),
            None => self.labels.default.clone(),
        };
        self.listing = false;
    }

    /// The input menu: it shows the level and may offer noise suppression.
    pub fn input(&self) -> bool {
        self.kind == Kind::Input
    }

    /// The level as the meter draws it, from 0 to 1.
    pub fn shown_level(&self) -> f32 {
        self.level.get()
    }

    /// The reason the level can't be shown, if there is one.
    pub fn note(&self) -> Option<&str> {
        Some(self.note.as_str()).filter(|note| !note.is_empty())
    }
}

/// The device menus and the input level: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Opens or closes the list of devices.
    ToggleList,
    /// Use the device at this index of the list: the app switches to it,
    /// restarts the meter on it and closes the list ([`Devices::choose`]).
    Select(usize),
    /// The volume moves, while dragging or by a key: the call follows.
    Volume(i32),
    /// The volume is kept (after a drag or a key): the app saves it.
    Commit(i32),
    /// Noise suppression of Discord calls is chosen and saved.
    Noise(Noise),
    /// Opens Voice settings.
    Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> Labels {
        Labels {
            title: "Output Device".into(),
            empty: "No speakers".into(),
            settings: "Voice Settings".into(),
            none: "None found".into(),
            default: "System default".into(),
        }
    }

    fn names() -> Vec<String> {
        vec!["Speakers".into(), "Headphones".into()]
    }

    #[test]
    fn the_head_names_the_device_in_use() {
        let menu = Devices::new(Kind::Output, labels(), &names(), Some(1));
        assert_eq!(menu.head.subtitle, "Headphones");
        assert!(menu.devices[1].checked && !menu.devices[0].checked);
    }

    #[test]
    fn no_choice_is_the_system_default_and_none_found_says_so() {
        let head = |names: &[String], current| {
            Devices::new(Kind::Input, labels(), names, current)
                .head
                .subtitle
        };
        assert_eq!(head(&names(), None), "System default");
        assert_eq!(head(&[], None), "None found");
        // An index past the list is no choice.
        let menu = Devices::new(Kind::Input, labels(), &names(), Some(7));
        assert!(menu.devices.iter().all(|row| !row.checked));
    }

    #[test]
    fn choosing_closes_the_list() {
        let mut menu = Devices::new(Kind::Input, labels(), &names(), None);
        menu.listing = true;
        menu.choose(Some(0));
        assert!(!menu.listing);
        assert_eq!(menu.head.subtitle, "Speakers");
    }

    #[test]
    fn a_note_is_only_what_was_set() {
        let mut menu = Devices::new(Kind::Input, labels(), &names(), None);
        assert_eq!(menu.note(), None);
        menu.note = "No level here".into();
        assert_eq!(menu.note(), Some("No level here"));
    }
}
