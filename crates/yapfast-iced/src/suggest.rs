//! Suggestions over the field being written in, as the Slint client's
//! `MentionList` and `EmojiSuggestions`: the people, roles or channels an
//! `@` or `#` word may mention (the core's `mention_suggestions`), or the
//! emoji a `:name` word may be (`yapfast_core::emoji::suggestions`). Up and
//! Down light one, Tab, Enter or a click takes it, Escape closes them until
//! the word is left. They follow the field typed in last: the composer or a
//! message's edit.

use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, text},
};
use std::sync::Arc;

use yapfast_core::{
    application::{Action, AppState},
    chat::{ChatMember, ChatMessage},
    emoji::{self, Found},
    mention,
};

use crate::{
    app::{App, Message},
    client::Client,
    composer::{Event, Writing},
    controls::lit_style,
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::{faded, hex},
    widgets::Pic,
};

/// The most emoji a `:name` suggests, as the Slint client's.
const EMOJI_SUGGESTIONS: usize = 10;
/// A suggestion's row, and its heading's.
const ROW: f32 = 36.0;
const HEADING: f32 = 24.0;
/// A suggested emoji's picture, in its slot.
const EMOJI: u32 = 20;

/// What the suggestions are for, and which is lit.
#[derive(Default)]
pub struct Suggest {
    /// The field typed in last.
    pub field: Writing,
    /// Where the `@` or `#` word starts, and the one whose suggestions
    /// Escape closed.
    mention_at: Option<usize>,
    mention_dismissed: Option<usize>,
    /// The `:name` word: where its colon is, and the name; the one Escape
    /// closed; the emoji it suggests.
    emoji_word: Option<(usize, String)>,
    emoji_dismissed: Option<usize>,
    pub emoji: Vec<Found>,
    /// Each one's picture's address, for a custom emoji.
    emoji_pictures: Vec<Option<String>>,
    /// The core's mention suggestions for the `@` or `#` word, and what
    /// they were worked out from: every member and author, so once per
    /// change, not per view.
    mentions: Vec<mention::Candidate>,
    mentions_for: Option<MentionsFrom>,
    /// The suggestion lit.
    index: usize,
}

/// What the core's mention suggestions come from: the word, the room,
/// its members and those found by name, its authors, its roles and
/// groups. The lists are held, so one let go can't be mistaken for a new
/// one made where it was.
struct MentionsFrom {
    query: Option<(char, String)>,
    selected: Option<String>,
    members: Option<Arc<Vec<ChatMember>>>,
    messages: Arc<Vec<Arc<ChatMessage>>>,
    found: u64,
}

