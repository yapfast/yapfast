//! A voice channel's view: its call's stage, as the Slint client's
//! `Stage` and `ParticipantGrid` lay it out, with Leave under it; or the
//! channel's Join button while this account isn't in its call. The call's
//! other controls (mute, deafen, camera, share) are M2's.

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size,
    widget::{Space, button, column, container, mouse_area, pin, row, stack, text},
};
use yapfast_core::voice::Participant;

use crate::{
    app::{App, Message},
    controls::{bar, lit_style},
    fonts::BOLD,
    header,
    pictures::Icon,
    rail, sidebar,
    style::{fill, rounded},
    theme::{Theme, rgb},
    video::{self, Fresh, Video},
    widgets::Above,
};

/// The room under the stage the Slint client's stage leaves for its
/// controls and note, so the tiles come out the same size.
const CONTROLS: f32 = 111.0;
/// The tiles' ground while a camera or stream fills them.
const STAGE_BLACK: Color = Color::BLACK;
/// Between tiles, and round the stage's edge.
const GAP: f32 = 12.0;
const INSET: f32 = 24.0;

/// The tile shown large, as the Slint client's `tiles::Focus`.
#[derive(Clone, Debug, PartialEq)]
pub enum Focus {
    Person(String),
    Stream(String),
}

/// What a tile shows: someone's stream in a tile of its own (they film as
/// well), or someone.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Stream(usize),
    Person(usize),
}

/// The call's tiles in order: the streams of those who film as well, then
/// everyone.
fn slots(participants: &[Participant]) -> Vec<Slot> {
    let streams = participants
        .iter()
        .enumerate()
        .filter(|(_, person)| person.streaming && person.video.is_some() && !person.local)
        .map(|(index, _)| Slot::Stream(index));
    streams
        .chain((0..participants.len()).map(Slot::Person))
        .collect()
}

/// Where `focus` is among `slots`.
fn focus_slot(slots: &[Slot], participants: &[Participant], focus: &Focus) -> Option<usize> {
    match focus {
        Focus::Person(id) => slots
            .iter()
            .position(|slot| matches!(slot, Slot::Person(i) if participants[*i].id == *id)),
        Focus::Stream(id) => slots
            .iter()
            .position(|slot| matches!(slot, Slot::Stream(i) if participants[*i].id == *id))
            .or_else(|| {
                slots.iter().position(|slot| {
                    matches!(slot, Slot::Person(i) if participants[*i].id == *id
                        && participants[*i].streaming
                        && !participants[*i].local)
                })
            }),
    }
}

/// Where each tile goes in a stage of `size`, by place, with `focused`
/// large and the others in rows of small tiles beneath it, as the Slint
/// client's `ParticipantGrid`.
fn tiles(count: usize, focused: Option<usize>, size: Size) -> Vec<Rectangle> {
    if count == 0 {
        return Vec::new();
    }
    let (width, height) = (size.width, size.height);
    let span = width - 2.0 * INSET;
    let focusing = focused.is_some_and(|focused| focused < count);
    let fitted = |columns: usize| {
        let rows = count.div_ceil(columns) as f32;
        let columns = columns as f32;
        420f32
            .min((span - (columns - 1.0) * GAP) / columns)
            .min((height - 2.0 * INSET - (rows - 1.0) * GAP) / rows / 0.5625)
    };
    let fewest = ((span + GAP) / (240.0 + GAP))
        .floor()
        .min(count as f32)
        .max(1.0) as usize;
    // A column more while it makes the tiles no more than a tenth smaller.
    let mut best_fit = 1;
    while best_fit < count.min(4) && fitted(best_fit + 1) * 1.1 > fitted(best_fit) {
        best_fit += 1;
    }
    let others = count - 1;
    let strip_width = if others == 0 {
        0.0
    } else {
        ((span - (others as f32 - 1.0) * GAP) / others as f32).clamp(120.0, 192.0)
    };
    let strip_columns = ((span + GAP) / (strip_width + GAP))
        .floor()
        .min(others as f32)
        .max(1.0) as usize;
    let strip_rows = if others == 0 {
        0
    } else {
        others.div_ceil(strip_columns)
    };
    let strip_height =
        strip_rows as f32 * strip_width * 0.5625 + (strip_rows as f32 - 1.0).max(0.0) * GAP;
    let under_focus = if strip_rows > 0 {
        GAP + strip_height
    } else {
        0.0
    };
    let focus_width = span.min((height - 2.0 * INSET - under_focus).max(160.0) / 0.5625);
    let focus_height = focus_width * 0.5625;
    let focus_top = INSET.max((height - focus_height - under_focus) / 2.0);
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
        420f32.min((span - (columns as f32 - 1.0) * GAP) / columns as f32)
    };
    let tile_height = tile_width * 0.5625;
    let placed = if focusing { others } else { count };
    let rows = placed.div_ceil(columns.max(1));
    let grid_height = rows as f32 * tile_height + (rows as f32 - 1.0).max(0.0) * GAP;
    let grid_width = columns as f32 * tile_width + (columns as f32 - 1.0).max(0.0) * GAP;
    let first_row = if focusing {
        focus_top + focus_height + GAP
    } else {
        INSET.max((height - grid_height) / 2.0)
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
    (0..count)
        .map(|slot| {
            if focusing && Some(slot) == focused {
                return whole(
                    (width - focus_width) / 2.0,
                    focus_top,
                    focus_width,
                    focus_height,
                );
            }
            let place = if focusing && focused.is_some_and(|focused| slot > focused) {
                slot - 1
            } else {
                slot
            };
            let x =
                (width - grid_width) / 2.0 + (place % columns.max(1)) as f32 * (tile_width + GAP);
            let y = first_row + (place / columns.max(1)) as f32 * (tile_height + GAP);
            whole(x, y, tile_width, tile_height)
        })
        .collect()
}

