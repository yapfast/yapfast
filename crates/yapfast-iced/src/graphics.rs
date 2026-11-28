//! Which renderer draws, chosen once as Yapfast starts: the GPU setting,
//! unless `--renderer` says otherwise for this start. A GPU that fails to
//! start ends the window (`Exit::UseSoftware`); `main` starts Yapfast again
//! on the CPU with the reason, and that start says so and turns the setting
//! off. The window system allows one renderer per process, so a changed
//! setting applies from the next start (Settings' Restart Yapfast).

use std::path::Path;

use crate::layout::Layout;
use yapfast_core::i18n::later;

/// The renderer for this start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub gpu: bool,
    /// This start replaced one whose GPU failed: it tells of it.
    pub failed: bool,
}

/// What a start after a failed GPU says. The technical reason stays in the
/// log.
pub const NOTICE: &str =
    later("GPU rendering failed to start, so Yapfast is using software rendering.");

/// The renderer `requested` (`--renderer`) or kept in `layout.json`; after
/// a GPU failure (`failure`), the CPU, with the setting turned off.
pub fn plan(root: &Path, requested: Option<bool>, failure: Option<String>) -> Plan {
    let saved = Layout::read(root).prefs.gpu_rendering;
    let plan = choose(requested, saved, failure.is_some());
    if let Some(reason) = failure {
        tracing::warn!(%reason, "started on the CPU after the GPU failed");
        if saved {
            crate::layout::store(root, "gpu_rendering", false);
        }
    }
    plan
}

fn choose(requested: Option<bool>, saved: bool, failed: bool) -> Plan {
    let gpu = requested.unwrap_or(saved) && !failed;
    Plan {
        gpu,
        failed: failed && !gpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_chooses_unless_the_flag_does() {
        assert!(!choose(None, false, false).gpu);
        assert!(choose(None, true, false).gpu);
        assert!(!choose(Some(false), true, false).gpu);
        assert!(choose(Some(true), false, false).gpu);
    }

    #[test]
    fn a_start_after_a_gpu_failure_draws_on_the_cpu_and_says_so() {
        let plan = choose(Some(false), true, true);
        assert_eq!(
            plan,
            Plan {
                gpu: false,
                failed: true
            }
        );
        assert!(!choose(None, false, false).failed);
    }

    #[test]
    fn the_failure_turns_the_setting_off() {
        let folder = tempfile::tempdir().expect("a folder");
        crate::layout::store(folder.path(), "gpu_rendering", true);
        crate::layout::flush(folder.path());
        assert!(plan(folder.path(), None, None).gpu);
        let after = plan(folder.path(), Some(false), Some("no adapter".into()));
        assert!(after.failed && !after.gpu);
        assert!(!Layout::read(folder.path()).prefs.gpu_rendering);
    }
}
