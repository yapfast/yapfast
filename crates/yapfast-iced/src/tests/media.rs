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
    assert!(app.viewer.size.is_some());
    assert!(shows(&app, "a.png"), "its name over it");
    let _ = app.update(crate::app::Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.client.state().viewer.is_none());
    assert!(app.viewer.size.is_none());
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
    assert!(app.viewer.size.is_none());
}

#[test]
fn a_picture_opened_after_the_layout_shrinks_fits_the_new_room() {
    use crate::{app::Message, image_viewer::Event};
    let (mut app, _folder) = preview();
    let opened = app.shell.frame;
    redraw_at(&mut app, opened);
    only(
        &mut app,
        ChatMessage {
            attachments: vec![picture_at("large.png", 500, Placement::Message, false)],
            ..Default::default()
        },
    );
    let smaller = iced::Size::new(660.0, 410.0);
    redraw_at(&mut app, smaller);
    let _ = app.update(Message::Image(Event::Open("i0".into(), 0)));
    let on_picture = |x| {
        clicked(&app, iced::Point::new(x, 220.0), iced::mouse::Button::Right)
            .iter()
            .any(|message| matches!(message, Message::Image(Event::Menu(..))))
    };
    assert!(on_picture(330.0));
    assert!(!on_picture(530.0));
}

#[test]
fn a_voice_messages_volume_opens_over_its_mute_and_the_wheel_turns_it() {
    use crate::playback::Event;
    let (mut app, _folder) = preview();
    let volume = |app: &crate::app::App| app.client.state().voice_tuning.volume;
    assert!((volume(&app) - 1.0).abs() < 0.001);
    let _ = crate::playback::voice_event(&mut app, Event::Turn(-1.0));
    assert!((volume(&app) - 0.95).abs() < 0.001, "{}", volume(&app));
    let _ = crate::playback::voice_event(&mut app, Event::Volume(0.3));
    assert!((volume(&app) - 0.3).abs() < 0.001);
    // The button and the slider over it overlap: it stays open until the
    // pointer has left both.
    let _ = crate::playback::voice_event(&mut app, Event::VolumeHover("v".into(), true));
    let _ = crate::playback::voice_event(&mut app, Event::VolumeHover("v".into(), true));
    let _ = crate::playback::voice_event(&mut app, Event::VolumeHover("v".into(), false));
    assert!(app.playback.voice_volume.is_some());
    let _ = crate::playback::voice_event(&mut app, Event::VolumeHover("v".into(), false));
    assert!(app.playback.voice_volume.is_none());
}

/// A picture placed `placement`, `side` px square, at example.com.
fn picture_at(name: &str, side: u32, placement: Placement, spoiler: bool) -> Attachment {
    Attachment {
        name: name.into(),
        size: None,
        image: true,
        dimensions: Some((side, side)),
        preview: None,
        preview_failed: false,
        placement,
        gif: None,
        spoiler,
        video: false,
        url: Some(format!("https://example.com/{name}").into()),
        description: None,
    }
}

/// The open room holding `message` alone, as item `i0` by Sam.
fn only(app: &mut crate::app::App, message: ChatMessage) {
    change(app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "i0".into(),
            author: "Sam".into(),
            ..message
        })]);
    });
}

/// What a click with `button` at `at` sends, not yet applied.
fn clicked(
    app: &crate::app::App,
    at: iced::Point,
    button: iced::mouse::Button,
) -> Vec<crate::app::Message> {
    use iced::{Event, mouse};
    let mut ui =
        iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(button)),
        Event::Mouse(mouse::Event::ButtonReleased(button)),
    ]);
    ui.into_messages().collect()
}

/// The bounds of the text in view reading `text`.
fn bounds_of(app: &crate::app::App, text: &str) -> iced::Rectangle {
    let mut ui =
        iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let found = ui
        .find(text)
        .unwrap_or_else(|_| panic!("{text:?} isn't there"));
    found.visible_bounds().expect("in view")
}

