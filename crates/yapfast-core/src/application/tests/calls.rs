//! Calls and voice channels: joining, watching, devices and sounds.

use super::*;

/// The sounds among effects, in order.
fn sounds(effects: &[Effect]) -> Vec<Sound> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Sound(sound) => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn the_voice_panels_channel_line_opens_the_call_unless_it_shows() {
    let mut app = connected();
    assert!(app.dispatch(Action::GoToCall).is_empty(), "no call");
    app.state.voice.room = Some("lounge".into());
    app.state.voice.phase = voice::Phase::Connected;
    // From a DM: to the channel's server, and the channel's call.
    app.dispatch(Action::GoToCall);
    assert_eq!(app.state.navigation, Navigation::Server("friends".into()));
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(!app.state.voice_chat);
    assert!(app.dispatch(Action::GoToCall).is_empty(), "already there");
    // A preview voice room has no chat; a Discord voice channel's has.
    let lounge = app.state.rooms.iter_mut().find(|room| room.id == "lounge");
    Arc::make_mut(lounge.unwrap()).network = Network::Discord;
    app.dispatch(Action::ShowVoiceChat(true));
    assert!(
        app.dispatch(Action::GoToCall).is_empty(),
        "its chat is beside the same call"
    );
    assert!(app.state.voice_chat);
    // From another channel of the server.
    app.dispatch(Action::SelectRoom("games".into()));
    app.dispatch(Action::GoToCall);
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    // A DM's call goes to the DM, from the server or the friends list.
    app.state.voice.room = Some("dm".into());
    app.dispatch(Action::GoToCall);
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(Action::GoToCall);
    assert_eq!(app.state.dm_screen, None);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
}

/// The streams effects watch, by user id.
fn watches(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend(Command::Voice(voice::Command::Watch(user))) => Some(user.as_str()),
            _ => None,
        })
        .collect()
}

fn in_call(id: &str, local: bool, streaming: bool) -> voice::Participant {
    voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming,
        video: None,
        avatar: None,
    }
}

fn call(phase: Phase, room: &str, participants: Vec<voice::Participant>) -> Event {
    Event::Voice(voice::State {
        phase,
        room: Some(room.into()),
        participants,
        ..Default::default()
    })
}

/// `connected()`, with Bob sharing his screen in the Lounge.
fn bob_live_in_lounge() -> Application {
    let mut app = connected();
    let lounge = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap();
    lounge.people = vec![chat::VoicePerson {
        id: "@bob:test".into(),
        name: "Bob".into(),
        streaming: true,
        ..Default::default()
    }];
    app
}

fn show_bob(app: &mut Application) -> Vec<Effect> {
    app.dispatch(Action::ShowVoicePerson {
        room: "lounge".into(),
        user: "@bob:test".into(),
    })
}

#[test]
fn someone_in_a_voice_channel_has_a_card_without_being_listed() {
    let mut app = bob_live_in_lounge();
    app.dispatch(Action::SelectRoom("lounge".into()));
    app.dispatch(Action::ShowProfile {
        user: "@bob:test".into(),
        large: false,
    });
    let view = app.state.profile.as_ref().expect("a card");
    assert_eq!(view.profile.name, "Bob");
}

#[test]
fn clicking_someone_sharing_from_outside_a_call_asks_then_watches_once_connected() {
    let mut app = bob_live_in_lounge();
    assert!(watches(&show_bob(&mut app)).is_empty());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert_eq!(app.state.switch_voice.as_deref(), Some("lounge"));
    assert!(app.state.pending_watch.is_some());
    // Cancelling keeps the channel shown and forgets the stream.
    app.dispatch(Action::CancelVoiceSwitch);
    assert!(app.state.pending_watch.is_none() && app.state.switch_voice.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));

    show_bob(&mut app);
    // Not in a call, joining leaves none.
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(room)))] if room == "lounge"
    ));
    let me = || in_call("@alice:test", true, false);
    assert!(watches(&app.apply(call(Phase::Joining, "lounge", vec![me()]))).is_empty());
    // Discord drops a watch before the call connects, and Bob may show
    // sharing only after.
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me()]))).is_empty());
    let bob = in_call("@bob:test", false, true);
    assert_eq!(
        watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob.clone()]))),
        ["@bob:test"]
    );
    assert!(app.state.pending_watch.is_none());
    // Watched once.
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob]))).is_empty());
}

