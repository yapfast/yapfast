//! Renderer choice at startup and recovery from a failed GPU start.
//!
//! On Linux and Windows Yapfast renders with Skia Software. In a ten-person call
//! it used about half the memory of Skia GPU at the same CPU, and scrolling a long
//! history at 2× scale cost less CPU too. On the Mac it renders with Skia GPU,
//! through Metal: there Skia Software hands the whole window to the compositor on
//! every frame, and scrolling cost it about three times the CPU at two thirds of
//! the frame rate, with more memory, not less. The GPU is a setting, which
//! `--renderer` overrides for one start; if the GPU fails, Yapfast restarts itself
//! with Skia Software, says why and turns the setting off. Slint picks one renderer
//! per process and never unloads the graphics driver, so a changed setting applies
//! when Yapfast restarts.
//!
//! Skia GPU draws through OpenGL on Linux, through Direct3D 12 on Windows and
//! through Metal on the Mac. On Linux, NVIDIA's OpenGL driver kept a core busy
//! while Yapfast drew with it.

/// Whether Yapfast draws with the GPU when the setting was never changed.
pub const GPU_BY_DEFAULT: bool = cfg!(target_os = "macos");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    Gpu,
    Software,
}

/// The renderer for this process, with the reason for a fallback from the GPU.
pub struct Plan {
    pub renderer: Renderer,
    pub notice: Option<String>,
    /// The GPU setting failed, so it's turned off for later starts.
    pub forget_gpu: bool,
}

/// `requested` comes from `--renderer`, `saved_gpu` from the settings, and
/// `relaunch_reason` is set when this process replaced a failed GPU process.
pub fn plan(requested: Option<Renderer>, saved_gpu: bool, relaunch_reason: Option<String>) -> Plan {
    let renderer = requested.unwrap_or(if saved_gpu {
        Renderer::Gpu
    } else {
        Renderer::Software
    });
    let failed = renderer == Renderer::Software && relaunch_reason.is_some();
    Plan {
        renderer,
        notice: failed.then(|| NOTICE.into()),
        forget_gpu: failed && saved_gpu,
    }
}

/// The technical reason stays in the log; the notice is for people.
const NOTICE: &str = "GPU rendering failed to start, so Yapfast is using software rendering.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_restart_after_a_gpu_failure_shows_the_notice() {
        assert!(plan(Some(Renderer::Software), false, None).notice.is_none());
        assert!(plan(Some(Renderer::Gpu), false, None).notice.is_none());
        let restarted = plan(
            Some(Renderer::Software),
            false,
            Some("no EGL display".into()),
        );
        assert_eq!(restarted.renderer, Renderer::Software);
        assert_eq!(restarted.notice.as_deref(), Some(NOTICE));
        assert!(!restarted.forget_gpu);
    }

    #[test]
    fn the_setting_chooses_unless_the_flag_does() {
        assert_eq!(plan(None, false, None).renderer, Renderer::Software);
        assert_eq!(plan(None, true, None).renderer, Renderer::Gpu);
        assert_eq!(
            plan(Some(Renderer::Software), true, None).renderer,
            Renderer::Software
        );
        assert_eq!(
            plan(Some(Renderer::Gpu), false, None).renderer,
            Renderer::Gpu
        );
        assert!(!plan(None, true, None).forget_gpu);
    }

    #[test]
    fn a_failed_gpu_setting_is_turned_off() {
        // The failed process restarts with `--renderer software`.
        let restarted = plan(
            Some(Renderer::Software),
            true,
            Some("no EGL display".into()),
        );
        assert_eq!(restarted.renderer, Renderer::Software);
        assert!(restarted.forget_gpu && restarted.notice.is_some());
    }
}
