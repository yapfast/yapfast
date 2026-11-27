//! A call's stage: this account's shared screen first, then the streams of
//! those who film as well in tiles of their own, then everyone. A click on
//! a tile shows it large, with the others in rows beneath, until a click
//! beside it; the large one goes full screen. Under the stage, the call's
//! controls; over it, a line on how the call is. A voice channel this
//! account isn't in shows its Join instead.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size, Vector,
    widget::{Space, button, column, container, mouse_area, pin, row, stack, text},
    window,
};
use yapfast_core::voice::{self, SharePhase};

use super::{Event, Popup, voice_ready};
use crate::{
    app::{App, Message},
    controls::{self, ROUND, ROUND_GLYPH, bar},
    header,
    pictures::Icon,
    style::{faded, fill, hex, rounded},
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
/// What a tile's name chip takes at its foot, with its margin.
const CHIP_ROOM: f32 = 40.0;
/// How wide the Watch stream pill draws, its 13 px bold words and padding.
const WATCH_PILL: f32 = 112.0;

/// About how wide a tile's name chip draws: 14 px bold, a mark's 20 px each,
/// LIVE's 44, and its padding. Enough to tell whether Watch stream fits
/// beside it.
fn chip_width(name: &str, marks: usize, live: bool) -> f32 {
    16.0 + name.chars().count() as f32 * 8.5 + marks as f32 * 20.0 + if live { 44.0 } else { 0.0 }
}

/// The controls' row under the stage: 8 px over the buttons, 16 under.
pub(crate) const CONTROLS_HEIGHT: f32 = 8.0 + ROUND + 16.0;

/// How large the call's controls are drawn: under the stage at the call's
/// own size, or in a picture's window of its own a size down, as that
/// window's buttons are.
#[derive(Clone, Copy)]
pub(crate) struct ControlsSize {
    side: f32,
    glyph: u32,
    /// The leave button's width, its pill.
    leave: f32,
    /// How far the controls stand above the foot they hang from.
    bottom: f32,
}

impl ControlsSize {
    /// The stage's own, under it.
    const CALL: Self = Self {
        side: ROUND,
        glyph: ROUND_GLYPH,
        leave: 76.0,
        bottom: 16.0,
    };
    /// A picture's window's, under its picture and the members' strip.
    pub(crate) const WINDOW: Self = Self {
        side: crate::call::popout::BUTTON,
        glyph: crate::call::popout::BUTTON_GLYPH,
        leave: 64.0,
        // As far in as the window's own buttons at its corner.
        bottom: crate::call::popout::CORNER,
    };

    /// The room the controls take under the stage.
    fn height(self) -> f32 {
        8.0 + self.side + self.bottom
    }
}
/// The Hide/Show Members capsule (`60 × 32`), 8 px into the strip's top
/// when shown, and 6 px above the bottom of the stage (14 px over the
/// controls' buttons) when hidden.
pub const MEMBERS_TOGGLE: &str = "stage-members-toggle";
const MEMBERS_WIDTH: f32 = 60.0;
const MEMBERS_HEIGHT: f32 = 32.0;
const MEMBERS_GLYPH: u32 = 24;
const MEMBERS_STRIP_OFFSET: f32 = 8.0;
const MEMBERS_BOTTOM_GAP: f32 = 6.0;
/// How wide a tile of the members strip in a picture's own window is: the
/// widest a tile of the stage's own strip gets, so the two read alike — and
/// how much air the strip leaves over and under it there.
const MEMBERS_TILE: f32 = 192.0;
const MEMBERS_ROOM: f32 = 8.0;

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
    voice_selected(app) && !app.client.state().voice_chat && !crate::call::in_window(app)
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
    // Banners over the call (the preview's line, a notice) take from its
    // height, so the size it was last laid out at wins over the sum below.
    if let Some(seen) = app.call.stage_seen.get() {
        return seen;
    }
    // A theme's floating panes stand `gap` in from the window's top and
    // foot.
    let below_header =
        app.window.height - header::HEIGHT - 1.0 - 2.0 * app.themes.glass_gap(&app.theme.id);
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
    stage_in(app, stage_size(app))
}

/// The same on a stage of `size`.
fn stage_in(app: &App, size: Size) -> (Vec<Slot>, Option<usize>, Vec<Rectangle>, f32) {
    let voice = &app.client.state().voice;
    let stage = Stage::of(voice, popped_key(app));
    let slots = stage.slots();
    let focused = app.call.focus.as_ref().and_then(|focus| stage.slot(focus));
    let full = full_shown(app);
    let (tiles, content) = tiles(
        slots.len(),
        focused,
        size,
        full,
        !full && crate::call::members_hidden(app),
    );
    (slots, focused, tiles, content)
}

/// What the corner picture shows while the call is off screen.
pub fn corner(app: &App) -> Pip {
    let voice = &app.client.state().voice;
    if crate::call::in_window(app)
        || app.shell.window.is_none()
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
    Stage::of(voice, popped_key(app)).pip(app.call.focus.as_ref(), watching)
}

/// The corner's tile, by its place on the stage.
pub fn corner_place(app: &App) -> Option<usize> {
    let voice = &app.client.state().voice;
    let watching = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    Stage::of(voice, popped_key(app)).pip_slot(corner(app), watching)
}

