// Yapfast: not part of upstream Slint. See docs/building.md in Yapfast.

//! Typing with nothing to type into, as after a menu closes: the app may take the text, as
//! Discord's composer takes what's typed anywhere in its window.

use alloc::boxed::Box;
use core::cell::RefCell;

std::thread_local! {
    static HOOK: RefCell<Option<Box<dyn Fn(&str) -> bool>>> = const { RefCell::new(None) };
}

/// Sets what's given text typed while no item takes it. It returns whether it took it.
pub fn on_unclaimed(hook: impl Fn(&str) -> bool + 'static) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Offers text no item took to the app; whether it took it.
pub(crate) fn offer(text: &str) -> bool {
    // Printable text only: Slint's own keys, such as arrows and Escape, are in the private use area.
    let printable = !text.is_empty()
        && text.chars().all(|c| !c.is_control() && !('\u{E000}'..='\u{F8FF}').contains(&c));
    printable && HOOK.with(|slot| slot.borrow().as_ref().is_some_and(|hook| hook(text)))
}
