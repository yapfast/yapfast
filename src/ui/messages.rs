//! Messages as the timeline shows them: their rows, Markdown, embeds, and
//! how they group by day and author.

use super::*;
use yapfast_core::blocks::{Block, Kind};

/// Links in a notice: to its author, by sender, to the message it tells of,
/// by item, and to the open channel's pinned messages. Unlike a mention's,
/// they show without a pill.
pub(super) const AUTHOR_LINK: &str = "yapfast:author/";
pub(super) const JUMP_LINK: &str = "yapfast:jump/";
pub(super) const PINS_LINK: &str = "yapfast:pins";

/// A message continues the previous one's group when the same person sent it
/// within seven minutes, without replying to something else. A notice, such
/// as one that someone pinned a message, is a group of its own.
pub(super) fn continues(before: &ChatMessage, message: &ChatMessage) -> bool {
    if before.notice.is_some() || message.notice.is_some() {
        return false;
    }
    fn sender(message: &ChatMessage) -> &str {
        if message.sender.is_empty() {
            &message.author
        } else {
            &message.sender
        }
    }
    let close = before.timestamp == 0
        || message.timestamp == 0
        || message.timestamp.abs_diff(before.timestamp) <= 7 * 60;
    message.reply.is_none() && sender(before) == sender(message) && close
}

/// The local calendar date of a timestamp, such as "3 October 2026"; empty when unknown.
pub(super) fn day(timestamp: u64) -> String {
    yapfast_core::clock::day(timestamp)
}

/// How a timeline marks its rows beyond what their messages say.
pub(super) struct Marks<'a> {
    /// The index of the first message that came since the timeline was read.
    pub first_new: usize,
    /// The item the "New" divider shows above.
    pub first_unread: Option<&'a str>,
    pub revealed: &'a HashSet<String>,
    /// Every row is built again, as for a new layout.
    pub restyle: bool,
    /// Decorations show around authors' avatars, as the profile switches say.
    pub decorations: bool,
}

/// What a row shows of the messages around it, and of its author's picture.
struct Decoration<'a> {
    first: bool,
    day: Option<String>,
    picture_url: &'a str,
    reply_picture_url: &'a str,
    /// The decoration around the author's avatar, shown with it.
    decoration_url: &'a str,
    unread_start: bool,
    highlighted: bool,
}

impl Decoration<'_> {
    /// Puts it on `row`, and says whether that changed it.
    fn apply(&self, row: &mut MessageRow) -> bool {
        let day = self.day.as_deref().unwrap_or_default();
        let changed = row.first != self.first
            || row.day.as_str() != day
            || row.picture_url.as_str() != self.picture_url
            || row.reply_picture_url.as_str() != self.reply_picture_url
            || row.decoration_url.as_str() != self.decoration_url
            || row.unread_start != self.unread_start
            || row.highlighted != self.highlighted;
        if changed {
            row.first = self.first;
            row.day = day.into();
            row.picture_url = self.picture_url.into();
            row.reply_picture_url = self.reply_picture_url.into();
            row.decoration_url = self.decoration_url.into();
            row.unread_start = self.unread_start;
            row.highlighted = self.highlighted;
        }
        changed
    }
}

/// Shows `messages` in `model`, which shows `rows`, built for `shown`. A row
/// is built again only for a message that changed since, and set in the
/// model only where it changed; the rest stay as they are, so the list keeps
/// their state. Older messages go in above the rows shown, and the list keeps
/// those where they are on screen, even when it shows the first row; see
/// `i_slint_core::yapfast_anchor`.
pub(super) fn show_timeline(
    model: &VecModel<MessageRow>,
    rows: &mut Vec<MessageRow>,
    shown: &[Arc<ChatMessage>],
    messages: &[Arc<ChatMessage>],
    marks: &Marks<'_>,
    build: impl Fn(&ChatMessage) -> MessageRow,
) {
    let at: HashMap<&str, usize> = shown
        .iter()
        .enumerate()
        .map(|(index, message)| (message.id.as_str(), index))
        .collect();
    debug_assert_eq!(
        model.row_count(),
        shown.len(),
        "the rows shown are those built for `shown`"
    );
    let mut earlier = std::mem::take(rows);
    rows.reserve_exact(messages.len());
    let mut changed = Vec::with_capacity(messages.len());
    let mut before: Option<&ChatMessage> = None;
    let mut before_date = None;
    for (index, message) in messages.iter().enumerate() {
        let was = at.get(message.id.as_str()).copied();
        // A row the backend sent again is the same one.
        let unchanged = !marks.restyle
            && was.is_some_and(|was| Arc::ptr_eq(&shown[was], message) || shown[was] == *message);
        let date = yapfast_core::clock::date(message.timestamp);
        let new_day = date.is_some() && (before.is_none() || before_date != date);
        // The author shows again below the "New" divider.
        let unread_start = marks.first_unread == Some(message.item.as_str());
        let decoration = Decoration {
            first: new_day
                || unread_start
                || before.is_none_or(|before| !continues(before, message)),
            day: new_day.then(|| day(message.timestamp)),
            picture_url: if collapsed(message, marks.revealed) {
                ""
            } else {
                message.avatar.as_deref().unwrap_or_default()
            },
            reply_picture_url: message
                .reply
                .as_ref()
                .and_then(|reply| reply.avatar.as_deref())
                .unwrap_or_default(),
            decoration_url: if collapsed(message, marks.revealed) || !marks.decorations {
                ""
            } else {
                message.decoration.as_deref().unwrap_or_default()
            },
            unread_start,
            highlighted: message.highlighted(index >= marks.first_new),
        };
        // Each id shows once; a second would find its row taken.
        let row = was
            .map(|was| std::mem::take(&mut earlier[was]))
            .filter(|row| !row.id.is_empty());
        let (row, dirty) = match row {
            Some(mut row) if unchanged => {
                let dirty = decoration.apply(&mut row);
                (row, dirty)
            }
            row => {
                let mut built = build(message);
                built.picture = slint::Image::default();
                built.reply_picture = slint::Image::default();
                decoration.apply(&mut built);
                let dirty = row.is_none_or(|row| row != built);
                (built, dirty)
            }
        };
        rows.push(row);
        changed.push(dirty);
        before = Some(message);
        before_date = date;
    }
    let update = || {
        let same = |old: usize, new: usize| shown[old].id == messages[new].id;
        let common = shown.len().min(messages.len());
        let start = (0..common).take_while(|&index| same(index, index)).count();
        let end = (0..common - start)
            .take_while(|&back| same(shown.len() - 1 - back, messages.len() - 1 - back))
            .count();
        for _ in start..shown.len() - end {
            model.remove(start);
        }
        for (index, row) in rows.iter().enumerate() {
            if (start..messages.len() - end).contains(&index) {
                model.insert(index, row.clone());
            } else if changed[index] {
                model.set_row_data(index, row.clone());
            }
        }
    };
    let older = shown.first().is_some_and(|first| {
        messages
            .iter()
            .skip(1)
            .any(|message| message.id == first.id)
    });
    if older {
        i_slint_core::yapfast_anchor::hold(update);
    } else {
        update();
    }
}

