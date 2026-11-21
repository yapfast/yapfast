//! A call's stage: this account's shared screen first, then the streams of
//! those who film as well in tiles of their own, then everyone. A click on
//! a tile shows it large, with the others in rows beneath, until a click
//! beside it; the large one goes full screen. Under the stage, the call's
//! controls; over it, a line on how the call is. A voice channel this
//! account isn't in shows its Join instead.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Size,
    widget::{Space, button, column, container, mouse_area, pin, row, stack, text},
};
use yapfast_core::voice::{self, SharePhase};

use super::{Event, Popup, voice_ready};
use crate::{
    app::{App, Message},
    controls::{self, ROUND, ROUND_GLYPH, bar},
    header,
    pictures::Icon,
    style::{faded, fill, rounded},
    theme::rgb,
    tile::{self, GLASS, NOTE, Tile},
    video::{self, Video},
    widgets::Spot,
};

mod layout;
pub use layout::{
    CORNER, Focus, Pip, Slot, Stage, share_status, shown_at, speaking, status, tiles,
};

/// The room a voice channel's stage leaves for the line over it and the
/// controls under it.
pub const CONTROLS: f32 = 111.0;
/// The controls' row under the stage: 8 px over the buttons, 16 under.
const CONTROLS_HEIGHT: f32 = 8.0 + ROUND + 16.0;

/// The open room is a voice channel: the window shows its call.
pub fn voice_selected(app: &App) -> bool {
    let state = app.client.state();
    state.selected.as_ref().is_some_and(|selected| {
        state
            .rooms
            .iter()
            .any(|room| &room.id == selected && room.voice && !room.direct)
    })
}

/// A voice channel is open on its call, not on its chat.
pub fn voice_panel(app: &App) -> bool {
    voice_selected(app) && !app.client.state().voice_chat
}

/// The open room's call is the one this account is in.
pub fn voice_here(app: &App) -> bool {
    let state = app.client.state();
    state.voice.active() && state.voice.room.is_some() && state.voice.room == state.selected
}

/// The stage shows: its call is open, here or above a DM's messages,
/// or full screen, in a window (closed to the tray, nothing shows).
pub fn stage_shown(app: &App) -> bool {
    app.shell.window.is_some()
        && (voice_here(app) && (voice_panel(app) || crate::call::dm::dm_call(app).is_some())
            || full_shown(app))
}

/// The large tile is over the whole window.
pub fn full_shown(app: &App) -> bool {
    app.call.full && app.client.state().voice.active() && app.call.focus.is_some()
}

/// The stage's size where it shows: full screen, in a voice channel
/// under its header, or above a DM's messages.
pub fn stage_size(app: &App) -> Size {
    if full_shown(app) {
        return app.window;
    }
    let below_header = app.window.height - header::HEIGHT - 1.0;
    let (right, height) = if voice_panel(app) {
        (app.window.width, below_header)
    } else {
        // Above a DM's messages, beside its profile panel.
        (
            crate::conversation::chat_right(app),
            crate::call::dm::dm_panel_height(app, below_header),
        )
    };
    let width = right - crate::conversation::chat_left(app);
    Size::new(width, (height - CONTROLS).max(0.0))
}

/// The stage's tiles, the one shown large, where each goes, and how tall
/// they are together.
pub fn stage(app: &App) -> (Vec<Slot>, Option<usize>, Vec<Rectangle>, f32) {
    let voice = &app.client.state().voice;
    let stage = Stage::of(voice);
    let slots = stage.slots();
    let focused = app.call.focus.as_ref().and_then(|focus| stage.slot(focus));
    let (tiles, content) = tiles(slots.len(), focused, stage_size(app), full_shown(app));
    (slots, focused, tiles, content)
}

/// What the corner picture shows while the call is off screen.
pub fn corner(app: &App) -> Pip {
    let voice = &app.client.state().voice;
    if app.shell.window.is_none()
        || !crate::signin::workspace(app)
        || !voice.active()
        || stage_shown(app)
    {
        return Pip::None;
    }
    let watching = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    Stage::of(voice).pip(app.call.focus.as_ref(), watching)
}