/// A tile shown large that's gone (its stream ended, they left) shows
/// none.
pub fn keep_focus(app: &mut App) {
    let voice = &app.client.state().voice;
    let stage = Stage::of(voice, popped_key(app));
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

/// The picture a window of its own shows, by its key, or none.
fn popped_key(app: &App) -> Option<&str> {
    app.call.popout.as_ref().map(|popout| popout.key.as_str())
}

/// The room a picture's own window gives its picture, in the window's pixels:
/// the size the frames it shows are made at.
fn popout_pixels(app: &App) -> (u32, u32) {
    let scale = app.device_scale();
    let size = app.call.popout_seen.get().unwrap_or(Size::new(
        crate::call::popout::WIDE,
        crate::call::popout::WIDE * 9.0 / 16.0,
    ));
    (
        (size.width * scale).round().max(2.0) as u32,
        (size.height * scale).round().max(2.0) as u32,
    )
}

/// Whether a picture's own window shows `key`, so the stage leaves it out
/// and its frames go there, and only there.
pub fn popped_out(app: &App, key: &str) -> bool {
    popped_key(app) == Some(key)
}

/// Tells each camera where its tile shows, or that it doesn't, so frames
/// are made only for tiles in sight, at their size, and whether it's
/// large; and has [`video::frames`] look for frames from those that
/// show: cameras, the stream watched, the screen shared, a message's
/// video. With the call off screen, the corner's shows there; a picture's own
/// window takes what it shows, and only it.
pub fn place_videos(app: &mut App) {
    // Its tiles show in Yapfast's window, or in the strip under a picture's
    // window of its own.
    let shown = stage_shown(app) || crate::call::in_window(app);
    let (slots, focused, tiles, content) = stage(app);
    let height = stage_size(app).height;
    let scrolled = if content <= height + 0.5 {
        0.0
    } else {
        app.call.scrolled
    };
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
    let corner_pixels = {
        let size = crate::call::corner::corner_size(app);
        (
            (size.width * scale).round() as u32,
            (size.height * scale).round() as u32,
        )
    };
    for (place, slot) in slots.iter().enumerate() {
        let Slot::Person(index) = *slot else {
            continue;
        };
        let person = &voice.participants[index];
        let Some(feed) = &person.video else {
            continue;
        };
        let in_corner = corner == Pip::Camera(index);
        // A camera the window of its own shows is made at that window's
        // size; the stage's tile keeps their picture instead.
        let at = if popped_out(app, &person.id) {
            Some(popout_pixels(app))
        } else if in_corner {
            Some(corner_pixels)
        } else {
            shown
                .then(|| shown_at(tiles[place], scrolled, height, scale))
                .flatten()
        };
        feed.show_at(at);
        feed.show_large(shown && !in_corner && focused == Some(place));
        if at.is_some() {
            sources.feeds.push((person.id.clone(), feed.clone()));
        }
    }
    if let Some(watching) = &voice.watching
        && let Some(frames) = &watching.frames
        && (shown
            || corner == Pip::Watched
            || popped_out(app, &format!("stream:{}", watching.user)))
    {
        sources
            .mapped
            .push((format!("stream:{}", watching.user), frames.clone()));
    }
    if let Some(sharing) = &voice.sharing
        && (shown || corner == Pip::Shared || popped_out(app, OWN_SCREEN))
    {
        sources
            .mapped
            .push((OWN_SCREEN.to_owned(), sharing.frames.clone()));
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
    app.call.focus = place.and_then(|place| Stage::of(voice, popped_key(app)).focus_at(place));
    // Another picture, shown as it comes.
    app.call.zoom = crate::call::Zoom::default();
    if app.call.focus.is_none() {
        app.call.full = false;
    }
    place_videos(app);
}

/// A frame by its key, fitted into its tile with `corners` cut round
/// on `behind` (none full screen), its bars and its ground in black.
pub fn frame<'a>(
    app: &'a App,
    key: &str,
    corners: Option<(u32, u32)>,
) -> Option<Element<'a, Message>> {
    frame_in(app, key, corners, Color::BLACK)
}

/// The same as the wheel over it last left it, its bars and its ground in
/// `ground`.
pub fn frame_zoom<'a>(
    app: &'a App,
    key: &str,
    corners: Option<(u32, u32)>,
    ground: Color,
) -> Option<Element<'a, Message>> {
    frame_at(app, key, corners, ground, Some(app.call.zoom))
}

/// The same as the wheel over it leaves it: a picture shown on its own comes
/// closer about the pointer, up a step a notch, and goes back down.
pub(crate) fn wheeled<'a>(picture: Element<'a, Message>) -> Element<'a, Message> {
    crate::widgets::Turn::new(picture, |turn, at| Message::Call(Event::Zoom(turn, at))).into()
}

/// The zoom bar's own: how wide its track draws, how tall the bar is, its
/// magnifiers across and their glyph.
pub(crate) const ZOOM_TRACK: f32 = 72.0;
pub(crate) const ZOOM_BAR: f32 = 30.0;
/// Its own id, which a test finds it by.
pub(crate) const ZOOM_BAR_ID: &str = "call-zoom-bar";
const ZOOM_BUTTON: f32 = 24.0;
const ZOOM_GLYPH: u32 = 14;

