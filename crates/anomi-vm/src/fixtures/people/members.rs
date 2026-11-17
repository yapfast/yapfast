//! Samples of the member list for tests and shots, after the fake Discord's
//! data (`--members 30 --member-looks`): task P05's. Bob and four Members
//! are Mods, then Alice, Carol and the rest are online; Bob wears his tag
//! and the server's crown, Carol listens to music with her status. Pictures
//! are the fake's; plates and decorations that move are the shots' to add.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::kind::bob_tag;
use crate::fixtures::{avatar, pictures, pictures::pictured, shell};
use crate::kind::{Doing, Expected, NameStyle, Presence, Status, Tag};
use crate::menu::{Choice, Menu, Row as MenuRow, Spot};
use crate::people::members::{Heading, Member, Members, Row};

/// A member before their picture arrives, as `pictured` gives it.
pub fn member(id: &str, name: &str, tint: u32, presence: Presence) -> Member {
    Member {
        id: id.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        presence,
        ..Member::default()
    }
}

fn playing(game: &str) -> Status {
    Status {
        text: game.into(),
        doing: Doing::Playing,
        ..Status::default()
    }
}

fn words(text: &str) -> Status {
    Status {
        text: text.into(),
        ..Status::default()
    }
}

fn heading(text: String) -> Row {
    Row::Heading(Heading {
        text,
        icon: Expected::None,
    })
}

/// The Mods role's blue (the fake's role colour `3447003`).
const MODS: Color = Color::from_rgb(
    0x34 as f32 / 255.0,
    0x98 as f32 / 255.0,
    0xdb as f32 / 255.0,
);

/// A Mods member's name as a server draws it: the style's face alone, in
/// the role's blue (`font_only_in`; the fake's `nameStyleSample`).
fn mod_looks(member: &mut Member, font: u8) {
    member.color = Some(MODS);
    member.name_style = NameStyle::new(font, 1, &[0x3f8fd8]).map(NameStyle::font_only);
}

/// Carol's style, the fake's `users.carol.style`.
fn carol_style() -> Option<NameStyle> {
    NameStyle::new(3, 2, &[0xff7eb3, 0x7a5cff])
}

/// The moon the fake puts before a custom status, a Unicode one that stays
/// in the words.
fn moon(text: &str) -> Status {
    words(&format!("\u{1f319} {text}"))
}

/// Carol listening, her status with its moon.
fn carol_listening() -> Status {
    Status {
        text: "\u{1f319} back later after dinner, probably".into(),
        emoji: Expected::None,
        doing: Doing::Listening,
    }
}

/// The fake's nth listed `Member n` with `--member-looks`: its presence by
/// `n % 4` (a phone for 3), a custom status for `n % 3 == 1`, a game for
/// `n % 3 == 2`, a tag for `n % 5 == 2` (`lookPresence`, `memberLook`).
fn looked(n: usize) -> Member {
    let presence = [
        Presence::Online,
        Presence::Idle,
        Presence::DoNotDisturb,
        Presence::Mobile,
    ][n % 4];
    let mut member = member(
        &format!("discord:m{n}"),
        &format!("Member {n}"),
        0x3a9357,
        presence,
    );
    match n % 3 {
        1 => member.status = moon(&format!("status line {n}")),
        2 => member.status = playing(["Factorio", "Celeste", "Hades", "Balatro"][n % 4]),
        _ => {}
    }
    if n % 5 == 2 {
        member.tag = Some(Tag {
            text: ["GG", "ZAP", "MOON", "404"][n % 4].into(),
            ..bob_tag()
        });
    }
    member
}

