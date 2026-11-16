//! Discord's Browse Channels for a Community server, in place of a
//! conversation, as the Slint client's `BrowseScreen`: a search of the
//! channels' names, then the channels under their categories' headings,
//! each with a box ticked while the channel list follows it and a button
//! that opens it; a category's Follow category switch follows every
//! channel in it, whose boxes then change only with it.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{button, column, container, row, text, tooltip},
};
use yapfast_core::application::{Action, BrowsedChannel, BrowsedKind};

use crate::{
    app::{App, Message},
    controls::{self, Kind, lit_style},
    elide::elide,
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    style::scrollbar,
};

/// A channel's row, and the search field's id.
const ROW: f32 = 56.0;
const HEADING: f32 = 44.0;
pub const FILTER: &str = "browse-filter";
const LOCKED: &str = "You must unfollow this category to add or remove specific channels.";

#[derive(Clone, Debug)]
pub enum Event {
    Filter(String),
    Follow(String, bool),
    View(String),
}

impl App {
    pub fn browse_channels_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Filter(filter) => self.browse_filter = filter,
            Event::Follow(room, follow) => {
                self.client.dispatch(Action::BrowseFollow { room, follow })
            }
            Event::View(room) => self.client.dispatch(Action::Open(room)),
        }
        self.sync()
    }

    /// Browse Channels, while it's open.
    pub fn browse_channels(&self) -> Option<Element<'_, Message>> {
        let browse = self.client.state().browse.as_ref()?;
        let theme = &self.theme;
        let head = container(
            row![
                self.glyph(Icon::Hash, 22, theme.muted),
                text("Browse Channels")
                    .size(16)
                    .font(BOLD)
                    .color(theme.text)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([0, 16])
        .width(Length::Fill)
        .height(crate::header::HEIGHT)
        .align_y(Alignment::Center);
        let field = controls::field(
            theme,
            FILTER,
            "Search channels",
            &self.browse_filter,
            false,
            true,
        )
        .size(15)
        .padding([8, 10])
        .on_input(|filter| Message::BrowseChannels(Event::Filter(filter)));
        let query = self.browse_filter.trim().to_lowercase();
        let mut list = column![];
        let mut heading = None;
        let mut shown = 0;
        for channel in &browse.rows {
            if channel.kind == BrowsedKind::Category {
                heading = Some(channel);
                continue;
            }
            if !channel.name.to_lowercase().contains(&query) {
                continue;
            }
            if let Some(category) = heading.take() {
                list = list.push(
                    container(self.browse_heading(category))
                        .padding(Padding::ZERO.top(if shown > 0 { 16 } else { 0 })),
                );
            }
            list = list.push(container(self.browse_row(channel)).padding(Padding::ZERO.bottom(2)));
            shown += 1;
        }
        let mut page = column![field]
            .spacing(8)
            .padding(Padding::new(16.0).bottom(0));
        if shown == 0 {
            let words = if query.is_empty() {
                "There are no channels to browse."
            } else {
                "No channel's name matches that."
            };
            page = page.push(
                container(text(words).size(14).color(theme.secondary))
                    .padding(Padding::ZERO.left(8).top(8)),
            );
        }
        page = page.push(
            scroll_view(list.padding(Padding::ZERO.bottom(16)))
                .height(Length::Fill)
                .style(scrollbar(theme)),
        );
        Some(
            column![
                head,
                controls::bar(theme.outline_chat, Length::Fill, 1),
                page
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        )
    }

    /// A category's heading with its Follow category switch.
    fn browse_heading<'a>(&'a self, category: &'a BrowsedChannel) -> Element<'a, Message> {
        let switch = form::switch(
            "Follow category",
            category.followed,
            Some(Message::BrowseChannels(Event::Follow(
                category.id.clone(),
                !category.followed,
            ))),
        )
        .view(self.look());
        row![
            container(
                elide(category.name.as_str())
                    .size(17)
                    .font(BOLD)
                    .color(self.theme.text)
            )
            .width(Length::Fill),
            container(switch).width(172),
        ]
        .spacing(12)
        .padding([0, 12])
        .height(HEADING)
        .align_y(Alignment::Center)
        .into()
    }

    /// A channel: its icon, name and topic, its button, and its box; a
    /// click ticks the box or clears it, unless its category follows it.
    fn browse_row<'a>(&'a self, channel: &'a BrowsedChannel) -> Element<'a, Message> {
        let theme = &self.theme;
        let icon = match channel.kind {
            BrowsedKind::Voice => Icon::Voice,
            BrowsedKind::Forum => Icon::Forum,
            BrowsedKind::Announcement => Icon::Announcement,
            BrowsedKind::Category | BrowsedKind::Text => Icon::Hash,
        };
        let mut words = column![elide(channel.name.as_str()).size(15).font(SEMIBOLD).color(
            if channel.by_category {
                theme.muted
            } else {
                theme.text
            }
        )]
        .spacing(2);
        if !channel.topic.is_empty() {
            words = words.push(elide(channel.topic.as_str()).size(12).color(theme.muted));
        }
        let label = match channel.kind {
            BrowsedKind::Voice => "Join",
            BrowsedKind::Forum => "Open",
            _ => "View",
        };
        let content = row![
            self.glyph(icon, 20, theme.muted),
            container(words).width(Length::Fill),
            controls::action(
                theme,
                label,
                Kind::Plain,
                Some(Message::BrowseChannels(Event::View(channel.id.clone()))),
            )
            .small(),
            self.checkbox(channel.followed, false),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let face =
            button(
                container(content)
                    .padding([0, 12])
                    .height(ROW)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .width(Length::Fill)
            .on_press_maybe((!channel.by_category).then(|| {
                Message::BrowseChannels(Event::Follow(channel.id.clone(), !channel.followed))
            }))
            .style(lit_style(None, theme.raised, 8.0));
        if channel.by_category {
            tooltip(face, controls::tip(theme, LOCKED), tooltip::Position::Top).into()
        } else {
            face.into()
        }
    }
}
