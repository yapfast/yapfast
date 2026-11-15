//! The timeline drawn: its rows over their tints, the dividers, the
//! history's edge and Discord's welcome, the conversation's start, and
//! the bars that jump to the first unread message and to the present.

use std::sync::Arc;

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, row, scrollable, stack, text},
};
use yapfast_core::chat::ChatMessage;

use crate::{
    app::{App, Message, key},
    controls::bar,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    pictures::{Face, Icon},
    style::{rounded, scrollbar},
    widgets::{Corner, RightPress, Rows, Watch},
};

use super::{
    ASIDE, DIVIDER, EDGE, Edge, Intro, MENTION_EDGE, MENTIONED, MENTIONED_HOVER, PRESENT_BAR,
    ROW_GAP, RowLook, TAIL, TIMELINE, UNREAD_BAR, top_space,
};

/// What the unread bar says, as the Slint client's `unread_label`: how many
/// new messages (more when the window is older ones), since when.
pub fn unread_label(unread: &[Arc<ChatMessage>], more: bool) -> String {
    let count = unread.iter().filter(|message| !message.own).count();
    let messages = if count == 1 && !more {
        "message"
    } else {
        "messages"
    };
    let count = if more {
        format!("{count}+")
    } else {
        count.to_string()
    };
    let Some(first) = unread.first().filter(|first| first.time.contains(':')) else {
        return format!("{count} new {messages}");
    };
    let on = yapfast_core::clock::day(first.timestamp);
    if on.is_empty() || on == yapfast_core::clock::day(yapfast_core::clock::now_ms() / 1000) {
        format!("{count} new {messages} since {}", first.time)
    } else {
        format!("{count} new {messages} since {} on {on}", first.time)
    }
}

