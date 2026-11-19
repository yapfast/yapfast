//! The device menus at the carets of the microphone's and the headphones'
//! toggles, as Discord's: what they show and what they ask for. Task V09
//! (docs/iced/tasks/voice.md); the Slint side is `QuickDeviceMenu`
//! (`ui/controls.slint:1610`), whose glue is `select_microphone` and
//! `select_speaker` (`src/ui/callbacks.rs:3605`, `:3637`).
//!
//! One menu serves both: `Layer::Devices` opens it from the caret of the
//! microphone's or the headphones' toggle, and [`Devices::kind`] says which.
//! From the top: the device in use under "Input Device" or "Output Device",
//! whose submenu lists the devices; for the microphone, with Discord linked,
//! the input profile under "Input Profile", whose submenu offers the three;
//! the device's volume; a way into Voice settings.
//!
//! The rows are made once by [`Devices::new`] (the view lays out
//! `menu::Row`s by reference, so the view-model holds them). Which submenu
//! shows and where the keys are is the menu's own state, which
//! [`Devices::apply`] moves.

use crate::icon::Icon;
use crate::menu::{Key, Row};
use crate::settings::voice::Profile;

/// Which device the menu is about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The microphone's: its profile comes with it.
    #[default]
    Input,
    /// The speakers' or headphones'.
    Output,
}

/// A row of the menu the pointer and the keys reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Device,
    Profile,
    Settings,
}

/// The words the menu's rows carry, translated by the projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Labels {
    /// "Input Device" or "Output Device".
    pub title: String,
    /// What the submenu says with nothing in it, under "None found":
    /// "No microphones found. ...".
    pub empty: String,
    /// "Voice Settings".
    pub settings: String,
    /// The head's second line with no device: "None found".
    pub none: String,
    /// The head's second line for the system's choice: "System default".
    pub default: String,
}

/// The device menus: what their screen draws. The window holds it as
/// `vm.voice.devices`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Devices {
    pub kind: Kind,
    /// "Input Device" or "Output Device", under it the one in use, on two
    /// lines when long; its submenu has one row per device, the one in use
    /// marked, each asking [`Intent::Select`].
    pub device: Row<Intent>,
    /// "Input Profile", under it the profile the switches make; its submenu
    /// the three, each asking [`Intent::Profile`]. Only the input menu's,
    /// with Discord linked.
    pub profile: Option<Row<Intent>>,
    /// The volume of the device, from 0 to 200 (100 marked).
    pub volume: i32,
    /// "Voice Settings".
    pub settings: Row<Intent>,
    /// The row whose submenu shows.
    pub open: Option<Part>,
    /// The row the keys or the pointer last lit.
    pub current: Option<Part>,
    /// The open submenu's row the keys light, while they're in it.
    pub sub: Option<usize>,
}

