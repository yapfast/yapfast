//! The people the invite dialog offers, as one virtual list: the server's
//! members under their foldable heading when a voice channel is invited
//! to, then friends and DMs under theirs; each row with its avatar, name
//! and username, and its Invite button, which says Sent once Discord took
//! it, or the reason by the name when Discord refused. Only the rows near
//! the view are built, and the app asks for the pictures of those in view.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{button, column, container, row, svg, text};
use iced::{Alignment, Fill, Length, Padding};
use iced_runtime::core::length::Constraint;

use anomi_theme::metrics::{AVATAR_ROW, SPACE_M, SPACE_S, SPACE_XS};
use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Presence;
use anomi_vm::places::invite::{Intent, Invite, Invitee, Sent};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::Id;
use anomi_widgets::icon::icon;
use anomi_widgets::list::list;

use super::frame::PADDING;
use yapfast_lang::{text_in, tr};

/// The list's id, which the app scrolls to its start as a search changes
/// the rows.
pub const LIST: &str = "invite-people";
/// A row's padding above and below its avatar, and a heading's chevron.
const ROW_PAD: f32 = 6.0;
const CHEVRON: f32 = 14.0;

/// One item of the list.
#[derive(Clone, Copy)]
enum Item<'a> {
    /// The server's members' heading, which folds them; friends' heading.
    Members,
    Friends,
    Person(&'a Invitee),
    /// A search that found no one.
    Nobody,
}

/// How many items each part of the list has, in order.
struct Shape {
    members_heading: usize,
    members: usize,
    friends_heading: usize,
    friends: usize,
    nobody: usize,
}

impl Shape {
    fn of(invite: &Invite) -> Self {
        let (members, friends) = (invite.members.len(), invite.friends.len());
        Self {
            members_heading: usize::from(members > 0),
            members: if invite.folded { 0 } else { members },
            friends_heading: usize::from(members > 0 && friends > 0),
            friends,
            nobody: usize::from(!invite.listed() && !invite.query.trim().is_empty()),
        }
    }

    fn count(&self) -> usize {
        self.members_heading + self.members + self.friends_heading + self.friends + self.nobody
    }

    fn at<'a>(&self, invite: &'a Invite, at: usize) -> Item<'a> {
        let mut at = at;
        if at < self.members_heading {
            return Item::Members;
        }
        at -= self.members_heading;
        if at < self.members {
            return Item::Person(&invite.members[at]);
        }
        at -= self.members;
        if at < self.friends_heading {
            return Item::Friends;
        }
        at -= self.friends_heading;
        match invite.friends.get(at) {
            Some(who) => Item::Person(who),
            None => Item::Nobody,
        }
    }
}

/// A row's identity, which stays with it as a search changes the rows.
fn key(item: Item<'_>) -> u64 {
    match item {
        Item::Members => 1,
        Item::Friends => 2,
        Item::Nobody => 3,
        Item::Person(who) => {
            let mut hasher = DefaultHasher::new();
            who.id.hash(&mut hasher);
            hasher.finish() | 4
        }
    }
}

/// The list, shrinking to its rows up to the room the card leaves it, the
/// foot keeping its own; none when it has no rows.
pub fn view(invite: &Invite, motion: Motion) -> Option<Element<'_, Intent>> {
    let count = Shape::of(invite).count();
    if count == 0 {
        return None;
    }
    let people = list(
        count,
        move |at| key(Shape::of(invite).at(invite, at)),
        move |at| item(Shape::of(invite).at(invite, at), invite.folded, motion),
        motion,
    )
    .id(Id::new(LIST))
    .height(Length::Fluid(Constraint::Max))
    .on_viewport(move |viewport| Intent::Seen(seen(invite, &viewport.keys)));
    Some(
        container(people)
            .height(Length::Fluid(Constraint::Max))
            // The rows' faces line up with the field's edges above them.
            .padding(Padding::from([0.0, PADDING - SPACE_S]).bottom(SPACE_S))
            .boxed(),
    )
}

/// The user ids of the people whose rows have these keys.
fn seen(invite: &Invite, keys: &[u64]) -> Vec<String> {
    let members = invite.members.iter().filter(|_| !invite.folded);
    members
        .chain(&invite.friends)
        .filter(|who| keys.contains(&key(Item::Person(who))))
        .map(|who| who.id.clone())
        .collect()
}

fn item<'a>(item: Item<'a>, folded: bool, motion: Motion) -> Element<'a, Intent> {
    match item {
        Item::Members => heading(tr!("Server members"), Some(folded)),
        Item::Friends => heading(tr!("Friends"), None),
        Item::Person(who) => person(who, motion),
        Item::Nobody => container(typography::label(tr!("No one found.")))
            .padding(SPACE_S)
            .width(Fill)
            .boxed(),
    }
}

/// A group's heading: the members' folds them, its chevron pointing the
/// way they are.
fn heading(label: &str, folded: Option<bool>) -> Element<'_, Intent> {
    let words = text(label)
        .size(typography::size::CAPTION)
        .font(typography::SEMIBOLD)
        .style(catalog::text::muted);
    let padding = Padding::from([SPACE_S, SPACE_S]).bottom(SPACE_XS);
    let Some(folded) = folded else {
        return container(words).padding(padding).width(Fill).boxed();
    };
    let chevron = icon(
        if folded {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        },
        CHEVRON,
    )
    .style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().muted),
    });
    let line = row![words, chevron]
        .spacing(SPACE_XS)
        .align_y(Alignment::Center);
    button(line)
        .padding(padding)
        .style(catalog::button::bare)
        .on_press(Intent::Fold)
        .boxed()
}

/// Someone to invite: their avatar, name, username (or why Discord refused
/// the invite), and Invite, which waits while it goes and says Sent after.
fn person(who: &Invitee, motion: Motion) -> Element<'_, Intent> {
    let face = avatar(&who.avatar, AVATAR_ROW, Presence::None, Ground::Panel);
    let under = match &who.sent {
        Sent::Failed(why) => text(why.as_str())
            .size(typography::size::CAPTION)
            .style(catalog::text::danger),
        _ => text(who.username.as_str())
            .size(typography::size::CAPTION)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    };
    let names = column![
        text(who.name.as_str())
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        under,
    ]
    .width(Fill);
    let label = match who.sent {
        Sent::Sent => text_in("InviteDialog", "Sent"),
        _ => text_in("InviteDialog", "Invite"),
    };
    let mut invite = action_button(label, Kind::Secondary, motion)
        .small()
        .enabled(who.sent.may_send());
    if who.sent.may_send() {
        invite = invite.on_press(Intent::Send(who.id.clone()));
    }
    row![face, names, invite.boxed()]
        .spacing(SPACE_M)
        .align_y(Alignment::Center)
        .padding([ROW_PAD, SPACE_S])
        .boxed()
}
