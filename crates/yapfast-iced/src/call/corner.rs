//! The call off screen and over it: the stream watched, this account's
//! own or someone's camera kept in a corner of the window while the call
//! isn't on screen, as the Slint client's `StreamPip` (a click returns to
//! the call, a drag moves it to the nearest corner); and the call's large
//! tile over the whole window, full screen.

use iced::{
    Alignment, Background, Color, Element, Length, Padding, Point, Size, gradient,
    widget::{Space, column, container, mouse_area, pin, stack, text},
};
use yapfast_core::voice::SharePhase;

use super::{
    Event,
    stage::{CORNER, OWN_SCREEN, Pip, share_status},
};
use crate::{
    app::{App, Message},
    controls,
    fonts::BOLD,
    header,
    pictures::Icon,
    tile::{self, GLASS, NOTE},
    viewer::Viewer,
    widgets::Hover,
};

/// From the window's edges, and the corner's own corners.
const MARGIN: f32 = 16.0;
const RADIUS: f32 = 8.0;

impl App {
    /// The corners the picture may sit in: its left, right, top and bottom
    /// as the window has room.
    fn corners(&self) -> (f32, f32, f32, f32) {
        let left = self.chat_left() + MARGIN;
        let right = self.window.width - CORNER.width - MARGIN;
        let top = header::HEIGHT + MARGIN;
        let bottom = self.window.height - CORNER.height - MARGIN;
        (left, right, top, bottom)
    }

    /// The picture let go after a drag: it goes to the nearest corner.
    pub(super) fn drop_corner(&mut self) {
        let (left, right, top, bottom) = self.corners();
        let at = self.corner_at();
        self.call.left = at.x + CORNER.width / 2.0 < (left + right + CORNER.width) / 2.0;
        self.call.bottom = at.y + CORNER.height / 2.0 >= (top + bottom + CORNER.height) / 2.0;
        self.call.drag = None;
    }

    /// Where the picture is: in its corner, moved as far as it's dragged.
    fn corner_at(&self) -> Point {
        let (left, right, top, bottom) = self.corners();
        let x = if self.call.left { left } else { right };
        let y = if self.call.bottom { bottom } else { top };
        let by = self.call.drag.unwrap_or_default();
        Point::new(x + by.x, y + by.y)
    }

