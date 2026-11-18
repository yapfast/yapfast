//! A channel's threads from the core's state: the threads popover
//! (`show_threads`, `src/ui/refresh.rs:1921`, with `push_section` and
//! `thread_row`, `:5660`, `:5677`), the "New Thread" form beside the
//! channel ([`new`]), and the thread beside it, which is a timeline of its
//! own (`crate::project::chat::timeline::aside`).
//!
//! The popover's sections (the joined threads, the other active ones, the
//! older ones a page at a time) or what a search of their names found are
//! built again only once they changed, a picture of theirs came, or a
//! minute passed for their times. Its rows go once it played its way out
//! ([`gone`]).

pub(crate) mod new;

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::Application;
use yapfast_core::chat::{ListedThread, Permission};
use yapfast_lang::{tr, trn};

use anomi_vm::chat::threads::{Person, Row, Thread, Threads};
use anomi_vm::kind::Avatar;

use crate::pictures::{self, Requests, Watch, noting};
use crate::project::rows;

/// The popover's search field, which takes the keys as it opens.
pub(crate) const QUERY: &str = "threads-query";

/// What the popover showed last.
#[derive(Default)]
pub(crate) struct Shown {
    open: bool,
    key: Option<u64>,
    pictures: Watch<()>,
}

pub(crate) fn show(
    core: &Application,
    threads: &mut Threads,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    // Closed: what it showed stays while it plays its way out.
    let Some(list) = state.thread_list() else {
        shown.open = false;
        return;
    };
    let searchable = core.may(&list.room, Permission::ReadHistory);
    if !std::mem::replace(&mut shown.open, true) {
        threads.query.clear();
        threads.focused = false;
        if searchable {
            *focus = Some(QUERY);
        }
    }
    threads.searchable = searchable;
    threads.more = list.more && list.search.is_none();
    threads.loading = list.loading;
    threads.can_create = {
        let (public, private) = core.thread_starts(&list.room);
        public || private
    };
    let sections = core.thread_sections().unwrap_or_default();
    let found = list.search.as_ref().map(|search| &search.found);
    let none = sections.joined.is_empty() && sections.other.is_empty() && sections.older.is_empty();
    // While a search shows, its results stand in for the sections.
    let (note, empty) = match found {
        Some(None) => (tr!("Searching...").to_owned(), false),
        Some(Some(Err(error))) => (error.clone(), false),
        Some(Some(Ok(found))) => (String::new(), found.is_empty()),
        None if none && list.loading => (tr!("Loading threads...").to_owned(), false),
        None if none => (list.error.clone().unwrap_or_default(), list.error.is_none()),
        None => (String::new(), false),
    };
    if threads.note != note {
        threads.note = note;
    }
    threads.empty = empty;
    let now = yapfast_core::clock::now_ms();
    let key = {
        let mut hasher = DefaultHasher::new();
        (&sections, found, now / 60_000).hash(&mut hasher);
        hasher.finish()
    };
    let moved = !shown.pictures.moved(state).is_empty();
    if shown.key == Some(key) && !moved {
        return;
    }
    shown.key = Some(key);
    threads.rows = noting(state, &mut shown.pictures, || {
        let mut rows = Vec::new();
        match found {
            Some(Some(Ok(found))) => {
                rows.extend(found.iter().map(|thread| thread_row(core, thread, now)));
            }
            _ => {
                let headed = [(&sections.joined, true), (&sections.other, false)];
                for (threads, joined) in headed {
                    let count = threads.len();
                    let heading = if joined {
                        trn!("{n} joined thread", "{n} joined threads", count)
                    } else {
                        trn!("{n} other active thread", "{n} other active threads", count)
                    };
                    section(core, &mut rows, threads, heading.to_uppercase(), now);
                }
                let older = tr!("OLDER THREADS").to_owned();
                section(core, &mut rows, &sections.older, older, now);
            }
        }
        rows
    });
    // The pictures of the people the rows show.
    let listed: Vec<&ListedThread> = match found {
        Some(Some(Ok(found))) => found.iter().collect(),
        _ => [&sections.joined, &sections.other, &sections.older]
            .into_iter()
            .flatten()
            .collect(),
    };
    let people = listed.into_iter().flat_map(|thread| {
        let person = match (&thread.recent, &thread.owner, thread.archived) {
            (Some(recent), _, false) => Some(&recent.author),
            (_, owner, true) => owner.as_ref(),
            _ => None,
        };
        (person.map(|person| person.avatar.as_str()).into_iter())
            .chain(thread.member_avatars.iter().map(String::as_str))
    });
    requests.want(state, people);
}

/// The popover played its way out: its rows go.
pub(crate) fn gone(threads: &mut Threads, shown: &mut Shown) {
    *threads = Threads::default();
    *shown = Shown::default();
}

/// A section: its heading over its first thread, if it has any.
fn section(
    core: &Application,
    rows: &mut Vec<Row>,
    threads: &[ListedThread],
    heading: String,
    now: u64,
) {
    if threads.is_empty() {
        return;
    }
    rows.push(Row::Heading(heading));
    rows.extend(threads.iter().map(|thread| thread_row(core, thread, now)));
}

/// A thread as its list shows it: an active one with its newest message,
/// an archived one with who started it, as Discord's client does.
fn thread_row(core: &Application, thread: &ListedThread, now: u64) -> Row {
    let state = &core.state;
    let ago = ago(thread.active_ms, now);
    let (person, lead, text, time) = match (&thread.recent, &thread.owner, thread.archived) {
        (Some(recent), _, false) => (Some(&recent.author), "", recent.text.clone(), ago),
        (None, _, false) => (None, "", tr!("No recent messages").to_owned(), ago),
        (_, Some(owner), true) => (
            Some(owner),
            tr!("Started by "),
            String::new(),
            tr!("Last active {}", ago),
        ),
        (_, None, true) => (None, "", String::new(), tr!("Last active {}", ago)),
    };
    Row::Thread(Box::new(Thread {
        room: thread.room.clone(),
        name: thread.name.clone(),
        private: thread.private,
        archived: thread.archived,
        person: person.map(|person| Person {
            name: person.name.clone(),
            avatar: rows::avatar(state, &person.name, &person.name, Some(&person.avatar)),
            color: person.color.map(rows::rgb),
        }),
        lead: lead.to_owned(),
        text,
        time,
        members: thread.members,
        member_avatars: (thread.member_avatars.iter())
            .map(|address| Avatar {
                picture: pictures::find(state, Some(address)),
                ..Avatar::default()
            })
            .collect(),
    }))
}

/// How long ago something was, as Discord's list of threads says it: "5s
/// ago" up to "30d ago", then ">30d ago" and ">1y ago".
pub(crate) fn ago(at_ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        seconds if seconds < 60 => tr!("{}s ago", seconds),
        seconds if seconds < 3600 => tr!("{}m ago", seconds / 60),
        seconds if seconds < 86400 => tr!("{}h ago", seconds / 3600),
        seconds if seconds <= 30 * 86400 => tr!("{}d ago", seconds / 86400),
        seconds if seconds < 365 * 86400 => tr!(">30d ago").into(),
        _ => tr!(">1y ago").into(),
    }
}
