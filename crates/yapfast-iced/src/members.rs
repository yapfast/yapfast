//! The member list beside a server channel's timeline, virtualized, as the
//! Slint client's `MemberPanel` draws it: each group's heading, then its
//! members with their looks (`looks::MemberLook`).

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Rectangle, Size, Task,
    widget::{column, container, mouse_area, row, scrollable, text},
};
use yapfast_core::chat::Presence;

use crate::{
    app::{App, Message, key},
    controls::bar,
    fonts::{MEDIUM, SEMIBOLD, shown_name, style_font},
    header,
    looks::{self, Chip, MemberLine, MemberLook},
    pictures::{Face, Icon},
    profile,
    style::{CHIP, CROWN, HEADING, MEMBER_HOVER, MEMBER_OPEN, faded, fill, hex, scrollbar},
    theme::rgb,
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
        // As the Slint client's `MemberPanel` with its Discord look: the
        // chat's ground, an outline down its left edge.
        let (ground, edge) = (self.theme.chat, self.theme.outline_chat);
        let list = scrollable(rows)
            .on_scroll(Message::Members)
            .height(Length::Fill)
            .style(scrollbar(&self.theme));
        container(row![bar(edge, 1, Length::Fill), list])
            .width(WIDTH)
            .height(Length::Fill)
            .style(move |_| fill(ground))
            .into()
    }

    /// A group's heading: 14 px medium in the channel grey, 20 px above,
    /// its role's icon before it.
    fn heading_row(&self, heading: &str, icon: Option<&str>) -> Element<'_, Message> {
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(icon) = icon.and_then(|icon| self.picture(icon, 20, 20, false)) {
            line = line.push(Pic::square(icon, 20.0));
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
        let theme = &self.theme;
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
                .map(|presence| (presence, rgb(theme.sidebar), Point::new(20.0, 20.0))),
        });
        let color = faded(look.color.map_or(theme.secondary, hex), opacity);
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
            name_line = name_line.push(Pic::square(icon, 16.0).opacity(opacity));
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
                    fill: CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        if look.bot {
            name_line = name_line.push(self.bot_tag(opacity));
        }
        if look.owner {
            let crown = self.icon(Icon::Crown, 14, CROWN);
            name_line = name_line.push(Pic::square(crown, 14.0).opacity(opacity));
        }
        let mut lines = column![name_line];
        if !look.status.words.is_empty() || look.status.emoji.is_some() {
            lines = lines.push(self.status_line(&look.status, opacity));
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
        // The ground the nameplate fades into, as the Slint client's
        // `PlateRow` has it.
        let ground = if selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
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
                        Some(MEMBER_OPEN.into())
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