    /// The picture in its corner, while the call is off screen.
    pub fn corner_view(&self) -> Option<Element<'_, Message>> {
        let pip = self.corner();
        let voice = &self.client.state().voice;
        let (title, status, key, live, stop, viewers) = match pip {
            Pip::None => return None,
            Pip::Watched => {
                let watching = voice.watching.as_ref()?;
                (
                    format!("{}'s screen", watching.name),
                    self.watch_status(),
                    format!("stream:{}", watching.user),
                    false,
                    Some((Icon::X, "Stop watching", Event::StopWatching)),
                    self.viewers(&watching.viewers, Some(&watching.user)),
                )
            }
            Pip::Shared => {
                let sharing = voice.sharing.as_ref()?;
                (
                    "Your screen".to_owned(),
                    share_status(voice).to_owned(),
                    OWN_SCREEN.to_owned(),
                    sharing.phase == SharePhase::Live,
                    Some((Icon::ScreenShareOff, "Stop sharing", Event::StopSharing)),
                    self.viewers(&sharing.viewers, None),
                )
            }
            Pip::Camera(index) => {
                let person = voice.participants.get(index)?;
                (
                    person.user.clone(),
                    String::new(),
                    person.id.clone(),
                    false,
                    None,
                    None,
                )
            }
        };
        let theme = &self.theme;
        let content = self.frame(&key, None).unwrap_or_else(|| {
            container(text(status).size(14).color(NOTE).align_x(Alignment::Center))
                .padding([0, 16])
                .center(Length::Fill)
                .into()
        });
        let mut base = stack![content].width(CORNER.width).height(CORNER.height);
        if live {
            base = base.push(pin(controls::live_pill(theme, None)).x(8).y(8));
        }
        if let Some(viewers) = viewers {
            base = base.push(
                container(viewers)
                    .padding(8)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            );
        }
        // Its title over a shade along its foot, and its stop, while the
        // pointer is on it and it isn't dragged.
        let shade = Background::Gradient(
            gradient::Linear::new(std::f32::consts::PI)
                .add_stop(0.55, Color::TRANSPARENT)
                .add_stop(1.0, Color::from_rgba8(0, 0, 0, 0xb0 as f32 / 255.0))
                .into(),
        );
        let mut over = stack![
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(shade),
                    ..Default::default()
                }),
            pin(container(
                text(title)
                    .size(14)
                    .font(BOLD)
                    .color(Color::WHITE)
                    .wrapping(text::Wrapping::None)
            )
            .width(CORNER.width - 72.0)
            .height(24)
            .clip(true)
            .align_y(Alignment::Center))
            .x(12)
            .y(CORNER.height - 34.0),
        ]
        .width(CORNER.width)
        .height(CORNER.height);
        if let Some((icon, label, event)) = stop {
            over = over.push(
                container(tile::glass_button(self, icon, label, Message::Call(event)))
                    .padding(8)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End),
            );
        }
        let dragging = self.call.drag.is_some();
        let face: Element<'_, Message> = if dragging {
            base.into()
        } else {
            Hover::new(base, over).into()
        };
        let edge = theme.edge;
        let framed = container(face)
            .width(CORNER.width)
            .height(CORNER.height)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(Color::BLACK.into()),
                border: crate::style::edge(edge, RADIUS),
                ..Default::default()
            });
        let held = mouse_area(framed)
            .on_press(Message::Call(Event::Grab))
            .interaction(if dragging {
                iced::mouse::Interaction::Grabbing
            } else {
                iced::mouse::Interaction::Pointer
            });
        let at = self.corner_at();
        Some(
            stack![pin(held).x(at.x).y(at.y)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        )
    }

    /// The call's large tile over the whole window, in full screen, until
    /// Escape, its button or a double click.
    pub fn stage_full_view(&self) -> Option<Element<'_, Message>> {
        if !self.full_shown() {
            return None;
        }
        Some(
            Viewer::new(
                Color::BLACK,
                self.stage_view(),
                self.window,
                Message::Nothing,
            )
            .view(self),
        )
    }

    /// A message's video full screen, as the Slint client's `VideoViewer`:
    /// its name and Exit full screen over it, the video fitted between,
    /// its controls under it.
    pub fn clip_viewer(&self) -> Option<Element<'_, Message>> {
        if !self.clip_full {
            return None;
        }
        let video = self.client.state().video.as_ref()?;
        let size = crate::viewer::fitted(self.window, video.size);
        let key = format!("clip:{}/{}", video.item, video.index);
        let mut screen = stack![
            container(Space::new())
                .width(size.width)
                .height(size.height)
                .style(|_| crate::style::fill(Color::BLACK))
        ];
        if let Some(frame) = self.frame(&key, None) {
            screen = screen.push(frame);
        }
        if let Some(error) = &video.error {
            screen = screen.push(
                container(
                    container(text(error.as_str()).size(13).color(Color::WHITE))
                        .padding([0, 10])
                        .height(28)
                        .align_y(Alignment::Center)
                        .style(|_| crate::style::rounded(Color::from_rgba8(0, 0, 0, 0.8), 6)),
                )
                .center(Length::Fill),
            );
        } else if video.loading {
            screen = screen.push(
                container(
                    container(text("Loading...").size(13).color(Color::WHITE))
                        .padding([0, 10])
                        .height(28)
                        .align_y(Alignment::Center)
                        .style(|_| crate::style::rounded(GLASS, 14)),
                )
                .center(Length::Fill),
            );
        }
        let screen = mouse_area(screen.width(size.width).height(size.height))
            .on_press(Message::Clip(crate::playback::Clip::Toggle))
            .interaction(iced::mouse::Interaction::Pointer);
        let name = self
            .client
            .state()
            .message(&video.item)
            .and_then(|message| message.attachments.get(video.index))
            .map(|attachment| attachment.name.clone())
            .unwrap_or_default();
        let close = self.icon_face_button(
            self.glyph(Icon::X, 20, Color::WHITE),
            "Exit full screen",
            32.0,
            Message::Clip(crate::playback::Clip::Full(false)),
            true,
        );
        let width = size.width.max(360.0);
        Some(
            Viewer::new(
                Color::from_rgba8(0, 0, 0, 0xe6 as f32 / 255.0),
                screen,
                Size::new(size.width, size.height),
                Message::Clip(crate::playback::Clip::Full(false)),
            )
            .title(name, vec![close])
            .foot(column![self.clip_controls(video, width, true)].padding(Padding::ZERO))
            .view(self),
        )
    }
}
