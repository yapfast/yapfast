//! Labels for a row of choices that outlive the frame that builds them.

use std::sync::Mutex;

/// Choice labels the interface keeps for its lifetime: the kit's `choices`
/// borrows its options, and a view returns what it built. A set is kept once
/// per distinct language, so a frame after the first allocates nothing.
pub fn words(set: &[&'static str]) -> &'static [&'static str] {
    static KEPT: Mutex<Vec<&'static [&'static str]>> = Mutex::new(Vec::new());
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = kept.iter().find(|have| **have == set) {
        return found;
    }
    let leaked: &'static [&'static str] = Box::leak(set.to_vec().into_boxed_slice());
    kept.push(leaked);
    leaked
}
