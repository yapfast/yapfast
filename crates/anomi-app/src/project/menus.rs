//! Every menu's rows: a server's, a folder's ([`folder`]), a conversation's,
//! a category's, a person's, the server header's dropdown, its events, the
//! header's bell and a thread's More (`menu`, `src/ui/menus.rs:110`), and the
//! message's facts (`message`). The rows are built as a menu opens, never on
//! a refresh: a closed menu costs nothing. A person's menu opened before
//! the open room's apps came is built again once they have
//! ([`show`]), as the Slint window's `changed app-commands-ready` does.
//!
//! Each row sends the [`Choice`] `src/ui/menus.rs` reads by id, about the
//! menu's target as it opened (`header:` and the server, `in-voice:` and
//! the channel and the person...); `intent::menus` carries it out.

pub(crate) mod folder;
mod label;
pub(crate) mod message;
pub(crate) mod notify;
pub(crate) mod subs;
#[cfg(test)]
pub(crate) mod tests;

use std::cell::OnceCell;

use yapfast_core::application::{Application, MenuChoice};
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::menu::{Choice, Row};
use anomi_vm::sidebar::Place;

pub(crate) use label::{choice_id, choice_of};

use self::notify::{Notify, level_name, mute_id, muted_until};
use self::subs::{BELL, EVENTS, HEADER, relationship_place, subjects, thread_more, voice_target};
use crate::layout::Layout;

/// The window's menu open, what it is about, and whether its Apps were
/// still to come as it opened.
#[derive(Default)]
pub(crate) struct Shown {
    /// Counts the menus opened, so each arrives anew.
    pub(crate) serial: u64,
    /// The target its rows were made for, and whether it's a person's.
    pub(crate) target: String,
    pub(crate) person: bool,
    /// A person's menu opened before the open room's apps came.
    pub(crate) apps_due: bool,
    /// The message menu's Apps were still to come as it opened.
    pub(crate) message_apps_due: bool,
    /// The link a right press in a message's text was on, by the message's
    /// key, for the menu opening there next.
    pub(crate) link: Option<(u64, anomi_vm::text::Link)>,
    /// The selection a right press found its row in, which the message
    /// menu opening next offers to copy (`selection-at`).
    pub(crate) selection: Option<String>,
    /// Where the message menu opened, in a window how large.
    pub(crate) message_spot: Option<(anomi_vm::menu::Spot, iced::Size)>,
    /// The link the message menu opened on is masked: its Open Link asks
    /// before leaving, as a click on it does.
    pub(crate) message_masked: bool,
}

/// Makes the rows of the menu about `target`, each sending its choice.
pub(crate) struct Make<'a> {
    target: &'a str,
    person: bool,
}

impl Make<'_> {
    fn choice(&self, id: &str) -> Choice {
        if self.person {
            Choice::person(self.target, id)
        } else {
            Choice::new(self.target, id)
        }
    }

    /// A row choosing `id`.
    pub(crate) fn row(&self, text: &str, id: &str) -> Row {
        Row::new(text, self.choice(id).into())
    }
}

