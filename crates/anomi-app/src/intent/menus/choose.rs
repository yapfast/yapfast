//! A row of the window's menu chosen, by the id its [`Choice`] carries:
//! `choose` in `src/ui/menus.rs:891`, carried out as the Slint window does.

use iced::Task;
use yapfast_core::application::{
    Action, ChannelAsk, MenuChoice, OwnershipInput, PersonAsk, network_id, network_link,
};
use yapfast_core::chat::{ChannelKind, MemberChange, Muting, RelationshipChange};

use anomi_vm::menu::{Choice, Intent, Menu, Row};
use anomi_widgets::focus;

use crate::app::App;
use crate::message::Message;
use crate::project::menus::choice_of;
use crate::project::menus::notify::{Notify, muting, server_change};
use crate::project::menus::subs::{
    APP, BELL, EVENTS, HEADER, INVITE_TO, MOVE_TO, ROLE, relationship_place, subjects, thread_more,
    voice_target,
};

/// Carries out the choice a row of the window's menu made.
pub(super) fn choose(app: &mut App, choice: Choice) -> Task<Message> {
    let Choice { target, person, id } = choice;
    // The events popout only lists them.
    if target.starts_with(EVENTS) {
        return Task::none();
    }
    // A folder's own: Move…'s rows, its settings, closing every folder.
    if let Some(task) = crate::intent::rail::folder::choose(app, &target, &id) {
        return task;
    }
    // A checkbox shows its new state at once, as the menu stays open.
    let ticked = toggle(&mut app.vm.menu, &id);
    // A profile's More menu is its person's, said to come from the profile.
    let (target, relationship) = {
        let (user, location) = relationship_place(&target);
        (user.to_owned(), location)
    };
    // Someone's menu in a voice channel acts on them in its server.
    let in_voice = voice_target(&target).map(|(room, user)| (room.to_owned(), user.to_owned()));
    let target = match &in_voice {
        Some((_, user)) => user.clone(),
        None => target,
    };
    let target = target
        .strip_prefix(BELL)
        .map_or(target.clone(), str::to_owned);
    let (target, location) = match thread_more(&target) {
        Some((room, location)) => (room.to_owned(), location),
        None => (target, "Context Menu"),
    };
    if let Some(change) = Notify::of(&app.core, &target).and_then(|notify| notify.change(&id)) {
        return app.dispatch(Action::ChangeNotifications {
            place: target,
            change,
        });
    }
    let target = target
        .strip_prefix(HEADER)
        .map_or(target.clone(), str::to_owned);
    let server = (app.core.state.servers.iter()).any(|server| server.id == target);
    if server && let Some(change) = server_change(&app.core, &target, &id) {
        return app.dispatch(Action::ChangeNotifications {
            place: target,
            change,
        });
    }
    let state = &app.core.state;
    let (room, user) = subjects(state, &target, person);
    // A member's choices act in the server of the channel they're in, or
    // of the settings listing them.
    #[cfg(feature = "discord")]
    let listed = app.core.member_place(&target).map(str::to_owned);
    #[cfg(not(feature = "discord"))]
    let listed = None;
    let place = in_voice
        .map(|(room, _)| room)
        .or(listed)
        .or_else(|| state.selected.clone());
    if let Some(to) = id.strip_prefix(MOVE_TO) {
        // This account moves by joining there; see `Application::move_to`.
        let action = place
            .as_deref()
            .and_then(|place| app.core.move_to(place, &target, to));
        return action.map_or_else(Task::none, |action| app.dispatch(action));
    }
    if let (Some(role), Some(add), Some(place)) = (id.strip_prefix(ROLE), ticked, place.clone()) {
        let change = MemberChange::Role {
            role: role.to_owned(),
            add,
        };
        return app.dispatch(Action::ChangeMember {
            place,
            user: target,
            change,
        });
    }
    if let (Some(server), Some(user)) = (id.strip_prefix(INVITE_TO), user.clone()) {
        let server = server.to_owned();
        return app.dispatch(Action::InviteToServer { user, server });
    }
    if let (Some(command), Some(user)) = (id.strip_prefix(APP), user.clone()) {
        let command = command.to_owned();
        return app.dispatch(Action::RunAppCommand {
            command,
            target: Some(user),
        });
    }
    if let Some(muting) = muting(&id) {
        return match room {
            Some(room) => app.dispatch(Action::MuteDm { room, muting }),
            None => Task::none(),
        };
    }
    let Some(choice) = choice_of(&id) else {
        return Task::none();
    };
    let subject = Subject {
        target,
        person,
        room,
        user,
        place,
        relationship,
        location,
        ticked,
    };
    act(app, choice, subject)
}