/// The zoom bar over a picture shown on its own, at its bottom left corner:
/// the magnifiers letting it back and bringing it closer, and the track
/// between them, at the share of the way it's come (`Event::ZoomTo`). Only
/// while it has come closer at all: fitted, there's nothing to say and
/// nothing to take back.
pub(crate) fn zoom_bar(app: &App) -> Option<Element<'_, Message>> {
    if app.call.zoom.by <= 1.0 {
        return None;
    }
    let theme = &app.theme;
    let by = app.call.zoom.by.clamp(1.0, ZOOM_MOST);
    let percent = (((by - 1.0) / (ZOOM_MOST - 1.0)) * 100.0).round() as u32;
    let (field, accent, knob) = (theme.field, theme.accent, theme.text);
    let track = iced::widget::slider(0..=100, percent, |value: u32| {
        Message::Call(Event::ZoomTo(
            1.0 + (ZOOM_MOST - 1.0) * value as f32 / 100.0,
        ))
    })
    .step(1u32)
    .width(ZOOM_TRACK)
    .height(14)
    .style(move |_, _| iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (accent.into(), field.into()),
            width: 3.0,
            border: iced::Border::default().rounded(2),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 5.0 },
            background: knob.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    });
    let magnifier = |icon: Icon, label: &'static str, by: f32| {
        let face = app.look().glyph(icon, ZOOM_GLYPH, theme.text);
        controls::round_button_of(
            face,
            label,
            theme.token_colour("card-button", GLASS),
            ZOOM_BUTTON,
            theme,
            Some(Message::Call(Event::ZoomTo(by))),
        )
    };
    Some(
        container(
            row![
                magnifier(Icon::ZoomOut, "Zoom out", by / ZOOM_STEP),
                track,
                magnifier(Icon::ZoomIn, "Zoom in", by * ZOOM_STEP),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 3.0,
            right: 4.0,
            bottom: 3.0,
            left: 4.0,
        })
        .id(iced::widget::Id::new(ZOOM_BAR_ID))
        .style(move |_| rounded(GLASS, 15))
        .into(),
    )
}

/// The picture drawn in `room`: fitted to it and in its middle, as
/// [`video`] draws its frames, or the room whole while no frame has come.
pub(crate) fn fitted_in(app: &App, key: &str, room: Size) -> Size {
    let Some((width, height)) = app.video.size(key) else {
        return room;
    };
    let (width, height) = (width.max(1) as f32, height.max(1) as f32);
    let scale = (room.width / width).min(room.height / height);
    Size::new(
        (width * scale).min(room.width),
        (height * scale).min(room.height),
    )
}

/// How much closer a notch of the wheel brings the picture, and how close it
/// comes at most.
const ZOOM_STEP: f32 = 1.15;
const ZOOM_MOST: f32 = 4.0;

/// The wheel over a picture on its own: turned up it comes closer about the
/// pointer, down it goes back, and its room's size again at the first, its
/// middle kept over its room.
pub(crate) fn turn_zoom(app: &mut App, turn: f32, at: Point) {
    let Some((room, fitted)) = app.call.zoom_seen.get() else {
        return;
    };
    // Nothing of a picture shows in no room at all.
    if room.width < 1.0 || room.height < 1.0 {
        return;
    }
    let by = app.call.zoom.by * ZOOM_STEP.powf(turn);
    // Where the pointer stands on the picture, as its share of it, from the
    // middle, which the turn keeps it over.
    let off = at - Rectangle::new(Point::ORIGIN, room).center();
    let (share_x, share_y) = (
        (off.x - app.call.zoom.pan.x) / (fitted.width * app.call.zoom.by),
        (off.y - app.call.zoom.pan.y) / (fitted.height * app.call.zoom.by),
    );
    let middle = Vector::new(
        off.x - share_x * fitted.width * by,
        off.y - share_y * fitted.height * by,
    );
    set_zoom(app, by, middle);
}

/// The zoom bar's own: the picture comes `by` times its room, its middle kept
/// where it was while it's still larger than the room, and its room's size
/// again at the first.
pub(crate) fn zoom_to(app: &mut App, by: f32) {
    let was = app.call.zoom.by.max(1.0);
    let middle = app.call.zoom.pan * (by.clamp(1.0, ZOOM_MOST) / was);
    set_zoom(app, by, middle);
}

/// The picture as close as `by`, cut at its room with its middle `middle`
/// from the room's: never further than the room shows it whole, and its
/// room's size again at the first.
fn set_zoom(app: &mut App, by: f32, middle: Vector) {
    use crate::call::Zoom;
    let Some((room, fitted)) = app.call.zoom_seen.get() else {
        return;
    };
    let by = by.clamp(1.0, ZOOM_MOST);
    // Nothing of a picture shows in no room at all.
    if by <= 1.0 || room.width < 1.0 || room.height < 1.0 {
        app.call.zoom = Zoom::default();
        return;
    }
    // Its own box is what it fills, and never further than that.
    let (along, down) = (
        (fitted.width * (by - 1.0) / 2.0).max(0.0),
        (fitted.height * (by - 1.0) / 2.0).max(0.0),
    );
    app.call.zoom = Zoom {
        by,
        pan: Vector::new(middle.x.clamp(-along, along), middle.y.clamp(-down, down)),
    };
}

