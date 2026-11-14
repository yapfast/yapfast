//! Discord apps' components on a message, as the Slint client's `CardView`
//! shows them: a container of components (framed, with a bar of its
//! colour) or those outside one; text beside a thumbnail or a button,
//! galleries and files, gaps and lines, rows of buttons, and menus. A link
//! button opens its page; a button that asks its app does so through the
//! core, which holds the message's other controls back until the app
//! answers; why one failed shows under them. A menu's list comes with the
//! dialogs of a later milestone, so a menu shows but doesn't open yet.

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Size,
    widget::{Space, column, container, row, text},
};
use yapfast_core::chat::{Beside, ButtonStyle, Card, CardButton, CardRow, ChatMessage, Placement};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action},
    markdown::{Body, FIRST_PART, Look},
    media::{Fit, Gallery, Shot},
    message::{PICTURE_RADIUS, files, reveal_key},
    pictures::Icon,
    style::rounded,
    theme::rgb,
};

/// A framed card at its widest, a section's thumbnail, a gallery's room
/// inside a card, a menu's box at its widest.
const WIDTH: f32 = 600.0;
const THUMBNAIL: f32 = 85.0;
const GALLERY: f32 = 566.0;
const SELECT: f32 = 400.0;
/// Its text's pieces in the selection, after the message's and its
/// embeds' (left out of copies).
const CARD_PARTS: u8 = 200;

/// A card's text rows laid out, in order.
pub fn laid(card: &Card, laid: impl Fn(&str) -> Body) -> Vec<Body> {
    card.rows
        .iter()
        .filter_map(|row| match row {
            CardRow::Text { text, .. } => Some(laid(text)),
            _ => None,
        })
        .collect()
}

/// A card's height, for estimates.
pub fn estimate(card: &Card) -> f32 {
    let rows: f32 = card
        .rows
        .iter()
        .map(|row| match row {
            CardRow::Text { .. } => 40.0,
            CardRow::Media(_) => 280.0,
            CardRow::Separator { large, .. } => {
                if *large {
                    17.0
                } else {
                    1.0
                }
            }
            CardRow::Buttons(_) => 28.0,
            CardRow::Select { .. } => 40.0,
            CardRow::Unknown => 52.0,
        })
        .sum();
    rows + card.rows.len().saturating_sub(1) as f32 * 8.0
        + if card.framed { 32.0 } else { 0.0 }
        + 4.0
}

