//! The Sounds settings page: what it shows and what it asks for. Task S05
//! (docs/iced/tasks/settings.md). The Slint side: `SoundSettings`
//! (`ui/settings.slint:1821`), set from `src/ui/callbacks.rs:2400`-`:2438`.

/// How many sounds there are, in the order of `yapfast_core::sounds::Sound::ALL`:
/// join, leave, mute, unmute, deafen, undeafen, message, mention, viewer
/// joins, viewer leaves, stream starts, stream stops.
pub const COUNT: usize = 12;
/// The sound a message makes, which the volume's preview plays.
pub const MESSAGE: usize = 6;
/// How many sets there are to choose from.
pub const SETS: usize = 6;

/// The Sounds settings page: what its screen draws. The window holds it as
/// `vm.settings.sounds`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sounds {
    /// The chosen set, by its place in the list.
    pub set: usize,
    /// How loud all the sounds are, in percent, up to 200.
    pub volume: i32,
    /// Whether each sound has a slider of its own.
    pub adjust_each: bool,
    /// Each sound's share of the volume in percent, in the order above.
    pub each: [i32; COUNT],
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            set: 0,
            volume: 100,
            adjust_each: false,
            each: [100; COUNT],
        }
    }
}

/// The Sounds settings page: what a click or a key on it asks for. The app
/// keeps the value in the layout and saves the preferences, and plays the
/// preview the doc of the variant names. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A set was chosen; the app plays the first sound in it.
    Set(usize),
    /// The volume moves under a drag: show it, don't save it yet.
    Volume(i32),
    /// The volume was settled; the app plays a message's sound.
    VolumeSettled(i32),
    /// Each sound gets a slider of its own, or loses it.
    AdjustEach(bool),
    /// A sound's share moves under a drag: show it, don't save it yet.
    Each(usize, i32),
    /// A sound's share was settled; the app plays that sound.
    EachSettled(usize, i32),
    /// Play a sound by its place, as it is set now.
    Play(usize),
}

impl Sounds {
    /// What an intent of the page changes in it, for the app's update and
    /// for the simulator's tests.
    pub fn apply(&mut self, intent: &Intent) {
        match *intent {
            Intent::Set(set) => self.set = set.min(SETS - 1),
            Intent::Volume(value) | Intent::VolumeSettled(value) => {
                self.volume = value.clamp(0, 200);
            }
            Intent::AdjustEach(on) => self.adjust_each = on,
            Intent::Each(index, value) | Intent::EachSettled(index, value) => {
                if let Some(share) = self.each.get_mut(index) {
                    *share = value.clamp(0, 100);
                }
            }
            Intent::Play(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_is_kept_in_its_range() {
        let mut page = Sounds::default();
        page.apply(&Intent::EachSettled(3, 140));
        page.apply(&Intent::Each(99, 5));
        page.apply(&Intent::Volume(-4));
        assert_eq!((page.each[3], page.volume), (100, 0));
        page.apply(&Intent::Set(40));
        assert_eq!(page.set, SETS - 1);
    }
}
