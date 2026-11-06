//! Mentioning people, roles and channels from the composer: the `@` or `#` word
//! being typed, who or what it may mean, and the message text that mentions
//! them on its network.

use std::ops::Range;

/// Someone or something a message can mention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// What follows the `@` in the composer.
    pub name: String,
    /// What else names them: a Discord username or a Matrix user id.
    pub username: Option<String>,
    /// How a message says it on its network, such as `<@20>` on Discord.
    pub mention: String,
    /// The address of their avatar; see [`crate::chat::DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// The RGB colour of the name.
    pub color: Option<u32>,
    /// A role or a group such as @everyone, or a channel, rather than a person.
    pub group: bool,
    /// A channel, which `#` mentions rather than `@`.
    pub channel: bool,
    /// A voice channel.
    pub voice: bool,
}

impl Candidate {
    /// What the composer writes before the name: `#` for a channel, else `@`.
    pub fn sigil(&self) -> char {
        if self.channel { '#' } else { '@' }
    }
}

/// Someone or something a message mentions, as the message shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mentioned {
    /// The text in the message, such as `@Bob`.
    pub text: String,
    pub target: Target,
    /// The RGB colour of the role, or of the person's name.
    pub color: Option<u32>,
    /// Where the text starts in the message's body, a byte offset each time
    /// it mentions them.
    pub at: Vec<usize>,
}

/// Who a mention points at: a user, by Discord or Matrix id, a Discord role,
/// a channel, by room id, or everyone, as Discord's `@everyone` and `@here` do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    User(String),
    Role(String),
    Channel(String),
    Everyone,
}

const USER_LINK: &str = "yapfast:user/";
const ROLE_LINK: &str = "yapfast:role/";
const CHANNEL_LINK: &str = "yapfast:channel/";
const EVERYONE_LINK: &str = "yapfast:everyone";
const MATRIX_TO: &str = "https://matrix.to/#/";

/// The address a mention links to in a message; clicking it opens the person,
/// role or channel.
pub fn link(target: &Target) -> String {
    match target {
        Target::User(id) => format!("{USER_LINK}{id}"),
        Target::Role(id) => format!("{ROLE_LINK}{id}"),
        Target::Channel(room) => format!("{CHANNEL_LINK}{room}"),
        Target::Everyone => EVERYONE_LINK.to_owned(),
    }
}

/// Who a clicked link mentions: Yapfast's mention links, and Matrix's links to a user.
pub fn target(link: &str) -> Option<Target> {
    if let Some(id) = link.strip_prefix(USER_LINK) {
        return Some(Target::User(id.to_owned()));
    }
    if let Some(id) = link.strip_prefix(ROLE_LINK) {
        return Some(Target::Role(id.to_owned()));
    }
    if let Some(room) = link.strip_prefix(CHANNEL_LINK) {
        return Some(Target::Channel(room.to_owned()));
    }
    if link == EVERYONE_LINK {
        return Some(Target::Everyone);
    }
    let user = link.strip_prefix(MATRIX_TO)?;
    let user = user.split(['?', '/']).next()?;
    let user = user
        .replace("%40", "@")
        .replace("%3A", ":")
        .replace("%3a", ":");
    (user.starts_with('@') && user.contains(':')).then_some(Target::User(user))
}