/// The same, its bars and its ground in `ground`: a picture standing in on
/// the ground it shows over takes that ground, so its margins don't read as
/// a box of their own.
pub fn frame_in<'a>(
    app: &'a App,
    key: &str,
    corners: Option<(u32, u32)>,
    ground: Color,
) -> Option<Element<'a, Message>> {
    frame_at(app, key, corners, ground, None)
}

/// The picture of `key` drawn with `corners` round on `behind`, at `zoom`
/// while it's shown on its own, else fitted as it comes.
fn frame_at<'a>(
    app: &'a App,
    key: &str,
    corners: Option<(u32, u32)>,
    ground: Color,
    zoom: Option<crate::call::Zoom>,
) -> Option<Element<'a, Message>> {
    app.video.has(key).then(|| {
        let video = Video::new(&app.video, key, ground).scale(app.device_scale());
        let video = match corners {
            Some((radius, behind)) => video.corners(radius, behind),
            None => video,
        };
        let video = match zoom {
            Some(zoom) => video.zoom(zoom.by, zoom.pan),
            None => video,
        };
        Element::from(video)
    })
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
    // As Slint: the header and the banners stay the conversation's; the
    // stage under them takes the theme's `call` ground, art and glass
    // (the stock stage paints its own only where it differs from the
    // chat's, as a ground under the video is drawn again every frame).
    let theme = &app.theme;
    let body = if app.theme.packaged() {
        crate::theme_art::surface(
            body,
            app.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Call,
        )
    } else {
        let stage = (theme.stage != theme.chat).then_some(theme.stage);
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| stage.map(fill).unwrap_or_default())
            .into()
    };
    // A theme's box frame round the conversation frames the call too, as
    // Slint's chat column holds the stage.
    let header = app.ctx().header;
    let box_name = if header.direct {
        header.name.clone()
    } else {
        format!("#{}", header.name)
    };
    let below = crate::theme_chrome::boxed(
        column![]
            .extend(crate::banners::view(app.ctx()))
            .push(body)
            .width(Length::Fill)
            .height(Length::Fill),
        app.ctx(),
        yapfast_core::themes::chrome::Pane::Chat,
        &box_name,
        None,
    );
    column![crate::header::view(app.ctx()), line]
        .push(below)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The call this account is in: the line over the stage, the stage
/// (unless it's full screen), and the controls.
pub fn call_panel(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let description = voice_description(app);
    let hint = [CALL_HINT, VOICE_HINT].contains(&description.as_str());
    let words: Element<'_, Message> = if hint && app.layout.hide_voice_hint {
        Space::new().width(Length::Fill).into()
    } else {
        let words = text(description).size(14).color(theme.muted);
        if hint {
            row![words, hint_close(app)]
                .spacing(4)
                .align_y(Alignment::Center)
                .into()
        } else {
            words.align_x(Alignment::Center).width(Length::Fill).into()
        }
    };
    let line = container(words)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .padding(Padding {
            top: 16.0,
            left: 16.0,
            right: 16.0,
            bottom: 0.0,
        })
        .height(CONTROLS - CONTROLS_HEIGHT);
    // The stage takes the height left between the line and the controls,
    // so the controls always sit under the tiles, whatever shows above.
    let stage: Element<'_, Message> = if full_shown(app) {
        Space::new().width(Length::Fill).height(Length::Fill).into()
    } else {
        stage_view(app)
    };
    // The stage takes what's left, so the controls always show (the
    // banners over the call take room the stage's size doesn't know).
    let stage = container(stage).height(Length::Fill).clip(true);
    // With the strip hidden, Show Members sits 6 px over the stage's foot,
    // whatever room the banners leave the stage.
    let stage: Element<'_, Message> = if has_strip(app) && crate::call::members_hidden(app) {
        stack![
            stage,
            container(members_toggle(app))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::End)
                .padding(Padding {
                    bottom: MEMBERS_BOTTOM_GAP,
                    ..Padding::ZERO
                })
        ]
        .into()
    } else {
        stage.into()
    };
    column![line, stage, call_controls(app, ControlsSize::CALL)]
        .height(Length::Fill)
        .into()
}

/// The line over a DM's call, and a voice channel's, once others are in
/// it; its close button hides it for good.
const CALL_HINT: &str = "The call stays connected while you browse other conversations.";
const VOICE_HINT: &str = "Voice stays connected while you browse other conversations.";