/// The pixels a tile of `bounds` shows at, while any of it is on a stage
/// `height` tall.
fn shown_at(bounds: Rectangle, height: f32) -> Option<(u32, u32)> {
    let pixels = |side: f32| (side.round() as u32).max(2);
    (bounds.y < height && bounds.y + bounds.height > 0.0)
        .then(|| (pixels(bounds.width), pixels(bounds.height)))
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

    /// The stage shows the call this account is in.
    pub fn stage_shown(&self) -> bool {
        let state = self.client.state();
        self.voice_selected() && state.voice.active() && state.voice.room == state.selected
    }

    fn stage_size(&self) -> Size {
        Size::new(
            self.window.width - rail::WIDTH - sidebar::WIDTH,
            self.window.height - header::HEIGHT - 1.0 - CONTROLS,
        )
    }

    /// The stage's tiles, the one shown large, and where each goes.
    fn stage(&self) -> (Vec<Slot>, Option<usize>, Vec<Rectangle>) {
        let participants = &self.client.state().voice.participants;
        let slots = slots(participants);
        let focused = self
            .focus
            .as_ref()
            .and_then(|focus| focus_slot(&slots, participants, focus));
        let tiles = tiles(slots.len(), focused, self.stage_size());
        (slots, focused, tiles)
    }

    /// Tells each camera where its tile shows, or that it doesn't, so frames
    /// are made only for tiles in sight, at their size, and whether it's
    /// large; and has [`video::frames`] look for frames from those that
    /// show. As the Slint client's `place_videos`, without its corner
    /// picture.
    pub fn place_videos(&mut self) {
        let shown = self.stage_shown();
        let (slots, focused, tiles) = self.stage();
        let height = self.stage_size().height;
        let voice = &self.client.state().voice;
        let mut sources = video::Sources::default();
        for (place, slot) in slots.iter().enumerate() {
            let Slot::Person(index) = *slot else {
                continue;
            };
            let person = &voice.participants[index];
            let Some(feed) = &person.video else {
                continue;
            };
            let at = shown.then(|| shown_at(tiles[place], height)).flatten();
            feed.show_at(at);
            feed.show_large(shown && focused == Some(place));
            if at.is_some() {
                sources.feeds.push((person.id.clone(), feed.clone()));
            }
        }
        if shown
            && let Some(watching) = &voice.watching
            && let Some(frames) = &watching.frames
        {
            sources.stream = Some((watching.user.clone(), frames.clone()));
        }
        // A message's video, wherever it plays.
        if let Some(playing) = &self.client.state().video
            && let Some(frames) = &playing.frames
        {
            sources.clip = Some((
                format!("{}/{}", playing.item, playing.index),
                frames.clone(),
            ));
        }
        // Frames of what no longer shows go.
        let stream_key = sources
            .stream
            .as_ref()
            .map(|(user, _)| format!("stream:{user}"));
        let clip_key = sources
            .clip
            .as_ref()
            .map(|(clip, _)| format!("clip:{clip}"));
        self.frames.retain(|key, _| {
            sources.feeds.iter().any(|(id, _)| id == key)
                || stream_key.as_deref() == Some(key)
                || clip_key.as_deref() == Some(key)
        });
        if clip_key.is_none() {
            self.clip_time = None;
        }
        self.video.set(sources);
    }

    /// New frames: each takes the place of its tile's last.
    pub fn frames_found(&mut self, found: Vec<Fresh>) {
        for fresh in found {
            if fresh.key.starts_with("stream:") {
                self.video.stream_taken();
            }
            if fresh.key.starts_with("clip:") {
                self.video.clip_taken();
                self.clip_time = fresh.time;
            }
            self.frames.insert(fresh.key, (fresh.handle, fresh.size));
        }
    }

    /// A tile was clicked, by its place, and is shown large; or beside the
    /// tiles, and none is.
    pub fn focus_tile(&mut self, place: Option<usize>) {
        let participants = &self.client.state().voice.participants;
        self.focus = place.and_then(|place| match *slots(participants).get(place)? {
            Slot::Stream(index) => Some(Focus::Stream(participants[index].id.clone())),
            Slot::Person(index) => {
                let person = &participants[index];
                let id = person.id.clone();
                Some(
                    if person.video.is_some() || !person.streaming || person.local {
                        Focus::Person(id)
                    } else {
                        Focus::Stream(id)
                    },
                )
            }
        });
        self.place_videos();
    }

    pub fn call_view(&self) -> Element<'_, Message> {
        let name = &self.header.name;
        let theme = &self.theme;
        let head = container(
            row![
                text("🔊").size(18),
                text(name.as_str()).font(BOLD).color(theme.text),
                Space::new().width(Length::Fill)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([0, 16])
        .width(Length::Fill)
        .height(header::HEIGHT)
        .align_y(Alignment::Center);
        let outline = theme.outline_chat;
        let line = bar(outline, Length::Fill, 1);
        if !self.stage_shown() {
            let id = self.client.state().selected.clone().unwrap_or_default();
            let (accent, accent_hover) = (theme.accent, theme.accent_hover);
            let join = button(text("Join voice").font(BOLD).color(theme.on_accent))
                .padding([10, 18])
                .on_press(Message::JoinVoice(id))
                .style(lit_style(Some(accent), accent_hover, 8.0));
            let panel = column![
                text(name.as_str()).size(26).font(BOLD).color(theme.text),
                join
            ]
            .spacing(14)
            .align_x(Alignment::Center);
            return column![head, line, container(panel).center(Length::Fill)].into();
        }
        let danger = theme.danger;
        let leave = button(self.glyph(Icon::X, 24, theme.on_danger).boxed(Length::Fill))
            .width(76)
            .height(48)
            .on_press(Message::LeaveCall)
            .style(move |_, _| button::Style {
                background: Some(danger.into()),
                border: Border::default().rounded(24),
                ..Default::default()
            });
        let controls = container(leave)
            .center_x(Length::Fill)
            .height(CONTROLS)
            .align_y(Alignment::Center);
        // The window's ground is the chat's; the stage paints its own only
        // where a theme gives it another (none built in does), as a ground
        // under the video is drawn again with every frame shown.
        let stage = (theme.stage != theme.chat).then_some(theme.stage);
        container(column![head, line, self.stage_view(), controls])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| stage.map(fill).unwrap_or_default())
            .into()
    }

    fn stage_view(&self) -> Element<'_, Message> {
        let size = self.stage_size();
        let (slots, focused, tiles) = self.stage();
        // A click beside the large tile shows the grid again.
        let mut layers = stack![
            mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(
                if focused.is_some() {
                    Message::FocusTile(None)
                } else {
                    Message::Nothing
                }
            )
        ];
        for (place, (slot, bounds)) in slots.iter().zip(&tiles).enumerate() {
            if bounds.y >= size.height {
                continue;
            }
            let tile = self.tile(place, *slot, *bounds, focused == Some(place));
            layers = layers.push(pin(tile).x(bounds.x).y(bounds.y));
        }
        container(layers.width(size.width).height(size.height))
            .width(size.width)
            .height(size.height)
            .clip(true)
            .into()
    }

    /// One tile: a camera's frame, a stream's, or the person's picture, with
    /// their name at its foot.
    fn tile(
        &self,
        place: usize,
        slot: Slot,
        bounds: Rectangle,
        large: bool,
    ) -> Element<'_, Message> {
        let voice = &self.client.state().voice;
        let watch_user = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        let size = bounds.size();
        let (index, own_stream) = match slot {
            Slot::Stream(index) => (index, true),
            Slot::Person(index) => (index, false),
        };
        let person = &voice.participants[index];
        let theme = &self.theme;
        let tile = Tile {
            theme,
            person,
            own_stream,
            streams_here: !own_stream
                && person.streaming
                && !person.local
                && person.video.is_none(),
            watched: false,
            size,
        };
        let tile = Tile {
            watched: (own_stream || tile.streams_here) && watch_user == Some(person.id.as_str()),
            ..tile
        };
        let on_press = if large {
            Message::Nothing
        } else if (own_stream || tile.streams_here) && !tile.watched {
            Message::Watch(person.id.clone())
        } else {
            Message::FocusTile(Some(place))
        };
        // A frame draws its own bars, so its tile has no ground and its
        // pixels are written once.
        let video = |frame: Option<&(iced::advanced::image::Handle, (u32, u32))>| {
            frame.map(|(handle, size)| {
                Element::from(
                    Video::new(handle.clone(), *size, STAGE_BLACK).corners(12, rgb(theme.stage)),
                )
            })
        };
        let (content, ground) = if own_stream || tile.watched {
            if tile.watched
                && let Some(frame) = video(self.frames.get(&format!("stream:{}", person.id)))
            {
                (frame, None)
            } else if tile.watched {
                let starting =
                    container(text("Starting the stream…").size(14).color(theme.secondary))
                        .center(Length::Fill);
                (starting.into(), Some(STAGE_BLACK))
            } else {
                let watch = container(pill(theme, "Watch stream")).center(Length::Fill);
                (watch.into(), Some(STAGE_BLACK))
            }
        } else if person.video.is_some() {
            match video(self.frames.get(&person.id)) {
                Some(frame) => (frame, None),
                None => (
                    Space::new().width(Length::Fill).height(Length::Fill).into(),
                    Some(STAGE_BLACK),
                ),
            }
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
            let content = container(avatar)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .padding(Padding {
                    bottom: 16.0,
                    ..Padding::ZERO
                });
            (content.into(), Some(theme.raised))
        };
        tile.face(content, ground, on_press)
    }
}

