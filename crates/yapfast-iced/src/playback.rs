//! What plays in the timeline. Voice messages, a mix of Discord's phone
//! and desktop players: a pill with a round button that plays or pauses
//! it, its waveform filled up to where it has played (pressed, it moves
//! there, and follows the pointer while held), its time counting down, and
//! while the pointer is on it (always with animations off) the speed and
//! mute. And videos: the poster with a play mark, then the video in its
//! cell, its controls along the bottom while the pointer is on it or it's
//! paused, and full screen with keys of its own. The core plays both and
//! says where they are; nothing here keeps time.

use iced::{
    Alignment, Background, Color, Element, Length, Padding, Size, Task, gradient,
    widget::{Space, button, container, mouse_area, row, stack, text},
};
use yapfast_core::{
    application::Action,
    chat::{ChatMessage, VideoCommand, VoiceMessage},
};

use crate::{
    app::{App, Message},
    controls::lights,
    fonts::BOLD,
    pictures::Icon,
    style::{faded, rounded},
    video::Video,
    widgets::Scrub,
};

/// The pill's height, the button across, its waveform's bars.
const PILL: f32 = 52.0;
const BUTTON: f32 = 36.0;
const BARS: usize = 32;
const BAR: f32 = 3.0;
const BAR_PITCH: f32 = 6.0;
const BAR_TALL: f32 = 24.0;

/// What the timeline's voice messages and videos show of their playing,
/// beyond what the core says of it.
#[derive(Default)]
pub struct Playback {
    /// A voice message's volume is open over its mute, by its item, while
    /// the pointer is on the button or the slider (counted, as they
    /// overlap).
    pub voice_volume: Option<(String, u8)>,
    /// The press that last took the keys for the voice messages' volume:
    /// a drag of its slider asks for them once.
    voice_press: iced::Point,
    /// The voice message paused while its waveform is held.
    pub voice_held: Option<String>,
    /// How far into the message's video playing its last frame is, in
    /// milliseconds.
    pub clip_time: Option<u64>,
    /// A message's video full screen; the size its cell asked for its
    /// frames at; the pointer on its volume's button or slider.
    pub clip_full: bool,
    pub clip_cell: (u32, u32),
    pub clip_volume: u8,
}

/// What a voice message's card asks for.
#[derive(Clone, Debug)]
pub enum Event {
    /// Play it, or pause it while it plays.
    Toggle(String),
    /// Move it this far through, from 0 to 1; pressing on its waveform
    /// pauses it while it plays, until let go.
    Seek(String, f32),
    Released(String),
    Speed,
    Mute(bool),
    /// The pointer came onto a card or left it.
    Point(String, bool),
    /// Play them this loud, from 0 to 1; a wheel's steps on the slider,
    /// up or down below zero; the pointer came onto a card's mute or its slider
    /// over it, or left.
    Volume(f32),
    Turn(f32),
    VolumeHover(String, bool),
}

/// What a video's cell asks for.
#[derive(Clone, Debug)]
pub enum Clip {
    /// Play a message's video (its item and attachment) at this size.
    Play(String, usize, Size),
    /// Pause the video playing, or play it on.
    Toggle,
    /// Move it this far through, from 0 to 1.
    Seek(f32),
    Mute(bool),
    /// Play it this loud, from 0 to 1; the pointer came onto the volume's
    /// button or its slider, or left.
    Volume(f32),
    VolumeHover(bool),
    /// A wheel's steps over the volume's button or slider, up or down
    /// below zero.
    Turn(f32),
    /// Full screen, or back in its cell.
    Full(bool),
    /// The pointer came onto a cell, by `{item}/{index}`, or left it.
    Point(String, bool),
}

/// A video's controls' bar, its buttons, and its track.
const CLIP_BAR: f32 = 36.0;
const CLIP_BUTTON: f32 = 28.0;
const TRACK: f32 = 4.0;
/// A note's and the volume slider's ground over a video.
pub const CLIP_SHADE: Color = Color::from_rgba8(0, 0, 0, 0.8);
const TRACK_REST: Color = Color::from_rgba8(255, 255, 255, 0.3);
/// How far the volume's slider rises over its button, and its gap to it.
const VOLUME_RISE: f32 = 112.0;
const VOLUME_GAP: f32 = 4.0;
/// A voice message's volume, a popup over its mute: the slider's box and
/// its gap to the button; a wheel's step turns it 5 %.
const VOICE_SLIDER: f32 = 120.0;
const VOICE_SLOT: f32 = 40.0;
/// The mute's button.
const MUTE: f32 = 28.0;
const VOICE_GAP: f32 = 6.0;
const VOICE_TURN: f32 = 0.05;
/// Page Up and Page Down on a voice message's volume.
const VOICE_PAGE: f32 = 0.2;

