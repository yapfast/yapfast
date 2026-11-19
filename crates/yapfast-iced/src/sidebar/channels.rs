//! A server's rows: headings, links, channels with their threads under
//! them, and their faces and inks.

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, column, container, mouse_area, row, stack, text},
};

use crate::{
    app::Message,
    controls::{self, ROW_COUNT, bar, count_pill},
    elide::elide,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::{corners, faded, rounded},
    widgets::{Drags, RightPress},
};

use super::{
    CHAT_BUTTON, Event, GAP, HEADING, INSET, ROW, ROW_RADIUS, Room, SideRow, VOICE_CARD_GAP,
};

impl<'a> super::View<'a> {
    pub(super) fn side_row(self, row: &'a SideRow, place: Option<usize>) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let inner: Element<'a, Message> = match row {
            SideRow::Screens {
                friends,
                requests,
                badge,
            } => column![
                self.link_row(
                    ("screen:friends", Icon::Users, "Friends"),
                    *friends,
                    "",
                    Message::Side(Event::Friends),
                    true
                ),
                self.link_row(
                    ("screen:requests", Icon::Inbox, "Requests"),
                    *requests,
                    badge,
                    Message::Side(Event::Requests),
                    true
                ),
            ]
            .spacing(GAP)
            .padding(Padding {
                top: 9.0,
                bottom: 6.0,
                ..Padding::ZERO
            })
            .into(),
            SideRow::DmHeading => row![
                text("Direct Messages")
                    .size(12)
                    .font(SEMIBOLD)
                    .color(theme.muted)
                    .width(Length::Fill),
                self.cx
                    .look
                    .icon_button(
                        Icon::UserPlus,
                        "Add a friend",
                        Message::Friends(crate::friends::Event::OpenAdd)
                    )
                    .size(24.0, 16)
                    .below(),
                self.cx
                    .look
                    .icon_button(
                        Icon::SquarePen,
                        "New message",
                        Message::Switcher(crate::switcher::Event::OpenNewDm)
                    )
                    .size(24.0, 16)
                    .below(),
            ]
            .spacing(4)
            .padding(Padding {
                left: 8.0,
                ..Padding::ZERO
            })
            .height(40)
            .align_y(Alignment::Center)
            .into(),
            SideRow::Browse { open } => container(self.link_row(
                ("screen:browse", Icon::Hash, "Browse channels"),
                *open,
                "",
                Message::Side(Event::Browse),
                false,
            ))
            .padding([6, 0])
            .into(),
            SideRow::Pinned => container(
                text("PINNED CHANNELS")
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted),
            )
            .padding(Padding {
                left: 16.0,
                ..Padding::ZERO
            })
            .height(36)
            .align_y(Alignment::Center)
            .into(),
            SideRow::PinnedEnd => container(bar(theme.line, Length::Fill, 1))
                .height(9)
                .align_y(Alignment::Center)
                .into(),
            SideRow::Heading {
                name,
                fold,
                collapsed,
            } => self.heading(name, fold, *collapsed, place),
            SideRow::Room(room) => self.room_rows(room, place),
            SideRow::Empty(words) => container(text(*words).size(14).color(theme.muted))
                .padding(8)
                .into(),
        };
        let inset = container(inner)
            .padding(Padding {
                left: INSET,
                right: INSET,
                bottom: GAP,
                top: 0.0,
            })
            .width(Length::Fill);
        // A row that may be dragged: where it would land marked by a line
        // across its top (dragged up) or its foot (dragged down).
        let inset: Element<'a, Message> = match self.s.drop_line(place) {
            Some(below) => {
                // From 8 px into the row to its end.
                let line = container(self.drop_line_view()).padding(Padding {
                    left: 2.0 * INSET,
                    right: INSET,
                    ..Padding::ZERO
                });
                let y = if below {
                    row.height() - crate::arrange::LINE
                } else {
                    0.0
                };
                stack![inset, controls::placed(line, 0.0, y)].into()
            }
            None => inset.into(),
        };
        // Where someone dragged from a voice channel would drop: outlined
        // in the drop line's green.
        let inset: Element<'a, Message> = match row {
            SideRow::Room(room) if self.s.move_aimed(&room.id) => {
                let live = theme.live;
                let outline = container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| iced::widget::container::Style {
                        border: iced::Border {
                            color: live,
                            width: 2.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    });
                stack![
                    inset,
                    container(outline).padding(Padding {
                        left: INSET,
                        right: INSET,
                        bottom: GAP,
                        top: 0.0,
                    })
                ]
                .into()
            }
            _ => inset,
        };
        // While someone is dragged, a voice channel's row says when the
        // pointer comes onto it, a folded category's busy one too.
        let onto = match row {
            SideRow::Room(room) if room.voice => Some(room.id.as_str()),
            _ => None,
        };
        let moving = self.s.moving_person.is_some();
        let over = move |place: Option<usize>, over: bool| match (onto, place) {
            (Some(room), _) if moving => {
                Message::Arrange(crate::arrange::Event::Onto(room.to_owned(), over))
            }
            (_, Some(place)) if !moving => {
                Message::Arrange(crate::arrange::Event::Over(place, over))
            }
            _ => Message::Nothing,
        };
        let inset: Element<'a, Message> = match place {
            Some(place) => {
                let click = match row {
                    SideRow::Heading { fold, .. } => Message::Side(Event::Toggle(fold.clone())),
                    SideRow::Room(room) if room.voice => {
                        Message::Side(Event::Voice(room.id.clone()))
                    }
                    SideRow::Room(room) => Message::Side(Event::Select(room.id.clone())),
                    _ => return inset,
                };
                let head = match row {
                    SideRow::Room(room) => room.row_height(),
                    _ => HEADING,
                };
                Drags::new(inset)
                    .click(click)
                    .head(head)
                    .drag(move |grab| Message::Arrange(crate::arrange::Event::Start(place, grab)))
                    .tracking(self.s.dragging(), move |on| over(Some(place), on))
                    .into()
            }
            None if onto.is_some() => Drags::new(inset)
                .tracking(moving, move |on| over(None, on))
                .into(),
            None => inset,
        };
        match row {
            // Unread activity is marked at the sidebar's edge, as on the
            // rail, a thread's too.
            SideRow::Room(room) if room.unread && !room.selected => {
                let ink = theme.text;
                let mark = container(Space::new())
                    .width(4)
                    .height(8)
                    .style(move |_| rounded(ink, corners([0.0, 4.0, 4.0, 0.0])));
                stack![
                    inset,
                    controls::placed(mark, 0.0, (room.row_height() - 8.0) / 2.0)
                ]
                .into()
            }
            _ => inset,
        }
    }

    /// A category's heading: 17 px above its name, the chevron after it
    /// turned while folded; a click folds it or opens it, and a Discord
    /// category's opens its menu on a right click.
    pub(super) fn heading(
        self,
        name: &'a str,
        fold: &'a str,
        collapsed: bool,
        place: Option<usize>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let id = format!("heading:{fold}");
        let pointed = self.cx.pointed.side.as_deref() == Some(id.as_str());
        let ink = if pointed {
            theme.secondary
        } else {
            theme.channel_read
        };
        // While a drag is on, as a text channel's row.
        let ink = if self.s.arrange_dimmed(None, false) {
            faded(ink, crate::arrange::DIM)
        } else {
            ink
        };
        let face = row![
            elide(name).size(14).font(SEMIBOLD).color(ink),
            self.cx.look.glyph(
                if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                12,
                ink
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let mut face = row![face, Space::new().width(Length::Fill)].align_y(Alignment::Center);
        // Discord's "+", while the pointer is over a category this account
        // may make channels in.
        if pointed && self.cx.app.may_create_channel(fold) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::Add,
                        "Create Channel",
                        Message::Side(Event::CreateIn(fold.to_owned())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        let area = mouse_area(
            container(face)
                .padding(Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 17.0,
                    bottom: 0.0,
                })
                .height(HEADING)
                .width(Length::Fill)
                .align_y(Alignment::Center),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_enter(Message::Side(Event::Point(id.clone(), true)))
        .on_exit(Message::Side(Event::Point(id, false)));
        // One that may be dragged folds on a click, not a press (`Drags`).
        let area = if place.is_none() {
            area.on_press(Message::Side(Event::Toggle(fold.to_owned())))
        } else {
            area
        };
        if !self.cx.state.is_category(fold) {
            return area.into();
        }
        // Where in the window it was pressed: the list's own space is under
        // the header, and scrolled.
        let (room, spot) = (fold.to_owned(), self.cx.pressed);
        RightPress::new(area, move |_| {
            Message::Side(Event::Menu(room.clone(), spot.get()))
        })
        .into()
    }

    /// A row that opens one of the DM home's screens or Browse Channels; the
    /// screens' rows are `tall`, Discord's 38 px with its own greys.
    pub(super) fn link_row(
        self,
        (id, icon, label): (&'static str, Icon, &'static str),
        selected: bool,
        badge: &str,
        message: Message,
        tall: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(id);
        let (open, read) = if tall {
            (theme.nav_open, theme.nav_read)
        } else {
            (theme.text, theme.muted)
        };
        let ink = if selected {
            open
        } else if pointed {
            theme.secondary
        } else {
            read
        };
        let icon_ink = match (selected, tall) {
            (true, _) => open,
            (false, true) => theme.nav_read,
            (false, false) => theme.faint,
        };
        let mut face = row![
            self.cx.look.glyph(icon, 20, icon_ink),
            text(label)
                .size(15)
                .font(MEDIUM)
                .color(ink)
                .width(Length::Fill)
        ]
        .spacing(if tall { 13 } else { 8 })
        .align_y(Alignment::Center);
        if !badge.is_empty() {
            face = face.push(count_pill(theme, badge, ROW_COUNT, None));
        }
        let height = if tall { super::NAV_ROW } else { ROW };
        self.row_face(
            id.into(),
            face.into(),
            height,
            selected,
            Some(message),
            None,
        )
    }

    /// A row's face: lit while selected or pointed at, a press (unless a
    /// click is the row's `Drags`) and a right click on it.
    pub(super) fn row_face(
        self,
        id: String,
        face: Element<'a, Message>,
        height: f32,
        selected: bool,
        message: Option<Message>,
        menu: Option<String>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(id.as_str());
        let ground = if selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            Color::TRANSPARENT
        };
        let body = container(face)
            .padding([0, 8])
            .height(height)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| rounded(ground, ROW_RADIUS));
        self.row_presses(id, body.into(), message, menu)
    }

    /// What a row's face takes: the pointer over it, a press (unless a
    /// click is the row's `Drags`) and a right click on it.
    pub(super) fn row_presses(
        self,
        id: String,
        body: Element<'a, Message>,
        message: Option<Message>,
        menu: Option<String>,
    ) -> Element<'a, Message> {
        let area = mouse_area(body)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(id.clone(), true)))
            .on_exit(Message::Side(Event::Point(id, false)));
        let area = match message {
            Some(message) => area.on_press(message),
            None => area,
        };
        match menu {
            Some(room) => {
                let spot = self.cx.pressed;
                RightPress::new(area, move |_| {
                    Message::Side(Event::Menu(room.clone(), spot.get()))
                })
                .into()
            }
            None => area.into(),
        }
    }

    /// A conversation's row, with its thread line, and who is in it for a
    /// busy voice channel.
    pub(super) fn room_rows(self, room: &'a Room, place: Option<usize>) -> Element<'a, Message> {
        let line = self.room_row(room, place.is_some());
        let line: Element<'a, Message> = if room.thread {
            row![self.thread_line(room.thread_continues), line].into()
        } else {
            line
        };
        if room.people.is_empty() {
            return line;
        }
        let mut people = column![].padding(Padding {
            left: 32.0,
            bottom: 4.0,
            ..Padding::ZERO
        });
        // The call this account is in shows who speaks, in the call's
        // order: this account first.
        let voice = &self.cx.state.voice;
        let call = voice.active() && voice.room.as_deref() == Some(room.id.as_str());
        let mut listed: Vec<_> = room
            .people
            .iter()
            .map(|person| {
                let live = call
                    .then(|| {
                        voice
                            .participants
                            .iter()
                            .position(|live| live.person == person.id || live.id == person.id)
                    })
                    .flatten();
                (live, person)
            })
            .collect();
        if call {
            listed.sort_by_key(|(live, _)| live.unwrap_or(usize::MAX));
        }
        for (live, person) in listed {
            let live = live.map(|place| &voice.participants[place]);
            people = people.push(self.voice_person(&room.id, person, live));
        }
        column![line, people].spacing(GAP).into()
    }

    /// The line from a channel to its thread, 28 px wide: down from the
    /// channel and round into the thread, on down while another follows.
    pub(super) fn thread_line(self, continues: bool) -> Element<'a, Message> {
        let faint = self.cx.look.theme.faint;
        let down = bar(faint, 1.5, if continues { ROW + GAP } else { 13.0 + GAP });
        let round = container(Space::new())
            .width(10.75)
            .height(7.5)
            .style(move |_| iced::widget::container::Style {
                border: iced::Border {
                    color: faint,
                    width: 1.5,
                    radius: corners([0.0, 0.0, 0.0, 6.0]),
                },
                ..Default::default()
            });
        container(stack![
            controls::placed(down, 17.25, 0.0),
            controls::placed(round, 17.25, 12.75),
        ])
        .width(28)
        .height(ROW)
        .into()
    }

    /// A conversation's row; one that may be `dragged` is clicked through
    /// its `Drags`.
    pub(super) fn room_row(self, room: &'a Room, dragged: bool) -> Element<'a, Message> {
        if room.direct {
            return self.dm_row(room);
        }
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(room.id.as_str());
        let (ink, opacity) = self.row_ink(room, pointed);
        // While a drag is on, rows it can't land among dim.
        let (ink, opacity) = if self.s.arrange_dimmed(Some(&room.id), room.voice) {
            (
                faded(ink, crate::arrange::DIM),
                opacity * crate::arrange::DIM,
            )
        } else {
            (ink, opacity)
        };
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        if !room.thread {
            let icon_ink = if room.selected {
                theme.channel_white
            } else if room.unread {
                theme.secondary
            } else {
                theme.channel_read
            };
            face = face.push(self.cx.look.glyph(room.icon, 20, faded(icon_ink, opacity)));
        }
        let name = elide(room.name.as_str()).size(15).font(MEDIUM).color(ink);
        face = face.push(column![name].width(Length::Fill));
        if let Some(started) = room.started {
            let lasted = yapfast_core::clock::lasted(started, self.cx.minute);
            face = face.push(text(lasted).size(12).color(theme.muted));
        }
        if !room.badge.is_empty() {
            face = face.push(count_pill(theme, &room.badge, ROW_COUNT, None));
        } else if !room.posts.is_empty() {
            // The forum's unread posts, a bold number in the nav grey.
            face = face.push(
                text(room.posts.as_str())
                    .size(11)
                    .font(BOLD)
                    .color(theme.nav_read),
            );
        }
        // Discord's invite button on a channel's row, under the pointer or
        // while it's selected, for one who may invite.
        if self.s.header.invite && !room.thread && (pointed || room.selected) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::UserPlus,
                        "Create invite",
                        Message::Side(Event::InviteTo(room.id.clone())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        // A voice channel's chat opens without joining it, from a button
        // shown under the pointer or while it's selected, as in Discord.
        if room.chat && (pointed || room.selected) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::Forum,
                        "Open chat",
                        Message::Side(Event::OpenChat(room.id.clone())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        let message = if room.voice {
            Message::Side(Event::Voice(room.id.clone()))
        } else {
            Message::Side(Event::Select(room.id.clone()))
        };
        let row = self.row_face(
            room.id.clone(),
            face.into(),
            room.row_height(),
            room.selected,
            (!dragged).then_some(message),
            Some(room.id.clone()),
        );
        // A busy voice channel's card beside it, under the pointer.
        if pointed && room.voice && !room.people.is_empty() && !self.s.voice_card_off {
            return iced::widget::tooltip(
                row,
                self.voice_channel_card(&room.people),
                iced::widget::tooltip::Position::Right,
            )
            .gap(VOICE_CARD_GAP)
            .snap_within_viewport(true)
            .into();
        }
        row
    }

    /// A row's ink, and its opacity: a muted one is faint until it's
    /// selected or pointed at.
    pub(super) fn row_ink(self, room: &Room, pointed: bool) -> (Color, f32) {
        let theme = self.cx.look.theme;
        let ink = if room.locked {
            if room.selected {
                theme.muted
            } else {
                theme.faint
            }
        } else if room.selected || room.unread {
            // Discord's channel list inks; a DM keeps the text colours.
            if room.direct {
                theme.text
            } else {
                theme.channel_white
            }
        } else if pointed {
            theme.secondary
        } else if room.direct {
            theme.muted
        } else {
            theme.channel_read
        };
        let opacity = if room.muted && !room.selected && !pointed {
            0.5
        } else {
            1.0
        };
        (faded(ink, opacity), opacity)
    }
}
