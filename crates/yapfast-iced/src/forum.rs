//! A Discord forum in place of a channel's messages, as the Slint client's
//! `ForumPanel` (`ui/forum.slint`): a search of its posts' titles and New
//! Post, then its posts as cards (the active ones, then older ones a page
//! at a time as the list nears its end, or what a search found), a click
//! opening one beside the forum and Shift and a click in its place. New
//! Post shows the form for one (`NewPostForm`) in the list's place: its
//! title, its tags from a menu, the forum's guidelines, and its first
//! message with files, Post making it. The cards are built again only
//! when what they show changed, and go as the forum closes.

use crate::widgets::scroll_view;
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use iced::{
    Alignment, Element, Length, Padding, Size, Task,
    widget::{Space, button, column, container, row, stack, text},
};
use yapfast_core::{
    application::{Action, ForumInfo, ForumPost, POST_TAGS},
    chat::{ForumTag, Permission, ShownEmoji, Topic},
};

use crate::{
    app::{App, Message},
    composer::Writing,
    controls::{self, Kind},
    elide::elide,
    field::Draft,
    fonts::{BOLD, SEMIBOLD},
    markdown::{Body, FIRST_PART, Look},
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::{hex, rounded, scrollbar},
    theme::rgb,
    thread::SideComposer,
    widgets::Pic,
};

/// The new post's first message and title, and the forum's search.
pub const BODY: &str = "new-post-body";
pub const TITLE: &str = "new-post-title";
const SEARCH: &str = "forum-search";
/// The search box's height, as Slint's forum sets it.
const SEARCH_HEIGHT: f32 = 36.0;
/// A card with tags, and without; its picture; the tags it shows.
const CARD_TAGS: f32 = 120.0;
const CARD: f32 = 96.0;
const PICTURE: f32 = 72.0;
const SHOWN_TAGS: usize = 3;
const NEAR_END: f32 = 200.0;
const TAGS_MENU: f32 = 320.0;
const GUIDELINES: f32 = 220.0;

/// What the forum keeps while it shows: the form's first message as typed,
/// the guidelines laid out, the cards, the search.
#[derive(Default)]
pub struct Post {
    pub body: Draft,
    pub body_known: String,
    pub query: String,
    pub tags_menu: bool,
    pub guidelines_hidden: bool,
    guidelines: Option<(Arc<Topic>, Body)>,
    cards: Vec<Card>,
    shown: Option<u64>,
    /// What shows in place of the cards: searching, loading or an error;
    /// and whether there are none, and of a search.
    note: Option<String>,
    empty: bool,
    searched: bool,
}

/// A post as its card shows it, as Discord's client does.
struct Card {
    room: String,
    heading: Option<&'static str>,
    name: String,
    pinned: bool,
    new: bool,
    unread: bool,
    tags: Vec<(String, Option<ShownEmoji>)>,
    more_tags: usize,
    /// Who wrote its first message, in their role colour, and its text.
    person: Option<(String, Option<u32>)>,
    text: String,
    reaction: Option<(ShownEmoji, String)>,
    replies: u64,
    time: String,
    picture: Option<(String, bool)>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Query(String),
    Search,
    Scrolled(f32, f32, f32),
    Open(String),
    NewPost,
    Title(String),
    TagsMenu(bool),
    ToggleTag(String),
    Guidelines,
    Attach,
    Create,
    Close,
    Leave,
    Stay,
}