#[test]
fn clicking_someone_sharing_from_another_call_switches_and_forgets_a_failed_join() {
    let mut app = bob_live_in_lounge();
    let me = || in_call("@alice:test", true, false);
    app.apply(call(Phase::Connected, "party-voice", vec![me()]));
    show_bob(&mut app);
    assert_eq!(app.state.switch_voice.as_deref(), Some("lounge"));
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Leave)),
            Effect::Backend(Command::Voice(voice::Command::Join(room))),
        ] if room == "lounge"
    ));
    // Leaving the other call keeps the stream to watch.
    app.apply(Event::Voice(voice::State::default()));
    app.apply(call(Phase::Joining, "lounge", vec![]));
    assert!(app.state.pending_watch.is_some());
    // Joining failed.
    app.apply(Event::Voice(voice::State {
        error: Some("Discord voice took too long to connect. Try again.".into()),
        ..Default::default()
    }));
    assert!(app.state.pending_watch.is_none());
    let bob = in_call("@bob:test", false, true);
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob]))).is_empty());
}

#[test]
fn clicking_someone_in_the_call_shows_it_and_watches_at_once() {
    let mut app = bob_live_in_lounge();
    let me = |streaming| in_call("@alice:test", true, streaming);
    let bob = in_call("@bob:test", false, true);
    app.apply(call(Phase::Connected, "lounge", vec![me(true), bob]));
    assert_eq!(watches(&show_bob(&mut app)), ["@bob:test"]);
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(app.state.switch_voice.is_none() && app.state.pending_watch.is_none());
    // Not one's own stream.
    app.dispatch(Action::Open("general".into()));
    let effects = app.dispatch(Action::ShowVoicePerson {
        room: "lounge".into(),
        user: "@alice:test".into(),
    });
    assert!(watches(&effects).is_empty() && app.state.pending_watch.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));

    // Until Bob shows sharing in the call, it waits; another room or
    // leaving forgets it.
    app.apply(call(Phase::Connected, "lounge", vec![me(false)]));
    assert!(watches(&show_bob(&mut app)).is_empty());
    assert!(app.state.pending_watch.is_some());
    app.dispatch(Action::Open("general".into()));
    assert!(app.state.pending_watch.is_none());
    show_bob(&mut app);
    app.dispatch(Action::Voice(voice::Command::Leave));
    assert!(app.state.pending_watch.is_none());
}

#[test]
fn clicking_someone_with_only_a_camera_shows_the_channel_without_joining() {
    let mut app = connected();
    let lounge = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap();
    lounge.people = vec![chat::VoicePerson {
        id: "@bob:test".into(),
        name: "Bob".into(),
        video: true,
        ..Default::default()
    }];
    let effects = show_bob(&mut app);
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Voice(_))))
    );
    assert!(app.state.switch_voice.is_none() && app.state.pending_watch.is_none());
    assert!(!app.state.voice.active());
}

