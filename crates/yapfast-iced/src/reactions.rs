//! Who reacted: a message's reactions down the left, the most first, and
//! the people who reacted with the one chosen, a page at a time as the list
//! nears its end; this account's own reaction, or as a moderator anyone's,
//! can be taken off. A poll's voters show in it too, by answer.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{Space, button, column, container, mouse_area, row, text},
};
use yapfast_core::application::{Action, AppState};

use crate::{
    app::Message,
    controls::{self, bar, lit_style},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::dialog,
    fonts::{BOLD, MEDIUM},
    message::emoji_shortcode,
    pictures::Icon,
    style::scrollbar,
    sync::Scroll,
    widgets::Pic,
};

/// A person's row, and the room before the list's end at which the next
/// page is asked for.
const ROW: f32 = 52.0;

#[derive(Clone, Debug)]
pub enum Event {
    /// Another reaction, or answer, by its key.
    Choose(String),
    Remove(String),
    Close,
    Scrolled(Scroll),
    /// The pointer on a person's row, or off it.
    Pointed(Option<String>),
}

/// What the window keeps beyond the core's state.
#[derive(Default)]
pub struct Reactions {
    scroll: Option<Scroll>,
    pointed: Option<String>,
}

impl Reactions {
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Choose(key) => {
                self.scroll = None;
                cx.dispatch(Action::ShowReactors(key));
            }
            Event::Remove(user) => cx.dispatch(Action::RemoveReactor(user)),
            Event::Close => {
                cx.dispatch(Action::CloseReactions);
                *self = Reactions::default();
                cx.request(Request::Focus(Focus::Default));
            }
            Event::Scrolled(scroll) => {
                self.scroll = Some(scroll);
                self.page(scroll.height, cx);
                cx.request(Request::Ask);
            }
            Event::Pointed(pointed) => self.pointed = pointed,
        }
        Task::none()
    }

    /// The next page comes as the list nears its end, as in Discord's
    /// client: at a scroll, and when a page comes short of the view, which
    /// is `height` tall until it's scrolled.
    pub fn page(&self, height: f32, cx: &mut Cx<'_>) {
        let state = cx.state();
        let Some(window) = &state.reactions else {
            return;
        };
        if window.loading || !state.more_reactors() {
            return;
        }
        let scroll = self.scroll.unwrap_or(Scroll::new(height));
        let content = window.people.len() as f32 * ROW + 16.0;
        if scroll.offset + scroll.height >= content - ROW {
            cx.dispatch(Action::MoreReactors);
        }
    }
}

/// The window over the dimmed window, while it's open.
pub fn view<'a>(reactions: &'a Reactions, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: reactions, cx }.view()
}