/// A video's time: where it is and how long it plays, such as
/// "0:12 / 1:30".
pub fn video_time(at_ms: u64, duration_ms: Option<u64>) -> String {
    let clock = |ms: u64| {
        let seconds = ms / 1000;
        match seconds / 3600 {
            0 => format!("{}:{:02}", seconds / 60, seconds % 60),
            hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
        }
    };
    match duration_ms {
        Some(duration) => format!("{} / {}", clock(at_ms.min(duration)), clock(duration)),
        None => clock(at_ms),
    }
}

/// A voice message's time as Discord's: whole seconds rounded up, so the
/// last moments read 0:01.
pub fn voice_time(ms: u64) -> String {
    let seconds = ms.div_ceil(1000);
    match seconds / 3600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

/// The loudest of each of [`BARS`] stretches of a waveform, from 0 to 1.
fn bars(waveform: &[u8]) -> impl Iterator<Item = f32> + '_ {
    (0..BARS).map(move |bar| {
        let start = bar * waveform.len() / BARS;
        let end = ((bar + 1) * waveform.len() / BARS).max(start + 1);
        let loudest = waveform.get(start..end.min(waveform.len()));
        f32::from(loudest.into_iter().flatten().copied().max().unwrap_or(0)) / 255.0
    })
}

/// What a voice message's card asked for.
pub fn voice_event(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Toggle(item) => app.client.dispatch(Action::ToggleVoiceMessage(item)),
        Event::Seek(item, at) => {
            // Playing, it pauses while held and plays on once let go.
            let playing = app
                .client
                .state()
                .voice_message
                .as_ref()
                .is_some_and(|playback| playback.item == item && playback.playing);
            if playing && app.playback.voice_held.is_none() {
                app.playback.voice_held = Some(item.clone());
                app.client
                    .dispatch(Action::ToggleVoiceMessage(item.clone()));
            }
            app.client.dispatch(Action::MoveVoiceMessage(item, at));
        }
        Event::Released(item) => {
            if app.playback.voice_held.take().is_some() {
                app.client.dispatch(Action::ToggleVoiceMessage(item));
            }
        }
        Event::Speed => app.client.dispatch(Action::CycleVoiceMessageSpeed),
        Event::Mute(muted) => app.client.dispatch(Action::MuteVoiceMessages(muted)),
        Event::Volume(volume) => {
            app.client
                .dispatch(Action::VoiceMessageVolume(volume.clamp(0.0, 1.0)));
            // Its slider pressed takes the keys from the fields, for the
            // arrows to turn it: once a press, not at each step of a drag.
            let pressed = app.pressed.get();
            let focus = if pressed == app.playback.voice_press {
                Task::none()
            } else {
                app.playback.voice_press = pressed;
                iced::widget::operation::focus(crate::app::NOBODY)
            };
            return Task::batch([app.sync(), focus]);
        }
        Event::Turn(steps) => {
            if steps == 0.0 {
                return Task::none();
            }
            turn_voice(app, steps * VOICE_TURN);
        }
        Event::VolumeHover(item, entered) => {
            let count = match &app.playback.voice_volume {
                Some((open, count)) if *open == item => *count,
                _ => 0,
            };
            let count = if entered {
                count.saturating_add(1)
            } else {
                count.saturating_sub(1)
            };
            app.playback.voice_volume = (count > 0).then_some((item, count));
            return Task::none();
        }
        Event::Point(item, entered) => {
            crate::app::point(&mut app.pointed.voice, item, entered);
            return Task::none();
        }
    }
    app.sync()
}

/// Turns the voice messages' volume `by`, from where its slider stands.
fn turn_voice(app: &mut App, by: f32) {
    let tuning = app.client.state().voice_tuning;
    let shown = if tuning.muted { 0.0 } else { tuning.volume };
    app.client
        .dispatch(Action::VoiceMessageVolume((shown + by).clamp(0.0, 1.0)));
}

