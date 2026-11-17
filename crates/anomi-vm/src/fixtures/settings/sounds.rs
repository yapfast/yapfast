//! Samples of the Sounds settings page for tests and shots, after the fake
//! Discord's data: task S05's.

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::frame::Page;
use crate::settings::sounds::Sounds;

/// The first set at 100%, each sound as loud as the volume: the golden
/// screenshot's.
pub fn sounds() -> Sounds {
    Sounds::default()
}

/// The window with the settings open on Sounds.
pub fn page() -> Vm {
    let mut vm = settings(Page::Sounds);
    vm.settings.sounds = sounds();
    vm
}

/// With a slider for each sound, a few set apart from the rest.
pub fn each() -> Vm {
    let mut vm = page();
    vm.settings.sounds.adjust_each = true;
    vm.settings.sounds.set = 3;
    vm.settings.sounds.volume = 140;
    vm.settings.sounds.each[2] = 45;
    vm.settings.sounds.each[7] = 0;
    vm
}