/// Someone's Markdown as styled text, with web addresses linked. Any HTML
/// in it shows as typed: Slint draws `<font>` and `<u>`, which would let a
/// sender colour text, hide it, or dress it up as a mention.
pub(super) fn markdown(body: &str) -> slint::StyledText {
    styled_or(&links::escape_html(&emoji_text::text(body)), body)
}

/// Someone's Markdown with its mentions linked, ready for [`styled`]. Its
/// HTML is escaped, so the only HTML left is the mentions' own. The
/// mentions are where the body as rendered has them, so this comes before
/// anything else changes it.
fn with_mentions(body: &str, mentions: &[yapfast_core::mention::Mentioned]) -> String {
    mention::linked(body, mentions, theme::mention_color())
}

/// Markdown as styled text, with web addresses linked. Slint's Markdown
/// subset rejects some syntax, such as headings; those show as typed,
/// keeping their links.
fn styled(body: &str) -> slint::StyledText {
    styled_or(body, body)
}

/// [`styled`], or `typed` as plain text when Slint rejects the Markdown. A
/// link showing other text than where it goes asks before it opens.
fn styled_or(body: &str, typed: &str) -> slint::StyledText {
    let body = no_rules(body);
    let body = keep_blank_lines(&body);
    slint::StyledText::from_markdown(&links::linkify(&links::mark_masked(&body)))
        .ok()
        .or_else(|| plain_with_links(typed))
        .unwrap_or_else(|| slint::StyledText::from_plain_text(typed))
}

/// `text` with its lines of nothing but `-`, `=`, `*` or `_` escaped, so they
/// show as typed, as in Discord's client: Markdown would make them rules,
/// or headings of the line above, which Slint rejects.
fn no_rules(text: &str) -> std::borrow::Cow<'_, str> {
    let rule = |line: &str| {
        let marks = line.trim();
        let indent = line.len() - line.trim_start().len();
        indent < 4
            && marks.chars().next().is_some_and(|first| {
                "-=*_".contains(first) && marks.chars().all(|c| c == first || c == ' ')
            })
    };
    if !text.split('\n').any(rule) {
        return text.into();
    }
    text.split('\n')
        .map(|line| {
            if rule(line) {
                let indent = line.len() - line.trim_start().len();
                format!("{}\\{}", &line[..indent], &line[indent..])
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into()
}

/// `text` with each blank line between its lines holding a no-break space.
/// Discord shows every line break as typed, where Markdown runs blank lines
/// together into one paragraph break, which Slint draws as a plain line
/// break; a line with a no-break space isn't blank to Markdown, so it shows
/// as an empty line. Fenced code keeps its lines as they are.
pub(super) fn keep_blank_lines(text: &str) -> std::borrow::Cow<'_, str> {
    let blank = |line: &str| line.trim().is_empty();
    let lines: Vec<&str> = text.split('\n').collect();
    let (Some(first), Some(last)) = (
        lines.iter().position(|line| !blank(line)),
        lines.iter().rposition(|line| !blank(line)),
    ) else {
        return text.into();
    };
    if !lines[first..last].iter().any(|line| blank(line)) {
        return text.into();
    }
    let mut fenced = false;
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            if !fenced && index > first && index < last && blank(line) {
                "\u{a0}"
            } else {
                line
            }
        })
        .collect();
    kept.join("\n").into()
}

/// An embed's title in bold, linking to its page when it has one.
pub(super) fn embed_title(title: &str, url: &str) -> slint::StyledText {
    let escaped = escape_markdown(title);
    let markdown = if url.is_empty() {
        format!("**{escaped}**")
    } else {
        format!("**[{escaped}](<{url}>)**")
    };
    slint::StyledText::from_markdown(&markdown)
        .unwrap_or_else(|_| slint::StyledText::from_plain_text(title))
}

/// `text` with Markdown's punctuation escaped by backslashes, so it shows as typed.
pub(super) fn escape_markdown(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escape = c.is_ascii_punctuation().then_some('\\');
            escape.into_iter().chain([c])
        })
        .collect()
}

/// A notice as Discord's client words it: its author's name in bold, in
/// their role colour, opening their card; what it tells of, a pin's with
/// links to the message and to the pinned messages; then when.
pub(super) fn notice_text(
    message: &ChatMessage,
    notice: &yapfast_core::chat::Notice,
    time: &str,
) -> slint::StyledText {
    use yapfast_core::chat::NoticeKind;
    let hex = |rgb: u32| format!("#{:06x}", rgb & 0xff_ffff);
    let strong = |text: &str, rgb: u32, link: Option<&str>| {
        let text = format!("**{}**", escape_markdown(text));
        let text = match link {
            Some(link) => format!("[{text}](<{link}>)"),
            None => text,
        };
        format!("<font color=\"{}\">{text}</font>", hex(rgb))
    };
    let sender = discord::sender_user(&message.sender).unwrap_or(&message.sender);
    let profile = (!sender.is_empty()).then(|| format!("{AUTHOR_LINK}{sender}"));
    let author = strong(
        &message.author,
        message.color.unwrap_or_else(theme::text_color),
        profile.as_deref(),
    );
    let text = match &notice.kind {
        NoticeKind::Pinned(item) => {
            let jump = item.as_ref().map(|item| format!("{JUMP_LINK}{item}"));
            fill(
                tr(" pinned {} to this channel. See all {}."),
                &[
                    &strong(tr("a message"), theme::text_color(), jump.as_deref()),
                    &strong(tr("pinned messages"), theme::text_color(), Some(PINS_LINK)),
                ],
            )
        }
        // A possessive follows the name without a space.
        _ if notice.text.starts_with('\'') => escape_markdown(&notice.text),
        _ => format!(" {}", escape_markdown(&notice.text)),
    };
    let time = format!(
        "<font color=\"{}\">{}</font>",
        hex(theme::muted_color()),
        escape_markdown(time)
    );
    slint::StyledText::from_markdown(&format!("{author}{text}  {time}"))
        .unwrap_or_else(|_| slint::StyledText::from_plain_text(&message.body))
}

