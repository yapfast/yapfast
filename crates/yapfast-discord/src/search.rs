//! A search of messages as Discord's search box reads it: words, and
//! filters such as `from:bob`, `in:general` and `has:image`, which become
//! the query string of `GET /guilds/{id}/messages/search` or
//! `GET /channels/{id}/messages/search`.
//!
//! It follows the client's tokenizer (its `SearchTokenizer` and the token
//! rules beside it): a filter's key may start anywhere, even inside a word;
//! the answer after it, spaces first allowed, counts only when it checks
//! out, as a person or channel that exists; and whatever isn't a key or an
//! answer is the words to find. A key left without an answer asks for
//! nothing. Nothing here fails: a value Discord wouldn't take is words.

use chrono::{DateTime, Datelike, Days, Months, NaiveDate, TimeZone};

/// The order results come in, as the results' Sort menu offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Relevant,
}

impl Sort {
    /// `sort_by` and `sort_order`, as Discord's client sends them.
    pub fn params(self) -> [(&'static str, String); 2] {
        let (by, order) = match self {
            Self::Newest => ("timestamp", "desc"),
            Self::Oldest => ("timestamp", "asc"),
            Self::Relevant => ("relevance", "desc"),
        };
        [("sort_by", by.to_owned()), ("sort_order", order.to_owned())]
    }
}

/// A filter, by its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    From,
    Mentions,
    Has,
    LinkFrom,
    FileType,
    FileName,
    Before,
    /// `during:`, or `on:`.
    During,
    After,
    In,
    Pinned,
    AuthorType,
}

/// The filters' keys in the order the client tries them, any letter case.
const KEYS: [(Filter, &str); 13] = [
    (Filter::From, "from:"),
    (Filter::Mentions, "mentions:"),
    (Filter::Has, "has:"),
    (Filter::LinkFrom, "linkfrom:"),
    (Filter::FileType, "filetype:"),
    (Filter::FileName, "filename:"),
    (Filter::Before, "before:"),
    (Filter::During, "on:"),
    (Filter::During, "during:"),
    (Filter::After, "after:"),
    (Filter::In, "in:"),
    (Filter::Pinned, "pinned:"),
    (Filter::AuthorType, "authortype:"),
];

impl Filter {
    /// Its key as the box writes it.
    pub fn key(self) -> &'static str {
        match self {
            Self::From => "from:",
            Self::Mentions => "mentions:",
            Self::Has => "has:",
            Self::LinkFrom => "linkFrom:",
            Self::FileType => "fileType:",
            Self::FileName => "fileName:",
            Self::Before => "before:",
            Self::During => "during:",
            Self::After => "after:",
            Self::In => "in:",
            Self::Pinned => "pinned:",
            Self::AuthorType => "authorType:",
        }
    }
}

/// What `has:` takes, as Discord names each in its box and in its API: a
/// forwarded message is a snapshot. Listed as its suggestions are.
pub const HAS: [(&str, &str); 9] = [
    ("image", "image"),
    ("video", "video"),
    ("link", "link"),
    ("file", "file"),
    ("embed", "embed"),
    ("sound", "sound"),
    ("poll", "poll"),
    ("sticker", "sticker"),
    ("forward", "snapshot"),
];

/// What `authorType:` takes, which the API calls the same.
pub const AUTHOR_TYPES: [(&str, &str); 3] =
    [("user", "user"), ("bot", "bot"), ("webhook", "webhook")];

/// Who `from:` or `mentions:` names, as it was typed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Who {
    /// A user id, which Discord takes as it is.
    Id(String),
    /// `@me`: this account.
    Me,
    /// An old-style `name#1234`.
    Tag(String, String),
    /// A username.
    Username(String),
}

/// What a search's text is checked against, as Discord's box checks it.
pub trait Context {
    /// Whether `in:` is a filter here: in a server, not in a DM.
    fn has_channels(&self) -> bool;
    /// The ids of the channels and threads called exactly `name`.
    fn channels_named(&self, name: &str) -> Vec<String>;
    /// The user `who` names, by id, if there is one.
    fn person(&self, who: &Who) -> Option<String>;
}

