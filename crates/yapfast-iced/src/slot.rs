//! Where a screen the window opens lives while it's open: a [`Slot`],
//! emptied only through [`Slot::close`], which runs the screen's own
//! close, so nothing it started outlives it.

use crate::ctx::Cx;

/// Why a screen closes: the person closed it, or the window went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    User,
    Window,
}

/// What a screen does as it closes, before it's dropped.
pub trait Closes {
    fn close(self, cx: &mut Cx<'_>, why: Why);
}

/// A screen while it's open; empty while it's closed.
pub struct Slot<T>(Option<Box<T>>);

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Closes> Slot<T> {
    /// Opens `screen` here; one already open stays.
    pub fn open(&mut self, screen: impl FnOnce() -> T) -> &mut T {
        self.0.get_or_insert_with(|| Box::new(screen()))
    }

    /// Closes the screen, if one is open, running its close.
    pub fn close(&mut self, cx: &mut Cx<'_>, why: Why) -> bool {
        match self.0.take() {
            Some(screen) => {
                screen.close(cx, why);
                true
            }
            None => false,
        }
    }

    pub fn get(&self) -> Option<&T> {
        self.0.as_deref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.0.as_deref_mut()
    }

    pub fn is_open(&self) -> bool {
        self.0.is_some()
    }
}