/// A key no field took while a voice message's volume is open: Up and
/// Down turn it 5 %, Page Up and Page Down 20 %, Escape closes it.
pub fn voice_key(
    app: &mut App,
    named: Option<iced::keyboard::key::Named>,
) -> Option<Task<Message>> {
    use iced::keyboard::key::Named;
    app.playback.voice_volume.as_ref()?;
    let by = match named? {
        Named::ArrowUp => VOICE_TURN,
        Named::ArrowDown => -VOICE_TURN,
        Named::PageUp => VOICE_PAGE,
        Named::PageDown => -VOICE_PAGE,
        Named::Escape => {
            app.playback.voice_volume = None;
            return Some(Task::none());
        }
        _ => return None,
    };
    turn_voice(app, by);
    Some(app.sync())
}

/// What a video's cell asked for.
pub fn clip_event(app: &mut App, clip: Clip) -> Task<Message> {
    let video = app.client.state().video.as_ref();
    match clip {
        Clip::Play(item, index, size) => {
            // In the window's pixels.
            let scale = app.device_scale();
            let pixels = |side: f32| ((side * scale).round() as u32).max(1);
            app.playback.clip_time = None;
            // A slider gone with the last video's controls never said the
            // pointer left it.
            app.playback.clip_volume = 0;
            app.playback.clip_cell = (pixels(size.width), pixels(size.height));
            app.client.dispatch(Action::PlayVideo {
                item,
                index,
                width: pixels(size.width),
                height: pixels(size.height),
            });
        }
        Clip::Toggle => {
            let command = if video.is_some_and(|video| video.playing) {
                VideoCommand::Pause
            } else {
                VideoCommand::Play
            };
            app.client.dispatch(Action::Video(command));
        }
        Clip::Seek(at) => {
            let Some(duration) = video.and_then(|video| video.duration_ms) else {
                return Task::none();
            };
            let at = (f64::from(at.clamp(0.0, 1.0)) * duration as f64) as u64;
            app.client.dispatch(Action::Video(VideoCommand::Seek(at)));
        }
        Clip::Mute(muted) => app
            .client
            .dispatch(Action::Video(VideoCommand::Mute(muted))),
        Clip::Volume(volume) => app
            .client
            .dispatch(Action::Video(VideoCommand::Volume(volume.clamp(0.0, 1.0)))),
        Clip::Turn(steps) => {
            let Some(video) = video.filter(|_| steps != 0.0) else {
                return Task::none();
            };
            let shown = if video.muted { 0.0 } else { video.volume };
            let turned = shown + steps * VOICE_TURN;
            app.client
                .dispatch(Action::Video(VideoCommand::Volume(turned.clamp(0.0, 1.0))));
        }
        Clip::VolumeHover(entered) => {
            app.playback.clip_volume = if entered {
                app.playback.clip_volume.saturating_add(1)
            } else {
                app.playback.clip_volume.saturating_sub(1)
            };
            return Task::none();
        }
        // Full screen, its pictures come at the viewer's size; back in
        // its cell, at the cell's.
        Clip::Full(full) => {
            app.playback.clip_full = full && video.is_some();
            app.playback.clip_volume = 0;
            let (width, height) = if app.playback.clip_full {
                let size = crate::viewer::fitted(app.window, video.and_then(|video| video.size));
                let scale = app.device_scale();
                ((size.width * scale) as u32, (size.height * scale) as u32)
            } else {
                app.playback.clip_cell
            };
            app.client
                .dispatch(Action::Video(VideoCommand::Size { width, height }));
            // Full screen keeps the keys from the fields under it.
            return Task::batch([app.sync(), crate::composer::refocus(app)]);
        }
        Clip::Point(key, entered) => {
            crate::app::point(&mut app.pointed.clip, key, entered);
            return Task::none();
        }
    }
    app.sync()
}

/// A key while a message's video is full screen: Escape brings it back
/// to its cell, Space plays or pauses it, Left and Right move it five
/// seconds.
pub fn clip_key(app: &mut App, named: Option<iced::keyboard::key::Named>) -> Task<Message> {
    use iced::keyboard::key::Named;
    let clip = match named {
        Some(Named::Escape) => Clip::Full(false),
        Some(Named::Space) => Clip::Toggle,
        Some(step @ (Named::ArrowLeft | Named::ArrowRight)) => {
            let Some((duration, ended)) = app
                .client
                .state()
                .video
                .as_ref()
                .and_then(|video| Some((video.duration_ms?, video.ended)))
                .filter(|(duration, _)| *duration > 0)
            else {
                return Task::none();
            };
            let at = if ended {
                duration
            } else {
                app.playback.clip_time.unwrap_or(0)
            };
            let to = if step == Named::ArrowLeft {
                at.saturating_sub(SKIP_MS)
            } else {
                at.saturating_add(SKIP_MS).min(duration)
            };
            Clip::Seek(to as f32 / duration as f32)
        }
        _ => return Task::none(),
    };
    clip_event(app, clip)
}

