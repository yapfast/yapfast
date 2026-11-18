//! Text the core words once, as it receives it, in the language chosen then:
//! the line under a name that says what someone is doing ("Playing
//! Factorio"), the member list's group headings ("Online — 3") and the line
//! about where the token is kept. The core keeps only the words, so after a
//! switch they would stay in the old language until Discord sends them again.
//! Each language chosen in this run notes its words for these texts, and a
//! text one of them made is made again in the language chosen now.

use std::borrow::Cow;
use std::sync::OnceLock;

use yapfast_core::chat::Activities;
use yapfast_lang::LANGUAGES;

/// What the line under a name says for each kind of activity, by its English
/// (`yapfast_discord::activity::Activity::line`).
const DOING: [&str; 5] = [
    "Playing {}",
    "Streaming {}",
    "Listening to {}",
    "Watching {}",
    "Competing in {}",
];

/// The member list's group headings (`member_list` `heading`), which a count
/// may follow, then the line about the token's storage (`chat::run`).
const WORDS: [&str; 4] = [
    "Online",
    "Offline",
    "Members",
    "The Discord token is stored in the OS credential store.",
];

/// The headings among [`WORDS`].
const HEADINGS: std::ops::Range<usize> = 0..3;

/// One language's words for [`DOING`] and [`WORDS`].
struct Noted {
    doing: [&'static str; DOING.len()],
    words: [&'static str; WORDS.len()],
}

/// The words of each language chosen in this run, at its place in
/// [`LANGUAGES`].
static NOTED: [OnceLock<Noted>; 2] = [OnceLock::new(), OnceLock::new()];

/// Notes the chosen language's words: call it after each choice.
pub fn note() {
    let Some(chosen) = yapfast_lang::translating() else {
        return;
    };
    let at = LANGUAGES
        .iter()
        .position(|language| std::ptr::eq(language, chosen));
    if let Some(noted) = at.and_then(|at| NOTED.get(at)) {
        noted.get_or_init(|| Noted {
            doing: DOING.map(yapfast_lang::text),
            words: WORDS.map(yapfast_lang::text),
        });
    }
}

/// The words every noted language, and the code's own English, has for one
/// text: `pick` takes it from a language's [`Noted`].
fn each(
    english: &'static str,
    pick: impl Fn(&Noted) -> &'static str,
) -> impl Iterator<Item = &'static str> {
    let noted = NOTED.iter().filter_map(OnceLock::get).map(pick);
    std::iter::once(english).chain(noted)
}

/// A template with one `{}` filled with `value` gives `line`.
fn fills(template: &str, value: &str, line: &str) -> bool {
    template.split_once("{}").is_some_and(|(before, after)| {
        line.len() == before.len() + value.len() + after.len()
            && line.starts_with(before)
            && line.ends_with(after)
            && &line[before.len()..line.len() - after.len()] == value
    })
}

/// Where in [`DOING`] an activity's kind is, and the name its line says;
/// none for a Hang status, which has no line.
#[cfg(feature = "discord")]
fn doing(activity: &yapfast_core::chat::Activity) -> Option<(usize, &str)> {
    use yapfast_core::chat::ActivityKind as Kind;
    let named = &*activity.name;
    Some(match activity.kind {
        Kind::Playing => (0, named),
        Kind::Streaming => (1, activity.details.as_deref().unwrap_or(named)),
        Kind::Listening => (2, named),
        Kind::Watching => (3, named),
        Kind::Competing => (4, named),
        Kind::Hang => return None,
    })
}

/// Whether a line under a name is the one an activity makes, in any noted
/// language: it says only what the activity's card says more of.
pub fn names_an_activity(line: &str, activities: &Activities) -> bool {
    #[cfg(feature = "discord")]
    return activities.iter().filter_map(doing).any(|(at, value)| {
        each(DOING[at], |noted| noted.doing[at]).any(|template| fills(template, value, line))
    });
    #[cfg(not(feature = "discord"))]
    {
        let _ = (line, activities);
        false
    }
}

/// The line under a name in the language chosen now: one the core made from
/// the first activity in another is made again; a custom status stays.
pub fn line<'a>(activities: &Activities, line: Option<&'a str>) -> Option<Cow<'a, str>> {
    let line = line?;
    #[cfg(feature = "discord")]
    if let Some((first, (at, value))) = activities
        .iter()
        .find_map(|activity| Some((activity, doing(activity)?)))
    {
        let now = yapfast_lang::text(DOING[at]);
        let then = each(DOING[at], |noted| noted.doing[at]);
        if made_in_another(now, then, value, line) {
            return first.line().map(Cow::Owned);
        }
    }
    #[cfg(not(feature = "discord"))]
    let _ = activities;
    Some(Cow::Borrowed(line))
}

