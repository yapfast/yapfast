//! Who is in a voice channel, under its row and on its card.

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, column, container, mouse_area, row, text, tooltip},
};
use yapfast_core::chat::VoicePerson;

use crate::{
    app::Message,
    controls::{self},
    elide::elide,
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::{faded, rounded},
    widgets::{Drags, RightPress},
};

use super::{Event, PERSON, VOICE_CARD_PEOPLE, VOICE_CARD_WIDTH};

impl<'a> super::View<'a> {
    /// Who is in a busy voice channel, beside its row: a line summing them
    /// up, then each person with their camera and LIVE, the first eight
    /// and how many more. This account on another client is dimmed, without
    /// its marks.
    pub(super) fn voice_channel_card(self, people: &'a [VoicePerson]) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let summary = yapfast_core::voice_roster::Roster::of(people).summary();
        let mut card = column![
            elide(summary)
                .size(theme.type_size("small", 12.0))
                .font(theme.font("small", BOLD))
                .color(theme.secondary)
        ]
        .spacing(4)
        .padding(Padding {
            top: 10.0,
            ..Padding::new(12.0)
        })
        .width(VOICE_CARD_WIDTH);
        for person in people.iter().take(VOICE_CARD_PEOPLE) {
            let opacity = if person.elsewhere { 0.5 } else { 1.0 };
            let mut line = row![
                self.cx
                    .avatar(person.avatar.as_deref(), &person.name, 20.0, 10.0, opacity),
                elide(person.name.as_str())
                    .size(theme.type_size("names", 14.0))
                    .font(theme.font("names", MEDIUM))
                    .color(faded(theme.text, opacity)),
            ]
            .spacing(8)
            .height(26)
            .align_y(Alignment::Center);
            if !person.elsewhere {
                if person.video {
                    line = line.push(self.cx.look.glyph(Icon::Video, 16, theme.muted));
                }
                if person.streaming {
                    let activity_themed = self
                        .cx
                        .themes
                        .content(&theme.id, self.cx.layout.theme_content.contains(&theme.id))
                        .is_some_and(|c| {
                            c.activity == yapfast_core::themes::content::Follow::Theme
                        });
                    line = line.push(controls::live_pill_in(theme, activity_themed, None));
                }
            }
            card = card.push(line);
        }
        let more = people.len().saturating_sub(VOICE_CARD_PEOPLE);
        if more > 0 {
            card = card.push(
                text(format!("and {more} more"))
                    .size(theme.type_size("small", 12.0))
                    .font(theme.font("small", crate::fonts::REGULAR))
                    .color(theme.muted),
            );
        }
        let tip = controls::tip_wrap(theme, container(card).style(controls::tip_style(theme)));
        controls::themed_outline(
            tip,
            theme,
            "tooltip",
            theme.line,
            1.0,
            theme.surface_radius("tooltip", 6.0),
        )
    }