/// A piece of a search's text, by its byte range in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Words to find, spaces and all.
    Text,
    /// A filter's key.
    Key(Filter),
    /// The answer to the key before it.
    Answer(Filter, Answer),
}

/// What an answer asks of the messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A query string value, under its name.
    Value(&'static str, String),
    /// The channels and threads `in:` picks.
    Channels(Vec<String>),
    /// The snowflakes a message falls after (`min_id`) and before (`max_id`).
    Dates { min: Option<u64>, max: Option<u64> },
}

/// A search's text, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub tokens: Vec<Token>,
}

/// The longest search Discord's box takes, in characters.
pub const MAX_LENGTH: usize = 512;

/// When Discord's snowflakes start: 2015 began, in Unix milliseconds.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

impl Query {
    /// Reads `text` as Discord's box does, with dates read at `now`.
    pub fn parse<Tz: TimeZone>(text: &str, context: &dyn Context, now: &DateTime<Tz>) -> Self {
        let mut tokens: Vec<Token> = Vec::new();
        let mut words = None::<usize>;
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            // An answer only right after its key, then a key.
            let answered = match tokens.last() {
                Some(Token {
                    end,
                    kind: Kind::Key(filter),
                    ..
                }) if *end == at => answer(*filter, rest, context, now)
                    .map(|(length, answer)| (length, Kind::Answer(*filter, answer))),
                _ => None,
            };
            let found = answered
                .or_else(|| key(rest, context).map(|(length, filter)| (length, Kind::Key(filter))));
            let Some((length, kind)) = found else {
                words.get_or_insert(at);
                at += rest.chars().next().map_or(1, char::len_utf8);
                continue;
            };
            if let Some(start) = words.take() {
                tokens.push(Token {
                    start,
                    end: at,
                    kind: Kind::Text,
                });
            }
            tokens.push(Token {
                start: at,
                end: at + length,
                kind,
            });
            at += length;
        }
        if let Some(start) = words {
            tokens.push(Token {
                start,
                end: text.len(),
                kind: Kind::Text,
            });
        }
        Self {
            text: text.to_owned(),
            tokens,
        }
    }

    /// The query string for Discord, as the client builds it: each name
    /// where it first comes, its values once each, the words as one
    /// `content`, and the last date for each end. Keys alone ask nothing.
    pub fn params(&self) -> Vec<(&'static str, String)> {
        let mut named: Vec<(&'static str, Vec<String>)> = Vec::new();
        let mut add = |name: &'static str, value: String| {
            let index = named
                .iter()
                .position(|(known, _)| *known == name)
                .unwrap_or_else(|| {
                    named.push((name, Vec::new()));
                    named.len() - 1
                });
            let values = &mut named[index].1;
            if matches!(name, "min_id" | "max_id") {
                values.clear();
            }
            if !values.contains(&value) {
                values.push(value);
            }
        };
        for token in &self.tokens {
            match &token.kind {
                Kind::Key(_) => {}
                Kind::Text => add(
                    "content",
                    self.text[token.start..token.end].trim().to_owned(),
                ),
                Kind::Answer(_, Answer::Value(name, value)) => add(name, value.clone()),
                Kind::Answer(_, Answer::Channels(ids)) => {
                    for id in ids {
                        add("channel_id", id.clone());
                    }
                }
                Kind::Answer(_, Answer::Dates { min, max }) => {
                    if let Some(min) = min {
                        add("min_id", min.to_string());
                    }
                    if let Some(max) = max {
                        add("max_id", max.to_string());
                    }
                }
            }
        }
        let mut params = Vec::new();
        for (name, values) in named {
            if name == "content" {
                let content = values.join(" ").trim().to_owned();
                if !content.is_empty() {
                    params.push((name, content));
                }
            } else {
                params.extend(values.into_iter().map(|value| (name, value)));
            }
        }
        params
    }

    /// Whether it asks for nothing, as a box holding only keys does, which
    /// Discord doesn't search.
    pub fn is_empty(&self) -> bool {
        self.params().is_empty()
    }

    /// How many filters it has answers for.
    pub fn filters(&self) -> usize {
        self.tokens
            .iter()
            .filter(|token| matches!(token.kind, Kind::Answer(..)))
            .count()
    }
}