/// The corner's tile, by its place on the stage.
pub fn corner_place(app: &App) -> Option<usize> {
    let voice = &app.client.state().voice;
    let watching = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    Stage::of(voice).pip_slot(corner(app), watching)
}

/// A tile shown large that's gone (its stream ended, they left) shows
/// none.
pub fn keep_focus(app: &mut App) {
    let voice = &app.client.state().voice;
    let stage = Stage::of(voice);
    if app
        .call
        .focus
        .as_ref()
        .is_some_and(|focus| stage.slot(focus).is_none())
    {
        app.call.focus = None;
        app.call.full = false;
    }
}

/// Tells each camera where its tile shows, or that it doesn't, so frames
/// are made only for tiles in sight, at their size, and whether it's
/// large; and has [`video::frames`] look for frames from those that
/// show: cameras, the stream watched, the screen shared, a message's
/// video. With the call off screen, the corner's shows there.
pub fn place_videos(app: &mut App) {
    let shown = stage_shown(app);
    let (slots, focused, tiles, _) = stage(app);
    let height = stage_size(app).height;
    let corner = corner(app);
    let voice = &app.client.state().voice;
    let mut sources = video::Sources::default();
    // The camera Voice's settings try.
    if let Some(feed) = app.camera_test() {
        sources
            .feeds
            .push((crate::settings::voice::CAMERA_TEST.to_owned(), feed.clone()));
    }
    // In the window's pixels.
    let scale = app.device_scale();
    let corner_pixels = (
        (CORNER.width * scale).round() as u32,
        (CORNER.height * scale).round() as u32,
    );
    for (place, slot) in slots.iter().enumerate() {
        let Slot::Person(index) = *slot else {
            continue;
        };
        let person = &voice.participants[index];
        let Some(feed) = &person.video else {
            continue;
        };
        let in_corner = corner == Pip::Camera(index);
        let at = if in_corner {
            Some(corner_pixels)
        } else {
            shown
                .then(|| shown_at(tiles[place], app.call.scrolled, height, scale))
                .flatten()
        };
        feed.show_at(at);
        feed.show_large(shown && !in_corner && focused == Some(place));
        if at.is_some() {
            sources.feeds.push((person.id.clone(), feed.clone()));
        }
    }
    if (shown || corner == Pip::Watched)
        && let Some(watching) = &voice.watching
        && let Some(frames) = &watching.frames
    {
        sources
            .mapped
            .push((format!("stream:{}", watching.user), frames.clone()));
    }
    if let Some(sharing) = &voice.sharing {
        let visible = shown || corner == Pip::Shared;
        sharing
            .frames
            .set_preview_paused(!visible || !app.shell.focused);
        // Keep the last frame in the visible tile while the window is unfocused.
        if visible {
            sources
                .mapped
                .push((OWN_SCREEN.to_owned(), sharing.frames.clone()));
        }
    }
    // A message's video, wherever it plays.
    let mut clip = false;
    if let Some(playing) = &app.client.state().video
        && let Some(frames) = &playing.frames
    {
        clip = true;
        sources.mapped.push((
            format!("clip:{}/{}", playing.item, playing.index),
            frames.clone(),
        ));
    }
    if !clip {
        app.playback.clip_time = None;
    }
    app.video.set(sources);
}

/// New frames that change the view: a source's first, or a message's
/// video's, how far it's got.
pub fn frames_found(app: &mut App, clip_time: Option<u64>) {
    if clip_time.is_some() {
        app.playback.clip_time = clip_time;
    }
}

/// A tile was clicked, by its place, and is shown large; or beside the
/// tiles, and none is.
pub fn focus_tile(app: &mut App, place: Option<usize>) {
    let voice = &app.client.state().voice;
    app.call.focus = place.and_then(|place| Stage::of(voice).focus_at(place));
    if app.call.focus.is_none() {
        app.call.full = false;
    }
    place_videos(app);
}

