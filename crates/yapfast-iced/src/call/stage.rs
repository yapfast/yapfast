//! A call's stage, as the Slint client's `ParticipantGrid` lays it out:
//! this account's shared screen first, then the streams of those who film
//! as well in tiles of their own, then everyone. A click on a tile shows it
//! large, with the others in rows beneath, until a click beside it; the
//! large one goes full screen. Under the stage, the call's controls; over
//! it, a line on how the call is. A voice channel this account isn't in
//! shows its Join instead.

use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size,
    widget::{Space, button, column, container, mouse_area, pin, row, scrollable, stack, text},
};
use yapfast_core::voice::{self, Participant, SharePhase};

use super::{Event, Popup, voice_ready};
use crate::{
    app::{App, Message},
    controls::{self, ROUND, ROUND_GLYPH, bar},
    header,
    pictures::Icon,
    style::{faded, fill, rounded},
    theme::rgb,
    tile::{self, GLASS, NOTE, Tile},
    video::{self, Fresh, Video},
    widgets::Spot,
};

/// The room under a voice channel's stage the Slint client leaves for the
/// line over it and the controls under it, so the tiles come out the same
/// size.
pub const CONTROLS: f32 = 111.0;
/// The controls' row under the stage: 8 px over the buttons, 16 under.
const CONTROLS_HEIGHT: f32 = 8.0 + ROUND + 16.0;
/// Between tiles, and round the stage's edge; none full screen.
const GAP: f32 = 12.0;
const INSET: f32 = 24.0;
/// Tiles' shape, and the widest a tile of the grid gets.
const ASPECT: f32 = 0.5625;
const WIDEST: f32 = 420.0;
/// The corner picture's size, as the Slint client's `StreamPip`.
pub const CORNER: Size = Size::new(320.0, 180.0);

/// The tile shown large, as the Slint client's `tiles::Focus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Focus {
    Person(String),
    Stream(String),
    OwnScreen,
}

/// What the corner picture shows, as the Slint client's `tiles::Pip`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    None,
    Watched,
    Shared,
    Camera(usize),
}

/// What a tile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    OwnScreen,
    Stream(usize),
    Person(usize),
}

/// The call's tiles, as the Slint client's `tiles::Stage`.
#[derive(Clone, Copy)]
pub struct Stage<'a> {
    pub participants: &'a [Participant],
    pub sharing: bool,
}

impl<'a> Stage<'a> {
    pub fn of(voice: &'a voice::State) -> Self {
        Self {
            participants: if voice.active() {
                &voice.participants
            } else {
                &[]
            },
            sharing: voice.sharing.is_some(),
        }
    }

