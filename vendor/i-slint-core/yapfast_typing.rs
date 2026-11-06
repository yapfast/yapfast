// Yapfast: not part of upstream Slint. See docs/building.md in Yapfast.

//! Keys nothing took, as after a menu closes: the app may act on them, as Discord's window sends
//! typing anywhere to its composer and takes Escape from anywhere.

use crate::input::key_codes;
use alloc::boxed::Box;
use core::cell::{Cell, RefCell};

std::thread_local! {
    static HOOK: RefCell<Option<Box<dyn Fn(&str) -> bool>>> = const { RefCell::new(None) };
    static DELIVERING: Cell<bool> = const { Cell::new(false) };
}

/// Sets what's given a key no item took. It returns whether it took it: for text, Backspace and
/// Delete, by moving the focus to the item that should get the key.
pub fn on_unclaimed(hook: impl Fn(&str) -> bool + 'static) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// What became of a key offered to the app.
pub(crate) enum Offer {
    Declined,
    Taken,
    /// The focus moved for it, and the key goes on to the item now focused.
    Deliver,
}

/// Offers a key no item took to the app. Enter isn't offered: Discord's sends the draft, which a
/// stray press after a menu closes shouldn't.
pub(crate) fn offer(text: &str, nothing_focused: bool) -> Offer {
    if DELIVERING.get() {
        return Offer::Declined;
    }
    let only = |key| text.chars().eq([key]);
    let done_there = only(key_codes::Escape) || (only(key_codes::Tab) && nothing_focused);
    // Slint's own keys, such as arrows, are in the private use area.
    let edits = only(key_codes::Backspace)
        || only(key_codes::Delete)
        || (!text.is_empty()
            && text.chars().all(|c| !c.is_control() && !('\u{E000}'..='\u{F8FF}').contains(&c)));
    if !(done_there || edits) || !HOOK.with(|slot| slot.borrow().as_ref().is_some_and(|hook| hook(text))) {
        return Offer::Declined;
    }
    if done_there { Offer::Taken } else { Offer::Deliver }
}

/// Runs `process`, which gives a key to the item the app just focused, offering nothing meanwhile.
pub(crate) fn delivering<R>(process: impl FnOnce() -> R) -> R {
    DELIVERING.set(true);
    let result = process();
    DELIVERING.set(false);
    result
}
