//! Renderer choice at startup and recovery from a failed GPU start.
//!
//! Yapfast renders with Skia Software. In a ten-person call it used about half the
//! memory of Skia GPU at the same CPU, and scrolling a long history at 2× scale
//! cost less CPU too. The GPU is a setting, which `--renderer` overrides for one
//! start; if the GPU fails, Yapfast restarts itself with Skia Software, says why and
//! turns the setting off. Slint picks one renderer per process and never unloads
//! the graphics driver, so a changed setting applies when Yapfast restarts.
//!
//! Skia GPU draws through Vulkan on Linux where the build carries Skia's Vulkan
//! and a device opens, and through OpenGL otherwise; on Windows through
//! Direct3D 12. There, NVIDIA's OpenGL driver kept a core busy while Yapfast
//! drew with it. The Vulkan device is the one a stream's picture can be shown
//! from without a copy; see `src/ui/vulkan.rs` and `src/ui/gpu.rs`.

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

/// What a window whose GPU setting is on came up drawing through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    /// Skia on Vulkan, through wgpu 30: the device a stream's own picture can
    /// be shown from, without a copy through the CPU.
    Vulkan,
    /// Skia on OpenGL, which is what Linux drew with before Vulkan, and what
    /// it keeps where the build or the machine has no Vulkan.
    OpenGl,
}

/// Which of the two the window draws with.
///
/// `carried` is whether this build carries Skia's Vulkan, and `opened` whether
/// a Vulkan device opened here. Either one missing keeps OpenGL: a device that
/// won't open is a fallback and never a startup failure, and what it costs is
/// the picture path rather than the window. Windows draws with Direct3D 12 and
/// asks neither.
pub fn presentation(carried: bool, opened: bool) -> Presentation {
    match carried && opened {
        true => Presentation::Vulkan,
        false => Presentation::OpenGl,
    }
}

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

    /// Vulkan needs both halves: a build with Skia's Vulkan, and a device that
    /// opens here. Either one missing is the OpenGL path Yapfast had before.
    #[test]
    fn vulkan_needs_the_build_and_a_device() {
        use super::Presentation::{OpenGl, Vulkan};
        assert_eq!(presentation(true, true), Vulkan);
        assert_eq!(presentation(true, false), OpenGl);
        assert_eq!(presentation(false, true), OpenGl);
        assert_eq!(presentation(false, false), OpenGl);
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