    /// Those whose streams take tiles of their own: others who share and
    /// film, as their camera takes their own tile.
    pub fn streams(&self) -> impl Iterator<Item = usize> + '_ {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, person)| person.streaming && person.video.is_some() && !person.local)
            .map(|(index, _)| index)
    }

    /// The tiles before the participants'.
    pub fn before(&self) -> usize {
        usize::from(self.sharing) + self.streams().count()
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|person| person.id == id)
    }

    pub fn slots(&self) -> Vec<Slot> {
        self.sharing
            .then_some(Slot::OwnScreen)
            .into_iter()
            .chain(self.streams().map(Slot::Stream))
            .chain((0..self.participants.len()).map(Slot::Person))
            .collect()
    }

    /// Where `focus` is: a stream without a tile of its own shows in its
    /// streamer's.
    pub fn slot(&self, focus: &Focus) -> Option<usize> {
        match focus {
            Focus::OwnScreen => self.sharing.then_some(0),
            Focus::Person(id) => Some(self.before() + self.index(id)?),
            Focus::Stream(id) => {
                let person = self.index(id)?;
                match self.streams().position(|stream| stream == person) {
                    Some(stream) => Some(usize::from(self.sharing) + stream),
                    None => (self.participants[person].streaming
                        && !self.participants[person].local)
                        .then(|| self.before() + person),
                }
            }
        }
    }

    /// What the tile at `slot` shows, to show it large.
    pub fn focus_at(&self, slot: usize) -> Option<Focus> {
        let sharing = usize::from(self.sharing);
        if slot < sharing {
            return Some(Focus::OwnScreen);
        }
        let before = self.before();
        if slot < before {
            let person = self.streams().nth(slot - sharing)?;
            return Some(Focus::Stream(self.participants[person].id.clone()));
        }
        let person = self.participants.get(slot - before)?;
        let id = person.id.clone();
        Some(if person.video.is_some() || !person.streaming {
            Focus::Person(id)
        } else if person.local {
            if self.sharing {
                Focus::OwnScreen
            } else {
                Focus::Person(id)
            }
        } else {
            Focus::Stream(id)
        })
    }

    /// The corner shows what's large if it moves, else the stream watched,
    /// this account's own, or someone else's camera.
    pub fn pip(&self, focus: Option<&Focus>, watching: Option<&str>) -> Pip {
        match focus {
            Some(Focus::OwnScreen) if self.sharing => return Pip::Shared,
            Some(Focus::Stream(id)) if watching == Some(id.as_str()) => return Pip::Watched,
            Some(Focus::Person(id)) => {
                if let Some(person) = self.index(id)
                    && self.participants[person].video.is_some()
                {
                    return Pip::Camera(person);
                }
            }
            _ => {}
        }
        if watching.is_some() {
            Pip::Watched
        } else if self.sharing {
            Pip::Shared
        } else {
            self.participants
                .iter()
                .position(|person| !person.local && person.video.is_some())
                .map_or(Pip::None, Pip::Camera)
        }
    }

    /// The tile the corner shows, which a click on it shows large.
    pub fn pip_slot(&self, pip: Pip, watching: Option<&str>) -> Option<usize> {
        match pip {
            Pip::None => None,
            Pip::Watched => self.slot(&Focus::Stream(watching?.to_owned())),
            Pip::Shared => self.slot(&Focus::OwnScreen),
            Pip::Camera(person) => Some(self.before() + person),
        }
    }
}

