//! One message's content in the timeline, as the Slint client's
//! `MessageContent`: the gutter (the author's avatar, a notice's icon, the
//! time beside a grouped message under the pointer, a reply's spine), the
//! line above (the reply it quotes, the command it answers), the author
//! line, the text in its blocks, `(edited)`, picture previews (GIFs
//! playing), and reaction chips with who reacted in their tooltips.

use std::time::Duration;

use iced::{
    Alignment, Color, Element, Length, Padding, Point, Size,
    widget::{Space, button, column, container, mouse_area, row, span, stack, text, tooltip},
};
use yapfast_core::chat::{self, Attachment, ChatMessage, NoticeKind, Placement};

use crate::{
    app::{App, Message},
    controls::{self, placed},
    fonts::{BOLD, ITALIC, MEDIUM, SEMIBOLD, shown_name, style_font},
    looks::{self, Chip},
    markdown::{FIRST_PART, Look},
    media::{self, Fit, Gallery, Shot},
    menus::MenuAct,
    pictures::{Face, Icon},
    select::Piece,
    style::{CHIP, hex, rounded},
    timeline::{
        AUTHOR_GAP, AUTHOR_LINE, AVATAR, CONTENT_PAD, END_PAD, EmbedText, GUTTER, LINE_HEIGHT,
        RowLook,
    },
    widgets::{Fold, Pic},
};

/// The line above a reply or a command's answer, and its avatar.
const HEADER_LINE: f32 = 22.0;
const HEADER_AVATAR: f32 = 16.0;
/// An embed at its widest, its bar, its thumbnail at its widest, and the
/// pieces its text is in the selection (left out of copies).
const EMBED_WIDTH: f32 = 432.0;
const EMBED_BAR: f32 = 4.0;
const THUMBNAIL: f32 = 80.0;
/// Past any message's body's parts (Discord's 4000 characters make far
/// fewer), each embed 16 of them, before a bot's cards' (`cards.rs`).
pub const EMBED_PARTS: crate::select::PartNo = 16384;
/// A file's card at its widest, and its icon.
const FILE_CARD_WIDTH: f32 = 432.0;
const FILE_ICON: u32 = 30;
/// An embed's padding across, around its text and pictures.
const EMBED_PADDING: f32 = 4.0 + 12.0 + 16.0;
/// A picture's corners in a message.
/// A custom emoji's picture on a reaction's chip.
const REACTION_EMOJI: u32 = 18;
pub const PICTURE_RADIUS: f32 = 8.0;
/// A message's sticker, as Discord draws it whatever size its picture is.
pub const STICKER: f32 = 160.0;
/// The play mark over a video's poster, and its ground.
const PLAY_BADGE: f32 = 48.0;
const PLAY_GROUND: Color = Color::from_rgba8(0, 0, 0, 0xb3 as f32 / 255.0);
/// The reply spine's square.
const SPINE: u32 = 33;
/// The hover bar's buttons.
const BAR_BUTTON: f32 = 32.0;
/// Discord's flag on an answer only this account sees.
const EPHEMERAL: u64 = 1 << 6;