#[test]
fn clicking_a_voice_channel_joins_it_where_you_are_and_asks_before_leaving_another() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    // Joining keeps the channel on screen, as Discord's client does: the
    // call shows in the voice dock.
    app.dispatch(Action::SelectRoom("lounge".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    // Clicking the channel you are in shows its call.
    app.dispatch(Action::SelectRoom("lounge".into()));
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(app.state.switch_voice.is_none());
    // Another server's channel asks first, and still doesn't show.
    app.dispatch(Action::Navigate(Navigation::Server("coop".into())));
    app.dispatch(Action::SelectRoom("looking-for-group".into()));
    app.dispatch(Action::SelectRoom("party-voice".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert_eq!(app.state.switch_voice.as_deref(), Some("party-voice"));
    app.dispatch(Action::CancelVoiceSwitch);
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    app.dispatch(Action::SelectRoom("party-voice".into()));
    app.dispatch(Action::ConfirmVoiceSwitch);
    assert_eq!(app.state.voice.room.as_deref(), Some("party-voice"));
    assert_eq!(app.state.selected.as_deref(), Some("looking-for-group"));
    // The voice dock's channel line goes to the call.
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::Open("party-voice".into()));
    assert_eq!(app.state.selected.as_deref(), Some("party-voice"));
    assert!(!app.state.voice_chat);
    // Coming back to a server restores its voice channel without joining it.
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::Voice(voice::Command::Leave));
    app.dispatch(Action::Navigate(Navigation::Server("coop".into())));
    assert_eq!(app.state.selected.as_deref(), Some("party-voice"));
    assert!(!app.state.voice.active() && app.state.switch_voice.is_none());
    app.dispatch(Action::SelectRoom("party-voice".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("party-voice"));
}

#[test]
fn a_voice_channel_whose_chat_mentions_you_opens_its_chat_as_it_joins() {
    let mut app = connected();
    let id = "discord:10:11";
    let mut room = preview_room(id, "Lounge", false, 3);
    room.voice = true;
    room.mentions = 1;
    app.state.discord_online = true;
    app.state.rooms.push(Arc::new(room));
    Arc::make_mut(&mut app.state.servers[0])
        .channels
        .push(id.into());
    assert!(
        app.state
            .rooms
            .iter()
            .any(|room| room.id == id && room.voice_chat())
    );
    let server = app.state.servers[0].id.clone();
    app.dispatch(Action::Navigate(Navigation::Server(server)));

    let effects = app.dispatch(Action::SelectRoom(id.into()));
    assert_eq!(app.state.selected.as_deref(), Some(id));
    assert!(app.state.voice_chat);
    assert!(effects.iter().any(|effect| matches!(
        effect, Effect::Backend(Command::Voice(voice::Command::Join(room))) if room == id
    )));
}

#[test]
fn a_discord_voice_channels_chat_opens_without_joining_it() {
    let mut app = connected();
    let id = "discord:10:11";
    let mut room = preview_room(id, "Lounge", false, 3);
    room.voice = true;
    room.join_locked = true;
    app.state.discord_online = true;
    app.state.rooms.push(Arc::new(room));
    Arc::make_mut(&mut app.state.servers[0])
        .channels
        .push(id.into());
    let opens = |effects: &[Effect]| {
        effects.iter().any(|effect| {
            matches!(
                effect, Effect::Backend(Command::Open(room)) if room == id
            )
        })
    };

    // Going to it shows its call, and loads no messages.
    let effects = app.dispatch(Action::Open(id.into()));
    assert!(!opens(&effects));
    assert!(!app.state.shows_text() && !app.state.can_write());
    assert!(app.dispatch(Action::Earlier).is_empty());
    assert!(app.state.menu_choices(id).contains(&MenuChoice::OpenChat));

    // Its chat opens without joining, which its roles don't allow.
    let effects = app.dispatch(Action::OpenChat(id.into()));
    assert!(opens(&effects));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Voice(_))))
    );
    assert!(app.state.voice_chat && !app.state.can_join_voice(id));
    app.apply(Event::Timeline {
        room: id.into(),
        messages: timeline(vec![]),
    });
    assert!(app.state.can_write());
    app.dispatch(Action::SetDraft("Hello from voice chat".into()));
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { room, body, .. } if room == id && body == "Hello from voice chat"
    ));
    assert!(app.dispatch(Action::Earlier).iter().any(|effect| matches!(
        effect, Effect::Backend(Command::Earlier(room)) if room == id
    )));

    // The header switches back to its call and to its chat again.
    let effects = app.dispatch(Action::ShowVoiceChat(false));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::CloseRoom)))
    );
    assert!(!app.state.voice_chat && !app.state.can_write());
    assert!(opens(&app.dispatch(Action::ShowVoiceChat(true))));
    assert!(
        app.state.voice_chat && app.state.room_ready,
        "its messages stay"
    );

    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == id)
        .unwrap()
        .read_only = true;
    assert!(!app.state.can_write());
}