/// The hint's close button, which hides it for good.
fn hint_close(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let hover = theme.hover;
    let face = crate::widgets::Hover::new(
        app.look().glyph(Icon::X, 14, theme.muted).boxed(20),
        app.look().glyph(Icon::X, 14, theme.text).boxed(20),
    );
    let close = button(face)
        .padding(0)
        .on_press(Message::Call(Event::HideHint))
        .style(move |_, status| iced::widget::button::Style {
            background: controls::lights(status).then(|| hover.into()),
            border: crate::style::round(4),
            ..Default::default()
        });
    iced::widget::tooltip(
        close,
        controls::tip(theme, "Don't show again"),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
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
            CALL_HINT.into()
        };
    }
    if voice_here(app) {
        return if voice.participants.len() == 1 {
            "You're here alone. Stay as long as you like; others can join later.".into()
        } else {
            VOICE_HINT.into()
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
/// than it, with the Hide/Show Members toggle when a tile is focused
/// beside others.
pub fn stage_view(app: &App) -> Element<'_, Message> {
    if full_shown(app) {
        return stage_at(app, app.window);
    }
    iced::widget::responsive(move |size| {
        app.call.stage_seen.set(Some(size));
        stage_at(app, size)
    })
    .into()
}

/// The stage laid out at `size`, the room the window gives it.
fn stage_at(app: &App, size: Size) -> Element<'_, Message> {
    let full = full_shown(app);
    let (slots, focused, tiles, content) = stage_in(app, size);
    let has_strip = has_strip(app);
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
        let tile = stage_tile(
            app,
            place,
            *slot,
            *bounds,
            focused == Some(place),
            full,
            Press::Call,
        );
        layers = layers.push(pin(tile).x(bounds.x).y(bounds.y));
    }
    let toggle_x = ((size.width - MEMBERS_WIDTH) / 2.0).max(0.0);
    if has_strip
        && !crate::call::members_hidden(app)
        && let Some(strip_top) = tiles
            .iter()
            .enumerate()
            .find(|&(place, _)| focused != Some(place))
            .map(|(_, bounds)| bounds.y)
    {
        let y = (strip_top + MEMBERS_STRIP_OFFSET).clamp(0.0, (content - MEMBERS_HEIGHT).max(0.0));
        layers = layers.push(pin(members_toggle(app)).x(toggle_x).y(y));
    }
    let grid = layers.width(size.width).height(content);
    let panel: Element<'_, Message> = if content <= size.height + 0.5 {
        container(grid)
            .width(size.width)
            .height(size.height)
            .clip(true)
            .into()
    } else {
        scroll_view(grid)
            .width(size.width)
            .height(size.height)
            .on_scroll(|viewport| Message::Call(Event::Scrolled(viewport.absolute_offset().y)))
            .into()
    };
    panel
}

/// Whether the Hide/Show Members pill shows: a tile is large beside
/// others, not full screen.
fn has_strip(app: &App) -> bool {
    let (slots, focused, _, _) = stage(app);
    focused.is_some_and(|place| place < slots.len()) && !full_shown(app) && slots.len() > 1
}

/// The pill toggling the bottom participant strip under a large tile: a
/// chevron and the users icon on a 60×32 capsule, with its tooltip above.
pub(crate) fn members_toggle(app: &App) -> Element<'_, Message> {
    let theme = &app.theme;
    let (icon, label) = if crate::call::members_hidden(app) {
        (Icon::ChevronUp, "Show Members")
    } else {
        (Icon::ChevronDown, "Hide Members")
    };
    let (rest, lit) = if theme.bonfire {
        (hex(0x2c2d32), hex(0x38393b))
    } else {
        (theme.button_default, theme.button_default_hover)
    };
    let face = container(
        row![
            app.look().glyph(icon, MEMBERS_GLYPH, theme.text),
            app.look().glyph(Icon::Users, MEMBERS_GLYPH, theme.text),
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    )
    .id(iced::widget::Id::new(MEMBERS_TOGGLE))
    .width(MEMBERS_WIDTH)
    .height(MEMBERS_HEIGHT)
    .padding(Padding {
        left: 4.0,
        right: 8.0,
        ..Padding::ZERO
    })
    .align_y(Alignment::Center);
    let toggle = button(face)
        .padding(0)
        .width(MEMBERS_WIDTH)
        .height(MEMBERS_HEIGHT)
        .on_press(Message::Call(Event::ToggleMembers))
        .style(controls::lit_style(Some(rest), lit, MEMBERS_HEIGHT / 2.0));
    iced::widget::tooltip(
        toggle,
        controls::tip(theme, label),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

/// What a picture's own window puts in its members' strip: the call's tiles,
/// the one that window shows large left out, with their places on the stage.
fn strip_places(app: &App) -> Vec<(usize, Slot)> {
    let voice = &app.client.state().voice;
    let large = app.call.popout.as_ref().map(|popout| popout.key.as_str());
    Stage::of(voice, popped_key(app))
        .slots()
        .into_iter()
        .enumerate()
        .filter(|(_, slot)| {
            !large.is_some_and(|key| match *slot {
                Slot::OwnScreen => key == OWN_SCREEN,
                Slot::Stream(index) => {
                    key.strip_prefix("stream:") == Some(voice.participants[index].id.as_str())
                }
                Slot::Person(index) => key == voice.participants[index].id,
            })
        })
        .collect()
}

/// How tall a tile of the members' strip is.
fn members_tile_height() -> f32 {
    (MEMBERS_TILE * layout::ASPECT).round()
}

/// The call's members in a row, as a picture's own window shows them under
/// it: the stage's tiles, scrolled sideways while the window can't hold them
/// all; none while the call has no one else in it.
pub(crate) fn members_strip(app: &App) -> Option<Element<'_, Message>> {
    let places = strip_places(app);
    if places.is_empty() {
        return None;
    }
    let strip = Size::new(MEMBERS_TILE, members_tile_height());
    let window = app.call.popout.as_ref()?.id;
    let mut tiles = row![].spacing(layout::GAP).align_y(Alignment::Center);
    for (place, slot) in places {
        tiles = tiles.push(stage_tile(
            app,
            place,
            slot,
            Rectangle::new(Point::ORIGIN, strip),
            false,
            false,
            Press::Window(window),
        ));
    }
    // As wide as its tiles, so the middle of the window is where it starts,
    // and scrolled once they outgrow the window.
    let tiles = iced::widget::scrollable(tiles)
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0),
        ))
        .width(Length::Shrink)
        .height(Length::Fixed(strip.height));
    Some(
        container(tiles)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into(),
    )
}