/// What a tile shows of someone, beside what fills it.
struct Tile<'a> {
    theme: &'a Theme,
    person: &'a Participant,
    /// The tile is their stream's own.
    own_stream: bool,
    /// They stream and don't film: their one tile is the stream's.
    streams_here: bool,
    watched: bool,
    size: Size,
}

impl<'a> Tile<'a> {
    /// The tile round `content`: the name at its foot, LIVE while they
    /// stream, and a stream's Watch button.
    fn face(
        self,
        content: Element<'a, Message>,
        ground: Option<Color>,
        on_press: Message,
    ) -> Element<'a, Message> {
        let person = self.person;
        let mut chip = row![
            text(person.user.as_str())
                .size(14)
                .font(BOLD)
                .color(Color::WHITE)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if person.streaming && (person.local || person.video.is_none() || self.own_stream) {
            chip = chip.push(live_pill(self.theme));
        }
        let chip = container(chip)
            .padding([0, 8])
            .height(26)
            .align_y(Alignment::Center)
            .style(|_| {
                rounded(
                    Color {
                        a: 0.65,
                        ..Color::BLACK
                    },
                    6,
                )
            });
        let mut over = stack![pin(chip).x(10).y(self.size.height - 36.0)];
        if self.streams_here && !self.watched {
            over = over.push(
                container(pill(self.theme, "Watch stream"))
                    .align_right(Length::Fill)
                    .padding(10),
            );
        }
        let face = container(stack![
            content,
            Above::new(over.width(Length::Fill).height(Length::Fill), 0.0)
        ])
        .width(self.size.width)
        .height(self.size.height)
        .clip(true)
        .style(move |_| container::Style {
            background: ground.map(Into::into),
            border: Border::default().rounded(12),
            ..Default::default()
        });
        mouse_area(face).on_press(on_press).into()
    }
}

