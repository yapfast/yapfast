//! Voice messages: the one playing, or paused, and how fast and loud they
//! all play (`show_voice_message`, `src/ui/refresh.rs:5033`; `VoicePlayer`'s
//! callbacks, `src/ui/callbacks.rs:3529`-`:3573`).
//!
//! The player is the window's one: every card shows its speed and volume,
//! and the card of the message it holds its progress. Cards are built with
//! their message; this fills them in place after the timeline showed,
//! writing only what differs.

use iced::Task;
use yapfast_core::application::{Action, AppState};

use anomi_vm::Vm;
use anomi_vm::chat::timeline::{Timeline, group};
use anomi_vm::chat::voice_message::{Intent, VoiceMessage};

use super::time;
use crate::app::App;
use crate::message::Message;

/// What the player holds, as each card shows it.
struct Player {
    speed: String,
    volume: f32,
    muted: bool,
    /// The message it holds, by its row's key, and what that card shows.
    key: Option<u64>,
    playing: bool,
    progress: f32,
    time: String,
    error: String,
}

/// Shows the player in each voice message's card.
pub fn show(state: &AppState, vm: &mut Vm) {
    let tuning = &state.voice_tuning;
    let playback = state.voice_message.as_ref();
    let message = playback.and_then(|playback| state.message(&playback.item));
    let duration = message
        .and_then(|message| message.voice.as_ref())
        .map_or(0, |voice| voice.duration_ms);
    let player = Player {
        speed: format!("{}x", tuning.speed),
        volume: tuning.volume,
        muted: tuning.muted,
        key: message.map(|message| group::key(&message.id)),
        playing: playback.is_some_and(|playback| playback.playing),
        progress: match playback {
            Some(playback) if duration > 0 => {
                (playback.position_ms as f64 / duration as f64).min(1.0) as f32
            }
            _ => 0.0,
        },
        // Once started it counts down what's left, as Discord's does; at
        // its start the card shows how long it is.
        time: match playback {
            Some(playback) if playback.playing || playback.position_ms > 0 => {
                time::voice_time(duration.saturating_sub(playback.position_ms))
            }
            _ => String::new(),
        },
        error: (playback.and_then(|playback| playback.error.clone())).unwrap_or_default(),
    };
    fill(&mut vm.chat.timeline, &player);
    fill(&mut vm.chat.thread_panel.timeline, &player);
}

fn fill(timeline: &mut Timeline, player: &Player) {
    for row in &mut timeline.rows {
        let active = player.key == Some(row.key);
        let attachments = &mut row.message.attachments;
        let enabled = attachments.acts();
        for card in attachments.files.iter_mut() {
            if let Some(voice) = card.voice.as_mut() {
                set(voice, player, active, enabled);
            }
        }
    }
}

/// Writes into a card what differs.
fn set(voice: &mut VoiceMessage, player: &Player, active: bool, enabled: bool) {
    fn put<T: PartialEq>(field: &mut T, value: T) {
        if *field != value {
            *field = value;
        }
    }
    put(&mut voice.enabled, enabled);
    put(&mut voice.active, active);
    if voice.speed != player.speed {
        voice.speed.clone_from(&player.speed);
    }
    put(&mut voice.volume, player.volume);
    put(&mut voice.muted, player.muted);
    put(&mut voice.playing, active && player.playing);
    put(
        &mut voice.progress,
        if active { player.progress } else { 0.0 },
    );
    let (time, error) = if active {
        (player.time.as_str(), player.error.as_str())
    } else {
        ("", "")
    };
    if voice.time != time {
        voice.time = time.to_owned();
    }
    if voice.error != error {
        voice.error = error.to_owned();
    }
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let item =
        |app: &App, key: u64| (app.shown.timeline.message(key)).map(|message| message.item.clone());
    match intent {
        Intent::Toggle { item: key } => match item(app, key) {
            Some(item) => app.dispatch(Action::ToggleVoiceMessage(item)),
            None => Task::none(),
        },
        Intent::Seek { item: key, at } => match item(app, key) {
            Some(item) => app.dispatch(Action::MoveVoiceMessage(item, at.clamp(0.0, 1.0))),
            None => Task::none(),
        },
        Intent::CycleSpeed => app.dispatch(Action::CycleVoiceMessageSpeed),
        Intent::Mute(muted) => app.dispatch(Action::MuteVoiceMessages(muted)),
        Intent::Volume(volume) => app.dispatch(Action::VoiceMessageVolume(volume)),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
