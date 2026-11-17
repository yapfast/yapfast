//! A Community server's Browse Channels, in place of a conversation: its
//! channels by category, each with a box ticked while the channel list
//! follows it, and each category with a switch that follows every channel in
//! it. The Slint side: `BrowseScreen` (`ui/browse.slint:102`). Task N14's.

mod ease;
mod heading;
mod item;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{Column, container, text};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::browse::{Browse, Intent};
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use yapfast_lang::tr;

/// The screen's padding, and the gap under the search.
const PAD: f32 = 16.0;
const GAP: f32 = 8.0;
/// The gap above a category after the first, and under a row.
const BEFORE_CATEGORY: f32 = 16.0;
const BETWEEN: f32 = 2.0;
/// Under the search, left of the empty line and above it.
const NOTE_LEFT: f32 = 8.0;
const NOTE_TOP: f32 = 8.0;
/// The search field's height and its padding.
const SEARCH_HEIGHT: f32 = 36.0;
const SEARCH_PAD: f32 = 10.0;

pub fn view(browse: &Browse, motion: Motion) -> impl View<Intent> + '_ {
    let search = field(tr!("Search channels"), &browse.filter)
        .on_input(Intent::Filter)
        .text_size(typography::size::BODY)
        .pad(SEARCH_PAD)
        .height(SEARCH_HEIGHT)
        .ring(false)
        .motion(motion)
        .view();
    let rows = &browse.rows;
    let channels = list(
        rows.len(),
        move |index| key(&rows[index].id),
        move |index| {
            let row = &rows[index];
            let top = if row.is_category() && index > 0 {
                BEFORE_CATEGORY
            } else {
                0.0
            };
            let bottom = if index + 1 == rows.len() {
                PAD
            } else {
                BETWEEN
            };
            let content = if row.is_category() {
                heading::view(row)
            } else {
                item::view(row, motion)
            };
            container(content).padding(Padding {
                top,
                bottom,
                ..Padding::ZERO
            })
        },
        motion,
    );
    let mut page: Vec<Element<'_, Intent>> = vec![search];
    if rows.is_empty() {
        let note = if browse.filter.is_empty() {
            tr!("There are no channels to browse.")
        } else {
            tr!("No channel's name matches that.")
        };
        page.push(
            container(
                text(note)
                    .size(typography::size::LABEL)
                    .style(catalog::text::muted),
            )
            .padding(Padding {
                top: NOTE_TOP,
                left: NOTE_LEFT,
                ..Padding::ZERO
            })
            .boxed(),
        );
    }
    page.push(channels.boxed());
    let page = Column::with_children(page).spacing(GAP).padding(Padding {
        bottom: 0.0,
        ..Padding::new(PAD)
    });
    container(page).width(Fill).height(Fill)
}

/// A row's identity in the list, from its room id.
fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// Its shots for `anomi-shots`, each the whole window with Browse Channels
/// in place of the conversation (see `crate::shots`). No golden shot of the
/// Slint app exists for them.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::{self, browse, shell};
    use anomi_vm::sidebar::Place;
    use anomi_vm::{Vm, conversation::Body, conversation::Conversation};

    fn window(browse: Browse) -> Vm {
        let mut vm = shell::server_home();
        if let Place::Server(server) = &mut vm.sidebar.place {
            server.channels = fixtures::sidebar::channels::browsing();
        }
        Vm {
            header: fixtures::header::browse(),
            browse,
            conversation: Conversation {
                body: Body::Browse,
                ..Conversation::default()
            },
            ..vm
        }
    }
    let shot = |name, vm| Shot {
        area: "browse",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("channels", || window(browse::browse())),
        shot("filtered", || window(browse::filtered())),
        shot("nothing_matches", || window(browse::nothing_matches())),
        shot("empty", || window(browse::empty())),
    ]
}