/// How far Left and Right move a video full screen.
const SKIP_MS: u64 = 5000;

/// How far a touchpad moves for one of a wheel's steps.
const STEP_PIXELS: f32 = 60.0;

/// A wheel's steps up, or down below zero: a touchpad's pixels as the
/// part of a step they are, and none for a turn sideways.
fn turned(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y / STEP_PIXELS,
    }
}

/// A voice message's card's height, with the room above it.
pub const VOICE_CARD: f32 = PILL + 4.0;

/// A voice message's card.
pub fn voice_card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    voice: &'a VoiceMessage,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let state = ctx.state;
    let item = &message.item;
    let playback = state
        .voice_message
        .as_ref()
        .filter(|playback| playback.item == *item);
    let playing = playback.is_some_and(|playback| playback.playing);
    let enabled = !state.preview && message.delivery == yapfast_core::chat::Delivery::default();
    let played = playback.map_or(0.0, |playback| {
        if voice.duration_ms == 0 {
            0.0
        } else {
            (playback.position_ms as f32 / voice.duration_ms as f32).min(1.0)
        }
    });
    // The accent shading towards the bottom right.
    let accent = theme.accent;
    let light = crate::theme::mix(Color::WHITE, accent, 0.2);
    let face = container(ctx.look.glyph(
        if playing { Icon::Pause } else { Icon::Play },
        17,
        Color::WHITE,
    ))
    .center(BUTTON);
    let play = button(face)
        .padding(0)
        .width(BUTTON)
        .height(BUTTON)
        .on_press_maybe(enabled.then(|| Message::Voice(Event::Toggle(item.clone()))))
        .style(move |_, status| {
            let opacity = match (enabled, lights(status)) {
                (false, _) => 0.5,
                (true, true) => 0.9,
                (true, false) => 1.0,
            };
            button::Style {
                background: Some(Background::Gradient(
                    gradient::Linear::new(std::f32::consts::FRAC_PI_4 * 3.0)
                        .add_stop(0.0, faded(light, opacity))
                        .add_stop(1.0, faded(accent, opacity))
                        .into(),
                )),
                border: crate::style::round(BUTTON / 2.0),
                text_color: Color::WHITE,
                ..button::Style::default()
            }
        });
    let mut wave = row![].spacing(BAR_PITCH - BAR).align_y(Alignment::Center);
    for (index, level) in bars(&voice.waveform).enumerate() {
        let ink = if (index as f32 + 0.5) / BARS as f32 <= played {
            theme.text
        } else {
            theme.muted
        };
        wave = wave.push(
            container(Space::new())
                .width(BAR)
                .height((level * BAR_TALL).max(BAR))
                .style(move |_| rounded(ink, BAR / 2.0)),
        );
    }
    let wave = container(wave).height(PILL).align_y(Alignment::Center);
    let wave: Element<'a, Message> = if enabled {
        let (seek, released) = (item.clone(), item.clone());
        Scrub::new(wave, move |at| {
            Message::Voice(Event::Seek(seek.clone(), at))
        })
        .released(Message::Voice(Event::Released(released)))
        .into()
    } else {
        wave.into()
    };
    let error = playback.and_then(|playback| playback.error.as_deref());
    let started = playback.is_some_and(|playback| playback.playing || playback.position_ms > 0);
    let (time, ink) = match (error, playback) {
        (Some(error), _) => (error.to_owned(), theme.danger),
        (None, Some(playback)) if started => (
            voice_time(voice.duration_ms.saturating_sub(playback.position_ms)),
            theme.muted,
        ),
        _ => (voice_time(voice.duration_ms), theme.muted),
    };
    // As wide as its widest time, so the card keeps its width as the
    // time counts down.
    let widest = voice_time(voice.duration_ms).len() as f32 * 8.6;
    let mut card = row![
        play,
        wave,
        container(text(time).size(14).color(ink))
            .width(if error.is_some() {
                Length::Shrink
            } else {
                Length::Fixed(widest)
            })
            .height(PILL)
            .align_y(Alignment::Center),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    // Speed and mute, while the pointer is on it or its volume; always
    // with animations off, as nothing would slide them out.
    let volume_open = ctx
        .playback
        .voice_volume
        .as_ref()
        .is_some_and(|(open, count)| open == item && *count > 0);
    if ctx.pointed.voice.as_ref() == Some(item)
        || volume_open
        || ctx.layout.prefs.animate == crate::layout::Animate::Never
    {
        let tuning = state.voice_tuning;
        let (rest, lit, text_ink) = (theme.raised, theme.hover, theme.text);
        let speed = button(
            container(text(format!("{}x", tuning.speed)).size(12).font(BOLD))
                .padding([0, 6])
                .height(22)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(Message::Voice(Event::Speed))
        .style(move |_, status| button::Style {
            background: Some(if lights(status) { lit } else { rest }.into()),
            text_color: text_ink,
            border: crate::style::round(11),
            ..button::Style::default()
        });
        let mute = ctx.look.icon_button(
            if tuning.muted {
                Icon::VolumeOff
            } else {
                Icon::Volume
            },
            match (volume_open, tuning.muted) {
                (true, _) => "",
                (false, true) => "Unmute",
                (false, false) => "Mute",
            },
            Message::Voice(Event::Mute(!tuning.muted)),
        );
        // The pointer on the mute opens the volume over it, upright as
        // Discord's desktop player has it, centred on the button.
        let slot = container(mute).center_x(MUTE);
        let slot: Element<'a, Message> = if volume_open {
            container(iced::widget::stack![
                slot,
                crate::widgets::Corner::new(
                    voice_volume_slider(ctx, item, tuning),
                    (MUTE - VOICE_SLOT) / 2.0,
                    -(VOICE_SLIDER + VOICE_GAP)
                )
                .free(),
            ])
            .width(MUTE)
            .into()
        } else {
            slot.into()
        };
        let mute = mouse_area(slot)
            .on_enter(Message::Voice(Event::VolumeHover(item.clone(), true)))
            .on_exit(Message::Voice(Event::VolumeHover(item.clone(), false)));
        card = card.push(row![speed, mute].spacing(4).align_y(Alignment::Center));
    }
    let ground = theme.sidebar;
    let pill = container(card)
        .padding(Padding {
            left: 8.0,
            right: 16.0,
            ..Padding::ZERO
        })
        .height(PILL)
        .style(move |_| rounded(ground, PILL / 2.0));
    container(
        mouse_area(pill)
            .on_enter(Message::Voice(Event::Point(item.clone(), true)))
            .on_exit(Message::Voice(Event::Point(item.clone(), false))),
    )
    .padding(Padding::ZERO.top(4))
    .into()
}

