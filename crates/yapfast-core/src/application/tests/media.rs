//! Videos, voice messages, the image viewer, and the pictures asked for.

use super::*;

#[test]
fn videos_play_in_the_open_room_until_it_closes() {
    let mut app = connected();
    let file = |name: &str, video: bool| chat::Attachment {
        name: name.into(),
        size: None,
        image: false,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video,
        url: None,
        description: None,
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![file("notes.txt", false), file("clip.mp4", true)],
            ..Default::default()
        }]),
    });
    let play = |index| Action::PlayVideo {
        item: "e:m".into(),
        index,
        width: 720,
        height: 404,
    };
    assert!(app.dispatch(play(0)).is_empty(), "a file isn't a video");
    assert!(matches!(
        app.dispatch(play(1)).as_slice(),
        [Effect::Backend(Command::PlayVideo { room, index: 1, width: 720, .. })] if room == "dm"
    ));
    let video = |room: &str| chat::Video {
        room: room.into(),
        item: "e:m".into(),
        index: 1,
        frames: None,
        duration_ms: Some(6000),
        size: Some((480, 270)),
        loading: false,
        playing: true,
        ended: false,
        sound: true,
        muted: false,
        volume: 1.0,
        error: None,
    };
    app.apply(Event::Video(Some(video("elsewhere"))));
    assert!(
        app.state.video.is_none(),
        "another room's video doesn't show"
    );
    app.apply(Event::Video(Some(video("dm"))));
    assert!(app.state.video.as_ref().is_some_and(|video| video.playing));
    assert!(matches!(
        app.dispatch(Action::Video(chat::VideoCommand::Pause))
            .as_slice(),
        [Effect::Backend(Command::Video(chat::VideoCommand::Pause))]
    ));
    app.state.aside = Some(aside::Aside {
        room: "thread".into(),
        ..Default::default()
    });
    app.apply(Event::Video(Some(video("thread"))));
    assert!(
        app.state
            .video
            .as_ref()
            .is_some_and(|video| video.room == "thread"),
        "the thread beside's video shows"
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.video.is_none(), "leaving the room stops it");
}

