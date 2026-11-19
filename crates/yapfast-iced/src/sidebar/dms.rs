//! A direct message's row: the person's face, styled name and tag, what
//! they're up to, and Wave on a new one.

use iced::{
    Alignment, Element, Length, Point,
    widget::{button, container, row, text},
};

use crate::{
    app::Message,
    controls::{self, ROW_COUNT, count_pill},
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    people::StatusInk,
    people::{NameInk, Person},
    pictures::{Face, Icon},
    style::rounded,
};

use super::{CHAT_BUTTON, DM_ROW, Event, ROW_RADIUS, Room};

impl<'a> super::View<'a> {
    /// A DM's row: the person's face with its decoration and presence, the
    /// name in its style with the server tag or OFFICIAL, what they're up
    /// to or the voice channel they're in, Wave for a DM nobody wrote in
    /// yet, the call, pin and mentions; the nameplate behind it while it's
    /// pointed at or selected, as Discord's client draws it only then.
    pub(super) fn dm_row(self, room: &'a Room) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(room.id.as_str());
        let (ink, opacity) = self.row_ink(room, pointed);
        let row_ground = if room.selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
        };
        let face = Face::new(room.avatar.as_deref(), &room.name, 32.0)
            .key(Some(&room.id))
            .opacity(opacity)
            .decoration(room.decoration.as_deref(), pointed)
            .dot(room.presence.map(|presence| {
                (
                    presence,
                    crate::theme::rgb(row_ground),
                    Point::new(20.0, 20.0),
                )
            }));
        let mut name = row![self.cx.person_name(
            &room.name,
            NameInk::new(15.0, 500, ink).style(room.style.as_ref()),
        )]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some((letters, badge)) = &room.tag {
            name = name.push(self.cx.tag_chip(
                letters,
                badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: crate::style::CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        if room.official {
            name = name.push(self.official_tag());
        }
        let mut person = Person::new(face, name);
        // Their voice channel in full when nothing else is said, else a
        // speaker before what is, as the friends list has it.
        let spot = room
            .voice_spot
            .as_ref()
            .map(|(id, channel, server)| (id.as_str(), channel.as_str(), server.as_str()));
        let mut line = row![].spacing(4).align_y(Alignment::Center);
        if let Some(spot) = spot {
            line = line.push(self.cx.voice_spot(spot, 12.0, !room.status.is_empty()));
        }
        if !room.status.is_empty() {
            line = line.push(self.cx.status_line(
                &room.status,
                StatusInk::under_name(self.cx.look.theme, opacity),
            ));
        } else if room.official && spot.is_none() {
            line = line.push(
                text("Official Discord Message")
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        if spot.is_some() || !room.status.is_empty() || room.official {
            person = person.sub(line);
        }
        let mut face = self.cx.person(person);
        if let Some(user) = room.wave.as_ref().filter(|_| pointed || room.selected) {
            face = face.push(self.wave_button(user));
        }
        if let Some(label) = room.close.filter(|_| pointed || room.selected) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::X,
                        label,
                        Message::Side(Event::CloseDm(room.id.clone())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        if room.call {
            face = face.push(self.cx.look.glyph(Icon::Phone, 16, theme.live));
        }
        if room.pinned {
            face = face.push(self.cx.look.glyph(Icon::Pin, 14, theme.faint));
        }
        if !room.badge.is_empty() {
            face = face.push(count_pill(theme, &room.badge, ROW_COUNT, None));
        }
        let select = Some(Message::Side(Event::Select(room.id.clone())));
        let Some((url, palette)) = room.nameplate.as_ref().filter(|_| pointed || room.selected)
        else {
            return self.row_face(
                room.id.clone(),
                face.into(),
                DM_ROW,
                room.selected,
                select,
                Some(room.id.clone()),
            );
        };
        // The plate fills the row to its edges and corners, so the row's
        // inset is its face's own, and the plate lies behind all of it.
        let plate = crate::members::nameplate(
            self.cx,
            url,
            (DM_ROW, ROW_RADIUS),
            pointed,
            room.selected,
            1.0,
        );
        let plated = crate::widgets::PlateRow::new(
            face.width(Length::Fill).padding([0, 8]).height(DM_ROW),
            plate,
            palette.map(crate::style::hex),
            row_ground,
            crate::members::plate_end(pointed, room.selected),
            1.0,
        )
        .radius(ROW_RADIUS);
        let body = container(plated)
            .width(Length::Fill)
            .height(DM_ROW)
            .clip(true)
            .style(move |_| rounded(row_ground, ROW_RADIUS));
        self.row_presses(room.id.clone(), body.into(), select, Some(room.id.clone()))
    }

    /// Discord's OFFICIAL badge by its own account's name: blurple, a
    /// check and the word.
    pub(super) fn official_tag(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let accent = theme.accent;
        container(
            row![
                self.cx.look.glyph(Icon::Check, 12, theme.on_accent),
                text("OFFICIAL").size(10).font(BOLD).color(theme.on_accent),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding([0, 4])
        .height(16)
        .align_y(Alignment::Center)
        .style(move |_| rounded(accent, 4))
        .into()
    }

    /// Wave, which Discord offers in a DM nobody has written in yet: it
    /// sends a waving hand.
    pub(super) fn wave_button(self, user: &str) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (rest, lit, on) = (theme.accent, theme.accent_hover, theme.on_accent);
        button(
            container(
                row![
                    self.cx.look.glyph(Icon::Hand, 14, on),
                    text("Wave").size(12).font(MEDIUM).color(on),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([0, 8])
            .height(24)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(Message::Side(Event::Wave(user.to_owned())))
        .style(controls::lit_style(Some(rest), lit, 4.0))
        .into()
    }
}