/// The block a picture's own window puts under its picture: the Hide/Show
/// Members chevron, the call's members in a strip under it while `strip`, and
/// the call's controls under them — the chevron and the controls alone over
/// the whole window's picture, under the pointer.
pub(crate) fn members_block(app: &App, strip: bool) -> Element<'_, Message> {
    let mut block = column![].push(
        container(members_toggle(app))
            .width(Length::Fill)
            .align_x(Alignment::Center),
    );
    if strip && let Some(strip) = members_strip(app) {
        block = block.push(container(strip).width(Length::Fill).padding(Padding {
            top: MEMBERS_ROOM,
            bottom: MEMBERS_ROOM,
            ..Padding::ZERO
        }));
    }
    block.push(call_controls(app, ControlsSize::WINDOW)).into()
}

/// How tall [`members_block`] is: the chevron over the strip over the
/// controls, and the chevron over the controls while the call has nobody else
/// in it.
pub(crate) fn members_block_height(app: &App, strip: bool) -> f32 {
    ControlsSize::WINDOW.height()
        + MEMBERS_HEIGHT
        + if strip && !strip_places(app).is_empty() {
            2.0 * MEMBERS_ROOM + members_tile_height()
        } else {
            0.0
        }
}

/// Where a tile is drawn: the call's stage, or a picture's window of its own,
/// whose press shows that picture there.
#[derive(Clone, Copy)]
pub(crate) enum Press {
    Call,
    Window(window::Id),
}