/// The icon a notice shows in place of its author's avatar.
fn notice_icon(notice: Option<&yapfast_core::chat::Notice>) -> &'static str {
    use yapfast_core::chat::NoticeKind;
    match notice.map(|notice| &notice.kind) {
        None => "",
        Some(NoticeKind::Pinned(_)) => "pin",
        Some(NoticeKind::Joined) => "join",
        Some(NoticeKind::ThreadStarted) => "thread",
        Some(NoticeKind::Call) => "call",
        Some(NoticeKind::PollResult) => "poll",
    }
}

/// `text` in bold, as typed.
pub(super) fn bold(text: &str) -> slint::StyledText {
    slint::StyledText::from_markdown(&format!("**{}**", escape_markdown(text)))
        .unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
}

/// An embed's fields in rows: up to three inline fields share one, and any
/// other field takes a row of its own, as in Discord.
pub(super) fn field_rows(fields: &[EmbedField]) -> ModelRc<EmbedFieldRow> {
    let mut rows: Vec<Vec<yapfast_ui::EmbedField>> = Vec::new();
    let mut open = false;
    for field in fields {
        let value_blocks = markdown_blocks(&field.value);
        let cell = yapfast_ui::EmbedField {
            name: bold(&field.name),
            value: if value_blocks.row_count() > 0 {
                slint::StyledText::default()
            } else {
                markdown(&field.value)
            },
            value_blocks,
        };
        match rows.last_mut() {
            Some(row) if open && field.inline && row.len() < 3 => row.push(cell),
            _ => rows.push(vec![cell]),
        }
        open = field.inline;
    }
    ModelRc::new(VecModel::from(
        rows.into_iter()
            .map(|fields| EmbedFieldRow {
                fields: ModelRc::new(VecModel::from(fields)),
            })
            .collect::<Vec<_>>(),
    ))
}

/// Plain text with its web addresses linked; none when it has none.
pub(super) fn plain_with_links(body: &str) -> Option<slint::StyledText> {
    links::plain_with_links(body)
        .and_then(|markdown| slint::StyledText::from_markdown(&markdown).ok())
}

/// Scrolls the timeline to the message at `index`: a third of the way down, highlighted
/// for a moment where `lit`, else to the top, as for the first unread message. The
/// list builds rows as they come into view, so the row settles its place a few times
/// as those around it measure.
pub(super) fn show_message(window: &YapfastWindow, index: usize, item: &str, lit: bool) {
    // At once, so the list stops following the newest message before anything
    // else scrolls it.
    window.invoke_show_message(i32::try_from(index).unwrap_or(i32::MAX), item.into(), lit);
    let shown: SharedString = item.into();
    for delay in [0, 30, 100, 250, 500] {
        let weak = window.as_weak();
        let shown = shown.clone();
        slint::Timer::single_shot(Duration::from_millis(delay), move || {
            if let Some(window) = weak.upgrade()
                && window.get_jump_item() == shown
            {
                // History may have loaded above it since.
                let index = window
                    .get_messages()
                    .iter()
                    .position(|row| row.item == shown)
                    .map_or(-1, |index| i32::try_from(index).unwrap_or(i32::MAX));
                window.invoke_settle_jump(index);
            }
        });
    }
    let weak = window.as_weak();
    slint::Timer::single_shot(Duration::from_millis(2500), move || {
        if let Some(window) = weak.upgrade()
            && window.get_jump_item() == shown
        {
            window.set_jump_item(SharedString::default());
            window.invoke_follow_at_end();
        }
    });
}

/// A message as the timeline shows it. `revealed` holds the spoilers clicked, as
/// `{item}/{index}` for text and `{item}/a{index}` for attachments, and what
/// was picked or shown on polls; see `polls`.
pub(super) fn message_row(message: &ChatMessage, revealed: &HashSet<String>) -> MessageRow {
    message_row_within(message, revealed, None)
}

