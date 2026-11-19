//! A server's role editor, as Discord's Roles page opens one: a Back link,
//! the notice for a role this account can't change, the Display,
//! Permissions and Manage Members tabs and the page of the one chosen.
//! Task X09's (docs/iced/tasks/settings.md). The Slint side: the open half
//! of `RolesSection` (`ui/roles.slint:432`) and `RoleTabButton` (`:295`).
//!
//! The role's title is the frame's, in its bar (`Server.title`). The page of
//! each tab is its module's: [`display`], [`permissions`] and
//! [`members`](super::members); the two dialogs are [`confirm`] and
//! [`adding`], which the shell draws over the window.

pub mod colors;
mod dialogs;
mod display;
mod permissions;
mod strip;

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::role_editor::{Intent, RoleEditor, Tab};
use yapfast_lang::tr;

use super::members;
use crate::kit::{Ink, Leaf, link, mix};

pub use dialogs::{ADD_SEARCH, adding, confirm};

/// Between the page's parts, and between the tabs.
pub(super) const GAP: f32 = 16.0;
const TABS: f32 = 24.0;
/// A tab's height, the words' inset beside.
const TAB: f32 = 36.0;
const TAB_PAD: f32 = 4.0;
/// The notice's corners and insets.
const NOTICE_RADIUS: f32 = 6.0;
const NOTICE_SIDES: f32 = 12.0;
const NOTICE_ABOVE: f32 = 10.0;

/// A line of the page's quieter words (`Label`).
pub(super) fn label<'a>(words: impl text::IntoFragment<'a>) -> Element<'a, Intent> {
    typography::label(words).width(Length::Fill).boxed()
}

/// One tab: its words, which light under the pointer; the accent's line
/// under the chosen one is the [`strip`](strip::strip)'s.
fn tab<'a>(
    words: &'a str,
    chosen: bool,
    enabled: bool,
    on_press: Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut leaf = Leaf::new(words, motion, move |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        let ink = if !enabled {
            palette.faint
        } else if chosen {
            palette.text
        } else {
            mix(palette.secondary, palette.text, lit)
        };
        Ink {
            fill: Color::TRANSPARENT,
            text: ink,
            glyph: ink,
        }
    })
    .on_press(on_press)
    .enabled(enabled);
    leaf.size = 15.0;
    leaf.font = typography::SEMIBOLD;
    leaf.height = TAB;
    leaf.pad = TAB_PAD;
    leaf.radius = 0.0;
    leaf.boxed()
}

/// The three tabs, then the line under them a gap below.
fn tabs<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let chosen = |tab: Tab| editor.tab == tab;
    let row = row![
        tab(
            tr!("Display"),
            chosen(Tab::Display),
            !editor.everyone,
            Intent::Tab(Tab::Display),
            motion
        ),
        tab(
            tr!("Permissions"),
            chosen(Tab::Permissions),
            true,
            Intent::Tab(Tab::Permissions),
            motion
        ),
        tab(
            &editor.members_title,
            chosen(Tab::Members),
            !editor.everyone,
            Intent::Tab(Tab::Members),
            motion
        ),
    ]
    .spacing(TABS);
    let at = match editor.tab {
        Tab::Display => 0,
        Tab::Permissions => 1,
        Tab::Members => 2,
    };
    column![
        strip::strip(row.boxed(), at, motion),
        container(space())
            .width(Length::Fill)
            .height(1)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().line.into()),
                ..container::Style::default()
            }),
    ]
    .spacing(GAP)
    .boxed()
}

/// Why the role can't change, or what manages it, on a raised card.
fn notice(words: &str) -> Element<'_, Intent> {
    container(
        text(words)
            .size(typography::size::SMALL)
            .style(catalog::text::secondary)
            .width(Length::Fill),
    )
    .padding(Padding::from([NOTICE_ABOVE, NOTICE_SIDES]))
    .width(Length::Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().raised.into()),
        border: Border::default().rounded(NOTICE_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

pub fn view(editor: &RoleEditor, motion: Motion) -> impl View<Intent> + '_ {
    let mut page = column![
        row![
            link(tr!("Back"), motion)
                .icon(Icon::ChevronLeft)
                .small(true)
                .on_press(Intent::Back)
                .view()
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(GAP);
    if !editor.notice.is_empty() {
        page = page.push(notice(&editor.notice));
    }
    page = page.push(tabs(editor, motion));
    page.push(match editor.tab {
        Tab::Display => display::view(editor, motion),
        Tab::Permissions => permissions::view(editor, motion),
        Tab::Members => members::view(editor, motion),
    })
    .width(Length::Fill)
    .height(if editor.tab == Tab::Display {
        Length::Shrink
    } else {
        Length::Fill
    })
}

/// Its shots for `anomi-shots`: `ssettings/role_editor.png` as the Slint
/// app showed it (Leads, locked), then a role that may be changed, a custom
/// colour, and each other tab (the dialogs are the shell's layer, drawn
/// by tests here).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::role_editor as sample;
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| Shot {
        area: "ssettings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("role_editor", sample::open),
        shot("role_editor_mods", || {
            sample::window(sample::editable(), Tab::Display)
        }),
        shot("role_editor_custom", || {
            sample::window(sample::custom(), Tab::Display)
        }),
        shot("role_editor_permissions", || {
            sample::window(sample::editable(), Tab::Permissions)
        }),
        shot("role_editor_members", || {
            sample::window(sample::editable(), Tab::Members)
        }),
        shot("role_editor_delete", sample::confirm_delete),
        shot("role_editor_remove", sample::confirm_remove),
        shot("role_editor_add", sample::adding_open),
    ]
}
