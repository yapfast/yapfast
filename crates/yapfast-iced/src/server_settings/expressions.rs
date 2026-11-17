//! Emoji and Stickers: the server's emoji in two tables, each renamed in
//! place or deleted at once, and Upload Emoji's dialog with the picture at
//! the sizes chat shows it; the stickers on Discord's four boost cards,
//! each edited or deleted, and the sticker form. What each holds, and what
//! this account may change ([`ExpressionRules`]), is the core's.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Color, Element, Length, Padding, Size, Task,
    widget::{Space, button, column, container, mouse_area, row, text, text_input},
};
use yapfast_core::{
    application::{
        EmojiInput, EmojiPage, ExpressionRules, LEVEL_BOOSTS, ServerSettings, StickerCard,
        StickerInput, StickersPage,
    },
    chat::{ServerEmoji, ServerPicture, ServerSticker},
    i18n::tr,
};

use super::View;
use super::{Event as Settings, Screen, message, moderation::face};
use crate::{
    app::Message,
    controls::{Kind, action, lights},
    ctx::{Cx, Request},
    dialog,
    fonts::{BOLD, SEMIBOLD},
    form,
    media::{Fit, Shot},
    pictures::Icon,
    style::{rounded, scrollbar},
    sync::Scroll,
    widgets::{Hover, Rows},
};

/// Emoji's rows: a table's heading with its columns' names, and an emoji.
const HEADING_ROW: f32 = 72.0;
const EMOJI_ROW: f32 = 52.0;
/// The image column and the emoji in it, an uploader's avatar, and the
/// delete button with its glyph.
const IMAGE_COLUMN: f32 = 48.0;
const EMOJI: f32 = 32.0;
const UPLOADER: f32 = 24.0;
const BUTTON: f32 = 32.0;
const BUTTON_GLYPH: u32 = 18;
/// A sticker's tile and the picture on it, the gap between tiles, and the
/// tile's buttons with their glyphs.
const TILE_WIDTH: f32 = 140.0;
const TILE_HEIGHT: f32 = 160.0;
const TILE_GAP: f32 = 12.0;
const STICKER: f32 = 80.0;
const NAME_BAND: f32 = 22.0;
const UPLOADER_BAND: f32 = 24.0;
const TILE_BUTTON: f32 = 28.0;
const TILE_GLYPH: u32 = 16;
const ADD_GLYPH: u32 = 32;
/// The dialogs' widths; the sizes the new emoji shows at, and the grounds
/// the emoji and the sticker show on, with the sticker's size.
const EMOJI_DIALOG: f32 = 440.0;
const STICKER_DIALOG: f32 = 520.0;
const EMOJI_SIZES: [f32; 3] = [128.0, 48.0, 22.0];
const EMOJI_STAGE: f32 = 144.0;
const STICKER_STAGE: f32 = 180.0;
const PREVIEW: f32 = 160.0;
/// Upload Emoji's name field, which takes the keys when the dialog opens.
pub(super) const EMOJI_NAME: &str = "emoji-upload-name";

/// Emoji's own while it shows: an emoji's name being written in its row,
/// and the list's viewport.
#[derive(Default)]
pub struct Local {
    renaming: Option<(String, String)>,
    scroll: Option<Scroll>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// An emoji's name typed in its row, by its id.
    Rename(String, String),
    /// Enter in an emoji's row, or a click beside it: the name written goes
    /// to Discord, as Discord's field sends it when it loses the keys.
    Renamed,
    Scrolled(Scroll),
}

fn local_event(event: Event) -> Message {
    message(Settings::ExpressionLocal(event))
}

fn emoji(input: EmojiInput) -> Message {
    message(Settings::Emoji(input))
}

fn sticker(input: StickerInput) -> Message {
    message(Settings::Sticker(input))
}

/// A row of Emoji's list.
enum Line<'a> {
    Heading { title: &'static str, free: usize },
    Emoji(&'a ServerEmoji),
}

impl Line<'_> {
    fn height(&self) -> f32 {
        match self {
            Line::Heading { .. } => HEADING_ROW,
            Line::Emoji(_) => EMOJI_ROW,
        }
    }
}

