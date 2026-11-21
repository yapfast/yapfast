//! Advanced: starting at sign-in, Discord links, the tray, more windows,
//! the GPU for drawing and for streams, the codec a share offers Discord
//! first, and Developer Mode.

use iced::{Element, widget::text};

use super::Pref;
use crate::settings::{Event, Page};
use crate::{
    app::Message,
    controls::{Kind, action, choices},
    form::{self},
    pictures::Icon,
};

/// Stream codec's choices, in [`StreamCodec::ALL`]'s order.
///
/// [`StreamCodec::ALL`]: yapfast_core::stream_worker::StreamCodec::ALL
const STREAM_CODECS: [&str; 4] = ["Automatic", "AV1", "H.265", "H.264"];

impl<'a> Page<'a> {
    pub(in crate::settings) fn advanced_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let mut parts = vec![form::title(theme, "Advanced")];
        let platform = self.s.platform;
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
        let tray = self.cx.tray;
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
        ]);
        parts.extend(self.restart_row());
        parts.extend([
            self.flip(
                "Encode streams with the GPU",
                "Screen sharing uses the graphics card's video encoder, which takes almost no processor time and looks better at low bitrates. Off, or without one, Yapfast encodes on the processor.",
                self.cx.layout.voice.gpu_encoding.unwrap_or(true),
                Pref::GpuEncoding,
            ),
            self.stream_codec(),
            self.flip(
                "Developer Mode",
                "Menus offer Copy ID for servers, channels and people, which helps when you write apps that use the Discord API.",
                self.cx.layout.developer_mode,
                Pref::DeveloperMode,
            ),
            text("Icons from Solar by 480 Design (CC BY 4.0), many of them adapted, and from Lucide (ISC).")
                .size(12)
                .color(theme.faint)
                .into(),
        ]);
        form::page(parts)
    }

    /// The codec a share asks Discord for first, still experimental: it
    /// travels with the next share, since a running one offered its codecs
    /// when it started.
    fn stream_codec(self) -> Element<'a, Message> {
        use yapfast_core::stream_worker::StreamCodec;
        let theme = self.cx.look.theme;
        let current = StreamCodec::ALL
            .iter()
            .position(|codec| *codec == self.cx.layout.voice.stream_codec)
            .unwrap_or(0);
        form::group([
            form::heading(theme, "STREAM CODEC (EXPERIMENTAL)"),
            choices(theme, &STREAM_CODECS, current, |index| {
                Message::Settings(Event::Pref(Pref::StreamCodec(index)))
            }),
            text("What Yapfast asks Discord to send when you share your screen. Automatic offers the best codec this computer's graphics card encodes — AV1, then H.265, then H.264 — and Discord sends the one every person watching can decode. Choose one to ask for it first: H.264 is the one every client decodes, and AV1 or H.265 look better at the same bitrate wherever everyone watching can take them. A codec this machine can't encode is left to the automatic order.")
                .size(13)
                .color(theme.secondary)
                .wrapping(text::Wrapping::Word)
                .into(),
        ])
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
        let words = if gpu {
            "Yapfast switches to the GPU when it restarts. Restarting leaves any call."
        } else {
            "Yapfast switches to the processor when it restarts. Restarting leaves any call."
        };
        let ink = crate::theme::rgb(theme.text);
        let restart = action(
            theme,
            "Restart Yapfast",
            Kind::Plain,
            Some(Message::Settings(Event::Restart)),
        )
        .icon(self.cx.look.icon(Icon::Refresh, 18, ink));
        Some(form::beside(
            text(words).size(14).color(theme.text),
            restart,
        ))
    }
}