/// [`message_row`] with its pictures, and its link previews', shrunk to fit
/// `fit` pixels beside the avatar, as the narrow thread beside a channel has.
pub(super) fn message_row_within(
    message: &ChatMessage,
    revealed: &HashSet<String>,
    fit: Option<i32>,
) -> MessageRow {
    let laid_out =
        |previews: &[Preview<'_>], covered: &dyn Fn(usize) -> bool, room: Option<i32>| match room {
            Some(room) => gallery_within(previews, covered, room.max(1)),
            None => gallery(previews, covered),
        };
    // Plain text shows as typed, its web addresses linked.
    // Mentions link to who they mention, which opens their card.
    // Custom emoji show as their glyphs once they have them, and clicking any
    // emoji, custom or Unicode, opens its card.
    let mentioned = if message.formatted {
        with_mentions(&message.body, &message.mentions)
    } else {
        String::new()
    };
    let glyphs = custom_emoji::text(&mentioned, &message.emoji);
    let jumbo = message.formatted && custom_emoji::only_emoji(&glyphs);
    // Headings, quotes and code each show apart; text alone shows as one.
    let blocks = if message.formatted && message.notice.is_none() {
        body_blocks(message, Some(revealed))
    } else {
        None
    };
    let styled = if blocks.is_some() {
        slint::StyledText::default()
    } else if let Some(notice) = &message.notice {
        notice_text(message, notice, &message.time)
    } else if message.formatted {
        let carded = emoji_text::linked(&custom_emoji::linked(&mentioned, &message.emoji));
        if message.spoilers {
            styled(&spoiler::mark(&carded, &message.item, 0, |index| {
                revealed.contains(&format!("{}/{index}", message.item))
            }))
        } else {
            styled(&carded)
        }
    } else {
        plain_with_links(&message.body)
            .unwrap_or_else(|| slint::StyledText::from_plain_text(&message.body))
    };
    // An image with a preview shows in the gallery; any other attachment shows
    // a file card. Embeds' pictures show in their cards.
    // Pictures, and those on their way, which show as skeletons.
    let previews = |placement: Placement| -> Vec<Preview<'_>> {
        message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| attachment.placement == placement)
            .filter(|(_, attachment)| {
                attachment.preview.is_some() || loading(attachment) || attachment.video
            })
            .map(|(index, attachment)| (index, attachment, attachment.preview.as_ref()))
            .collect()
    };
    let covered = |index: usize| !revealed.contains(&format!("{}/a{index}", message.item));
    let (images, gallery_width, gallery_height) =
        laid_out(&previews(Placement::Message), &covered, fit);
    let stickers = previews(Placement::Sticker)
        .into_iter()
        .map(|(index, attachment, preview)| {
            let Some(preview) = preview else {
                return (
                    ImageCell {
                        index: index as i32,
                        name: attachment.name.clone().into(),
                        width: STICKER_SIZE,
                        height: STICKER_SIZE,
                        loading: true,
                        ..Default::default()
                    },
                    None,
                );
            };
            let (frames, delays) = animation(preview);
            (
                ImageCell {
                    index: index as i32,
                    name: attachment.name.clone().into(),
                    // Discord draws a message's sticker 160px square, whatever
                    // its picture came at.
                    width: STICKER_SIZE,
                    height: STICKER_SIZE,
                    frames,
                    delays,
                    ..Default::default()
                },
                Some(preview.clone()),
            )
        })
        .collect();
    let embeds: Vec<EmbedCard> = message
        .embeds
        .iter()
        .enumerate()
        .map(|(index, embed)| {
            let thumbnail = previews(Placement::Thumbnail(index)).into_iter().next();
            // A card's padding, and its thumbnail beside the text, take from it.
            let room = fit.map(|fit| {
                fit - EMBED_PADDING
                    - if thumbnail.is_some() {
                        EMBED_THUMBNAIL
                    } else {
                        0
                    }
            });
            let (images, gallery_width, gallery_height) =
                laid_out(&previews(Placement::Embed(index)), &covered, room);
            let description_blocks = markdown_blocks(&embed.description);
            EmbedCard {
                color: role_color(embed.color),
                has_color: embed.color.is_some(),
                provider: slint::StyledText::from_plain_text(&embed.provider),
                has_provider: !embed.provider.is_empty(),
                author: bold(&embed.author),
                has_author: !embed.author.is_empty(),
                title: embed_title(&embed.title, &embed.url),
                has_title: !embed.title.is_empty(),
                url: embed.url.clone().into(),
                description: if description_blocks.row_count() > 0 {
                    slint::StyledText::default()
                } else {
                    markdown(&embed.description)
                },
                has_description: !embed.description.is_empty(),
                description_blocks,
                field_rows: field_rows(&embed.fields),
                footer: slint::StyledText::from_plain_text(&embed.footer),
                has_footer: !embed.footer.is_empty(),
                thumbnail: thumbnail
                    .and_then(|(_, _, preview)| preview.map(|preview| picture(preview)))
                    .unwrap_or_default(),
                thumbnail_loading: thumbnail.is_some_and(|(_, _, preview)| preview.is_none()),
                thumbnail_index: thumbnail.map_or(-1, |(index, _, _)| index as i32),
                has_thumbnail: thumbnail.is_some(),
                images,
                gallery_width,
                gallery_height,
                video: embed.video,
            }
        })
        .collect();
    let files = |placement: Placement| -> Vec<FileCell> {
        message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| {
                attachment.preview.is_none()
                    && !loading(attachment)
                    && !attachment.video
                    && attachment.placement == placement
            })
            .map(|(index, attachment)| {
                let size = attachment.size.map(file_size);
                let detail = match (attachment.image, attachment.preview_failed, size) {
                    (true, true, Some(size)) => fill(tr("{} · Preview unavailable"), &[&size]),
                    (true, true, None) => tr("Preview unavailable").into(),
                    (_, _, Some(size)) => size,
                    _ => String::new(),
                };
                // A voice message's recording is its first attachment.
                let voice = message.voice.as_ref().filter(|_| index == 0);
                FileCell {
                    index: index as i32,
                    name: attachment.name.clone().into(),
                    detail: detail.into(),
                    voice: voice.is_some(),
                    waveform: voice.map_or_else(ModelRc::default, |voice| {
                        ModelRc::new(VecModel::from(waveform_bars(&voice.waveform)))
                    }),
                    duration: voice
                        .map(|voice| super::refresh::video_time(voice.duration_ms, None).into())
                        .unwrap_or_default(),
                }
            })
            .collect()
    };
    let cards: Vec<ComponentCard> = message
        .cards
        .iter()
        .map(|card| ComponentCard {
            framed: card.framed,
            color: role_color(card.color),
            has_color: card.color.is_some(),
            rows: ModelRc::new(VecModel::from(
                card.rows
                    .iter()
                    .map(|row| card_row(message, row, &previews, &covered, &files))
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    let (delivery, can_retry, can_cancel) = match message.delivery {
        Delivery::Sent => ("", false, false),
        Delivery::Sending => (tr("Sending..."), false, true),
        Delivery::Waiting => (
            tr("Not sent yet. Yapfast sends it when the connection returns."),
            false,
            true,
        ),
        Delivery::Failed => (tr("Not sent. The server refused this message."), true, true),
    };
    let reply_author = message
        .reply
        .as_ref()
        .map(|reply| reply.author.as_str())
        .unwrap_or_default();
    let row = MessageRow {
        picture_url: SharedString::default(),
        reply_picture_url: SharedString::default(),
        decoration_url: SharedString::default(),
        name_style: name_style::row(message.style.as_deref(), &message.author),
        id: message.id.clone().into(),
        item: message.item.clone().into(),
        author: message.author.clone().into(),
        tag: tag_text(message.tag.as_ref()),
        tag_badge: tag_badge(message.tag.as_ref()),
        sender: discord::sender_user(&message.sender)
            .unwrap_or(&message.sender)
            .into(),
        color: role_color(message.color),
        has_color: message.color.is_some(),
        bot: message.bot,
        app_id: message
            .application_id
            .as_deref()
            .or_else(|| {
                message
                    .bot
                    .then(|| discord::sender_user(&message.sender))
                    .flatten()
            })
            .unwrap_or_default()
            .into(),
        initial: initial(&message.author),
        tint: tint(&message.author),
        // Virtualized avatars resolve their URLs when their components are built.
        picture: slint::Image::default(),
        body: message.body.clone().into(),
        time: message.time.clone().into(),
        // The gutter beside a grouped message shows only the clock time.
        short_time: message
            .time
            .split_whitespace()
            .next()
            .filter(|time| time.contains(':'))
            .unwrap_or_default()
            .into(),
        own: message.own,
        // Grouping depends on the neighbouring messages; refresh sets both.
        first: true,
        day: SharedString::default(),
        unread_start: false,
        // A reply to this account's is highlighted too while it's new; refresh knows.
        highlighted: message.mentions_me,
        jumbo,
        styled,
        notice: notice_icon(message.notice.as_ref()).into(),
        author_styled: bold(&message.author),
        time_styled: slint::StyledText::from_plain_text(&message.time),
        edited: message.edited,
        deleted: message.deleted,
        forwarded: message.forwarded,
        has_reply: message.reply.is_some(),
        reply_author: reply_author.into(),
        reply_initial: initial(reply_author),
        reply_tint: tint(reply_author),
        reply_color: role_color(message.reply.as_ref().and_then(|reply| reply.color)),
        reply_has_color: message
            .reply
            .as_ref()
            .is_some_and(|reply| reply.color.is_some()),
        // Virtualized avatars resolve their URLs when their components are built.
        reply_picture: slint::Image::default(),
        reply_body: message
            .reply
            .as_ref()
            .map(|reply| reply.body.clone())
            .unwrap_or_default()
            .into(),
        reply_item: message
            .reply
            .as_ref()
            .and_then(|reply| reply.item.clone())
            .unwrap_or_default()
            .into(),
        reactions: ModelRc::new(VecModel::from(
            message
                .reactions
                .iter()
                .map(|reaction| ReactionChip {
                    key: reaction.key.clone().into(),
                    label: custom_emoji::text(&reaction.key, &message.emoji).into(),
                    count: i32::try_from(reaction.count).unwrap_or(i32::MAX),
                    mine: reaction.mine,
                })
                .collect::<Vec<_>>(),
        )),
        images,
        gallery_width,
        gallery_height,
        stickers: image_cells(stickers),
        files: ModelRc::new(VecModel::from(files(Placement::Message))),
        embeds: ModelRc::new(VecModel::from(embeds)),
        cards: ModelRc::new(VecModel::from(cards)),
        blocks: blocks.map_or_else(ModelRc::default, |blocks| {
            ModelRc::new(VecModel::from(blocks))
        }),
        expanded: expanded(&message.item, revealed),
        hidden_card: SharedString::default(),
        delivery: delivery.into(),
        delivery_failed: message.delivery == Delivery::Failed,
        can_retry,
        can_cancel,
        can_reply: message.can_reply,
        can_edit: message.can_edit,
        can_delete: message.can_delete,
        can_react: message.can_react,
        can_forward: message.can_forward,
        pinned: message.pinned,
        can_pin: message.can_pin,
        thread_room: message
            .thread
            .as_ref()
            .map_or_else(SharedString::default, |thread| thread.room.as_str().into()),
        thread_name: message
            .thread
            .as_ref()
            .map_or_else(SharedString::default, |thread| thread.name.as_str().into()),
        thread_detail: message
            .thread
            .as_ref()
            .map_or_else(SharedString::default, |thread| thread_detail(thread).into()),
        poll: message
            .poll
            .as_ref()
            .map(|poll| {
                let now = yapfast_core::clock::now_ms() / 1000;
                super::polls::poll_card(&message.item, poll, &message.emoji, revealed, now, true)
            })
            .unwrap_or_default(),
        has_poll: message.poll.is_some(),
    };
    if collapsed(message, revealed) {
        // Discord shows nothing of a blocked message until asked.
        let note = format!(
            "*Blocked message.* [Show it](<{BLOCKED_LINK}{}>)",
            message.item
        );
        return MessageRow {
            author: tr("Blocked user").into(),
            sender: SharedString::default(),
            author_styled: bold(tr("Blocked user")),
            initial: "?".into(),
            tint: Default::default(),
            color: Default::default(),
            has_color: false,
            bot: false,
            forwarded: false,
            body: tr("Blocked message.").into(),
            styled: slint::StyledText::from_markdown(&note)
                .unwrap_or_else(|_| slint::StyledText::from_plain_text(tr("Blocked message."))),
            has_reply: false,
            reactions: ModelRc::default(),
            images: ModelRc::default(),
            gallery_width: 0,
            gallery_height: 0,
            stickers: ModelRc::default(),
            files: ModelRc::default(),
            embeds: ModelRc::default(),
            cards: ModelRc::default(),
            has_poll: false,
            can_reply: false,
            can_edit: false,
            can_react: false,
            can_forward: false,
            ..row
        };
    }
    row
}

/// How many bars a voice message's waveform shows, as Discord's client
/// draws it.
const WAVEFORM_BARS: usize = 32;

/// A voice message's waveform as its card's bars, each the loudest of its
/// stretch of the points, from 0 to 1. Without a waveform they're flat.
pub(super) fn waveform_bars(waveform: &[u8]) -> Vec<f32> {
    (0..WAVEFORM_BARS)
        .map(|bar| {
            let start = bar * waveform.len() / WAVEFORM_BARS;
            let end = ((bar + 1) * waveform.len() / WAVEFORM_BARS).max(start + 1);
            let loudest = waveform.get(start..end.min(waveform.len()));
            f32::from(loudest.into_iter().flatten().copied().max().unwrap_or(0)) / 255.0
        })
        .collect()
}

/// A row of a Discord app's card, its pictures and files found among the
/// message's attachments by slot.
fn card_row<'a>(
    message: &'a ChatMessage,
    row: &yapfast_core::chat::CardRow,
    previews: &dyn Fn(Placement) -> Vec<Preview<'a>>,
    covered: &dyn Fn(usize) -> bool,
    files: &dyn Fn(Placement) -> Vec<FileCell>,
) -> CardRow {
    use yapfast_core::chat::{Beside, CardRow as Row};
    let blocks = |text: &str| {
        ModelRc::new(VecModel::from(text_blocks(&custom_emoji::text(
            text,
            &message.emoji,
        ))))
    };
    match row {
        Row::Text { text, beside } => {
            let thumbnail = match beside {
                Some(Beside::Thumbnail(slot)) => {
                    previews(Placement::Component(*slot)).into_iter().next()
                }
                _ => None,
            };
            let button = match beside {
                Some(Beside::Button(button)) => Some(card_button(button, &message.emoji)),
                _ => None,
            };
            CardRow {
                kind: CardRowKind::Text,
                blocks: blocks(text),
                thumbnail: thumbnail
                    .and_then(|(_, _, preview)| preview.map(|preview| picture(preview)))
                    .unwrap_or_default(),
                thumbnail_loading: thumbnail.is_some_and(|(_, _, preview)| preview.is_none()),
                thumbnail_index: thumbnail.map_or(-1, |(index, _, _)| index as i32),
                thumbnail_spoiler: thumbnail
                    .is_some_and(|(index, attachment, _)| attachment.spoiler && covered(index)),
                has_thumbnail: thumbnail.is_some(),
                has_button: button.is_some(),
                button: button.unwrap_or_default(),
                ..Default::default()
            }
        }
        Row::Media(slot) => {
            let placement = Placement::Component(*slot);
            let (images, gallery_width, gallery_height) =
                gallery_within(&previews(placement), covered, CARD_GALLERY_WIDTH);
            CardRow {
                kind: CardRowKind::Media,
                images,
                gallery_width,
                gallery_height,
                files: ModelRc::new(VecModel::from(files(placement))),
                ..Default::default()
            }
        }
        Row::Separator { divider, large } => CardRow {
            kind: CardRowKind::Separator,
            divider: *divider,
            large: *large,
            ..Default::default()
        },
        Row::Buttons(buttons) => CardRow {
            kind: CardRowKind::Buttons,
            buttons: ModelRc::new(VecModel::from(
                buttons
                    .iter()
                    .map(|button| card_button(button, &message.emoji))
                    .collect::<Vec<_>>(),
            )),
            ..Default::default()
        },
        Row::Select {
            text,
            disabled,
            custom_id,
            ..
        } => CardRow {
            kind: CardRowKind::Select,
            select: text.as_str().into(),
            select_id: custom_id.as_str().into(),
            select_enabled: !disabled && !custom_id.is_empty(),
            ..Default::default()
        },
        Row::Unknown => CardRow {
            kind: CardRowKind::Unknown,
            ..Default::default()
        },
    }
}

/// The widest a card's gallery shows, inside the card's padding.
const CARD_GALLERY_WIDTH: i32 = 566;
/// What a link preview's card takes across beside its pictures, as
/// `EmbedView` in ui/messages.slint pads it: 16px each side.
const EMBED_PADDING: i32 = 32;
/// A card's thumbnail and the space before it, as `EmbedView` lays it out.
const EMBED_THUMBNAIL: i32 = 96;

fn card_button(
    button: &yapfast_core::chat::CardButton,
    emoji: &[yapfast_core::chat::CustomEmoji],
) -> CardButton {
    use yapfast_core::chat::ButtonStyle;
    CardButton {
        label: custom_emoji::text(&button.label, emoji).into(),
        style: match button.style {
            ButtonStyle::Primary => CardButtonStyle::Primary,
            ButtonStyle::Secondary => CardButtonStyle::Secondary,
            ButtonStyle::Success => CardButtonStyle::Success,
            ButtonStyle::Danger => CardButtonStyle::Danger,
            ButtonStyle::Link => CardButtonStyle::Link,
            ButtonStyle::Premium => CardButtonStyle::Premium,
        },
        url: button.url.clone().unwrap_or_default().into(),
        custom_id: button.custom_id.clone().unwrap_or_default().into(),
        sku_id: button.sku_id.clone().unwrap_or_default().into(),
        enabled: !button.disabled
            && (button.url.is_some() || button.custom_id.is_some() || button.sku_id.is_some()),
    }
}

/// Discord Markdown in the blocks Discord's client lays out, as a card
/// shows them; text alone is one block.
pub(super) fn text_blocks(text: &str) -> Vec<TextBlock> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    blocks_of(text, |_, text| links::escape_html(text).into_owned()).unwrap_or_else(|| {
        vec![TextBlock {
            text: markdown(text.trim_matches('\n')),
            ..Default::default()
        }]
    })
}

/// Someone's Markdown in blocks, as [`markdown`] styles it, when it has
/// headings, quotes or code; otherwise none, and [`markdown`] shows it.
pub(super) fn markdown_blocks(markdown: &str) -> ModelRc<TextBlock> {
    blocks_of(markdown, |_, text| links::escape_html(text).into_owned())
        .map_or_else(ModelRc::default, |blocks| {
            ModelRc::new(VecModel::from(blocks))
        })
}

/// A message's body in the blocks Discord's client lays out, with its
/// mentions, custom emoji and spoilers; `None` when it's text alone. Its
/// spoilers are covered but those `revealed`, or all hidden as text
/// without it, as a found message's are.
fn body_blocks(
    message: &ChatMessage,
    revealed: Option<&HashSet<String>>,
) -> Option<Vec<TextBlock>> {
    let mut spoilers = 0;
    blocks_of(&message.body, |block, text| {
        let mentions: Vec<_> = message
            .mentions
            .iter()
            .filter_map(|mention| {
                let at: Vec<usize> = mention
                    .at
                    .iter()
                    .filter_map(|&at| block.offset(at, mention.text.len()))
                    .collect();
                (!at.is_empty()).then(|| yapfast_core::mention::Mentioned {
                    at,
                    ..mention.clone()
                })
            })
            .collect();
        let linked = custom_emoji::linked(&with_mentions(text, &mentions), &message.emoji);
        let Some(revealed) = revealed else {
            return emoji_text::text(&spoiler::hide(&linked));
        };
        if !message.spoilers {
            return emoji_text::linked(&linked);
        }
        // Spoilers count on through the message, as clicking one reveals it.
        let marked = spoiler::mark(&linked, &message.item, spoilers, |index| {
            revealed.contains(&format!("{}/{index}", message.item))
        });
        spoilers += spoiler::count(&linked);
        emoji_text::linked(&marked)
    })
}

/// `markdown`'s blocks, each but code made Markdown with its HTML escaped
/// by `prepare`, and spaced as Discord's client spaces them; `None` when
/// it's text alone.
fn blocks_of(
    markdown: &str,
    mut prepare: impl FnMut(&Block, &str) -> String,
) -> Option<Vec<TextBlock>> {
    let blocks = yapfast_core::blocks::split(markdown)?;
    // The space above a block and below it, which the next block's takes
    // in, as margins between them do; a first heading's is less.
    let above = |kind: Kind, first: bool| match kind {
        Kind::Heading(3) if first => 4.0,
        Kind::Heading(_) if first => 8.0,
        Kind::Heading(_) => 16.0,
        Kind::Code => 6.0,
        Kind::Text | Kind::Subtext => 0.0,
    };
    let below = |kind: Kind| {
        if matches!(kind, Kind::Heading(_)) {
            8.0
        } else {
            0.0
        }
    };
    // A quote has 4px above and below its bar.
    const QUOTE: f32 = 4.0;
    let mut shown = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let before = index.checked_sub(1).map(|index| &blocks[index]);
        let typed = block.text(markdown);
        let text = match block.kind {
            Kind::Code => slint::StyledText::from_plain_text(&typed),
            Kind::Heading(_) => styled_or(&format!("**{}**", prepare(block, &typed)), &typed),
            Kind::Subtext => styled_or(&prepare(block, &typed), &typed),
            // Discord's client shows every line, blank or not.
            Kind::Text => {
                let prepared = prepare(block, &typed);
                let lines: Vec<&str> = prepared
                    .split('\n')
                    .map(|line| {
                        if line.trim().is_empty() {
                            "\u{a0}"
                        } else {
                            line
                        }
                    })
                    .collect();
                styled_or(&lines.join("\n"), &typed)
            }
        };
        let starts_quote = block.quoted && !before.is_some_and(|before| before.quoted);
        let top = above(block.kind, before.is_none() || starts_quote);
        let (margin, padding) = match before {
            None if block.quoted => (QUOTE, top),
            None => (top, 0.0),
            Some(_) if starts_quote => (QUOTE.max(below(blocks[index - 1].kind)), top),
            Some(before) if block.quoted => (0.0, top.max(below(before.kind))),
            Some(before) if before.quoted => (QUOTE.max(top), 0.0),
            Some(before) => (top.max(below(before.kind)), 0.0),
        };
        shown.push(TextBlock {
            text,
            level: match block.kind {
                Kind::Text => 0,
                Kind::Heading(level) => level.into(),
                Kind::Subtext => 4,
                Kind::Code => 5,
            },
            quoted: block.quoted,
            quote_start: starts_quote,
            quote_end: block.quoted && !blocks.get(index + 1).is_some_and(|next| next.quoted),
            margin,
            padding,
        });
    }
    Some(shown)
}

