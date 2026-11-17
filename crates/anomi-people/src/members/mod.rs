//! The member list: groups by role with their counts, then the people in
//! them, 42 px a row, under the activity cards. The Slint side: the list
//! half of `MemberPanel` (`ui/people.slint:97`) and `RoleIcon` (`:46`);
//! `show_members` and `show_roster` (`src/ui/refresh.rs:2429`, `:2507`) fill
//! it.
//!
//! One virtual list of headings and people, so a server of thousands costs
//! the few rows in view. The panel's ground, its left edge and its width are
//! the shell's (`anomi_screens::conversation::beside`); the cards above the
//! list are `activity`'s.

mod group;
mod member_row;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::column;
use iced::{Color, Fill, Padding};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::people::members::{Intent, Members, Row};
use anomi_vm::people::{self};
use anomi_widgets::list::list;

#[cfg(feature = "shots")]
pub use sampled::shots;

/// What an offline member's words, marks and plate keep of their strength
/// (`opacity: ... "offline" ? 0.5 : 1`), and their avatar (`0.4`).
const OFFLINE: f32 = 0.5;
const OFFLINE_FACE: f32 = 0.4;
/// The wash on a chosen row and one under the pointer, Discord's grey at
/// 20 % and 12 % (`#97979f33`, `#97979f1f`).
const WASH: Color = Color::from_rgb(
    0x97 as f32 / 255.0,
    0x97 as f32 / 255.0,
    0x9f as f32 / 255.0,
);
const WASH_SELECTED: f32 = 0x33 as f32 / 255.0;
const WASH_HOVER: f32 = 0x1f as f32 / 255.0;
/// Between two rows (`padding-bottom: 2px`), above the first (`8px`), under
/// the last (`12px`) and at both sides (`padding-left: 8px`).
const SPACING: f32 = 2.0;
const TOP: f32 = 8.0;
const BOTTOM: f32 = 12.0;
const SIDE: f32 = metrics::SPACE_S;

/// `ground` with the wash over it, as one opaque colour.
fn wash(ground: Color, alpha: f32) -> Color {
    Color {
        r: ground.r + (WASH.r - ground.r) * alpha,
        g: ground.g + (WASH.g - ground.g) * alpha,
        b: ground.b + (WASH.b - ground.b) * alpha,
        a: 1.0,
    }
}

/// A row's identity, which stays with it as others come and go.
fn key(row: &Row) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        Row::Heading(heading) => ("heading", &heading.text).hash(&mut hasher),
        Row::Member(member) => ("member", &member.id).hash(&mut hasher),
    }
    hasher.finish()
}

pub fn view(members: &Members, motion: Motion) -> impl View<people::Intent> + '_ {
    let rows = list(
        members.rows.len(),
        |at| key(&members.rows[at]),
        move |at| match &members.rows[at] {
            Row::Heading(heading) => group::heading(heading),
            Row::Member(member) => {
                member_row::view(member, member.id == members.selected, members.gems, motion)
            }
        },
        motion,
    )
    .spacing(SPACING)
    .padding(Padding {
        top: TOP,
        right: SIDE,
        bottom: BOTTOM,
        left: SIDE,
    })
    .on_viewport(|view| Intent::Shown(view.first, view.last))
    .height(Fill);
    let mut panel = column![].width(Fill).height(Fill);
    if !members.note.is_empty() {
        panel = panel.push(group::note(&members.note));
    }
    panel.push(rows.boxed()).map(people::Intent::from)
}

/// The member list's shots for `anomi-shots`: the right column of the golden
/// `server/general_unread_divider`, `dm/group` and `menu/member_context`
/// screenshots, each the whole window from a fixture (see `crate::shots`).
/// They sit in the `members` area so as not to take the names of the
/// screenshots the timeline's and the DM list's shots already carry.
#[cfg(feature = "shots")]
mod sampled {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::members as sample;
    use anomi_vm::fixtures::{GOLDEN, Shot, conversation, pictures, shell};
    use anomi_vm::kind::Expected;
    use anomi_vm::people::members::{Members, Row};
    use anomi_vm::sidebar::Place;
    use anomi_widgets::nameplate::sample as plates;

    /// A decoration like the golden screenshots': a pink ring on nothing,
    /// which the avatar sits inside (a fifth larger than it).
    fn ring() -> iced::widget::image::Handle {
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

    /// Who wears what in the golden screenshots: the decorations (the fake
    /// gives one when n % 3 == 0), the plates, and the clan badge of the tags.
    pub fn dress(members: &mut Members) {
        let badge = || {
            Expected::Ready(pictures::picture(
                "/clan-badges/1400000000000000100/bobtag.png",
            ))
        };
        let plates = [
            ("Bob", "sky"),
            ("Alice", "bubble_gum"),
            ("Carol", "violet"),
            ("Member 1", "sky"),
        ];
        let ringed = ["Bob", "Alice", "Member 0", "Member 3", "Member 24"];
        for row in &mut members.rows {
            let Row::Member(member) = row else { continue };
            if let Some(tag) = &mut member.tag {
                tag.badge = badge();
            }
            if let Some((_, palette)) = plates.iter().find(|(name, _)| *name == member.name) {
                member.plate.picture = Some(plates::strip(&member.name));
                member.plate.palette = plates::palette(palette);
            }
            if ringed.contains(&member.name.as_str()) {
                member.decoration = Some(ring());
            }
        }
    }

    fn with(fill: fn() -> Members, shell: fn() -> Vm) -> Vm {
        let mut vm = shell();
        let mut members = fill();
        dress(&mut members);
        vm.people.members = members;
        vm
    }

    /// `dm/group`: 'Weekend plans' open, its header and its sidebar row.
    fn group_window() -> Vm {
        let mut vm = shell::dm_with(conversation::channel);
        vm.header = anomi_vm::fixtures::header::group();
        if let Place::Dms(dms) = &mut vm.sidebar.place {
            dms.rows[2].selected = false;
            dms.rows[1].selected = true;
        }
        vm.sidebar.version += 1;
        vm
    }

    fn server_window() -> Vm {
        let mut vm = shell::server_home();
        vm.people.activity = anomi_vm::fixtures::people::activity::panel();
        vm
    }

    pub fn shots() -> Vec<Shot> {
        fn shot(name: &'static str, vm: fn() -> Vm) -> Shot {
            Shot {
                area: "members",
                name,
                size: GOLDEN,
                theme: "bonfire",
                vm,
            }
        }
        vec![
            shot("general_unread_divider", || {
                with(sample::server, server_window)
            }),
            shot("group", || with(sample::group, group_window)),
            shot("states", || with(sample::states, shell::server_home)),
            shot("loading", || with(sample::loading, shell::server_home)),
            shot("member_context", || {
                let mut vm = sample::context();
                dress(&mut vm.people.members);
                vm
            }),
        ]
    }
}
