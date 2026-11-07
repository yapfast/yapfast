//! A search of messages as Discord's search box takes it: words, and
//! filters such as `from:bob`, `in:general` and `has:image`, which become
//! the query string of `GET /guilds/{id}/messages/search` or
//! `GET /channels/{id}/messages/search`.

use chrono::{Local, NaiveDate, TimeZone};

/// What a search's text asks for. Names are as typed; the service finds
/// the people and channels they name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    /// The words to find, without the filters.
    pub content: String,
    /// `from:`, who wrote it.
    pub from: Vec<String>,
    /// `mentions:`, whom it mentions.
    pub mentions: Vec<String>,
    /// `in:`, the server's channels it's in, without their `#`.
    pub channels: Vec<String>,
    /// `has:`, what it carries, as Discord's `has` names it.
    pub has: Vec<&'static str>,
    /// `after:` and `before:`, or `during:`: the snowflakes it falls
    /// between, as `min_id` and `max_id`.
    pub min_id: Option<u64>,
    pub max_id: Option<u64>,
    /// `pinned:true` or `pinned:false`.
    pub pinned: Option<bool>,
}

/// What `has:` takes, as Discord's search box offers it, and the name its
/// API gives each: a forwarded message is a snapshot.
const HAS: [(&str, &str); 9] = [
    ("link", "link"),
    ("embed", "embed"),
    ("poll", "poll"),
    ("file", "file"),
    ("video", "video"),
    ("image", "image"),
    ("sound", "sound"),
    ("sticker", "sticker"),
    ("forward", "snapshot"),
];

/// When Discord's snowflakes start: 2015 began, in Unix milliseconds.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

impl Query {
    /// Reads a search's text: filters where a word starts with one, a
    /// value in quotes when it has spaces, and the rest as the words to
    /// find. A filter Discord doesn't know stays a word, as `http://` does.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut query = Self::default();
        let mut words = Vec::new();
        for token in tokens(text) {
            let Some((key, value)) = token.split_once(':').filter(|(key, value)| {
                !value.is_empty() && FILTERS.contains(&key.to_lowercase().as_str())
            }) else {
                words.push(token);
                continue;
            };
            let value = value.trim_matches('"');
            match key.to_lowercase().as_str() {
                "from" => query.from.push(value.trim_start_matches('@').to_owned()),
                "mentions" => query
                    .mentions
                    .push(value.trim_start_matches('@').to_owned()),
                "in" => query
                    .channels
                    .push(value.trim_start_matches('#').to_owned()),
                "has" => {
                    let has = HAS
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(value))
                        .map(|(_, has)| *has)
                        .ok_or_else(|| {
                            format!("has: takes {}.", HAS.map(|(name, _)| name).join(", "))
                        })?;
                    query.has.push(has);
                }
                "pinned" => {
                    query.pinned = Some(match value.to_lowercase().as_str() {
                        "true" | "yes" => true,
                        "false" | "no" => false,
                        _ => return Err("pinned: takes true or false.".into()),
                    })
                }
                day => {
                    let (start, end) = day_span(value)?;
                    match day {
                        "before" => query.max_id = Some(start),
                        "after" => query.min_id = Some(end),
                        _ => (query.min_id, query.max_id) = (Some(start), Some(end)),
                    }
                }
            }
        }
        query.content = words.join(" ");
        Ok(query)
    }

    /// Whether it asks for nothing at all.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// The query string for Discord, without the people and channels,
    /// which the service adds by id.
    pub fn params(&self) -> Vec<(&'static str, String)> {
        let mut params = Vec::new();
        if !self.content.is_empty() {
            params.push(("content", self.content.clone()));
        }
        params.extend(self.has.iter().map(|has| ("has", (*has).to_owned())));
        params.extend(self.min_id.map(|id| ("min_id", id.to_string())));
        params.extend(self.max_id.map(|id| ("max_id", id.to_string())));
        params.extend(self.pinned.map(|pinned| ("pinned", pinned.to_string())));
        params
    }
}

const FILTERS: [&str; 8] = [
    "from", "mentions", "in", "has", "before", "after", "during", "pinned",
];

/// The words of a search, a quoted part, even after a filter's colon,
/// kept whole with its quotes.
fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                token.push(character);
            }
            character if character.is_whitespace() && !quoted => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            character => token.push(character),
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

/// The snowflakes at the start of a day, `YYYY-MM-DD` in local time, and of
/// the next.
fn day_span(day: &str) -> Result<(u64, u64), String> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| format!("RatFast reads dates as 2026-10-13, not {day}."))?;
    let start = |date: NaiveDate| {
        let midnight = date.and_hms_opt(0, 0, 0)?;
        let at = Local.from_local_datetime(&midnight).earliest()?;
        u64::try_from(at.timestamp_millis()).ok()
    };
    let next = date.succ_opt();
    match (start(date), next.and_then(start)) {
        (Some(start), Some(end)) => Ok((snowflake(start), snowflake(end))),
        _ => Err(format!("Discord can't search around {day}.")),
    }
}

/// The smallest snowflake made at a Unix time in milliseconds.
pub fn snowflake(ms: u64) -> u64 {
    ms.saturating_sub(DISCORD_EPOCH) << 22
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_filters_and_keeps_the_rest_as_words() {
        let query =
            Query::parse(r#"trip from:@bob in:#general has:Image mentions:"Carol M" notes"#)
                .unwrap();
        assert_eq!(query.content, "trip notes");
        assert_eq!(query.from, ["bob"]);
        assert_eq!(query.channels, ["general"]);
        assert_eq!(query.has, ["image"]);
        assert_eq!(query.mentions, ["Carol M"]);
        assert_eq!(
            query.params(),
            [("content", "trip notes".into()), ("has", "image".into())]
        );
        let forwarded = Query::parse("has:forward pinned:true").unwrap();
        assert_eq!(forwarded.has, ["snapshot"]);
        assert_eq!(forwarded.pinned, Some(true));
        assert!(forwarded.content.is_empty() && !forwarded.is_empty());
    }

    #[test]
    fn unknown_filters_and_empty_values_are_words() {
        let query = Query::parse("see https://example.com at: 10:30").unwrap();
        assert_eq!(query.content, "see https://example.com at: 10:30");
        assert!(Query::parse("from:").unwrap().from.is_empty());
        assert!(Query::parse("  ").unwrap().is_empty());
    }

    #[test]
    fn refuses_what_discord_wouldnt_take() {
        assert!(Query::parse("has:pictures").unwrap_err().contains("image"));
        assert!(Query::parse("before:yesterday").is_err());
        assert!(Query::parse("pinned:maybe").is_err());
    }

    #[test]
    fn days_become_snowflakes_around_them() {
        let during = Query::parse("during:2026-10-13").unwrap();
        let (start, end) = (during.min_id.unwrap(), during.max_id.unwrap());
        // A day is 86,400,000 ms, give or take a change of clocks.
        let day = (end - start) >> 22;
        assert!((82_800_000..=90_000_000).contains(&day), "{day}");
        assert_eq!(
            Query::parse("before:2026-10-13").unwrap().max_id,
            Some(start)
        );
        assert_eq!(Query::parse("after:2026-10-13").unwrap().min_id, Some(end));
        assert_eq!(snowflake(DISCORD_EPOCH + 1), 1 << 22);
    }
}