/// Emoji's list: each table's heading, then its emoji newest first; a
/// table without emoji isn't shown.
fn lines(page: &EmojiPage) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for (animated, title) in [(false, tr("Emoji")), (true, tr("Animated Emoji"))] {
        let start = lines.len();
        lines.push(Line::Heading {
            title,
            free: page.free(animated),
        });
        lines.extend(
            page.emojis
                .iter()
                .rev()
                .filter(|emoji| emoji.animated == animated)
                .map(Line::Emoji),
        );
        if lines.len() == start + 1 {
            lines.pop();
        }
    }
    lines
}

/// Where each line starts, and the list's height.
fn tops(lines: &[Line<'_>]) -> (Vec<f32>, f32) {
    let mut top = 0.0;
    let tops = lines
        .iter()
        .map(|line| {
            let at = top;
            top += line.height();
            at
        })
        .collect();
    (tops, top)
}

/// The lines in view, and a viewport's height beyond either side.
fn in_view(tops: &[f32], scroll: Option<Scroll>, height: f32) -> (usize, usize) {
    let scroll = scroll.unwrap_or(Scroll::new(height));
    let from = scroll.offset - scroll.height;
    let to = scroll.offset + scroll.height * 2.0;
    let first = tops.partition_point(|top| *top <= from).saturating_sub(1);
    let last = tops.partition_point(|top| *top < to);
    (first.min(last), last)
}

/// The pictures of the emoji in view, and their uploaders'.
pub fn emoji_pictures(page: &EmojiPage, local: &Local, height: f32) -> Vec<String> {
    let lines = lines(page);
    let (tops, _) = tops(&lines);
    let (first, last) = in_view(&tops, local.scroll, height);
    let mut urls = Vec::new();
    for line in &lines[first..last] {
        if let Line::Emoji(listed) = line {
            urls.push(listed.picture.clone());
            urls.extend(listed.user.as_ref().map(|user| user.avatar.clone()));
        }
    }
    urls.retain(|url| !url.is_empty());
    urls
}

/// The stickers' pictures: at most a few dozen, on their cards.
pub fn sticker_pictures(page: &StickersPage) -> Vec<String> {
    page.stickers
        .iter()
        .filter_map(|listed| listed.picture.clone())
        .collect()
}

/// The field that takes the keys while Upload Emoji's dialog shows.
pub(super) fn dialog_field(page: &EmojiPage) -> Option<&'static str> {
    page.upload.as_ref().map(|_| EMOJI_NAME)
}

/// How many slots are left, in Discord's words.
fn slots(free: usize) -> String {
    match free {
        0 => tr("no slots available").to_owned(),
        1 => "1 slot available".to_owned(),
        free => format!("{free} slots available"),
    }
}

/// An error under a dialog's fields.
fn error_text<'a>(theme: &crate::theme::Theme, error: &'a str) -> Element<'a, Message> {
    form::error(theme, error).into()
}

/// A dialog's foot: Cancel, then what it does in `kind`.
pub(super) fn answers<'a>(
    theme: &crate::theme::Theme,
    cancel: Option<Message>,
    label: &'a str,
    kind: Kind,
    confirm: Option<Message>,
) -> Element<'a, Message> {
    row![
        Space::new().width(Length::Fill),
        action(theme, tr("Cancel"), Kind::Ghost, cancel),
        action(theme, label, kind, confirm),
    ]
    .spacing(form::TIGHT)
    .into()
}

impl Screen {
    /// What Emoji's own controls did: a name written in a row goes to
    /// Discord on Enter, on a click beside it, or once another row's is
    /// written (and as the settings' other controls are used:
    /// [`Screen::rename_written`]).
    pub(super) fn expressions_local(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let local = &mut self.expressions;
        let renamed = match event {
            Event::Rename(id, name) => {
                let earlier = local.renaming.take().filter(|(renamed, _)| *renamed != id);
                local.renaming = Some((id, name));
                earlier
            }
            Event::Renamed => local.renaming.take(),
            Event::Scrolled(scroll) => {
                local.scroll = Some(scroll);
                cx.request(Request::Ask);
                return Task::none();
            }
        };
        match renamed {
            Some((id, name)) => self.update(Settings::Emoji(EmojiInput::Rename { id, name }), cx),
            None => Task::none(),
        }
    }

