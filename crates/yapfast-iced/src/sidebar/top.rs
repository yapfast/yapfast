//! The sidebar's top: a server's banner and name, its boost goal and
//! events, or the DMs' search.

use iced::{
    Alignment, Background, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, stack, text, text_input},
};

use crate::{
    app::Message,
    controls::{bar, lit_style},
    elide::elide,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::{corners, faded, rounded},
    widgets::Pic,
};

use super::{
    BANNER, BOOST_GOAL, BOOST_PILL, BOOST_PINK, BOOST_ROW, EVENTS_ROW, Event, FILTER, HEADER,
    INSET, INVITE,
};

/// The number in a server's Boost level badge, measured (237, 237, 239).
const TIER_INK: Color = Color::from_rgb(237.0 / 255.0, 237.0 / 255.0, 239.0 / 255.0);

impl<'a> super::View<'a> {
    /// The sidebar's top: a server's name (with Invite people beside it),
    /// or the DMs' search field, over the outline; with a banner, the
    /// header grows over it, shaded at its top, with no line under it.
    pub(super) fn side_header_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let head = &self.s.header;
        let inner: Element<'a, Message> = match &head.server {
            None => {
                let (ground, ink, hint, selection) =
                    (theme.field, theme.text, theme.muted, theme.selection);
                text_input("Find a conversation", &self.s.filter)
                    .id(FILTER)
                    .on_input(|filter| Message::Side(Event::Filter(filter)))
                    .size(13)
                    .padding([8, 10])
                    .style(move |_, _| text_input::Style {
                        background: ground.into(),
                        border: crate::style::round(6),
                        icon: ink,
                        placeholder: hint,
                        value: ink,
                        selection,
                    })
                    .into()
            }
            Some(_) => {
                let mut line = row![self.server_name_button()].spacing(INSET);
                if head.invite {
                    line = line.push(
                        self.cx
                            .look
                            .icon_button(
                                Icon::UserPlus,
                                "Invite people",
                                Message::Side(Event::Invite),
                            )
                            .size(INVITE, 20)
                            .below(),
                    );
                }
                line.into()
            }
        };
        let bar_ = container(inner)
            .padding(INSET)
            .height(HEADER - 1.0)
            .width(Length::Fill);
        let banner = head.banner.as_deref().filter(|_| head.server.is_some());
        let Some(banner) = banner else {
            return column![bar_, bar(theme.outline, Length::Fill, 1)].into();
        };
        let height = HEADER + BANNER;
        let mut layers = stack![];
        let width = super::width(self.cx.layout);
        if let Some(picture) = self.cx.picture(banner, width as u32, height as u32, true) {
            layers = layers.push(
                Pic::new(picture, iced::Size::new(width, height))
                    .radius_each([12.0, 0.0, 0.0, 0.0]),
            );
        }
        let shade = Background::Gradient(
            iced::gradient::Linear::new(iced::Degrees(180.0))
                .add_stop(0.0, Color::from_rgba(0.0, 0.0, 0.0, 0.6))
                .add_stop(1.0, Color::TRANSPARENT)
                .into(),
        );
        layers = layers
            .push(
                container(Space::new())
                    .width(Length::Fill)
                    .height(HEADER)
                    .style(move |_| container::Style {
                        background: Some(shade),
                        border: crate::style::round(corners([12.0, 0.0, 0.0, 0.0])),
                        ..Default::default()
                    }),
            )
            .push(bar_);
        layers.width(Length::Fill).height(height).into()
    }

    /// Discord's boost goal under the server's name: a pill whose pink
    /// fill is the share of the goal the boosts reach, "Boost Goal" and
    /// how many of the goal's boosts, with a line under the row.
    pub(super) fn boost_goal(self, boosts: u32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let share = (boosts as f32 / BOOST_GOAL as f32).min(1.0);
        let fill = Background::Gradient(
            iced::gradient::Linear::new(iced::Degrees(270.0))
                .add_stop(
                    0.0,
                    Color {
                        a: 0.4,
                        ..BOOST_PINK
                    },
                )
                .add_stop(
                    1.0,
                    Color {
                        a: 0.1,
                        ..BOOST_PINK
                    },
                )
                .into(),
        );
        let inner = BOOST_PILL - 4.0;
        // The fill takes its share of the pill's inside, 2 px in.
        let filled = (share * 1000.0).round() as u16;
        let mut track = row![];
        if filled > 0 {
            track = track.push(
                container(Space::new())
                    .width(Length::FillPortion(filled))
                    .height(inner)
                    .style(move |_| container::Style {
                        background: Some(fill),
                        border: crate::style::round(inner / 2.0),
                        ..Default::default()
                    }),
            );
        }
        if filled < 1000 {
            track = track.push(Space::new().width(Length::FillPortion(1000 - filled)));
        }
        // The stack's first layer sets its size: the whole pill.
        let pill = container(track)
            .padding(2)
            .width(Length::Fill)
            .height(Length::Fill);
        let words = row![
            text("Boost Goal")
                .size(14)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            text(format!("{boosts}/{BOOST_GOAL} Boosts"))
                .size(14)
                .font(MEDIUM)
                .color(faded(theme.text, 0.7))
                .wrapping(text::Wrapping::None),
            self.cx.look.glyph(Icon::ChevronRight, 16, theme.muted),
        ]
        .spacing(10)
        .padding([0, 8])
        .height(BOOST_PILL)
        .align_y(Alignment::Center);
        // Discord's (32,32,36) on its dark theme: the field colour, a step
        // above the sheet.
        let ground = theme.field;
        let pill = container(stack![pill, words])
            .width(Length::Fill)
            .height(BOOST_PILL)
            .style(move |_| rounded(ground, BOOST_PILL / 2.0));
        column![
            container(pill)
                .padding(Padding {
                    top: 16.0,
                    right: 13.0,
                    bottom: 0.0,
                    left: 12.0,
                })
                .height(BOOST_ROW - 1.0),
            container(bar(theme.outline, Length::Fill, 1)).padding(Padding {
                left: INSET,
                right: 9.0,
                ..Padding::ZERO
            })
        ]
        .into()
    }

    /// Discord's "N Events" row under the server's name, which opens the
    /// server's scheduled events under it.
    pub(super) fn events_row(self, events: usize) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let label = match events {
            0 => "Events".to_owned(),
            1 => "1 Event".to_owned(),
            count => format!("{count} Events"),
        };
        let (hover, ink) = (theme.hover, theme.channel_read);
        let face = button(
            row![
                self.cx.look.glyph(Icon::CalendarFilled, 20, ink),
                text(label)
                    .size(14)
                    .font(MEDIUM)
                    .color(ink)
                    .wrapping(text::Wrapping::None)
            ]
            .spacing(8)
            .height(Length::Fill)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            left: INSET,
            right: INSET,
            ..Padding::ZERO
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .on_press(Message::Side(Event::Events))
        .style(lit_style(None, hover, 6.0));
        column![
            container(face)
                .padding(INSET)
                .height(EVENTS_ROW - 1.0)
                .width(Length::Fill),
            container(bar(theme.outline, Length::Fill, 1)).padding([0.0, 16.0])
        ]
        .into()
    }

    /// A server's name at the top of its channels: its Community badge,
    /// the name and a chevron, lit under the pointer or while its menu is
    /// open, when the chevron turns.
    pub(super) fn server_name_button(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let head = &self.s.header;
        let open = self.cx.menu.is_some_and(|menu| menu.header);
        // Over the banner, in white.
        let on_banner = head.banner.is_some();
        let (ink, chevron) = if on_banner {
            (Color::WHITE, Color::WHITE)
        } else {
            (theme.text, theme.secondary)
        };
        let mut face = row![].spacing(10).align_y(Alignment::Center);
        // Discord marks a verified server with its green badge, and one
        // that is only a Community server with the Community badge; any
        // other shows its boost level in a scallop.
        if head.verified {
            face = face.push(self.cx.look.drawn(Icon::Verified, 16));
        } else if head.community {
            face = face.push(self.cx.look.glyph(Icon::Community, 16, ink));
        } else if let Some(tier) = head.tier {
            face = face.push(
                stack![
                    self.cx.look.glyph(Icon::Scallop, 18, theme.nav_read),
                    container(text(tier.to_string()).size(11).font(BOLD).color(TIER_INK))
                        .center(Length::Fill)
                ]
                .width(18)
                .height(18),
            );
        }
        face = face.push(
            row![
                elide(head.name.as_str()).size(15).font(BOLD).color(ink),
                self.cx.look.glyph(
                    if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    16,
                    chevron
                ),
            ]
            .align_y(Alignment::Center),
        );
        let hover = theme.hover;
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding(Padding {
            left: if head.community || head.verified || head.tier.is_some() {
                10.0
            } else {
                8.0
            },
            right: 8.0,
            ..Padding::ZERO
        })
        .height(32)
        .width(Length::Fill)
        .on_press(Message::Side(Event::Header))
        .style(lit_style(open.then_some(hover), hover, 6.0))
        .into()
    }
}