/// Where each tile goes on a stage of `size`, by place, with `focused`
/// large and the others in rows beneath it, and how tall the tiles are
/// together: as the Slint client's `ParticipantGrid`. Full screen, the
/// large one fills the stage alone.
pub fn tiles(
    count: usize,
    focused: Option<usize>,
    size: Size,
    full: bool,
) -> (Vec<Rectangle>, f32) {
    if count == 0 {
        return (Vec::new(), size.height);
    }
    let (gap, inset) = if full { (0.0, 0.0) } else { (GAP, INSET) };
    let (width, height) = (size.width, size.height);
    let span = width - 2.0 * inset;
    let focusing = focused.is_some_and(|focused| focused < count);
    // A few people take the columns, and the size, that make their tiles
    // largest within both the width and the height. Side by side wins
    // unless stacking makes tiles a tenth larger. More people take tiles at
    // least 240 px wide, and scroll.
    let fitted = |columns: usize| {
        let rows = count.div_ceil(columns) as f32;
        let columns = columns as f32;
        WIDEST
            .min((span - (columns - 1.0) * gap) / columns)
            .min((height - 2.0 * inset - (rows - 1.0) * gap) / rows / ASPECT)
    };
    let fewest = ((span + gap) / (240.0 + gap))
        .floor()
        .min(count as f32)
        .max(1.0) as usize;
    let mut best_fit = 1;
    while best_fit < count.min(4) && fitted(best_fit + 1) * 1.1 > fitted(best_fit) {
        best_fit += 1;
    }
    let others = count - 1;
    let strip_width = if others == 0 {
        0.0
    } else {
        ((span - (others as f32 - 1.0) * gap) / others as f32).clamp(120.0, 192.0)
    };
    let strip_columns = ((span + gap) / (strip_width + gap))
        .floor()
        .min(others as f32)
        .max(1.0) as usize;
    let strip_rows = if full || others == 0 {
        0
    } else {
        others.div_ceil(strip_columns)
    };
    let strip_height =
        strip_rows as f32 * strip_width * ASPECT + (strip_rows as f32 - 1.0).max(0.0) * gap;
    let under_focus = if strip_rows > 0 {
        gap + strip_height
    } else {
        0.0
    };
    let focus_width = if full {
        width
    } else {
        span.min((height - 2.0 * inset - under_focus).max(160.0) / ASPECT)
    };
    let focus_height = if full { height } else { focus_width * ASPECT };
    let focus_top = if full {
        0.0
    } else {
        inset.max((height - focus_height - under_focus) / 2.0)
    };
    let columns = if focusing {
        strip_columns
    } else if count <= 4 {
        best_fit
    } else {
        fewest
    };
    let tile_width = if focusing {
        strip_width
    } else if count <= 4 {
        fitted(columns).max(160.0)
    } else {
        WIDEST.min((span - (columns as f32 - 1.0) * gap) / columns as f32)
    };
    let tile_height = tile_width * ASPECT;
    let placed = if focusing { others } else { count };
    let rows = placed.div_ceil(columns.max(1));
    let grid_height = rows as f32 * tile_height + (rows as f32 - 1.0).max(0.0) * gap;
    let grid_width = columns as f32 * tile_width + (columns as f32 - 1.0).max(0.0) * gap;
    let first_row = if focusing {
        focus_top + focus_height + gap
    } else {
        inset.max((height - grid_height) / 2.0)
    };
    let content = if focusing {
        height.max(focus_top + focus_height + under_focus + inset)
    } else {
        height.max(grid_height + 2.0 * inset)
    };
    // On whole pixels: a frame made at a tile's size is then copied in one
    // to one, and the tile's round corners take the renderer's fast path.
    let whole = |x: f32, y: f32, width: f32, height: f32| {
        let (left, top) = (x.round(), y.round());
        Rectangle::new(
            Point::new(left, top),
            Size::new((x + width).round() - left, (y + height).round() - top),
        )
    };
    let rects = (0..count)
        .map(|slot| {
            if focusing && Some(slot) == focused {
                return whole(
                    (width - focus_width) / 2.0,
                    focus_top,
                    focus_width,
                    focus_height,
                );
            }
            // Full screen shows the large one alone.
            if focusing && full {
                return Rectangle::new(Point::new(0.0, height), Size::ZERO);
            }
            let place = if focusing && focused.is_some_and(|focused| slot > focused) {
                slot - 1
            } else {
                slot
            };
            let x =
                (width - grid_width) / 2.0 + (place % columns.max(1)) as f32 * (tile_width + gap);
            let y = first_row + (place / columns.max(1)) as f32 * (tile_height + gap);
            whole(x, y, tile_width, tile_height)
        })
        .collect();
    (rects, content)
}

/// The pixels a tile of `bounds` shows at in a window of `scale` pixels
/// to the unit, while any of it is in the stage's `height` scrolled
/// `scrolled` down.
pub fn shown_at(bounds: Rectangle, scrolled: f32, height: f32, scale: f32) -> Option<(u32, u32)> {
    let pixels = |side: f32| ((side * scale).round() as u32).max(2);
    let top = bounds.y - scrolled;
    (bounds.width > 0.0 && top < height && top + bounds.height > 0.0)
        .then(|| (pixels(bounds.width), pixels(bounds.height)))
}

/// How someone shows in their tile: what they're doing, or what's wrong.
pub fn status(person: &Participant) -> &str {
    if let Some(problem) = &person.problem {
        problem
    } else if !person.connected {
        "Connection lost"
    } else if person.deafened {
        "Deafened"
    } else if person.muted {
        "Muted"
    } else if person.speaking {
        "Speaking"
    } else {
        "Listening"
    }
}

/// Speaking, as the ring and the lists show it: unmuted and connected.
pub fn speaking(person: &Participant) -> bool {
    person.speaking && !person.muted && person.connected
}

/// What this account's sharing is doing, in words.
pub fn share_status(voice: &voice::State) -> &'static str {
    match voice.sharing.as_ref().map(|sharing| sharing.phase) {
        Some(SharePhase::Choosing) => "Choose what to share",
        Some(SharePhase::Starting) => "Starting your stream...",
        Some(SharePhase::Live) => "Live",
        None => "",
    }
}