/// Someone's Markdown with its HTML escaped, as [`crate::links::escape_html`]
/// does, and each mention outside code linked to its target and in its
/// colour, which the message draws behind it too. A mention is only where
/// the message's tag for it was, at its offsets, as Discord's client shows
/// them: `@Bobby` typed as text isn't Bob's. A mention without a colour of
/// its own takes `link_color`, `0xrrggbb`: the theme's for links.
pub fn linked(markdown: &str, mentions: &[Mentioned], link_color: u32) -> String {
    let escaped = crate::links::escape_html(markdown);
    let mut found: Vec<(usize, &Mentioned)> = mentions
        .iter()
        .filter(|mention| !mention.text.is_empty())
        .flat_map(|mention| mention.at.iter().map(move |&at| (at, mention)))
        .collect();
    if found.is_empty() {
        return escaped.into_owned();
    }
    found.sort_by_key(|(at, _)| *at);
    shift(markdown, &escaped, &mut found);
    let mut out = String::with_capacity(escaped.len() + 64 * found.len());
    let mut code = false;
    let mut last = 0;
    for (at, mention) in found {
        if at < last
            || !escaped
                .get(at..)
                .is_some_and(|rest| rest.starts_with(&mention.text))
        {
            continue;
        }
        let before = &escaped[last..at];
        code ^= before.matches('`').count() % 2 == 1;
        out.push_str(before);
        last = at;
        if code {
            continue;
        }
        // A name is shown as typed: one such as `[x](https://…)` would
        // otherwise be a link of its own. A channel's comes escaped already.
        let text = match mention.target {
            Target::Channel(_) => mention.text.clone(),
            _ => crate::links::escape_markdown(&mention.text),
        };
        let color = mention.color.unwrap_or(link_color) & 0xff_ffff;
        out.push_str(&format!(
            "<font color=\"#{color:06x}\">[{text}](<{}>)</font>",
            link(&mention.target)
        ));
        last = at + mention.text.len();
    }
    out.push_str(&escaped[last..]);
    out
}

/// A message's `body` as its author would type it again, to edit: a channel's
/// name is escaped where it shows, and would go out as text with its backslashes.
pub fn as_typed(body: &str, mentions: &[Mentioned]) -> String {
    let mut places: Vec<(usize, &str)> = mentions
        .iter()
        .filter(|mention| matches!(mention.target, Target::Channel(_)))
        .flat_map(|mention| {
            mention
                .at
                .iter()
                .map(move |&at| (at, mention.text.as_str()))
        })
        .collect();
    places.sort_unstable_by_key(|(at, _)| std::cmp::Reverse(*at));
    let mut typed = body.to_owned();
    for (at, text) in places {
        if typed.get(at..at + text.len()) == Some(text) {
            typed.replace_range(at..at + text.len(), &without_escapes(text));
        }
    }
    typed
}

/// `text` without the backslashes that escape its Markdown's punctuation.
fn without_escapes(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(next) = chars.next() {
        if next == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            continue;
        }
        plain.push(next);
    }
    plain
}

/// Moves each offset into `original` to the same place in `escaped`, which
/// is `original` with backslashes put in before some of its `<`. The
/// offsets are in order.
fn shift(original: &str, escaped: &str, offsets: &mut [(usize, &Mentioned)]) {
    if original.len() == escaped.len() {
        return;
    }
    let (original, escaped) = (original.as_bytes(), escaped.as_bytes());
    let (mut from, mut to) = (0, 0);
    for (at, _) in offsets {
        while from < *at && from < original.len() {
            if escaped[to] == original[from] {
                from += 1;
            }
            to += 1;
        }
        *at = to;
    }
}

