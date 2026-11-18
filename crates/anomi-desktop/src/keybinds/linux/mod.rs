//! Keybinds in every app on Linux: the GlobalShortcuts portal on Wayland
//! ([`portal`]), XInput 2's raw key events on X11 ([`x11`]).

mod portal;
#[cfg(test)]
mod tests;
mod x11;

use std::collections::HashMap;

use yapfast_core::runtime::{BackgroundRuntime, BackgroundTask};

use super::{Matcher, Reach, Report, Sink, Status, Watched};
use portal::portal;
pub use x11::X11;

/// Global keybinds on Wayland or X11; stop when dropped.
pub enum Global {
    Portal { _session: BackgroundTask<()> },
    X11 { _events: X11 },
}

/// A shortcut to bind through the portal: its id, what settings call
/// it, and the keys Yapfast would like for it.
pub(super) struct Wanted {
    pub id: String,
    pub description: &'static str,
    pub trigger: Option<String>,
}

impl Global {
    /// Wayland's desktops grab keys through the portal; X11 lets any
    /// client hear every key.
    pub fn start(
        watched: &[Watched],
        runtime: &BackgroundRuntime,
        report: Report,
        sink: Sink,
    ) -> Option<Self> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
        let matcher = Matcher::new(watched);
        if wayland {
            report(Status::default());
            let wanted = watched
                .iter()
                .map(|watched| Wanted {
                    id: watched.id.clone(),
                    description: watched.action.label(),
                    trigger: watched.keys.trigger(),
                })
                .collect();
            return Some(Global::Portal {
                _session: runtime.spawn(async move {
                    let mut matcher = matcher;
                    let error = portal(wanted, &mut matcher, &sink, &report).await;
                    matcher.release_all(&mut |action, down| sink.act(action, down));
                    if let Err(error) = error {
                        tracing::info!(target: crate::LOG,
                            error = format!("{error:#}"),
                            "no global keybinds through the portal"
                        );
                        report(Status {
                            reach: Reach::Focused(
                                "Your desktop doesn't offer Yapfast global shortcuts",
                            ),
                            given: HashMap::new(),
                        });
                    }
                }),
            });
        }
        match X11::start(matcher, sink, report.clone()) {
            Ok(x11) => {
                report(Status {
                    reach: Reach::Everywhere,
                    given: HashMap::new(),
                });
                Some(Global::X11 { _events: x11 })
            }
            Err(error) => {
                tracing::info!(target: crate::LOG, error = format!("{error:#}"), "no global keybinds on X11");
                report(Status {
                    reach: Reach::Focused("Yapfast couldn't watch keys in other apps"),
                    given: HashMap::new(),
                });
                None
            }
        }
    }
}