impl App {
    pub fn forum_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Query(query) => {
                let cleared = query.is_empty() && !self.forum_post.query.is_empty();
                self.forum_post.query = query;
                if cleared {
                    self.client.dispatch(Action::SearchPosts(String::new()));
                }
            }
            Event::Search => {
                let query = self.forum_post.query.trim().to_owned();
                self.client.dispatch(Action::SearchPosts(query));
            }
            Event::Scrolled(top, seen, whole) => {
                let cards = self.forum_post.cards.len();
                let more = self
                    .client
                    .state()
                    .forum()
                    .is_some_and(|forum| forum.wants_more(cards) && !forum.loading());
                // Only a page asked for changes what shows.
                if !(more && top > 0.0 && top + seen >= whole - NEAR_END) {
                    return Task::none();
                }
                self.client.dispatch(Action::MorePosts);
            }
            Event::Open(room) => self.client.dispatch(if self.modifiers.shift() {
                Action::Open(room)
            } else {
                Action::OpenAside(room)
            }),
            Event::NewPost => {
                self.client.dispatch(Action::NewPost);
                return Task::batch([self.sync(), iced::widget::operation::focus(TITLE)]);
            }
            Event::Title(title) => self.client.dispatch(Action::WriteNewPost {
                title: Some(title),
                body: None,
            }),
            Event::TagsMenu(open) => self.forum_post.tags_menu = open,
            Event::ToggleTag(tag) => self.client.dispatch(Action::TogglePostTag(tag)),
            Event::Guidelines => {
                self.forum_post.guidelines_hidden = !self.forum_post.guidelines_hidden
            }
            Event::Attach => return self.attach(),
            Event::Create => self.client.dispatch(Action::CreatePost),
            Event::Close => self.client.dispatch(Action::CloseNewPost),
            Event::Leave => self.client.dispatch(Action::LeaveNewPost),
            Event::Stay => self.client.dispatch(Action::StayInNewPost),
        }
        self.sync()
    }

    /// The post's first message as typed.
    pub fn forum_body(&mut self, body: String) {
        self.client.dispatch(Action::WriteNewPost {
            title: None,
            body: Some(body.clone()),
        });
        self.forum_post.body_known = body;
    }

    /// Takes in the forum's posts and the form, and lets go of them as the
    /// forum or the form closes.
    pub fn forum_sync(&mut self) {
        let state = self.client.state();
        match state.new_post() {
            Some(post) => {
                if post.body != self.forum_post.body_known {
                    if post.body != self.forum_post.body.text() {
                        self.forum_post.body.reset(&post.body);
                    }
                    self.forum_post.body_known = post.body.clone();
                }
                let topic = state
                    .topic
                    .as_ref()
                    .filter(|topic| !topic.body.trim().is_empty());
                let same = matches!(
                    (topic, &self.forum_post.guidelines),
                    (Some(topic), Some((shown, _))) if Arc::ptr_eq(topic, shown)
                );
                if !same {
                    let link = rgb(self.theme.mention);
                    self.forum_post.guidelines = topic.map(|topic| {
                        let body = crate::markdown::parse(
                            &topic.body,
                            &topic.mentions,
                            link,
                            "topic",
                            |_| true,
                        );
                        (topic.clone(), body)
                    });
                }
            }
            None => {
                if !self.forum_post.body_known.is_empty() || !self.forum_post.body.is_empty() {
                    self.forum_post.body.reset("");
                    self.forum_post.body_known.clear();
                }
                self.forum_post.tags_menu = false;
                self.forum_post.guidelines = None;
            }
        }
        let state = self.client.state();
        let Some(forum) = state.forum() else {
            if self.forum_post.shown.take().is_some() {
                self.forum_post.cards = Vec::new();
                self.forum_post.query.clear();
            }
            return;
        };
        let room = forum.list.room.clone();
        let posts = self.client.app.forum_posts().unwrap_or_default();
        let cards = posts.active.len() + posts.older.len();
        let (note, empty) = match &posts.found {
            Some(None) => (Some("Searching...".to_owned()), false),
            Some(Some(Err(error))) => (Some(error.clone()), false),
            Some(Some(Ok(found))) => (None, found.is_empty()),
            None if cards == 0 && forum.list.loading => (Some("Loading posts...".into()), false),
            None if cards == 0 => (forum.list.error.clone(), forum.list.error.is_none()),
            None => (None, false),
        };
        self.forum_post.note = note;
        self.forum_post.empty = empty;
        self.forum_post.searched = posts.found.is_some();
        let info = self.client.app.forum_info(&room).unwrap_or_default();
        let now = yapfast_core::clock::now_ms();
        let mut hasher = std::hash::DefaultHasher::new();
        (&posts, &room, now / 60_000).hash(&mut hasher);
        let shown = hasher.finish();
        if self.forum_post.shown == Some(shown) {
            return;
        }
        self.forum_post.shown = Some(shown);
        let card = |post| card(post, &info, now);
        self.forum_post.cards = match &posts.found {
            Some(Some(Ok(found))) => found.iter().map(card).collect(),
            _ => {
                let mut cards: Vec<Card> = posts.active.iter().map(card).collect();
                let older = posts
                    .older
                    .iter()
                    .map(card)
                    .enumerate()
                    .map(|(index, mut card)| {
                        if index == 0 {
                            card.heading = Some("OLDER POSTS");
                        }
                        card
                    });
                cards.extend(older);
                cards
            }
        };
    }

    /// The pictures the forum's cards show: first pictures, custom emoji of
    /// tags and reactions.
    pub fn forum_pictures(&self) -> Vec<&str> {
        let mut urls = Vec::new();
        for card in &self.forum_post.cards {
            urls.extend(
                card.picture
                    .as_ref()
                    .filter(|(_, spoiler)| !spoiler)
                    .map(|(url, _)| url.as_str()),
            );
            let emoji = card.tags.iter().filter_map(|(_, emoji)| emoji.as_ref());
            for emoji in emoji.chain(card.reaction.as_ref().map(|(emoji, _)| emoji)) {
                urls.extend(emoji.picture.as_deref());
            }
        }
        urls
    }

    /// The forum, in place of the timeline and composer.
    pub fn forum_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let forum = state.forum()?;
        if state.new_post().is_some() {
            return Some(self.new_post_view());
        }
        let theme = &self.theme;
        let app = &self.client.app;
        let searchable = app.may(&forum.list.room, Permission::ReadHistory);
        let can_post = app.can_post();
        let mut top = row![].spacing(8).align_y(Alignment::Center);
        if searchable {
            // The header's search box, across the room New Post leaves.
            top = top.push(
                self.ctx()
                    .search_field(
                        SEARCH,
                        "Search posts",
                        &self.forum_post.query,
                        |query| Message::Forum(Event::Query(query)),
                        Message::Forum(Event::Search),
                        Message::Forum(Event::Query(String::new())),
                    )
                    .width(Length::Fill)
                    .height(SEARCH_HEIGHT),
            );
        } else {
            top = top.push(Space::new().width(Length::Fill));
        }
        if can_post {
            top = top.push(
                controls::action(
                    theme,
                    "New Post",
                    Kind::Primary,
                    Some(Message::Forum(Event::NewPost)),
                )
                .icon(self.icon(Icon::Thread, 18, rgb(theme.text))),
            );
        }
        let mut page = column![];
        if searchable || can_post {
            page = page.push(container(top).padding(Padding::new(16.0).bottom(8)));
        }
        if let Some(note) = &self.forum_post.note {
            page = page.push(
                container(text(note).size(14).color(theme.muted).center())
                    .padding(24)
                    .center(Length::Fill),
            );
        } else if self.forum_post.empty {
            let words = if self.forum_post.searched {
                "No results found"
            } else {
                "There are no posts yet."
            };
            page = page.push(
                container(
                    column![
                        self.glyph(Icon::Thread, 48, theme.faint),
                        text(words).size(17).font(BOLD).color(theme.text),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(32)
                .center(Length::Fill),
            );
        } else {
            let mut list = column![];
            for (index, card) in self.forum_post.cards.iter().enumerate() {
                if let Some(heading) = card.heading {
                    let above = if index == 0 { 8.0 } else { 32.0 };
                    list = list.push(
                        container(elide(heading).size(12).font(BOLD).color(theme.muted))
                            .padding(Padding::ZERO.top(above).left(16).right(16))
                            .height(above + 24.0)
                            .align_y(Alignment::End),
                    );
                }
                list = list.push(self.post_card(card));
            }
            if forum.loading() {
                list = list.push(
                    container(text("Loading...").size(13).color(theme.muted))
                        .padding(8)
                        .center_x(Length::Fill),
                );
            }
            page = page.push(
                scroll_view(list)
                    .on_scroll(|viewport| {
                        Message::Forum(Event::Scrolled(
                            viewport.absolute_offset().y,
                            viewport.bounds().height,
                            viewport.content_bounds().height,
                        ))
                    })
                    .height(Length::Fill)
                    .style(scrollbar(theme)),
            );
        }
        Some(page.width(Length::Fill).height(Length::Fill).into())
    }

    /// A tag's or reaction's emoji: Twemoji's, or a custom one's picture.
    fn post_emoji(&self, emoji: &ShownEmoji, size: u32) -> Element<'_, Message> {
        let handle = match &emoji.picture {
            Some(url) => self.picture(url, size, size, false),
            None => {
                emojis::get(&emoji.text).and_then(|found| self.emoji_picture(found.as_str(), size))
            }
        };
        match handle {
            Some(handle) => Pic::square(handle, size as f32).into(),
            None => Space::new().width(size as f32).height(size as f32).into(),
        }
    }

    /// A tag on a card, as a pill.
    fn tag_pill<'a>(&'a self, name: &'a str, emoji: Option<&ShownEmoji>) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut pill = row![].spacing(4).align_y(Alignment::Center);
        if let Some(emoji) = emoji {
            pill = pill.push(self.post_emoji(emoji, 12));
        }
        pill = pill.push(text(name).size(12).font(SEMIBOLD).color(theme.secondary));
        let ground = theme.raised;
        container(pill)
            .padding([0, 8])
            .height(20)
            .align_y(Alignment::Center)
            .style(move |_| rounded(ground, 10))
            .into()
    }

    /// A post's card, as the Slint client's `PostCard`.
    fn post_card<'a>(&'a self, card: &'a Card) -> Element<'a, Message> {
        let theme = &self.theme;
        let has_tags = card.pinned || !card.tags.is_empty();
        let mut words = column![].spacing(6).width(Length::Fill);
        if has_tags {
            let mut tags = row![].spacing(6).align_y(Alignment::Center).height(20);
            if card.pinned {
                tags = tags.push(self.glyph(Icon::Pin, 14, theme.muted));
            }
            for (name, emoji) in &card.tags {
                tags = tags.push(self.tag_pill(name, emoji.as_ref()));
            }
            if card.more_tags > 0 {
                tags = tags.push(
                    text(format!("+{}", card.more_tags))
                        .size(12)
                        .font(SEMIBOLD)
                        .color(theme.muted),
                );
            }
            words = words.push(tags);
        }
        let ink = if card.unread || card.new {
            theme.text
        } else {
            theme.secondary
        };
        let mut title = row![elide(card.name.as_str()).size(16).font(BOLD).color(ink)]
            .spacing(8)
            .align_y(Alignment::Center);
        if card.new {
            let accent = theme.accent;
            title = title.push(
                container(text("NEW").size(10).font(BOLD).color(theme.on_accent))
                    .padding([0, 5])
                    .height(16)
                    .align_y(Alignment::Center)
                    .style(move |_| rounded(accent, 8)),
            );
        }
        words = words.push(container(title).width(Length::Fill));
        let mut said = row![].height(18).align_y(Alignment::Center);
        if let Some((person, color)) = &card.person {
            said = said.push(
                elide(person.as_str())
                    .size(14)
                    .font(SEMIBOLD)
                    .color(color.map_or(theme.secondary, hex))
                    .shrink(),
            );
        }
        let lead = if card.person.is_some() && !card.text.is_empty() {
            format!(": {}", card.text)
        } else {
            card.text.clone()
        };
        said = said.push(elide(lead).size(14).color(theme.muted));
        words = words.push(said);
        let mut foot = row![].spacing(8).height(22).align_y(Alignment::Center);
        if let Some((emoji, count)) = &card.reaction {
            let mut chip = row![self.post_emoji(emoji, 14)]
                .spacing(4)
                .align_y(Alignment::Center);
            if !count.is_empty() {
                chip = chip.push(
                    text(count.as_str())
                        .size(13)
                        .font(SEMIBOLD)
                        .color(theme.secondary),
                );
            }
            let (ground, edge) = (theme.raised, theme.line);
            foot = foot.push(
                container(chip)
                    .padding([0, 6])
                    .height(22)
                    .align_y(Alignment::Center)
                    .style(move |_| crate::style::outlined(ground, edge, 6.0)),
            );
        }
        foot = foot
            .push(self.glyph(Icon::MessageCircle, 14, theme.muted))
            .push(
                text(card.replies.to_string())
                    .size(13)
                    .font(SEMIBOLD)
                    .color(theme.muted),
            )
            .push(text("•").size(13).color(theme.faint))
            .push(text(card.time.as_str()).size(13).color(theme.muted));
        words = words.push(foot);
        let mut content = row![words].spacing(16).align_y(Alignment::Center);
        if let Some((url, spoiler)) = &card.picture {
            let ground = theme.raised;
            let handle = (!spoiler)
                .then(|| self.picture(url, PICTURE as u32, PICTURE as u32, true))
                .flatten();
            let picture: Element<'_, Message> = match handle {
                Some(handle) => Pic::new(handle, Size::new(PICTURE, PICTURE))
                    .radius(8.0)
                    .into(),
                None => {
                    let words: Element<'_, Message> = if *spoiler {
                        text("SPOILER")
                            .size(10)
                            .font(BOLD)
                            .color(theme.muted)
                            .into()
                    } else {
                        Space::new().into()
                    };
                    container(words)
                        .center(PICTURE)
                        .style(move |_| rounded(ground, 8))
                        .into()
                }
            };
            content = content.push(picture);
        }
        let (rest, lit, edge, edge_lit) = (theme.hover, theme.raised, theme.line, theme.active);
        let face = button(
            container(content)
                .padding(Padding::new(12.0).left(16))
                .height(if has_tags { CARD_TAGS } else { CARD })
                .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Forum(Event::Open(card.room.clone())))
        .style(crate::controls::outlined_lit_style(
            rest, lit, edge, edge_lit, 8.0,
        ));
        container(face)
            .padding(Padding::ZERO.left(16).right(16).bottom(8))
            .into()
    }

    /// The form for a post, as the Slint client's `NewPostForm`.
    fn new_post_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let state = self.client.state();
        let Some(post) = state.new_post() else {
            return Space::new().into();
        };
        let posting = post.posting;
        let head = self.panel_head(row![
            self.glyph(Icon::Thread, 18, theme.muted),
            container(text("New Post").size(15).font(BOLD).color(theme.text)).width(Length::Fill),
            self.look()
                .icon_button(Icon::X, "Close", Message::Forum(Event::Close))
                .below(),
        ]);
        let heading = |words: &'static str| text(words).size(12).font(BOLD).color(theme.muted);
        let title = controls::field(theme, TITLE, "Title", &post.title, false, !posting);
        let title = if posting {
            title
        } else {
            title
                .on_input(|title| Message::Forum(Event::Title(title)))
                .on_submit(Message::Focus(BODY))
        };
        let mut fields = column![column![heading("POST TITLE"), title].spacing(8)]
            .spacing(16)
            .padding(16);
        let info = app.forum_info(&post.room).unwrap_or_default();
        let choices = app.post_tags(&post.room);
        if !choices.is_empty() {
            let more = post.tags.len() < POST_TAGS;
            let mut chips = row![].spacing(6).align_y(Alignment::Center);
            for id in &post.tags {
                if let Some(tag) = choices.iter().find(|tag| &tag.id == id) {
                    chips = chips.push(self.chosen_tag(tag, posting));
                }
            }
            let add = controls::action(
                theme,
                "Add tags",
                Kind::Ghost,
                (!posting && more).then_some(Message::Forum(Event::TagsMenu(true))),
            )
            .icon(self.icon(Icon::CirclePlus, 16, rgb(theme.text)))
            .small();
            let menu = self
                .forum_post
                .tags_menu
                .then(|| self.tags_menu(&choices, &post.tags, more));
            chips = chips.push(
                Popover::new(add, menu, Side::Below)
                    .width(Width::Own(TAGS_MENU))
                    .gap(4.0)
                    .outside(Message::Forum(Event::TagsMenu(false))),
            );
            let label = if info.require_tag {
                "TAGS (REQUIRED)"
            } else {
                "TAGS"
            };
            fields = fields.push(column![heading(label), chips].spacing(8));
        }
        if let Some((_, guidelines)) = &self.forum_post.guidelines {
            fields = fields.push(self.guidelines_view(guidelines));
        }
        let mut composer = SideComposer::new(Writing::NewPost, "Enter a message...".into())
            .enabled(!posting)
            .attach((state.can_attach() && !posting).then_some(Message::Forum(Event::Attach)));
        if let Some(tray) = self.tray() {
            composer = composer.tray(tray);
        }
        fields = fields.push(composer.view(self));
        if let Some(error) = &post.error {
            fields = fields.push(crate::form::error(theme, error.as_str()));
        }
        let label = if posting { "Posting..." } else { "Post" };
        fields = fields.push(
            container(controls::action(
                theme,
                label,
                Kind::Primary,
                (!posting).then_some(Message::Forum(Event::Create)),
            ))
            .align_right(Length::Fill),
        );
        let form: Element<'_, Message> = column![
            head,
            scroll_view(fields)
                .height(Length::Fill)
                .style(scrollbar(theme)),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        if post.leaving {
            stack![
                form,
                self.leave_question(Message::Forum(Event::Stay), Message::Forum(Event::Leave))
            ]
            .into()
        } else {
            form
        }
    }

    /// Lets go of the guidelines, laid out in the old theme's inks.
    pub fn forget_guidelines(&mut self) {
        self.forum_post.guidelines = None;
    }

    /// The forum's post guidelines, folding away.
    fn guidelines_view<'a>(&'a self, guidelines: &'a Body) -> Element<'a, Message> {
        let theme = &self.theme;
        let shown = !self.forum_post.guidelines_hidden;
        let (icon, label) = if shown {
            (Icon::ChevronDown, "Hide post guidelines")
        } else {
            (Icon::ChevronRight, "Show post guidelines")
        };
        let mut guide = column![
            row![
                container(
                    text("Post Guidelines")
                        .size(14)
                        .font(BOLD)
                        .color(theme.text)
                )
                .width(Length::Fill),
                self.look()
                    .icon_button(icon, label, Message::Forum(Event::Guidelines))
                    .size(24.0, 16)
                    .below(),
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(8);
        if shown {
            let body = self.body_view(
                guidelines,
                usize::MAX,
                FIRST_PART,
                Look {
                    size: 14.0,
                    line_height: 20.0,
                    ink: theme.secondary,
                },
            );
            guide = guide
                .push(container(scroll_view(body).style(scrollbar(theme))).max_height(GUIDELINES));
        }
        let (ground, edge) = (theme.sidebar, theme.line);
        container(guide)
            .padding(12)
            .width(Length::Fill)
            .style(move |_| crate::style::outlined(ground, edge, 8.0))
            .into()
    }

    /// A tag chosen for the post: a click takes it away.
    fn chosen_tag(&self, tag: &ForumTag, posting: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let mut chip = row![].spacing(4).align_y(Alignment::Center);
        if let Some(emoji) = &tag.emoji {
            chip = chip.push(self.post_emoji(emoji, 14));
        }
        chip = chip
            .push(
                text(tag.name.clone())
                    .size(13)
                    .font(SEMIBOLD)
                    .color(theme.text),
            )
            .push(self.glyph(Icon::X, 14, theme.muted));
        button(
            container(chip)
                .padding(Padding::ZERO.left(10).right(6))
                .height(28)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press_maybe((!posting).then(|| Message::Forum(Event::ToggleTag(tag.id.clone()))))
        .style(controls::lit_style(Some(theme.raised), theme.active, 14.0))
        .into()
    }

    /// The menu of the forum's tags, each with its box.
    fn tags_menu<'a>(
        &'a self,
        choices: &[ForumTag],
        chosen: &[String],
        more: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut list = column![
            container(
                text("Select up to 5 tags")
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted)
            )
            .padding(Padding::ZERO.left(8))
            .height(28)
            .align_y(Alignment::Center)
        ];
        for tag in choices {
            let ticked = chosen.contains(&tag.id);
            let enabled = ticked || more;
            let mut line = row![].spacing(8).align_y(Alignment::Center);
            if let Some(emoji) = &tag.emoji {
                line = line.push(self.post_emoji(emoji, 16));
            }
            let ink = if enabled { theme.text } else { theme.faint };
            line = line
                .push(container(text(tag.name.clone()).size(14).color(ink)).width(Length::Fill))
                .push(self.checkbox(ticked, false));
            list = list.push(
                button(
                    container(line)
                        .padding([0, 8])
                        .height(36)
                        .align_y(Alignment::Center),
                )
                .padding(0)
                .width(Length::Fill)
                .on_press_maybe(enabled.then(|| Message::Forum(Event::ToggleTag(tag.id.clone()))))
                .style(controls::lit_style(None, theme.hover, 4.0)),
            );
        }
        container(list.padding(6))
            .style(controls::menu_panel(theme))
            .into()
    }
}