/// What a thread's link says after its name: how many messages it has,
/// as Discord counts them, and whether it's archived.
/// What a reaction's tooltip says, as Discord's client words it: the
/// emoji's name, then up to three of the `count` who reacted by name and
/// how many more.
pub(super) fn reaction_tip(emoji: &str, names: &[String], count: usize) -> String {
    let others = count.saturating_sub(names.len());
    let more = match others {
        1 => "1 other".to_owned(),
        others => fill(tr("{} others"), &[&others]),
    };
    let people = match (names, others) {
        ([one], 0) => one.clone(),
        ([one], _) => fill(tr("{0} and {1}"), &[&one, &more]),
        ([one, two], 0) => fill(tr("{0} and {1}"), &[&one, &two]),
        ([one, two], _) => fill(tr("{0}, {1}, and {2}"), &[&one, &two, &more]),
        ([one, two, three], 0) => fill(tr("{0}, {1}, and {2}"), &[&one, &two, &three]),
        ([one, two, three, ..], _) => {
            fill(tr("{0}, {1}, {2}, and {3}"), &[&one, &two, &three, &more])
        }
        _ => fill(tr("{} people"), &[&count]),
    };
    fill(tr("{0} reacted by {1}"), &[&emoji, &people])
}

fn thread_detail(thread: &yapfast_core::chat::ThreadLink) -> String {
    let count = match thread.messages {
        None => None,
        Some(0) => Some(tr("No messages yet").to_owned()),
        Some(count) => Some(trn("{n} message", "{n} messages", count as usize)),
    };
    match (count, thread.archived) {
        (Some(count), true) => fill(tr("{}, archived"), &[&count]),
        (Some(count), false) => count,
        (None, true) => "Archived".into(),
        (None, false) => String::new(),
    }
}