impl App {
    /// The open room is a voice channel: the window shows its call.
    pub fn voice_selected(&self) -> bool {
        let state = self.client.state();
        state.selected.as_ref().is_some_and(|selected| {
            state
                .rooms
                .iter()
                .any(|room| &room.id == selected && room.voice && !room.direct)
        })
    }

    /// A voice channel is open on its call, not on its chat.
    pub fn voice_panel(&self) -> bool {
        self.voice_selected() && !self.client.state().voice_chat
    }

    /// The open room's call is the one this account is in.
    pub fn voice_here(&self) -> bool {
        let state = self.client.state();
        state.voice.active() && state.voice.room.is_some() && state.voice.room == state.selected
    }

    /// The stage shows: its call is open, here or above a DM's messages,
    /// or full screen, in a window (closed to the tray, nothing shows).
    pub fn stage_shown(&self) -> bool {
        self.shell.window.is_some()
            && (self.voice_here() && (self.voice_panel() || self.dm_call().is_some())
                || self.full_shown())
    }

    /// The large tile is over the whole window.
    pub fn full_shown(&self) -> bool {
        self.call.full && self.client.state().voice.active() && self.call.focus.is_some()
    }

    /// The stage's size where it shows: full screen, in a voice channel
    /// under its header, or above a DM's messages.
    pub fn stage_size(&self) -> Size {
        if self.full_shown() {
            return self.window;
        }
        let below_header = self.window.height - header::HEIGHT - 1.0;
        let (right, height) = if self.voice_panel() {
            (self.window.width, below_header)
        } else {
            // Above a DM's messages, beside its profile panel.
            (self.chat_right(), self.dm_panel_height(below_header))
        };
        let width = right - self.chat_left();
        Size::new(width, (height - CONTROLS).max(0.0))
    }

    /// The stage's tiles, the one shown large, where each goes, and how tall
    /// they are together.
    pub fn stage(&self) -> (Vec<Slot>, Option<usize>, Vec<Rectangle>, f32) {
        let voice = &self.client.state().voice;
        let stage = Stage::of(voice);
        let slots = stage.slots();
        let focused = self.call.focus.as_ref().and_then(|focus| stage.slot(focus));
        let (tiles, content) = tiles(slots.len(), focused, self.stage_size(), self.full_shown());
        (slots, focused, tiles, content)
    }

    /// What the corner picture shows while the call is off screen.
    pub fn corner(&self) -> Pip {
        let voice = &self.client.state().voice;
        if self.shell.window.is_none() || !self.workspace() || !voice.active() || self.stage_shown()
        {
            return Pip::None;
        }
        let watching = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        Stage::of(voice).pip(self.call.focus.as_ref(), watching)
    }

    /// The corner's tile, by its place on the stage.
    pub fn corner_place(&self) -> Option<usize> {
        let voice = &self.client.state().voice;
        let watching = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        Stage::of(voice).pip_slot(self.corner(), watching)
    }

    /// A tile shown large that's gone (its stream ended, they left) shows
    /// none.
    pub fn keep_focus(&mut self) {
        let voice = &self.client.state().voice;
        let stage = Stage::of(voice);
        if self
            .call
            .focus
            .as_ref()
            .is_some_and(|focus| stage.slot(focus).is_none())
        {
            self.call.focus = None;
            self.call.full = false;
        }
    }

