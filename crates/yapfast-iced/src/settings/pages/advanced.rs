//! Advanced: starting at sign-in, Discord links, the tray, more windows,
//! the GPU for drawing and for streams, and Developer Mode.

use iced::{Element, widget::text};

use super::Pref;
use crate::{
    app::{App, Message},
    form::{self},
};

impl App {
    pub(in crate::settings) fn advanced_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let mut parts = vec![form::title(theme, "Advanced")];
        let platform = self
            .settings
            .as_ref()
            .map(|settings| settings.platform)
            .unwrap_or_default();
        if let Some(start) = platform.autostart {
            parts.push(self.flip(
                "Start when you sign in",
                "Yapfast starts in the system tray when you sign in to your computer, so messages and calls reach you.",
                start,
                Pref::StartOnSignIn,
            ));
        }
        if let Some(links) = platform.discord_links {
            parts.push(self.flip(
                "Open Discord links in Yapfast",
                "discord:// links from other apps and shortcuts open here instead of Discord. Turning it off gives them back to the app that had them.",
                links,
                Pref::DiscordLinks,
            ));
        }
        let tray = self
            .shell
            .tray
            .as_ref()
            .is_some_and(|tray| tray.available());
        parts.extend([
            self.flip(
                "Close to tray",
                if tray {
                    "Closing the window keeps Yapfast running in the system tray, with any call. Quit from the tray icon's menu."
                } else {
                    "No system tray is running, so closing the window quits Yapfast."
                },
                prefs.close_to_tray.unwrap_or(true),
                Pref::CloseToTray,
            ),
            self.flip(
                "Allow multiple windows",
                "Starting Yapfast while it runs opens another Yapfast, with its own window and tray icon. Off, it brings the open window forward.",
                prefs.multiple_windows,
                Pref::MultipleWindows,
            ),
            self.flip(
                "Render with the GPU",
                "Hardware acceleration. Off, Yapfast draws on the processor, which uses about half the memory.",
                prefs.gpu_rendering,
                Pref::GpuRendering,
            ),
            self.renderer_note(),
            self.flip(
                "Encode streams with the GPU",
                "Screen sharing uses the graphics card's video encoder, which takes almost no processor time and looks better at low bitrates. Off, or without one, Yapfast encodes on the processor.",
                self.layout.voice.gpu_encoding.unwrap_or(true),
                Pref::GpuEncoding,
            ),
            self.flip(
                "Developer Mode",
                "Menus offer Copy ID for servers, channels and people, which helps when you write apps that use the Discord API.",
                self.layout.developer_mode,
                Pref::DeveloperMode,
            ),
            text("Icons from Solar by 480 Design (CC BY 4.0), many of them adapted, and from Lucide (ISC).")
                .size(12)
                .color(theme.faint)
                .into(),
        ]);
        form::page(parts)
    }

    /// What the renderer setting means for this build, which draws with the
    /// one renderer it was built with: the setting is kept for builds that
    /// can follow it.
    pub(super) fn renderer_note(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let gpu = cfg!(feature = "gpu");
        let words = if self.layout.prefs.gpu_rendering == gpu {
            if gpu {
                "This build draws with the GPU (wgpu)."
            } else {
                "This build draws on the processor (tiny-skia)."
            }
        } else if gpu {
            "This build draws with the GPU (wgpu) whatever the setting says: it was built with --features iced-gpu."
        } else {
            "This build draws on the processor (tiny-skia) whatever the setting says: it was built with --features iced."
        };
        form::note(theme, words)
    }
}