/// How wide a found message's card shows its pictures at most: its pinned
/// messages' and search's 420px, less the card's padding and avatar.
const FOUND_GALLERY_WIDTH: i32 = 300;

/// A pinned message or a search result as its card shows it, stamped as the
/// timeline stamps it on the chosen clock, its spoilers hidden, its first
/// pictures shown and its other files named.
pub(super) fn found_row(message: &ChatMessage, twelve_hour: bool) -> FoundRow {
    let blocks = if message.formatted {
        body_blocks(message, None)
    } else {
        None
    };
    let styled = if blocks.is_some() {
        slint::StyledText::default()
    } else if message.formatted {
        let linked = with_mentions(&message.body, &message.mentions);
        styled(&emoji_text::text(&spoiler::hide(&custom_emoji::text(
            &linked,
            &message.emoji,
        ))))
    } else {
        plain_with_links(&message.body)
            .unwrap_or_else(|| slint::StyledText::from_plain_text(&message.body))
    };
    let now = yapfast_core::clock::now_ms() / 1000;
    let time = yapfast_core::clock::sent(message.timestamp, now, twelve_hour, false);
    let pictures: Vec<Preview<'_>> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, attachment)| {
            attachment.placement == Placement::Message && (attachment.image || attachment.video)
        })
        .take(yapfast_core::chat::FOUND_PICTURES)
        .filter(|(_, attachment)| attachment.preview.is_some() || loading(attachment))
        .map(|(index, attachment)| (index, attachment, attachment.preview.as_ref()))
        .collect();
    // Spoilers stay covered: clicking the card shows the message.
    let (images, gallery_width, gallery_height) =
        gallery_within(&pictures, &|_| true, FOUND_GALLERY_WIDTH);
    let files: Vec<&str> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|(index, attachment)| {
            attachment.placement == Placement::Message
                && !pictures.iter().any(|(shown, _, _)| shown == index)
        })
        .map(|(_, attachment)| attachment.name.as_str())
        .collect();
    FoundRow {
        item: message.item.clone().into(),
        author: message.author.clone().into(),
        initial: initial(&message.author),
        tint: tint(&message.author),
        picture_url: message.avatar.clone().unwrap_or_default().into(),
        color: role_color(message.color),
        has_color: message.color.is_some(),
        when: if message.timestamp == 0 {
            SharedString::default()
        } else {
            time.into()
        },
        styled,
        has_body: !message.body.trim().is_empty(),
        blocks: blocks.map_or_else(ModelRc::default, |blocks| {
            ModelRc::new(VecModel::from(blocks))
        }),
        files: files.join(", ").into(),
        images,
        gallery_width,
        gallery_height,
        poll: message
            .poll
            .as_ref()
            .map(|poll| {
                let now = yapfast_core::clock::now_ms() / 1000;
                super::polls::poll_card(
                    &message.item,
                    poll,
                    &message.emoji,
                    &HashSet::new(),
                    now,
                    false,
                )
            })
            .unwrap_or_default(),
        has_poll: message.poll.is_some(),
        ..Default::default()
    }
}