/// What the window keeps and reads, as its parts draw them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Reactions,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        let window = state.reactions.as_ref()?;
        let theme = self.cx.look.theme;
        let close = button(self.cx.look.glyph(Icon::X, 18, theme.secondary).boxed(28))
            .padding(0)
            .on_press(Message::Reactions(Event::Close))
            .style(lit_style(None, theme.hover, 6.0));
        let head = row![
            text(if window.poll { "Votes" } else { "Reactions" })
                .size(20)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            close
        ]
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            left: 24.0,
            right: 16.0,
            bottom: 12.0,
        });
        // The reactions, the most first, or the answers.
        let mut pills = column![].padding(Padding {
            top: 12.0,
            right: 16.0,
            ..Padding::ZERO
        });
        let message = state.message(&window.item);
        if window.poll {
            for answer in state.window_answers() {
                let key = answer.id.to_string();
                let emoji = answer
                    .emoji
                    .as_deref()
                    .map(|emoji| self.reaction_face(message, emoji));
                pills = pills.push(self.pill(
                    key == window.key,
                    emoji,
                    Some(answer.text.as_str()),
                    answer.votes,
                    key,
                ));
            }
        } else {
            for reaction in state.window_reactions() {
                pills = pills.push(self.pill(
                    reaction.key == window.key,
                    Some(self.reaction_face(message, &reaction.key)),
                    None,
                    reaction.count,
                    reaction.key.clone(),
                ));
            }
        }
        let side = if window.poll { 220.0 } else { 98.0 };
        let pills = row![
            scroll_view(pills)
                .width(side - 1.0)
                .height(Length::Fill)
                .style(scrollbar(theme)),
            bar(theme.line, 1, Length::Fill),
        ];
        // Who reacted with the chosen one.
        let mut people = column![].padding([8, 0]);
        for person in &window.people {
            let removable = !window.poll && (person.me || window.can_remove);
            let pointed = self.s.pointed.as_deref() == Some(person.id.as_str());
            let avatar = (!person.avatar.is_empty()).then_some(person.avatar.as_str());
            let mut words = column![
                text(person.name.as_str())
                    .size(16)
                    .font(MEDIUM)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None)
            ]
            .width(Length::Fill);
            if !person.username.is_empty() {
                words = words.push(
                    text(person.username.as_str())
                        .size(12)
                        .color(theme.muted)
                        .wrapping(text::Wrapping::None),
                );
            }
            let mut line = row![self.cx.avatar(avatar, &person.name, 32.0, 16.0, 1.0), words]
                .spacing(10)
                .padding([0, 16])
                .height(ROW)
                .align_y(Alignment::Center);
            if removable && pointed {
                let remove = button(self.cx.look.glyph(Icon::X, 16, theme.secondary).boxed(28))
                    .padding(0)
                    .on_press(Message::Reactions(Event::Remove(person.id.clone())))
                    .style(lit_style(None, theme.hover, 6.0));
                line = line.push(
                    iced::widget::tooltip(
                        remove,
                        controls::tip(theme, "Remove Reaction"),
                        iced::widget::tooltip::Position::Top,
                    )
                    .gap(6),
                );
            }
            people = people.push(
                mouse_area(line)
                    .on_enter(Message::Reactions(Event::Pointed(Some(person.id.clone()))))
                    .on_exit(Message::Reactions(Event::Pointed(None))),
            );
        }
        if window.loading || state.more_reactors() {
            people = people.push(
                container(text("Loading...").size(14).color(theme.secondary))
                    .center_x(Length::Fill)
                    .height(ROW)
                    .align_y(Alignment::Center),
            );
        }
        if let Some(error) = &window.error {
            people = people
                .push(container(text(error.as_str()).size(14).color(theme.secondary)).padding(16));
        }
        let people = scroll_view(people)
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(|viewport| Message::Reactions(Event::Scrolled(Scroll::of(viewport))))
            .style(scrollbar(theme));
        let body = column![
            head,
            bar(theme.line, Length::Fill, 1),
            row![pills, people]
                .padding(Padding {
                    left: 24.0,
                    ..Padding::ZERO
                })
                .height(Length::Fill),
        ]
        .padding(Padding {
            top: 16.0,
            ..Padding::ZERO
        });
        let width = 600.0_f32.min(self.cx.look.window.width - 48.0);
        let height = 720.0_f32.min(self.cx.look.window.height - 96.0);
        Some(dialog(
            theme,
            container(body).height(height).into(),
            width,
            Message::Reactions(Event::Close),
        ))
    }

    /// A reaction's or an answer's row on the left: its emoji, its name for
    /// an answer, and how many.
    fn pill(
        self,
        chosen: bool,
        emoji: Option<Element<'a, Message>>,
        name: Option<&'a str>,
        count: usize,
        key: String,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        if let Some(emoji) = emoji {
            face = face.push(container(emoji).center_x(24));
        }
        if let Some(name) = name {
            face = face.push(
                text(name)
                    .size(14)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill),
            );
        }
        let count_words = count.to_string();
        let count_words = if name.is_none() {
            self.cx.themes.label(
                &theme.id,
                yapfast_core::themes::chrome::Label::Reaction,
                count_words.clone(),
                &[("n", count_words.as_str())],
            )
        } else {
            count_words
        };
        let count_font = self
            .cx
            .themes
            .chrome(&theme.id)
            .and_then(|chrome| chrome.font)
            .map_or("body", yapfast_core::themes::package::Role::name);
        face = face.push(
            text(count_words)
                .size(14)
                .font(theme.font(count_font, BOLD))
                .color(theme.text),
        );
        let (active, hover) = (theme.active, theme.hover);
        column![
            button(
                container(face)
                    .padding(Padding {
                        left: 4.0,
                        right: 8.0,
                        ..Padding::ZERO
                    })
                    .height(32)
                    .align_y(Alignment::Center)
            )
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Reactions(Event::Choose(key)))
            .style(move |_, status| button::Style {
                background: if chosen {
                    Some(active.into())
                } else if controls::lights(status) {
                    Some(hover.into())
                } else {
                    None
                },
                border: crate::style::round(8),
                ..Default::default()
            }),
            Space::new().height(8),
        ]
        .into()
    }

    /// A reaction's emoji: a custom one's picture, as the message has it,
    /// or the Unicode one drawn from Twemoji.
    fn reaction_face(
        self,
        message: Option<&yapfast_core::chat::ChatMessage>,
        key: &str,
    ) -> Element<'a, Message> {
        let custom = message
            .and_then(|message| message.emoji.iter().find(|emoji| emoji.text == key))
            .and_then(|emoji| self.cx.picture(&emoji.picture, 20, 20, false));
        if let Some(picture) = custom {
            return Pic::square(picture, 20.0).into();
        }
        match emojis::get(key)
            .map(emojis::Emoji::as_str)
            .and_then(|key| self.cx.emoji_picture(key, 20))
        {
            Some(picture) => Pic::square(picture, 20.0).into(),
            None => text(emoji_shortcode(key)).size(12).into(),
        }
    }
}

/// The pictures the window shows: who reacted, and custom emoji.
pub fn pictures(state: &AppState) -> Vec<String> {
    let Some(window) = &state.reactions else {
        return Vec::new();
    };
    let mut urls: Vec<String> = window
        .people
        .iter()
        .filter(|person| !person.avatar.is_empty())
        .map(|person| person.avatar.clone())
        .collect();
    if let Some(message) = state.message(&window.item) {
        urls.extend(message.emoji.iter().map(|emoji| emoji.picture.clone()));
    }
    urls
}
