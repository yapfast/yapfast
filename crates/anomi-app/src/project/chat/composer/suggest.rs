//! What the word being typed is offered (`MentionList` and
//! `EmojiSuggestions`, filled by `show_composer` and
//! `show_emoji_suggestions`, `src/ui/refresh.rs:4040`, `:4127`): the people,
//! roles and channels the `@` or `#` word may name, or else the emoji the
//! `:name` may mean, over the field typed in last: the channel's composer,
//! or the message edited in place.

use yapfast_core::application::{AppState, Application};
use yapfast_core::chat::EmojiSet;
use yapfast_core::emoji::{self, Found};
use yapfast_core::mention::Candidate;
use yapfast_lang::tr;

use anomi_theme::fonts::twemoji;
use anomi_vm::chat::Chat;
use anomi_vm::chat::emoji_picker::Emoji;
use anomi_vm::chat::inline_edit;
use anomi_vm::chat::suggestions::{Cell, Mark, Mention, Offer, Suggestions};
use anomi_vm::chat::timeline::Timeline;

use super::{Shown, Sources};
use crate::pictures::Requests;
use crate::project::rows;

/// How many emoji a `:name` is offered (`EMOJI_SUGGESTIONS`).
const EMOJI_SUGGESTIONS: usize = 10;

/// The field the suggestions are for: the one typed in last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Field {
    #[default]
    Channel,
    /// The message edited in place.
    Edit,
    /// The thread beside the channel's composer.
    Aside,
}

/// Where the caret last was in the channel's composer, with whose draft it
/// was (`Composer::room`), and in the message edited in place: an emoji from
/// the picker goes in there (`caret`, `edit_caret`,
/// `src/ui/callbacks.rs:2876`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Carets {
    pub(crate) channel: Option<(u64, usize)>,
    pub(crate) edit: Option<usize>,
    pub(crate) aside: Option<(u64, usize)>,
}

impl Carets {
    /// The caret of `field` is at `at`; `room` is whose draft the
    /// channel's composer shows.
    pub(crate) fn note(&mut self, field: Field, room: u64, at: usize) {
        match field {
            Field::Channel => self.channel = Some((room, at)),
            Field::Edit => self.edit = Some(at),
            Field::Aside => self.aside = Some((room, at)),
        }
    }
}

/// The words being typed, which the intents follow as the caret moves
/// (`composer_caret`, `src/ui/callbacks.rs:4078`), and what of them Escape
/// closed.
#[derive(Clone, Debug, Default)]
pub(crate) struct Words {
    pub(crate) field: Field,
    /// Where the `@` or `#` word starts, whose query the core keeps
    /// (`mention_at`); the one Escape closed (`mention_dismissed`).
    pub(crate) mention_at: Option<usize>,
    pub(crate) mention_dismissed: Option<usize>,
    /// The `:name` word: where its colon is and the name so far
    /// (`emoji_word`); the one Escape closed.
    pub(crate) emoji: Option<(usize, String)>,
    pub(crate) emoji_dismissed: Option<usize>,
    /// The highlighted mention or emoji (`mention-index`,
    /// `emoji-suggestion-index`), and the highlighted row of the slash
    /// commands' list (`command-choice-index`).
    pub(crate) index: usize,
    pub(crate) command: usize,
}

impl Words {
    /// What the core changed ends words: an edit that ended takes its
    /// words with it, and a sent message or another room's draft ends the
    /// `:name` word.
    pub(crate) fn settle(&mut self, state: &AppState) {
        let gone = match self.field {
            Field::Edit => state.edit().is_none(),
            Field::Aside => state.aside.is_none(),
            Field::Channel => false,
        };
        if gone {
            *self = Self::default();
        }
        let text = text(state, self.field);
        if let Some((at, name)) = &self.emoji
            && text.get(*at..at + 1 + name.len()) != Some(format!(":{name}").as_str())
        {
            self.emoji = None;
        }
        if let Some(at) = self.mention_at
            && !text
                .get(at..)
                .is_some_and(|word| word.starts_with(['@', '#']))
        {
            self.mention_at = None;
        }
    }
}

/// What `field` holds.
pub(crate) fn text(state: &AppState, field: Field) -> &str {
    match field {
        Field::Channel => state.draft(),
        Field::Edit => state.edit().map_or("", |edit| edit.text.as_str()),
        Field::Aside => state.aside_draft(),
    }
}

/// What the suggestions were built from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Key {
    field: Field,
    query: Option<(char, String)>,
    emoji: Option<(usize, String)>,
    index: usize,
    pictures: u64,
    sources: Sources,
    /// Something was offered: an edited row built again since must be
    /// given it again.
    offered: bool,
}