#[test]
fn the_preview_lists_and_chooses_real_devices_across_calls() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    let effects = app.dispatch(Action::Voice(voice::Command::Devices));
    assert!(matches!(effects.as_slice(), [Effect::ListDevices]));
    let device = |id: &str| voice::Device {
        id: id.into(),
        name: id.into(),
    };
    app.preview_devices(Ok(voice::Devices {
        microphones: vec![device("mic-a"), device("mic-b")],
        speakers: vec![device("out")],
        cameras: vec![device("camera-a"), device("camera-b")],
    }));
    assert!(
        app.dispatch(Action::Voice(voice::Command::Microphone("mic-b".into())))
            .is_empty()
    );
    app.dispatch(Action::Voice(voice::Command::Camera("camera-b".into())));
    app.dispatch(Action::RequestJoinVoice("lounge".into()));
    app.dispatch(Action::Voice(voice::Command::Leave));
    assert_eq!(app.state.voice.microphones.len(), 2);
    assert_eq!(app.state.voice.microphone.as_deref(), Some("mic-b"));
    assert_eq!(app.state.voice.cameras.len(), 2);
    assert_eq!(app.state.voice.camera.as_deref(), Some("camera-b"));
    app.preview_devices(Err("No sound server.".into()));
    assert_eq!(app.state.voice.error.as_deref(), Some("No sound server."));
    // A signed-in session lists devices in its voice worker instead.
    app.dispatch(Action::Logout);
    app.preview_devices(Ok(voice::Devices::default()));
    assert!(app.state.voice.error.is_none());
}

#[test]
fn preview_and_voice_switching_need_no_backend_and_voice_survives_navigation() {
    let mut app = Application::default();
    assert!(app.dispatch(Action::Preview).is_empty());
    assert_eq!(app.state.messages.len(), 5);
    app.dispatch(Action::SetDraft("Local message".into()));
    assert!(app.dispatch(Action::Send).is_empty());
    assert_eq!(app.state.messages.len(), 6);
    assert_eq!(app.state.messages.last().unwrap().body, "Local message");
    assert!(app.state.draft().is_empty());
    // The preview's calls sound as real ones do.
    assert_eq!(
        sounds(&app.dispatch(Action::RequestJoinVoice("lounge".into()))),
        [Sound::Join]
    );
    assert_eq!(app.state.voice.phase, Phase::Connected);
    assert!(app.state.voice.muted);
    assert!(
        app.dispatch(Action::Navigate(Navigation::DirectMessages))
            .is_empty()
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .is_empty()
    );
    assert_eq!(app.state.switch_voice.as_deref(), Some("party-voice"));
    app.dispatch(Action::CancelVoiceSwitch);
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    app.dispatch(Action::RequestJoinVoice("party-voice".into()));
    assert_eq!(
        sounds(&app.dispatch(Action::ConfirmVoiceSwitch)),
        [Sound::Leave, Sound::Join]
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("party-voice"));
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Deafen(true)))),
        [Sound::Deafen]
    );
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Deafen(false)))),
        [Sound::Undeafen]
    );
    // Muted before the deafening, and so after it.
    assert!(app.state.voice.muted);
    // Unmuting while deafened undeafens too.
    app.dispatch(Action::Voice(voice::Command::Deafen(true)));
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Mute(false)))),
        [Sound::Undeafen]
    );
    assert!(!app.state.voice.muted && !app.state.voice.deafened);
    // Deafening mutes one who talks, and undeafening gives the microphone
    // back, as Discord's client does.
    app.dispatch(Action::Voice(voice::Command::Deafen(true)));
    assert!(app.state.voice.muted && app.state.voice.deafened);
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Deafen(false)))),
        [Sound::Undeafen]
    );
    assert!(!app.state.voice.muted && !app.state.voice.deafened);
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Mute(true)))),
        [Sound::Mute]
    );
    // The preference that unmutes on joining does so without a sound.
    assert!(app.dispatch(Action::UnmuteOnJoin).is_empty());
    assert!(!app.state.voice.muted);
    // And the one that deafens, which mutes as well, until undeafened.
    assert!(
        app.dispatch(Action::DeafenOnJoin { unmuted: false })
            .is_empty()
    );
    assert!(app.state.voice.muted && app.state.voice.deafened);
    app.dispatch(Action::Voice(voice::Command::Deafen(false)));
    assert!(!app.state.voice.muted, "unmuted before the deafening");
    // Joined muted and deafened, undeafening leaves the mute; with the
    // preference to join unmuted, it turns the microphone on.
    for unmuted in [false, true] {
        app.dispatch(Action::Voice(voice::Command::Mute(true)));
        app.dispatch(Action::DeafenOnJoin { unmuted });
        assert!(app.state.voice.muted && app.state.voice.deafened);
        app.dispatch(Action::Voice(voice::Command::Deafen(false)));
        assert_eq!(app.state.voice.muted, !unmuted);
    }
    // Muting while deafened is the user's own mute, which stays.
    app.dispatch(Action::Voice(voice::Command::Deafen(true)));
    app.dispatch(Action::Voice(voice::Command::Mute(true)));
    app.dispatch(Action::Voice(voice::Command::Deafen(false)));
    assert!(app.state.voice.muted && !app.state.voice.deafened);
    assert!(app.dispatch(Action::Logout).is_empty());
    assert!(!app.state.preview);
    assert!(!app.state.voice.active());
}