impl App {
    /// A message's card at `place`, its text laid out in `bodies`.
    pub fn app_card<'a>(
        &'a self,
        index: usize,
        message: &'a ChatMessage,
        card: &'a Card,
        bodies: &'a [Body],
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let busy = state.interactions.newest_busy() == Some(message.item.as_str());
        let mut rows = column![].spacing(8).width(Length::Fill);
        let mut texts = bodies.iter().enumerate();
        for each in &card.rows {
            let shown: Element<'_, Message> = match each {
                CardRow::Text { beside, .. } => {
                    let Some((place, body)) = texts.next() else {
                        continue;
                    };
                    let first =
                        CARD_PARTS.saturating_add(u8::try_from(place * 8).unwrap_or(u8::MAX));
                    let look = Look {
                        size: crate::timeline::FONT_SIZE,
                        line_height: crate::timeline::LINE_HEIGHT,
                        ink: theme.text,
                    };
                    let mut line = row![
                        container(self.body_view(body, index, first, look)).width(Length::Fill)
                    ]
                    .spacing(12);
                    match beside {
                        Some(Beside::Thumbnail(slot)) => {
                            if let Some(at) = message.attachments.iter().position(|attachment| {
                                attachment.placement == Placement::Component(*slot)
                            }) {
                                let attachment = &message.attachments[at];
                                line = line.push(
                                    self.shot(
                                        Shot::new(
                                            attachment.preview.as_ref(),
                                            Size::new(THUMBNAIL, THUMBNAIL),
                                        )
                                        .fit(Fit::Cover)
                                        .radius(PICTURE_RADIUS)
                                        .covered(
                                            attachment.spoiler
                                                && !self
                                                    .revealed
                                                    .contains(&reveal_key(&message.item, at)),
                                        ),
                                    ),
                                );
                            }
                        }
                        Some(Beside::Button(button)) => {
                            line = line.push(self.card_button(message, button, busy));
                        }
                        None => {}
                    }
                    line.into()
                }
                CardRow::Media(slot) => {
                    let placement = Placement::Component(*slot);
                    let room =
                        GALLERY.min(self.content_width() - if card.framed { 32.0 } else { 0.0 });
                    let gallery = Gallery::of(&message.attachments, placement, room);
                    let mut media = column![].spacing(4);
                    if !gallery.is_empty() {
                        media = media.push(self.gallery(index, message, &gallery));
                    }
                    for attachment in files(message, placement) {
                        media = media.push(self.file_card(attachment));
                    }
                    media.into()
                }
                // With the spacing either side, a small gap is 16 px and a
                // large one 32, a divider's line between.
                CardRow::Separator { divider, large } => {
                    let gap = if *large { 16.0 } else { 0.0 };
                    let mut separator = column![Space::new().height(gap / 2.0)];
                    if *divider {
                        separator = separator.push(controls::bar(theme.line, Length::Fill, 1));
                    }
                    separator.push(Space::new().height(gap / 2.0)).into()
                }
                CardRow::Buttons(buttons) => {
                    let mut line = row![].spacing(8);
                    for button in buttons {
                        line = line.push(self.card_button(message, button, busy));
                    }
                    line.into()
                }
                CardRow::Select {
                    text: shown,
                    disabled,
                    options,
                    ..
                } => self.select_box(
                    shown,
                    options.iter().any(|option| option.default),
                    !disabled && !busy,
                ),
                // As Discord's client tells of components it doesn't know.
                CardRow::Unknown => {
                    let accent = theme.accent;
                    container(
                        text("Yapfast can't show this part of the message yet.")
                            .size(14)
                            .color(theme.secondary),
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        border: Border {
                            color: accent,
                            width: 1.0,
                            radius: 12.0.into(),
                        },
                        ..container::Style::default()
                    })
                    .into()
                }
            };
            rows = rows.push(shown);
        }
        let shown: Element<'_, Message> = if card.framed {
            let (ground, edge) = (theme.sidebar, theme.edge);
            let inner = container(rows).padding(16).width(Length::Fill);
            let inner: Element<'_, Message> = match card.color {
                Some(color) => iced::widget::stack![
                    inner,
                    controls::bar(crate::style::hex(color), 4, Length::Fill)
                ]
                .into(),
                None => inner.into(),
            };
            container(inner)
                .max_width(WIDTH)
                .clip(true)
                .style(move |_| container::Style {
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..rounded(ground, 8)
                })
                .into()
        } else {
            rows.into()
        };
        container(shown).padding(Padding::ZERO.top(4)).into()
    }

    /// An app's button, as the Slint client's `CardButtonView`: a link's
    /// with its icon opens its page, a store's offers to open in Discord,
    /// one that asks its app does, unless another of the message's is
    /// waiting for its app.
    fn card_button<'a>(
        &'a self,
        message: &ChatMessage,
        card_button: &'a CardButton,
        busy: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let kind = match card_button.style {
            ButtonStyle::Primary => Kind::Primary,
            ButtonStyle::Success => Kind::Success,
            ButtonStyle::Danger => Kind::Danger,
            ButtonStyle::Secondary | ButtonStyle::Link | ButtonStyle::Premium => Kind::Plain,
        };
        let store = match (&card_button.sku_id, &message.application_id) {
            (Some(sku), Some(app)) => Some(format!(
                "https://discord.com/application-directory/{app}/store/{sku}"
            )),
            _ => None,
        };
        let press = if card_button.disabled {
            None
        } else if let Some(custom_id) = &card_button.custom_id {
            (!busy).then(|| Message::UseButton(message.item.clone(), custom_id.clone()))
        } else if let Some(store) = store {
            Some(Message::Link(store, Point::ORIGIN))
        } else {
            card_button
                .url
                .clone()
                .map(|url| Message::Link(url, Point::ORIGIN))
        };
        let label = if card_button.style == ButtonStyle::Premium {
            "Open in Discord"
        } else {
            card_button.label.as_str()
        };
        let mut face = action(theme, label, kind, press).small();
        if card_button.style == ButtonStyle::Link {
            face = face.icon(self.icon(Icon::Link, 16, rgb(theme.text)));
        }
        face.into()
    }

    /// A menu's box, as the Slint client's `BotSelectBox`: what it picked,
    /// or its placeholder, and a chevron.
    fn select_box<'a>(
        &'a self,
        shown: &'a str,
        picked: bool,
        enabled: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let (ground, edge) = (theme.field, theme.edge);
        let face = row![
            text(shown)
                .font(crate::fonts::MEDIUM)
                .color(if picked { theme.text } else { theme.secondary })
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            self.glyph(Icon::ChevronDown, 18, theme.secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let opacity = if enabled { 1.0 } else { 0.5 };
        container(
            container(face)
                .padding([0, 12])
                .height(40)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |_| container::Style {
                    background: Some(crate::style::faded(ground, opacity).into()),
                    border: Border {
                        color: crate::style::faded(edge, opacity),
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..container::Style::default()
                }),
        )
        .max_width(SELECT)
        .into()
    }

    /// Why the last control of the message's used failed, under its
    /// controls, as Discord's client says.
    pub fn bot_failed(&self, message: &ChatMessage) -> Option<Element<'_, Message>> {
        let (item, why) = self.client.state().interaction_failed.as_ref()?;
        if *item != message.item {
            return None;
        }
        let theme = &self.theme;
        Some(
            container(
                row![
                    self.glyph(Icon::TriangleAlert, 16, theme.danger),
                    text(why.as_str())
                        .size(14)
                        .color(theme.danger)
                        .width(Length::Fill)
                ]
                .spacing(6),
            )
            .padding(Padding::ZERO.top(4))
            .into(),
        )
    }
}

/// A card's pieces in the selection start after the message's own.
const _: () = assert!(CARD_PARTS > FIRST_PART);