    /// Tells each camera where its tile shows, or that it doesn't, so frames
    /// are made only for tiles in sight, at their size, and whether it's
    /// large; and has [`video::frames`] look for frames from those that
    /// show: cameras, the stream watched, the screen shared, a message's
    /// video. With the call off screen, the corner's shows there. As the
    /// Slint client's `place_videos`.
    pub fn place_videos(&mut self) {
        let shown = self.stage_shown();
        let (slots, focused, tiles, _) = self.stage();
        let height = self.stage_size().height;
        let corner = self.corner();
        let voice = &self.client.state().voice;
        let mut sources = video::Sources::default();
        // The camera Voice's settings try.
        if let Some(feed) = self.camera_test() {
            sources
                .feeds
                .push((crate::settings::voice::CAMERA_TEST.to_owned(), feed.clone()));
        }
        // In the window's pixels, as the Slint client asks (`tiles.rs`).
        let scale = self.device_scale();
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
                    .then(|| shown_at(tiles[place], self.call.scrolled, height, scale))
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
        if (shown || corner == Pip::Shared)
            && let Some(sharing) = &voice.sharing
        {
            sources
                .mapped
                .push((OWN_SCREEN.to_owned(), sharing.frames.clone()));
        }
        // A message's video, wherever it plays.
        let mut clip = false;
        if let Some(playing) = &self.client.state().video
            && let Some(frames) = &playing.frames
        {
            clip = true;
            sources.mapped.push((
                format!("clip:{}/{}", playing.item, playing.index),
                frames.clone(),
            ));
        }
        // Frames of what no longer shows go.
        self.frames.retain(|key, _| {
            sources.feeds.iter().any(|(id, _)| id == key)
                || sources.mapped.iter().any(|(id, _)| id == key)
        });
        if !clip {
            self.clip_time = None;
        }
        self.video.set(sources);
    }

    /// New frames: each takes the place of its tile's last.
    pub fn frames_found(&mut self, found: Vec<Fresh>) {
        for fresh in found {
            if fresh.time.is_some() {
                self.video.taken(&fresh.key);
            }
            if fresh.key.starts_with("clip:") {
                self.clip_time = fresh.time;
            }
            self.frames.insert(fresh.key, (fresh.handle, fresh.size));
        }
    }

    /// A tile was clicked, by its place, and is shown large; or beside the
    /// tiles, and none is.
    pub fn focus_tile(&mut self, place: Option<usize>) {
        let voice = &self.client.state().voice;
        self.call.focus = place.and_then(|place| Stage::of(voice).focus_at(place));
        if self.call.focus.is_none() {
            self.call.full = false;
        }
        self.place_videos();
    }

