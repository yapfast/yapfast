//! The test binary's allocator: the system's, counting the bytes a thread
//! holds and the most it held while that thread asks it to ([`measure`]).
//! Other threads' allocations, other tests' among them, never count, so a
//! measure holds under any load, unlike the process's resident memory.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    /// Bytes held since counting began on this thread, and the most held
    /// since; none while it doesn't count.
    static HELD: Cell<Option<(isize, isize)>> = const { Cell::new(None) };
}

fn count(delta: isize) {
    let _ = HELD.try_with(|held| {
        if let Some((now, most)) = held.get() {
            let now = now + delta;
            held.set(Some((now, most.max(now))));
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

/// What `work` returns, the bytes it left held on this thread (what it
/// returns among them) and the most it held at once.
pub fn measure<T>(work: impl FnOnce() -> T) -> (T, isize, isize) {
    HELD.with(|held| held.set(Some((0, 0))));
    let made = work();
    let (held, most) = HELD.with(|held| held.replace(None)).unwrap_or_default();
    (made, held, most)
}