/// What a choice is about.
struct Subject {
    /// The person, conversation, server or thread, its prefixes gone.
    target: String,
    person: bool,
    room: Option<String>,
    user: Option<String>,
    /// Where a member's choices act.
    place: Option<String>,
    /// Where Discord's client says a relationship's and a thread's change
    /// came from.
    relationship: &'static str,
    location: &'static str,
    /// A checkbox's new state.
    ticked: Option<bool>,
}

fn act(app: &mut App, choice: MenuChoice, subject: Subject) -> Task<Message> {
    use MenuChoice as C;
    let Subject {
        target,
        person,
        room,
        user,
        place,
        relationship,
        location,
        ticked,
    } = subject;
    let ask = |ask| {
        user.clone().map(|user| Action::AskPerson {
            user,
            ask,
            location: relationship,
        })
    };
    let change = |change| {
        user.clone().map(|user| Action::ChangeRelationship {
            user,
            change,
            location: relationship,
        })
    };
    let ask_member = |ask| {
        place.clone().map(|place| Action::AskMember {
            place,
            user: target.clone(),
            ask,
        })
    };
    let member = |change| {
        place.clone().map(|place| Action::ChangeMember {
            place,
            user: target.clone(),
            change,
        })
    };
    let action = match choice {
        C::MarkRead => Some(Action::MarkRead(target)),
        C::JoinVoice => Some(Action::RequestJoinVoice(target)),
        C::OpenChat => Some(Action::OpenChat(target)),
        C::Call if person => ask(PersonAsk::Call),
        C::Call => room.map(Action::RequestJoinVoice),
        // Discord's Profile opens the full modal, not the popout.
        C::Profile if person => Some(Action::ShowProfile {
            user: target,
            large: true,
        }),
        C::Profile => Some(Action::ShowDmProfile(target)),
        C::Message => Some(Action::MessagePerson(target)),
        C::AddFriend | C::AcceptFriend => user.map(Action::Befriend),
        C::CancelFriendRequest => user.map(Action::RemoveFriendRequest),
        C::Pin | C::Unpin => room.map(|room| Action::Pin {
            room,
            pinned: choice == C::Pin,
        }),
        C::AddNote => ask(PersonAsk::Note),
        C::AddNickname | C::EditNickname => ask(PersonAsk::Nickname),
        C::CloseDm => room.map(Action::CloseDm),
        C::LeaveGroup => room.map(Action::AskLeaveGroup),
        C::Unfriend => ask(PersonAsk::Unfriend),
        C::Block => ask(PersonAsk::Block),
        C::Ignore => change(RelationshipChange::Ignore),
        C::Unignore => change(RelationshipChange::Unignore),
        C::Unblock => change(RelationshipChange::Unblock),
        C::Unmute => room.map(|room| Action::MuteDm {
            room,
            muting: Muting::Off,
        }),
        C::CopyLink => return iced::clipboard::write(network_link(&target)).discard(),
        C::CopyId => return iced::clipboard::write(network_id(&target).to_owned()).discard(),
        // The mention goes in the composer, which takes the keys with its
        // caret at the end.
        C::Mention => {
            let task = app.dispatch(Action::MentionPerson(target));
            return Task::batch([task, focus::into(focus::COMPOSER)]);
        }
        C::ServerNickname => ask_member(PersonAsk::ServerNickname),
        C::Timeout => ask_member(PersonAsk::Timeout),
        C::Kick => ask_member(PersonAsk::Kick),
        C::Ban => ask_member(PersonAsk::Ban),
        C::TransferOwnership => Some(Action::Ownership(OwnershipInput::Transfer(target))),
        C::RemoveTimeout => member(MemberChange::Timeout {
            until_ms: None,
            reason: String::new(),
        }),
        C::LocalMute | C::SoundboardMute => {
            if let Some(muted) = ticked {
                mute_locally(app, choice == C::SoundboardMute, &target, muted);
            }
            None
        }
        C::SelfMute => ticked.map(|muted| Action::Voice(yapfast_core::voice::Command::Mute(muted))),
        C::SelfDeafen => {
            ticked.map(|deafened| Action::Voice(yapfast_core::voice::Command::Deafen(deafened)))
        }
        C::ServerMute => ticked.and_then(|mute| member(MemberChange::ServerMute(mute))),
        C::ServerDeafen => ticked.and_then(|deaf| member(MemberChange::ServerDeafen(deaf))),
        C::Disconnect => member(MemberChange::Move(None)),
        C::Invite => Some(Action::OpenInvite(target)),
        C::PinChannel | C::UnpinChannel => Some(Action::PinChannel {
            room: target,
            pinned: choice == C::PinChannel,
        }),
        C::AddToChannelList | C::RemoveFromChannelList => Some(Action::FollowChannel {
            room: target,
            follow: choice == C::AddToChannelList,
        }),
        // The channel's category goes, with every channel it lists.
        C::UnfollowCategory => {
            app.core
                .category_of(&target)
                .map(|category| Action::FollowChannel {
                    room: category,
                    follow: false,
                })
        }
        C::ShowAllChannels => ticked.map(|show| Action::ShowAllChannels {
            server: target,
            show,
        }),
        C::CreateChannel => Some(Action::NewChannel {
            place: target,
            kind: ChannelKind::Text,
        }),
        C::CreateCategory => Some(Action::NewChannel {
            place: target,
            kind: ChannelKind::Category,
        }),
        C::EditChannel => Some(Action::OpenChannelSettings(target)),
        C::DuplicateChannel => Some(Action::AskChannel(ChannelAsk::Duplicate(target))),
        C::DeleteChannel => Some(Action::AskChannel(ChannelAsk::Delete(target))),
        C::LeaveServer => Some(Action::AskLeaveServer(target)),
        C::JoinThread | C::LeaveThread => Some(Action::JoinThread {
            room: target,
            join: choice == C::JoinThread,
            location,
        }),
        // From a server's menu, or one's own in its member list.
        C::EditServerProfile if person => place.map(Action::EditServerProfile),
        C::EditServerProfile => Some(Action::EditServerProfile(target)),
        C::ServerSettings => Some(Action::OpenServerSettings(target)),
        // Their submenus' rows act; a server's notifications change above.
        C::Mute
        | C::Roles
        | C::InviteToServer
        | C::MoveTo
        | C::Apps
        | C::ChannelMute
        | C::ChannelNotifications
        | C::MuteServer
        | C::UnmuteServer
        | C::ServerNotifications
        | C::HideMuted => None,
    };
    action.map_or_else(Task::none, |action| app.dispatch(action))
}