/// A voice message's volume, a popup over its mute: an upright slider
/// on the raised ground with its edge and shadow; the wheel on it turns
/// it 5 % a step.
fn voice_volume_slider<'a>(
    ctx: crate::ctx::Ctx<'a>,
    item: &str,
    tuning: yapfast_core::chat::VoiceTuning,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let shown = if tuning.muted { 0.0 } else { tuning.volume };
    let (fill, rest, knob) = (theme.accent, theme.line, theme.text);
    let slider = iced::widget::vertical_slider(0.0..=1.0, shown, |volume| {
        Message::Voice(Event::Volume(volume))
    })
    .step(0.01)
    .width(VOICE_SLOT)
    .style(move |_, _| iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (fill.into(), rest.into()),
            width: TRACK,
            border: crate::style::round(TRACK / 2.0),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 6.0 },
            background: knob.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    });
    let (ground, edge) = (theme.raised, theme.edge);
    let boxed = container(slider)
        .padding([6, 0])
        .width(VOICE_SLOT)
        .height(VOICE_SLIDER)
        .style(move |_| crate::style::panel(ground, edge, 8.0));
    // Down to the button, so the pointer stays on it on the way.
    let reach = container(boxed).padding(Padding::ZERO.bottom(VOICE_GAP));
    mouse_area(reach)
        .on_enter(Message::Voice(Event::VolumeHover(item.to_owned(), true)))
        .on_exit(Message::Voice(Event::VolumeHover(item.to_owned(), false)))
        .on_scroll(|delta| Message::Voice(Event::Turn(turned(delta))))
        .into()
}