/// A frame by its key, fitted into its tile with `corners` cut round
/// on `behind` (none full screen).
pub fn frame<'a>(
    app: &'a App,
    key: &str,
    corners: Option<(u32, u32)>,
) -> Option<Element<'a, Message>> {
    app.video.has(key).then(|| {
        let video = Video::new(&app.video, key, Color::BLACK).scale(app.device_scale());
        let video = match corners {
            Some((radius, behind)) => video.corners(radius, behind),
            None => video,
        };
        Element::from(video)
    })
}

/// Holds the last frame under a note while the outgoing preview is paused.
pub(super) fn paused_share_preview(content: Element<'_, Message>) -> Element<'_, Message> {
    let note = column![
        text("Your stream is still running!")
            .size(15)
            .font(crate::fonts::BOLD)
            .color(Color::WHITE)
            .align_x(Alignment::Center),
        text("We've paused this preview to save your resources.")
            .size(13)
            .color(NOTE)
            .align_x(Alignment::Center),
    ]
    .spacing(4)
    .align_x(Alignment::Center);
    stack![
        content,
        container(note)
            .padding([0, 12])
            .center(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgba(0.07, 0.08, 0.11, 0.85).into()),
                ..Default::default()
            }),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A voice channel's view: its header, then its call's stage with the
/// controls, or its Join while this account is elsewhere.
pub fn call_view(app: &App) -> Element<'_, Message> {
    let line = bar(app.theme.outline_chat, Length::Fill, 1);
    let body = if voice_here(app) {
        call_panel(app)
    } else {
        join_view(app)
    };
    // The window's ground is the chat's; the stage paints its own only
    // where a theme gives it another (none built in does), as a ground
    // under the video is drawn again with every frame shown.
    let theme = &app.theme;
    let stage = (theme.stage != theme.chat).then_some(theme.stage);
    // The banners show over the call too, under the header.
    let column = column![crate::header::view(app.ctx()), line]
        .extend(crate::banners::view(app.ctx()))
        .push(body);
    container(column)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| stage.map(fill).unwrap_or_default())
        .into()
}

/// The call this account is in: the line over the stage, the stage
/// (unless it's full screen), and the controls.
pub fn call_panel(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let line = container(
        text(voice_description(app))
            .size(14)
            .color(theme.muted)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(Padding {
        top: 16.0,
        left: 16.0,
        right: 16.0,
        bottom: 0.0,
    })
    .height(CONTROLS - CONTROLS_HEIGHT);
    let size = stage_size(app);
    let stage: Element<'_, Message> = if full_shown(app) {
        Space::new().width(size.width).height(size.height).into()
    } else {
        stage_view(app)
    };
    column![line, stage, call_controls(app)].into()
}

/// What the line over the stage, or a channel's Join, says of the call.
pub fn voice_description(app: &App) -> String {
    let state = app.client.state();
    let voice = &state.voice;
    let room = state.selected_room();
    let present = room.map_or(&[][..], |room| room.people.as_slice());
    if room.is_some_and(|room| room.locked) {
        return "Your roles on this Discord server don't let you join this channel.".into();
    }
    if let Some(room) = crate::call::dm::dm_call(app).filter(|_| voice_here(app)) {
        let ringing: Vec<&str> = room
            .ringing
            .iter()
            .map(|person| person.name.as_str())
            .collect();
        return if !ringing.is_empty() {
            format!("Calling {}...", ringing.join(", "))
        } else if voice.participants.len() == 1 {
            "Nobody else is in the call yet.".into()
        } else {
            "The call stays connected while you browse other conversations.".into()
        };
    }
    if voice_here(app) {
        return if voice.participants.len() == 1 {
            "You're here alone. Stay as long as you like; others can join later.".into()
        } else {
            "Voice stays connected while you browse other conversations.".into()
        };
    }
    let join = if joins_muted(app) {
        "Join muted and hang out."
    } else {
        "Join and hang out."
    };
    if present.is_empty() {
        format!("{join} The channel stays here when empty.")
    } else {
        format!("{} here. {join}", super::who_is_here(present))
    }
}

/// The next call starts muted.
pub fn joins_muted(app: &App) -> bool {
    !app.layout.unmute_on_join.unwrap_or(true) || app.layout.deafen_on_join
}

