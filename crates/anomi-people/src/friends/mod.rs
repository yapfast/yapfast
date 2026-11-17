//! The Friends screen: the Discord friends list in place of a conversation,
//! those around or all, with a search and a welcome when there are none.
//! Task P01 (docs/iced/tasks/people.md). The Slint side: `FriendsScreen`
//! (`ui/friends.slint:247`) and the friend half of `show_sidebar`
//! (`src/ui/refresh.rs:1059`, the handlers at `src/ui/callbacks.rs:3037`).
//! Its tabs are the conversation header's; Active Now is drawn beside it by
//! the conversation body.

pub mod add;
mod friend_row;
pub mod requests;
pub mod tabs;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use iced::widget::{column, container, space, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::friends::{Friends, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::list;
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

#[cfg(feature = "shots")]
mod shots {
    //! The Friends screen's shots: the whole window with the screen open,
    //! beside the golden screenshots they match. Task P01's.

    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::friends;
    use anomi_vm::fixtures::{conversation, shell};
    use anomi_vm::header::{Header, Place, Screen, Title, Tools};

    use crate::shots::{GOLDEN, SMALL, Shot};

    /// The window on Friends with `screen`'s rows, its tab `all`.
    fn open(screen: anomi_vm::people::friends::Friends) -> Vm {
        let mut vm = shell::dm_with(conversation::friends);
        vm.header = Header::Open(Box::new(Place {
            title: Title::Screen(Screen::Friends { all: screen.all }),
            tools: Tools::default(),
        }));
        if let anomi_vm::sidebar::Place::Dms(dms) = &mut vm.sidebar.place {
            dms.friends_selected = true;
            for row in &mut dms.rows {
                row.selected = false;
            }
        }
        vm.people.friends = screen;
        vm.people.active_now = anomi_vm::fixtures::people::active_now::around();
        vm
    }

    fn online() -> Vm {
        open(friends::online())
    }

    fn all() -> Vm {
        open(friends::all())
    }

    fn nobody_around() -> Vm {
        open(friends::nobody_around())
    }

    fn no_match() -> Vm {
        open(friends::no_match())
    }

    fn none() -> Vm {
        open(friends::none())
    }

    pub fn shots() -> Vec<Shot> {
        let shot = |area, name, size, vm: fn() -> Vm| Shot {
            area,
            name,
            size,
            theme: "bonfire",
            vm,
        };
        vec![
            shot("friends", "online_tab_active_now", GOLDEN, online),
            shot("friends", "all_tab", GOLDEN, all),
            shot("friends", "nobody_around", GOLDEN, nobody_around),
            shot("friends", "no_match", GOLDEN, no_match),
            shot("friends", "none", GOLDEN, none),
            shot("small", "friends", SMALL, online),
            Shot {
                theme: "catppuccin-latte",
                ..shot("light", "friends", GOLDEN, all)
            },
        ]
    }
}

/// The screen's padding, and the gap between its search, heading and list
/// (`ui/friends.slint:279`).
const PAD: f32 = metrics::SPACE_L;
const GAP: f32 = metrics::SPACE_S;
/// The search well: 36 px tall, its words 15 px, inset 10 (`:280`-`:281`).
const SEARCH_TEXT: f32 = 15.0;
const SEARCH_PAD: f32 = 10.0;
/// A heading's height and the inset it shares with the rows (`:150`).
const HEADING: f32 = 36.0;
const INSET: f32 = 10.0;
/// The words of the heading (`:152`, used at `:285`).
const HEADING_TEXT: f32 = typography::size::CAPTION;
/// The welcome: its disc, glyph and title (`:263`-`:268`).
const WELCOME_PAD: f32 = 32.0;
const WELCOME_GAP: f32 = 10.0;
const DISC: f32 = 80.0;
const DISC_GLYPH: f32 = 36.0;
const WELCOME_TITLE: f32 = 22.0;
/// The search field's id.
pub const SEARCH: &str = "friends-search";

pub fn view(friends: &Friends, motion: Motion) -> impl View<Intent> + '_ {
    let screen: Element<'_, Intent> = if friends.total == 0 {
        welcome(motion)
    } else {
        listing(friends, motion)
    };
    container(screen).width(Fill).height(Fill).clip(true)
}

/// The welcome, when there are no friends at all.
fn welcome<'a>(motion: Motion) -> Element<'a, Intent> {
    container(
        column![
            container(icon(Icon::Users, DISC_GLYPH).style(|theme: &Theme, _| {
                iced::widget::svg::Style {
                    color: Some(theme.palette().secondary),
                }
            }))
            .center(DISC)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border::default().rounded(DISC / 2.0),
                ..container::Style::default()
            }),
            text(tr!("No friends yet"))
                .size(WELCOME_TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary),
            text(tr!("Add someone with their Discord username."))
                .size(typography::size::BODY)
                .style(catalog::text::secondary)
                .align_x(Alignment::Center),
            action_button(tr!("Add a friend"), Kind::Primary, motion).on_press(Intent::AddFriend),
        ]
        .spacing(WELCOME_GAP)
        .align_x(Alignment::Center),
    )
    .padding(WELCOME_PAD)
    .center(Fill)
    .boxed()
}

/// The search, the count and the rows.
fn listing(friends: &Friends, motion: Motion) -> Element<'_, Intent> {
    let count = friends.rows.len();
    let heading = if friends.all {
        tr!("ALL FRIENDS — {}", count)
    } else {
        tr!("ONLINE — {}", count)
    };
    let mut screen = column![
        field(tr!("Search"), &friends.filter)
            .id(SEARCH)
            .on_input(Intent::Filter)
            .text_size(SEARCH_TEXT)
            .pad(SEARCH_PAD)
            .ring(false)
            .motion(motion)
            .view(),
        container(
            text(heading)
                .size(HEADING_TEXT)
                .font(typography::BOLD)
                .style(catalog::text::muted)
                .wrapping(text::Wrapping::None),
        )
        .padding(Padding::ZERO.left(INSET))
        .height(HEADING)
        .center_y(HEADING),
    ]
    .spacing(GAP);
    if count == 0 {
        screen = screen.push(
            container(typography::label(if friends.filter.is_empty() {
                tr!("None of your friends are around right now.")
            } else {
                tr!("No friend's name matches that.")
            }))
            .padding(Padding::ZERO.left(INSET))
            .boxed(),
        );
    }
    let rows = list(
        count,
        move |at| key(&friends.rows[at].id),
        move |at| friend_row::view(&friends.rows[at], motion),
        motion,
    )
    .estimate(friend_row::HEIGHT)
    .padding(Padding::ZERO.bottom(PAD));
    screen
        .push(if count == 0 {
            space().height(Fill).boxed()
        } else {
            rows.boxed()
        })
        .padding(Padding::new(PAD).bottom(0.0))
        .boxed()
}

/// A row's identity, which stays with it as others come and go.
fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::key;

    #[test]
    fn a_key_follows_the_person() {
        assert_eq!(key("1"), key("1"));
        assert_ne!(key("1"), key("2"));
    }
}