/// A message's video in its gallery cell: `poster` with a play mark
/// until it plays, then its frames with the controls.
pub fn clip_cell<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    index: usize,
    size: Size,
    poster: Element<'a, Message>,
) -> Element<'a, Message> {
    let state = ctx.state;
    let key = format!("{}/{index}", message.item);
    let playing = state
        .video
        .as_ref()
        .filter(|video| video.item == message.item && video.index == index);
    let failed = playing.and_then(|video| video.error.as_deref());
    let pointed = ctx.pointed.clip.as_deref() == Some(key.as_str());
    let source = format!("clip:{key}");
    let mut layers = stack![poster].width(size.width).height(size.height);
    if failed.is_none() && playing.is_some() && ctx.video.has(&source) {
        layers = layers.push(Video::new(ctx.video, source, Color::BLACK).scale(ctx.scale));
    }
    let ended = playing.is_some_and(|video| video.ended);
    if playing.is_none() || failed.is_some() || (ended && !pointed) {
        layers = layers
            .push(container(crate::message::attachments::play_badge(ctx)).center(Length::Fill));
    }
    if let Some(video) = playing {
        if let Some(why) = failed {
            layers = layers.push(
                container(
                    container(
                        text(why)
                            .size(13)
                            .color(Color::WHITE)
                            .wrapping(text::Wrapping::None),
                    )
                    .padding([0, 10])
                    .height(28)
                    .align_y(Alignment::Center)
                    .clip(true)
                    .style(|_| rounded(CLIP_SHADE, 6)),
                )
                .padding(8)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::End),
            );
        } else if video.loading {
            layers = layers.push(
                container(
                    container(text("Loading...").size(13).color(Color::WHITE))
                        .padding([0, 10])
                        .height(28)
                        .align_y(Alignment::Center)
                        .style(|_| rounded(crate::style::SMOKE, 14)),
                )
                .center(Length::Fill),
            );
        } else if (pointed || !video.playing || ctx.playback.clip_volume > 0)
            && !ctx.playback.clip_full
        {
            // The volume's slider may rise past a short cell's top: the
            // pointer on it keeps the controls up.
            layers = layers.push(
                container(clip_controls(ctx, video, size.width, false))
                    .height(Length::Fill)
                    .align_y(Alignment::End),
            );
        }
    }
    // A click plays it, pauses it, or plays it on; the controls take
    // their own.
    let press = match playing {
        Some(_) if failed.is_none() => Message::Clip(Clip::Toggle),
        _ => Message::Clip(Clip::Play(message.item.clone(), index, size)),
    };
    mouse_area(layers)
        .on_press(press)
        .on_enter(Message::Clip(Clip::Point(key.clone(), true)))
        .on_exit(Message::Clip(Clip::Point(key, false)))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// The bar along a playing video's foot: play or pause, its time, the