    /// A name being written in a row goes to Discord as the keys leave it
    /// for another of the settings' controls: another page, an emoji's
    /// own buttons, closing the settings. A rename on its way is the
    /// earlier row's, sent as the keys went to this one, whose name is
    /// still being written.
    pub(super) fn rename_written(&mut self, event: &Settings, cx: &mut Cx<'_>) {
        let leaves = match event {
            Settings::Emoji(EmojiInput::Rename { .. }) => false,
            Settings::Section(_)
            | Settings::Close
            | Settings::DeleteServer
            | Settings::Emoji(_) => true,
            _ => false,
        };
        if let Some((id, name)) = leaves.then(|| self.expressions.renaming.take()).flatten() {
            cx.dispatch(yapfast_core::application::Action::Emoji(
                EmojiInput::Rename { id, name },
            ));
        }
    }
}

impl<'a> View<'a> {
    /// Emoji: what may be uploaded, Upload Emoji while a slot is free, and
    /// the two tables, or Discord's words while there are none.
    pub(super) fn emoji_page(
        self,
        core: &'a ServerSettings,
        page: &'a EmojiPage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let rules = self.cx.app.expression_rules(&core.server);
        let mut parts = column![
            form::title(theme, tr("Emoji")),
            form::note(
                theme,
                format!(
                    "Add up to {} custom emojis that anyone can use in this server. Animated GIF emojis may be used by members with Discord Nitro. Emoji names must be at least 2 characters long and can only contain alphanumeric characters and underscores. Emoji must be under 256 KB in size.",
                    page.slots()
                ),
            ),
        ]
        .spacing(form::GAP)
        .height(Length::Fill);
        if (rules.manage || rules.create) && (page.free(false) > 0 || page.free(true) > 0) {
            parts = parts.push(action(
                theme,
                tr("Upload Emoji"),
                Kind::Primary,
                (core.reading.is_none() && page.upload.is_none())
                    .then(|| message(Settings::Pick(ServerPicture::Emoji))),
            ));
        }
        let lines = lines(page);
        if lines.is_empty() {
            return parts
                .push(
                    container(
                        column![
                            text(tr("No Emoji")).size(17).font(BOLD).color(theme.text),
                            text(tr("Get the party started by uploading an emoji"))
                                .size(15)
                                .color(theme.secondary),
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center),
                    )
                    .center(Length::Fill),
                )
                .into();
        }
        let local = &screen.expressions;
        let (tops, total) = tops(&lines);
        let (first, last) = in_view(&tops, local.scroll, self.cx.look.window.height);
        let mut rows = Rows::new(total);
        for (index, line) in lines.iter().enumerate().take(last).skip(first) {
            let (key, view) = match line {
                Line::Heading { title, free } => {
                    (crate::app::key(title), self.emoji_heading(title, *free))
                }
                Line::Emoji(listed) => (
                    crate::app::key(&listed.id),
                    self.emoji_row(listed, &rules, local),
                ),
            };
            rows.push(key, tops[index], line.height(), view);
        }
        let list = scroll_view(rows)
            .on_scroll(|viewport| local_event(Event::Scrolled(Scroll::of(viewport))))
            .style(scrollbar(theme))
            .height(Length::Fill);
        // A click beside a name being written sends it, as Discord's field
        // does when it loses the keys.
        mouse_area(parts.push(list))
            .on_press(local_event(Event::Renamed))
            .into()
    }

