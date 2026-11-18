//! Samples of a voice message's player for tests and shots, after the fake
//! Discord's data: task C11's. Bob's twelve seconds in #general.

use crate::Vm;
use crate::chat::attachments::{Attachments, File};
use crate::chat::message::Message;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Row, Timeline};
use crate::chat::voice_message::VoiceMessage;
use crate::fixtures::chat::{message::bobs, timeline};
use crate::fixtures::shell;
use crate::text::Source;

pub const ITEM: u64 = 9;

fn waveform() -> Vec<f32> {
    (0..40)
        .map(|i| {
            let i = i as f32;
            (0.15 + 0.85 * ((i * 0.7).sin() * (i * 0.23).cos()).abs()).min(1.0)
        })
        .collect()
}

/// Not played yet: its length, the speed at 1x.
pub fn idle() -> VoiceMessage {
    VoiceMessage {
        item: ITEM,
        duration: "0:12".into(),
        waveform: waveform(),
        speed: "1x".into(),
        volume: 0.8,
        enabled: true,
        ..VoiceMessage::default()
    }
}

/// Playing, a third of the way.
pub fn playing() -> VoiceMessage {
    VoiceMessage {
        active: true,
        playing: true,
        progress: 0.33,
        time: "0:08".into(),
        ..idle()
    }
}

/// Paused half way.
pub fn paused() -> VoiceMessage {
    VoiceMessage {
        active: true,
        progress: 0.5,
        time: "0:06".into(),
        ..idle()
    }
}

/// Muted, at 1.5x.
pub fn muted() -> VoiceMessage {
    VoiceMessage {
        muted: true,
        speed: "1.5x".into(),
        ..idle()
    }
}

/// It couldn't load.
pub fn failed() -> VoiceMessage {
    VoiceMessage {
        active: true,
        error: "Couldn't play".into(),
        ..idle()
    }
}

/// A preview of a message being written: its button is off.
pub fn preview() -> VoiceMessage {
    VoiceMessage {
        enabled: false,
        ..idle()
    }
}

/// #general with Bob's voice message last, its player as `voice` has it.
pub fn in_channel(voice: VoiceMessage) -> Vm {
    let item = key("voice");
    let message = Message {
        key: item,
        body: crate::text::markdown(Source::new("Listen to this")),
        attachments: Attachments {
            item,
            files: vec![File {
                index: 0,
                name: "voice-message.ogg".into(),
                voice: Some(VoiceMessage { item, ..voice }),
                ..File::default()
            }],
            ..Attachments::default()
        },
        time: "01:12".into(),
        short_time: "01:12".into(),
        ..bobs(30, true)
    };
    let mut general = timeline::general();
    general.rows.push(Row {
        key: message.key,
        first: true,
        message,
        ..Row::default()
    });
    let mut vm = shell::server_home();
    vm.chat.timeline = Timeline {
        shown: None,
        unread: None,
        ..general
    };
    vm
}
