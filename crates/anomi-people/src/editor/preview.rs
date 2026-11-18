//! The profile editor's preview (`ProfileTabs`, `ui/server.slint:1062`): the
//! tabs beside the card, laid out as the full profile's are, 24 px in from
//! the card and the edge, on the line the side panel's header keeps. Board,
//! Activity and Wishlist show what the profile wears, or Yapfast's words
//! where it has nothing; Discord's Board and Wishlist can't be set up in
//! Yapfast yet, and say so rather than offer buttons that do nothing.
//! Task P15 (docs/iced/tasks/people.md).

mod look;
mod pages;
mod tabs;

use iced::Fill;
use iced::Padding;
use iced::widget::{Column, responsive};

use anomi_theme::prelude::*;
use anomi_vm::people::editor_preview::{EditorPreview, Intent};
use anomi_widgets::list::scroll_view;

use look::Look;

/// The tabs' inset from the card and the edge, and what the tabs sit over.
const EDGE: f32 = 24.0;
const GAP: f32 = 16.0;

pub fn view(preview: &EditorPreview, motion: Motion) -> impl View<Intent> + '_ {
    let look = Look::new(preview);
    responsive(move |size| {
        let width = size.width - 2.0 * EDGE;
        let content = pages::content(preview, width, look, motion);
        // An empty tab's words sit in the middle of what's left; a list
        // scrolls.
        let content = if preview.blank() {
            content
        } else {
            scroll_view(content, motion).boxed()
        };
        Column::<Element<'_, Intent>>::new()
            .spacing(GAP)
            .width(Fill)
            .height(Fill)
            .padding(Padding::ZERO.left(EDGE).right(EDGE).bottom(EDGE))
            .push(tabs::bar(preview, look))
            .push(content)
            .boxed()
    })
}

/// Its shots for `anomi-shots`: the editor with its tabs filled, the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    vec![
        Shot {
            area: "profile",
            name: "editor_board",
            size: GOLDEN,
            theme: "bonfire",
            vm: || sample::on(Page::Board),
        },
        Shot {
            area: "profile",
            name: "editor_activity",
            size: GOLDEN,
            theme: "bonfire",
            vm: || sample::on(Page::Activity),
        },
        Shot {
            area: "profile",
            name: "editor_wishlist",
            size: GOLDEN,
            theme: "bonfire",
            vm: || sample::on(Page::Wishlist),
        },
    ]
}

#[cfg(feature = "shots")]
mod sample {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::{editor as editors, editor_preview as samples};
    use anomi_vm::people::editor_preview::Page;

    /// The editor on Alice with her tabs filled, on `page`.
    pub fn on(page: Page) -> Vm {
        let mut editor = editors::main();
        editor.preview = samples::on(page);
        editors::open(editor)
    }
}

#[cfg(feature = "shots")]
use anomi_vm::people::editor_preview::Page;