/// A post's card from what the core lists of it.
fn card(post: &ForumPost, info: &ForumInfo, now: u64) -> Card {
    let thread = &post.thread;
    let starter = post.starter.as_ref();
    let first = starter.and_then(Option::as_ref);
    let (person, words) = match starter {
        Some(Some(first)) => (Some(&first.author), first.text.as_str()),
        Some(None) => (None, "Original message was deleted"),
        None => (None, ""),
    };
    let reaction = match first.and_then(|first| first.reaction.as_ref()) {
        Some((emoji, count)) => Some((emoji.clone(), count.to_string())),
        // A post without reactions offers the forum's own.
        None if first.is_some() => info.reaction.clone().map(|emoji| (emoji, String::new())),
        None => None,
    };
    let at = if info.by_creation {
        thread.created_ms
    } else {
        thread.active_ms
    };
    Card {
        room: thread.room.clone(),
        heading: None,
        name: thread.name.clone(),
        pinned: thread.pinned,
        new: post.new,
        unread: thread.unread,
        tags: post
            .tags
            .iter()
            .take(SHOWN_TAGS)
            .map(|tag| (tag.name.clone(), tag.emoji.clone()))
            .collect(),
        more_tags: post.tags.len().saturating_sub(SHOWN_TAGS),
        person: person.map(|person| (person.name.clone(), person.color)),
        text: words.to_owned(),
        reaction,
        replies: thread.replies,
        time: crate::popovers::ago(at, now),
        picture: first.and_then(|first| first.picture.clone().map(|url| (url, first.spoiler))),
    }
}

