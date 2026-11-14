//! The member list beside a server channel's timeline, virtualized, as the
//! Slint client's `MemberPanel` draws it: each group's heading, then its
//! members with their looks (`looks::MemberLook`).

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size, Task,
    widget::{Space, column, container, mouse_area, row, scrollable, text},
};
use yapfast_core::chat::Presence;

use crate::{
    app::{App, Message, key},
    fonts::{MEDIUM, SEMIBOLD, shown_name, style_font},
    header,
    looks::{self, Chip, MemberLine, MemberLook},
    pictures::{Face, Icon, Mark},
    profile,
    style::{
        ACTIVITY, BACKGROUND, HEADING, LIVE, MEMBER_GROUND, MEMBER_HOVER, SECONDARY, TEXT, blend,
        faded, fill, hex, scrollbar,
    },
    widgets::{Pic, PlateRow, Rows},
};

pub const WIDTH: f32 = 265.0;
/// Where each line starts after the last, and the room above the first.
pub const PITCH: f32 = 44.0;
pub const TOP: f32 = 8.0;
/// A line's height.
const ROW: f32 = 42.0;

impl App {
    pub fn members_view(&self) -> Element<'_, Message> {
        let (first, last) = self.member_scroll.range(self.lines.len(), PITCH, TOP);
        let total = TOP + self.lines.len() as f32 * PITCH + 10.0;
        let mut rows = Rows::new(total);
        for (index, line) in self.lines.iter().enumerate().take(last).skip(first) {
            let top = TOP + index as f32 * PITCH;
            let (id, row) = match line {
                MemberLine::Heading { text, icon } => {
                    (text.as_str(), self.heading_row(text, icon.as_deref()))
                }
                MemberLine::Member(look) => (look.id.as_str(), self.member_row(index, look)),
            };
            rows.push(key(id), top, ROW, container(row).padding([0, 8]));
        }
        container(
            scrollable(rows)
                .on_scroll(Message::Members)
                .height(Length::Fill)
                .style(scrollbar),
        )
        .width(WIDTH)
        .height(Length::Fill)
        .style(|_| fill(BACKGROUND))
        .into()
    }

    /// A group's heading: 14 px medium in the channel grey, 20 px above,
    /// its role's icon before it.
    fn heading_row(&self, heading: &str, icon: Option<&str>) -> Element<'_, Message> {
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(icon) = icon.and_then(|icon| self.picture(icon, 20, 20, false)) {
            line = line.push(Pic::new(icon, Size::new(20.0, 20.0)));
        }
        line = line.push(
            text(heading.to_owned())
                .size(14)
                .font(MEDIUM)
                .color(hex(HEADING)),
        );
        container(line)
            .padding(Padding {
                top: 20.0,
                right: 0.0,
                bottom: 4.0,
                left: 8.0,
            })
            .height(ROW)
            .align_y(Alignment::Center)
            .into()
    }

    /// A member's row: the face with its decoration (playing while pointed
    /// at) and presence, the name in its style's font and its role's
    /// colour, the role icon, server tag, APP tag and crown, the activity
    /// line, and the nameplate behind it all.
    fn member_row<'a>(&'a self, index: usize, look: &'a MemberLook) -> Element<'a, Message> {
        let pointed = self.pointed_member.as_ref() == Some(&look.id);
        // The member whose card is open.
        let selected = self.card.as_ref().is_some_and(|card| card.user == look.id);
        let offline = look.offline();
        let (avatar_opacity, opacity) = if offline { (0.4, 0.5) } else { (1.0, 1.0) };
        let face = self.face(Face {
            url: look.avatar.as_deref(),
            name: &look.name,
            size: 32.0,
            opacity: avatar_opacity,
            decoration: look.decoration.as_deref(),
            play: pointed,
            dot: look
                .presence
                .filter(|presence| *presence != Presence::Offline)
                .map(|presence| (presence, MEMBER_GROUND, Point::new(20.0, 20.0))),
        });
        let color = faded(look.color.map_or(hex(SECONDARY), hex), opacity);
        let mut name_line = row![
            text(shown_name(&look.name, look.style.as_ref()))
                .size(16)
                .font(style_font(look.style.as_ref(), 500))
                .color(color)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some(icon) = look
            .role_icon
            .as_deref()
            .and_then(|icon| self.picture(icon, 16, 16, false))
        {
            name_line = name_line.push(Pic::new(icon, Size::new(16.0, 16.0)).opacity(opacity));
        }
        if let Some((letters, badge)) = &look.tag {
            name_line = name_line.push(self.tag_chip(
                letters,
                badge.as_deref(),
                Chip {
                    size: 12.0,
                    font: SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: Color::from_rgba8(0x97, 0x97, 0x9f, 0.2),
                    ink: hex(TEXT),
                },
                opacity,
            ));
        }
        if look.bot {
            name_line = name_line.push(self.bot_tag(opacity));
        }
        if look.owner {
            let crown = self.icon(Icon::Crown, 14, 0xf0b232);
            name_line = name_line.push(Pic::new(crown, Size::new(14.0, 14.0)).opacity(opacity));
        }
        let mut lines = column![name_line];
        if !look.activity.is_empty() || look.activity_emoji.is_some() {
            lines = lines.push(self.status_line(look, opacity));
        }
        let plate = look.nameplate.as_ref().and_then(|(url, _)| {
            let picture = self.client.state().pictures.get(&**url)?;
            let width = (ROW * picture.width as f32 / picture.height.max(1) as f32).round();
            let size = Size::new(width, ROW);
            let strength = if pointed || selected { 0.9 } else { 0.6 } * opacity;
            let moving = pointed
                .then(|| {
                    self.moving(
                        &looks::animated_url(url),
                        size,
                        Rectangle::with_size(size),
                        false,
                    )
                })
                .flatten();
            Some(match moving {
                Some(moving) => moving.radius(8.0).opacity(strength).into(),
                None => Element::from(
                    Pic::new(
                        self.fit(picture, size.width as u32, size.height as u32, false)?,
                        size,
                    )
                    .radius(8.0)
                    .opacity(strength),
                ),
            })
        });
        let content = row![face, lines.width(Length::Shrink)]
            .width(Length::Fill)
            .spacing(12)
            .padding(Padding {
                top: 0.0,
                right: if look.nameplate.is_some() { 12.0 } else { 8.0 },
                bottom: 0.0,
                left: 8.0,
            })
            .height(ROW)
            .align_y(Alignment::Center);
        let ground = if pointed {
            blend(MEMBER_GROUND, MEMBER_HOVER)
        } else {
            hex(MEMBER_GROUND)
        };
        let palette = look
            .nameplate
            .as_ref()
            .and_then(|(_, palette)| *palette)
            .map(hex);
        let end = if selected {
            0.5
        } else if pointed {
            0.3
        } else {
            0.2
        };
        let plated = PlateRow::new(content, plate, palette, ground, end, opacity);
        let id = look.id.clone();
        mouse_area(
            container(plated)
                .width(Length::Fill)
                .height(ROW)
                .clip(true)
                .style(move |_| container::Style {
                    background: if selected {
                        Some(
                            Color {
                                a: 0x33 as f32 / 255.0,
                                ..MEMBER_HOVER
                            }
                            .into(),
                        )
                    } else {
                        pointed.then(|| MEMBER_HOVER.into())
                    },
                    border: Border::default().rounded(8),
                    ..Default::default()
                }),
        )
        .on_enter(Message::PointMember(id.clone(), true))
        .on_exit(Message::PointMember(id, false))
        .on_press(Message::OpenMember(index))
        .into()
    }

    /// What someone is up to, under their name: the activity's icon, a
    /// bullet, a custom emoji's picture, the words, 12 px.
    fn status_line<'a>(&'a self, look: &'a MemberLook, opacity: f32) -> Element<'a, Message> {
        let mut line = row![].spacing(4).align_y(Alignment::Center);
        let side = 15.0;
        let ink = faded(hex(ACTIVITY), opacity);
        if let Some(icon) = Icon::activity(look.activity_icon) {
            // Discord draws the game controller in its online green.
            let color = if icon == Icon::Gamepad {
                LIVE
            } else {
                0x949599
            };
            if let Some(icon) = self.mark(Mark::Icon(icon, side as u32, color)) {
                line = line.push(Pic::new(icon, Size::new(side, side)).opacity(opacity));
            }
            if look.activity_bullet {
                line = line.push(text("\u{2022}").size(12).color(ink));
            }
        }
        if let Some(emoji) = &look.activity_emoji {
            line = line.push(match self.picture(emoji, side as u32, side as u32, false) {
                Some(picture) => {
                    Element::from(Pic::new(picture, Size::new(side, side)).opacity(opacity))
                }
                None => Space::new().width(side).height(side).into(),
            });
        }
        if !look.activity.is_empty() {
            line = line.push(
                text(look.activity.as_str())
                    .size(12)
                    .color(ink)
                    .wrapping(text::Wrapping::None),
            );
        }
        line.into()
    }

    /// A member's line was clicked: their card opens beside the list, its
    /// top 16 px above the row's.
    pub fn open_member(&mut self, index: usize) -> Task<Message> {
        let Some(MemberLine::Member(look)) = self.lines.get(index) else {
            return Task::none();
        };
        let top = header::HEIGHT + 1.0 + TOP + index as f32 * PITCH - self.member_scroll.offset;
        let at = Point::new(self.window.width - WIDTH - 8.0 - profile::WIDTH, top - 16.0);
        let user = look.id.clone();
        self.open_card(user, at)
    }
}
