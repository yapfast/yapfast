//! One of someone's activities on their profile, as the Slint client's
//! `ActivityCard` shows it: its heading, its pictures, its lines, and how
//! long it's run or how far a song has got.

use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Size,
    widget::{Space, button, column, container, row, stack, text},
};

use super::look::{ActivityLook, Inks, clock};
use crate::{
    app::{App, Message},
    controls::{Kind, action, bar, ink_style},
    fonts::{BOLD, SEMIBOLD},
    looks::Status,
    style::rounded,
    widgets::Pic,
};

/// The large picture, the small one on its ring, and where the ring sits.
const LARGE: f32 = 64.0;
const SMALL: f32 = 22.0;
const RING: f32 = 26.0;
const RING_AT: f32 = 44.0;

impl App {
    /// An activity's card at `now` (Unix seconds).
    pub fn activity_card_view<'a>(
        &'a self,
        activity: &'a ActivityLook,
        inks: &Inks,
        now: u64,
        menu: Option<bool>,
    ) -> Element<'a, Message> {
        let mut head = row![
            text(activity.heading.as_str())
                .size(12)
                .font(BOLD)
                .color(inks.ink3)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
        ]
        .height(16)
        .align_y(Alignment::Center);
        // Discord's more menu, on anyone's card but a bot's; on one's own
        // game it offers its privacy too.
        if let Some(own) = menu {
            let face = self.glyph(crate::pictures::Icon::Ellipsis, 14, inks.ink2);
            head = head.push(self.icon_face_button(
                face,
                "More",
                20.0,
                Message::Profile(super::Event::ActivityMenu(own && activity.game)),
                false,
            ));
        }
        let mut card = column![head].spacing(8);
        let mut body = row![].spacing(12).align_y(Alignment::Center);
        if let Some(large) = &activity.large {
            body = body.push(self.activity_art(large, activity.small.as_deref(), inks));
        }
        let mut lines = column![].spacing(2).width(Length::Fill);
        let title: Element<'a, Message> = match &activity.link {
            // A song's title opens its page, as in Discord.
            Some(link) => button(
                text(activity.name.as_str())
                    .size(14)
                    .font(BOLD)
                    .wrapping(text::Wrapping::None),
            )
            .padding(0)
            .on_press(Message::Link(link.clone(), Point::ORIGIN))
            .style(ink_style(inks.ink, self.theme.link))
            .into(),
            None => {
                let status = Status {
                    words: activity.name.clone(),
                    emoji: activity.name_emoji.clone(),
                    ..Status::default()
                };
                self.status_text(status, 14.0, SEMIBOLD, inks.ink)
            }
        };
        lines = lines.push(title);
        for line in [&activity.details, &activity.state] {
            if !line.is_empty() {
                lines = lines.push(
                    text(line.as_str())
                        .size(13)
                        .color(inks.ink2)
                        .wrapping(text::Wrapping::None),
                );
            }
        }
        match (activity.start, activity.end) {
            // A song, with its start and end, shows how far it's got on a
            // bar, with how long it is either side.
            (Some(start), Some(end)) if end > start => {
                let length = end - start;
                let played = now.saturating_sub(start).min(length);
                let share = played as f32 / length.max(1) as f32;
                let track = if inks.themed {
                    Color { a: 0.2, ..inks.ink }
                } else {
                    self.theme.raised
                };
                let ink = inks.ink;
                let done = (share * 1000.0).round() as u16;
                let filled = container(row![
                    bar(ink, Length::FillPortion(done.max(1)), 3).style(move |_| rounded(ink, 1.5)),
                    Space::new().width(Length::FillPortion(1000 - done.min(999))),
                ])
                .width(Length::Fill)
                .style(move |_| rounded(track, 1.5));
                lines = lines.push(
                    row![
                        text(clock(played)).size(12).color(inks.ink2),
                        container(filled).width(Length::Fill),
                        text(clock(length)).size(12).color(inks.ink2),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            (Some(start), _) => {
                lines = lines.push(
                    text(format!("{} elapsed", clock(now.saturating_sub(start))))
                        .size(12)
                        .color(inks.ink3),
                );
            }
            (None, Some(end)) => {
                lines = lines.push(
                    text(format!("{} left", clock(end.saturating_sub(now))))
                        .size(12)
                        .color(inks.ink3),
                );
            }
            (None, None) => {}
        }
        body = body.push(lines);
        card = card.push(body);
        if let Some(url) = &activity.url {
            card = card.push(
                action(
                    &self.theme,
                    "Watch stream",
                    Kind::Plain,
                    Some(Message::Link(url.clone(), Point::ORIGIN)),
                )
                .small(),
            );
        }
        card.into()
    }

    /// The large picture, with the small one on a ring of the card's ground
    /// at its corner.
    fn activity_art<'a>(
        &self,
        large: &str,
        small: Option<&str>,
        inks: &Inks,
    ) -> Element<'a, Message> {
        let raised = self.theme.raised;
        let square = Size::new(LARGE, LARGE);
        let mut pictures = stack![
            container(Space::new())
                .width(LARGE)
                .height(LARGE)
                .style(move |_| rounded(raised, LARGE / 6.0))
        ];
        if let Some(picture) = self.picture(large, LARGE as u32, LARGE as u32, true) {
            pictures = pictures.push(Pic::new(picture, square).radius(LARGE / 6.0));
        }
        if let Some(small) = small {
            let ground = inks.ground;
            pictures = pictures.push(crate::controls::placed(
                container(Space::new())
                    .width(RING)
                    .height(RING)
                    .style(move |_| rounded(ground, RING / 2.0)),
                RING_AT,
                RING_AT,
            ));
            if let Some(picture) = self.picture(small, SMALL as u32, SMALL as u32, true) {
                let inset = RING_AT + (RING - SMALL) / 2.0;
                pictures = pictures.push(
                    Pic::new(picture, Size::new(RING_AT + RING, RING_AT + RING))
                        .at(Rectangle::new(
                            Point::new(inset, inset),
                            Size::new(SMALL, SMALL),
                        ))
                        .radius(SMALL / 2.0),
                );
            }
        }
        container(pictures).width(LARGE).height(LARGE).into()
    }

    /// A status's emoji and words on one line, in a size, weight and ink of
    /// its own, as the Slint client's `StatusText`.
    pub fn status_text<'a>(
        &self,
        status: Status,
        size: f32,
        font: iced::Font,
        ink: Color,
    ) -> Element<'a, Message> {
        let side = (size + 3.0).round();
        let mut line = row![].spacing(4).align_y(Alignment::Center);
        if let Some(emoji) = &status.emoji {
            line = line.push(match self.picture(emoji, side as u32, side as u32, false) {
                Some(picture) => Element::from(Pic::square(picture, side)),
                None => Space::new().width(side).height(side).into(),
            });
        }
        if !status.words.is_empty() {
            line = line.push(
                text(status.words)
                    .size(size)
                    .font(font)
                    .color(ink)
                    .wrapping(text::Wrapping::None),
            );
        }
        line.into()
    }
}