#[test]
fn streams_sound_as_others_start_and_stop_watching() {
    let mut app = connected();
    let person = |id: &str, local: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    };
    let frames = std::sync::Arc::new(crate::frames::Frames::create(2, 2).unwrap());
    let call = |sharing: Option<&[&str]>, watching: Option<&[&str]>| voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        participants: vec![
            person("me", true),
            person("sam", false),
            person("kim", false),
        ],
        sharing: sharing.map(|viewers| voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: frames.clone(),
            viewers: viewers.iter().map(|&viewer| viewer.into()).collect(),
        }),
        watching: watching.map(|viewers| {
            Box::new(voice::Watching {
                user: "sam".into(),
                name: "sam".into(),
                opening: false,
                frames: None,
                viewers: viewers.iter().map(|&viewer| viewer.into()).collect(),
            })
        }),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    assert_eq!(heard(call(None, None)), [Sound::Join]);
    // Going live, with no one watching yet, and someone starting.
    assert_eq!(heard(call(Some(&[]), None)), []);
    assert_eq!(heard(call(Some(&["sam"]), None)), [Sound::ViewerJoin]);
    assert_eq!(heard(call(Some(&["sam"]), None)), []);
    assert_eq!(heard(call(Some(&[]), None)), [Sound::ViewerLeave]);
    // Watching Sam's stream: this account among its viewers makes no sound,
    // and nor do those already watching; others coming and going do.
    assert_eq!(heard(call(Some(&[]), Some(&["me", "kim"]))), []);
    assert_eq!(
        heard(call(Some(&["kim"]), Some(&["me", "kim"]))),
        [Sound::ViewerJoin]
    );
    assert_eq!(
        heard(call(Some(&["kim"]), Some(&["me"]))),
        [Sound::ViewerLeave]
    );
    // A stream ending makes no viewer sound.
    assert_eq!(heard(call(None, Some(&["me"]))), []);
}

#[test]
fn others_going_live_and_ending_sounds_but_not_as_anyone_comes_or_goes_streaming() {
    let mut app = connected();
    let person = |id: &str, local: bool, streaming: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming,
        video: None,
        avatar: None,
    };
    let call = |me: bool, people: &[(&str, bool)]| voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        participants: std::iter::once(person("me", true, me))
            .chain(
                people
                    .iter()
                    .map(|&(id, streaming)| person(id, false, streaming)),
            )
            .collect(),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    // Sam streams already as this account joins, and Kim as they arrive.
    assert_eq!(heard(call(false, &[("sam", true)])), [Sound::Join]);
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", true)])),
        [Sound::Join]
    );
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", false)])),
        [Sound::StreamStop]
    );
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", true)])),
        [Sound::StreamStart]
    );
    assert_eq!(heard(call(false, &[("sam", true), ("kim", true)])), []);
    assert_eq!(
        heard(call(false, &[("sam", false), ("kim", true)])),
        [Sound::StreamStop]
    );
    // Kim leaving mid-stream makes only the sound of leaving.
    assert_eq!(heard(call(false, &[("sam", false)])), [Sound::Leave]);
    // This account going live and ending makes none.
    assert_eq!(heard(call(true, &[("sam", false)])), []);
    assert_eq!(heard(call(false, &[("sam", false)])), []);
}