#[test]
fn a_voice_message_plays_and_pauses_one_at_a_time() {
    let mut app = connected();
    let recorded = |id: &str, voice: bool| {
        Arc::new(ChatMessage {
            id: id.into(),
            item: format!("d:{id}"),
            voice: voice.then(|| chat::VoiceMessage {
                duration_ms: 5000,
                waveform: [0, 128, 255].into(),
            }),
            ..Default::default()
        })
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: vec![
            recorded("1", true),
            recorded("2", true),
            recorded("3", false),
        ],
    });
    let toggle =
        |app: &mut Application, item: &str| app.dispatch(Action::ToggleVoiceMessage(item.into()));
    assert!(toggle(&mut app, "d:3").is_empty(), "not a voice message");
    assert!(matches!(
        toggle(&mut app, "d:1").as_slice(),
        [Effect::Backend(Command::PlayVoiceMessage { room, item, .. })] if room == "dm" && item == "d:1"
    ));
    let playing = app.state.voice_message.clone().unwrap();
    assert!(playing.playing && playing.loading);
    app.apply(Event::VoiceMessage(Some(chat::VoicePlayback {
        loading: false,
        position_ms: 1200,
        ..playing
    })));
    assert!(matches!(
        toggle(&mut app, "d:1").as_slice(),
        [Effect::Backend(Command::PauseVoiceMessage)]
    ));
    let paused = app.state.voice_message.clone().unwrap();
    assert!(!paused.playing);
    assert_eq!(paused.position_ms, 1200, "it pauses where it was");
    // Played again, it goes on from there; another starts from the start.
    toggle(&mut app, "d:1");
    let resumed = app.state.voice_message.clone().unwrap();
    assert!(resumed.playing && !resumed.loading);
    assert_eq!(resumed.position_ms, 1200);
    assert!(matches!(
        toggle(&mut app, "d:2").as_slice(),
        [Effect::Backend(Command::PlayVoiceMessage { item, .. })] if item == "d:2"
    ));
    let other = app.state.voice_message.clone().unwrap();
    assert_eq!((other.item.as_str(), other.position_ms), ("d:2", 0));
    // Its waveform moves it, playing as it was; another's waits there
    // paused in its place.
    let moved = |app: &mut Application, item: &str, fraction| {
        app.dispatch(Action::MoveVoiceMessage(item.into(), fraction))
    };
    assert!(matches!(
        moved(&mut app, "d:2", 0.5).as_slice(),
        [Effect::Backend(Command::MoveVoiceMessage { item, at_ms: 2500, .. })] if item == "d:2"
    ));
    let playback = app.state.voice_message.clone().unwrap();
    assert!(playback.playing && playback.position_ms == 2500);
    assert!(matches!(
        moved(&mut app, "d:1", 2.0).as_slice(),
        [Effect::Backend(Command::MoveVoiceMessage { item, at_ms: 5000, .. })] if item == "d:1"
    ));
    let playback = app.state.voice_message.clone().unwrap();
    assert_eq!(playback.item, "d:1");
    assert!(!playback.playing && playback.position_ms == 5000);
    assert!(
        moved(&mut app, "d:3", 0.5).is_empty(),
        "not a voice message"
    );
    app.state.voice_message = Some(other.clone());
    // Its speed and volume change as it plays, and last for the next.
    app.dispatch(Action::CycleVoiceMessageSpeed);
    assert!(matches!(
        app.dispatch(Action::VoiceMessageVolume(0.5)).as_slice(),
        [Effect::Backend(Command::TuneVoiceMessage(tuning))]
            if *tuning == chat::VoiceTuning { speed: 1.5, volume: 0.5, muted: false }
    ));
    // Muting keeps its volume to unmute to; the slider at 0 mutes it too.
    app.dispatch(Action::MuteVoiceMessages(true));
    assert!(app.state.voice_tuning.muted);
    app.dispatch(Action::VoiceMessageVolume(0.25));
    assert!(!app.state.voice_tuning.muted, "turning it up unmutes it");
    app.dispatch(Action::VoiceMessageVolume(0.0));
    assert_eq!(
        (app.state.voice_tuning.muted, app.state.voice_tuning.volume),
        (true, 0.25)
    );
    app.dispatch(Action::MuteVoiceMessages(false));
    app.dispatch(Action::CycleVoiceMessageSpeed);
    assert_eq!(app.state.voice_tuning.speed, 2.0);
    app.dispatch(Action::CycleVoiceMessageSpeed);
    assert_eq!(app.state.voice_tuning.speed, 1.0);
    app.apply(Event::VoiceMessage(Some(chat::VoicePlayback {
        room: "elsewhere".into(),
        ..other
    })));
    assert!(
        app.state.voice_message.is_none(),
        "another room's voice message doesn't show"
    );
}

#[test]
fn the_viewer_steps_through_a_messages_images() {
    let mut app = connected();
    let file = |name: &str, image: bool| chat::Attachment {
        name: name.into(),
        size: None,
        image,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: None,
        description: None,
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![
                file("a.png", true),
                file("notes.txt", false),
                file("b.png", true),
            ],
            ..Default::default()
        }]),
    });
    let viewed = |effects: Vec<Effect>| -> Vec<usize> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::ViewImage { index, .. }) => Some(index),
                _ => None,
            })
            .collect()
    };
    let view = |index| Action::ViewImage {
        item: "e:m".into(),
        index,
    };
    assert!(
        viewed(app.dispatch(view(1))).is_empty(),
        "a file isn't an image"
    );
    assert!(app.state.viewer.is_none());
    assert_eq!(viewed(app.dispatch(view(0))), [0]);
    assert_eq!(
        viewed(app.dispatch(Action::StepImage(1))),
        [2],
        "the file is skipped"
    );
    assert!(
        viewed(app.dispatch(Action::StepImage(1))).is_empty(),
        "back to the first, which is already loading"
    );
    assert_eq!(
        app.state.viewer.as_ref().map(|viewer| viewer.index),
        Some(0)
    );
    app.dispatch(Action::StepImage(-1));
    assert_eq!(
        app.state.viewer.as_ref().map(|viewer| viewer.index),
        Some(2),
        "and around backwards"
    );
    app.apply(Event::Image {
        room: "dm".into(),
        item: "e:m".into(),
        index: 2,
        picture: Err("gone"),
    });
    assert_eq!(
        app.state.viewer.as_ref().unwrap().pictures[&2],
        Some(Err("gone"))
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.viewer.is_none(), "another room closes it");
}

