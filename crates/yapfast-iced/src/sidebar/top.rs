//! The sidebar's top: a server's banner and name, its boost goal and
//! events, or the DMs' search.

use iced::{
    Alignment, Background, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, stack, text, text_input},
};

use crate::{
    app::{App, Message},
    controls::{bar, lit_style},
    elide::elide,
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::{corners, faded, rounded},
    widgets::Pic,
};

use super::{
    BANNER, BOOST_GOAL, BOOST_PILL, BOOST_PINK, BOOST_ROW, EVENTS_ROW, Event, FILTER, HEADER,
    INSET, INVITE, WIDTH,
};

impl App {
    /// The sidebar's top: a server's name (with Invite people beside it),
    /// or the DMs' search field, over the outline; with a banner, the
    /// header grows over it, shaded at its top, with no line under it.
    pub(super) fn side_header_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let head = &self.side_header;
        let inner: Element<'_, Message> = match &head.server {
            None => {
                let (ground, ink, hint, selection) =
                    (theme.field, theme.text, theme.muted, theme.selection);
                text_input("Find a conversation", &self.filter)
                    .id(FILTER)
                    .on_input(|filter| Message::Side(Event::Filter(filter)))
                    .size(13)
                    .padding([8, 10])
                    .style(move |_, _| text_input::Style {
                        background: ground.into(),
                        border: iced::Border::default().rounded(6),
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
                        self.look()
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
        if let Some(picture) = self.picture(banner, WIDTH as u32, height as u32, true) {
            layers = layers.push(
                Pic::new(picture, iced::Size::new(WIDTH, height))
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
                        border: iced::Border::default().rounded(corners([12.0, 0.0, 0.0, 0.0])),
                        ..Default::default()
                    }),
            )
            .push(bar_);
        layers.width(Length::Fill).height(height).into()
    }

    /// Discord's boost goal under the server's name: a pill whose pink
    /// fill is the share of the goal the boosts reach, "Boost Goal" and
    /// how many of the goal's boosts, with a line under the row.
    pub(super) fn boost_goal(&self, boosts: u32) -> Element<'_, Message> {
        let theme = &self.theme;
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
                        border: iced::Border::default().rounded(inner / 2.0),
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
            self.glyph(Icon::ChevronRight, 16, theme.muted),
        ]
        .spacing(10)
        .padding([0, 8])
        .height(BOOST_PILL)
        .align_y(Alignment::Center);
        let hover = theme.hover;
        let pill = container(stack![pill, words])
            .width(Length::Fill)
            .height(BOOST_PILL)
            .style(move |_| rounded(hover, BOOST_PILL / 2.0));
        column![
            container(pill)
                .padding(Padding {
                    top: 4.0,
                    right: 4.0,
                    bottom: 0.0,
                    left: 12.0,
                })
                .height(BOOST_ROW - 1.0),
            container(bar(theme.outline, Length::Fill, 1)).padding(Padding {
                left: INSET,
                ..Padding::ZERO
            })
        ]
        .into()
    }

    /// Discord's "N Events" row under the server's name, which opens the
    /// server's scheduled events under it.
    pub(super) fn events_row(&self, events: usize) -> Element<'_, Message> {
        let theme = &self.theme;
        let label = match events {
            0 => "Events".to_owned(),
            1 => "1 Event".to_owned(),
            count => format!("{count} Events"),
        };
        let (hover, muted) = (theme.hover, theme.muted);
        let face = button(
            row![
                self.glyph(Icon::Calendar, 20, muted),
                text(label)
                    .size(14)
                    .font(MEDIUM)
                    .color(muted)
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

    /// A server's name at the top of its channels, as the Slint client's
    /// `ServerHeader`: its Community badge, the name and a chevron, lit
    /// under the pointer or while its menu is open, when the chevron turns.
    pub(super) fn server_name_button(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let head = &self.side_header;
        let open = self.menu.as_ref().is_some_and(|menu| menu.header);
        // Over the banner, in white.
        let on_banner = head.banner.is_some();
        let (ink, chevron) = if on_banner {
            (Color::WHITE, Color::WHITE)
        } else {
            (theme.text, theme.secondary)
        };
        let mut face = row![].spacing(10).align_y(Alignment::Center);
        // Discord marks a verified server with its green badge, and one
        // that is only a Community server with the Community badge.
        if head.verified {
            face = face.push(self.drawn(Icon::Verified, 16));
        } else if head.community {
            face = face.push(self.glyph(Icon::Community, 16, ink));
        }
        face = face.push(
            row![
                elide(head.name.as_str()).size(15).font(SEMIBOLD).color(ink),
                self.glyph(
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
            left: if head.community || head.verified {
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
