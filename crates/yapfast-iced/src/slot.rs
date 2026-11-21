//! Where a screen the window opens lives while it's open: a [`Slot`],
//! emptied only through [`Slot::close`], which runs the screen's own
//! close, so nothing it started outlives it. A screen that closes as a
//! popup does ([`Closes::LEAVES`]) is kept, closed, while it's drawn
//! going ([`Slot::leaving`]).

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
    /// Whether it's drawn going once it's closed, as a popup closes
    /// (`widgets::motion`): its close has run by then, so what it shows
    /// is what it showed.
    const LEAVES: bool = false;

    fn close(&mut self, cx: &mut Cx<'_>, why: Why);
}

/// A screen while it's open; empty while it's closed, but for one
/// [`Closes::LEAVES`] kept while it's drawn going.
pub struct Slot<T> {
    open: Option<Box<T>>,
    leaving: Option<Box<T>>,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            open: None,
            leaving: None,
        }
    }
}

impl<T: Closes> Slot<T> {
    /// Opens `screen` here; one already open stays, and one going goes.
    pub fn open(&mut self, screen: impl FnOnce() -> T) -> &mut T {
        self.leaving = None;
        self.open.get_or_insert_with(|| Box::new(screen()))
    }

    /// Closes the screen, if one is open, running its close; one that
    /// [`Closes::LEAVES`] is kept going, unless the window went.
    pub fn close(&mut self, cx: &mut Cx<'_>, why: Why) -> bool {
        if why == Why::Window {
            self.leaving = None;
        }
        match self.open.take() {
            Some(mut screen) => {
                screen.close(cx, why);
                if T::LEAVES && why != Why::Window {
                    self.leaving = Some(screen);
                }
                true
            }
            None => false,
        }
    }

    pub fn get(&self) -> Option<&T> {
        self.open.as_deref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.open.as_deref_mut()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The screen closed, while it's drawn going.
    pub fn leaving(&self) -> Option<&T> {
        self.leaving.as_deref()
    }

    /// The screen closed has gone.
    pub fn gone(&mut self) {
        self.leaving = None;
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
        if self.open.is_none() {
            self.leaving = None;
            self.open = Some(Box::new(make(key, cx)));
        }
        self.get_mut()
    }
}