/// A Discord channel's topic, as its header shows it: on one line, or in
/// full.
pub(super) fn topic_text(topic: &yapfast_core::chat::Topic, one_line: bool) -> slint::StyledText {
    let linked = with_mentions(&topic.body, &topic.mentions);
    let body = custom_emoji::text(&linked, &topic.emoji);
    let body = emoji_text::text(&body);
    let body = if one_line {
        body.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        body
    };
    styled(&body)
}

/// Whether Show more was clicked on a long message, which shows it whole
/// until Show less, while Yapfast runs.
pub(super) fn expanded(item: &str, revealed: &HashSet<String>) -> bool {
    revealed.contains(&fold_key(item))
}

/// What `revealed` holds for a long message shown whole.
pub(super) fn fold_key(item: &str) -> String {
    format!("{}/more", item)
}

/// Whether a message shows collapsed, being from someone blocked.
pub(super) fn collapsed(message: &ChatMessage, revealed: &HashSet<String>) -> bool {
    message.blocked && !revealed.contains(&format!("{}/blocked", message.item))
}

/// A Unicode emoji's card says it can be used anywhere.
const DEFAULT_EMOJI: &str = "This is a default emoji. It can be used anywhere.";
/// A server's emoji, in the card of one whose server this account knows.
const SERVER_EMOJI: &str = "This emoji is from one of your servers.";
/// A server's emoji, in the card of one whose server this account can't reach:
/// one it never joined, or one that's gone. Its card says only this, with no
/// count of who's online and no separator, since Discord tells it nothing.
const OTHER_EMOJI: &str = "Invite-Only Server";

