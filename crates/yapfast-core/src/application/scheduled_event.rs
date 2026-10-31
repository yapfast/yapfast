//! A Discord server's scheduled event that a clicked event link opens.
//! The event is looked up and shown first; marking this account
//! interested, or no longer interested, waits for the click, as joining
//! a server by an invite does.

use super::*;
use crate::chat::EventPreview;

/// The scheduled event dialog, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventDialog {
    /// The event looked up or shown, by its id.
    pub event: Option<String>,
    /// What the event shows, once Discord answered.
    pub preview: Option<EventPreview>,
    pub resolving: bool,
    /// Changing whether this account is interested.
    pub marking: bool,
    pub error: Option<String>,
}

impl Application {
    /// Opens the dialog for a clicked scheduled-event link, by its
    /// server and event, and looks the event up.
    pub(super) fn open_event_link(&mut self, guild: String, event: String) -> Vec<Effect> {
        if !self.state.discord_online && !self.state.preview {
            return vec![];
        }
        self.state.scheduled_event = Some(EventDialog {
            event: Some(event.clone()),
            resolving: true,
            ..EventDialog::default()
        });
        if self.state.preview {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ResolveEvent { guild, event },
        ))]
    }

    pub(super) fn event_resolved(
        &mut self,
        event: String,
        result: Result<EventPreview, String>,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .scheduled_event
            .as_mut()
            .filter(|dialog| dialog.resolving && dialog.event.as_ref() == Some(&event))
        else {
            return vec![];
        };
        dialog.resolving = false;
        match result {
            Ok(preview) => dialog.preview = Some(preview),
            Err(error) => dialog.error = Some(error),
        }
        vec![]
    }

    /// Marks this account interested in the event the dialog shows, or
    /// no longer interested in it, while the event is still to come.
    pub(super) fn toggle_event_interest(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .scheduled_event
            .as_mut()
            .filter(|dialog| !dialog.marking)
        else {
            return vec![];
        };
        let Some(preview) = dialog.preview.clone() else {
            return vec![];
        };
        if preview.over || self.state.preview {
            return vec![];
        }
        dialog.marking = true;
        dialog.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SetEventInterest {
                interested: !preview.interested,
                preview: Box::new(preview),
            },
        ))]
    }

    pub(super) fn event_interest_set(
        &mut self,
        event: String,
        interested: bool,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .scheduled_event
            .as_mut()
            .filter(|dialog| dialog.marking && dialog.event.as_ref() == Some(&event))
        else {
            return vec![];
        };
        dialog.marking = false;
        match result {
            Ok(()) => {
                if let Some(preview) = dialog.preview.as_mut() {
                    preview.interested = interested;
                    if let Some(count) = preview.interested_count.as_mut() {
                        *count = if interested {
                            count.saturating_add(1)
                        } else {
                            count.saturating_sub(1)
                        };
                    }
                }
            }
            Err(error) => dialog.error = Some(error),
        }
        vec![]
    }
}