/// A voice channel this account isn't in: what it is, and a way to join.
fn join_view(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let state = app.client.state();
    let id = state.selected.clone().unwrap_or_default();
    let raised = theme.raised;
    let badge = container(app.look().glyph(Icon::Volume, 38, theme.secondary))
        .center(80)
        .style(move |_| rounded(raised, 40));
    let mut panel = column![
        badge,
        text(app.header.name.as_str())
            .size(26)
            .font(crate::fonts::BOLD)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
        text(voice_description(app))
            .color(theme.secondary)
            .align_x(Alignment::Center),
    ]
    .spacing(14)
    .padding(32)
    .align_x(Alignment::Center);
    let encrypted = state.preview
        || state
            .selected_room()
            .is_some_and(|room| room.call_encrypted());
    if !encrypted {
        panel = panel.push(
            text("This channel is unencrypted. Yapfast only joins encrypted voice channels.")
                .size(15)
                .color(theme.accent)
                .align_x(Alignment::Center),
        );
    }
    let label = if state.voice.active() {
        "Switch voice channel"
    } else {
        "Join voice"
    };
    let can_join = app.client.state().can_join_voice(&id);
    let join: Element<'_, Message> = controls::action(
        theme,
        label,
        controls::Kind::Primary,
        can_join.then(|| Message::Call(Event::Join(id))),
    )
    .into();
    panel = panel.push(container(join).padding(Padding {
        top: 6.0,
        ..Padding::ZERO
    }));
    container(panel).center(Length::Fill).into()
}

/// The stage: the tiles where they go, scrolled when they're taller
/// than it.
pub fn stage_view(app: &App) -> Element<'_, Message> {
    let size = stage_size(app);
    let full = full_shown(app);
    let (slots, focused, tiles, content) = stage(app);
    // A click beside the large tile shows the grid again.
    let beside = if focused.is_some() && !full {
        Message::Call(Event::Focus(None))
    } else {
        Message::Nothing
    };
    let mut layers =
        stack![mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(beside)];
    for (place, (slot, bounds)) in slots.iter().zip(&tiles).enumerate() {
        if bounds.width <= 0.0 || (full && focused != Some(place)) {
            continue;
        }
        let tile = stage_tile(app, place, *slot, *bounds, focused == Some(place), full);
        layers = layers.push(pin(tile).x(bounds.x).y(bounds.y));
    }
    let grid = layers.width(size.width).height(content);
    if content <= size.height + 0.5 {
        return container(grid)
            .width(size.width)
            .height(size.height)
            .clip(true)
            .into();
    }
    scroll_view(grid)
        .width(size.width)
        .height(size.height)
        .on_scroll(|viewport| Message::Call(Event::Scrolled(viewport.absolute_offset().y)))
        .into()
}