/// Mutes someone's voice, or their soundboard, on this device alone, and
/// keeps it.
fn mute_locally(app: &mut App, soundboard: bool, person: &str, muted: bool) {
    if soundboard {
        yapfast_core::volume::mute_soundboard(person, muted);
    } else {
        yapfast_core::volume::mute(person, muted);
    }
    app.layout.edit(|saved| {
        let people = if soundboard {
            &mut saved.soundboard_muted_people
        } else {
            &mut saved.muted_people
        };
        if muted {
            people.insert(person.to_owned());
        } else {
            people.remove(person);
        }
    });
}

/// Ticks or unticks the box of the row choosing `id` in the menu showing,
/// as choosing it does while the menu stays open; whether it's ticked now,
/// when there was one.
fn toggle(menu: &mut Menu, id: &str) -> Option<bool> {
    fn flip(rows: &mut [Row], id: &str) -> Option<bool> {
        for row in rows {
            let chooses = matches!(&row.intent,
                Some(anomi_vm::Intent::Menu(Intent::Choose(choice))) if choice.id == id);
            if row.checkbox && chooses {
                row.checked = !row.checked;
                return Some(row.checked);
            }
            if let Some(ticked) = flip(&mut row.submenu, id) {
                return Some(ticked);
            }
        }
        None
    }
    flip(&mut menu.rows, id)
}