/// Whether `text` asks Discord for anything: Discord's box doesn't search
/// when it holds only filters' keys. `server` says whether `in:` is a
/// filter. It's read as if nobody and no channel had a name, which leaves
/// a person's or channel's answer as words, so it asks something, and
/// leaves a key after such an answer a key: only a username that is also
/// a key, as in `from: has:` with someone called has, is missed.
pub fn asks_anything(text: &str, server: bool) -> bool {
    struct Nameless(bool);
    impl Context for Nameless {
        fn has_channels(&self) -> bool {
            self.0
        }
        fn channels_named(&self, _: &str) -> Vec<String> {
            Vec::new()
        }
        fn person(&self, _: &Who) -> Option<String> {
            None
        }
    }
    !Query::parse(text, &Nameless(server), &chrono::Utc::now()).is_empty()
}

/// The key at the start of `text`, and its length.
fn key(text: &str, context: &dyn Context) -> Option<(usize, Filter)> {
    let (filter, key) = KEYS
        .iter()
        .find(|(_, key)| starts_with_any_case(text, key))?;
    (*filter != Filter::In || context.has_channels()).then_some((key.len(), *filter))
}

fn starts_with_any_case(text: &str, start: &str) -> bool {
    text.get(..start.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(start))
}

/// The answer to `filter` at the start of `text`, if it checks out, and
/// its length with the spaces before it.
fn answer<Tz: TimeZone>(
    filter: Filter,
    text: &str,
    context: &dyn Context,
    now: &DateTime<Tz>,
) -> Option<(usize, Answer)> {
    let spaces = text.len() - text.trim_start().len();
    let rest = &text[spaces..];
    let (length, answer) = match filter {
        Filter::From | Filter::Mentions => {
            let (length, who) = who(rest)?;
            let id = match &who {
                Who::Id(id) => Some(id.clone()),
                who => context.person(who),
            }?;
            let name = if filter == Filter::From {
                "author_id"
            } else {
                "mentions"
            };
            (length, Answer::Value(name, id))
        }
        Filter::Has => word(rest, "has", &HAS)?,
        Filter::AuthorType => word(rest, "author_type", &AUTHOR_TYPES)?,
        Filter::Pinned => {
            let (length, value) = ["true", "false"]
                .into_iter()
                .find(|word| starts_with_any_case(rest, word))
                .map(|word| (word.len(), &rest[..word.len()]))?;
            // The client takes them in small letters only.
            if value != "true" && value != "false" {
                return None;
            }
            (length, Answer::Value("pinned", value.to_owned()))
        }
        Filter::LinkFrom | Filter::FileType | Filter::FileName => {
            let length = rest.find(char::is_whitespace).unwrap_or(rest.len());
            if length == 0 {
                return None;
            }
            let name = match filter {
                Filter::LinkFrom => "link_hostname",
                Filter::FileType => "attachment_extension",
                _ => "attachment_filename",
            };
            (length, Answer::Value(name, rest[..length].to_owned()))
        }
        Filter::In => {
            let (length, value) = channel(rest)?;
            let ids = if is_snowflake(value) {
                vec![value.to_owned()]
            } else {
                let name = if value.starts_with('"') && value.ends_with('"') {
                    unescape(value.get(1..value.len() - 1).unwrap_or_default())
                } else {
                    value.to_owned()
                };
                context.channels_named(&name)
            };
            if ids.is_empty() {
                return None;
            }
            (length, Answer::Channels(ids))
        }
        Filter::Before | Filter::During | Filter::After => {
            let length = date_length(rest)?;
            let (start, end) = period(&rest[..length].to_lowercase(), now)?;
            let (min, max) = match filter {
                Filter::Before => (None, Some(start)),
                Filter::After => (Some(end), None),
                _ => (Some(start), Some(end)),
            };
            (length, Answer::Dates { min, max })
        }
    };
    Some((spaces + length, answer))
}

