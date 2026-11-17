//! What a thousand paragraphs hold: their texts and their shapes, counted
//! by an allocator that counts only on a thread that asks it to, against
//! the 51 KB a built message row of the version scout held
//! (`docs/iced/iced-notes.md`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use anomi_vm::fixtures::text as samples;

use super::*;

struct Counting;

thread_local! {
    /// Bytes held since counting began on this thread; none while it doesn't count.
    static HELD: Cell<Option<isize>> = const { Cell::new(None) };
}

fn count(delta: isize) {
    let _ = HELD.try_with(|held| {
        if let Some(now) = held.get() {
            held.set(Some(now + delta));
        }
    });
}

// SAFETY: every call goes to the system's allocator unchanged; counting
// only reads and writes a thread-local without a destructor, which doesn't
// allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            count(layout.size() as isize);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            count(layout.size() as isize);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        count(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, size) };
        if !moved.is_null() {
            count(size as isize - layout.size() as isize);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What the version scout measured a built message row at.
const ROW: isize = 51 * 1024;

#[test]
fn a_thousand_paragraphs_hold_less_than_a_row_each() {
    // The font system's caches fill once, whatever comes after.
    for index in 0..8 {
        let text = samples::history(index);
        let _ = tree_of(&mut linked(&text), SIZE.width);
    }
    HELD.with(|held| held.set(Some(0)));
    let built: Vec<(Text, Tree)> = (0..1000)
        .map(|index| {
            let text = samples::history(index);
            let (tree, _) = tree_of(&mut linked(&text), SIZE.width);
            (text, tree)
        })
        .collect();
    let held = HELD.with(|held| held.replace(None)).unwrap_or_default();
    let each = held / built.len() as isize;
    eprintln!("a paragraph holds {each} bytes");
    assert!(each < ROW, "{each} bytes a paragraph");
    drop(built);
}