    /// Someone in a voice channel, under its row: their avatar and name, and
    /// their camera, LIVE (which watches their stream), and mic and
    /// headphone marks. In the call this account is in, `live` says who
    /// speaks, ringed and bold, and who lost their connection, faint. A
    /// right click opens their voice menu.
    pub(super) fn voice_person(
        self,
        room: &str,
        person: &'a VoicePerson,
        live: Option<&yapfast_core::voice::Participant>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let id = format!("person:{room}:{}", person.id);
        let pointed = self.cx.pointed.side.as_deref() == Some(id.as_str());
        let opacity = if person.elsewhere && !pointed {
            0.5
        } else {
            1.0
        };
        let speaking = live.is_some_and(crate::call::speaking);
        let ink = if speaking {
            theme.text
        } else if live.is_some_and(|live| !live.connected) {
            theme.muted
        } else {
            theme.secondary
        };
        let avatar = self
            .cx
            .avatar(person.avatar.as_deref(), &person.name, 22.0, 11.0, opacity);
        // The ring of someone speaking, round the avatar, outside it.
        let avatar: Element<'a, Message> =
            if speaking {
                let ring = theme.live;
                iced::widget::stack![
                    avatar,
                    iced::widget::pin(container(Space::new()).width(28).height(28).style(
                        move |_| iced::widget::container::Style {
                            border: iced::Border {
                                color: ring,
                                width: 2.0,
                                radius: 14.0.into(),
                            },
                            ..Default::default()
                        }
                    ))
                    .x(-3.0)
                    .y(-3.0),
                ]
                .into()
            } else {
                avatar
            };
        // The name takes the room, its marks at the row's end: in its
        // display name style's font alone, in the row's own colour, as
        // names in a server show, then the server tag's chip.
        let mut name = row![person_name(self.cx, person, speaking, faded(ink, opacity))]
            .spacing(4)
            .align_y(Alignment::Center);
        if let Some(tag) = self.cx.state.cosmetics.tag(person.tag.as_ref()) {
            name = name.push(self.cx.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: crate::fonts::SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: crate::style::CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        let mut face = row![avatar, container(name).width(Length::Fill)]
            .spacing(8)
            .align_y(Alignment::Center);
        if !person.elsewhere {
            if person.video {
                face = face.push(self.cx.look.glyph(Icon::Video, 16, theme.muted));
            }
            if person.streaming {
                let activity_themed = self
                    .cx
                    .themes
                    .content(&theme.id, self.cx.layout.theme_content.contains(&theme.id))
                    .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
                let local = live.is_some_and(|live| live.local);
                let watch = (!local).then(|| {
                    Message::Call(crate::call::Event::ShowPerson(
                        room.to_owned(),
                        person.id.clone(),
                    ))
                });
                face = face.push(controls::live_pill_in(theme, activity_themed, watch));
            }
            if person.muted || person.deafened {
                let ink = if person.server_muted {
                    theme.danger
                } else {
                    theme.muted
                };
                face = face.push(self.cx.look.glyph(Icon::MicOff, 16, ink));
            }
            if person.deafened {
                let ink = if person.server_deafened {
                    theme.danger
                } else {
                    theme.muted
                };
                face = face.push(self.cx.look.glyph(Icon::HeadphoneOff, 16, ink));
            }
        }
        let ground = if pointed {
            theme.hover
        } else {
            Color::TRANSPARENT
        };
        let area = mouse_area(
            container(face)
                .padding(Padding {
                    left: 4.0,
                    right: 8.0,
                    ..Padding::ZERO
                })
                .height(PERSON)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .style(move |_| rounded(ground, 4)),
        )
        .on_enter(Message::Side(Event::Point(id.clone(), true)))
        .on_exit(Message::Side(Event::Point(id, false)));
        let (room, user) = (room.to_owned(), person.id.clone());
        let spot = self.cx.pressed;
        let menu = {
            let (room, user) = (room.clone(), user.clone());
            RightPress::new(area, move |_| {
                Message::Call(crate::call::Event::Person(
                    room.clone(),
                    user.clone(),
                    spot.get(),
                ))
            })
        };
        // A click opens their card; pressed and moved, they're dragged
        // toward another voice channel, where their menu's Move to offers
        // one.
        let drags = Drags::new(menu)
            .click(Message::Call(crate::call::Event::Profile(user.clone())))
            .drag(move |_| {
                Message::Arrange(crate::arrange::Event::Take(room.clone(), user.clone()))
            });
        if !person.elsewhere {
            return drags.into();
        }
        // This account, in the call from another client.
        tooltip(
            drags,
            controls::tip(theme, "Connected on another client"),
            tooltip::Position::Top,
        )
        .gap(4)
        .into()
    }
}

/// A person's name in voice: in their display name style's font alone,
/// shortened to fit, bold while they speak; the usual font with name
/// styles off or none.
pub(crate) fn person_name<'a>(
    cx: crate::ctx::Ctx<'a>,
    person: &'a VoicePerson,
    speaking: bool,
    ink: Color,
) -> Element<'a, Message> {
    let own = cx
        .state
        .cosmetics
        .style(person.style.as_deref())
        .and_then(|style| crate::looks::name_look(Some(style), &person.name, true, true))
        .and_then(|look| look.font());
    let weight = if speaking { 700 } else { 500 };
    let font = match own {
        Some(own) => crate::fonts::style_font(Some(own), weight),
        None if speaking => BOLD,
        None => MEDIUM,
    };
    elide(crate::fonts::shown_name(&person.name, own))
        .size(15)
        .font(font)
        .color(ink)
        .into()
}