/// The red "Watch stream" button on a stream's tile.
fn pill<'a>(theme: &Theme, label: &'static str) -> Element<'a, Message> {
    let danger = theme.danger;
    container(text(label).size(13).font(BOLD).color(theme.on_danger))
        .padding([0, 10])
        .height(26)
        .align_y(Alignment::Center)
        .style(move |_| rounded(danger, 13))
        .into()
}

fn live_pill<'a>(theme: &Theme) -> Element<'a, Message> {
    let danger = theme.danger;
    container(text("LIVE").size(10).font(BOLD).color(theme.on_danger))
        .padding([1, 4])
        .style(move |_| rounded(danger, 4))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tiles_sit_side_by_side_and_one_large_puts_the_other_under_it() {
        let stage = Size::new(1500.0, 800.0);
        let grid = tiles(2, None, stage);
        assert_eq!(grid[0].y, grid[1].y);
        assert!(grid[0].x + grid[0].width <= grid[1].x);
        let focused = tiles(2, Some(1), stage);
        assert!(focused[1].width > focused[0].width);
        assert!(focused[0].y > focused[1].y + focused[1].height);
    }

    #[test]
    fn a_tile_below_the_stage_has_no_frames_made() {
        let below = Rectangle::new(Point::new(0.0, 600.0), Size::new(300.0, 168.0));
        assert_eq!(shown_at(below, 598.0), None);
        let half = Rectangle::new(Point::new(0.0, 500.0), Size::new(300.4, 168.6));
        assert_eq!(shown_at(half, 598.0), Some((300, 169)));
    }
}
