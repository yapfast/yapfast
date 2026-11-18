//! Settings → Plugins: the plugins installed, what each is doing, and the
//! switch that runs it.
//!
//! The page draws a copy of what the plugin runtime knows
//! (`yapfast_core::plugins::page::View`, read through
//! `Application::plugins_page`), and what the person does with a row goes
//! back as a `page::Request`, which the runtime answers. Nothing is guessed
//! here: a switch that was flipped shows the state the runtime settles it
//! in, carried back as an effect and synced into the next view.
//!
//! A build without the `plugins` feature carries the section and says so,
//! as the Slint adapter did, rather than hiding it.

#[cfg(feature = "plugins")]
use super::Event;
use super::{Page, Settings};
use crate::{app::Message, ctx::Cx, form};

#[cfg(not(feature = "plugins"))]
use iced::Element;

#[cfg(feature = "plugins")]
use crate::{fonts::BOLD, theme::Theme};
#[cfg(feature = "plugins")]
use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, row, text},
};

/// What the Plugins page sends.
#[derive(Clone, Debug)]
pub enum PluginEvent {
    /// Runs a plugin or stops it.
    SetEnabled { id: String, enabled: bool },
    /// The page opened or closed, so the runtime samples the helper's
    /// memory while it's up and drops the marketplace's catalog when it
    /// isn't.
    Opened(bool),
}

#[cfg(feature = "plugins")]
fn plugin_message(event: PluginEvent) -> Message {
    Message::Settings(Event::Plugin(event))
}

impl Settings {
    #[cfg_attr(
        not(feature = "plugins"),
        allow(
            unused_variables,
            clippy::needless_pass_by_ref_mut,
            reason = "the signature is the one the feature builds; without it the                       request goes nowhere"
        )
    )]
    pub(in crate::settings) fn plugin(&self, event: PluginEvent, cx: &mut Cx<'_>) {
        match event {
            #[cfg(feature = "plugins")]
            PluginEvent::SetEnabled { id, enabled } => {
                cx.plugin_request(yapfast_core::plugins::page::Request::SetEnabled {
                    plugin: id,
                    enabled,
                });
            }
            #[cfg(not(feature = "plugins"))]
            PluginEvent::SetEnabled { .. } => {}
            #[cfg(feature = "plugins")]
            PluginEvent::Opened(open) => {
                cx.plugin_request(if open {
                    yapfast_core::plugins::page::Request::Open
                } else {
                    yapfast_core::plugins::page::Request::Close
                });
            }
            #[cfg(not(feature = "plugins"))]
            PluginEvent::Opened(_) => {}
        }
    }
}

impl<'a> Page<'a> {
    pub(in crate::settings) fn plugins_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        #[cfg(not(feature = "plugins"))]
        {
            return form::page([form::card(
                theme,
                form::group([
                    form::heading(theme, "Plugins"),
                    form::note(
                        theme,
                        "This build carries no plugin system: no plugin can be \
                         installed or run.",
                    ),
                ]),
            )]);
        }
        #[cfg(feature = "plugins")]
        {
            let view = self.cx.app.plugins_page();
            let mut parts: Vec<Element<'a, Message>> = vec![form::heading(theme, "Plugins")];
            if let Some(line) = helper_line(theme, &view.helper) {
                parts.push(line);
            }
            if view.installed.is_empty() {
                parts.push(form::note(
                    theme,
                    "No plugins are installed. A plugin adds to Yapfast's interface \
                     and to what it does, and installing one is asked about first.",
                ));
            } else {
                let rows: Vec<Element<'a, Message>> = view
                    .installed
                    .iter()
                    .map(|plugin| installed_row(theme, plugin))
                    .collect();
                parts.push(form::card(theme, form::group(rows)));
            }
            form::page(parts)
        }
    }
}

/// What the helper process is doing.
#[cfg(feature = "plugins")]
fn helper_line<'a>(
    theme: &Theme,
    helper: &yapfast_core::plugins::page::Helper,
) -> Option<Element<'a, Message>> {
    use yapfast_core::plugins::page::Helper;
    let words = match helper {
        Helper::Running { memory } => match memory {
            Some(used) => format!(
                "The plugin helper is running, using {}.",
                yapfast_core::plugins::page::bytes(*used)
            ),
            None => "The plugin helper is running.".to_owned(),
        },
        Helper::Stopped => {
            "The plugin helper isn't running: it starts with the first plugin that runs.".to_owned()
        }
        Helper::Missing => {
            "The yapfast-plugins helper isn't beside Yapfast, so no plugin can run here.".to_owned()
        }
        Helper::Crashed => "The helper crashed three times in a minute, so plugins stay off until \
             Yapfast restarts."
            .to_owned(),
    };
    Some(form::note(theme, words))
}

/// One installed plugin: its name and version, what it is, what it's doing,
/// and the switch that runs it.
#[cfg(feature = "plugins")]
fn installed_row<'a>(
    theme: &Theme,
    plugin: &yapfast_core::plugins::page::Installed,
) -> Element<'a, Message> {
    let mut lines = column![
        row![
            text(plugin.name.clone())
                .size(15)
                .font(BOLD)
                .color(theme.text),
            text(format!(" {}", plugin.version))
                .size(12)
                .color(theme.muted),
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(4);

    if !plugin.description.is_empty() {
        lines = lines.push(text(plugin.description.clone()).size(13).color(theme.muted));
    }

    let status = plugin.status.text();
    if !status.is_empty() {
        let ink = tone_colour(theme, plugin.status.tone());
        lines = lines.push(text(status).size(12).color(ink));
    }
    if let Some(update) = &plugin.update {
        lines = lines.push(
            text(format!(
                "Version {} is available; it installs from the plugin's page.",
                update.version
            ))
            .size(12)
            .color(theme.accent),
        );
    }

    let switch = form::toggle(
        theme,
        plugin.enabled,
        Some(plugin_message(PluginEvent::SetEnabled {
            id: plugin.id.clone(),
            enabled: !plugin.enabled,
        })),
    );

    row![
        container(lines).width(Length::Fill).clip(true),
        Space::new().width(form::GAP),
        container(switch).align_y(Alignment::Center),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A `page::Tone` as the theme's colour.
#[cfg(feature = "plugins")]
fn tone_colour(theme: &Theme, tone: yapfast_core::plugins::page::Tone) -> iced::Color {
    use yapfast_core::plugins::page::Tone;
    match tone {
        Tone::Normal | Tone::Strong => theme.text,
        Tone::Muted => theme.muted,
        Tone::Success => theme.live,
        Tone::Warning => theme.loud,
        Tone::Danger => theme.danger,
    }
}