    /// A frame by its key, fitted into its tile with `corners` cut round
    /// on `behind` (none full screen).
    pub fn frame(&self, key: &str, corners: Option<(u32, u32)>) -> Option<Element<'_, Message>> {
        self.frames.get(key).map(|(handle, size)| {
            let video = Video::new(handle.clone(), *size, Color::BLACK).scale(self.device_scale());
            let video = match corners {
                Some((radius, behind)) => video.corners(radius, behind),
                None => video,
            };
            Element::from(video)
        })
    }

    /// A voice channel's view: its header, then its call's stage with the
    /// controls, or its Join while this account is elsewhere.
    pub fn call_view(&self) -> Element<'_, Message> {
        let line = bar(self.theme.outline_chat, Length::Fill, 1);
        let body = if self.voice_here() {
            self.call_panel()
        } else {
            self.join_view()
        };
        // The window's ground is the chat's; the stage paints its own only
        // where a theme gives it another (none built in does), as a ground
        // under the video is drawn again with every frame shown.
        let theme = &self.theme;
        let stage = (theme.stage != theme.chat).then_some(theme.stage);
        // The banners show over the call too, under the header.
        let column = column![self.header_view(), line]
            .extend(self.banners())
            .push(body);
        container(column)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| stage.map(fill).unwrap_or_default())
            .into()
    }

    /// The call this account is in: the line over the stage, the stage
    /// (unless it's full screen), and the controls.
    pub fn call_panel(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let line = container(
            text(self.voice_description())
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
        let size = self.stage_size();
        let stage: Element<'_, Message> = if self.full_shown() {
            Space::new().width(size.width).height(size.height).into()
        } else {
            self.stage_view()
        };
        column![line, stage, self.call_controls()].into()
    }

    /// What the line over the stage, or a channel's Join, says of the call.
    pub fn voice_description(&self) -> String {
        let state = self.client.state();
        let voice = &state.voice;
        let room = state.selected_room();
        let present = room.map_or(&[][..], |room| room.people.as_slice());
        if room.is_some_and(|room| room.locked) {
            return "Your roles on this Discord server don't let you join this channel.".into();
        }
        if let Some(room) = self.dm_call().filter(|_| self.voice_here()) {
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
        if self.voice_here() {
            return if voice.participants.len() == 1 {
                "You're here alone. Stay as long as you like; others can join later.".into()
            } else {
                "Voice stays connected while you browse other conversations.".into()
            };
        }
        let join = if self.joins_muted() {
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
    pub fn joins_muted(&self) -> bool {
        !self.layout.unmute_on_join.unwrap_or(true) || self.layout.deafen_on_join
    }

    /// A voice channel this account isn't in, as the Slint client's
    /// `JoinVoice`: what it is, and a way to join.
    fn join_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let id = state.selected.clone().unwrap_or_default();
        let raised = theme.raised;
        let badge = container(self.glyph(Icon::Volume, 38, theme.secondary))
            .center(80)
            .style(move |_| rounded(raised, 40));
        let mut panel = column![
            badge,
            text(self.header.name.as_str())
                .size(26)
                .font(crate::fonts::BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            text(self.voice_description())
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
        let can_join = self.client.state().can_join_voice(&id);
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
    pub fn stage_view(&self) -> Element<'_, Message> {
        let size = self.stage_size();
        let full = self.full_shown();
        let (slots, focused, tiles, content) = self.stage();
        // A click beside the large tile shows the grid again.
        let beside = if focused.is_some() && !full {
            Message::Call(Event::Focus(None))
        } else {
            Message::Nothing
        };
        let mut layers = stack![
            mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(beside)
        ];
        for (place, (slot, bounds)) in slots.iter().zip(&tiles).enumerate() {
            if bounds.width <= 0.0 || (full && focused != Some(place)) {
                continue;
            }
            let tile = self.stage_tile(place, *slot, *bounds, focused == Some(place), full);
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
        scrollable(grid)
            .width(size.width)
            .height(size.height)
            .on_scroll(|viewport| Message::Call(Event::Scrolled(viewport.absolute_offset().y)))
            .into()
    }

    /// One tile on the stage.
    fn stage_tile(
        &self,
        place: usize,
        slot: Slot,
        bounds: Rectangle,
        large: bool,
        full: bool,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.client.state().voice;
        let size = bounds.size();
        let pointed = self.call.pointed == Some(place);
        let corners = (!full).then(|| (tile::RADIUS as u32, rgb(theme.stage)));
        let watch_user = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        let watch_viewers = || {
            voice
                .watching
                .as_ref()
                .and_then(|watching| self.viewers(&watching.viewers, Some(&watching.user)))
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
                let framed = self.frame(OWN_SCREEN, corners);
                let note = (!live && framed.is_some()).then_some(status);
                viewers = sharing.and_then(|sharing| self.viewers(&sharing.viewers, None));
                let content =
                    framed.unwrap_or_else(|| tile::note(self, Some(Icon::ScreenShare), status));
                Tile::new(size, content, Some(Color::BLACK)).chip(tile::chip(
                    self,
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
                    self.frame(&format!("stream:{}", person.id), corners)
                        .unwrap_or_else(|| tile::note(self, None, self.watch_status()))
                } else {
                    on_press = Message::Call(Event::Watch(person.id.clone()));
                    let column = column![
                        self.glyph(Icon::ScreenShare, 30, NOTE),
                        tile::pill("Watch stream", theme.danger, theme.on_danger),
                    ]
                    .spacing(10)
                    .align_x(Alignment::Center);
                    container(column).center(Length::Fill).into()
                };
                Tile::new(size, content, Some(Color::BLACK)).chip(tile::chip(
                    self,
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
                    .and_then(|_| self.frame(&person.id, corners));
                let (content, ground) = if let Some(filmed) = filmed {
                    (filmed, Color::BLACK)
                } else if watched {
                    viewers = watch_viewers();
                    stop = Some("Stop watching");
                    let content = self
                        .frame(&format!("stream:{}", person.id), corners)
                        .unwrap_or_else(|| tile::note(self, None, self.watch_status()));
                    (content, Color::BLACK)
                } else if person.video.is_some() {
                    (Space::new().into(), Color::BLACK)
                } else {
                    let side = (size.height * 0.45).min(80.0).round();
                    let opacity = if person.connected { 1.0 } else { 0.5 };
                    let avatar = self.avatar(
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
                let note = (!matches!(said, "Muted" | "Deafened" | "Speaking" | "Listening"))
                    .then_some(said);
                // A stream with a tile of its own is LIVE there.
                let live = person.streaming && (person.local || person.video.is_none());
                let mut tile = Tile::new(size, content, Some(ground))
                    .ring(speaking(person).then_some(theme.live))
                    .chip(tile::chip(self, marks, &person.user, live, note));
                if person.local && voice.camera_phase == voice::CameraPhase::Starting {
                    tile = tile.top_left(tile::pill("Starting camera...", GLASS, Color::WHITE));
                }
                if streams_here && !watched {
                    tile =
                        tile.top_right(tile::pill("Watch stream", theme.danger, theme.on_danger));
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
                self,
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
                self,
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
            tile = tile
                .on_right(move |at| Message::Call(Event::Person(room.clone(), user.clone(), at)));
        }
        tile.on_press(on_press)
            .on_point(
                Message::Call(Event::Point(place, true)),
                Message::Call(Event::Point(place, false)),
            )
            .into()
    }

    /// What a watched stream's tile says until its first picture.
    pub fn watch_status(&self) -> String {
        let Some(watching) = &self.client.state().voice.watching else {
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
    pub fn viewers(&self, ids: &[String], streamer: Option<&str>) -> Option<Element<'_, Message>> {
        let voice = &self.client.state().voice;
        let people: Vec<(&str, Option<&str>)> = voice
            .participants
            .iter()
            .filter(|person| Some(person.id.as_str()) != streamer && ids.contains(&person.id))
            .map(|person| (person.user.as_str(), person.avatar.as_deref()))
            .collect();
        let names: Vec<&str> = people.iter().map(|(name, _)| *name).collect();
        let names = format!("Watching: {}", names.join(", "));
        tile::viewers(self, &people, names)
    }

    /// The call's controls under the stage, as the Slint client's
    /// `CallControls`: mute, deafen, the soundboard, the camera, share the
    /// screen, and leave.
    fn call_controls(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.client.state().voice;
        let ready = voice_ready(voice);
        let when = |message: Message| ready.then_some(message);
        let alert = faded(theme.danger, 0.18);
        let face = |icon| self.glyph(icon, ROUND_GLYPH, theme.text);
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
            controls = controls.push(self.spot(controls::round_button(
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
        if self.can_share() || sharing {
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
            controls = controls.push(self.spot(share));
        }
        let leaving = voice.phase == voice::Phase::Leaving;
        let (danger, danger_hover) = (theme.danger, theme.danger_hover);
        let opacity = if leaving { 0.5 } else { 1.0 };
        let leave = button(
            self.glyph(Icon::PhoneOff, ROUND_GLYPH, theme.on_danger)
                .boxed(Length::Fill),
        )
        .width(76)
        .height(ROUND)
        .padding(0)
        .on_press_maybe((!leaving).then_some(Message::Call(Event::Leave)))
        .style(move |_, status| button::Style {
            background: Some(
                faded(
                    if controls::lights(status) {
                        danger_hover
                    } else {
                        danger
                    },
                    opacity,
                )
                .into(),
            ),
            border: iced::Border::default().rounded(ROUND / 2.0),
            ..Default::default()
        });
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

    /// A control that opens a popup above itself: where it is is kept as
    /// it's pressed.
    pub fn spot<'a>(&'a self, child: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        Spot::new(child, &self.call.spot).into()
    }

    /// Sharing a screen can start: Discord's call, connected.
    pub fn can_share(&self) -> bool {
        let state = self.client.state();
        let voice = &state.voice;
        !state.preview
            && voice.phase == voice::Phase::Connected
            && voice.room.as_deref().is_some_and(|room| {
                yapfast_core::network::Network::of(room) == yapfast_core::network::Network::Discord
            })
    }
}

/// The frames of the screen this account shares, by their key.
pub const OWN_SCREEN: &str = "share:own";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tiles_sit_side_by_side_and_one_large_puts_the_other_under_it() {
        let stage = Size::new(1500.0, 800.0);
        let (grid, _) = tiles(2, None, stage, false);
        assert_eq!(grid[0].y, grid[1].y);
        assert!(grid[0].x + grid[0].width <= grid[1].x);
        let (focused, _) = tiles(2, Some(1), stage, false);
        assert!(focused[1].width > focused[0].width);
        assert!(focused[0].y > focused[1].y + focused[1].height);
        let (full, _) = tiles(2, Some(1), stage, true);
        assert_eq!(full[1], Rectangle::new(Point::ORIGIN, stage));
        assert_eq!(shown_at(full[0], 0.0, stage.height, 1.0), None, "alone");
    }

    #[test]
    fn many_tiles_scroll_and_only_those_in_sight_are_made() {
        let stage = Size::new(900.0, 500.0);
        let (grid, content) = tiles(12, None, stage, false);
        assert!(content > stage.height, "taller than the stage");
        let last = *grid.last().unwrap();
        assert_eq!(shown_at(last, 0.0, stage.height, 1.0), None, "below");
        assert!(shown_at(last, content - stage.height, stage.height, 1.0).is_some());
        assert_eq!(
            shown_at(grid[0], content - stage.height, stage.height, 1.0),
            None,
            "scrolled away"
        );
    }

    #[test]
    fn a_tile_below_the_stage_has_no_frames_made() {
        let below = Rectangle::new(Point::new(0.0, 600.0), Size::new(300.0, 168.0));
        assert_eq!(shown_at(below, 0.0, 598.0, 1.0), None);
        let half = Rectangle::new(Point::new(0.0, 500.0), Size::new(300.4, 168.6));
        assert_eq!(shown_at(half, 0.0, 598.0, 1.0), Some((300, 169)));
        // In the window's pixels: twice as many on a display at 200%.
        assert_eq!(shown_at(half, 0.0, 598.0, 2.0), Some((601, 337)));
    }

    fn person(id: &str, local: bool, streaming: bool, filming: bool) -> Participant {
        Participant {
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
            video: filming.then(yapfast_core::picture::Feed::new),
            avatar: None,
        }
    }

    #[test]
    fn a_stream_takes_its_own_tile_only_beside_a_camera() {
        let people = [
            person("me", true, true, true),
            person("ana", false, true, false),
            person("bo", false, true, true),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: true,
        };
        assert_eq!(stage.streams().collect::<Vec<_>>(), [2]);
        assert_eq!(stage.before(), 2);
        assert_eq!(
            stage.slots(),
            [
                Slot::OwnScreen,
                Slot::Stream(2),
                Slot::Person(0),
                Slot::Person(1),
                Slot::Person(2),
                Slot::Person(3)
            ]
        );
        assert_eq!(stage.slot(&Focus::OwnScreen), Some(0));
        assert_eq!(stage.slot(&Focus::Stream("bo".into())), Some(1));
        assert_eq!(
            stage.slot(&Focus::Stream("ana".into())),
            Some(3),
            "in her tile"
        );
        assert_eq!(stage.slot(&Focus::Stream("cy".into())), None);
        assert_eq!(stage.slot(&Focus::Person("cy".into())), Some(5));
        for slot in 0..6 {
            let focus = stage.focus_at(slot).unwrap();
            assert_eq!(stage.slot(&focus), Some(slot), "{focus:?}");
        }
        assert_eq!(stage.focus_at(6), None);
    }

    #[test]
    fn the_corner_shows_what_is_large_or_else_a_stream_or_a_camera() {
        let people = [
            person("me", true, false, true),
            person("ana", false, true, false),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: false,
        };
        let cy = Focus::Person("cy".into());
        assert_eq!(stage.pip(Some(&cy), Some("ana")), Pip::Camera(2));
        assert_eq!(stage.pip(None, Some("ana")), Pip::Watched);
        assert_eq!(stage.pip_slot(Pip::Watched, Some("ana")), Some(1));
        assert_eq!(
            stage.pip(None, None),
            Pip::Camera(2),
            "not this account's own"
        );
        let sharing = Stage {
            sharing: true,
            ..stage
        };
        assert_eq!(
            sharing.pip(Some(&Focus::OwnScreen), Some("ana")),
            Pip::Shared
        );
        assert_eq!(sharing.pip_slot(Pip::Shared, None), Some(0));
    }
}