#[test]
fn calls_sound_as_people_come_and_go_but_not_while_reconnecting_or_ringing() {
    let mut app = connected();
    let person = |id: &str, local: bool, connected: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    };
    let call = |phase: Phase, people: &[(&str, bool)]| voice::State {
        phase,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        muted: false,
        participants: std::iter::once(person("me", true, true))
            .chain(
                people
                    .iter()
                    .map(|&(id, connected)| person(id, false, connected)),
            )
            .collect(),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    assert_eq!(heard(call(Phase::Joining, &[])), []);
    // Those already there when this account arrives make no sound of their own.
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true)])),
        [Sound::Join]
    );
    // Someone a DM call is ringing isn't there until they answer.
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true), ("kim", false)])),
        []
    );
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true), ("kim", true)])),
        [Sound::Join]
    );
    // Two going at once make one sound.
    assert_eq!(heard(call(Phase::Connected, &[])), [Sound::Leave]);
    assert_eq!(heard(call(Phase::Reconnecting, &[])), []);
    assert_eq!(heard(call(Phase::Connected, &[("sam", true)])), []);
    assert_eq!(heard(call(Phase::Leaving, &[])), [Sound::Leave]);
    assert_eq!(heard(voice::State::default()), []);

    // Muting sounds as it's asked for, rather than once the call confirms it.
    app.apply(Event::Voice(call(Phase::Connected, &[])));
    let effects = app.dispatch(Action::Voice(voice::Command::Mute(true)));
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Mute(true))),
            Effect::Sound(Sound::Mute)
        ]
    ));
    assert!(matches!(
        app.dispatch(Action::UnmuteOnJoin).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Mute(false)))]
    ));
    assert!(matches!(
        app.dispatch(Action::DeafenOnJoin { unmuted: false })
            .as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Deafen(
            true
        )))]
    ));
    assert!(matches!(
        app.dispatch(Action::DeafenOnJoin { unmuted: true })
            .as_slice(),
        [Effect::Backend(Command::Voice(
            voice::Command::DeafenUnmuted
        ))]
    ));
}

#[test]
fn a_call_moved_to_another_channel_takes_its_view_along() {
    let mut app = connected();
    let call = |room: &str| {
        Event::Voice(voice::State {
            phase: Phase::Connected,
            room: Some(room.into()),
            ..Default::default()
        })
    };
    app.dispatch(Action::Open("lounge".into()));
    app.apply(call("lounge"));
    app.apply(call("party-voice"));
    assert_eq!(app.state.selected.as_deref(), Some("party-voice"));
    // Viewing another room, the view stays.
    app.dispatch(Action::Open("games".into()));
    app.apply(call("lounge"));
    assert_eq!(app.state.selected.as_deref(), Some("games"));
}

#[test]
fn native_voice_switch_is_explicit_and_rechecks_trust_before_leaving() {
    let mut app = connected();
    app.apply(Event::Voice(voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .is_empty()
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    app.apply(Event::Trust {
        trusted: false,
        error: None,
    });
    assert!(app.dispatch(Action::ConfirmVoiceSwitch).is_empty());
    assert!(app.state.switch_voice.is_none());
    app.apply(Event::Trust {
        trusted: true,
        error: None,
    });
    app.dispatch(Action::RequestJoinVoice("party-voice".into()));
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Leave)),
            Effect::Backend(Command::Voice(voice::Command::Join(id)))
        ] if id == "party-voice"
    ));
}