/// The server's list as the golden screenshot shows it: Mods (Bob with his
/// tag and crown, Member 0 styled, 8 and 16 with their games, 24), then the
/// online group (Alice, Carol listening, Member 1 idle, 2 with a tag, 3 on
/// a phone).
pub fn server() -> Members {
    let mut bob = member("discord:u2", "Bob", 0xc2554d, Presence::DoNotDisturb);
    bob.tag = Some(bob_tag());
    bob.owner = true;
    bob.status = playing("Factorio");
    bob.color = Some(MODS);
    bob.name_style = NameStyle::new(14, 3, &[0x00ff99]).map(NameStyle::font_only);
    let mut zero = member("discord:m0", "Member 0", 0x3a9357, Presence::Online);
    mod_looks(&mut zero, 3);
    let mut eight = member("discord:m8", "Member 8", 0x3a9357, Presence::Online);
    eight.status = playing("Factorio");
    mod_looks(&mut eight, 12);
    let mut sixteen = member("discord:m16", "Member 16", 0x3a9357, Presence::Online);
    sixteen.status = moon("status line 16");
    mod_looks(&mut sixteen, 16);
    let mut twenty_four = member("discord:m24", "Member 24", 0x3a9357, Presence::Online);
    mod_looks(&mut twenty_four, 6);
    let alice = member("discord:u1", "Alice", 0x9c8a6a, Presence::Online);
    let mut carol = member("discord:u3", "Carol", 0xd05b9b, Presence::Idle);
    // Carol set another avatar for the server (`testing/fake_discord.ts:959`).
    carol.avatar.picture = Some(pictures::picture(
        "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
    ));
    carol.status = carol_listening();
    carol.bullet = true;
    carol.name_style = carol_style().map(NameStyle::font_only);
    let mut one = member("discord:m1", "Member 1", 0x3a9357, Presence::Idle);
    one.status = moon("status line 1");
    let mut two = member("discord:m2", "Member 2", 0x3a9357, Presence::DoNotDisturb);
    two.tag = Some(Tag {
        text: "MOON".into(),
        ..bob_tag()
    });
    two.status = playing("Hades");
    two.name_style = NameStyle::new(4, 1, &[0x3f8fd8]).map(NameStyle::font_only);
    let three = member("discord:m3", "Member 3", 0x3a9357, Presence::Mobile);
    let mods = [bob, zero, eight, sixteen, twenty_four];
    let mut online = vec![alice, carol, one, two, three];
    online.extend((4..=29).filter(|n| n % 8 != 0).map(looked));
    let dave = member("discord:u4", "Dave", 0x6a8a9c, Presence::Offline);
    let mut rows = vec![heading("Mods \u{2014} 5".to_owned())];
    rows.extend(mods.into_iter().map(|m| Row::Member(Box::new(m))));
    rows.push(heading(format!(
        "{} \u{2014} {}",
        tr!("Online"),
        online.len()
    )));
    rows.extend(online.into_iter().map(|m| Row::Member(Box::new(m))));
    rows.push(heading(format!("{} \u{2014} 1", tr!("Offline"))));
    rows.push(Row::Member(Box::new(dave)));
    Members {
        rows,
        ..Members::default()
    }
}

/// A group DM's list: its three people, Bob its creator, Carol with her
/// styled name.
pub fn group() -> Members {
    let mut alice = member("discord:u1", "Alice", 0x9c8a6a, Presence::Online);
    alice.status = Status::default();
    let mut bob = member("discord:u2", "Bob", 0xc2554d, Presence::DoNotDisturb);
    bob.tag = Some(bob_tag());
    bob.owner = true;
    bob.status = playing("Factorio");
    let mut carol = member("discord:u3", "Carol", 0xd05b9b, Presence::Idle);
    carol.status = carol_listening();
    carol.bullet = true;
    carol.name_style = carol_style();
    Members {
        rows: vec![
            heading(tr!("Members — {}", 3)),
            Row::Member(Box::new(alice)),
            Row::Member(Box::new(bob)),
            Row::Member(Box::new(carol)),
        ],
        group: true,
        ..Members::default()
    }
}

