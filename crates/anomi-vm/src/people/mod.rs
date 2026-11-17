//! Friends, Active Now, the member list, profiles, status and the profile
//! editor: one module each, the view-model and `Intent` of one task
//! (docs/iced/tasks.md). F01 keeps this file: its fields and variants are
//! wired for every module below, so a task fills only its own module.
//!
//! `badges` and `editor_preview` have a variant but no field: the profile
//! card's view-model holds a profile's badges, `editor::Editor` its preview.

pub mod active_now;
pub mod activity;
pub mod add_friend;
pub mod add_people;
pub mod badges;
pub mod cards;
pub mod editor;
pub mod editor_preview;
pub mod friends;
pub mod full_profile;
pub mod members;
pub mod name_style;
pub mod person;
pub mod profile;
pub mod requests;
pub mod status;

/// What friends, Active Now, the member list, profiles, status and the
/// profile editor show, by area.
#[derive(Clone, Debug, Default)]
pub struct People {
    pub friends: friends::Friends,
    pub requests: requests::Requests,
    pub add_friend: add_friend::AddFriend,
    pub active_now: active_now::ActiveNow,
    pub members: members::Members,
    pub activity: activity::Activity,
    pub profile: profile::Profile,
    pub full_profile: full_profile::FullProfile,
    pub status: status::Status,
    pub person: person::Person,
    pub cards: cards::Cards,
    pub add_people: add_people::AddPeople,
    pub editor: editor::Editor,
    pub name_style: name_style::NameStyle,
}

/// What friends, Active Now, the member list, profiles, status and the
/// profile editor ask for, by area.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Friends(friends::Intent),
    Requests(requests::Intent),
    AddFriend(add_friend::Intent),
    ActiveNow(active_now::Intent),
    Members(members::Intent),
    Activity(activity::Intent),
    Profile(profile::Intent),
    Badges(badges::Intent),
    FullProfile(full_profile::Intent),
    Status(status::Intent),
    Person(person::Intent),
    Cards(cards::Intent),
    AddPeople(add_people::Intent),
    Editor(editor::Intent),
    EditorPreview(editor_preview::Intent),
    NameStyle(name_style::Intent),
}

crate::parts!(Intent in People:
    Friends(friends::Intent),
    Requests(requests::Intent),
    AddFriend(add_friend::Intent),
    ActiveNow(active_now::Intent),
    Members(members::Intent),
    Activity(activity::Intent),
    Profile(profile::Intent),
    Badges(badges::Intent),
    FullProfile(full_profile::Intent),
    Status(status::Intent),
    Person(person::Intent),
    Cards(cards::Intent),
    AddPeople(add_people::Intent),
    Editor(editor::Intent),
    EditorPreview(editor_preview::Intent),
    NameStyle(name_style::Intent),
);
