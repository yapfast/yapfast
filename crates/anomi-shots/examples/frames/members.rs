//! `presence` and `churn`: a member list of 300 people dressed as the fake's
//! `--member-looks` dresses them, beside #general, one of them changing at
//! each frame. As the app does, only the member who changed gets a new row;
//! the others keep theirs (the same `Arc`).

use std::sync::Arc;
use std::time::Instant;

use iced::advanced::mouse;
use iced::widget::image;
use iced::{Color, Event};
use iced_runtime::user_interface;

use anomi_screens::shots;
use anomi_vm::fixtures::kind::bob_tag;
use anomi_vm::fixtures::people::members::member;
use anomi_vm::fixtures::pictures;
use anomi_vm::kind::{Doing, Expected, NameStyle, Presence, Status, Tag};
use anomi_vm::people::members::{Heading, Member, Members, Row};
use anomi_widgets::nameplate::sample as plates;

use super::{Options, Window, pace};

const PEOPLE: usize = 300;
const PALETTES: [&str; 6] = ["bubble_gum", "sky", "violet", "teal", "forest", "sunset"];
const MODS: Color = Color::from_rgb(
    0x34 as f32 / 255.0,
    0x98 as f32 / 255.0,
    0xdb as f32 / 255.0,
);

struct Roster {
    people: Vec<Arc<Member>>,
    mods: Vec<bool>,
    /// What each was before going offline.
    was: Vec<Presence>,
}

/// The fake's nth listed member with `--member-looks` (`memberLook`,
/// `lookPresence`): a name style for even n, a decoration for n.is_multiple_of(3), a
/// nameplate for n % 4 == 1, a tag for n % 5 == 2, a status or a game by
/// n % 3, and Mods for n.is_multiple_of(8).
fn dressed(n: usize, ring: &image::Handle) -> Member {
    let presence = [
        Presence::Online,
        Presence::Idle,
        Presence::DoNotDisturb,
        Presence::Mobile,
    ][n % 4];
    let name = format!("Member {n}");
    let mut member = member(&format!("discord:m{n}"), &name, 0x3a9357, presence);
    member.status = match n % 3 {
        1 => Status {
            text: format!("\u{1f319} status line {n}"),
            ..Status::default()
        },
        2 => Status {
            text: ["Factorio", "Celeste", "Hades", "Balatro"][n % 4].into(),
            doing: Doing::Playing,
            ..Status::default()
        },
        _ => Status::default(),
    };
    if n.is_multiple_of(2) {
        member.name_style = match (n / 2) % 3 {
            0 => NameStyle::new([3, 4, 6, 12, 14, 16][n % 6], 1, &[0x3f8fd8]),
            1 => NameStyle::new(3, 2, &[0xff7eb3, 0x7a5cff]),
            _ => NameStyle::new(14, 3, &[0x00ff99]),
        };
    }
    if n.is_multiple_of(3) {
        member.decoration = Some(ring.clone());
    }
    if n % 4 == 1 {
        member.plate.picture = Some(plates::strip(&format!("plate {}", n % 6)));
        member.plate.palette = plates::palette(PALETTES[n % 6]);
    }
    if n % 5 == 2 {
        member.tag = Some(Tag {
            text: ["GG", "ZAP", "MOON", "404"][n % 4].into(),
            badge: Expected::Ready(pictures::picture(
                "/clan-badges/1400000000000000100/bobtag.png",
            )),
            ..bob_tag()
        });
    }
    if n.is_multiple_of(8) {
        member.color = Some(MODS);
        member.name_style = member.name_style.map(NameStyle::font_only);
    }
    member
}

/// A decoration as the shots draw one: a pink ring the avatar sits in.
fn ring() -> image::Handle {
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
    image::Handle::from_rgba(SIDE, SIDE, pixels)
}

impl Roster {
    fn new() -> Self {
        let ring = ring();
        let people: Vec<Arc<Member>> = (0..PEOPLE).map(|n| Arc::new(dressed(n, &ring))).collect();
        Self {
            was: people.iter().map(|member| member.presence).collect(),
            mods: (0..PEOPLE).map(|n| n.is_multiple_of(8)).collect(),
            people,
        }
    }

    /// The list as Discord groups it: Mods, the others online, then those
    /// offline, each by name.
    fn rows(&self) -> Vec<Row> {
        let mut groups: [Vec<&Arc<Member>>; 3] = Default::default();
        for (n, member) in self.people.iter().enumerate() {
            let group = if member.offline() {
                2
            } else if self.mods[n] {
                0
            } else {
                1
            };
            groups[group].push(member);
        }
        let mut rows = Vec::with_capacity(PEOPLE + 3);
        for (group, title) in groups.iter_mut().zip(["Mods", "Online", "Offline"]) {
            if group.is_empty() {
                continue;
            }
            group.sort_by(|a, b| a.name.cmp(&b.name));
            rows.push(Row::Heading(Heading {
                text: format!("{title} \u{2014} {}", group.len()),
                icon: Expected::None,
            }));
            rows.extend(group.iter().map(|member| Row::Member(Arc::clone(member))));
        }
        rows
    }

    /// Member `n` changes: a new row for them alone.
    fn change(&mut self, n: usize, presence: Presence) {
        let mut changed = (*self.people[n]).clone();
        changed.presence = presence;
        self.people[n] = Arc::new(changed);
    }
}

pub fn run(window: &mut Window, options: &Options, churn: bool) {
    let shot = shots::all()
        .into_iter()
        .find(|shot| shot.area == "members" && shot.name == "general_unread_divider")
        .expect("the members shot");
    let mut vm = (shot.vm)();
    // #general's history beside it, as the measurement has.
    vm.chat.timeline = super::scroll::vm().chat.timeline;
    let mut roster = Roster::new();
    vm.people.members = Members {
        rows: roster.rows(),
        ..Members::default()
    };
    // Off the list: as in the measurement, the pointer rests on the timeline.
    let mut cache = user_interface::Cache::default();
    for _ in 0..3 {
        let mut ui = window.build(&vm, cache);
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: window.cursor.position().unwrap_or_default(),
        });
        let _ = window.events(&mut ui, &[moved]);
        let _ = window.redraw(&mut ui, Instant::now());
        window.draw(&mut ui);
        cache = ui.into_cache();
    }
    let mut shown: Vec<usize> = (0..PEOPLE).step_by(8).collect();
    shown.sort_by(|&a, &b| roster.people[a].name.cmp(&roster.people[b].name));
    shown.truncate(8);
    window.totals = super::Totals::default();
    let _ = iced_tiny_skia::stats::take();
    let started = Instant::now();
    for frame in 0..options.frames {
        if churn {
            // The fake's churn: every seventh member in turn.
            let n = (frame * 7) % PEOPLE;
            let presence = if roster.people[n].offline() {
                roster.was[n]
            } else {
                Presence::Offline
            };
            roster.change(n, presence);
        } else {
            // One of the first eight Mods, in view, idle and back.
            let n = shown[frame % shown.len()];
            let presence = if roster.people[n].presence == Presence::Idle {
                Presence::Online
            } else {
                Presence::Idle
            };
            roster.change(n, presence);
        }
        vm.people.members.rows = roster.rows();
        let mut ui = window.build(&vm, cache);
        if window.redraw(&mut ui, Instant::now()) {
            cache = ui.into_cache();
            ui = window.build(&vm, cache);
            let _ = window.redraw(&mut ui, Instant::now());
        }
        window.draw(&mut ui);
        cache = ui.into_cache();
        pace(started, frame, options.fps);
    }
}