impl MentionsFrom {
    fn of(state: &AppState) -> Self {
        use std::hash::{Hash, Hasher};
        let mut found = std::hash::DefaultHasher::new();
        for member in &state.found_members {
            (&member.id, &member.name, &member.avatar).hash(&mut found);
        }
        for group in &state.mention_groups {
            (&group.name, &group.mention).hash(&mut found);
        }
        Self {
            query: state.mention_query.clone(),
            selected: state.selected.clone(),
            members: state
                .members
                .as_ref()
                .and_then(|members| members.as_ref().ok())
                .cloned(),
            messages: state.messages.clone(),
            found: found.finish(),
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.query == other.query
            && self.selected == other.selected
            && self.found == other.found
            && Arc::ptr_eq(&self.messages, &other.messages)
            && match (&self.members, &other.members) {
                (Some(this), Some(that)) => Arc::ptr_eq(this, that),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Suggest {
    /// The light moves `step` rows among `count`, going round both ways,
    /// as the Slint client's.
    pub fn step(&mut self, step: i32, count: usize) {
        if count == 0 {
            return;
        }
        let count = count as i64;
        self.index = (self.lit(count as usize) as i64 + step as i64).rem_euclid(count) as usize;
    }

    pub fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    /// Sending or switching rooms closes them all.
    pub fn clear(&mut self, client: &mut Client) {
        self.mention_at = None;
        self.mentions.clear();
        self.mentions_for = None;
        self.emoji_word = None;
        self.emoji.clear();
        self.emoji_pictures.clear();
        if client.state().mention_query.is_some() {
            client.dispatch(Action::MentionQuery(None));
        }
    }

    /// The lit one of `count`, the arrows going round.
    fn lit(&self, count: usize) -> usize {
        if count == 0 { 0 } else { self.index % count }
    }
}

/// One list's rows: an app's commands or an option's values, mentions, or
/// emoji.
enum Shown<'a> {
    Commands(Vec<yapfast_core::application::CommandRow>, String),
    Mentions(&'a [mention::Candidate]),
    Emoji,
}

impl App {
    /// How many suggestions show, for the arrows to go round.
    pub fn suggestion_count(&self) -> usize {
        match self.ctx().shown_suggestions() {
            Some(Shown::Commands(rows, _)) => rows.len(),
            Some(Shown::Mentions(found)) => found.len(),
            Some(Shown::Emoji) => self.composer.suggest.emoji.len(),
            None => 0,
        }
    }

    /// The caret moved in `which`: the `@`, `#` or `:name` word at it, as
    /// the Slint client's `composer_caret` finds them, unless Escape closed
    /// its suggestions.
    pub fn caret_moved(&mut self, which: Writing, text: &str, caret: usize) {
        let suggest = &mut self.composer.suggest;
        if suggest.field != which {
            suggest.field = which;
            suggest.mention_dismissed = None;
            suggest.emoji_dismissed = None;
        }
        let word = emoji::query(text, caret);
        if word.is_none() {
            suggest.emoji_dismissed = None;
        }
        let word = word
            .filter(|(at, _)| suggest.emoji_dismissed != Some(*at))
            .map(|(at, name)| (at, name.to_owned()));
        if suggest.emoji_word != word {
            suggest.index = 0;
            suggest.emoji.clear();
            suggest.emoji_pictures.clear();
            if let Some((_, name)) = &word {
                let sets = self.client.app.usable_emoji();
                suggest.emoji =
                    emoji::suggestions(name, &sets, crate::picker::drawn, EMOJI_SUGGESTIONS);
                suggest.emoji_pictures = suggest
                    .emoji
                    .iter()
                    .map(|found| {
                        let (_, id) = found.key.split_once(':')?;
                        sets.iter()
                            .flat_map(|set| &set.emoji)
                            .find(|emoji| emoji.id == id)
                            .map(|emoji| emoji.picture.clone())
                    })
                    .collect();
            }
            suggest.emoji_word = word;
        }
        // An app command's option offers its own people, roles and
        // channels.
        let commanding = which == Writing::Draft && self.client.state().command_draft().is_some();
        let found = mention::query(text, caret).filter(|_| !commanding);
        if found.is_none() {
            suggest.mention_dismissed = None;
        }
        let found = found.filter(|(at, ..)| suggest.mention_dismissed != Some(*at));
        suggest.mention_at = found.map(|(at, ..)| at);
        let query = found.map(|(_, sigil, query)| (sigil, query.to_owned()));
        if self.client.state().mention_query != query {
            self.composer.suggest.index = 0;
            self.client.dispatch(Action::MentionQuery(query));
        }
        self.mentions_sync();
    }

    /// Works the mention suggestions out again when what they come from
    /// changed; on each sync and as the caret moves.
    pub fn mentions_sync(&mut self) {
        let state = self.client.state();
        if self.composer.suggest.mention_at.is_none() || state.mention_query.is_none() {
            self.composer.suggest.mentions.clear();
            self.composer.suggest.mentions_for = None;
            return;
        }
        let from = MentionsFrom::of(state);
        if self
            .composer
            .suggest
            .mentions_for
            .as_ref()
            .is_some_and(|known| known.same(&from))
        {
            return;
        }
        self.composer.suggest.mentions = state.mention_suggestions();
        self.composer.suggest.mentions_for = Some(from);
    }

    /// Escape with suggestions open closes them until their word is left;
    /// whether it did.
    pub fn dismiss_suggestions(&mut self) -> bool {
        match self.ctx().shown_suggestions() {
            Some(Shown::Commands(..)) => self.close_command(false),
            Some(Shown::Mentions(_)) => {
                self.composer.suggest.mention_dismissed = self.composer.suggest.mention_at.take();
                self.client.dispatch(Action::MentionQuery(None));
                self.mentions_sync();
                true
            }
            Some(Shown::Emoji) => {
                self.composer.suggest.emoji_dismissed =
                    self.composer.suggest.emoji_word.take().map(|(at, _)| at);
                self.composer.suggest.emoji.clear();
                self.composer.suggest.emoji_pictures.clear();
                true
            }
            None => false,
        }
    }

    /// The lit suggestion goes in place of the word typed, with a space
    /// after it, as the Slint client's `mention-chosen` and
    /// `emoji-suggestion-chosen` write them.
    pub fn choose_suggestion(&mut self, which: Writing) -> Task<Message> {
        let text = self.ctx().writing(which).text();
        let (at, end, inserted) = match self.ctx().shown_suggestions() {
            Some(Shown::Commands(rows, _)) => {
                if rows.is_empty() {
                    return Task::none();
                }
                let lit = self.composer.suggest.lit(rows.len());
                self.composer.suggest.set_index(0);
                self.client.dispatch(Action::PickCommand(lit));
                return self.sync();
            }
            Some(Shown::Mentions(found)) => {
                let state = self.client.state();
                let (Some(at), Some((sigil, query))) = (
                    self.composer.suggest.mention_at,
                    state.mention_query.clone(),
                ) else {
                    return Task::none();
                };
                let chosen = found[self.composer.suggest.lit(found.len())].clone();
                let end = at + 1 + query.len();
                if text.get(at..end) != Some(format!("{sigil}{query}").as_str()) {
                    return Task::none();
                }
                self.composer.suggest.mention_at = None;
                self.client.dispatch(Action::MentionQuery(None));
                self.mentions_sync();
                (at, end, format!("{}{} ", chosen.sigil(), chosen.name))
            }
            Some(Shown::Emoji) => {
                let Some((at, name)) = self.composer.suggest.emoji_word.take() else {
                    return Task::none();
                };
                let chosen = self.composer.suggest.emoji
                    [self.composer.suggest.lit(self.composer.suggest.emoji.len())]
                .clone();
                self.composer.suggest.emoji.clear();
                self.composer.suggest.emoji_pictures.clear();
                let end = at + 1 + name.len();
                if text.get(at..end) != Some(format!(":{name}").as_str()) {
                    return Task::none();
                }
                (at, end, format!("{} ", emoji::composer_text(&chosen.key)))
            }
            None => return Task::none(),
        };
        let text = format!("{}{inserted}{}", &text[..at], &text[end..]);
        self.replace_text(which, text, at + inserted.len());
        Task::none()
    }

    /// The pictures the suggestions show, to ask for.
    pub fn suggestion_pictures(&self) -> Vec<String> {
        let mut urls = Vec::new();
        match self.ctx().shown_suggestions() {
            Some(Shown::Commands(..)) => urls.extend(self.command_pictures()),
            Some(Shown::Mentions(found)) => {
                urls.extend(
                    found
                        .iter()
                        .filter_map(|candidate| candidate.avatar.clone()),
                );
            }
            Some(Shown::Emoji) => urls.extend(
                self.composer
                    .suggest
                    .emoji_pictures
                    .iter()
                    .flatten()
                    .cloned(),
            ),
            None => {}
        }
        urls
    }
}

/// A list's heading, as the Slint client's `SuggestionHeading`.
fn heading<'a>(theme: &crate::theme::Theme, label: String) -> Element<'a, Message> {
    container(text(label).size(11).font(BOLD).color(theme.muted))
        .padding([0, 6])
        .height(HEADING)
        .align_y(Alignment::Center)
        .into()
}

impl<'a> crate::ctx::Ctx<'a> {
    /// An emoji as a button `side` across, its picture `glyph` across (its
    /// text where there's no picture), on `rest` and `lit` under the
    /// pointer: the menu's quick reactions, the hover bar's.
    pub fn emoji_button(
        self,
        key: &str,
        glyph: u32,
        side: f32,
        (rest, lit): (Option<iced::Color>, iced::Color),
        press: Message,
    ) -> Element<'a, Message> {
        button(container(self.emoji_face(key, glyph)).center(side))
            .padding(0)
            .width(side)
            .height(side)
            .style(lit_style(rest, lit, 6.0))
            .on_press(press)
            .into()
    }

    /// An emoji's picture by its key: Twemoji's, or a custom emoji's once
    /// it has come (a quiet square until then).
    pub fn emoji_face(self, key: &str, size: u32) -> Element<'a, Message> {
        let side = size as f32;
        let handle = match key.split_once(':') {
            Some((_, id)) => self
                .custom_emoji(id)
                .and_then(|url| self.picture(&url, size, size, false)),
            None => emojis::get(key).and_then(|emoji| self.emoji_picture(emoji.as_str(), size)),
        };
        match handle {
            Some(handle) => Pic::square(handle, side).into(),
            None => Space::new().width(side).height(side).into(),
        }
    }

    /// A custom emoji's picture's address, by its id, among those this
    /// account has.
    pub fn custom_emoji(self, id: &str) -> Option<String> {
        let ids = std::collections::HashSet::from([id]);
        self.app
            .emoji_by_id(&ids)
            .into_iter()
            .next()
            .map(|emoji| emoji.picture)
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// Whether suggestions over `which` take its arrow keys.
    pub fn suggesting(self, which: Writing) -> bool {
        self.composer.suggest.field == which
            && match self.shown_suggestions() {
                Some(Shown::Commands(rows, _)) => !rows.is_empty(),
                shown => shown.is_some(),
            }
    }

    /// The suggestions over `which`, as wide as it, in the Slint client's
    /// `SuggestionFrame`; none while they're for the other field.
    pub fn suggestions_view(self, which: Writing) -> Option<Element<'a, Message>> {
        if self.composer.suggest.field != which {
            return None;
        }
        let theme = self.look.theme;
        let mut list = column![].padding(6);
        match self.shown_suggestions()? {
            Shown::Commands(rows, heading) => {
                list =
                    self.command_list_view(&rows, heading, self.composer.suggest.lit(rows.len()));
            }
            Shown::Mentions(found) => {
                let lit = self.composer.suggest.lit(found.len());
                for (index, candidate) in found.iter().enumerate() {
                    if index == 0 || found[index - 1].group != candidate.group {
                        list = list.push(heading(
                            theme,
                            match (candidate.group, candidate.channel) {
                                (false, _) => "MEMBERS".to_owned(),
                                (true, false) => "ROLES".to_owned(),
                                (true, true) => "CHANNELS".to_owned(),
                            },
                        ));
                    }
                    list = list.push(self.mention_row(index, candidate, index == lit));
                }
            }
            Shown::Emoji => {
                let query = self
                    .composer
                    .suggest
                    .emoji_word
                    .as_ref()
                    .map(|(_, name)| name.as_str())
                    .unwrap_or_default();
                list = list.push(heading(theme, format!("EMOJI MATCHING :{query}")));
                let lit = self.composer.suggest.lit(self.composer.suggest.emoji.len());
                let pictures = &self.composer.suggest.emoji_pictures;
                for (index, found) in self.composer.suggest.emoji.iter().enumerate() {
                    let face = match pictures.get(index).and_then(Option::as_deref) {
                        Some(url) => self.picture_face(url, EMOJI),
                        None => self.emoji_face(&found.key, EMOJI),
                    };
                    let face = container(face).width(24).center_x(24);
                    list = list.push(self.suggestion_row(
                        index,
                        row![
                            face,
                            text(found.name.clone())
                                .size(14)
                                .font(SEMIBOLD)
                                .color(theme.text)
                                .wrapping(text::Wrapping::None),
                        ],
                        index == lit,
                    ));
                }
            }
        }
        let (ground, edge) = (theme.raised, theme.edge);
        Some(
            container(list)
                .width(Length::Fill)
                .style(move |_| crate::style::panel(ground, edge, 8.0))
                .into(),
        )
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    fn shown_suggestions(self) -> Option<Shown<'a>> {
        let suggest = &self.composer.suggest;
        // The `/` list over the composer comes first, and with a command
        // being written, it's the only one.
        if suggest.field == Writing::Draft {
            if let Some((rows, heading)) = self.command_list() {
                return Some(Shown::Commands(rows, heading));
            }
            if self.state.command_draft().is_some() {
                return None;
            }
        }
        if suggest.mention_at.is_some() && !suggest.mentions.is_empty() {
            return Some(Shown::Mentions(&suggest.mentions));
        }
        (suggest.emoji_word.is_some() && !suggest.emoji.is_empty()).then_some(Shown::Emoji)
    }

    fn mention_row(
        self,
        index: usize,
        candidate: &mention::Candidate,
        lit: bool,
    ) -> Element<'a, Message> {
        let theme = self.look.theme;
        let color = candidate.color.map(hex);
        let mark: Element<'a, Message> = if candidate.group {
            let ink = color.unwrap_or(theme.secondary);
            let ground = faded(color.unwrap_or(theme.muted), 0.2);
            let face: Element<'a, Message> = if candidate.voice {
                self.look.glyph(Icon::Volume, 14, theme.secondary).into()
            } else {
                text(candidate.sigil().to_string())
                    .size(13)
                    .font(BOLD)
                    .color(ink)
                    .into()
            };
            container(face)
                .center(24)
                .style(move |_| crate::style::rounded(ground, 12))
                .into()
        } else {
            self.avatar(
                candidate.avatar.as_deref(),
                &candidate.name,
                24.0,
                12.0,
                1.0,
            )
        };
        let detail = match candidate.mention.as_str() {
            "@everyone" => "Everyone in this channel".to_owned(),
            "@here" => "Everyone online here".to_owned(),
            _ => candidate.username.clone().unwrap_or_default(),
        };
        self.suggestion_row(
            index,
            row![
                mark,
                text(candidate.name.clone())
                    .size(14)
                    .font(SEMIBOLD)
                    .color(color.unwrap_or(theme.text))
                    .wrapping(text::Wrapping::None),
                text(detail)
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ],
            lit,
        )
    }

    /// A row of the list: lit by the keys, or lighter under the pointer.
    fn suggestion_row(
        self,
        index: usize,
        face: iced::widget::Row<'a, Message>,
        lit: bool,
    ) -> Element<'a, Message> {
        let (active, hover) = (self.look.theme.active, self.look.theme.hover);
        button(face.spacing(10).align_y(Alignment::Center))
            .padding(Padding::ZERO.left(8).right(10))
            .width(Length::Fill)
            .height(ROW)
            .clip(true)
            .on_press(Message::Write(Event::Suggestion(index)))
            .style(lit_style(
                lit.then_some(active),
                if lit { active } else { hover },
                6.0,
            ))
            .into()
    }

    /// A picture by its address, `size` square once it has come (a quiet
    /// square until then).
    fn picture_face(self, url: &str, size: u32) -> Element<'a, Message> {
        let side = size as f32;
        match self.picture(url, size, size, false) {
            Some(handle) => Pic::square(handle, side).into(),
            None => Space::new().width(side).height(side).into(),
        }
    }
}
