//! Which keybinds hold, from keys going down and up: the one matcher every
//! way of hearing keys feeds, by scan code.

use std::sync::atomic::Ordering;

use super::{Action, PAUSED, Watched, modifier};

/// Which keybinds hold, from keys going down and up. It allocates only when
/// made: each event looks through the few keys bound.
pub(super) struct Matcher {
    /// The scan codes bound, both sides of a modifier in a combination.
    codes: Vec<u32>,
    held: Vec<bool>,
    bindings: Vec<Compiled>,
    /// How many keybinds hold each holding action; see [`Action::hold_slot`].
    holding: [u8; 2],
}

struct Compiled {
    action: Action,
    /// For each key, its index in `codes`, and its other side's for a
    /// modifier in a combination (the same index otherwise).
    keys: Vec<[usize; 2]>,
    active: bool,
    /// Completed by the key going down now.
    fresh: bool,
}

impl Compiled {
    fn uses(&self, index: usize) -> bool {
        self.keys.iter().any(|key| key.contains(&index))
    }

    fn satisfied(&self, held: &[bool]) -> bool {
        self.keys
            .iter()
            .all(|&[one, other]| held[one] || held[other])
    }

    /// Holds every key `other` holds, and more.
    fn covers(&self, other: &Compiled) -> bool {
        self.keys.len() > other.keys.len() && other.keys.iter().all(|key| self.keys.contains(key))
    }
}

pub(super) type Out<'a> = &'a mut dyn FnMut(Action, bool);

impl Matcher {
    pub(super) fn new(watched: &[Watched]) -> Self {
        let mut codes = Vec::new();
        let mut index = |code: u32| match codes.iter().position(|&c| c == code) {
            Some(index) => index,
            None => {
                codes.push(code);
                codes.len() - 1
            }
        };
        let bindings = watched
            .iter()
            .map(|watched| {
                let combination = watched.keys.keys().len() > 1;
                let keys = watched
                    .keys
                    .keys()
                    .iter()
                    .map(|key| match modifier(key.scancode).filter(|_| combination) {
                        Some((left, right, _)) => [index(left), index(right)],
                        None => {
                            let one = index(key.scancode);
                            [one, one]
                        }
                    })
                    .collect();
                Compiled {
                    action: watched.action,
                    keys,
                    active: false,
                    fresh: false,
                }
            })
            .collect();
        let held = vec![false; codes.len()];
        Self {
            codes,
            held,
            bindings,
            holding: [0; 2],
        }
    }

    fn position(&self, code: u32) -> Option<usize> {
        self.codes.iter().position(|&c| c == code)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn bound(&self, code: u32) -> bool {
        self.position(code).is_some()
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn is_held(&self, code: u32) -> bool {
        self.position(code).is_some_and(|index| self.held[index])
    }

    /// Some keybind holds.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn holding(&self) -> bool {
        self.holding.iter().any(|&count| count > 0)
    }

    /// A key went down or came up. A key going down again before coming up
    /// is a repeat and changes nothing. Among the keybinds a key completes,
    /// one that toggles gives way to one that holds all its keys and more,
    /// as Discord's do.
    pub(super) fn key(&mut self, code: u32, down: bool, out: Out) {
        let Some(index) = self.position(code) else {
            return;
        };
        if self.held[index] == down {
            return;
        }
        self.held[index] = down;
        if !down {
            for binding in 0..self.bindings.len() {
                let compiled = &self.bindings[binding];
                if compiled.active && compiled.uses(index) && !compiled.satisfied(&self.held) {
                    self.deactivate(binding, out);
                }
            }
            return;
        }
        if PAUSED.load(Ordering::Relaxed) {
            return;
        }
        let mut any = false;
        for compiled in &mut self.bindings {
            compiled.fresh =
                !compiled.active && compiled.uses(index) && compiled.satisfied(&self.held);
            any |= compiled.fresh;
        }
        if !any {
            return;
        }
        for binding in 0..self.bindings.len() {
            let compiled = &self.bindings[binding];
            if !compiled.fresh {
                continue;
            }
            let shadowed = !compiled.action.holds()
                && self
                    .bindings
                    .iter()
                    .any(|other| other.fresh && other.covers(compiled));
            self.activate(binding, !shadowed, out);
        }
    }

    /// The desktop says a keybind went down or came up, as the portal does,
    /// having matched its keys itself.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(super) fn set(&mut self, binding: usize, down: bool, out: Out) {
        let Some(compiled) = self.bindings.get(binding) else {
            return;
        };
        if down && !compiled.active && !PAUSED.load(Ordering::Relaxed) {
            self.activate(binding, true, out);
        } else if !down && compiled.active {
            self.deactivate(binding, out);
        }
    }

    fn activate(&mut self, binding: usize, fire: bool, out: Out) {
        let compiled = &mut self.bindings[binding];
        compiled.active = true;
        match compiled.action.hold_slot() {
            Some(slot) => {
                self.holding[slot] += 1;
                if self.holding[slot] == 1 {
                    out(compiled.action, true);
                }
            }
            None if fire => out(compiled.action, true),
            None => {}
        }
    }

    fn deactivate(&mut self, binding: usize, out: Out) {
        let compiled = &mut self.bindings[binding];
        compiled.active = false;
        if let Some(slot) = compiled.action.hold_slot() {
            self.holding[slot] -= 1;
            if self.holding[slot] == 0 {
                out(compiled.action, false);
            }
        }
    }

    /// Every key comes up, as when the keys stop coming.
    pub(super) fn release_all(&mut self, out: Out) {
        self.held.fill(false);
        for binding in 0..self.bindings.len() {
            if self.bindings[binding].active {
                self.deactivate(binding, out);
            }
        }
    }

    /// Lets go of keys held here that `is_down` says are up, as a key-up
    /// that never came; all but `except`.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn resync(&mut self, except: Option<u32>, is_down: impl Fn(u32) -> bool, out: Out) {
        for index in 0..self.codes.len() {
            let code = self.codes[index];
            if self.held[index] && Some(code) != except && !is_down(code) {
                self.key(code, false, out);
            }
        }
    }
}