/// One of `words` at the start of `text`, perhaps after a `-` that
/// negates it: the longest that starts it in any case, which then has to
/// be written as the box lists it.
fn word(text: &str, name: &'static str, words: &[(&str, &str)]) -> Option<(usize, Answer)> {
    let negated = text.starts_with('-');
    let rest = &text[usize::from(negated)..];
    let (typed, api) = words
        .iter()
        .filter(|(typed, _)| starts_with_any_case(rest, typed))
        .max_by_key(|(typed, _)| typed.len())?;
    if !rest.starts_with(typed) {
        return None;
    }
    let value = if negated {
        format!("-{api}")
    } else {
        (*api).to_owned()
    };
    Some((
        usize::from(negated) + typed.len(),
        Answer::Value(name, value),
    ))
}

/// A person's answer at the start of `text`: an id, `@me`, `name#1234` or
/// a username, tried in that order.
fn who(text: &str) -> Option<(usize, Who)> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits >= 17 {
        let id = &text[..digits.min(20)];
        // An id has 17 to 19 digits; twenty are nobody's.
        return is_snowflake(id).then(|| (id.len(), Who::Id(id.to_owned())));
    }
    if starts_with_any_case(text, "@me") {
        return Some((3, Who::Me));
    }
    let name = text.find(['@', '#', ':']).map_or(text, |end| &text[..end]);
    let tag = text[name.len()..]
        .strip_prefix('#')
        .and_then(|rest| rest.get(..4))
        .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()));
    if let (false, Some(digits)) = (name.is_empty(), tag) {
        return Some((name.len() + 5, Who::Tag(name.to_owned(), digits.to_owned())));
    }
    let username = text
        .bytes()
        .take(32)
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.')
        .count();
    (username >= 2).then(|| (username, Who::Username(text[..username].to_owned())))
}

/// An `in:` answer at the start of `text`: an optional `#`, then a quoted
/// name, backslashes escaping a quote or a backslash, or a run of anything
/// but spaces. Its length, and the name as typed, quotes and all.
fn channel(text: &str) -> Option<(usize, &str)> {
    let value = |at: usize| -> Option<usize> {
        let rest = &text[at..];
        if let Some(quoted) = rest.strip_prefix('"') {
            let mut chars = quoted.char_indices();
            while let Some((index, character)) = chars.next() {
                match character {
                    '"' => return Some(index + 2),
                    '\\' => match chars.next() {
                        Some((_, '"' | '\\')) => {}
                        _ => break,
                    },
                    _ => {}
                }
            }
        }
        let length = rest.find(char::is_whitespace).unwrap_or(rest.len());
        (length > 0).then_some(length)
    };
    if text.starts_with('#')
        && let Some(length) = value(1)
    {
        return Some((1 + length, &text[1..=length]));
    }
    value(0).map(|length| (length, &text[..length]))
}

/// A quoted name's backslashes taken out, each keeping what follows it.
fn unescape(text: &str) -> String {
    let mut name = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => name.extend(chars.next()),
            character => name.push(character),
        }
    }
    name
}