impl Devices {
    /// A menu for `names`, the one at `current` in use (`None`: the system's
    /// default). Volume 100, no submenu open.
    pub fn new(kind: Kind, labels: Labels, names: &[String], current: Option<usize>) -> Self {
        let current = current.filter(|&index| index < names.len());
        let subtitle = match current {
            _ if names.is_empty() => labels.none.clone(),
            Some(index) => names[index].clone(),
            None => labels.default.clone(),
        };
        let mut rows: Vec<Row<Intent>> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (Row::new(name.as_str(), Intent::Select(index)))
                    .radio(Some(index) == current)
                    .plain()
            })
            .collect();
        if rows.is_empty() {
            rows.push(
                Row::opens(labels.none, Vec::new())
                    .subtitle(labels.empty)
                    .wraps()
                    .plain()
                    .disabled(true),
            );
        }
        Self {
            kind,
            device: Row::new(labels.title, Intent::Hover(Some(Part::Device)))
                .submenu(rows)
                .subtitle(subtitle)
                .wraps()
                .plain(),
            profile: None,
            volume: 100,
            settings: Row::new(labels.settings, Intent::Settings).icon(Icon::Settings),
            open: None,
            current: None,
            sub: None,
        }
    }

    /// With the input profile's row: `title` over the profile in use, its
    /// submenu the profiles by their `names`, in [`Profile::ALL`]'s order.
    pub fn profile(
        mut self,
        title: impl Into<String>,
        names: [String; 3],
        chosen: Profile,
    ) -> Self {
        let subtitle = names[chosen.index()].clone();
        let rows = Profile::ALL
            .into_iter()
            .zip(names)
            .map(|(profile, name)| {
                (Row::new(name, Intent::Profile(profile)))
                    .radio(profile == chosen)
                    .plain()
            })
            .collect();
        let opens = Intent::Hover(Some(Part::Profile));
        self.profile = Some(
            Row::new(title, opens)
                .submenu(rows)
                .subtitle(subtitle)
                .plain(),
        );
        self
    }

    /// The input menu.
    pub fn input(&self) -> bool {
        self.kind == Kind::Input
    }

    /// The rows the keys move through, in order.
    fn parts(&self) -> Vec<Part> {
        let mut parts = vec![Part::Device];
        if self.profile.is_some() {
            parts.push(Part::Profile);
        }
        parts.push(Part::Settings);
        parts
    }

    /// The submenu of a row; none for Voice Settings.
    pub fn submenu(&self, part: Part) -> &[Row<Intent>] {
        match part {
            Part::Device => &self.device.submenu,
            Part::Profile => self.profile.as_ref().map_or(&[], |row| &row.submenu),
            Part::Settings => &[],
        }
    }

    /// A row of the menu drawn lit: the one whose submenu shows, and the
    /// one the keys are on while they're in the menu.
    pub fn lit(&self, part: Part) -> bool {
        self.open == Some(part) || (self.sub.is_none() && self.current == Some(part))
    }

    /// What the pointer or a key did to the menu's own state. A choice the
    /// keys made comes back, for the app to do as the click would.
    pub fn apply(&mut self, intent: &Intent) -> Option<Intent> {
        match intent {
            Intent::Hover(part) => {
                self.current = *part;
                self.sub = None;
                self.open = part.filter(|&part| !self.submenu(part).is_empty());
                None
            }
            Intent::Key(key) => self.press(*key),
            _ => None,
        }
    }

    /// Made again as what it shows changed, it keeps where the pointer and
    /// the keys were in `was`, as far as its rows still have them.
    pub fn keep(&mut self, was: &Devices) {
        let parts = self.parts();
        self.current = was.current.filter(|part| parts.contains(part));
        self.open = was.open.filter(|&part| !self.submenu(part).is_empty());
        let rows = self.open.map_or(0, |part| self.submenu(part).len());
        self.sub = was.sub.filter(|_| rows > 0).map(|at| at.min(rows - 1));
    }

    fn press(&mut self, key: Key) -> Option<Intent> {
        if let (Some(open), Some(at)) = (self.open, self.sub) {
            let rows = self.submenu(open);
            match key {
                Key::Up | Key::Down => self.sub = Some(step(at, key, rows.len())),
                Key::Enter => {
                    let row = rows.get(at).filter(|row| !row.disabled)?;
                    return row.intent.clone();
                }
                Key::Left | Key::Escape => (self.open, self.sub) = (None, None),
                Key::Right => {}
            }
            return None;
        }
        let parts = self.parts();
        let at = self
            .current
            .and_then(|part| parts.iter().position(|&p| p == part));
        match (key, self.current) {
            (Key::Up | Key::Down, _) => {
                let next = match at {
                    Some(at) => step(at, key, parts.len()),
                    None if key == Key::Down => 0,
                    None => parts.len() - 1,
                };
                self.current = Some(parts[next]);
                if self.open != self.current {
                    self.open = None;
                }
            }
            (Key::Enter, Some(Part::Settings)) => return Some(Intent::Settings),
            (Key::Right | Key::Enter, Some(part)) if !self.submenu(part).is_empty() => {
                self.open = Some(part);
                self.sub = Some(0);
            }
            (Key::Left | Key::Escape, _) => self.open = None,
            _ => {}
        }
        None
    }
}

/// The place one up or down from `at` among `len`, wrapping.
fn step(at: usize, key: Key, len: usize) -> usize {
    let by: isize = if key == Key::Up { -1 } else { 1 };
    (at as isize + by).rem_euclid(len.max(1) as isize) as usize
}

/// The device menus: what a click or a key on them asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The pointer came onto a row of the menu, or a click on one: it
    /// lights and its submenu opens, closing another; `None`, onto the
    /// volume, which closes it.
    Hover(Option<Part>),
    /// A key, while the menu has them ([`Devices::apply`]).
    Key(Key),
    /// Use the device at this index of the list: the app switches to it
    /// and the menu closes.
    Select(usize),
    /// Use an input profile: a preset sets the switches as Voice & video
    /// does; Custom opens it.
    Profile(Profile),
    /// The volume moves, while dragging or by a key: the call follows.
    Volume(i32),
    /// The volume is kept (after a drag or a key): the app saves it.
    Commit(i32),
    /// Opens Voice settings.
    Settings,
}

#[cfg(test)]
mod tests;