#[cfg(test)]
mod tests {
    use yapfast_core::chat::{ForumTag, ListedThread, PostStarter, ShownEmoji, ThreadPerson};

    use super::*;

    fn tag(name: &str) -> ForumTag {
        ForumTag {
            id: name.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_posts_card_reads_as_discords() {
        let now = 10 * 86_400_000;
        let forum_reaction = ShownEmoji {
            text: "👍".into(),
            picture: None,
        };
        let info = ForumInfo {
            reaction: Some(forum_reaction.clone()),
            ..Default::default()
        };
        let mut post = ForumPost {
            thread: ListedThread {
                room: "p".into(),
                name: "Idea".into(),
                active_ms: now - 120_000,
                replies: 3,
                ..Default::default()
            },
            tags: ["a", "b", "c", "d", "e"].map(tag).to_vec(),
            starter: Some(Some(PostStarter {
                author: ThreadPerson {
                    name: "Carol".into(),
                    ..Default::default()
                },
                text: "Hello".into(),
                ..Default::default()
            })),
            new: true,
        };
        let shown = card(&post, &info, now);
        assert_eq!(shown.tags.len(), SHOWN_TAGS);
        assert_eq!(shown.more_tags, 2);
        assert_eq!(shown.time, "2m ago");
        // Without reactions, the forum's own, without a count.
        assert_eq!(shown.reaction, Some((forum_reaction, String::new())));
        assert_eq!(shown.person.map(|(name, _)| name).as_deref(), Some("Carol"));
        post.starter = Some(None);
        let deleted = card(&post, &info, now);
        assert_eq!(deleted.text, "Original message was deleted");
        assert!(deleted.person.is_none() && deleted.reaction.is_none());
    }
}