impl App {
    pub(crate) fn timeline(&self) -> Element<'_, Message> {
        if self.messages.is_empty() {
            return self.conversation_start();
        }
        let (first, last) = self.in_view(crate::sync::MARGIN);
        // While someone reads earlier ones, the row at the view's top: it
        // and those after it keep their tops as they're measured.
        let pivot = (!self.at_end).then(|| {
            let top = self.view_top();
            (first..last)
                .position(|index| self.tops[index + 1] > top)
                .unwrap_or(last - first)
        });
        let mut rows =
            Rows::new(self.total_height()).measured(Message::Measured, self.found.clone(), pivot);
        // A short history is held down, its newest message over the composer.
        let lead = (self.timeline_height - self.total_height()).max(0.0);
        for index in first..last {
            rows.push(
                key(&self.messages[index].item),
                self.tops[index],
                self.heights[index],
                self.timeline_row(index),
            );
        }
        let list = scrollable(column![Space::new().height(lead), rows])
            .id(TIMELINE)
            .anchor_bottom()
            .on_scroll(Message::Timeline)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(scrollbar(&self.theme));
        // A press on no text clears the selection; letting go ends a drag.
        let list = Watch::new(
            list,
            self.selection.is_some().then_some(Message::SelectClear),
            self.selecting.then_some(Message::SelectEnd),
        );
        let mut layers = stack![list].width(Length::Fill).height(Length::Fill);
        if let Some(typing) = self.typing_view() {
            layers = layers.push(typing);
        }
        if let Some(bar) = self.unread_bar() {
            layers = layers.push(
                container(bar)
                    .width(Length::Fill)
                    .padding(Padding::ZERO.top(12))
                    .align_x(Alignment::Center),
            );
        }
        if self.client.state().detached {
            layers = layers.push(
                container(self.present_bar())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([8, 16])
                    .align_y(Alignment::End),
            );
        }
        layers.into()
    }

    /// One row: the history's edge above the first, a divider above a new
    /// day or the first unread message, then the message on its tint.
    pub fn timeline_row(&self, index: usize) -> Element<'_, Message> {
        let (Some(message), Some(look)) = (self.row_message(index), self.row_look(index)) else {
            return Space::new().into();
        };
        let theme = &self.theme;
        let aside = index >= ASIDE;
        let edge = if aside {
            index == ASIDE && self.aside.as_ref().is_some_and(|aside| aside.ready)
        } else {
            index == 0 && self.edge.ready
        };
        let head = edge || look.divided();
        let mut whole = column![].width(Length::Fill);
        if head {
            let mut above = column![].width(Length::Fill);
            if edge && aside {
                above = above.push(self.aside_edge());
            } else if edge {
                above = above.push(self.history_edge());
            }
            if look.divided() {
                above = above.push(self.divider(look));
            }
            whole = whole.push(container(above).padding(
                Padding::ZERO.top(top_space(look) - if look.divided() { DIVIDER } else { 0.0 }),
            ));
        }
        let pointed = self.pointed.message.as_deref() == Some(message.item.as_str());
        let jumped = self.jump_lit.as_deref() == Some(message.item.as_str());
        let edited = self.editing.as_deref() == Some(message.item.as_str());
        let ground = if edited {
            // Discord's edit tint (#29292d on its dark theme).
            Some(crate::theme::mix(theme.active, theme.chat, 0.79))
        } else if jumped {
            Some(Color {
                a: 0.14,
                ..theme.accent
            })
        } else if look.highlighted {
            Some(if pointed { MENTIONED_HOVER } else { MENTIONED })
        } else if pointed {
            Some(theme.row_hover)
        } else {
            None
        };
        let content = container(self.message_content(index, look))
            .width(Length::Fill)
            .padding(Padding {
                top: if head { 0.0 } else { top_space(look) },
                bottom: ROW_GAP,
                ..Padding::ZERO
            })
            .style(move |_| container::Style {
                background: ground.map(Into::into),
                ..container::Style::default()
            });
        let mut content = stack![content];
        if look.highlighted {
            content = content.push(bar(theme.accent, MENTION_EDGE, Length::Fill));
        }
        // The hover bar floats over the row's top edge, so it never moves
        // the timeline.
        if pointed && let Some(actions) = self.hover_bar(index) {
            let body = if head { 0.0 } else { top_space(look) };
            content = content.push(Corner::new(actions, 16.0, body - 16.0));
        }
        // A right click opens the message's menu where the pointer is.
        let (lane, _) = self.lane_place(index);
        let origin = iced::Vector::new(lane.left, lane.top);
        let item = look.item.clone();
        let content = RightPress::new(content, move |at| {
            Message::RowMenu(item.clone(), at + origin)
        });
        whole = whole.push(
            mouse_area(content)
                .on_enter(Message::PointRow(look.item.clone(), true))
                .on_exit(Message::PointRow(look.item.clone(), false)),
        );
        if look.last {
            whole = whole.push(Space::new().height(TAIL - ROW_GAP));
        }
        // A window of older messages ends with what's after it: newer
        // messages loading, or a way to load them.
        if !aside && index + 1 == self.messages.len() && self.edge.foot() > 0.0 {
            let edge = &self.edge;
            whole = whole.push(
                container(self.edge_button(
                    edge.loading_later,
                    edge.later_failed,
                    true,
                    Message::Later,
                ))
                .height(edge.foot())
                .padding(Padding {
                    top: 4.0,
                    bottom: 8.0,
                    ..Padding::ZERO
                })
                .center_x(Length::Fill)
                .align_y(Alignment::Center),
            );
        }
        whole.into()
    }

    /// A new day's date, and the "New" divider in marigold, in the middle
    /// of one line, together when both are here.
    pub(super) fn divider<'a>(&'a self, look: &'a RowLook) -> Element<'a, Message> {
        let theme = &self.theme;
        let ink = if look.unread_start {
            theme.accent
        } else {
            theme.line
        };
        let line = || {
            container(bar(ink, Length::Fill, 1))
                .width(Length::Fill)
                .center_y(Length::Fill)
        };
        let mut middle = row![line()].spacing(10).align_y(Alignment::Center);
        if let Some(day) = &look.day {
            middle = middle.push(
                text(day.as_str())
                    .size(12)
                    .font(SEMIBOLD)
                    .color(theme.muted),
            );
        }
        if look.day.is_some() && look.unread_start {
            middle = middle.push(text("·").size(13).font(BOLD).color(theme.muted));
        }
        if look.unread_start {
            let (accent, on_accent) = (theme.accent, theme.on_accent);
            middle = middle.push(
                container(text("New").size(12).font(BOLD).color(on_accent))
                    .padding([0, 5])
                    .height(18)
                    .align_y(Alignment::Center)
                    .style(move |_| rounded(accent, 4)),
            );
        }
        container(middle.push(line()))
            .padding(Padding {
                left: 16.0,
                right: 16.0,
                bottom: 6.0,
                top: 0.0,
            })
            .height(DIVIDER)
            .into()
    }

    /// The history's edge, as the Slint client's `HistoryEdge`: Discord's
    /// welcome at a server channel's start, a line at another's, else the
    /// Earlier messages button, loading or failed.
    pub(super) fn history_edge(&self) -> Element<'_, Message> {
        if self.edge.welcome() {
            return self.welcome();
        }
        self.edge_line(&self.edge, Message::Earlier)
    }

    /// The history's edge of a timeline whose start has no welcome: where
    /// it starts, or Earlier messages (`earlier`), loading, or failed.
    pub fn edge_line(&self, edge: &Edge, earlier: Message) -> Element<'_, Message> {
        let theme = &self.theme;
        let shown: Element<'_, Message> = if edge.start {
            let words = if edge.direct {
                format!("This is the beginning of your messages with {}.", edge.name)
            } else if edge.thread {
                format!("This is the start of the {} thread.", edge.name)
            } else {
                format!("This is the start of #{}.", edge.name)
            };
            text(words).size(14).color(theme.muted).into()
        } else {
            self.edge_button(edge.loading, edge.failed, false, earlier)
        };
        container(shown)
            .width(Length::Fill)
            .height(EDGE)
            .padding(Padding {
                top: 4.0,
                bottom: 8.0,
                ..Padding::ZERO
            })
            .center_x(Length::Fill)
            .align_y(Alignment::Center)
            .into()
    }

    /// Earlier messages, or with `newer` Newer messages, as a pill that
    /// loads them (`load`): loading, or failed and offering a retry.
    pub(super) fn edge_button(
        &self,
        loading: bool,
        failed: bool,
        newer: bool,
        load: Message,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let which = if newer { "newer" } else { "earlier" };
        let (icon, ink, words) = if loading {
            (
                Icon::History,
                theme.secondary,
                format!("Loading {which} messages..."),
            )
        } else if failed {
            (
                Icon::TriangleAlert,
                theme.danger,
                format!("Couldn't load {which} messages. Retry"),
            )
        } else if newer {
            (Icon::History, theme.secondary, "Newer messages".to_owned())
        } else {
            (
                Icon::History,
                theme.secondary,
                "Earlier messages".to_owned(),
            )
        };
        let face = row![
            self.glyph(icon, 16, ink),
            text(words).size(14).font(MEDIUM).color(theme.secondary)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let enabled = !loading && !self.client.state().preview;
        let (rest, lit) = (theme.raised, theme.active);
        button(
            container(face)
                .padding(Padding {
                    left: 12.0,
                    right: 14.0,
                    ..Padding::ZERO
                })
                .height(30)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press_maybe(enabled.then_some(load))
        .style(crate::controls::lit_style(Some(rest), lit, 15.0))
        .into()
    }

    /// Discord's welcome: a server's in two lines over its line of text, a
    /// channel's under a hash in a circle.
    pub(super) fn welcome(&self) -> Element<'_, Message> {
        let edge = &self.edge;
        let theme = &self.theme;
        let title = |words: String, weight| {
            text(words)
                .size(32)
                .font(weight)
                .color(theme.text)
                .height(40)
                .wrapping(text::Wrapping::None)
        };
        let mut block = column![]
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .padding(Padding {
                top: 47.0,
                bottom: 12.0,
                ..Padding::ZERO
            });
        let note = match &edge.intro {
            Intro::Server(name) => {
                block = block
                    .push(title("Welcome to".to_owned(), SEMIBOLD))
                    .push(title(name.clone(), SEMIBOLD));
                text("This is the beginning of this server.").size(14)
            }
            _ => {
                let field = theme.field;
                block = block
                    .push(
                        container(self.glyph(Icon::Hash, 40, theme.text))
                            .center(68)
                            .style(move |_| rounded(field, 34)),
                    )
                    .push(Space::new().height(8))
                    .push(title(format!("Welcome to #{}!", edge.name), BOLD));
                text(format!("This is the start of the #{} channel.", edge.name)).size(16)
            }
        };
        block
            .push(Space::new().height(8))
            .push(note.color(theme.secondary).height(20))
            .height(self.edge.height())
            .into()
    }

    /// An empty conversation's opening, as the Slint client's
    /// `ConversationStart`.
    pub(super) fn conversation_start(&self) -> Element<'_, Message> {
        let edge = &self.edge;
        let theme = &self.theme;
        let mut start = column![].spacing(8).padding(24).width(Length::Fill);
        if self.client.state().selected.is_some() && edge.ready {
            if edge.direct {
                start = start.push(self.face(Face::new(
                    self.header.avatar.as_deref(),
                    &self.header.name,
                    72.0,
                )));
            } else {
                let raised = theme.raised;
                let icon = self.header.icon.unwrap_or(Icon::Hash);
                start = start.push(
                    container(self.glyph(icon, 36, theme.text))
                        .center(68)
                        .style(move |_| rounded(raised, 34)),
                );
            }
        }
        let (title, size) = if !edge.ready {
            ("Loading conversation...".to_owned(), 18)
        } else if edge.direct || edge.thread {
            (edge.name.clone(), 28)
        } else {
            (format!("Welcome to #{}", edge.name), 28)
        };
        start = start.push(text(title).size(size).font(BOLD).color(theme.text));
        if edge.ready {
            let words = if edge.direct {
                format!("This is the beginning of your messages with {}.", edge.name)
            } else if edge.thread {
                format!(
                    "This is the start of the {} thread. No messages here yet.",
                    edge.name
                )
            } else {
                format!("This is the start of #{}. No messages here yet.", edge.name)
            };
            start = start.push(text(words).color(theme.secondary));
        }
        container(start)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into()
    }

    /// While the "New" divider is out of view, until the newest message is
    /// read, a bar that jumps to it, saying how much is new.
    pub(super) fn unread_bar(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        if state.caught_up {
            return None;
        }
        let index = self.unread_index()?;
        let view_top = (self.total_height() - self.timeline_height - self.from_end).max(0.0);
        let view_bottom = view_top + self.timeline_height;
        let top = self.tops[index];
        let above = top < view_top - 20.0;
        if !above && top <= view_bottom {
            return None;
        }
        let theme = &self.theme;
        let label = unread_label(&self.messages[index..], state.detached);
        let chevron = if above {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        let face = row![
            text(label).size(14).font(BOLD).color(theme.on_accent),
            self.glyph(chevron, 16, theme.on_accent)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (rest, lit) = (theme.accent, theme.accent_hover);
        Some(
            button(
                container(face)
                    .padding(Padding {
                        left: 14.0,
                        right: 10.0,
                        ..Padding::ZERO
                    })
                    .height(UNREAD_BAR)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .on_press(Message::JumpUnread)
            .style(move |theme, status| button::Style {
                shadow: crate::style::bar_shadow(),
                ..crate::controls::lit_style(Some(rest), lit, UNREAD_BAR / 2.0)(theme, status)
            })
            .into(),
        )
    }

    /// A window of older messages says so over its end, with the way back.
    pub(super) fn present_bar(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let face = row![
            text("You're viewing older messages")
                .size(14)
                .font(MEDIUM)
                .color(theme.secondary)
                .width(Length::Fill),
            text("Jump to present")
                .size(14)
                .font(BOLD)
                .color(theme.accent),
            self.glyph(Icon::ChevronDown, 16, theme.accent),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (ground, edge) = (theme.raised, theme.edge);
        button(
            container(face)
                .padding(Padding {
                    left: 14.0,
                    right: 12.0,
                    ..Padding::ZERO
                })
                .height(PRESENT_BAR)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::JumpPresent)
        .style(move |_, _| button::Style {
            background: Some(ground.into()),
            border: crate::style::edge(edge, 8.0),
            shadow: crate::style::bar_shadow(),
            ..button::Style::default()
        })
        .into()
    }
}