/// The card a click on an emoji in a message's text opens: what the emoji is,
/// and, for one of a server's, the server it is from.
pub(super) fn emoji_card(
    target: &emoji_text::Target,
    server: Option<&yapfast_core::chat::ChatServer>,
    state: &AppState,
    pictures: &mut HashMap<String, slint::Image>,
) -> EmojiCard {
    let id = match target {
        emoji_text::Target::Unicode(emoji) => {
            return EmojiCard {
                name: emoji::shortcode(emoji).into(),
                glyph: emoji.as_str().into(),
                // The font's own 136px rendition, so a larger card and a
                // doubled display both draw it from more pixels than they show.
                picture: emoji_text::picture(emoji).unwrap_or_default(),
                description: DEFAULT_EMOJI.into(),
                ..Default::default()
            };
        }
        emoji_text::Target::Custom(id) => id,
    };
    // A custom emoji's name and picture come from the message that showed it:
    // its server's list of emoji may not reach here, being another server's.
    let listed = state
        .messages
        .iter()
        .flat_map(|message| &message.emoji)
        .find(|emoji| &emoji.id == id);
    // The server its card shows: one this account is in, and can reach. One
    // it left, or that's out of reach, shows no server at all, and says why.
    let server = server.filter(|server| !server.unavailable);
    let (server_id, server_name, server_letter, server_picture_url) = match server {
        Some(server) => (
            server.id.as_str().into(),
            server.name.as_str().into(),
            initial(&server.name),
            server.icon.as_deref().unwrap_or_default().into(),
        ),
        None => Default::default(),
    };
    // What Discord said of the server, which its card's counts and note come
    // from; before it answers, the note follows the features Yapfast has.
    let preview = server.and_then(|server| state.server_previews.get(&server.id));
    // Discover lists a server whose preview says so, and Yapfast keeps its own
    // word for a Community server either way: a preview that answers without
    // the feature would otherwise take the note away from a server that has it.
    let discoverable = server.is_some_and(|server| server.community)
        || preview.is_some_and(|preview| preview.discoverable);
    // A custom emoji whose picture moves animates on its card too, by the
    // name the rows register it under.
    let picture_key = listed
        .filter(|emoji| emoji.animated)
        .map_or_else(String::new, emoji::custom_key);
    EmojiCard {
        name: listed
            .map_or_else(String::new, |emoji| emoji.text.clone())
            .into(),
        picture_url: listed
            .map_or_else(String::new, |emoji| emoji.picture.clone())
            .into(),
        picture_key: picture_key.into(),
        picture: listed
            .map(|emoji| cached_picture(pictures, state, Some(&emoji.picture)))
            .unwrap_or_default(),
        description: if server.is_some() {
            SERVER_EMOJI
        } else {
            OTHER_EMOJI
        }
        .into(),
        server: server_id,
        server_name,
        server_picture_url,
        server_initial: server_letter,
        server_online: preview
            .and_then(|preview| preview.online)
            .map_or_else(String::new, |online| {
                fill(tr("{} Online"), &[&grouped(online)])
            })
            .into(),
        server_note: if discoverable {
            tr("Discoverable Server")
        } else {
            ""
        }
        .into(),
        ..Default::default()
    }
}

/// A default sticker's card: Discord's own packs are usable anywhere.
const DEFAULT_STICKER: &str = "This is a default sticker. It can be used anywhere.";

/// A server's sticker's card, when the server is one this account can reach.
const SERVER_STICKER: &str = "This sticker is from one of your servers.";

/// The id of the sticker a message's attachment is, from its picture's
/// address: `/stickers/{id}.png?size=160`, `.gif` or `.json`.
fn sticker_id(url: &str) -> Option<&str> {
    let name = url.rsplit('/').next()?;
    name.split(['.', '?']).next().filter(|id| !id.is_empty())
}

/// The card a click on a sticker in a message opens: the sticker, its name as
/// the message names it, and the server it is from when one of this account's
/// lists it.
pub(super) fn sticker_card(app: &Application, item: &str, index: usize) -> MessageStickerCard {
    let Some(attachment) = app
        .state
        .message(item)
        .and_then(|message| message.attachments.get(index))
    else {
        return MessageStickerCard::default();
    };
    let url = attachment.url.as_deref().unwrap_or_default();
    // Which server lists it: Discord's packs are nobody's, and a message's
    // sticker names no server itself.
    let server = sticker_id(url)
        .and_then(|id| {
            app.state
                .stickers
                .servers
                .iter()
                .find(|server| server.stickers.iter().any(|sticker| sticker.id == id))
        })
        .and_then(|server| {
            let id = yapfast_core::discord::server_id(&server.id);
            app.state.servers.iter().find(|known| known.id == id)
        });
    // Its frames as the message plays them, and the still it shows until they
    // come, or for a still sticker.
    let still = attachment
        .preview
        .as_ref()
        .map(|picture| pictures::picture(picture));
    let (frames, delays) = attachment
        .preview
        .as_ref()
        .map_or_else(Default::default, pictures::animation);
    // What Discord said of the server, as the emoji card has it.
    let preview = server.and_then(|server| app.state.server_previews.get(&server.id));
    let discoverable = preview.map_or_else(
        || server.is_some_and(|server| server.community),
        |preview| preview.discoverable,
    );
    MessageStickerCard {
        name: attachment.name.as_str().into(),
        picture_url: url.into(),
        picture: still.unwrap_or_default(),
        frames,
        delays,
        server_online: preview
            .and_then(|preview| preview.online)
            .map_or_else(String::new, |online| {
                fill(tr("{} Online"), &[&grouped(online)])
            })
            .into(),
        server_note: if discoverable {
            tr("Discoverable Server")
        } else {
            ""
        }
        .into(),
        description: if server.is_some() {
            SERVER_STICKER
        } else {
            DEFAULT_STICKER
        }
        .into(),
        server: server
            .map(|server| server.id.clone())
            .unwrap_or_default()
            .into(),
        server_name: server
            .map(|server| server.name.clone())
            .unwrap_or_default()
            .into(),
        server_picture_url: server
            .and_then(|server| server.icon.clone())
            .unwrap_or_default()
            .into(),
        server_initial: server
            .map(|server| initial(&server.name))
            .unwrap_or_default(),
    }
}

/// The card an emoji opens, with the server the app knows it from: a custom
/// emoji's is the one that lists it.
pub(super) fn emoji_card_for(
    app: &Application,
    target: &emoji_text::Target,
    pictures: &mut HashMap<String, slint::Image>,
) -> EmojiCard {
    let server = target
        .custom()
        .and_then(|id| app.emoji_server(id))
        .and_then(|server| app.state.servers.iter().find(|known| known.id == server));
    emoji_card(target, server.map(|server| &**server), &app.state, pictures)
}