pub(super) fn show(
    core: &Application,
    chat: &mut Chat,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let words = &shown.words;
    let mut key = Key {
        field: words.field,
        query: words.mention_at.and(state.mention_query.clone()),
        emoji: words.emoji.clone(),
        index: words.index,
        pictures: state.picture_changes,
        sources: Sources::of(state),
        offered: false,
    };
    let lost = words.field == Field::Edit
        && edited(&mut chat.timeline).is_some_and(|field| !field.suggesting());
    if let Some(was) = &shown.offer {
        key.offered = was.offered;
        if *was == key && !(lost && was.offered) {
            return;
        }
    }
    let offer = build(core, words, requests);
    key.offered = offer != Offer::Nothing;
    shown.offer = Some(key);
    let suggestions = Suggestions { offer };
    // Over the field typed in last, and over no other.
    let none = Suggestions::default;
    let (channel, aside) = match words.field {
        Field::Channel => (suggestions, none()),
        Field::Aside => (none(), suggestions),
        Field::Edit => {
            if let Some(field) = edited(&mut chat.timeline)
                && field.suggestions != suggestions
            {
                field.suggestions = suggestions;
            }
            (none(), none())
        }
    };
    if chat.composer.suggestions != channel {
        chat.composer.suggestions = channel;
    }
    let beside = &mut chat.thread_panel.composer;
    if beside.suggestions != aside {
        beside.suggestions = aside;
    }
}

/// The field of the message edited in place, in its row.
fn edited(timeline: &mut Timeline) -> Option<&mut inline_edit::Field> {
    let key = timeline.editing?;
    let row = timeline.rows.iter_mut().find(|row| row.key == key)?;
    row.message.editing.field.as_deref_mut()
}

/// The mentions for the `@` or `#` word, else the emoji for the `:name`.
fn build(core: &Application, words: &Words, requests: &mut Requests) -> Offer {
    let state = &core.state;
    if words.mention_at.is_some() && state.mention_query.is_some() {
        let candidates = state.mention_suggestions();
        if !candidates.is_empty() {
            requests.want(
                state,
                candidates
                    .iter()
                    .filter(|candidate| !candidate.group)
                    .filter_map(|candidate| candidate.avatar.as_deref()),
            );
            let index = within(words.index, candidates.len());
            return Offer::Mentions {
                rows: mentions(state, &candidates),
                index,
            };
        }
    }
    let Some((_, name)) = &words.emoji else {
        return Offer::Nothing;
    };
    let usable = core.usable_emoji();
    let cells: Vec<Cell> =
        emoji::suggestions(name, &usable, twemoji::has_picture, EMOJI_SUGGESTIONS)
            .into_iter()
            .filter_map(|found| cell(state, &usable, found, requests))
            .collect();
    if cells.is_empty() {
        return Offer::Nothing;
    }
    Offer::Emoji {
        query: name.clone(),
        index: within(words.index, cells.len()),
        cells,
    }
}

/// `index`, or the first row when there aren't that many.
fn within(index: usize, len: usize) -> usize {
    if index < len { index } else { 0 }
}

/// People come first, then roles and groups, each under a heading; `#`
/// lists channels alone.
fn mentions(state: &AppState, candidates: &[Candidate]) -> Vec<Mention> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let starts = index == 0 || candidates[index - 1].group != candidate.group;
            let heading = match (starts, candidate.group, candidate.channel) {
                (false, ..) => "",
                (true, false, _) => tr!("MEMBERS"),
                (true, true, false) => tr!("ROLES"),
                (true, true, true) => tr!("CHANNELS"),
            };
            let mark = if !candidate.group {
                Mark::Person(rows::avatar(
                    state,
                    &candidate.name,
                    &candidate.name,
                    candidate.avatar.as_deref(),
                ))
            } else if candidate.voice {
                Mark::Voice
            } else {
                Mark::Group(candidate.sigil().to_string())
            };
            Mention {
                name: candidate.name.clone(),
                mark,
                color: candidate.color.map(rows::rgb),
                detail: match candidate.mention.as_str() {
                    "@everyone" => tr!("Everyone in this channel").to_owned(),
                    "@here" => tr!("Everyone online here").to_owned(),
                    _ => candidate.username.clone().unwrap_or_default(),
                },
                heading: heading.to_owned(),
            }
        })
        .collect()
}

/// An emoji found for the `:name`: a server's with its picture, which is
/// asked for until it comes.
fn cell(
    state: &AppState,
    usable: &[EmojiSet],
    found: Found,
    requests: &mut Requests,
) -> Option<Cell> {
    let emoji = match found.key.split_once(':') {
        None => Emoji::unicode(&found.key)?,
        Some((name, id)) => {
            let custom = usable
                .iter()
                .flat_map(|set| &set.emoji)
                .find(|custom| custom.id == id)?;
            requests.want(state, [custom.picture.as_str()]);
            let picture = rows::expected(state, Some(&custom.picture));
            Emoji::custom(name, id, picture, true)
        }
    };
    Some(Cell {
        emoji,
        name: found.name,
    })
}
