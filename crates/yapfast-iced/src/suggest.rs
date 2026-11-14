//! Suggestions over the field being written in, as the Slint client's
//! `MentionList` and `EmojiSuggestions`: the people, roles or channels an
//! `@` or `#` word may mention (the core's `mention_suggestions`), or the
//! emoji a `:name` word may be (`yapfast_core::emoji::suggestions`). Up and
//! Down light one, Tab, Enter or a click takes it, Escape closes them until
//! the word is left. They follow the field typed in last: the composer or a
//! message's edit.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::{
    application::Action,
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
    style::{faded, hex, raised_shadow},
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
    /// The suggestion lit.
    index: usize,
}

impl Suggest {
    pub fn step(&mut self, step: i32) {
        self.index = (self.index as i32 + step).max(0) as usize;
    }

    pub fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    /// Sending or switching rooms closes them all.
    pub fn clear(&mut self, client: &mut Client) {
        self.mention_at = None;
        self.emoji_word = None;
        self.emoji.clear();
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
enum Shown {
    Commands(Vec<yapfast_core::application::CommandRow>, String),
    Mentions(Vec<mention::Candidate>),
    Emoji,
}

impl App {
    /// Whether suggestions over `which` take its arrow keys.
    pub fn suggesting(&self, which: Writing) -> bool {
        self.suggest.field == which
            && match self.shown_suggestions() {
                Some(Shown::Commands(rows, _)) => !rows.is_empty(),
                shown => shown.is_some(),
            }
    }

    fn shown_suggestions(&self) -> Option<Shown> {
        let suggest = &self.suggest;
        // The `/` list over the composer comes first, and with a command
        // being written, it's the only one.
        if suggest.field == Writing::Draft {
            if let Some((rows, heading)) = self.command_list() {
                return Some(Shown::Commands(rows, heading));
            }
            if self.client.state().command_draft().is_some() {
                return None;
            }
        }
        if suggest.mention_at.is_some() {
            let found = self.client.state().mention_suggestions();
            if !found.is_empty() {
                return Some(Shown::Mentions(found));
            }
        }
        (suggest.emoji_word.is_some() && !suggest.emoji.is_empty()).then_some(Shown::Emoji)
    }

    /// The caret moved in `which`: the `@`, `#` or `:name` word at it, as
    /// the Slint client's `composer_caret` finds them, unless Escape closed
    /// its suggestions.
    pub fn caret_moved(&mut self, which: Writing, text: &str, caret: usize) {
        let suggest = &mut self.suggest;
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
            suggest.emoji = match &word {
                Some((_, name)) => emoji::suggestions(
                    name,
                    &self.client.app.usable_emoji(),
                    crate::picker::drawn,
                    EMOJI_SUGGESTIONS,
                ),
                None => Vec::new(),
            };
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
            self.suggest.index = 0;
            self.client.dispatch(Action::MentionQuery(query));
        }
    }

    /// Escape with suggestions open closes them until their word is left;
    /// whether it did.
    pub fn dismiss_suggestions(&mut self) -> bool {
        match self.shown_suggestions() {
            Some(Shown::Commands(..)) => self.close_command(false),
            Some(Shown::Mentions(_)) => {
                self.suggest.mention_dismissed = self.suggest.mention_at.take();
                self.client.dispatch(Action::MentionQuery(None));
                true
            }
            Some(Shown::Emoji) => {
                self.suggest.emoji_dismissed = self.suggest.emoji_word.take().map(|(at, _)| at);
                self.suggest.emoji.clear();
                true
            }
            None => false,
        }
    }

    /// The lit suggestion goes in place of the word typed, with a space
    /// after it, as the Slint client's `mention-chosen` and
    /// `emoji-suggestion-chosen` write them.
    pub fn choose_suggestion(&mut self, which: Writing) -> Task<Message> {
        let text = self.writing(which).text();
        let (at, end, inserted) = match self.shown_suggestions() {
            Some(Shown::Commands(rows, _)) => {
                if rows.is_empty() {
                    return Task::none();
                }
                let lit = self.suggest.lit(rows.len());
                self.suggest.set_index(0);
                self.client.dispatch(Action::PickCommand(lit));
                return self.sync();
            }
            Some(Shown::Mentions(found)) => {
                let state = self.client.state();
                let (Some(at), Some((sigil, query))) =
                    (self.suggest.mention_at, state.mention_query.clone())
                else {
                    return Task::none();
                };
                let chosen = &found[self.suggest.lit(found.len())];
                let end = at + 1 + query.len();
                if text.get(at..end) != Some(format!("{sigil}{query}").as_str()) {
                    return Task::none();
                }
                self.suggest.mention_at = None;
                self.client.dispatch(Action::MentionQuery(None));
                (at, end, format!("{}{} ", chosen.sigil(), chosen.name))
            }
            Some(Shown::Emoji) => {
                let Some((at, name)) = self.suggest.emoji_word.take() else {
                    return Task::none();
                };
                let chosen = self.suggest.emoji[self.suggest.lit(self.suggest.emoji.len())].clone();
                self.suggest.emoji.clear();
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

    /// The suggestions over `which`, as wide as it, in the Slint client's
    /// `SuggestionFrame`; none while they're for the other field.
    pub fn suggestions_view(&self, which: Writing) -> Option<Element<'_, Message>> {
        if self.suggest.field != which {
            return None;
        }
        let theme = &self.theme;
        let mut list = column![].padding(6);
        match self.shown_suggestions()? {
            Shown::Commands(rows, heading) => {
                list = self.command_list_view(&rows, heading, self.suggest.lit(rows.len()));
            }
            Shown::Mentions(found) => {
                let lit = self.suggest.lit(found.len());
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
                    .suggest
                    .emoji_word
                    .as_ref()
                    .map(|(_, name)| name.as_str())
                    .unwrap_or_default();
                list = list.push(heading(theme, format!("EMOJI MATCHING :{query}")));
                let lit = self.suggest.lit(self.suggest.emoji.len());
                for (index, found) in self.suggest.emoji.iter().enumerate() {
                    let face = self.emoji_face(&found.key, EMOJI);
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
                .style(move |_| container::Style {
                    background: Some(ground.into()),
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    shadow: raised_shadow(),
                    ..Default::default()
                })
                .into(),
        )
    }

    fn mention_row<'a>(
        &'a self,
        index: usize,
        candidate: &mention::Candidate,
        lit: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let color = candidate.color.map(hex);
        let mark: Element<'_, Message> = if candidate.group {
            let ink = color.unwrap_or(theme.secondary);
            let ground = faded(color.unwrap_or(theme.muted), 0.2);
            let face: Element<'_, Message> = if candidate.voice {
                self.glyph(Icon::Volume, 14, theme.secondary).into()
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
    fn suggestion_row<'a>(
        &self,
        index: usize,
        face: iced::widget::Row<'a, Message>,
        lit: bool,
    ) -> Element<'a, Message> {
        let (active, hover) = (self.theme.active, self.theme.hover);
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

    /// An emoji's picture by its key: Twemoji's, or a custom emoji's once
    /// it has come (a quiet square until then).
    pub fn emoji_face(&self, key: &str, size: u32) -> Element<'_, Message> {
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
    pub fn custom_emoji(&self, id: &str) -> Option<String> {
        let ids = std::collections::HashSet::from([id]);
        self.client
            .app
            .emoji_by_id(&ids)
            .into_iter()
            .next()
            .map(|emoji| emoji.picture)
    }

    /// The pictures the suggestions show, to ask for.
    pub fn suggestion_pictures(&self) -> Vec<String> {
        let mut urls = Vec::new();
        match self.shown_suggestions() {
            Some(Shown::Commands(..)) => urls.extend(self.command_pictures()),
            Some(Shown::Mentions(found)) => {
                urls.extend(found.into_iter().filter_map(|candidate| candidate.avatar));
            }
            Some(Shown::Emoji) => {
                let sets = self.client.app.usable_emoji();
                for found in &self.suggest.emoji {
                    if let Some((_, id)) = found.key.split_once(':')
                        && let Some(custom) = sets
                            .iter()
                            .flat_map(|set| &set.emoji)
                            .find(|emoji| emoji.id == id)
                    {
                        urls.push(custom.picture.clone());
                    }
                }
            }
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