/// Whether a name followed by `after` ends there, rather than going on into a
/// longer word.
fn ends_name(after: &str) -> bool {
    !after.starts_with(is_name_char)
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Names longer than this aren't looked up while typing.
const MAX_QUERY: usize = 32;

/// The `@` or `#` word ending at `cursor`, a byte offset into `text`: where its
/// sigil is, which one, and what follows it.
pub fn query(text: &str, cursor: usize) -> Option<(usize, char, &str)> {
    let before = text.get(..cursor)?;
    let at = before.rfind(['@', '#'])?;
    let sigil = if before[at..].starts_with('#') {
        '#'
    } else {
        '@'
    };
    let query = &before[at + 1..];
    let starts_word = before[..at]
        .chars()
        .next_back()
        .is_none_or(|previous| previous.is_whitespace() || "([{\"'".contains(previous));
    (starts_word && query.len() <= MAX_QUERY && !query.contains(char::is_whitespace))
        .then_some((at, sigil, query))
}

/// The candidates `query` may mean, at most `limit`: names that start with it,
/// then usernames that do, then names with a later word that does.
pub fn matching<'a>(candidates: &'a [Candidate], query: &str, limit: usize) -> Vec<&'a Candidate> {
    let query = query.to_lowercase();
    let rank = |candidate: &Candidate| {
        let name = candidate.name.to_lowercase();
        if name.starts_with(&query) {
            Some(0)
        } else if candidate
            .username
            .as_ref()
            .is_some_and(|username| username.to_lowercase().starts_with(&query))
        {
            Some(1)
        } else if name
            .split_whitespace()
            .skip(1)
            .any(|word| word.starts_with(&query))
        {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, &Candidate)> = candidates
        .iter()
        .filter_map(|candidate| Some((rank(candidate)?, candidate)))
        .collect();
    found.sort_by_key(|(rank, _)| *rank);
    found
        .into_iter()
        .take(limit)
        .map(|(_, candidate)| candidate)
        .collect()
}

/// `text` with each `@name` or `#channel` of a candidate, outside code, in its
/// network's form.
pub fn wire(text: &str, candidates: &[Candidate]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut end = 0;
    for (range, candidate) in found(text, candidates) {
        out.push_str(&text[end..range.start]);
        out.push_str(&candidate.mention);
        end = range.end;
    }
    out.push_str(&text[end..]);
    out
}

/// Where `text` mentions a candidate, as [`wire`] finds them, each with the
/// candidate's colour: the composer shows them as the message will.
pub fn marks(text: &str, candidates: &[Candidate]) -> Vec<(Range<usize>, Option<u32>)> {
    found(text, candidates)
        .into_iter()
        .map(|(range, candidate)| (range, candidate.color))
        .collect()
}

