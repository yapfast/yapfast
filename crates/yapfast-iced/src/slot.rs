//! Where a screen the window opens lives while it's open: a [`Slot`],
//! emptied only through [`Slot::close`], which runs the screen's own
//! close, so nothing it started outlives it.

use crate::ctx::Cx;

/// Why a screen closes: the person closed it, the core's state took it
/// away, or the window went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    User,
    Core,
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

    /// A screen the core opens follows the core: while `key` says it's
    /// open, the screen for that key is made (closing one for another
    /// key first), and when it goes the screen closes with [`Why::Core`],
    /// so a close the core started isn't sent to it again.
    pub fn follow<K>(
        &mut self,
        key: Option<K>,
        cx: &mut Cx<'_>,
        same: impl Fn(&T, &K) -> bool,
        make: impl FnOnce(K, &mut Cx<'_>) -> T,
    ) -> Option<&mut T> {
        let Some(key) = key else {
            self.close(cx, Why::Core);
            return None;
        };
        if self.get().is_some_and(|screen| !same(screen, &key)) {
            self.close(cx, Why::Core);
        }
        if self.0.is_none() {
            self.0 = Some(Box::new(make(key, cx)));
        }
        self.get_mut()
    }
}
