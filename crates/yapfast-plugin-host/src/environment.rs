//! The environment the helper refuses to inherit.
//!
//! Extism reads `EXTISM_*` variables for behaviour this process can't allow.
//! `EXTISM_ENABLE_WASI_OUTPUT` sends a guest's standard output to the host's,
//! and the host's standard output is the protocol's frame stream, so a single
//! guest `print` would corrupt every frame after it. `EXTISM_CACHE_CONFIG` moves
//! the compile cache Yapfast chose.
//!
//! The parent starts the helper with a cleared environment, so this is the
//! second line rather than the first: a helper started by hand, or by a
//! measurement harness, does not get that treatment. Dropping the switches here
//! makes the guarantee hold however the process was started.

use std::ffi::OsStr;

/// The prefix Extism's switches share.
const PREFIX: &[u8] = b"EXTISM_";

/// Whether `name` is one of Extism's environment switches.
///
/// Windows environment names are case-insensitive while Extism's own lookup is
/// not, so the comparison ignores ASCII case: `extism_cache_config` is read by
/// Extism on Windows and has to be dropped here too. It compares bytes, so a
/// name that starts with non-ASCII text cannot panic a slice.
pub fn is_extism_variable(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    let bytes = name.as_bytes();
    bytes.len() >= PREFIX.len() && bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
}

/// Removes every Extism switch from this process's environment, and returns how
/// many it dropped.
///
/// # Safety
///
/// [`std::env::remove_var`] is unsafe in edition 2024 because the C environment
/// is shared with every thread: changing it while another thread reads or writes
/// it is undefined behaviour. The helper calls this from `main`, before Extism,
/// wasmtime or a worker thread has started, and nothing here mutates the
/// environment afterwards. A caller that has already started a thread must not
/// call it.
pub unsafe fn clear_extism_variables() -> usize {
    // Collected first: `vars_os` holds a lock of its own, and removing a
    // variable while walking what it yielded would be a change under it.
    let switches: Vec<_> = std::env::vars_os()
        .map(|(name, _value)| name)
        .filter(|name| is_extism_variable(name))
        .collect();
    for name in &switches {
        // Safety: the caller promises no other thread exists yet.
        unsafe { std::env::remove_var(name) };
    }
    switches.len()
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
            "extism_cache_config",
            "Extism_Timeout",
        ] {
            assert!(is_extism_variable(OsStr::new(name)), "{name}");
        }
    }

    #[test]
    fn anything_else_is_left_alone() {
        for name in [
            "YAPFAST_LOG",
            "PATH",
            "EXTIS",
            "EXTISM",
            "MY_EXTISM_VARIABLE",
            "",
        ] {
            assert!(!is_extism_variable(OsStr::new(name)), "{name}");
        }
    }
}