/// One tile on the stage.
fn stage_tile(
    app: &App,
    place: usize,
    slot: Slot,
    bounds: Rectangle,
    large: bool,
    full: bool,
) -> Element<'_, Message> {
    let theme = &app.theme;
    let voice = &app.client.state().voice;
    let size = bounds.size();
    let pointed = app.call.pointed == Some(place);
    let corners = (!full).then(|| (tile::RADIUS as u32, rgb(theme.stage)));
    let watch_user = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    let watch_viewers = || {
        voice
            .watching
            .as_ref()
            .and_then(|watching| stream_viewers(app, &watching.viewers, Some(&watching.user)))
    };
    let mut stop = None;
    let mut viewers = None;
    let mut on_press = Message::Call(Event::Focus(Some(place)));
    let mut right = None;
    let mut tile = match slot {
        Slot::OwnScreen => {
            let sharing = voice.sharing.as_ref();
            let live = sharing.is_some_and(|sharing| sharing.phase == SharePhase::Live);
            let status = share_status(voice);
            let framed = frame(app, OWN_SCREEN, corners);
            let note = (!live && framed.is_some()).then_some(status);
            viewers = sharing.and_then(|sharing| stream_viewers(app, &sharing.viewers, None));
            let content =
                framed.unwrap_or_else(|| tile::note(app.look(), Some(Icon::ScreenShare), status));
            let content = if live && !app.shell.focused {
                paused_share_preview(content)
            } else {
                content
            };
            Tile::new(size, content, Some(Color::BLACK)).chip(tile::chip(
                app.look(),
                Vec::new(),
                "Your screen",
                live,
                note,
            ))
        }
        Slot::Stream(index) => {
            let person = &voice.participants[index];
            let watched = watch_user == Some(person.id.as_str());
            right = Some(person.id.clone());
            let content = if watched {
                viewers = watch_viewers();
                stop = Some("Stop watching");
                frame(app, &format!("stream:{}", person.id), corners)
                    .unwrap_or_else(|| tile::note(app.look(), None, watch_status(app)))
            } else {
                on_press = Message::Call(Event::Watch(person.id.clone()));
                let column = column![
                    app.look().glyph(Icon::ScreenShare, 30, NOTE),
                    tile::pill("Watch stream", theme.danger, theme.on_danger),
                ]
                .spacing(10)
                .align_x(Alignment::Center);
                container(column).center(Length::Fill).into()
            };
            Tile::new(size, content, Some(Color::BLACK)).chip(tile::chip(
                app.look(),
                vec![(Icon::ScreenShare, Color::WHITE)],
                &person.user,
                true,
                None,
            ))
        }
        Slot::Person(index) => {
            let person = &voice.participants[index];
            right = Some(person.id.clone());
            // Someone who shares without a camera shows the stream in
            // their tile, once it's watched; until then a click watches
            // it.
            let streams_here = person.streaming && !person.local && person.video.is_none();
            let watched = streams_here && watch_user == Some(person.id.as_str());
            let filmed = person
                .video
                .as_ref()
                .and_then(|_| frame(app, &person.id, corners));
            let (content, ground) = if let Some(filmed) = filmed {
                (filmed, Color::BLACK)
            } else if watched {
                viewers = watch_viewers();
                stop = Some("Stop watching");
                let content = frame(app, &format!("stream:{}", person.id), corners)
                    .unwrap_or_else(|| tile::note(app.look(), None, watch_status(app)));
                (content, Color::BLACK)
            } else {
                // Their picture until their camera's first frame, as
                // without one; a camera whose frames stop keeps its
                // last.
                let side = (size.height * 0.45).min(80.0).round();
                let opacity = if person.connected { 1.0 } else { 0.5 };
                let avatar = app.ctx().avatar(
                    person.avatar.as_deref(),
                    &person.user,
                    side,
                    side / 2.0,
                    opacity,
                );
                let content = container(avatar).center(Length::Fill).padding(Padding {
                    bottom: 16.0,
                    ..Padding::ZERO
                });
                (content.into(), theme.raised)
            };
            if streams_here && !watched {
                on_press = Message::Call(Event::Watch(person.id.clone()));
            }
            let mut marks = Vec::new();
            if person.muted && !person.deafened {
                marks.push((Icon::MicOff, theme.danger));
            }
            if person.deafened {
                marks.push((Icon::HeadphoneOff, theme.danger));
            }
            let said = status(person);
            let note =
                (!matches!(said, "Muted" | "Deafened" | "Speaking" | "Listening")).then_some(said);
            // A stream with a tile of its own is LIVE there.
            let live = person.streaming && (person.local || person.video.is_none());
            let mut tile = Tile::new(size, content, Some(ground))
                .ring(speaking(person).then_some(theme.live))
                .chip(tile::chip(app.look(), marks, &person.user, live, note));
            if person.local && voice.camera_phase == voice::CameraPhase::Starting {
                tile = tile.top_left(tile::pill("Starting camera...", GLASS, Color::WHITE));
            }
            if streams_here && !watched {
                tile = tile.top_right(tile::pill("Watch stream", theme.danger, theme.on_danger));
            }
            tile
        }
    };
    if full {
        tile = tile.square();
    }
    if let Some(viewers) = viewers {
        tile = tile.top_right(viewers);
    }
    // The stream's stop shows on the large tile, and on another while
    // the pointer is on it.
    if let Some(label) = stop
        && (pointed || large)
    {
        tile = tile.top_right(tile::glass_button(
            app.look(),
            Icon::X,
            label,
            Message::Call(Event::StopWatching),
        ));
    }
    if large && pointed {
        let (icon, label) = if full {
            (Icon::Minimize, "Exit full screen")
        } else {
            (Icon::Maximize, "Full screen")
        };
        tile = tile.bottom_right(tile::glass_button(
            app.look(),
            icon,
            label,
            Message::Call(Event::Full(!full)),
        ));
    }
    if large {
        on_press = Message::Nothing;
        tile = tile.on_double(Message::Call(Event::Full(!full)));
    }
    if let (Some(user), Some(room)) = (right, voice.room.clone()) {
        tile =
            tile.on_right(move |at| Message::Call(Event::Person(room.clone(), user.clone(), at)));
    }
    tile.on_press(on_press)
        .on_point(
            Message::Call(Event::Point(place, true)),
            Message::Call(Event::Point(place, false)),
        )
        .into()
}