#[test]
#[cfg(feature = "discord")]
fn discord_dm_calls_start_ring_and_are_answered() {
    use yapfast_discord::service::{Conversation, Snapshot, Status, VoiceMember};
    let person = |id: &str, name: &str| VoiceMember {
        user_id: id.into(),
        name: name.into(),
        avatar: format!("{id}.png"),
        muted: false,
        deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        server_muted: false,
        server_deafened: false,
    };
    let snapshot = |ringing: Vec<VoiceMember>, in_call: Vec<VoiceMember>| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        conversations: vec![
            Arc::new(Conversation {
                id: "30".into(),
                name: "Ann".into(),
                people: 2,
                person: Some("2".into()),
                ringing,
                ..Default::default()
            }),
            Arc::new(Conversation {
                id: "40".into(),
                name: "Erin".into(),
                people: 2,
                request: true,
                ..Default::default()
            }),
        ],
        voice: [("30".to_owned(), in_call)].into(),
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(vec![], vec![])));
    let dm = "discord:@me:30".to_owned();
    assert!(app.state.can_join_voice(&dm), "a DM has a call");
    assert!(
        !app.state.can_join_voice("discord:@me:40"),
        "a request waits for an answer"
    );

    // Ann calls: the DM rings once, with a notification, until answered.
    let effects = app.apply(discord_event(snapshot(
        vec![person("1", "Me")],
        vec![person("2", "Ann")],
    )));
    let alerts: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notify(alert) => Some((alert.call, alert.room.as_str(), alert.title.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(alerts, [(true, dm.as_str(), "Ann")]);
    let room = app.state.rooms.iter().find(|room| room.id == dm).unwrap();
    assert!(room.incoming && room.ringing.is_empty());
    assert_eq!(room.people[0].name, "Ann");
    assert!(
        !app.apply(discord_event(snapshot(
            vec![person("1", "Me")],
            vec![person("2", "Ann")],
        )))
        .iter()
        .any(|effect| matches!(effect, Effect::Notify(_))),
        "a call notifies once"
    );
    assert!(matches!(
        app.dispatch(Action::DeclineCall(dm.clone())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::DeclineCall { room }))] if *room == dm
    ));
    let erin = "discord:@me:40";
    app.dispatch(Action::Open(erin.into()));
    assert_eq!(app.state.selected.as_deref(), Some(erin));
    let effects = app.dispatch(Action::AcceptCall(dm.clone()));
    assert_eq!(
        app.state.selected.as_deref(),
        Some(erin),
        "answering leaves what shows, as Discord's client does"
    );
    assert!(matches!(
        effects.last(),
        Some(Effect::Backend(Command::Voice(voice::Command::Join(id)))) if *id == dm
    ));
}

#[test]
#[cfg(feature = "discord")]
fn do_not_disturb_shows_a_call_without_notifying() {
    use yapfast_discord::service::{Conversation, DO_NOT_DISTURB, Snapshot, Status, VoiceMember};
    let ann = VoiceMember {
        user_id: "2".into(),
        name: "Ann".into(),
        avatar: "2.png".into(),
        muted: false,
        deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        server_muted: false,
        server_deafened: false,
    };
    let mut app = Application::default();
    let effects = app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        presence: DO_NOT_DISTURB.into(),
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            people: 2,
            person: Some("2".into()),
            ringing: vec![VoiceMember {
                user_id: "1".into(),
                name: "Me".into(),
                ..ann.clone()
            }],
            ..Default::default()
        })],
        voice: [("30".to_owned(), vec![ann])].into(),
        ..Default::default()
    }));
    assert!(app.state.do_not_disturb());
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Notify(_))),
        "Do Not Disturb doesn't notify"
    );
    let room = app
        .state
        .rooms
        .iter()
        .find(|room| room.id == "discord:@me:30");
    assert!(
        room.is_some_and(|room| room.incoming),
        "the call still shows"
    );
}

#[test]
fn a_video_call_joins_then_turns_the_camera_on_once_connected_and_forgets_a_failed_join() {
    let films = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Backend(Command::Voice(voice::Command::Video(true)))
                )
            })
            .count()
    };
    let mut app = bob_live_in_lounge();
    let me = || in_call("@alice:test", true, false);
    assert!(matches!(
        app.dispatch(Action::StartVideoCall("lounge".into())).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(room)))] if room == "lounge"
    ));
    assert_eq!(app.state.pending_camera.as_deref(), Some("lounge"));
    assert_eq!(
        films(&app.apply(call(Phase::Joining, "lounge", vec![me()]))),
        0
    );
    assert_eq!(
        films(&app.apply(call(Phase::Connected, "lounge", vec![me()]))),
        1
    );
    assert!(app.state.pending_camera.is_none());
    // Filmed once.
    assert_eq!(
        films(&app.apply(call(Phase::Connected, "lounge", vec![me()]))),
        0
    );

    // A join that fails forgets the camera.
    let mut app = bob_live_in_lounge();
    app.dispatch(Action::StartVideoCall("lounge".into()));
    app.apply(call(Phase::Joining, "lounge", vec![]));
    app.apply(Event::Voice(voice::State {
        error: Some("Discord voice took too long to connect. Try again.".into()),
        ..Default::default()
    }));
    assert!(app.state.pending_camera.is_none());
}