/// Each `@name` or `#channel` of a candidate in `text`, outside code, and whom
/// it names. The longest name that fits wins, so `@Ann Lee` isn't taken for
/// `@Ann`, and of names that differ only in case the one written so, as a pick
/// writes it, so `#General` isn't taken for `#general`. Two names written alike
/// can't be told apart: the first listed wins.
fn found<'a>(text: &str, candidates: &'a [Candidate]) -> Vec<(Range<usize>, &'a Candidate)> {
    if !text.contains(['@', '#']) {
        return Vec::new();
    }
    let mut sorted: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| !candidate.name.is_empty())
        .collect();
    sorted.sort_by_key(|candidate| std::cmp::Reverse(candidate.name.len()));
    let mut found = Vec::new();
    let mut code = false;
    let mut at = 0;
    let mut previous: Option<char> = None;
    while let Some(next) = text[at..].chars().next() {
        if next == '`' {
            code = !code;
        }
        let starts_word = previous.is_none_or(|previous| !previous.is_alphanumeric());
        if (next == '@' || next == '#') && !code && starts_word {
            let after = &text[at + 1..];
            let mut fits = sorted.iter().copied().filter(|candidate| {
                let name = &candidate.name;
                candidate.sigil() == next
                    && after
                        .get(..name.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(name))
                    && ends_name(&after[name.len()..])
            });
            let longest = fits.next();
            let candidate = longest.map(|longest| {
                std::iter::once(longest)
                    .chain(fits.take_while(|other| other.name.len() == longest.name.len()))
                    .find(|candidate| after.starts_with(&candidate.name))
                    .unwrap_or(longest)
            });
            if let Some(candidate) = candidate {
                let end = at + 1 + candidate.name.len();
                found.push((at..end, candidate));
                previous = candidate.name.chars().next_back();
                at = end;
                continue;
            }
        }
        at += next.len_utf8();
        previous = Some(next);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, mention: &str) -> Candidate {
        Candidate {
            name: name.into(),
            username: None,
            mention: mention.into(),
            avatar: None,
            color: None,
            group: false,
            channel: false,
            voice: false,
        }
    }

    fn channel(name: &str, mention: &str) -> Candidate {
        Candidate {
            group: true,
            channel: true,
            ..person(name, mention)
        }
    }

    #[test]
    fn mentions_link_to_who_they_mention() {
        let bob = |at: &[usize]| Mentioned {
            text: "@Bob".into(),
            target: Target::User("2".into()),
            color: None,
            at: at.to_vec(),
        };
        let mods = Mentioned {
            text: "@Mods".into(),
            target: Target::Role("30".into()),
            color: Some(0x3498db),
            at: vec![13],
        };
        let pill = "<font color=\"#7cc4ff\">[\\@Bob](<yapfast:user/2>)</font>";
        assert_eq!(
            linked(
                "hi @Bob, ask @Mods but not `@Bob`",
                &[bob(&[3, 28]), mods],
                0x7cc4ff
            ),
            format!(
                "hi {pill}, ask <font color=\"#3498db\">[\\@Mods](<yapfast:role/30>)</font> \
                 but not `@Bob`"
            )
        );
        // Only where the message's tag was: `<@2>by`, not `@Bob` typed.
        assert_eq!(
            linked("@Bobby and @Bob", &[bob(&[0])], 0x7cc4ff),
            format!("{pill}by and @Bob")
        );
        assert_eq!(
            linked("<b>x</b> @Bob", &[bob(&[9])], 0x7cc4ff),
            format!("\\<b>x\\</b> {pill}"),
            "escaping the HTML moves the mention"
        );
        let named = [Mentioned {
            text: "@[Support](https://evil.example)".into(),
            target: Target::User("3".into()),
            color: None,
            at: vec![0],
        }];
        assert!(
            linked("@[Support](https://evil.example)", &named, 0)
                .contains("[\\@\\[Support\\]\\(https\\:\\/\\/evil\\.example\\)]"),
            "a name shows as typed"
        );
        assert_eq!(target("yapfast:user/2"), Some(Target::User("2".into())));
        assert_eq!(target("yapfast:role/30"), Some(Target::Role("30".into())));
        assert_eq!(target(&link(&Target::Everyone)), Some(Target::Everyone));
        assert_eq!(
            target("https://matrix.to/#/@sam:test?via=test"),
            Some(Target::User("@sam:test".into()))
        );
        assert_eq!(
            target("https://matrix.to/#/%40sam%3Atest"),
            Some(Target::User("@sam:test".into()))
        );
        assert_eq!(target("https://matrix.to/#/#games:test"), None);
        assert_eq!(target("https://example.com"), None);
    }

    #[test]
    fn finds_the_word_being_typed() {
        assert_eq!(query("hi @bo", 6), Some((3, '@', "bo")));
        assert_eq!(query("@", 1), Some((0, '@', "")));
        assert_eq!(query("(@carol", 7), Some((1, '@', "carol")));
        assert_eq!(query("mail me@home", 12), None, "not after a letter");
        assert_eq!(query("@bob is here", 12), None, "a space ends it");
        assert_eq!(
            query("hi @bo and", 6),
            Some((3, '@', "bo")),
            "the cursor ends it"
        );
        assert_eq!(query("see #gen", 8), Some((4, '#', "gen")));
        assert_eq!(query("#", 1), Some((0, '#', "")));
        assert_eq!(query("C#", 2), None, "not after a letter");
        assert_eq!(query("# Title", 7), None, "a heading's space ends it");
    }

    #[test]
    fn matches_names_and_later_words() {
        let candidates = [
            person("Bob", "<@2>"),
            person("Status Bot", "<@7>"),
            person("Carol", "<@3>"),
            person("bobby", "<@9>"),
            Candidate {
                username: Some("bob_the_builder".into()),
                ..person("Robert", "<@11>")
            },
        ];
        let names = |query| -> Vec<&str> {
            matching(&candidates, query, 8)
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect()
        };
        assert_eq!(names("bo"), ["Bob", "bobby", "Robert", "Status Bot"]);
        assert_eq!(names(""), ["Bob", "Status Bot", "Carol", "bobby", "Robert"]);
        assert!(names("x").is_empty());
        assert_eq!(matching(&candidates, "", 2).len(), 2);
    }

    #[test]
    fn mentions_go_out_in_the_network_form() {
        let candidates = [
            person("Ann", "<@1>"),
            person("Ann Lee", "<@2>"),
            person("everyone", "@everyone"),
            Candidate {
                group: true,
                ..person("Mods", "<@&20>")
            },
        ];
        assert_eq!(
            wire("@Ann Lee and @ann, ask @Mods!", &candidates),
            "<@2> and <@1>, ask <@&20>!"
        );
        assert_eq!(wire("@everyone look", &candidates), "@everyone look");
        assert_eq!(wire("@Annie stays", &candidates), "@Annie stays");
        assert_eq!(wire("me@Ann.com", &candidates), "me@Ann.com");
        assert_eq!(wire("`@Ann` but @Ann", &candidates), "`@Ann` but <@1>");
        assert_eq!(wire("café @Ann", &candidates), "café <@1>");
    }

    #[test]
    fn channels_go_out_with_hash() {
        let candidates = [
            channel("general", "<#11>"),
            channel("off-topic", "<#12>"),
            person("general", "<@3>"),
        ];
        assert_eq!(
            wire("see #general and #off-topic, ask @general", &candidates),
            "see <#11> and <#12>, ask <@3>"
        );
        assert_eq!(
            wire("C#general and #generalist", &candidates),
            "C#general and #generalist"
        );
        assert_eq!(wire("`#general`", &candidates), "`#general`");
    }

    #[test]
    fn a_name_written_alike_wins_over_one_in_another_case() {
        let text = channel("general", "<#1>");
        let voice = channel("General", "<#2>");
        assert_eq!(
            wire(
                "#General, #general, #GENERAL",
                &[text.clone(), voice.clone()]
            ),
            "<#2>, <#1>, <#1>"
        );
        assert_eq!(wire("#General", &[voice, text]), "<#2>");
        // The longest still wins first.
        let candidates = [person("Ann", "<@1>"), person("ann lee", "<@2>")];
        assert_eq!(wire("@Ann Lee", &candidates), "<@2>");
    }

    #[test]
    fn a_channels_mention_opens_it_and_shows_its_escaped_name() {
        let target = Target::Channel("discord:1:2".into());
        assert_eq!(super::target(&link(&target)), Some(target.clone()));
        let mentions = [Mentioned {
            text: "#off\\-topic".into(),
            target,
            color: None,
            at: vec![4],
        }];
        assert_eq!(
            linked("see #off\\-topic", &mentions, 0x112233),
            "see <font color=\"#112233\">[#off\\-topic](<yapfast:channel/discord:1:2>)</font>"
        );
    }

    #[test]
    fn editing_writes_a_channels_name_as_typed() {
        let channel = Mentioned {
            text: "#off\\-topic".into(),
            target: Target::Channel("discord:1:2".into()),
            color: None,
            at: vec![0, 16],
        };
        let role = Mentioned {
            text: "@a\\-b".into(),
            target: Target::Role("3".into()),
            color: None,
            at: vec![28],
        };
        let body = "#off\\-topic 1\\* #off\\-topic @a\\-b";
        assert_eq!(
            as_typed(body, &[channel, role]),
            "#off-topic 1\\* #off-topic @a\\-b",
            "the rest stays as it is"
        );
    }

    #[test]
    fn marks_what_goes_out_as_a_mention() {
        let candidates = [
            person("Ann", "<@1>"),
            Candidate {
                color: Some(0x3498db),
                group: true,
                ..person("Mods", "<@&20>")
            },
        ];
        assert_eq!(
            marks("café @ann, ask @Mods but `@Ann` or @Annie", &candidates),
            [(6..10, None), (16..21, Some(0x3498db))]
        );
        assert!(marks("no one", &candidates).is_empty());
    }
}