/// What a watched stream's tile says until its first picture.
pub fn watch_status(app: &App) -> String {
    let Some(watching) = &app.client.state().voice.watching else {
        return String::new();
    };
    if watching.frames.is_none() {
        format!("Connecting to {}'s stream...", watching.name)
    } else {
        format!("Waiting for {}'s picture...", watching.name)
    }
}

/// Who watches a stream, in the call's order, but its streamer: their
/// faces over its tile.
pub fn stream_viewers<'a>(
    app: &'a App,
    ids: &[String],
    streamer: Option<&str>,
) -> Option<Element<'a, Message>> {
    let voice = &app.client.state().voice;
    let people: Vec<(&str, Option<&str>)> = voice
        .participants
        .iter()
        .filter(|person| Some(person.id.as_str()) != streamer && ids.contains(&person.id))
        .map(|person| (person.user.as_str(), person.avatar.as_deref()))
        .collect();
    let names: Vec<&str> = people.iter().map(|(name, _)| *name).collect();
    let names = format!("Watching: {}", names.join(", "));
    viewers(app.ctx(), &people, names)
}

/// The call's controls under the stage: mute, deafen, the soundboard, the
/// camera, share the screen, and leave.
fn call_controls(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let voice = &app.client.state().voice;
    let ready = voice_ready(voice);
    let when = |message: Message| ready.then_some(message);
    let alert = faded(theme.danger, 0.18);
    let face = |icon| app.look().glyph(icon, ROUND_GLYPH, theme.text);
    let mut controls = row![
        controls::round_button(
            face(if voice.muted { Icon::MicOff } else { Icon::Mic }),
            if voice.muted { "Unmute" } else { "Mute" },
            if voice.muted { alert } else { theme.raised },
            theme,
            when(Message::Call(Event::Mute)),
        ),
        controls::round_button(
            face(if voice.deafened {
                Icon::HeadphoneOff
            } else {
                Icon::Headphones
            }),
            if voice.deafened { "Undeafen" } else { "Deafen" },
            if voice.deafened { alert } else { theme.raised },
            theme,
            when(Message::Call(Event::Deafen)),
        ),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if voice.soundboard.is_some() {
        controls = controls.push(app.ctx().spot(controls::round_button(
            face(Icon::Soundboard),
            "Open Soundboard",
            theme.raised,
            theme,
            when(Message::Call(Event::Open(Popup::Soundboard))),
        )));
    }
    if voice.can_film {
        let on = voice.camera_phase != voice::CameraPhase::Off;
        let starting = voice.camera_phase == voice::CameraPhase::Starting;
        controls = controls.push(controls::round_button(
            face(if on { Icon::Video } else { Icon::VideoOff }),
            if starting {
                "Starting camera..."
            } else if on {
                "Turn off camera"
            } else {
                "Turn on camera"
            },
            if on {
                faded(theme.live, 0.22)
            } else {
                theme.raised
            },
            theme,
            when(Message::Call(Event::Camera)),
        ));
    }
    let sharing = voice.sharing.is_some();
    if can_share(app.ctx()) || sharing {
        let share = controls::round_button(
            face(if sharing {
                Icon::ScreenShareOff
            } else {
                Icon::ScreenShare
            }),
            if sharing {
                "Stop sharing"
            } else {
                "Share your screen"
            },
            if sharing {
                faded(theme.live, 0.22)
            } else {
                theme.raised
            },
            theme,
            when(Message::Call(if sharing {
                Event::StopSharing
            } else {
                Event::Open(Popup::Share)
            })),
        );
        controls = controls.push(app.ctx().spot(share));
    }
    let leaving = voice.phase == voice::Phase::Leaving;
    let (danger, danger_hover) = (theme.danger, theme.danger_hover);
    let opacity = if leaving { 0.5 } else { 1.0 };
    let leave = button(
        app.look()
            .glyph(Icon::PhoneOff, ROUND_GLYPH, theme.on_danger)
            .boxed(Length::Fill),
    )
    .width(76)
    .height(ROUND)
    .padding(0)
    .on_press_maybe((!leaving).then_some(Message::Call(Event::Leave)))
    .style(controls::lit_style(
        Some(faded(danger, opacity)),
        faded(danger_hover, opacity),
        ROUND / 2.0,
    ));
    let leave = iced::widget::tooltip(
        leave,
        controls::tip(theme, "Leave voice"),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6);
    container(controls.push(leave))
        .padding(Padding {
            top: 8.0,
            bottom: 16.0,
            left: 16.0,
            right: 16.0,
        })
        .center_x(Length::Fill)
        .height(CONTROLS_HEIGHT)
        .into()
}

/// The frames of the screen this account shares, by their key.
pub const OWN_SCREEN: &str = "share:own";

/// Who watches a stream, over its top right: an eye and how many, the first
/// three's avatars overlapping and how many more; their names in a tooltip.
pub fn viewers<'a>(
    ctx: crate::ctx::Ctx<'a>,
    people: &[(&'a str, Option<&'a str>)],
    names: String,
) -> Option<Element<'a, Message>> {
    if people.is_empty() {
        return None;
    }
    let mut faces = stack![];
    let shown = people.len().min(3);
    for (index, (name, avatar)) in people.iter().take(shown).enumerate() {
        let ring = container(ctx.avatar(*avatar, name, 22.0, 11.0, 1.0))
            .padding(2)
            .style(|_| rounded(Color::BLACK, 13));
        faces = faces.push(pin(ring).x(index as f32 * 15.0));
    }
    let faces = faces.width(26.0 + (shown as f32 - 1.0) * 15.0).height(26);
    let mut face = row![
        ctx.look.glyph(Icon::Eye, 16, Color::WHITE),
        text(people.len().to_string())
            .size(13)
            .font(crate::fonts::BOLD)
            .color(Color::WHITE),
        Space::new().width(2),
        faces,
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if people.len() > 3 {
        face = face.push(
            text(format!("+{}", people.len() - 3))
                .size(13)
                .font(crate::fonts::BOLD)
                .color(Color::WHITE),
        );
    }
    let stack = container(face)
        .padding(iced::Padding {
            left: 8.0,
            right: 4.0,
            ..iced::Padding::ZERO
        })
        .height(30)
        .align_y(Alignment::Center)
        .style(|_| rounded(GLASS, 15));
    Some(
        iced::widget::tooltip(
            stack,
            controls::tip(ctx.look.theme, names),
            iced::widget::tooltip::Position::Bottom,
        )
        .gap(6)
        .into(),
    )
}

impl<'a> crate::ctx::Ctx<'a> {
    /// A control that opens a popup above itself: where it is is kept as
    /// it's pressed.
    pub fn spot(self, child: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        Spot::new(child, &self.call.spot).into()
    }
}

/// Sharing a screen can start: Discord's call, connected.
pub fn can_share(ctx: crate::ctx::Ctx<'_>) -> bool {
    let state = ctx.state;
    let voice = &state.voice;
    !state.preview
        && voice.phase == voice::Phase::Connected
        && voice.room.as_deref().is_some_and(|room| {
            yapfast_core::network::Network::of(room) == yapfast_core::network::Network::Discord
        })
}