/// One tile on the stage.
fn stage_tile(
    app: &App,
    place: usize,
    slot: Slot,
    bounds: Rectangle,
    large: bool,
    full: bool,
    press: Press,
) -> Element<'_, Message> {
    let theme = &app.theme;
    let voice = &app.client.state().voice;
    let size = bounds.size();
    // Full screen the tile is the window: what the pointer does elsewhere
    // is nothing to say, and its buttons stand over it, the pointer or not.
    let pointed = full || app.call.pointed == Some(place);
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
    // A picture shown large is the wheel's to bring closer, and the room it
    // stands in is kept for the turn about the pointer; on a tile of the grid
    // it comes as it is.
    let picture = |key: &str| -> Option<Element<'_, Message>> {
        if !large {
            return frame(app, key, corners);
        }
        app.call
            .zoom_seen
            .set(Some((size, fitted_in(app, key, size))));
        frame_zoom(app, key, corners, Color::BLACK).map(wheeled)
    };
    let mut tile = match slot {
        Slot::OwnScreen => {
            let sharing = voice.sharing.as_ref();
            let live = sharing.is_some_and(|sharing| sharing.phase == SharePhase::Live);
            let status = share_status(voice);
            let framed = picture(OWN_SCREEN);
            let note = (!live && framed.is_some()).then_some(status);
            viewers = sharing.and_then(|sharing| stream_viewers(app, &sharing.viewers, None));
            let content =
                framed.unwrap_or_else(|| tile::note(app.look(), Some(Icon::ScreenShare), status));
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
                picture(&format!("stream:{}", person.id))
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
            // it. A window of its own shows it instead, and their tile
            // keeps their picture.
            let streams_here = Stage::of(voice, popped_key(app)).stream_in_tile(person);
            let watched = streams_here && watch_user == Some(person.id.as_str());
            let filmed = person
                .video
                .as_ref()
                .filter(|_| !popped_out(app, &person.id))
                .and_then(|_| picture(&person.id));
            let (content, ground) = if let Some(filmed) = filmed {
                (filmed, Color::BLACK)
            } else if watched {
                viewers = watch_viewers();
                stop = Some("Stop watching");
                let content = picture(&format!("stream:{}", person.id))
                    .unwrap_or_else(|| tile::note(app.look(), None, watch_status(app)));
                (content, Color::BLACK)
            } else {
                // Their picture until their camera's first frame, as
                // without one; a camera whose frames stop keeps its
                // last.
                // On a small tile the picture sits above the name's chip
                // rather than behind it: the chip's 40 px are left free.
                let small = size.height < 200.0;
                let below = if small { CHIP_ROOM } else { 16.0 };
                let mut side = (size.height * 0.45).min(80.0);
                if small {
                    side = side.min(size.height - below - 8.0).max(16.0);
                }
                let side = side.round();
                let opacity = if person.connected { 1.0 } else { 0.5 };
                let avatar = app.ctx().avatar(
                    person.avatar.as_deref(),
                    &person.user,
                    side,
                    side / 2.0,
                    opacity,
                );
                let content = container(avatar).center(Length::Fill).padding(Padding {
                    bottom: below,
                    ..Padding::ZERO
                });
                (content.into(), theme.raised)
            };
            if streams_here && !watched {
                on_press = Message::Call(Event::Watch(person.id.clone()));
            }
            let mut marks = Vec::new();
            if person.speaking_blocked || (person.muted && !person.deafened) {
                marks.push((Icon::MicOff, theme.danger));
            }
            if person.deafened {
                marks.push((Icon::HeadphoneOff, theme.danger));
            }
            let marks_count = marks.len();
            let said = status(person);
            let note =
                (!matches!(said, "Muted" | "Deafened" | "Speaking" | "Listening")).then_some(said);
            // A stream with a tile of its own is LIVE there.
            let live = person.streaming && (person.local || person.video.is_none());
            let mut tile = Tile::new(size, content, Some(ground))
                .ring(speaking(person).then_some(theme.live))
                .chip(tile::chip_in(
                    app.look(),
                    marks,
                    &person.user,
                    call_name_font(app, &person.person),
                    live,
                    note,
                ));
            if person.local && voice.camera_phase == voice::CameraPhase::Starting {
                tile = tile.top_left(tile::pill("Starting camera...", GLASS, Color::WHITE));
            }
            if streams_here && !watched && size.height < 200.0 {
                // A small tile keeps its picture where its neighbours' are:
                // Watch stream goes on the chip's line, opposite it, or as
                // its icon alone where both don't fit.
                let chip = chip_width(&person.user, marks_count, live);
                let room = size.width - 2.0 * 10.0 - 8.0;
                let corner: Element<'_, Message> = if room >= chip + WATCH_PILL {
                    // As tall as the chip, so the two share its line.
                    tile::pill("Watch stream", theme.danger, theme.on_danger)
                } else {
                    tile::glass_button(
                        app.look(),
                        Icon::Eye,
                        "Watch stream",
                        Message::Call(Event::Watch(person.id.clone())),
                    )
                };
                tile = tile.bottom_right(corner);
            } else if streams_here && !watched {
                tile = tile.top_right(tile::pill("Watch stream", theme.danger, theme.on_danger));
            }
            tile
        }
    };
    tile = tile.surface(theme, "call");
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
        // The picture out into a window of its own, beside going full screen.
        let out = tile::glass_button(
            app.look(),
            Icon::Link,
            "Open in a window",
            Message::Call(Event::PopoutSlot(slot)),
        );
        let size = tile::glass_button(app.look(), icon, label, Message::Call(Event::Full(!full)));
        tile = tile.bottom_right(row![out, size].spacing(6).into());
        // The wheel has brought it closer, and the bar says how far and
        // takes it there, at the other corner.
        if let Some(bar) = zoom_bar(app) {
            tile = tile.bottom_left(bar);
        }
    }
    if large {
        on_press = Message::Nothing;
        tile = tile.on_double(Message::Call(Event::Full(!full)));
    }
    if let (Some(user), Some(room)) = (right, voice.room.clone()) {
        // A window's own press opens the menu there, where the pointer is.
        tile = tile.on_right(move |at| {
            Message::Call(match press {
                Press::Call => Event::Person(room.clone(), user.clone(), at),
                Press::Window(id) => Event::PersonIn(id, room.clone(), user.clone(), at),
            })
        });
    }
    // In that window a tile shows its picture there, rather than changing
    // the call's stage behind it.
    let on_press = match press {
        Press::Call => on_press,
        Press::Window(_) => Message::Call(Event::PopoutSlot(slot)),
    };
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
/// camera, share the screen, and leave, at `size`.
pub(crate) fn call_controls(app: &App, size: ControlsSize) -> Element<'_, Message> {
    let theme = &app.theme;
    let voice = &app.client.state().voice;
    let ready = voice_ready(voice);
    let speaking_blocked = voice.speaking_blocked();
    let mic_off = voice.muted || speaking_blocked;
    let mic_label = if speaking_blocked {
        "Speaking is restricted in this channel"
    } else if voice.muted {
        "Unmute"
    } else {
        "Mute"
    };
    let mic_action = (ready && !speaking_blocked).then_some(Message::Call(Event::Mute));
    let when = |message: Message| ready.then_some(message);
    // A gel theme's call buttons are glossy gel buttons (as asked):
    // plain at rest, lit while on, the danger gel while muted and to leave.
    if crate::theme_art::has_gel(app.ctx()) {
        use crate::theme_art::{GelTone, gel_call_button};
        let ctx = app.ctx();
        let round = (size.side, size.side);
        let gel = |icon, label: &'static str, tone, message| {
            gel_call_button(ctx, icon, size.glyph, label, tone, round, message)
        };
        let mut row_of = row![].spacing(12).align_y(Alignment::Center);
        row_of = row_of.extend(gel(
            if mic_off { Icon::MicOff } else { Icon::Mic },
            mic_label,
            if mic_off {
                GelTone::Danger
            } else {
                GelTone::Plain
            },
            mic_action,
        ));
        row_of = row_of.extend(gel(
            if voice.deafened {
                Icon::HeadphoneOff
            } else {
                Icon::Headphones
            },
            if voice.deafened { "Undeafen" } else { "Deafen" },
            if voice.deafened {
                GelTone::Danger
            } else {
                GelTone::Plain
            },
            when(Message::Call(Event::Deafen)),
        ));
        if voice.soundboard.is_some()
            && let Some(button) = gel(
                Icon::Soundboard,
                "Open Soundboard",
                GelTone::Plain,
                when(Message::Call(Event::Open(Popup::Soundboard))),
            )
        {
            row_of = row_of.push(ctx.spot(button));
        }
        if voice.can_film {
            let on = voice.camera_phase != voice::CameraPhase::Off;
            let starting = voice.camera_phase == voice::CameraPhase::Starting;
            row_of = row_of.extend(gel(
                if on { Icon::Video } else { Icon::VideoOff },
                if starting {
                    "Starting camera..."
                } else if on {
                    "Turn off camera"
                } else {
                    "Turn on camera"
                },
                if on { GelTone::Lit } else { GelTone::Plain },
                when(Message::Call(Event::Camera)),
            ));
        }
        let sharing = voice.sharing.is_some();
        if (can_share(ctx) || sharing)
            && let Some(button) = gel(
                if sharing {
                    Icon::ScreenShareOff
                } else {
                    Icon::ScreenShare
                },
                if sharing {
                    "Stop sharing"
                } else {
                    "Share your screen"
                },
                if sharing {
                    GelTone::Lit
                } else {
                    GelTone::Plain
                },
                when(Message::Call(if sharing {
                    Event::StopSharing
                } else {
                    Event::Open(Popup::Share)
                })),
            )
        {
            row_of = row_of.push(ctx.spot(button));
        }
        let leaving = voice.phase == voice::Phase::Leaving;
        row_of = row_of.extend(gel_call_button(
            ctx,
            Icon::PhoneOff,
            ROUND_GLYPH,
            "Leave voice",
            GelTone::Danger,
            (size.leave, size.side),
            (!leaving).then_some(Message::Call(Event::Leave)),
        ));
        return container(row_of)
            .padding(Padding {
                top: 8.0,
                bottom: size.bottom,
                left: 16.0,
                right: 16.0,
            })
            .center_x(Length::Fill)
            .height(size.height())
            .into();
    }
    let alert = faded(theme.danger, 0.18);
    let face = |icon| app.look().glyph(icon, size.glyph, theme.text);
    let mut controls = row![
        controls::round_button_of(
            face(if mic_off { Icon::MicOff } else { Icon::Mic }),
            mic_label,
            if mic_off { alert } else { theme.raised },
            size.side,
            theme,
            mic_action,
        ),
        controls::round_button_of(
            face(if voice.deafened {
                Icon::HeadphoneOff
            } else {
                Icon::Headphones
            }),
            if voice.deafened { "Undeafen" } else { "Deafen" },
            if voice.deafened { alert } else { theme.raised },
            size.side,
            theme,
            when(Message::Call(Event::Deafen)),
        ),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if voice.soundboard.is_some() {
        controls = controls.push(app.ctx().spot(controls::round_button_of(
            face(Icon::Soundboard),
            "Open Soundboard",
            theme.raised,
            size.side,
            theme,
            when(Message::Call(Event::Open(Popup::Soundboard))),
        )));
    }
    if voice.can_film {
        let on = voice.camera_phase != voice::CameraPhase::Off;
        let starting = voice.camera_phase == voice::CameraPhase::Starting;
        controls = controls.push(controls::round_button_of(
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
            size.side,
            theme,
            when(Message::Call(Event::Camera)),
        ));
    }
    let sharing = voice.sharing.is_some();
    if can_share(app.ctx()) || sharing {
        let share = controls::round_button_of(
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
            size.side,
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
            .glyph(Icon::PhoneOff, size.glyph, theme.on_danger)
            .boxed(Length::Fill),
    )
    .width(size.leave)
    .height(size.side)
    .padding(0)
    .on_press_maybe((!leaving).then_some(Message::Call(Event::Leave)))
    // In the theme's corners, as Slint's `Look.radius(26px)`: a pill
    // stock, a square box in Zine's.
    .style(controls::lit_style(
        Some(faded(danger, opacity)),
        faded(danger_hover, opacity),
        theme.radius(size.side / 2.0),
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
            bottom: size.bottom,
            left: 16.0,
            right: 16.0,
        })
        .center_x(Length::Fill)
        .height(size.height())
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

/// The font of someone's name on their call tile: their display name
/// style's, as the call's voice channel lists them by their user id, else
/// the usual bold.
pub(crate) fn call_name_font(app: &App, person: &str) -> iced::Font {
    let state = app.client.state();
    let own = state
        .voice
        .room
        .as_deref()
        .and_then(|room| state.rooms.iter().find(|listed| listed.id == room))
        .and_then(|room| room.people.iter().find(|listed| listed.id == person))
        .and_then(|listed| {
            let style = state.cosmetics.style(listed.style.as_deref())?;
            crate::looks::name_look(Some(style), &listed.name, true, true)
        })
        .and_then(|look| look.font());
    crate::fonts::style_font(own, 700)
}
