//! What a message shows large or plays: the picture viewer, and a voice
//! message's playing.

use std::{collections::HashMap, sync::Arc};

use iced::keyboard::{self, key};
use yapfast_core::{
    application::Viewer,
    chat::{Attachment, ChatMessage, Picture, Pixels, Placement},
};

use super::{change, preview, shows};

/// The open room holding one message with a 40 by 30 picture, viewed.
fn viewing(app: &mut crate::app::App) {
    let picture = Arc::new(Picture {
        width: 40,
        height: 30,
        rgba: Pixels::new(40, 30, vec![128; 40 * 30 * 4]),
        ..Default::default()
    });
    change(app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "i0".into(),
            author: "Sam".into(),
            attachments: vec![Attachment {
                name: "a.png".into(),
                size: None,
                image: true,
                dimensions: Some((40, 30)),
                preview: Some(picture),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            ..Default::default()
        })]);
        state.viewer = Some(Viewer {
            item: "i0".into(),
            index: 0,
            pictures: HashMap::new(),
        });
    });
}

#[test]
fn closing_the_viewer_lets_go_of_its_picture() {
    let (mut app, _folder) = preview();
    viewing(&mut app);
    assert!(app.viewer_size.is_some());
    assert!(shows(&app, "a.png"), "its name over it");
    let _ = app.update(crate::app::Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.client.state().viewer.is_none());
    assert!(app.viewer_size.is_none());
    assert!(!shows(&app, "a.png"), "and the viewer is gone");
}

#[test]
fn the_viewer_closes_when_its_message_goes_and_gives_the_keys_back() {
    let (mut app, _folder) = preview();
    viewing(&mut app);
    assert!(shows(&app, "a.png"));
    // A busy channel lets the message go.
    change(&mut app, |state| state.messages = Arc::new(Vec::new()));
    assert!(
        app.client.state().viewer.is_none(),
        "closed, its picture let go"
    );
    assert!(app.viewer_size.is_none());
}

#[test]
fn a_voice_messages_volume_opens_over_its_mute_and_the_wheel_turns_it() {
    use crate::playback::Event;
    let (mut app, _folder) = preview();
    let volume = |app: &crate::app::App| app.client.state().voice_tuning.volume;
    assert!((volume(&app) - 1.0).abs() < 0.001);
    let _ = app.voice_event(Event::Turn(-1.0));
    assert!((volume(&app) - 0.95).abs() < 0.001, "{}", volume(&app));
    let _ = app.voice_event(Event::Volume(0.3));
    assert!((volume(&app) - 0.3).abs() < 0.001);
    // The button and the slider over it overlap: it stays open until the
    // pointer has left both.
    let _ = app.voice_event(Event::VolumeHover("v".into(), true));
    let _ = app.voice_event(Event::VolumeHover("v".into(), true));
    let _ = app.voice_event(Event::VolumeHover("v".into(), false));
    assert!(app.voice_volume.is_some());
    let _ = app.voice_event(Event::VolumeHover("v".into(), false));
    assert!(app.voice_volume.is_none());
}