/// What a server's, conversation's or person's menu offers; nothing for
/// one Yapfast doesn't know.
pub(crate) fn rows(core: &Application, layout: &Layout, target: &str, person: bool) -> Vec<Row> {
    let make = Make { target, person };
    let twelve = layout.twelve_hour_clock;
    if let Some(place) = target.strip_prefix(BELL) {
        return Notify::of(core, place).map_or_else(Vec::new, |notify| notify.bell(&make, twelve));
    }
    if let Some(server) = target.strip_prefix(EVENTS) {
        return subs::events(&core.state, &make, server, twelve);
    }
    if !person && folder::is_folder(&core.state, target) {
        return folder::rows(core, layout, &make, target);
    }
    let state = &core.state;
    let (id, _) = relationship_place(target);
    let in_voice = voice_target(id);
    let header = id.starts_with(HEADER);
    let thread = thread_more(id);
    let mut choices = if let Some((room, user)) = in_voice {
        core.voice_person_choices(room, user)
    } else if let Some((room, _)) = thread {
        core.thread_more_choices(room)
    } else if person {
        core.person_choices(id)
    } else if let Some(server) = id.strip_prefix(HEADER) {
        core.header_choices(server)
    } else {
        core.menu_choices(id)
    };
    // Discord's menus have an icon on Copy ID alone; a person's also take
    // its order. A call's and a thread's keep theirs.
    let plain = thread.is_none() && in_voice.is_none();
    if plain && person {
        label::discord_order(&mut choices);
    }
    let id = match (in_voice, thread) {
        (Some((_, user)), _) => user,
        (None, Some((room, _))) => room,
        (None, None) => id.strip_prefix(HEADER).unwrap_or(id),
    };
    let voice_person = in_voice.and_then(|(room, user)| core.voice_person(room, user));
    // Moderation names whom it's about, as Discord's does.
    let named = OnceCell::new();
    let name = || {
        named
            .get_or_init(|| {
                state
                    .person(id)
                    .map_or_else(|| tr!("them").to_owned(), |person| person.name)
            })
            .clone()
    };
    let (room, user) = subjects(state, id, person);
    let in_call = state
        .rooms
        .iter()
        .any(|found| Some(&found.id) == room.as_ref() && !found.people.is_empty());
    let server = state.servers.iter().any(|server| server.id == id);
    let category = !server && state.is_category(id);
    let voice = state
        .rooms
        .iter()
        .any(|found| found.id == id && found.voice);
    let notifications = OnceCell::new();
    let notifications = || *notifications.get_or_init(|| core.server_notifications(id));
    let mut group = None;
    let mut rows: Vec<(MenuChoice, Row)> = Vec::with_capacity(choices.len() + 1);
    // Copy ID shows only with Developer Mode on, as in Discord.
    for choice in choices
        .into_iter()
        .filter(|choice| layout.developer_mode || *choice != MenuChoice::CopyId)
    {
        use MenuChoice as C;
        let (text, icon) = match choice {
            C::Timeout => (tr!("Time out {}", name()), anomi_vm::icon::Icon::Clock),
            C::Kick => (tr!("Kick {}", name()), anomi_vm::icon::Icon::UserX),
            C::Ban => (tr!("Ban {}", name()), anomi_vm::icon::Icon::Gavel),
            C::Invite if server => (
                tr!("Invite to server").to_owned(),
                label::label(C::InviteToServer, false).1,
            ),
            C::CreateChannel if server => (
                tr!("Create channel").to_owned(),
                label::label(choice, false).1,
            ),
            C::CreateChannel if voice => (
                tr!("Create voice channel").to_owned(),
                label::label(choice, false).1,
            ),
            C::EditChannel if category => (
                tr!("Edit category").to_owned(),
                label::label(choice, false).1,
            ),
            C::DeleteChannel if category => (
                tr!("Delete category").to_owned(),
                label::label(choice, false).1,
            ),
            choice => {
                let (text, icon) = label::label(choice, in_call);
                (text.to_owned(), icon)
            }
        };
        let starts = label::group(choice);
        let separated = group.replace(starts).is_some_and(|last| last != starts);
        let mut row = make.row(&text, choice_id(choice));
        row.icon = (!plain || choice == C::CopyId).then_some(icon);
        row.plain = plain;
        row.separated = separated;
        row.danger = label::danger(choice);
        row.subtitle = match choice {
            C::AddNote => tr!("Only visible to you").to_owned(),
            C::UnmuteServer => notifications()
                .and_then(|known| known.own.muted_until)
                .map(|until| muted_until(until / 1000, twelve))
                .unwrap_or_default(),
            C::ServerNotifications => notifications()
                .map(|known| level_name(known.own.level.unwrap_or(known.own.inherited)).to_owned())
                .unwrap_or_default(),
            _ => String::new(),
        };
        if label::checkbox(choice) {
            row = row.checkbox(match choice {
                C::ShowAllChannels => {
                    notifications().is_some_and(|known| known.show_all == Some(true))
                }
                C::HideMuted => notifications().is_some_and(|known| known.hide_muted),
                C::LocalMute => yapfast_core::volume::muted(id),
                C::SoundboardMute => yapfast_core::volume::soundboard_muted(id),
                C::SelfMute => state.voice.muted,
                C::SelfDeafen => state.voice.deafened,
                C::ServerMute => voice_person.is_some_and(|person| person.server_muted),
                _ => voice_person.is_some_and(|person| person.server_deafened),
            });
        }
        let submenu = match choice {
            C::Mute | C::MuteServer => notify::mute_times(&make),
            C::ServerNotifications => notify::server_rows(core, &make, id),
            C::Roles => subs::roles(core, &make, id),
            // A person's run in the open room; a DM's in the DM.
            C::Apps => subs::apps(
                core,
                &make,
                if person {
                    state.selected.as_deref()
                } else {
                    Some(id)
                },
            ),
            C::InviteToServer => subs::invite(core, &make, user.as_deref()),
            C::MoveTo => match in_voice {
                Some((room, user)) => subs::move_to(core, &make, room, user),
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        if !submenu.is_empty() {
            // A mute row clicked itself mutes until it's turned back on, as
            // Discord's does; its submenu offers the times.
            row.intent = matches!(choice, C::Mute | C::MuteServer).then(|| {
                make.choice(&mute_id(yapfast_core::chat::Muting::Always))
                    .into()
            });
            row.submenu = submenu;
        }
        rows.push((choice, row));
    }
    // A conversation's and a server's menu always starts with Mark as
    // read, greyed when there is nothing to read.
    let listed = !person
        && plain
        && !state.preview
        && !header
        && (server || state.rooms.iter().any(|found| found.id == id));
    if listed
        && !rows
            .iter()
            .any(|(choice, _)| *choice == MenuChoice::MarkRead)
    {
        let mark = make
            .row(tr!("Mark as read"), choice_id(MenuChoice::MarkRead))
            .disabled(true);
        rows.insert(0, (MenuChoice::MarkRead, Row { plain, ..mark }));
        if let Some((_, next)) = rows.get_mut(1) {
            next.separated = true;
        }
    }
    if let Some(notify) = Notify::of(core, id) {
        for (choice, row) in &mut rows {
            notify.dress(&make, row, *choice, twelve);
        }
    }
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The window's menu as the state goes on: a person's opened before the
/// open room's apps came gets its Apps once they have; the server's header
/// shows its menu open while it is.
pub(crate) fn show(core: &Application, layout: &Layout, vm: &mut Vm, shown: &mut Shown) {
    let state = &core.state;
    if shown.apps_due && vm.menu.open {
        let loaded = state
            .selected
            .as_deref()
            .is_some_and(|room| state.app_commands.index(room).is_some());
        if loaded {
            shown.apps_due = false;
            vm.menu.rows = rows(core, layout, &shown.target, shown.person);
        }
    }
    if shown.message_apps_due && message::apps_came(core, vm, shown.message_spot) {
        shown.message_apps_due = false;
    }
    header_open(vm, shown);
}

/// The server header's chevron turns while its menu is open.
pub(crate) fn header_open(vm: &mut Vm, shown: &Shown) {
    let open = vm.menu.open && shown.target.starts_with(HEADER);
    if let Place::Server(server) = &mut vm.sidebar.place
        && server.header.open != open
    {
        server.header.open = open;
        vm.sidebar.version += 1;
    }
}