    /// A table's heading: its name with the slots left, then the columns'.
    fn emoji_heading(self, title: &'a str, free: usize) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ink = theme.secondary;
        let name = move |name: &'a str| text(name).size(12).font(BOLD).color(ink);
        column![
            row![
                text(title).size(16).font(BOLD).color(theme.text),
                text(format!("– {}", slots(free)))
                    .size(16)
                    .color(theme.secondary)
            ]
            .spacing(6),
            // The spare height goes between the title and the columns' names.
            Space::new().height(Length::Fill),
            row![
                container(name(tr("IMAGE"))).width(IMAGE_COLUMN),
                container(name(tr("NAME")))
                    .padding(Padding {
                        left: 8.0,
                        ..Padding::ZERO
                    })
                    .width(Length::Fill),
                container(name(tr("UPLOADED BY"))).width(Length::Fill),
                Space::new().width(BUTTON),
            ]
            .spacing(16),
        ]
        .spacing(8)
        .padding(Padding {
            top: 16.0,
            right: 8.0,
            bottom: 6.0,
            left: 8.0,
        })
        .height(HEADING_ROW)
        .into()
    }

    /// An emoji: its picture, its name (a field where this account may
    /// change it, which shows its ground under the pointer or while it has
    /// the keys), who uploaded it, and Delete under the pointer.
    fn emoji_row(
        self,
        listed: &'a ServerEmoji,
        rules: &ExpressionRules<'_>,
        local: &'a Local,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let uploader = listed.user.as_ref();
        let may_change = rules.may_change(uploader.map(|user| user.id.as_str()));
        let picture = self.cx.shot(
            Shot::new(
                self.cx.state.pictures.get(&listed.picture),
                Size::new(EMOJI, EMOJI),
            )
            .fit(Fit::Contain)
            .bare(),
        );
        let name: Element<'a, Message> = if may_change {
            let shown = match &local.renaming {
                Some((id, typed)) if *id == listed.id => typed.as_str(),
                _ => listed.name.as_str(),
            };
            let id = listed.id.clone();
            let field = crate::controls::field_style(theme, theme.sunken, true);
            text_input("", shown)
                .id(iced::widget::Id::from(format!("emoji-name-{}", listed.id)))
                .on_input(move |name| local_event(Event::Rename(id.clone(), name)))
                .on_submit(local_event(Event::Renamed))
                .size(14)
                .padding(8)
                .style(move |look, status| {
                    let lit = matches!(
                        status,
                        text_input::Status::Focused { .. } | text_input::Status::Hovered
                    );
                    let style = field(look, status);
                    if lit {
                        style
                    } else {
                        text_input::Style {
                            background: Color::TRANSPARENT.into(),
                            ..style
                        }
                    }
                })
                .into()
        } else {
            container(
                text(listed.name.as_str())
                    .size(14)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None),
            )
            .padding(Padding {
                left: 8.0,
                ..Padding::ZERO
            })
            .clip(true)
            .into()
        };
        let mut by = row![].spacing(8).align_y(Alignment::Center);
        if let Some(user) = uploader {
            by = by
                .push(face(self.cx, &user.avatar, &user.name, UPLOADER))
                .push(
                    text(user.name.as_str())
                        .size(14)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                );
        }
        let base = row![
            container(picture).width(IMAGE_COLUMN),
            container(name).width(Length::Fill),
            container(by).width(Length::Fill).clip(true),
            Space::new().width(BUTTON),
        ]
        .spacing(16)
        .padding([0, 8])
        .height(EMOJI_ROW)
        .align_y(Alignment::Center);
        let delete: Element<'a, Message> = if may_change {
            self.cx
                .look
                .icon_button(
                    Icon::X,
                    tr("Delete"),
                    Some(emoji(EmojiInput::Delete(listed.id.clone()))),
                )
                .size(BUTTON, BUTTON_GLYPH)
                .danger(None)
                .into()
        } else {
            Space::new().into()
        };
        let wash = theme.wash(0.06);
        let over = container(delete)
            .padding([0, 8])
            .align_right(Length::Fill)
            .center_y(EMOJI_ROW)
            .style(move |_| rounded(wash, 6));
        Hover::new(base, over).into()
    }

    /// Upload Emoji's dialog: the new emoji at the sizes chat shows it, its
    /// name (as Discord makes one of the file's), what went wrong, and
    /// Upload.
    pub(super) fn emoji_dialog(self, page: &'a EmojiPage) -> Option<Element<'a, Message>> {
        let upload = page.upload.as_ref()?;
        let theme = self.cx.look.theme;
        let saving = upload.saving;
        let mut sizes = row![].spacing(24).align_y(Alignment::Center);
        for side in EMOJI_SIZES {
            sizes = sizes.push(
                self.cx.shot(
                    Shot::new(
                        upload.picture.as_ref().map(|picture| &picture.preview),
                        Size::new(side, side),
                    )
                    .fit(Fit::Contain)
                    .bare(),
                ),
            );
        }
        let sunken = theme.sunken;
        let stage = container(sizes)
            .center_x(Length::Fill)
            .center_y(EMOJI_STAGE)
            .style(move |_| rounded(sunken, 8));
        let may_finish = upload.picture.is_some() && upload.name.chars().count() >= 2 && !saving;
        let mut field = form::entry(theme, EMOJI_NAME, "", &upload.name);
        if !saving {
            field = field
                .on_input(|name| emoji(EmojiInput::Name(name)))
                .on_submit(emoji(EmojiInput::Finish));
        }
        let mut body = column![
            text(tr("Upload Emoji"))
                .size(20)
                .font(BOLD)
                .color(theme.text),
            stage,
            column![form::heading(theme, tr("NAME")), field].spacing(6),
        ]
        .spacing(14)
        .padding(16);
        if let Some(error) = &upload.error {
            body = body.push(error_text(theme, error));
        }
        body = body.push(answers(
            theme,
            (!saving).then(|| emoji(EmojiInput::Cancel)),
            if saving {
                tr("Uploading...")
            } else {
                tr("Upload")
            },
            Kind::Primary,
            may_finish.then(|| emoji(EmojiInput::Finish)),
        ));
        let outside = if saving {
            Message::Nothing
        } else {
            emoji(EmojiInput::Cancel)
        };
        Some(dialog::panel(
            theme,
            body.into(),
            EMOJI_DIALOG,
            None,
            outside,
        ))
    }

    /// Stickers: what may be uploaded, then Discord's four cards.
    pub(super) fn stickers_page(
        self,
        core: &'a ServerSettings,
        page: &'a StickersPage,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let rules = self.cx.app.expression_rules(&core.server);
        let may_upload = (rules.manage || rules.create) && !page.full();
        let mut parts = vec![
            form::title(theme, tr("Stickers")),
            form::note(
                theme,
                tr(
                    "Stickers can be static (JPG, PNG) or animated (APNG, GIF). Stickers must be exactly 320 x 320 pixels and no larger than 512 KB. We will automatically resize static JPG, PNG and animated GIF stickers for you.",
                ),
            ),
        ];
        for card in page.cards() {
            parts.push(self.sticker_card(&card, &rules, may_upload));
        }
        form::page(parts)
    }

    /// A boost level's card: its name (with a lock while the server's level
    /// doesn't reach it), its slots or the boosts it takes, and its
    /// stickers, with Upload Sticker where the next one goes.
    fn sticker_card(
        self,
        card: &StickerCard<'a>,
        rules: &ExpressionRules<'_>,
        may_upload: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut head = row![].spacing(8).align_y(Alignment::Center);
        if !card.unlocked {
            head = head.push(self.cx.look.glyph(Icon::Lock, 16, theme.secondary));
        }
        let title = match card.level {
            0 => tr("Free Slots").to_owned(),
            level => format!("Level {level}"),
        };
        let status = if card.unlocked {
            let free = card.slots.saturating_sub(card.stickers.len());
            let word = if card.slots == 1 { "slot" } else { "slots" };
            format!("{free} of {} {word} available", card.slots)
        } else {
            format!("{} Boosts", LEVEL_BOOSTS[card.level])
        };
        head = head
            .push(
                text(title)
                    .size(16)
                    .font(BOLD)
                    .color(theme.text)
                    .width(Length::Fill),
            )
            .push(text(status).size(14).color(theme.secondary));
        let mut body = column![head].spacing(12);
        let adds = card.next && may_upload;
        if !card.stickers.is_empty() || adds {
            let mut tiles = row![].spacing(TILE_GAP);
            for listed in card.stickers {
                tiles = tiles.push(self.sticker_tile(listed, rules));
            }
            if adds {
                tiles = tiles.push(self.add_sticker());
            }
            body = body.push(tiles.wrap().vertical_spacing(TILE_GAP));
        }
        let (ground, line) = (theme.wash(0.03), theme.line);
        container(body)
            .padding(16)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: crate::style::edge(line, 8.0),
                ..Default::default()
            })
            .into()
    }

    /// A sticker: its picture, name and uploader, with Edit and Delete in
    /// its corner under the pointer where this account may change it.
    fn sticker_tile(
        self,
        listed: &'a ServerSticker,
        rules: &ExpressionRules<'_>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let picture = listed
            .picture
            .as_ref()
            .and_then(|url| self.cx.state.pictures.get(url));
        let uploader = listed
            .user
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or_default();
        // Each line centred in a band of its own height.
        let line = |words: &'a str, size: u32, font, ink, band: f32| {
            container(
                text(words)
                    .size(size)
                    .font(font)
                    .color(ink)
                    .wrapping(text::Wrapping::None),
            )
            .center_x(Length::Fill)
            .center_y(band)
            .clip(true)
        };
        let rest = theme.wash(0.03);
        let base = container(
            column![
                container(
                    self.cx.shot(
                        Shot::new(picture, Size::new(STICKER, STICKER))
                            .fit(Fit::Contain)
                            .bare()
                    )
                )
                .center_x(Length::Fill),
                line(
                    listed.form.name.as_str(),
                    14,
                    SEMIBOLD,
                    theme.text,
                    NAME_BAND
                ),
                line(
                    uploader,
                    12,
                    crate::fonts::REGULAR,
                    theme.secondary,
                    UPLOADER_BAND
                ),
            ]
            .spacing(6),
        )
        .padding(10)
        .width(TILE_WIDTH)
        .height(TILE_HEIGHT)
        .style(move |_| rounded(rest, 8));
        let mut buttons = row![].spacing(2);
        if rules.may_change(listed.user_id.as_deref()) {
            buttons = buttons
                .push(
                    self.cx
                        .look
                        .icon_button(
                            Icon::Pencil,
                            tr("Edit"),
                            Some(sticker(StickerInput::Edit(listed.id.clone()))),
                        )
                        .size(TILE_BUTTON, TILE_GLYPH)
                        .fill(theme.raised),
                )
                .push(
                    self.cx
                        .look
                        .icon_button(
                            Icon::X,
                            tr("Delete"),
                            Some(sticker(StickerInput::Delete(listed.id.clone()))),
                        )
                        .size(TILE_BUTTON, TILE_GLYPH)
                        .danger(Some(theme.raised)),
                );
        }
        // Under the pointer the tile lightens, as Discord's does.
        let lit = theme.wash(0.05);
        let over = container(buttons)
            .padding(4)
            .align_right(TILE_WIDTH)
            .height(TILE_HEIGHT)
            .style(move |_| rounded(lit, 8));
        Hover::new(base, over).into()
    }

    /// Upload Sticker's tile, where the next sticker goes.
    fn add_sticker(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (line, lit) = (theme.line, theme.secondary);
        button(
            container(
                column![
                    self.cx
                        .look
                        .glyph(Icon::CirclePlus, ADD_GLYPH, theme.secondary),
                    text(tr("Upload Sticker"))
                        .size(14)
                        .font(SEMIBOLD)
                        .color(theme.secondary),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .center(Length::Fill),
        )
        .padding(0)
        .width(TILE_WIDTH)
        .height(TILE_HEIGHT)
        .on_press(sticker(StickerInput::Upload))
        .style(move |_, status| button::Style {
            border: Border {
                color: if lights(status) { lit } else { line },
                width: 2.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    /// The sticker form: the preview, the file for a new one (an edit can't
    /// change it), the related emoji, name and description, what went
    /// wrong, and Upload or Update.
    pub(super) fn sticker_dialog(
        self,
        core: &'a ServerSettings,
        page: &'a StickersPage,
    ) -> Option<Element<'a, Message>> {
        let draft = page.draft.as_ref()?;
        let theme = self.cx.look.theme;
        let saving = draft.saving;
        let editing = draft.editing.is_some();
        let kept = draft
            .editing
            .as_ref()
            .and_then(|id| page.stickers.iter().find(|listed| &listed.id == id))
            .and_then(|listed| listed.picture.as_ref())
            .and_then(|url| self.cx.state.pictures.get(url));
        let preview: Element<'a, Message> =
            match kept.or(draft.file.as_ref().map(|file| &file.preview)) {
                Some(picture) => self.cx.shot(
                    Shot::new(Some(picture), Size::new(PREVIEW, PREVIEW))
                        .fit(Fit::Contain)
                        .bare(),
                ),
                None if editing => Space::new().into(),
                None => text(tr("Preview")).size(14).color(theme.muted).into(),
            };
        let sunken = theme.sunken;
        let mut body = column![
            text(if editing {
                tr("Edit Sticker")
            } else {
                tr("Upload Sticker")
            })
            .size(20)
            .font(BOLD)
            .color(theme.text),
            container(preview)
                .center_x(Length::Fill)
                .center_y(STICKER_STAGE)
                .style(move |_| rounded(sunken, 8)),
        ]
        .spacing(14)
        .padding(16);
        if !editing {
            let has_file = draft.file.is_some();
            body = body.push(
                column![
                    form::heading(theme, tr("FILE")),
                    row![
                        container(
                            text(if has_file {
                                draft.file_name.as_str()
                            } else {
                                tr("File should be APNG, JPG, PNG or GIF (512 KB max)")
                            })
                            .size(13)
                            .color(if has_file {
                                theme.text
                            } else {
                                theme.secondary
                            })
                            .wrapping(text::Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        action(
                            theme,
                            tr("Browse"),
                            Kind::Plain,
                            (!saving && core.reading.is_none())
                                .then(|| message(Settings::Pick(ServerPicture::Sticker))),
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            );
        }
        let field = |heading: &'a str,
                     id: &'static str,
                     placeholder: &'a str,
                     value: &'a str,
                     input: fn(String) -> StickerInput| {
            let mut field = form::entry(theme, id, placeholder, value);
            if !saving {
                field = field
                    .on_input(move |typed| sticker(input(typed)))
                    .on_submit(sticker(StickerInput::Save));
            }
            column![form::heading(theme, heading), field].spacing(6)
        };
        body = body
            .push(field(
                tr("RELATED EMOJI"),
                "sticker-tags",
                tr("Type or paste an emoji"),
                &draft.form.tags,
                StickerInput::Tags,
            ))
            .push(field(
                tr("STICKER NAME"),
                "sticker-name",
                tr("e.g.: cat hug"),
                &draft.form.name,
                StickerInput::Name,
            ))
            .push(
                field(
                    tr("DESCRIPTION"),
                    "sticker-description",
                    "",
                    &draft.form.description,
                    StickerInput::Description,
                )
                .push(
                    text(tr(
                        "Descriptions are optional but must be more than 1 character.",
                    ))
                    .size(12)
                    .color(theme.secondary),
                ),
            );
        if let Some(error) = &draft.error {
            body = body.push(error_text(theme, error));
        }
        body = body.push(answers(
            theme,
            (!saving).then(|| sticker(StickerInput::Cancel)),
            if saving {
                tr("Saving...")
            } else if editing {
                tr("Update")
            } else {
                tr("Upload")
            },
            Kind::Primary,
            draft.ready().then(|| sticker(StickerInput::Save)),
        ));
        let outside = if saving {
            Message::Nothing
        } else {
            sticker(StickerInput::Cancel)
        };
        Some(dialog::panel(
            theme,
            scroll_view(body).style(scrollbar(theme)).into(),
            STICKER_DIALOG,
            None,
            outside,
        ))
    }
}