fn apply(app: &mut crate::app::App, messages: Vec<crate::app::Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

/// A redraw in the given layout, with no window resize event.
fn redraw_at(app: &mut crate::app::App, size: iced::Size) {
    let messages = {
        let mut ui = iced_test::Simulator::with_size(iced::Settings::default(), size, app.view());
        ui.simulate([iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);
        ui.into_messages().collect()
    };
    apply(app, messages);
}

/// A spoiler beside a card's text: a click uncovers it, the next shows
/// it large, and a right click opens the message's menu with its rows.
fn a_thumbnail_uncovers_opens_and_has_its_menu(message: ChatMessage) {
    use crate::{app::Message, image_viewer::Event};
    let (mut app, _folder) = preview();
    only(&mut app, message);
    let at = bounds_of(&app, "SPOILER").center();
    let uncovered = clicked(&app, at, iced::mouse::Button::Left);
    apply(&mut app, uncovered);
    assert!(!shows(&app, "SPOILER"), "uncovered");
    let opened = clicked(&app, at, iced::mouse::Button::Left);
    assert!(
        opened
            .iter()
            .any(|message| matches!(message, Message::Image(Event::Open(item, 0)) if item == "i0")),
        "shown large"
    );
    let menu = clicked(&app, at, iced::mouse::Button::Right);
    apply(&mut app, menu);
    let menu = app.win.menu.as_ref().expect("the message's menu");
    assert!(menu.rows.iter().any(|row| row.label == "Copy Link"));
    assert!(menu.rows.iter().any(|row| row.label == "Open Link"));
}

#[test]
fn a_link_previews_thumbnail_uncovers_opens_and_has_its_menu() {
    a_thumbnail_uncovers_opens_and_has_its_menu(ChatMessage {
        body: "Look".into(),
        embeds: vec![yapfast_core::chat::Embed {
            title: "A page".into(),
            url: "https://example.com/page".into(),
            ..Default::default()
        }],
        attachments: vec![picture_at("t.png", 80, Placement::Thumbnail(0), true)],
        ..Default::default()
    });
}

#[test]
fn an_app_cards_thumbnail_uncovers_opens_and_has_its_menu() {
    use yapfast_core::chat::{Beside, Card, CardRow};
    a_thumbnail_uncovers_opens_and_has_its_menu(ChatMessage {
        bot: true,
        cards: vec![Card {
            framed: true,
            color: None,
            rows: vec![CardRow::Text {
                text: "Today's map".into(),
                beside: Some(Beside::Thumbnail(0)),
            }],
        }],
        attachments: vec![picture_at("t.png", 85, Placement::Component(0), true)],
        ..Default::default()
    });
}

#[test]
fn a_video_links_poster_opens_the_videos_page() {
    use crate::app::Message;
    let (mut app, _folder) = preview();
    let page = "https://video.example.com/watch?v=1";
    let mut poster = picture_at("poster.jpg", 200, Placement::Embed(0), true);
    poster.image = false;
    poster.preview = Some(Arc::new(Picture {
        width: 200,
        height: 200,
        rgba: Pixels::new(200, 200, vec![128; 200 * 200 * 4]),
        ..Default::default()
    }));
    only(
        &mut app,
        ChatMessage {
            body: "Look".into(),
            embeds: vec![yapfast_core::chat::Embed {
                title: "A video".into(),
                url: page.into(),
                video: true,
                ..Default::default()
            }],
            attachments: vec![poster],
            ..Default::default()
        },
    );
    let at = bounds_of(&app, "SPOILER").center();
    let uncovered = clicked(&app, at, iced::mouse::Button::Left);
    apply(&mut app, uncovered);
    let opened = clicked(&app, at, iced::mouse::Button::Left);
    assert!(
        opened
            .iter()
            .any(|message| matches!(message, Message::Link(link, _) if link == page)),
        "its page"
    );
}

/// The open room holding Sam's message "Look" with a 200 px picture,
/// sent, out of the preview; where the picture's top left corner is.
fn a_sent_picture(app: &mut crate::app::App, picture: Attachment) -> iced::Point {
    only(
        app,
        ChatMessage {
            body: "Look".into(),
            attachments: vec![picture],
            ..Default::default()
        },
    );
    signed_in(app);
    let body = bounds_of(app, "Look");
    // Under the text, past the gallery's 4 px.
    iced::Point::new(body.x, body.y + body.height + 4.0)
}

/// Out of the preview, signed in, the preview's rooms kept.
fn signed_in(app: &mut crate::app::App) {
    change(app, |state| {
        state.preview = false;
        state.account = Some("1".into());
    });
}

#[test]
fn a_picture_of_the_message_under_the_pointer_has_save() {
    use crate::{app::Message, image_viewer::Event};
    let (mut app, _folder) = preview();
    let corner = a_sent_picture(
        &mut app,
        picture_at("a.png", 200, Placement::Message, false),
    );
    // The button's middle: 8 px in from the top right, 32 px across.
    let save = iced::Point::new(corner.x + 200.0 - 24.0, corner.y + 24.0);
    let away = clicked(&app, save, iced::mouse::Button::Left);
    assert!(
        !away
            .iter()
            .any(|message| matches!(message, Message::Image(Event::Save(..)))),
        "no Save off the message"
    );
    app.pointed.message = Some("i0".into());
    let saved = clicked(&app, save, iced::mouse::Button::Left);
    assert!(
        saved
            .iter()
            .any(|message| matches!(message, Message::Image(Event::Save(item, 0)) if item == "i0")),
        "saved"
    );
}

#[test]
fn a_messages_gif_has_a_star_that_makes_it_a_favourite() {
    let (mut app, _folder) = preview();
    let url = "https://tenor.com/view/wave-1";
    let mut gif = picture_at("wave.gif", 200, Placement::Message, false);
    gif.gif = Some(yapfast_core::chat::Gif {
        url: url.into(),
        src: "https://media.tenor.com/wave.mp4".into(),
        gif_src: "https://media.tenor.com/wave.gif".into(),
        width: 200,
        height: 200,
        ..Default::default()
    });
    let corner = a_sent_picture(&mut app, gif);
    change(&mut app, |state| state.favorite_gifs = Some(Vec::new()));
    // The star's middle: 8 px in from the top left, 32 px across.
    let star = iced::Point::new(corner.x + 24.0, corner.y + 24.0);
    app.pointed.message = Some("i0".into());
    let starred = clicked(&app, star, iced::mouse::Button::Left);
    apply(&mut app, starred);
    assert!(app.client.state().is_favorite_gif(url), "a favourite");
    assert!(
        app.client.sent.iter().any(|command| matches!(
            command,
            yapfast_core::chat::Command::Discord(yapfast_core::chat::DiscordCommand::FavoriteGifs)
        )),
        "the favourites asked for"
    );
}

/// Sam's message with a 40 s video, playing full screen, out of the
/// preview.
fn a_video_full_screen(app: &mut crate::app::App) {
    let mut clip = picture_at("clip.mp4", 200, Placement::Message, false);
    (clip.image, clip.video) = (false, true);
    signed_in(app);
    only(
        app,
        ChatMessage {
            body: "Look".into(),
            attachments: vec![clip],
            ..Default::default()
        },
    );
    change(app, |state| {
        state.video = Some(yapfast_core::chat::Video {
            room: String::new(),
            item: "i0".into(),
            index: 0,
            frames: None,
            duration_ms: Some(40_000),
            size: Some((1280, 720)),
            loading: false,
            playing: true,
            ended: false,
            sound: true,
            muted: false,
            volume: 0.5,
            error: None,
        });
    });
    let _ = crate::playback::clip_event(app, crate::playback::Clip::Full(true));
    assert!(app.playback.clip_full);
    app.client.sent.clear();
}

/// The video commands the core sent since last asked.
fn video_commands(app: &mut crate::app::App) -> Vec<yapfast_core::chat::VideoCommand> {
    std::mem::take(&mut app.client.sent)
        .into_iter()
        .filter_map(|command| match command {
            yapfast_core::chat::Command::Video(command) => Some(command),
            _ => None,
        })
        .collect()
}

#[test]
fn a_full_screen_videos_frames_follow_the_window_layout() {
    use yapfast_core::chat::VideoCommand;
    let (mut app, _folder) = preview();
    let opened = app.shell.frame;
    redraw_at(&mut app, opened);
    a_video_full_screen(&mut app);
    let smaller = iced::Size::new(660.0, 410.0);
    redraw_at(&mut app, smaller);
    assert_eq!(
        video_commands(&mut app),
        [VideoCommand::Size {
            width: 501,
            height: 282,
        }]
    );
}

#[test]
fn a_video_full_screen_plays_pauses_and_moves_five_seconds_from_the_keys() {
    use yapfast_core::chat::VideoCommand;
    let (mut app, _folder) = preview();
    a_video_full_screen(&mut app);
    let press = |app: &mut crate::app::App, named| {
        let _ = app.update(crate::app::Message::Key(
            keyboard::Key::Named(named),
            keyboard::Modifiers::default(),
        ));
    };
    press(&mut app, key::Named::Space);
    assert_eq!(video_commands(&mut app), [VideoCommand::Pause]);
    // 10 s in, as its last frame says.
    app.playback.clip_time = Some(10_000);
    press(&mut app, key::Named::ArrowRight);
    assert_eq!(video_commands(&mut app), [VideoCommand::Seek(15_000)]);
    app.playback.clip_time = Some(10_000);
    press(&mut app, key::Named::ArrowLeft);
    assert_eq!(video_commands(&mut app), [VideoCommand::Seek(5_000)]);
    press(&mut app, key::Named::Escape);
    assert!(!app.playback.clip_full, "back in its cell");
}

#[test]
fn a_video_full_screen_has_save_and_a_menu_of_the_videos_rows() {
    use crate::{app::Message, image_viewer::Event};
    let (mut app, _folder) = preview();
    a_video_full_screen(&mut app);
    let window = app.shell.frame;
    // Left of Exit full screen in the title bar.
    let save = iced::Point::new(window.width - 12.0 - 32.0 - 8.0 - 16.0, 32.0);
    let saved = clicked(&app, save, iced::mouse::Button::Left);
    assert!(
        saved
            .iter()
            .any(|message| matches!(message, Message::Image(Event::Save(item, 0)) if item == "i0")),
        "saved"
    );
    let middle = iced::Point::new(window.width / 2.0, window.height / 2.0);
    let menu = clicked(&app, middle, iced::mouse::Button::Right);
    apply(&mut app, menu);
    let menu = app.win.menu.as_ref().expect("its menu");
    let rows: Vec<String> = menu.rows.iter().map(|row| row.label.to_string()).collect();
    assert_eq!(rows, ["Save Video", "Copy Link", "Open Link"]);
}

#[test]
fn clicking_a_full_screen_videos_volume_slider_does_not_pause_it() {
    use crate::{app::Message, playback::Clip};
    let (mut app, _folder) = preview();
    a_video_full_screen(&mut app);
    app.playback.clip_volume = 1;
    let window = app.shell.frame;
    let size = crate::viewer::fitted(window, Some((1280, 720)));
    let room = crate::viewer::room(window);
    let top = ((room.height - size.height) / 2.0).max(0.0);
    let foot_y = crate::viewer::BAR + top + size.height + 8.0;
    let foot_right = (window.width + size.width.max(360.0)) / 2.0;
    // The mute button sits left of Exit full screen (6 px right padding,
    // 28 px button, 6 px gap, 28 px mute button); the slider rises 112 px
    // above it, overlapping the bottom-right of the video.
    let slider_x = foot_right - 6.0 - 28.0 - 6.0 - 14.0;
    // Upper half of the track (away from the initial 0.5 value) so the
    // slider publishes a new volume while overlapping the video.
    let on_slider = clicked(
        &app,
        iced::Point::new(slider_x, foot_y - 74.0),
        iced::mouse::Button::Left,
    );
    assert!(
        on_slider
            .iter()
            .any(|message| matches!(message, Message::Clip(Clip::Volume(_)))),
        "changed the volume"
    );
    assert!(
        !on_slider
            .iter()
            .any(|message| matches!(message, Message::Clip(Clip::Toggle | Clip::Full(false)))),
        "did not toggle or close the video"
    );
    // Its top padding also overlaps the video and leaves playback alone.
    let on_padding = clicked(
        &app,
        iced::Point::new(slider_x, foot_y - 104.0),
        iced::mouse::Button::Left,
    );
    assert!(
        !on_padding
            .iter()
            .any(|message| matches!(message, Message::Clip(Clip::Toggle | Clip::Full(false)))),
        "padding did not toggle or close the video"
    );
    // Clicking the video itself still toggles playback.
    let middle = iced::Point::new(window.width / 2.0, window.height / 2.0);
    let on_video = clicked(&app, middle, iced::mouse::Button::Left);
    assert!(
        on_video
            .iter()
            .any(|message| matches!(message, Message::Clip(Clip::Toggle))),
        "toggled the video"
    );
}

#[test]
fn the_wheel_on_a_videos_volume_turns_it_five_percent() {
    use yapfast_core::chat::VideoCommand;
    let (mut app, _folder) = preview();
    a_video_full_screen(&mut app);
    let mut turn = |up: f32| {
        let _ = crate::playback::clip_event(&mut app, crate::playback::Clip::Turn(up));
        match video_commands(&mut app).as_slice() {
            [VideoCommand::Volume(volume)] => *volume,
            other => panic!("{other:?}"),
        }
    };
    assert!((turn(1.0) - 0.55).abs() < 0.001);
    assert!((turn(-1.0) - 0.45).abs() < 0.001);
    // A turn sideways is none: nothing is sent.
    let _ = crate::playback::clip_event(&mut app, crate::playback::Clip::Turn(0.0));
    assert!(video_commands(&mut app).is_empty());
}

/// Sam's voice message of 5 s, sent.
fn a_voice_message(app: &mut crate::app::App) {
    let mut recording = picture_at("voice-message.ogg", 0, Placement::Message, false);
    (recording.image, recording.dimensions) = (false, None);
    only(
        app,
        ChatMessage {
            attachments: vec![recording],
            voice: Some(yapfast_core::chat::VoiceMessage {
                duration_ms: 5_000,
                waveform: Arc::from(vec![128; 32]),
            }),
            ..Default::default()
        },
    );
}

#[test]
fn a_voice_messages_speed_and_mute_always_show_with_animations_off() {
    let (mut app, _folder) = preview();
    a_voice_message(&mut app);
    assert!(shows(&app, "0:05"), "the card");
    assert!(!shows(&app, "1x"), "its speed only under the pointer");
    app.layout.prefs.animate = crate::layout::Animate::Never;
    assert!(shows(&app, "1x"), "and always with animations off");
}

#[test]
fn a_voice_messages_volume_takes_the_arrows_and_page_keys_until_escape() {
    use crate::{app::Message, playback::Event};
    let (mut app, _folder) = preview();
    a_voice_message(&mut app);
    let volume = |app: &crate::app::App| app.client.state().voice_tuning.volume;
    let press = |app: &mut crate::app::App, named| {
        let _ = app.update(Message::Key(
            keyboard::Key::Named(named),
            keyboard::Modifiers::default(),
        ));
    };
    let _ = crate::playback::voice_event(&mut app, Event::VolumeHover("i0".into(), true));
    let _ = crate::playback::voice_event(&mut app, Event::Volume(0.5));
    press(&mut app, key::Named::ArrowUp);
    assert!((volume(&app) - 0.55).abs() < 0.001, "{}", volume(&app));
    press(&mut app, key::Named::PageDown);
    assert!((volume(&app) - 0.35).abs() < 0.001, "{}", volume(&app));
    press(&mut app, key::Named::Escape);
    assert!(app.playback.voice_volume.is_none(), "closed");
    press(&mut app, key::Named::ArrowUp);
    assert!(
        (volume(&app) - 0.35).abs() < 0.001,
        "the keys go on as before"
    );
}

#[test]
fn a_store_button_on_a_bots_message_without_its_app_opens_the_bots_store() {
    use crate::app::Message;
    use yapfast_core::chat::{ButtonStyle, Card, CardButton, CardRow};
    let (mut app, _folder) = preview();
    only(
        &mut app,
        ChatMessage {
            bot: true,
            sender: "discord:42".into(),
            cards: vec![Card {
                framed: false,
                color: None,
                rows: vec![CardRow::Buttons(vec![CardButton {
                    label: String::new(),
                    style: ButtonStyle::Premium,
                    url: None,
                    custom_id: None,
                    sku_id: Some("7".into()),
                    disabled: false,
                }])],
            }],
            ..Default::default()
        },
    );
    let at = bounds_of(&app, "Open in Discord").center();
    let opened = clicked(&app, at, iced::mouse::Button::Left);
    assert!(
        opened.iter().any(|message| matches!(
            message,
            Message::Link(link, _) if link == "https://discord.com/application-directory/42/store/7"
        )),
        "the bot's store"
    );
}

/// The size of a picture a handle holds.
fn held(handle: &iced::advanced::image::Handle) -> (u32, u32) {
    match handle {
        iced::advanced::image::Handle::Rgba { width, height, .. } => (*width, *height),
        _ => panic!("pictures are pixels"),
    }
}

#[test]
fn pictures_are_fitted_at_the_windows_pixels() {
    let (mut app, _folder) = preview();
    let picture = Arc::new(Picture {
        width: 64,
        height: 64,
        rgba: Pixels::new(64, 64, vec![200; 64 * 64 * 4]),
        ..Default::default()
    });
    let at_100 = held(&app.ctx().fit(&picture, 20, 20, false).expect("fitted"));
    assert_eq!(at_100, (20, 20));
    // At 110% a 20-unit emoji spans 22 pixels, so it's made at 22, not
    // stretched there by the renderer.
    app.shell.zoom = 1.1;
    let at_110 = held(&app.ctx().fit(&picture, 20, 20, false).expect("fitted"));
    assert_eq!(at_110, (22, 22));
    // One that came at the window's pixels is shown as it is.
    let exact = Arc::new(Picture {
        width: 22,
        height: 22,
        rgba: Pixels::new(22, 22, vec![200; 22 * 22 * 4]),
        ..Default::default()
    });
    assert_eq!(
        held(&app.ctx().fit(&exact, 20, 20, false).expect("own")),
        (22, 22)
    );
}
