//! Samples of the account card and its popover. Task N06's.

use iced::Color;

use crate::account::{Account, Choice, Game, Popover, Profile};
use crate::kind::{Expected, Presence, Status};

use super::avatar;
use super::pictures::{badge, pictured};

/// Alice, signed in with Discord linked, as the fake has her: her custom
/// status's emoji arrived.
pub fn account() -> Account {
    let status = Status {
        text: "Testing Yapfast".into(),
        emoji: Expected::Ready(badge("bandage", 32)),
        ..Status::default()
    };
    Account {
        name: "Alice".into(),
        status: status.clone(),
        avatar: pictured(avatar("A", 0x7a5fc9), "Alice"),
        username: "alice".into(),
        user_id: "1400000000000000001".into(),
        linked: true,
        presence: Presence::Online,
        online: true,
        ready: true,
        popover: Popover {
            custom: Some(status),
            profile: Profile {
                accent: Some(Color::from_rgb8(0xb8, 0xa8, 0x8a)),
                ..Profile::default()
            },
            ..Popover::default()
        },
        ..Account::default()
    }
}

/// Her microphone muted and her headphones off, in a call, with the
/// pointer's username not shown.
pub fn muted() -> Account {
    Account {
        muted: true,
        deafened: true,
        in_call: true,
        ..account()
    }
}

/// Not linked to Discord: the preview's card, local only.
pub fn preview() -> Account {
    Account {
        status: Status {
            text: "Local only".into(),
            ..Status::default()
        },
        username: String::new(),
        user_id: String::new(),
        linked: false,
        presence: Presence::None,
        preview: true,
        online: false,
        popover: Popover::default(),
        ..account()
    }
}

/// Offline: sync is paused, with the reconnect button.
pub fn offline() -> Account {
    Account {
        status: Status {
            text: "Offline, sync paused".into(),
            ..Status::default()
        },
        presence: Presence::None,
        online: false,
        ..account()
    }
}

/// The popover with the four statuses unfolded, Idle chosen, and the game
/// they play.
pub fn unfolded() -> Account {
    let mut account = account();
    account.popover.expanded = true;
    account.popover.choice = Choice::Idle;
    account.popover.game = Some(Game {
        name: "Factorio".into(),
        duration: "2 hours".into(),
    });
    account
}

/// The popover once their profile came: a theme, a styled name, a server
/// tag and badges.
pub fn themed() -> Account {
    let mut account = account();
    let profile = &mut account.popover.profile;
    profile.accent = None;
    profile.theme = Some([
        Color::from_rgb8(0x2d, 0x1b, 0x69),
        Color::from_rgb8(0x8a, 0x3f, 0xb8),
    ]);
    profile.tag = Some(super::kind::bob_tag());
    profile.badges = super::people::badges::few();
    account
}