/// A line was filled with `value` from one of the templates `then` and not
/// from the template `now`.
fn made_in_another<'a>(
    now: &str,
    mut then: impl Iterator<Item = &'a str>,
    value: &str,
    line: &str,
) -> bool {
    !fills(now, value, line) && then.any(|template| fills(template, value, line))
}

/// A member list heading in the language chosen now, its count kept.
pub fn heading(heading: &str) -> Cow<'_, str> {
    HEADINGS
        .filter_map(|at| {
            let now = yapfast_lang::text(WORDS[at]);
            heading_in(heading, now, each(WORDS[at], |noted| noted.words[at]))
        })
        .next()
        .map_or(Cow::Borrowed(heading), Cow::Owned)
}

/// A heading that starts with one of the words `then`, worded `now`.
fn heading_in<'a>(
    heading: &str,
    now: &str,
    mut then: impl Iterator<Item = &'a str>,
) -> Option<String> {
    then.find_map(|then| {
        let rest = heading.strip_prefix(then)?;
        (then != now && (rest.is_empty() || rest.starts_with(" — ")))
            .then(|| format!("{now}{rest}"))
    })
}

/// The line about the token's storage in the language chosen now.
pub fn storage(storage: &'static str) -> &'static str {
    let at = WORDS.len() - 1;
    if each(WORDS[at], |noted| noted.words[at]).any(|then| then == storage) {
        return yapfast_lang::text(WORDS[at]);
    }
    yapfast_lang::text(storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_place_to_note_its_words() {
        assert_eq!(NOTED.len(), LANGUAGES.len());
    }

    #[test]
    fn a_template_fills_only_with_its_value() {
        assert!(fills("Играет в {}", "Factorio", "Играет в Factorio"));
        assert!(fills("{} is on", "Bob", "Bob is on"));
        assert!(!fills("Играет в {}", "Factorio", "Играет в Chess"));
        assert!(!fills("Playing", "Factorio", "Playing Factorio"));
    }

    #[test]
    fn a_line_made_in_another_language_is_made_again() {
        let then = || ["Playing {}", "Играет в {}"].into_iter();
        assert!(made_in_another(
            "Playing {}",
            then(),
            "Factorio",
            "Играет в Factorio"
        ));
        // Already in the language chosen now, or a custom status.
        assert!(!made_in_another(
            "Playing {}",
            then(),
            "Factorio",
            "Playing Factorio"
        ));
        assert!(!made_in_another(
            "Playing {}",
            then(),
            "Factorio",
            "back at 5"
        ));
    }

    #[test]
    fn a_heading_keeps_its_count_in_the_language_chosen_now() {
        let then = || ["Online", "В сети"].into_iter();
        assert_eq!(
            heading_in("В сети — 3", "Online", then()).as_deref(),
            Some("Online — 3")
        );
        assert_eq!(
            heading_in("В сети", "Online", then()).as_deref(),
            Some("Online")
        );
        assert_eq!(heading_in("Online — 3", "Online", then()), None);
        // A role's name that only starts with the word is a role's.
        assert_eq!(heading_in("В сетиках — 2", "Online", then()), None);
    }

    /// Tests choose no language, so the code's English is the only one
    /// noted: a heading or line already in it stays as it is.
    #[test]
    fn text_in_the_language_chosen_now_stays() {
        assert!(matches!(heading("Online — 3"), Cow::Borrowed("Online — 3")));
        assert!(matches!(heading("Moderators — 2"), Cow::Borrowed(_)));
        let storage_line = WORDS[3];
        assert_eq!(storage(storage_line), storage_line);
        let none = Activities::from([]);
        assert_eq!(line(&none, Some("brb")).as_deref(), Some("brb"));
        assert_eq!(line(&none, None), None);
    }
}