/// track to move through it, mute with a slider over it for how loud,
/// and full screen. A narrow video leaves out the time, then mute. Full
/// screen, it's under the video, on no shade.
pub fn clip_controls<'a>(
    ctx: crate::ctx::Ctx<'a>,
    video: &'a yapfast_core::chat::Video,
    width: f32,
    full: bool,
) -> Element<'a, Message> {
    let at = ctx.playback.clip_time.unwrap_or(0);
    let at = match video.duration_ms {
        Some(duration) if video.ended => duration,
        _ => at,
    };
    let progress = match video.duration_ms {
        Some(duration) if duration > 0 => (at as f32 / duration as f32).min(1.0),
        _ => 0.0,
    };
    let time = video_time(at, video.duration_ms);
    // Play, full screen, the padding and the spacing; the track keeps
    // 32 px.
    let fixed = 4.0 + CLIP_BUTTON + 6.0 + 32.0 + 6.0 + CLIP_BUTTON + 6.0;
    let show_mute = video.sound && width >= fixed + 34.0;
    let show_time =
        width >= fixed + if show_mute { 34.0 } else { 0.0 } + time.len() as f32 * 7.0 + 6.0;
    let button = |icon, label, message| {
        ctx.look.face_button(
            ctx.look.glyph(icon, 16, Color::WHITE),
            CLIP_BUTTON,
            label,
            message,
        )
    };
    let mut bar = row![if video.playing {
        button(Icon::Pause, "Pause", Message::Clip(Clip::Toggle))
    } else {
        button(Icon::Play, "Play", Message::Clip(Clip::Toggle))
    }]
    .spacing(6)
    .padding(Padding {
        left: 4.0,
        right: 6.0,
        ..Padding::ZERO
    })
    .height(CLIP_BAR)
    .align_y(Alignment::Center);
    if show_time {
        bar = bar.push(text(time).size(12).color(Color::WHITE));
    }
    let filled = row![
        container(Space::new().height(TRACK))
            .width(Length::FillPortion(
                ((progress * 1000.0).round() as u16).max(1)
            ))
            .style(|_| rounded(Color::WHITE, TRACK / 2.0)),
        Space::new().width(Length::FillPortion(
            ((1000.0 - progress * 1000.0).round() as u16).max(1)
        )),
    ];
    let track = stack![
        container(Space::new().width(Length::Fill).height(TRACK))
            .style(|_| rounded(TRACK_REST, TRACK / 2.0)),
        filled,
    ];
    bar = bar.push(Scrub::new(
        container(track)
            .height(CLIP_BAR)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        |at| Message::Clip(Clip::Seek(at)),
    ));
    if show_mute {
        // Its tip would cover the slider over it while that's open.
        let open = ctx.playback.clip_volume > 0;
        let mute = if video.muted {
            button(
                Icon::VolumeOff,
                if open { "" } else { "Unmute" },
                Message::Clip(Clip::Mute(false)),
            )
        } else {
            button(
                Icon::Volume,
                if open { "" } else { "Mute" },
                Message::Clip(Clip::Mute(true)),
            )
        };
        let mute: Element<'a, Message> = if ctx.playback.clip_volume > 0 {
            iced::widget::stack![
                mute,
                crate::widgets::Corner::new(volume_slider(video), 0.0, -VOLUME_RISE),
            ]
            .into()
        } else {
            mute.into()
        };
        bar = bar.push(
            mouse_area(container(mute).width(CLIP_BUTTON))
                .on_enter(Message::Clip(Clip::VolumeHover(true)))
                .on_exit(Message::Clip(Clip::VolumeHover(false)))
                .on_scroll(|delta| Message::Clip(Clip::Turn(turned(delta)))),
        );
    }
    let (icon, label) = if full {
        (Icon::Minimize, "Exit full screen")
    } else {
        (Icon::Maximize, "Full screen")
    };
    bar = bar.push(button(icon, label, Message::Clip(Clip::Full(!full))));
    // Between the controls, a click leaves the video be.
    mouse_area(container(bar).width(Length::Fill).style(move |_| {
        if full {
            return iced::widget::container::Style::default();
        }
        iced::widget::container::Style {
            background: Some(Background::Gradient(
                gradient::Linear::new(0.0)
                    .add_stop(0.0, CLIP_SHADE)
                    .add_stop(1.0, Color::TRANSPARENT)
                    .into(),
            )),
            ..iced::widget::container::Style::default()
        }
    }))
    .on_press(Message::Nothing)
    .interaction(iced::mouse::Interaction::Idle)
    .into()
}

/// How loud the video plays, a slider over its mute: a 4 px track
/// filled in white to a 12 px knob, on dark glass.
fn volume_slider<'a>(video: &'a yapfast_core::chat::Video) -> Element<'a, Message> {
    let shown = if video.muted { 0.0 } else { video.volume };
    let slider = iced::widget::vertical_slider(0.0..=1.0, shown, |volume| {
        Message::Clip(Clip::Volume(volume))
    })
    .step(0.01)
    .width(CLIP_BUTTON)
    .style(|_, _| iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (Color::WHITE.into(), TRACK_REST.into()),
            width: TRACK,
            border: crate::style::round(TRACK / 2.0),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 6.0 },
            background: Color::WHITE.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    });
    let boxed = container(slider)
        .padding([14, 0])
        .width(CLIP_BUTTON)
        .height(VOLUME_RISE - VOLUME_GAP)
        .style(|_| rounded(CLIP_SHADE, 6));
    // Down to the button, so the pointer stays on it on the way.
    let reach = container(boxed).padding(Padding::ZERO.bottom(VOLUME_GAP));
    mouse_area(reach)
        .on_enter(Message::Clip(Clip::VolumeHover(true)))
        .on_exit(Message::Clip(Clip::VolumeHover(false)))
        .on_scroll(|delta| Message::Clip(Clip::Turn(turned(delta))))
        .into()
}