impl App {
    /// Row `index`'s message, beside its gutter.
    pub fn message_content<'a>(&'a self, index: usize, look: &'a RowLook) -> Element<'a, Message> {
        let Some(message) = self.row_message(index) else {
            return Space::new().into();
        };
        if message.blocked && !self.revealed.contains(&blocked_key(&message.item)) {
            return self.blocked_row(look);
        }
        let theme = &self.theme;
        let header = message.reply.is_some() || message.used.is_some();
        let mut content = column![].width(Length::Fill);
        if let Some(reply) = &message.reply {
            content = content.push(self.reply_line(reply));
        } else if let Some(used) = &message.used {
            content = content.push(self.used_line(used));
        }
        if look.first && message.notice.is_none() {
            content = content.push(
                container(self.author_line(index, message))
                    .padding(Padding::ZERO.bottom(AUTHOR_GAP)),
            );
        }
        if message.forwarded {
            content = content.push(
                row![
                    self.glyph(Icon::Forward, 16, theme.muted),
                    text("Forwarded").size(14).font(ITALIC).color(theme.muted)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
        let ink = if message.deleted || message.delivery != chat::Delivery::default() {
            theme.muted
        } else if message.notice.is_some() {
            theme.secondary
        } else {
            theme.text
        };
        let editing = self.editing.as_deref() == Some(message.item.as_str());
        if editing {
            content = content.push(self.inline_edit());
        } else if !look.body.parts.is_empty() && !look.body.plain().is_empty() {
            let body = self.body_view(
                &look.body,
                index,
                FIRST_PART,
                Look {
                    size: self.text_size(),
                    line_height: LINE_HEIGHT,
                    ink,
                },
            );
            content = content.push(if self.layout.collapse_long {
                self.folding(index, body)
            } else {
                body
            });
        }
        if message.edited && message.notice.is_none() && !editing {
            content = content.push(text("(edited)").size(10).color(theme.muted));
        }
        let gallery = Gallery::of(
            &message.attachments,
            Placement::Message,
            self.content_width_of(index),
        );
        if !gallery.is_empty() {
            content = content.push(
                container(self.gallery(index, message, &gallery)).padding(Padding::ZERO.top(4)),
            );
        }
        if let Some(stickers) = self.stickers(message) {
            content = content.push(container(stickers).padding(Padding::ZERO.top(4)));
        }
        for (at, (index, attachment)) in files(message, Placement::Message).enumerate() {
            // A voice message's recording is its first attachment.
            if at == 0
                && let Some(voice) = &message.voice
            {
                content = content.push(self.voice_card(message, voice));
                continue;
            }
            content = content.push(
                container(self.file_card(&message.item, index, attachment))
                    .padding(Padding::ZERO.top(4)),
            );
        }
        if let Some(poll) = &message.poll {
            content = content.push(self.poll_view(message, poll));
        }
        if self.layout.prefs.link_previews.unwrap_or(true) {
            for (place, embed) in message.embeds.iter().enumerate() {
                if let Some(laid) = look.body.embeds.get(place) {
                    content = content.push(self.embed(index, message, place, embed, laid));
                }
            }
        } else if let Some(hidden) = hidden_card(message) {
            // A bot's card can be the whole message: say it's there.
            content = content.push(text(hidden).size(self.text_size()).color(self.theme.muted));
        }
        for (place, card) in message.cards.iter().enumerate() {
            if let Some(bodies) = look.body.cards.get(place) {
                content = content.push(self.app_card(index, message, card, bodies));
            }
        }
        if let Some(failed) = self.bot_failed(message) {
            content = content.push(failed);
        }
        if let Some(thread) = &message.thread {
            content = content.push(self.thread_chip(thread));
        }
        if !message.reactions.is_empty() {
            content =
                content.push(container(self.reactions(message)).padding(Padding::ZERO.top(4)));
        }
        if let Some(delivery) = self.delivery_line(message) {
            content = content.push(delivery);
        }
        if message.flags & EPHEMERAL != 0 {
            content = content.push(self.ephemeral_line(message));
        }
        let gutter = self.gutter(index, message, look, header);
        let shown = container(row![gutter, content].spacing(16))
            .width(Length::Fill)
            .padding(Padding {
                top: CONTENT_PAD,
                bottom: CONTENT_PAD,
                left: (GUTTER - AVATAR) / 2.0,
                right: if editing { 21.0 } else { END_PAD },
            });
        if !header {
            return shown.into();
        }
        // The spine from the line above down to the avatar, over the
        // gutter's edge and the space after it.
        let spine = self.glyph(Icon::ReplySpine, SPINE, theme.reply_spine);
        stack![
            shown,
            placed(spine, (GUTTER - AVATAR) / 2.0 + 19.0, CONTENT_PAD + 11.0)
        ]
        .into()
    }

    /// A message from someone this account blocked, as Discord shows it
    /// until asked: "Blocked user" by a blank face, and "Blocked message."
    /// with Show it.
    fn blocked_row<'a>(&'a self, look: &RowLook) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut content = column![].width(Length::Fill);
        let mut gutter =
            stack![
                Space::new()
                    .width(AVATAR)
                    .height(if look.first { AVATAR } else { 0.0 })
            ];
        if look.first {
            content = content.push(
                container(
                    text("Blocked user")
                        .size(self.text_size())
                        .font(MEDIUM)
                        .color(theme.text)
                        .height(AUTHOR_LINE),
                )
                .padding(Padding::ZERO.bottom(AUTHOR_GAP)),
            );
            gutter = gutter.push(placed(
                self.avatar(None, "?", AVATAR, AVATAR / 2.0, 1.0),
                0.0,
                2.0,
            ));
        }
        content = content.push(
            row![
                text("Blocked message.")
                    .size(self.text_size())
                    .font(ITALIC)
                    .color(theme.text),
                button(text("Show it").size(self.text_size()))
                    .padding(0)
                    .on_press(Message::ShowBlocked(look.item.clone()))
                    .style(controls::ink_style(theme.link, theme.text)),
            ]
            .spacing(4)
            .height(LINE_HEIGHT)
            .align_y(Alignment::Center),
        );
        container(row![container(gutter).width(AVATAR), content].spacing(16))
            .width(Length::Fill)
            .padding(Padding {
                top: CONTENT_PAD,
                bottom: CONTENT_PAD,
                left: (GUTTER - AVATAR) / 2.0,
                right: END_PAD,
            })
            .into()
    }

    /// What's left of a message: its author's avatar, a notice's icon, the
    /// clock time of a grouped message under the pointer, and the spine
    /// that leads a reply's line to its avatar.
    fn gutter<'a>(
        &'a self,
        index: usize,
        message: &'a ChatMessage,
        look: &RowLook,
        header: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut layers =
            stack![
                Space::new()
                    .width(AVATAR)
                    .height(if look.first && message.notice.is_none() {
                        if header { 64.0 } else { AVATAR }
                    } else {
                        0.0
                    })
            ];
        if let Some(notice) = &message.notice {
            let (icon, ink) = match notice.kind {
                NoticeKind::Pinned(_) => (Icon::Pin, theme.muted),
                NoticeKind::Joined => (Icon::UserPlus, theme.live),
                NoticeKind::ThreadStarted => (Icon::Thread, theme.muted),
                NoticeKind::Call => (Icon::Phone, theme.live),
                NoticeKind::PollResult => (Icon::ChartBar, theme.muted),
            };
            layers = layers.push(placed(
                self.glyph(icon, 16, ink),
                12.0,
                (self.text_size() * 1.375 - 16.0) / 2.0 + 1.0,
            ));
        } else if look.first {
            let decoration = self
                .client
                .state()
                .cosmetics
                .decoration(message.decoration.as_deref());
            let face = self.face(Face {
                url: message.avatar.as_deref(),
                name: &message.author,
                key: None,
                size: AVATAR,
                opacity: 1.0,
                decoration,
                play: self.pointed.message.as_deref() == Some(message.item.as_str()),
                dot: None,
            });
            layers = layers.push(placed(
                mouse_area(face).on_press(Message::OpenAuthor(look.item.clone())),
                0.0,
                if header { 24.0 } else { 2.0 },
            ));
        } else if self.pointed.message.as_deref() == Some(message.item.as_str()) {
            // Right-aligned to the avatar's edge.
            let clock = self
                .sent_time(index, message)
                .split_whitespace()
                .find(|word| word.contains(':'))
                .unwrap_or_default();
            layers = layers.push(placed(
                container(
                    text(clock)
                        .size(12)
                        .font(MEDIUM)
                        .color(theme.muted)
                        .wrapping(text::Wrapping::None),
                )
                .width(AVATAR)
                .align_x(Alignment::End),
                0.0,
                4.0,
            ));
        }
        container(layers).width(AVATAR).into()
    }

