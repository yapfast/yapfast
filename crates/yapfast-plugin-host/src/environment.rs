//! The environment the helper will not inherit.
//!
//! Extism reads `EXTISM_*` variables for behaviour this helper cannot allow.
//! `EXTISM_ENABLE_WASI_OUTPUT` sends a guest's stdout to the host's, and the
//! helper's stdout is the protocol's frame stream, so one guest `print` would
//! corrupt every frame after it; `EXTISM_CACHE_CONFIG` moves the compile cache
//! the parent chose. The parent spawns the helper with a cleared environment
//! ([PLAN.md](../PLAN.md#34-extism-configuration)), but the measurement harness
//! and a person at a terminal do not, so the helper drops them itself, in
//! `main`, before anything reads one.

use std::ffi::OsStr;

/// The prefix Extism's switches share.
const PREFIX: &[u8] = b"EXTISM_";

/// Whether `name` is one of Extism's environment switches.
///
/// Windows environment names are case-insensitive while Extism's own lookup is
/// not, so the comparison ignores ASCII case: `extism_cache_config` is read by
/// Extism on Windows and has to be dropped here too. The comparison is on bytes
/// so a name that starts with non-ASCII text cannot panic a slice.
pub fn is_extism_variable(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    let bytes = name.as_bytes();
    bytes.len() >= PREFIX.len() && bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
}

/// Removes every Extism switch from this process's environment.
///
/// # Safety
///
/// [`std::env::remove_var`] is unsafe in edition 2024 because the C environment
/// is shared with every other thread: mutating it while another thread reads or
/// writes it is undefined behaviour. The helper calls this from `main`, before
/// `Runtime::new` and so before Extism, wasmtime or rayon has started a thread,
/// and nothing in the helper mutates the environment afterwards. A caller that
/// has already started a thread must not call it.
pub unsafe fn clear_extism_variables() {
    // Collected first: `vars_os` holds a lock and removing a variable during
    // its iteration would be a mutation of what it is walking.
    let switches: Vec<_> = std::env::vars_os()
        .map(|(name, _value)| name)
        .filter(|name| is_extism_variable(name))
        .collect();
    for name in switches {
        // Safety: the caller promises this runs before any other thread.
        unsafe { std::env::remove_var(name) };
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::is_extism_variable;

    #[test]
    fn extism_switches_are_recognised() {
        for name in [
            "EXTISM_ENABLE_WASI_OUTPUT",
            "EXTISM_CACHE_CONFIG",
            "EXTISM_",
            // Windows environment names are case-insensitive.
            "extism_enable_wasi_output",
            "Extism_Cache_Config",
        ] {
            assert!(is_extism_variable(OsStr::new(name)), "{name} is a switch");
        }
    }

    #[test]
    fn other_variables_are_left_alone() {
        for name in [
            "RAYON_NUM_THREADS",
            "EXTISM",
            "EXTISMX_CACHE_CONFIG",
            "XEXTISM_CACHE_CONFIG",
            "EXTISMÉ_CACHE_CONFIG",
        ] {
            assert!(!is_extism_variable(OsStr::new(name)), "{name} is not one");
        }
    }

    // Removing a variable is exercised by the process test instead of here:
    // `env::remove_var` is unsafe because the environment is process-wide, and
    // a test harness runs tests on more than one thread.
}
