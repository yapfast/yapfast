// Yapfast: not part of upstream Slint. See docs/building.md in Yapfast.

//! A theme's mouse cursors: pictures the app gives in place of the system's cursors, by kind.
//! The window asks [`custom`] before it hands a built-in cursor to its adapter, which shows a
//! custom one as the winit backend does any `MouseCursor.custom`.

use crate::cursor::MouseCursorInner;
use crate::graphics::Image;
use crate::items::BuiltInMouseCursor;
use alloc::boxed::Box;
use core::cell::{Cell, RefCell};

/// Gives the picture and hotspot, in its pixels, for a kind of cursor at a scale factor, or
/// none for the system's cursor.
pub type Hook = Box<dyn Fn(BuiltInMouseCursor, f32) -> Option<(Image, i32, i32)>>;

std::thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static GENERATION: Cell<u32> = const { Cell::new(0) };
}

/// Sets the cursors in place of the system's, or puts the system's back with `None`. Windows
/// show the change at the pointer's next move.
pub fn set(hook: Option<Hook>) {
    HOOK.with(|slot| *slot.borrow_mut() = hook);
    GENERATION.set(GENERATION.get().wrapping_add(1));
}

/// Counts the calls to [`set`], so a window knows to show its cursor again.
pub(crate) fn generation() -> u32 {
    GENERATION.get()
}

/// The cursor the adapter is given for `cursor`.
pub(crate) fn custom(cursor: MouseCursorInner, scale: f32) -> MouseCursorInner {
    let MouseCursorInner::BuiltIn(kind) = cursor else {
        return cursor;
    };
    if kind == BuiltInMouseCursor::None {
        return cursor;
    }
    HOOK.with(|slot| {
        slot.borrow().as_ref().and_then(|hook| hook(kind, scale)).map_or(
            cursor,
            |(image, hotspot_x, hotspot_y)| MouseCursorInner::CustomMouseCursor {
                image,
                hotspot_x,
                hotspot_y,
            },
        )
    })
}