/// Whether `text` is an id as the client tells one: 17 to 19 digits.
fn is_snowflake(text: &str) -> bool {
    (17..=19).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// How much of `text` a date answer takes: `2026-10-13`, `2026-10`, four
/// digits, or a run of anything but digits and spaces.
fn date_length(text: &str) -> Option<usize> {
    let digits = |from: usize, most: usize| {
        text[from..]
            .bytes()
            .take(most)
            .take_while(u8::is_ascii_digit)
            .count()
    };
    if digits(0, 4) == 4 {
        let month = text[4..].strip_prefix('-').map(|_| digits(5, 2));
        return Some(match month {
            Some(month @ 1..) => {
                let at = 5 + month;
                match text[at..].strip_prefix('-').map(|_| digits(at + 1, 2)) {
                    Some(day @ 1..) => at + 1 + day,
                    _ => at,
                }
            }
            _ => 4,
        });
    }
    let length = text
        .find(|character: char| character.is_ascii_digit() || character.is_whitespace())
        .unwrap_or(text.len());
    (length > 0).then_some(length)
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const WEEKDAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

/// The snowflakes at the start of the period `word` names and of the next,
/// as the client's dates go: today, yesterday, this week (from Sunday),
/// month or year; a month or weekday of this year or week; a year since
/// Discord began; or a day as `2026-10-13`, `2026-10` being its first.
fn period<Tz: TimeZone>(word: &str, now: &DateTime<Tz>) -> Option<(u64, u64)> {
    let today = now.date_naive();
    let week = today - Days::new(u64::from(today.weekday().num_days_from_sunday()));
    let month = today.with_day(1)?;
    let year = month.with_month(1)?;
    let day = |date: NaiveDate| Some((date, date.succ_opt()?));
    let (start, end) = match word {
        "today" => day(today)?,
        "yesterday" => day(today.pred_opt()?)?,
        "week" => (week, week + Days::new(7)),
        "month" => (month, month.checked_add_months(Months::new(1))?),
        "year" => (year, year.checked_add_months(Months::new(12))?),
        word => {
            if let Some(index) = MONTHS.iter().position(|name| *name == word) {
                let start = year.with_month(u32::try_from(index).ok()? + 1)?;
                (start, start.checked_add_months(Months::new(1))?)
            } else if let Some(index) = WEEKDAYS.iter().position(|name| *name == word) {
                day(week + Days::new(u64::try_from(index).ok()?))?
            } else if let Some(year) = word
                .parse::<i32>()
                .ok()
                .filter(|year| word.len() == 4 && (2015..=today.year()).contains(year))
            {
                let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
                (start, NaiveDate::from_ymd_opt(year + 1, 1, 1)?)
            } else {
                day(calendar_day(word)?)?
            }
        }
    };
    let instant = |date: NaiveDate| {
        let midnight = now
            .timezone()
            .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
            .earliest()?;
        u64::try_from(midnight.timestamp_millis()).ok()
    };
    Some((snowflake(instant(start)?), snowflake(instant(end)?)))
}

/// A day written as `2026-10-13`, or a month or a year, which is its first
/// day, as the client's day format reads it.
fn calendar_day(word: &str) -> Option<NaiveDate> {
    let mut parts = word.split('-');
    let year = parts.next()?.parse().ok()?;
    let mut number = || parts.next().map_or(Some(1), |part| part.parse().ok());
    let (month, day) = (number()?, number()?);
    NaiveDate::from_ymd_opt(year, month, day)
}

/// The smallest snowflake made at a Unix time in milliseconds.
pub fn snowflake(ms: u64) -> u64 {
    ms.saturating_sub(DISCORD_EPOCH) << 22
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    /// A server with #general, #trip notes and two channels called trip,
    /// and Bob and Carol in it; or a DM.
    struct Place {
        server: bool,
    }

    impl Context for Place {
        fn has_channels(&self) -> bool {
            self.server
        }
        fn channels_named(&self, name: &str) -> Vec<String> {
            match name {
                "general" => vec!["11111111111111111".into()],
                "trip notes" => vec!["22222222222222222".into()],
                "trip" => vec!["33333333333333333".into(), "44444444444444444".into()],
                _ => vec![],
            }
        }
        fn person(&self, who: &Who) -> Option<String> {
            match who {
                Who::Me => Some("1".into()),
                Who::Username(name) if name.eq_ignore_ascii_case("bob") => Some("2".into()),
                Who::Tag(name, _) if name == "Carol M" => Some("3".into()),
                _ => None,
            }
        }
    }

    const SERVER: Place = Place { server: true };
    const DM: Place = Place { server: false };

    /// A Thursday afternoon.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 19, 15, 30, 0).unwrap()
    }

    fn params(text: &str) -> Vec<(&'static str, String)> {
        Query::parse(text, &SERVER, &now()).params()
    }

    fn pairs(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        pairs
            .iter()
            .map(|(name, value)| (*name, (*value).to_owned()))
            .collect()
    }

    fn day(year: i32, month: u32, day: u32) -> u64 {
        let midnight = Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap();
        snowflake(u64::try_from(midnight.timestamp_millis()).unwrap())
    }

    #[test]
    fn reads_filters_and_keeps_the_rest_as_words() {
        // The spaces between two filters are words too, which leave two
        // spaces in content, as the client sends it.
        assert_eq!(
            params("trip from:bob in:general has:image mentions:@me notes"),
            pairs(&[
                ("content", "trip  notes"),
                ("author_id", "2"),
                ("channel_id", "11111111111111111"),
                ("has", "image"),
                ("mentions", "1"),
            ])
        );
        // A space may come between a key and its answer.
        assert_eq!(params("from: bob"), pairs(&[("author_id", "2")]));
    }

    #[test]
    fn answers_that_dont_check_out_are_words() {
        // Nobody called erin; @bob isn't a username; the box writes has:
        // and pinned: answers in small letters.
        assert_eq!(
            params("from:erin from:@bob has:Image pinned:True"),
            pairs(&[("content", "erin @bob Image True")])
        );
        assert_eq!(params("has:pictures"), pairs(&[("content", "pictures")]));
        // The same words twice count once, as the client's set of them does.
        assert_eq!(
            params("a from:bob a"),
            pairs(&[("content", "a"), ("author_id", "2")])
        );
    }

    #[test]
    fn keys_alone_ask_nothing() {
        assert!(Query::parse("from: in: has:", &SERVER, &now()).is_empty());
        assert!(Query::parse("  ", &SERVER, &now()).is_empty());
        assert!(!Query::parse("has:link", &SERVER, &now()).is_empty());
        assert!(!asks_anything("from: has:", false));
        assert!(!asks_anything("in: ", true));
        for asks in ["from:bob", "in:general", "has:link", "in:x", "on:today"] {
            assert!(asks_anything(asks, true), "{asks}");
        }
    }

    #[test]
    fn a_key_may_start_inside_a_word() {
        // As in the client, whose keys match anywhere: `on:` here.
        assert_eq!(
            params("person:bob https://example.com"),
            pairs(&[("content", "pers bob https://example.com")])
        );
    }

    #[test]
    fn in_names_channels_by_name_quotes_or_id_and_only_in_servers() {
        assert_eq!(
            params(r#"in:#"trip notes" in:trip"#),
            pairs(&[
                ("channel_id", "22222222222222222"),
                ("channel_id", "33333333333333333"),
                ("channel_id", "44444444444444444"),
            ])
        );
        assert_eq!(
            params("in:12345678901234567"),
            pairs(&[("channel_id", "12345678901234567")])
        );
        assert_eq!(params("in:General"), pairs(&[("content", "General")]));
        let dm = Query::parse("in:general", &DM, &now()).params();
        assert_eq!(dm, pairs(&[("content", "in:general")]));
    }

    #[test]
    fn people_by_id_tag_or_username() {
        assert_eq!(
            params("from:123456789012345678 mentions:Carol M#1234"),
            pairs(&[("author_id", "123456789012345678"), ("mentions", "3")])
        );
        assert_eq!(
            params("from:12345678901234567890"),
            pairs(&[("content", "12345678901234567890")])
        );
    }

    #[test]
    fn has_and_author_type_negate_and_map_to_the_api() {
        assert_eq!(
            params("has:forward has:-link authorType:-bot authorType:webhook"),
            pairs(&[
                ("has", "snapshot"),
                ("has", "-link"),
                ("author_type", "-bot"),
                ("author_type", "webhook"),
            ])
        );
        // The longest kind that starts the word; the rest is words.
        assert_eq!(
            params("has:images"),
            pairs(&[("has", "image"), ("content", "s")])
        );
    }

    #[test]
    fn typed_only_filters_take_any_word() {
        assert_eq!(
            params("linkFrom:youtube.com fileType:png fileName:notes.txt pinned:true"),
            pairs(&[
                ("link_hostname", "youtube.com"),
                ("attachment_extension", "png"),
                ("attachment_filename", "notes.txt"),
                ("pinned", "true"),
            ])
        );
    }

    #[test]
    fn dates_name_days_months_years_and_this_period() {
        let between = |text: &str| {
            let params = Query::parse(text, &SERVER, &now()).params();
            let found = |name| {
                params
                    .iter()
                    .find(|(known, _)| *known == name)
                    .map(|(_, value)| value.parse::<u64>().unwrap())
            };
            (found("min_id"), found("max_id"))
        };
        let on = |text: &str, start: u64, end: u64| {
            assert_eq!(between(text), (Some(start), Some(end)), "{text}");
        };
        on("during:2026-10-13", day(2026, 10, 13), day(2026, 10, 14));
        on("on:2026-2-3", day(2026, 2, 3), day(2026, 2, 4));
        on("on:2026-10", day(2026, 10, 1), day(2026, 10, 2));
        on("on:today", day(2026, 11, 19), day(2026, 11, 20));
        on("on:Yesterday", day(2026, 11, 18), day(2026, 11, 19));
        // The week starts on Sunday, as Discord's English client has it.
        on("on:week", day(2026, 11, 15), day(2026, 11, 22));
        on("on:monday", day(2026, 11, 16), day(2026, 11, 17));
        on("on:month", day(2026, 11, 1), day(2026, 12, 1));
        on("on:march", day(2026, 3, 1), day(2026, 4, 1));
        on("on:year", day(2026, 1, 1), day(2027, 1, 1));
        on("on:2020", day(2020, 1, 1), day(2021, 1, 1));
        // A year before Discord's is only its first day.
        on("on:2014", day(2014, 1, 1), day(2014, 1, 2));
        assert_eq!(
            between("before:2026-10-13"),
            (None, Some(day(2026, 10, 13)))
        );
        assert_eq!(between("after:2026-10-13"), (Some(day(2026, 10, 14)), None));
        // The last date says, for each end.
        assert_eq!(
            between("after:2026-01-01 before:2026-10-13 after:2026-05-05"),
            (Some(day(2026, 5, 6)), Some(day(2026, 10, 13)))
        );
        assert_eq!(between("before:2026-13-01"), (None, None));
        assert_eq!(between("before:soon"), (None, None));
    }

    #[test]
    fn days_start_at_local_midnight() {
        let prague = FixedOffset::east_opt(3600).unwrap();
        let query = Query::parse("on:2026-10-13", &SERVER, &now().with_timezone(&prague));
        let midnight = prague.with_ymd_and_hms(2026, 10, 13, 0, 0, 0).unwrap();
        let expected = snowflake(u64::try_from(midnight.timestamp_millis()).unwrap());
        assert_eq!(query.params()[0], ("min_id", expected.to_string()));
        assert_eq!(snowflake(DISCORD_EPOCH + 1), 1 << 22);
    }

    #[test]
    fn tokens_keep_their_place() {
        let query = Query::parse("hi from: bob", &SERVER, &now());
        let pieces: Vec<_> = query
            .tokens
            .iter()
            .map(|token| (&query.text[token.start..token.end], &token.kind))
            .collect();
        assert_eq!(
            pieces,
            [
                ("hi ", &Kind::Text),
                ("from:", &Kind::Key(Filter::From)),
                (
                    " bob",
                    &Kind::Answer(Filter::From, Answer::Value("author_id", "2".into()))
                ),
            ]
        );
        assert_eq!(query.filters(), 1);
    }

    #[test]
    fn sorts_as_the_menu_does() {
        assert_eq!(
            Sort::Relevant.params(),
            [
                ("sort_by", "relevance".into()),
                ("sort_order", "desc".into())
            ]
        );
        assert_eq!(Sort::Oldest.params()[1], ("sort_order", "asc".into()));
    }
}
