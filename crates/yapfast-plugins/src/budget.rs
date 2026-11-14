//! What the person allows the helper, kept between runs.
//!
//! The two Advanced settings — the helper's whole memory budget and how long
//! an instance may sit without a call — live in `budget.json` in the plugins
//! directory, beside the lock file, so a choice made once is still there next
//! time. The defaults are [`Limits::default`]: 256 MiB and five minutes, the
//! caps the helper ran with before there was a choice.
//!
//! A file that isn't the shape this build writes is not guessed at and not
//! moved aside: the numbers fall back to their defaults with a line in the
//! log, and the file is left exactly where it is. Unlike a grants file, a
//! budget nobody can read is nobody's answers — it is two numbers — and
//! deleting a person's file over a typo is worse than starting from the
//! default for one run.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use yapfast_plugin_api::limits::Limits;

use crate::json_file::write_atomically;

/// The shape of `budget.json`. A file with another number was written by
/// another version: its numbers are not read as if they were this one's.
const VERSION: u32 = 1;

/// The file's name, in the plugins directory.
const FILE: &str = "budget.json";

/// The file's contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    memory_mib: u32,
    idle_minutes: u32,
}

impl From<Limits> for Stored {
    fn from(limits: Limits) -> Self {
        Stored {
            version: VERSION,
            memory_mib: limits.memory_mib,
            idle_minutes: limits.idle_minutes,
        }
    }
}

/// Reads `<plugins>/budget.json`, or the defaults.
///
/// A missing file is a fresh start and says nothing. Anything else that can't
/// be read as what this build writes — malformed, another version, an
/// unreadable file — warns and falls back to the defaults, leaving the file
/// where it is. A number that isn't one of the offered steps is snapped to
/// the nearest rather than refused, as [`Limits::snapped`] explains.
pub(crate) fn load(plugins: &Path) -> Limits {
    let path = plugins.join(FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Limits::default(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "the plugins' budget file couldn't be read; the helper runs with the default limits"
            );
            return Limits::default();
        }
    };
    match serde_json::from_slice::<Stored>(&bytes) {
        Ok(stored) if stored.version == VERSION => Limits {
            memory_mib: stored.memory_mib,
            idle_minutes: stored.idle_minutes,
        }
        .snapped(),
        Ok(stored) => {
            tracing::warn!(
                path = %path.display(),
                found = stored.version,
                writes = VERSION,
                "the plugins' budget file was written by another version; the helper runs with \
                 the default limits"
            );
            Limits::default()
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "the plugins' budget file isn't the one this build writes; the helper runs with \
                 the default limits"
            );
            Limits::default()
        }
    }
}

/// Writes the choice down, replacing `budget.json` whole.
///
/// The caller has already [`Limits::snapped`] what it passes here: what is
/// written is what the person chose, and reading it back gives the same pair.
pub(crate) fn save(plugins: &Path, limits: Limits) -> Result<()> {
    let text = serde_json::to_vec_pretty(&Stored::from(limits))
        .context("could not write the plugins' budget as JSON")?;
    write_atomically(&plugins.join(FILE), &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_round_trips() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let chosen = Limits {
            memory_mib: 512,
            idle_minutes: 15,
        };
        save(directory.path(), chosen).expect("it is written down");
        assert_eq!(load(directory.path()), chosen, "and read back the same");
    }

    #[test]
    fn a_number_off_the_offered_steps_is_snapped_rather_than_refused() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            directory.path().join(FILE),
            r#"{"version":1,"memory_mib":300,"idle_minutes":3}"#,
        )
        .expect("a hand-edited file");
        assert_eq!(
            load(directory.path()),
            Limits {
                memory_mib: 256,
                idle_minutes: 1
            },
            "the nearest step, and the lower one on a tie"
        );
    }

    #[test]
    fn a_file_this_build_cannot_read_is_left_alone_and_the_defaults_are_used() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join(FILE);
        for nonsense in [
            &b"not json at all"[..],
            br#"{"version":99,"memory_mib":512,"idle_minutes":15}"#,
            br#"{"version":1,"memory_mib":512,"idle_minutes":15,"extra":true}"#,
        ] {
            std::fs::write(&path, nonsense).expect("a file to refuse");
            assert_eq!(
                load(directory.path()),
                Limits::default(),
                "a malformed or future file falls back to the defaults"
            );
            assert_eq!(
                std::fs::read(&path).expect("the file stays"),
                nonsense,
                "its contents are left for the person to look at"
            );
        }
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_stays_missing() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        assert_eq!(load(directory.path()), Limits::default());
        assert!(!directory.path().join(FILE).exists());
    }
}