#[test]
fn a_picture_with_an_address_is_copied_to_the_clipboard() {
    let mut app = connected();
    let file = |url: Option<&str>| chat::Attachment {
        name: "cat.png".into(),
        size: None,
        image: true,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: url.map(Into::into),
        description: None,
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![file(None), file(Some("https://cdn.example/cat.png"))],
            ..Default::default()
        }]),
    });
    let copy = |index| Action::CopyAttachment {
        item: "e:m".into(),
        index,
    };
    assert!(
        app.dispatch(copy(0)).is_empty(),
        "a picture without an address can't be downloaded to copy"
    );
    assert!(matches!(
        app.dispatch(copy(1)).as_slice(),
        [Effect::Backend(Command::CopyAttachment { room, index: 1, .. })] if room == "dm"
    ));
    assert!(matches!(
        app.apply(Event::Copied(Ok(b"png".to_vec()))).as_slice(),
        [Effect::CopyPicture(picture)] if picture == b"png"
    ));
    app.apply(Event::Copied(Err(
        "Yapfast couldn't copy this picture. Save it instead.",
    )));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Yapfast couldn't copy this picture. Save it instead.")
    );
}

#[test]
#[cfg(feature = "discord")]
fn asks_once_for_the_pictures_on_screen() {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Snapshot, Status, VoiceMember},
    };
    let snapshot = |member: &str| Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            icon: Some("icon.png".into()),
            voice_channels: vec![Channel {
                id: "11".into(),
                name: "Lounge".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        voice: [(
            "11".to_owned(),
            vec![VoiceMember {
                user_id: member.into(),
                name: member.into(),
                avatar: format!("{member}.png"),
                muted: false,
                deafened: false,
                streaming: false,
                video: false,
                elsewhere: false,
                server_muted: false,
                server_deafened: false,
            }],
        )]
        .into(),
        ..Default::default()
    };
    let asked = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .flat_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
                _ => vec![],
            })
            .collect()
    };
    let mut app = connected();
    assert_eq!(
        asked(app.apply(discord_event(snapshot("ann")))),
        Vec::<String>::new(),
        "virtualized server icons are requested by the UI"
    );
    assert_eq!(
        asked(app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())))),
        ["ann.png"]
    );
    assert!(asked(app.apply(discord_event(snapshot("ann")))).is_empty());
    assert_eq!(
        asked(app.apply(discord_event(snapshot("bob")))),
        ["bob.png"]
    );
    app.apply(Event::Picture {
        url: "icon.png".into(),
        picture: Arc::new(Picture {
            width: 1,
            height: 1,
            rgba: crate::chat::Pixels::new(1, 1, vec![0; 4]),
            ..Default::default()
        }),
    });
    assert!(app.state.pictures.contains_key("icon.png"));
    assert_eq!(
        asked(app.dispatch(Action::LoadPictures(vec!["icon.png".into()]))),
        ["icon.png"]
    );
    assert!(asked(app.dispatch(Action::LoadPictures(vec!["icon.png".into()]))).is_empty());
}
