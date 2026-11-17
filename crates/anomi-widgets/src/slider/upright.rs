//! `UprightVolumeSlider` (`ui/controls.slint:1737`): how loud something
//! sounds, from 0% to 100%, on an upright track filled from the bottom, as
//! Discord's desktop voice message player has it. Up and Down move 5% at a
//! time, Page Up and Page Down 20%. It only asks for the next value
//! (`on_change`); the player keeps it.

use super::dial::Dial;
use super::track::{Axis, Span};

/// The arrow keys' step and the page keys', in percent.
const STEP: i32 = 5;
const PAGE: i32 = 20;

/// An upright slider at `value` percent; it is 24 px wide and as tall as it
/// is given.
pub fn upright<'a, Message: Clone + 'a>(
    value: i32,
    on_change: impl Fn(i32) -> Message + 'a,
) -> Dial<'a, Message> {
    Dial::new(Axis::Up, Span { min: 0, max: 100 }, value, on_change)
        .step(STEP)
        .page(PAGE)
}
