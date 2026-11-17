//! Samples of the account card and its popover. Task N06's.

use iced::Color;

use crate::account::{Account, Choice, Game, Intent, Popover, Profile, Rows};
use crate::kind::{Expected, Presence, Status};

use super::avatar;
use super::pictures::pictured;

/// Alice, signed in with Discord linked, as the fake has her: her custom
/// status's emoji arrived.
pub fn account() -> Account {
    let status = Status {
        // The fake sets the Unicode test tube, which `status_words` turns into
        // words with no picture (`src/ui/profile.rs:221`).
        text: "\u{1f9ea} Testing Yapfast".into(),
        emoji: Expected::None,
        ..Status::default()
    };
    settled(Account {
        name: "Alice".into(),
        status: status.clone(),
        avatar: pictured(avatar("A", 0x7a5fc9), "Alice"),
        decoration: Some(decoration()),
        username: "alice".into(),
        user_id: "1400000000000000001".into(),
        linked: true,
        presence: Presence::Online,
        online: true,
        ready: true,
        popover: Popover {
            custom: Some(status),
            profile: Profile {
                accent: Some(Color::from_rgb8(0xae, 0x9c, 0x83)),
                ..Profile::default()
            },
            ..Popover::default()
        },
        ..Account::default()
    })
}

/// `account` with its popover's rows made from what it says, as the
/// projection does; a test that changes the chosen status, the custom status
/// or the id calls it again.
pub fn settled(mut account: Account) -> Account {
    use crate::icon::Icon;
    use crate::menu::Row;
    use yapfast_lang::tr;
    let row = |icon, text: &str, intent| Row {
        icon,
        text: text.into(),
        intent: Some(intent),
        ..Row::default()
    };
    let popover = &mut account.popover;
    let custom = if popover.custom.is_some() {
        tr!("Change custom status")
    } else {
        tr!("Set a custom status")
    };
    let chosen = match popover.choice {
        Choice::Online => tr!("Online"),
        Choice::Idle => tr!("Idle"),
        Choice::DoNotDisturb => tr!("Do Not Disturb"),
        Choice::Invisible => tr!("Invisible"),
    };
    popover.rows = Rows {
        edit: row(Some(Icon::Pencil), tr!("Edit Profile"), Intent::EditProfile),
        // Its dot is drawn over the room an icon would have.
        status: row(None, chosen, Intent::ToggleStatuses),
        custom: row(Some(Icon::SmilePlus), custom, Intent::CustomStatus),
        copy: (!account.user_id.is_empty())
            .then(|| row(Some(Icon::Copy), tr!("Copy User ID"), Intent::CopyId)),
        settings: row(Some(Icon::Settings), tr!("User Settings"), Intent::Settings),
    };
    account
}

/// Her decoration, as the golden screenshots show it: a pink ring on
/// nothing, 96 px wide, which her avatar sits inside. Made once a thread, so
/// its handle keeps its id.
fn decoration() -> iced::widget::image::Handle {
    use iced::widget::image::Handle;
    thread_local! {
        static RING: Handle = make_ring();
    }
    RING.with(Handle::clone)
}

fn make_ring() -> iced::widget::image::Handle {
    const SIDE: u32 = 96;
    let (middle, outer, inner) = (SIDE as f32 / 2.0, 47.0_f32, 41.0_f32);
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let distance =
                ((x as f32 + 0.5 - middle).powi(2) + (y as f32 + 0.5 - middle).powi(2)).sqrt();
            let alpha = ((outer - distance).clamp(0.0, 1.0)
                * (distance - inner).clamp(0.0, 1.0)
                * 255.0) as u8;
            pixels.extend([255, 105, 180, alpha]);
        }
    }
    iced::widget::image::Handle::from_rgba(SIDE, SIDE, pixels)
}

/// Her microphone muted and her headphones off, in a call, with the
/// pointer's username not shown.
pub fn muted() -> Account {
    settled(Account {
        muted: true,
        deafened: true,
        in_call: true,
        ..account()
    })
}

/// Not linked to Discord: the preview's card, local only.
pub fn preview() -> Account {
    settled(Account {
        status: Status {
            text: yapfast_lang::tr!("Local only").into(),
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
    })
}

/// Offline: sync is paused, with the reconnect button.
pub fn offline() -> Account {
    settled(Account {
        status: Status {
            text: yapfast_lang::tr!("Offline, sync paused").into(),
            ..Status::default()
        },
        presence: Presence::None,
        online: false,
        ..account()
    })
}

/// The popover with the four statuses unfolded, Idle chosen, and the game
/// they play.
pub fn unfolded() -> Account {
    let mut account = account();
    account.popover.expanded = true;
    account.popover.choice = Choice::Idle;
    account.presence = Presence::Idle;
    account.popover.game = Some(Game {
        name: "Factorio".into(),
        duration: yapfast_lang::tr!("2 hours").into(),
    });
    settled(account)
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
    settled(account)
}