/// Every state a row has: offline (faded, no dot), a bot, a booster, timed
/// out, a coloured name, a role's picture on its way, Alice's card open.
pub fn states() -> Members {
    let mut rows = vec![heading("Staff \u{2014} 7".to_owned())];
    let mut offline = member("discord:s1", "Offline Olga", 0x3a9357, Presence::Offline);
    offline.status = words("away for a while");
    let mut bot = member("discord:s2", "Status Bot", 0x5865f2, Presence::Online);
    bot.bot = true;
    bot.role_icon = Expected::Waiting;
    let mut booster = member("discord:s3", "Booster Bea", 0xb86e22, Presence::Online);
    booster.boosting = tr!("Server boosting since {}", "3 March 2024");
    let mut quiet = member("discord:s4", "Timed-out Tim", 0x707070, Presence::Idle);
    quiet.timed_out = true;
    let mut red = member("discord:s5", "Red Role Rex", 0xc2554d, Presence::Online);
    red.color = Some(Color::from_rgb8(0xe7, 0x4c, 0x3c));
    let mut crowned = member("discord:s6", "Crowned Cy", 0x2d9f8a, Presence::Online);
    crowned.owner = true;
    let mut long = member(
        "discord:s7",
        "A member with a very, very long display name indeed",
        0x4a7bd0,
        Presence::Online,
    );
    long.tag = Some(bob_tag());
    long.owner = true;
    long.boosting = tr!("Server boosting since {}", "3 March 2024");
    long.timed_out = true;
    long.status = words("and a status that is just as long, to see it elided at the edge");
    for m in [offline, bot, booster, quiet, red, crowned, long] {
        rows.push(Row::Member(Box::new(m)));
    }
    Members {
        rows,
        gems: true,
        selected: "discord:s3".into(),
        ..Members::default()
    }
}

/// `count` members under one heading, for a list that only draws what is
/// near the view.
pub fn many(count: usize) -> Members {
    let mut rows = vec![heading(tr!("Members — {}", count))];
    rows.extend((0..count).map(|at| {
        let name = format!("Member {at}");
        let presence = if at % 3 == 0 {
            Presence::Offline
        } else {
            Presence::Online
        };
        Row::Member(Box::new(member(
            &format!("discord:n{at}"),
            &name,
            0x3a9357,
            presence,
        )))
    }));
    Members {
        rows,
        ..Members::default()
    }
}

/// A server with a note where the list will be, before Discord answers.
pub fn loading() -> Members {
    Members {
        note: tr!("Loading members...").into(),
        ..Members::default()
    }
}

/// `menu/member_context.png`: Bob's menu, right-clicked on his row, under
/// the Activity panel the window shows above the list.
pub fn context() -> Vm {
    let mut vm = shell::server_home();
    vm.people.activity = super::activity::panel();
    vm.people.members = server();
    let target = "discord:u2";
    let row = |text: &str, id: &str| MenuRow::new(text, Choice::person(target, id).into());
    let rows = vec![
        row(tr!("Profile"), "profile"),
        row(tr!("Mention"), "mention"),
        row(yapfast_lang::text_in("Menu", "Message"), "message"),
        row(tr!("Start call"), "call"),
        row(tr!("Add note"), "note").subtitle(tr!("Only visible to you")),
        row(tr!("Add friend nickname"), "nickname"),
        MenuRow::opens(tr!("Apps"), vec![row(tr!("Profile"), "apps")]).separated(),
        MenuRow::opens(tr!("Invite to server"), vec![row(tr!("Profile"), "invite")]).separated(),
        row(tr!("Remove friend"), "unfriend").danger(),
        row(tr!("Ignore"), "ignore"),
        row(tr!("Block"), "block").danger(),
        MenuRow::opens(tr!("Roles"), vec![row(tr!("Profile"), "roles")]).separated(),
    ];
    let rows = rows.into_iter().map(MenuRow::plain).collect();
    vm.menu = Menu::new(1, Spot::pointer(iced::Point::new(1270.0, 387.0)), rows);
    vm
}
