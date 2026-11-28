//! Advanced: starting at sign-in, Discord links, the tray, more windows,
//! the GPU for drawing and for streams, and Developer Mode.

use iced::{Element, widget::text};

use super::{Pref, pref};
use crate::settings::{Event, Page};
use crate::{
    app::Message,
    controls::{Kind, action},
    desktop::Autostart,
    form::{self, switch},
    i18n::Key,
    pictures::Icon,
};

impl<'a> Page<'a> {
    pub(in crate::settings) fn advanced_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let tr = |key| self.cx.i18n.text(key);
        let mut parts = vec![form::title(theme, tr(Key::Advanced))];
        let platform = self.s.platform;
        if let Some(start) = platform.autostart {
            // Discord's OS_OPEN_ON_STARTUP and OS_START_MINIMIZED: the second
            // greyed out and shown off while the first is off.
            let open = start != Autostart::Off;
            let minimized = start == Autostart::Tray;
            parts.push(
                switch(
                    tr(Key::OpenOnStartup),
                    open,
                    Some(pref(Pref::OpenOnStartup(!open))),
                )
                .view(self.cx.look),
            );
            parts.push(
                switch(
                    tr(Key::StartMinimized),
                    minimized,
                    open.then(|| pref(Pref::StartMinimized(!minimized))),
                )
                .description(tr(Key::StartMinimizedDescription))
                .view(self.cx.look),
            );
        }
        if let Some(links) = platform.discord_links {
            parts.push(self.flip(
                tr(Key::DiscordLinks),
                tr(Key::DiscordLinksDescription),
                links,
                Pref::DiscordLinks,
            ));
        }
        let tray = self.cx.tray;
        parts.extend([
            self.flip(
                tr(Key::CloseToTray),
                tr(if tray {
                    Key::CloseToTrayDescription
                } else {
                    Key::NoTrayDescription
                }),
                prefs.close_to_tray.unwrap_or(true),
                Pref::CloseToTray,
            ),
            self.flip(
                tr(Key::MultipleWindows),
                tr(Key::MultipleWindowsDescription),
                prefs.multiple_windows,
                Pref::MultipleWindows,
            ),
            self.flip(
                tr(Key::GpuRendering),
                tr(Key::GpuRenderingDescription),
                prefs.gpu_rendering,
                Pref::GpuRendering,
            ),
        ]);
        parts.extend(self.restart_row());
        parts.extend([
            self.flip(
                tr(Key::GpuEncoding),
                tr(Key::GpuEncodingDescription),
                self.cx.layout.voice.gpu_encoding.unwrap_or(true),
                Pref::GpuEncoding,
            ),
            self.flip(
                tr(Key::DeveloperMode),
                tr(Key::DeveloperModeDescription),
                self.cx.layout.developer_mode,
                Pref::DeveloperMode,
            ),
            text(tr(Key::IconCredits))
                .size(12)
                .color(theme.faint)
                .into(),
        ]);
        form::page(parts)
    }

    /// While the GPU setting differs from what draws now: when it takes
    /// effect, and Restart Yapfast. The renderer is chosen once as Yapfast
    /// starts (`graphics::plan`).
    fn restart_row(self) -> Option<Element<'a, Message>> {
        let gpu = self.cx.layout.prefs.gpu_rendering;
        if gpu == crate::renderer::gpu() {
            return None;
        }
        let theme = self.cx.look.theme;
        let words = self.cx.i18n.text(if gpu {
            Key::RestartForGpu
        } else {
            Key::RestartForCpu
        });
        let ink = crate::theme::rgb(theme.text);
        let restart = action(
            theme,
            self.cx.i18n.text(Key::Restart),
            Kind::Plain,
            Some(crate::app::settings(Event::Restart)),
        )
        .icon(self.cx.look.icon(Icon::Refresh, 18, ink));
        Some(form::beside(
            text(words).size(14).color(theme.text),
            restart,
        ))
    }
}
