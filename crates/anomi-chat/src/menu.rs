//! The message menu: `MessageMenu` (`ui/messages.slint:1394`). Task C12
//! (docs/iced/tasks/chat.md). The shell places the card at the spot the
//! menu opened at; this is the card: the four quick reactions over the
//! rows ([`build`] makes them, in Discord's order) and, for Apps, a submenu beside
//! it, level with its row, on the left when there is no room on the right
//! (`flip`).

mod rows;

use iced::widget::{column, container, row, space};
use iced::{Background, Border, Fill, Shadow, Vector};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::chat::menu::{Intent, Menu};
use anomi_vm::menu::Row;
use anomi_widgets::menu::{item, width};

use crate::pickers::emoji::emoji_button;

pub use rows::build;

/// `padding: 6px; spacing: 2px` of the menu's column.
const PAD: f32 = 6.0;
const GAP: f32 = 2.0;
/// A row's height, as `menu::item` lays it out (`MenuItem`), and an app's
/// with its second line.
const ROW: f32 = 32.0;
const APP_ROW: f32 = 44.0;
/// The quick reactions: 44 px tiles with 24 px emoji, 4 px under them,
/// 2 px at the sides.
const TILE: f32 = 44.0;
const GLYPH: f32 = 24.0;
const TILES_BELOW: f32 = 4.0;
const TILES_SIDE: f32 = 2.0;
/// The menu's narrowest and widest (`max(224px, min(320px, ..))`), the
/// submenu's width and the space between them (`:1490`).
const NARROWEST: f32 = 224.0;
const APPS_WIDTH: f32 = 224.0;
const BESIDE: f32 = 4.0;
/// The line between groups.
const LINE: f32 = 1.0;
/// `drop-shadow-blur: 24px; drop-shadow-color: #00000080;
/// drop-shadow-offset-y: 8px`.
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DOWN: f32 = 8.0;

pub fn view(menu: &Menu, motion: Motion) -> impl View<Intent> + '_ {
    let rows = &menu.rows[..];
    let tiles = menu.facts.can_react && !menu.reactions.is_empty();
    let panel = card(
        panel_column(menu, rows, tiles, motion),
        width(rows, NARROWEST),
    );
    if menu.apps.is_empty() {
        return panel;
    }
    // With apps, the submenu's room stays beside the menu whether it shows
    // or not, so its opening moves nothing (`content-width`).
    let (top, height) = apps_place(rows, tiles, menu.apps.len());
    let beside = if menu.apps_open {
        apps(rows, top, motion)
    } else {
        space().width(APPS_WIDTH).height(top + height).boxed()
    };
    let across = if menu.flip {
        row![beside, panel]
    } else {
        row![panel, beside]
    };
    across.spacing(BESIDE).boxed()
}

/// The menu's column: the tiles, then the rows with a line before each
/// that starts a group.
fn panel_column<'a>(
    menu: &'a Menu,
    rows: &'a [Row<Intent>],
    tiles: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut column = column![].spacing(GAP).padding(PAD);
    if tiles {
        column = column.push(quick(menu, motion));
    }
    let leave_apps = (!menu.apps.is_empty()).then_some(Intent::Apps(false));
    for each in rows {
        if each.separated {
            column = column.push(rule());
        }
        let mut entry = item(each, motion);
        if !each.submenu.is_empty() {
            entry = entry
                .arrow(true)
                .lit(menu.apps_open)
                .on_hover(Intent::Apps(true));
        } else if let Some(leave) = &leave_apps {
            entry = entry.on_hover(leave.clone());
        }
        column = column.push(entry.boxed());
    }
    column.boxed()
}

/// Discord's quick reactions, spread across the menu.
fn quick(menu: &Menu, motion: Motion) -> Element<'_, Intent> {
    let mut tiles: Vec<Element<'_, Intent>> = Vec::new();
    for (index, emoji) in menu.reactions.iter().enumerate() {
        if index > 0 {
            tiles.push(space().width(Fill).boxed());
        }
        tiles.push(
            emoji_button(emoji, motion)
                .size(TILE)
                .glyph(GLYPH)
                .fill(|theme| theme.palette().field)
                .on_press(Intent::React(emoji.key().to_owned()))
                .on_hover(Intent::Apps(false))
                .view(),
        );
    }
    row(tiles)
        .padding(iced::Padding {
            left: TILES_SIDE,
            right: TILES_SIDE,
            bottom: TILES_BELOW,
            ..iced::Padding::ZERO
        })
        .boxed()
}

/// The Apps submenu: each command with its app's name under it, `top`
/// down from the menu's.
fn apps<'a>(rows: &'a [Row<Intent>], top: f32, motion: Motion) -> Element<'a, Intent> {
    let commands = rows
        .iter()
        .find(|row| !row.submenu.is_empty())
        .map_or(&[][..], |row| &row.submenu[..]);
    let list = column(commands.iter().map(|command| item(command, motion).boxed()))
        .spacing(GAP)
        .padding(PAD);
    column![space().height(top), card(list.boxed(), APPS_WIDTH)].boxed()
}

/// Where the Apps submenu goes and how tall it is: level with the Apps row,
/// less the menu's padding (`apps-y - 6px`), but no lower than ends it with
/// the menu or itself, whichever is taller (`:1596`).
fn apps_place(rows: &[Row<Intent>], tiles: bool, commands: usize) -> (f32, f32) {
    let mut y = if tiles { TILE + TILES_BELOW + GAP } else { 0.0 };
    let mut level = 0.0;
    for each in rows {
        if each.separated {
            y += LINE + GAP;
        }
        if !each.submenu.is_empty() {
            level = y;
        }
        y += ROW + GAP;
    }
    let menu = y - GAP + 2.0 * PAD;
    let height = commands as f32 * (APP_ROW + GAP) - GAP + 2.0 * PAD;
    (level.min(menu.max(height) - height).max(0.0), height)
}

/// A one pixel line (`Theme.line`).
fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Fill)
        .height(LINE)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        })
        .boxed()
}

/// `background: Theme.menu; border-color: Theme.menu-line; border-width:
/// 1px; border-radius: 8px;` and its shadow.
fn card(content: Element<'_, Intent>, width: f32) -> Element<'_, Intent> {
    container(content)
        .width(width)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.menu())),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(palette.menu_line()),
                shadow: Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                    offset: Vector::new(0.0, SHADOW_DOWN),
                    blur_radius: SHADOW_BLUR,
                },
                ..container::Style::default()
            }
        })
        .boxed()
}

/// Its shots for `anomi-shots`: the whole window with a message's menu
/// open over the timeline, as `menu/message_context.png` and, with the
/// pointer on Apps, `menu/apps_submenu.png`; and its other states.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::Vm;
    use anomi_vm::fixtures::chat::menu as fixture;
    // The fixtures hold the facts; the rows carry the text, made here.
    fn built(make: fn() -> Vm) -> Vm {
        let mut vm = make();
        build(&mut vm.chat.menu);
        vm
    }
    let shot = |name, vm| Shot {
        area: "menu",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("message_context", || built(fixture::message_context)),
        shot("apps_submenu", || built(fixture::apps_submenu)),
        shot("message_own", || built(fixture::own)),
        shot("message_picture", || built(fixture::picture)),
        shot("message_video", || built(fixture::video)),
        shot("message_sending", || built(fixture::sending)),
        shot("message_poll", || built(fixture::poll)),
        shot("message_apps_flipped", || built(fixture::apps_flipped)),
    ]
}