    /// The quote a reply leads with: its author's avatar and "@name" in
    /// their colour, its text on one line; a click jumps to it.
    fn reply_line<'a>(&'a self, reply: &'a chat::Reply) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut line = row![].spacing(6).align_y(Alignment::Center);
        if !reply.author.is_empty() {
            line = line
                .push(self.face(Face {
                    url: reply.avatar.as_deref(),
                    name: &reply.author,
                    key: None,
                    size: HEADER_AVATAR,
                    opacity: 1.0,
                    decoration: None,
                    play: false,
                    dot: None,
                }))
                .push(
                    text(format!("@{}", reply.author))
                        .size(14)
                        .color(reply.color.map_or(theme.secondary, hex))
                        .wrapping(text::Wrapping::None),
                );
        }
        let one_line = reply.body.split_whitespace().collect::<Vec<_>>().join(" ");
        line = line.push(
            container(
                text(one_line)
                    .size(13)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::None),
            )
            .width(Length::Fill)
            .clip(true),
        );
        let line = container(line)
            .height(HEADER_LINE)
            .align_y(Alignment::Center);
        match &reply.item {
            Some(item) => button(line)
                .padding(0)
                .width(Length::Fill)
                .on_press(Message::Link(
                    format!("{}{item}", crate::timeline::JUMP_LINK),
                    Point::ORIGIN,
                ))
                .style(|_, _| button::Style::default())
                .into(),
            None => line.into(),
        }
    }

    /// An app's answer's line: "Ann used /ping".
    fn used_line<'a>(&'a self, used: &'a chat::UsedCommand) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut line = row![
            self.face(Face {
                url: used.avatar.as_deref(),
                name: &used.user,
                key: None,
                size: HEADER_AVATAR,
                opacity: 1.0,
                decoration: None,
                play: false,
                dot: None,
            }),
            text(used.user.as_str())
                .size(14)
                .font(MEDIUM)
                .color(used.color.map_or(theme.secondary, hex)),
            text("used").size(14).color(theme.muted),
            text(used.name.as_str())
                .size(14)
                .font(MEDIUM)
                .color(theme.link),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some(target) = used.target.as_deref().filter(|target| !target.is_empty()) {
            line = line.push(text(format!("on @{target}")).size(14).color(theme.muted));
        }
        container(line)
            .height(HEADER_LINE)
            .align_y(Alignment::Center)
            .clip(true)
            .into()
    }

    /// Messages' text size, as Messages' settings chose it.
    pub fn text_size(&self) -> f32 {
        crate::settings::message_size(self.layout.prefs.text_size)
    }

    /// When the message at `index` was sent, on the clock Messages'
    /// settings chose.
    fn sent_time<'a>(&'a self, index: usize, message: &'a ChatMessage) -> &'a str {
        self.row_look(index)
            .map_or(message.time.as_str(), |look| look.time.as_str())
    }

    /// A message's author line, as the Slint client's with its Discord
    /// look: the name in its style's font (only the font, in a server) and
    /// role colour, the server tag chip, APP for a bot, the time.
    fn author_line<'a>(&'a self, index: usize, message: &'a ChatMessage) -> Element<'a, Message> {
        let theme = &self.theme;
        let cosmetics = self.client.state().cosmetics;
        let style = looks::name_look(
            cosmetics.style(message.style.as_deref()),
            &message.author,
            self.in_server,
            theme.dark,
        );
        let color = message.color.map_or(theme.text, hex);
        // Out of a server a whole style shows; its first colour stands in
        // for the effect in a timeline row.
        let color = match &style {
            Some(style) if !self.in_server => style.colors.first().map_or(color, |rgb| hex(*rgb)),
            _ => color,
        };
        let name = Piece::new(
            vec![
                span(shown_name(&message.author, style.as_ref()).into_owned())
                    .font(style_font(style.as_ref(), 500)),
            ],
            index,
            0,
        )
        .item(Some(self.row_item(index)))
        .size(self.text_size(), AUTHOR_LINE)
        .color(color)
        .starts(false)
        .selection(self.selected(index, 0), self.selecting, theme.selection);
        let mut line = row![mouse_area(name).on_press(Message::OpenAuthor(self.row_item(index)))]
            .spacing(8)
            .align_y(Alignment::Center);
        if let Some(tag) = cosmetics.tag(message.tag.as_ref()) {
            let size = self.text_size() * 0.72;
            line = line.push(self.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                Chip {
                    size,
                    font: SEMIBOLD,
                    height: size * 1.3333,
                    pad: 4.0,
                    radius: 4.0,
                    fill: CHIP,
                    ink: theme.text,
                },
                1.0,
            ));
        }
        if message.bot {
            line = line.push(self.bot_tag(1.0));
        }
        let time = Piece::new(vec![span(self.sent_time(index, message))], index, 1)
            .item(Some(self.row_item(index)))
            .size(self.text_size() - 4.0, 16.0)
            .color(theme.muted)
            .selection(self.selected(index, 1), self.selecting, theme.selection);
        line.push(time).height(AUTHOR_LINE).into()
    }

    /// A message's pictures laid out as its gallery ([`Gallery`]): each in
    /// its cell with round corners, a spoiler covered until clicked.
    pub fn gallery<'a>(
        &'a self,
        index: usize,
        message: &'a ChatMessage,
        gallery: &Gallery,
    ) -> Element<'a, Message> {
        let mut cells = stack![
            Space::new()
                .width(gallery.size.width)
                .height(gallery.size.height)
        ];
        for &(at, cell) in &gallery.cells {
            let attachment = &message.attachments[at];
            let covered =
                attachment.spoiler && !self.revealed.contains(&reveal_key(&message.item, at));
            let shot = self.shot(
                Shot::new(attachment.preview.as_ref(), cell.size())
                    .radius(PICTURE_RADIUS)
                    .covered(covered),
            );
            let shot: Element<'_, Message> = if covered {
                mouse_area(shot)
                    .on_press(Message::Reveal(self.row_item(index), at))
                    .interaction(iced::mouse::Interaction::Pointer)
                    .into()
            } else if attachment.video
                && !media::loading(attachment)
                && !self.client.state().preview
            {
                self.media_press(
                    message,
                    at,
                    true,
                    self.clip_cell(message, at, cell.size(), shot),
                )
            } else if attachment.video {
                stack![shot, container(self.play_badge()).center(Length::Fill)].into()
            } else if attachment.image {
                let opened = mouse_area(shot)
                    .on_press(Message::Image(crate::image_viewer::Event::Open(
                        message.item.clone(),
                        at,
                    )))
                    .interaction(iced::mouse::Interaction::Pointer);
                self.media_press(message, at, false, opened)
            } else {
                shot
            };
            cells = cells.push(placed(shot, cell.x, cell.y));
        }
        cells.into()
    }

    /// A message's picture or video that a right click opens the
    /// message's menu on, with what can be done with it.
    fn media_press<'a>(
        &'a self,
        message: &ChatMessage,
        at: usize,
        video: bool,
        shown: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        if message.deleted || message.delivery != chat::Delivery::default() {
            return shown.into();
        }
        let item = message.item.clone();
        crate::widgets::RightPress::new(shown, move |_| {
            Message::Image(crate::image_viewer::Event::Menu(item.clone(), at, video))
        })
        .into()
    }

    /// The round play mark over a video's poster, as the Slint client's
    /// `VideoCell` before it plays.
    pub fn play_badge(&self) -> Element<'_, Message> {
        container(self.glyph(Icon::PlayMark, 20, Color::WHITE))
            .center(PLAY_BADGE)
            .style(|_| rounded(PLAY_GROUND, PLAY_BADGE / 2.0))
            .into()
    }

    /// A message's Discord stickers, each on its own at Discord's size,
    /// moving ones playing; none when it has none.
    fn stickers<'a>(&'a self, message: &'a ChatMessage) -> Option<Element<'a, Message>> {
        let mut stickers = row![].spacing(8);
        let mut any = false;
        for (index, attachment) in message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| attachment.placement == Placement::Sticker)
        {
            any = true;
            let size = Size::new(STICKER, STICKER);
            let shown = match attachment.preview.as_ref() {
                Some(picture) => self.shot(Shot::new(Some(picture), size).fit(Fit::Contain).bare()),
                None => self.shot(Shot::new(None, size).radius(12.0)),
            };
            // A click opens its card under it.
            let open = Message::Info(crate::info_cards::Event::Sticker(
                message.item.clone(),
                index,
            ));
            let pressable = iced::widget::mouse_area(shown)
                .on_press(open)
                .interaction(iced::mouse::Interaction::Pointer);
            stickers =
                stickers.push(crate::widgets::Spot::new(pressable, &self.link_spot).by_press());
        }
        any.then(|| stickers.into())
    }

    /// A file's card, as the Slint client's `FileCard`: its icon in the
    /// accent, its name and size (and why a picture shows no preview), and
    /// Save, which asks where.
    pub fn file_card<'a>(
        &'a self,
        item: &str,
        index: usize,
        attachment: &'a Attachment,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let size = attachment.size.map(chat::file_size);
        let detail = match (attachment.image, attachment.preview_failed, size) {
            (true, true, Some(size)) => format!("{size} · Preview unavailable"),
            (true, true, None) => "Preview unavailable".to_owned(),
            (_, _, Some(size)) => size,
            _ => String::new(),
        };
        let mut words = column![
            text(attachment.name.as_str())
                .font(MEDIUM)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ];
        if !detail.is_empty() {
            words = words.push(text(detail).size(12).color(theme.muted));
        }
        let card = row![
            self.glyph(Icon::File, FILE_ICON, theme.accent),
            container(words).width(Length::Fill).clip(true),
            self.icon_button(
                Icon::Download,
                "Save",
                32.0,
                20,
                Message::Image(crate::image_viewer::Event::Save(item.to_owned(), index)),
                false,
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let (ground, edge) = (theme.sidebar, theme.edge);
        container(card)
            .padding(Padding {
                top: 12.0,
                bottom: 12.0,
                left: 12.0,
                right: 8.0,
            })
            .max_width(FILE_CARD_WIDTH)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..rounded(ground, 8)
            })
            .into()
    }

    /// A link preview or bot card, as the Slint client's `EmbedView`:
    /// behind a bar of its colour, its site, author, title (a link to its
    /// page), description, fields (inline ones sharing a row), its large
    /// picture and its footer.
    fn embed<'a>(
        &'a self,
        index: usize,
        message: &'a ChatMessage,
        place: usize,
        embed: &'a chat::Embed,
        laid: &'a EmbedText,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let small = |size: f32, ink: Color| Look {
            size,
            line_height: (size * 1.3).round(),
            ink,
        };
        // Its text isn't selected with the message's.
        let first = EMBED_PARTS
            + crate::select::PartNo::try_from(place * 16)
                .unwrap_or(0)
                .min(EMBED_PARTS - 16);
        let mut card = column![].spacing(8).width(Length::Fill);
        if !embed.provider.is_empty() {
            card = card.push(text(embed.provider.as_str()).size(12).color(theme.muted));
        }
        if !embed.author.is_empty() {
            card = card.push(
                text(embed.author.as_str())
                    .size(14)
                    .font(MEDIUM)
                    .color(theme.text),
            );
        }
        if !embed.title.is_empty() {
            let title = text(embed.title.as_str()).size(16).font(SEMIBOLD);
            card = card.push(if embed.url.is_empty() {
                Element::from(title.color(theme.text))
            } else {
                button(title.color(theme.link))
                    .padding(0)
                    .on_press(Message::Link(embed.url.clone(), Point::ORIGIN))
                    .style(|_, _| button::Style::default())
                    .into()
            });
        }
        if !laid.description.parts.is_empty() && !embed.description.is_empty() {
            card = card.push(self.body_view(
                &laid.description,
                index,
                first,
                small(14.0, theme.secondary),
            ));
        }
        // Inline fields share a row, up to three; others have one each.
        let mut fields = column![].spacing(8);
        let mut shared = row![].spacing(8);
        let mut in_row = 0;
        for ((name, value), field) in laid.fields.iter().zip(&embed.fields) {
            let cell = column![
                self.body_view(name, index, first, small(14.0, theme.text)),
                self.body_view(value, index, first, small(14.0, theme.secondary)),
            ]
            .spacing(2)
            .width(Length::Fill);
            if !field.inline || in_row == 3 {
                if in_row > 0 {
                    fields = fields.push(std::mem::replace(&mut shared, row![].spacing(8)));
                    in_row = 0;
                }
                if !field.inline {
                    fields = fields.push(cell);
                    continue;
                }
            }
            shared = shared.push(cell);
            in_row += 1;
        }
        if in_row > 0 {
            fields = fields.push(shared);
        }
        if !embed.fields.is_empty() {
            card = card.push(fields);
        }
        let thumbnail = message
            .attachments
            .iter()
            .position(|attachment| attachment.placement == Placement::Thumbnail(place));
        // Its padding, and a thumbnail beside the text, take from the room.
        let room = (self.content_width_of(index).min(EMBED_WIDTH)
            - EMBED_PADDING
            - if thumbnail.is_some() {
                THUMBNAIL + 16.0
            } else {
                0.0
            })
        .max(1.0);
        let gallery = Gallery::of(&message.attachments, Placement::Embed(place), room);
        if !gallery.is_empty() {
            card = card.push(self.gallery(index, message, &gallery));
        }
        if !embed.footer.is_empty() {
            card = card.push(text(embed.footer.as_str()).size(12).color(theme.muted));
        }
        let mut beside = row![card].spacing(16);
        if let Some(at) = thumbnail {
            let attachment = &message.attachments[at];
            let size = media::coming_size(attachment);
            let scale = (THUMBNAIL / size.width.max(size.height)).min(1.0);
            let size = Size::new((size.width * scale).round(), (size.height * scale).round());
            beside = beside.push(
                self.shot(
                    Shot::new(attachment.preview.as_ref(), size)
                        .fit(Fit::Contain)
                        .radius(PICTURE_RADIUS)
                        .covered(
                            attachment.spoiler
                                && !self.revealed.contains(&reveal_key(&message.item, at)),
                        ),
                ),
            );
        }
        let bar_ink = embed.color.map_or(theme.edge, hex);
        let ground = theme.sidebar;
        let shown = row![
            controls::bar(bar_ink, EMBED_BAR, Length::Fill),
            container(beside).padding(Padding {
                top: 12.0,
                bottom: 12.0,
                left: 12.0,
                right: 16.0,
            })
        ]
        .height(Length::Shrink);
        container(
            container(shown)
                .max_width(EMBED_WIDTH)
                .clip(true)
                .style(move |_| rounded(ground, 4)),
        )
        .padding(Padding::ZERO.top(4))
        .into()
    }

    /// A message's reaction chips. Each has its tooltip from the start, so
    /// it knows the pointer is on the chip when the names come: empty until
    /// then.
    fn reactions<'a>(&'a self, message: &'a ChatMessage) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut chips = row![].spacing(4);
        for reaction in &message.reactions {
            // As the Slint client's chip: the accent's wash and edge when
            // this account reacted.
            let (ground, edge, count) = if reaction.mine {
                (
                    theme.accent_soft,
                    Color {
                        a: 0.7,
                        ..theme.accent
                    },
                    theme.accent,
                )
            } else {
                (theme.raised, Color::TRANSPARENT, theme.secondary)
            };
            // A custom emoji's picture, as the message's own emoji have it.
            let picture = message
                .emoji
                .iter()
                .find(|emoji| emoji.text == reaction.key)
                .and_then(|emoji| {
                    self.picture(&emoji.picture, REACTION_EMOJI, REACTION_EMOJI, false)
                });
            let face: Element<'_, Message> = match picture {
                Some(picture) => Pic::square(picture, REACTION_EMOJI as f32).into(),
                None => text(reaction.key.as_str()).color(theme.text).into(),
            };
            let chip = container(
                row![
                    face,
                    text(reaction.count.to_string()).font(BOLD).color(count)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([2, 8])
            .style(move |_| container::Style {
                border: iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..rounded(ground, 8)
            });
            let chip = mouse_area(chip)
                .on_enter(Message::PointReaction(
                    message.item.clone(),
                    reaction.key.clone(),
                    true,
                ))
                .on_exit(Message::PointReaction(
                    message.item.clone(),
                    reaction.key.clone(),
                    false,
                ));
            let words = self
                .reaction_tip(&message.item, &reaction.key)
                .unwrap_or_else(|| Space::new().width(0).height(0).into());
            chips = chips.push(
                tooltip(chip, words, tooltip::Position::Top)
                    .gap(6)
                    .delay(Duration::from_millis(500))
                    .style(|_| container::Style::default()),
            );
        }
        chips.into()
    }

    /// A reaction's tooltip, once the core has the names, for the chip the
    /// pointer stays on.
    fn reaction_tip(&self, item: &str, key: &str) -> Option<Element<'_, Message>> {
        let (tip_of, reaction) = self.client.state().reaction_tip()?;
        if tip_of.item != item || tip_of.key != key {
            return None;
        }
        let theme = &self.theme;
        let names = tip_of.names.as_deref().unwrap_or_default();
        let words = reaction_words(&emoji_shortcode(key), names, reaction.count);
        let picture = emojis::get(key)
            .map(emojis::Emoji::as_str)
            .and_then(|key| self.emoji_picture(key, 32));
        let face: Element<'_, Message> = match picture {
            Some(picture) => Pic::square(picture, 32.0).into(),
            None => text(key.to_owned()).size(26).into(),
        };
        Some(
            container(
                row![
                    face,
                    text(words)
                        .size(14)
                        .font(MEDIUM)
                        .color(theme.text)
                        .width(Length::Shrink)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(16)
            .max_width(288)
            .style(controls::tip_style(theme))
            .into(),
        )
    }

    /// A body that folds, as the Slint client's when long messages fold:
    /// once it's a few lines past twelve, so Show more never uncovers only
    /// a line or two, it's cut to twelve with Show more under it.
    fn folding<'a>(&'a self, index: usize, body: Element<'a, Message>) -> Element<'a, Message> {
        let item = self.row_item(index);
        let whole = self.revealed.contains(&fold_key(&item));
        let theme = &self.theme;
        let more = button(
            text(if whole { "Show less" } else { "Show more" })
                .size(self.text_size() * 0.875)
                .font(SEMIBOLD),
        )
        .padding(0)
        .on_press(Message::Unfold(item))
        .style(controls::ink_style(theme.link, theme.text));
        Fold::new(
            body,
            more,
            !whole,
            (LINE_HEIGHT * 15.0, LINE_HEIGHT * 12.0, 3.0),
        )
        .into()
    }

    /// Where the picker and the menu of row `index`'s message open, from
    /// its hover bar or the keys: under the bar's right end.
    pub fn bar_menu_at(&self, index: usize) -> Point {
        let (lane, _) = self.lane_place(index);
        Point::new(
            lane.left + lane.width - 16.0 - 200.0,
            self.row_body_top(index) + 24.0,
        )
    }

    /// A message's hover bar, as the Slint client's: the three reactions
    /// this device uses most and Add reaction, Edit on one's own message or
    /// Reply on someone else's, Forward, and More, which opens its menu.
    /// Holding Shift swaps the reactions for Pin and, on one's own message,
    /// Reply, and More for a Delete that doesn't ask, as Discord's client
    /// does; not while the message's menu or picker is open.
    pub fn hover_bar(&self, index: usize) -> Option<Element<'_, Message>> {
        let message = self.row_message(index)?;
        if message.deleted || message.delivery != chat::Delivery::default() {
            return None;
        }
        let theme = &self.theme;
        let at = self.bar_menu_at(index);
        // The thread beside the channel is narrower: two quick reactions.
        let quick_reactions = if index >= crate::timeline::ASIDE {
            2
        } else {
            3
        };
        let item = &message.item;
        let shared = self.row_item(index);
        let writes = if index >= crate::timeline::ASIDE {
            self.aside_writes()
        } else {
            self.composer_look().enabled
        };
        let open_here = self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.item.as_ref() == Some(item))
            || self
                .picker
                .as_ref()
                .is_some_and(|picker| matches!(&picker.target, crate::picker::Target::React(on, _) if on == item));
        let quick = self.modifiers.shift() && !open_here;
        let deletes = quick && message.can_delete && writes;
        let act = |act: MenuAct| Message::Act(act, item.clone(), at);
        let button = |icon: Icon, label: &'static str, message: Message| {
            self.icon_button(icon, label, BAR_BUTTON, 20, message, false)
        };
        let mut buttons = row![].align_y(Alignment::Center);
        if quick && message.can_pin {
            buttons = buttons.push(if message.pinned {
                button(Icon::PinOff, "Unpin", act(MenuAct::Pin))
            } else {
                button(Icon::Pin, "Pin", act(MenuAct::Pin))
            });
        }
        if quick && message.own && message.can_edit && message.can_reply && writes {
            buttons = buttons.push(button(Icon::Reply, "Reply", act(MenuAct::Reply)));
        }
        if message.can_react {
            if !quick {
                for key in self.client.app.frequent_reactions(quick_reactions) {
                    buttons = buttons.push(self.emoji_button(
                        &key,
                        20,
                        BAR_BUTTON,
                        (None, theme.wash(0.1)),
                        Message::React(item.clone(), key.clone()),
                    ));
                }
                buttons = buttons.push(
                    container(controls::bar(theme.line, 1, 20))
                        .width(9)
                        .center_x(9),
                );
            }
            buttons = buttons.push(button(
                Icon::SmilePlus,
                "Add reaction",
                act(MenuAct::AddReaction),
            ));
        }
        if message.own && message.can_edit && writes {
            buttons = buttons.push(button(Icon::Pencil, "Edit", act(MenuAct::Edit)));
        } else if message.can_reply && writes {
            buttons = buttons.push(button(Icon::Reply, "Reply", act(MenuAct::Reply)));
        }
        // Beside Reply, as in Discord, whether or not Shift is held.
        if message.can_forward && self.forward_allowed() {
            buttons = buttons.push(button(Icon::Forward, "Forward", act(MenuAct::Forward)));
        }
        buttons = buttons.push(if deletes {
            self.icon_face_button(
                self.glyph(Icon::Trash, 20, theme.danger),
                "Delete",
                BAR_BUTTON,
                act(MenuAct::DeleteNow),
                false,
            )
        } else {
            button(Icon::Ellipsis, "More", Message::RowMenu(shared.clone(), at))
        });
        let (ground, edge) = (theme.raised, theme.edge);
        let bar = mouse_area(
            container(buttons)
                .padding(3)
                .style(move |_| container::Style {
                    background: Some(ground.into()),
                    border: iced::Border {
                        color: edge,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba8(0, 0, 0, 0x59 as f32 / 255.0),
                        offset: iced::Vector::new(0.0, 2.0),
                        blur_radius: 10.0,
                    },
                    ..container::Style::default()
                }),
        )
        .on_enter(Message::PointRow(shared, true));
        // The pointer on the bar is on its message, though the bar floats
        // over the row above: the bar keeps its moves from that row, and
        // from its own row's saying the pointer left. Not while a drag
        // selects, whose moves the text under it follows.
        Some(if self.selecting {
            bar.into()
        } else {
            crate::widgets::Holds::new(bar).into()
        })
    }

    /// The thread a message started, as the Slint client's `ThreadChip`:
    /// its name, how many messages it has and whether it's archived, and a
    /// click that opens it beside the channel.
    fn thread_chip<'a>(&'a self, thread: &'a chat::ThreadLink) -> Element<'a, Message> {
        let theme = &self.theme;
        let count = match thread.messages {
            None => None,
            Some(0) => Some("No messages yet".to_owned()),
            Some(1) => Some("1 message".to_owned()),
            Some(count) => Some(format!("{count} messages")),
        };
        let detail = match (count, thread.archived) {
            (Some(count), true) => format!("{count}, archived"),
            (Some(count), false) => count,
            (None, true) => "Archived".into(),
            (None, false) => String::new(),
        };
        // The name as long as it is, the count after it, the chevron at
        // the end.
        let mut words = row![
            crate::elide::elide(thread.name.as_str())
                .size(self.text_size())
                .font(BOLD)
                .color(theme.text),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if !detail.is_empty() {
            words = words.push(text(detail).size(14).color(theme.link));
        }
        let face = row![
            self.glyph(Icon::Thread, 18, theme.muted),
            container(words).width(Length::Fill),
            self.glyph(Icon::ChevronRight, 16, theme.muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let (rest, lit, edge) = (theme.sidebar, theme.hover, theme.edge);
        let chip = button(face)
            .padding([8, 12])
            .on_press(Message::Thread(crate::thread::Event::Open(
                thread.room.clone(),
            )))
            .style(move |_, status| button::Style {
                background: Some(
                    if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        lit
                    } else {
                        rest
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..button::Style::default()
            });
        container(container(chip).max_width(EMBED_WIDTH))
            .padding(Padding::ZERO.top(4))
            .into()
    }

    /// How a message of one's own is going, under it, as the Slint
    /// client's: "Sending..." with Cancel while its files go, "Not sent
    /// yet" while the connection is down, or "Not sent" with Retry.
    fn delivery_line(&self, message: &ChatMessage) -> Option<Element<'_, Message>> {
        use crate::composer::Event;
        let theme = &self.theme;
        let (words, retry, cancel) = match message.delivery {
            chat::Delivery::Sent => return None,
            chat::Delivery::Sending if message.attachments.is_empty() => return None,
            chat::Delivery::Sending => ("Sending...", false, true),
            chat::Delivery::Waiting => (
                "Not sent yet. Yapfast sends it when the connection returns.",
                false,
                true,
            ),
            // An app's command, whose row says why.
            chat::Delivery::Failed if message.item.starts_with("c:") => ("Not sent.", true, true),
            chat::Delivery::Failed => ("Not sent. The server refused this message.", true, true),
        };
        let failed = message.delivery == chat::Delivery::Failed;
        let mut line =
            row![
                text(words)
                    .size(13)
                    .color(if failed { theme.danger } else { theme.muted })
            ]
            .spacing(4)
            .align_y(Alignment::Center);
        let item = &message.item;
        if retry {
            line = line.push(
                controls::action(
                    theme,
                    "Retry",
                    controls::Kind::Ghost,
                    Some(Message::Write(Event::Retry(item.clone()))),
                )
                .small(),
            );
        }
        if cancel {
            line = line.push(
                controls::action(
                    theme,
                    "Cancel",
                    controls::Kind::Ghost,
                    Some(Message::Write(Event::CancelSend(item.clone()))),
                )
                .small(),
            );
        }
        Some(line.into())
    }

    /// As Discord's client marks an answer only this account sees.
    fn ephemeral_line(&self, message: &ChatMessage) -> Element<'_, Message> {
        let theme = &self.theme;
        let preview = self.client.state().preview;
        let dismiss = button(text("Dismiss message").size(12))
            .padding(0)
            .on_press_maybe(
                (!preview)
                    .then(|| Message::Write(crate::composer::Event::Dismiss(message.item.clone()))),
            )
            .style(controls::ink_style(
                if preview { theme.muted } else { theme.link },
                theme.text,
            ));
        row![
            self.glyph(Icon::Eye, 16, theme.muted),
            text("Only you can see this ·").size(12).color(theme.muted),
            dismiss,
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    /// The room row `row`'s content has across: its timeline's width but
    /// the gutter and the room kept for its actions.
    pub fn content_width_of(&self, row: usize) -> f32 {
        let (lane, _) = self.lane_place(row);
        (lane.width - GUTTER - END_PAD).max(0.0)
    }

    /// A message's avatar or author was clicked: the card opens beside the
    /// avatar, as the Slint client's `beside`.
    pub fn open_author(&mut self, index: usize) -> iced::Task<Message> {
        let Some(message) = self.row_message(index) else {
            return iced::Task::none();
        };
        let (lane, _) = self.lane_place(index);
        // The avatar's top: moved down under a reply's line.
        let header = message.reply.is_some() || message.used.is_some();
        let y = self.row_body_top(index) + CONTENT_PAD + if header { 24.0 } else { 2.0 };
        let x = lane.left + (GUTTER - AVATAR) / 2.0;
        let width = crate::profile::WIDTH;
        let beside = if x + AVATAR + 8.0 + width <= self.window.width - 8.0 {
            x + AVATAR + 8.0
        } else {
            (x - width - 8.0).max(8.0)
        };
        let user = message.sender.clone();
        self.open_card(user, Point::new(beside, y - 8.0))
    }
}

/// A message's files shown as cards in `placement`, with their places
/// among its attachments.
pub fn files(
    message: &ChatMessage,
    placement: Placement,
) -> impl Iterator<Item = (usize, &Attachment)> {
    message
        .attachments
        .iter()
        .enumerate()
        .filter(move |(_, attachment)| {
            attachment.placement == placement && !media::pictured(attachment)
        })
}

/// What `revealed` holds for a blocked person's message shown.
pub fn blocked_key(item: &str) -> String {
    format!("{item}/blocked")
}

/// What `revealed` holds for a long message shown whole.
pub fn fold_key(item: &str) -> String {
    format!("{item}/more")
}

/// What `revealed` holds for a message's attachment shown past its
/// spoiler's cover, as the Slint client keeps it.
pub fn reveal_key(item: &str, index: usize) -> String {
    format!("{item}/a{index}")
}

/// What a reaction's tooltip says, as the Slint client's `reaction_tip`
/// words it after Discord's client.
pub fn reaction_words(emoji: &str, names: &[String], count: usize) -> String {
    let others = count.saturating_sub(names.len());
    let more = match others {
        1 => "1 other".to_owned(),
        others => format!("{others} others"),
    };
    let people = match (names, others) {
        ([one], 0) => one.clone(),
        ([one], _) => format!("{one} and {more}"),
        ([one, two], 0) => format!("{one} and {two}"),
        ([one, two], _) => format!("{one}, {two}, and {more}"),
        ([one, two, three], 0) => format!("{one}, {two}, and {three}"),
        ([one, two, three, ..], _) => format!("{one}, {two}, {three}, and {more}"),
        _ => format!("{count} people"),
    };
    format!("{emoji} reacted by {people}")
}

/// A reaction's emoji as Discord's client names it: `:name:` for a custom
/// one, the first shortcode that's a word for a Unicode one.
pub fn emoji_shortcode(key: &str) -> String {
    if key.starts_with(':') && key.ends_with(':') && key.len() > 2 {
        return key.to_owned();
    }
    let codes = || {
        emojis::get(key)
            .into_iter()
            .flat_map(emojis::Emoji::shortcodes)
    };
    codes()
        .find(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
        .or_else(|| codes().next())
        .map_or_else(|| key.to_owned(), |code| format!(":{code}:"))
}

/// What a message whose only content is a card says while link previews
/// are off, as the Slint client says it.
fn hidden_card(message: &ChatMessage) -> Option<String> {
    let embed = message.embeds.first()?;
    let pictures = message.attachments.iter().any(|attachment| {
        matches!(
            attachment.placement,
            Placement::Message | Placement::Sticker
        )
    });
    if !message.body.trim().is_empty() || pictures {
        return None;
    }
    Some(if embed.title.is_empty() {
        "A card, hidden while link previews are off.".to_owned()
    } else {
        format!(
            "A card, \"{}\", hidden while link previews are off.",
            embed.title
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reactions_tooltip_names_up_to_three_then_counts_the_others() {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice"]), 1),
            ":tada: reacted by Alice"
        );
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice", "Bob", "Carol"]), 4),
            ":tada: reacted by Alice, Bob, Carol, and 1 other"
        );
        assert_eq!(emoji_shortcode("🎉"), ":tada:");
    }
}
