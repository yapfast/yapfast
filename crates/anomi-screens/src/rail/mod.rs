//! The server rail: the DM home, a line, the unread DMs, the servers and
//! their folders, and joining one. The Slint side: `ServerRail`, `RailItem`
//! and `FolderItem` in `ui/rail.slint`, its rows from `rail_rows` in
//! `src/ui/refresh.rs`.
//!
//! - The rows scroll in a virtual list (an account may be in hundreds of
//!   servers); the home tile and the "+" stay.
//! - An open folder's servers sit on a strip of its colour, joined to its
//!   tile (`rows::Strip`).
//! - A right click on a server, folder or unread DM asks for its menu at
//!   the pointer ([`Intent::Menu`]); the shell draws it.
//! - Tooltips come from the view-model: the app writes them in the language.
//! - A server or folder dragged moves, the rail making room for it
//!   (`drag`).
//! - A folder's settings, which its menu opens, are a dialog of the
//!   shell's layers ([`folder`]).

mod drag;
pub mod folder;
mod rows;
mod shots;

use iced::border::Radius;
use iced::widget::{container, space, stack};
use iced::{Background, Border, Color, Fill, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::menu::Spot;
use anomi_vm::rail::{Dm, Folder, Intent, Rail, Server};
use anomi_widgets::brand;
use anomi_widgets::icon::Icon;
use anomi_widgets::list::list;
use anomi_widgets::menu::opener;
use anomi_widgets::rail_item::{Face, Preview, RailItem, rail_item};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use rows::{Row, Strip, rows};

#[cfg(feature = "shots")]
pub use shots::shots;

/// An unavailable server is shown as last known: half there.
const UNAVAILABLE: f32 = 0.5;
/// An open folder's strip is its colour this clear.
const STRIP_ALPHA: f32 = 0.16;
/// The strip's rounded corners.
const STRIP_RADIUS: f32 = 12.0;
/// The room under the "+", the shell having put the account card below.
const BOTTOM: f32 = 12.0;
/// The line under the home tile and between DMs and servers: this wide.
const LINE_WIDTH: f32 = 32.0;

pub fn view(rail: &Rail, motion: Motion) -> impl View<Intent> + '_ {
    let home = rail_item(Face::Picture(brand::logo()), motion)
        .selected(rail.home.selected)
        .unread(rail.home.unread)
        .badge(&rail.home.badge)
        .on_press(Intent::Home);
    let rows = rows(rail);
    let count = rows.len();
    // The rows are built a few at a time, by index: they live as long as
    // the list does.
    let keys: Vec<u64> = rows.iter().map(Row::key).collect();
    let rows = std::rc::Rc::new(rows);
    let (built, told, picked, aimed, copied) = (
        rows.clone(),
        rows.clone(),
        rows.clone(),
        rows.clone(),
        rows.clone(),
    );
    let scroll = list(
        count,
        move |index| keys[index],
        move |index| row(&built[index], motion),
        motion,
    )
    // The app asks for the pictures of the rows in view.
    .on_viewport(move |viewport| Intent::Seen(seen(&told, &viewport.keys)))
    .reorder(
        move |index| drag::may_drag(&picked, index),
        |_, _| true,
        move |moved| drag::intent(&rows, rail.drag.as_ref(), moved),
    )
    .live(move |from, to| drag::onto(&aimed, from, to))
    .drag_ground(drag::ground)
    .drag_copy(move |index| drag::copy(&copied[index], rail.drag.as_ref(), motion))
    .height(Fill);
    let mut parts: Vec<Element<'_, Intent>> = vec![
        tip(home, tr!("Direct messages"), Side::Right, motion)
            .stays()
            .boxed(),
        line().boxed(),
        scroll.boxed(),
    ];
    if rail.can_join {
        let join = rail_item(Face::Icon(Icon::CirclePlus.handle()), motion)
            .accent_icon(true)
            .on_press(Intent::Join);
        parts.push(
            tip(join, tr!("Join a server"), Side::Right, motion)
                .stays()
                .boxed(),
        );
    }
    container(
        iced::widget::Column::with_children(parts)
            .spacing(metrics::RAIL_GAP)
            .padding(
                iced::Padding::default()
                    .top(metrics::RAIL_GAP)
                    .bottom(BOTTOM),
            ),
    )
    .width(Length::Fixed(metrics::RAIL_WIDTH))
    .height(Fill)
    .style(catalog::container::rail)
}

/// The ids of the servers, folders and DMs whose rows have these keys.
fn seen(rows: &[Row<'_>], keys: &[u64]) -> Vec<String> {
    rows.iter()
        .filter(|row| keys.contains(&row.key()))
        .filter_map(|row| match row {
            Row::Divider => None,
            Row::Dm(dm) => Some(dm.id.clone()),
            Row::Folder { folder, .. } => Some(folder.id.clone()),
            Row::Server { server, .. } => Some(server.id.clone()),
            Row::Gap { .. } => None,
        })
        .collect()
}

/// The short line in Discord's outline colour.
fn line<'a>() -> impl View<Intent> + 'a {
    container(
        container(space())
            .width(LINE_WIDTH)
            .height(1)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().outline)),
                ..container::Style::default()
            }),
    )
    .width(metrics::RAIL_WIDTH)
    .align_x(alignment::Horizontal::Center)
}

/// One row: its item at the top, the strip behind it when it's a folder's.
fn row<'a>(row: &Row<'a>, motion: Motion) -> Element<'a, Intent> {
    let height = row.height();
    let item: Element<'a, Intent> = match row {
        Row::Divider => line().boxed(),
        Row::Dm(dm) => dm_item(dm, motion),
        Row::Folder {
            folder,
            servers,
            aimed,
        } => folder_item(folder, servers, *aimed, motion),
        Row::Server { server, aimed, .. } => server_item(server, *aimed, motion),
        Row::Gap { moved, .. } => drag::gap(moved),
    };
    let item = container(item)
        .width(metrics::RAIL_WIDTH)
        .height(Length::Fixed(height))
        .align_y(alignment::Vertical::Top);
    match row.strip() {
        Some(strip) => stack![strip_behind(strip), item]
            .width(metrics::RAIL_WIDTH)
            .height(Length::Fixed(height))
            .boxed(),
        None => item.boxed(),
    }
}

fn strip_behind<'a>(strip: Strip) -> impl View<Intent> + 'a {
    let corners = Radius::default()
        .top(if strip.top { STRIP_RADIUS } else { 0.0 })
        .bottom(if strip.bottom { STRIP_RADIUS } else { 0.0 });
    let body = container(space())
        .width(metrics::FOLDER_TILE)
        .height(Length::Fixed(strip.height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(Color {
                a: STRIP_ALPHA,
                ..strip.tint
            })),
            border: Border {
                radius: corners,
                ..Border::default()
            },
            ..container::Style::default()
        });
    container(body)
        .width(metrics::RAIL_WIDTH)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Top)
}

pub(super) fn face(avatar: &anomi_vm::kind::Avatar) -> Face<'_> {
    match &avatar.picture {
        Some(picture) => Face::Picture(picture.clone()),
        None => Face::Initials(&avatar.initial),
    }
}

/// `item` with a right click that asks for `id`'s menu, and `text` as its tip.
fn named<'a>(
    item: RailItem<'a, Intent>,
    id: &'a str,
    text: &'a str,
    motion: Motion,
) -> Element<'a, Intent> {
    let item = opener(item.on_menu(|at| Intent::Menu(id.to_owned(), Spot::pointer(at))))
        .on_right_press(|at| Intent::Menu(id.to_owned(), Spot::pointer(at)));
    // Slint's rail keeps its tooltip after a click.
    tip(item, text, Side::Right, motion).stays().boxed()
}

fn dm_item<'a>(dm: &'a Dm, motion: Motion) -> Element<'a, Intent> {
    let item = rail_item(face(&dm.avatar), motion)
        .unread(true)
        .badge(&dm.badge)
        .discord(dm.discord)
        .on_press(Intent::Dm(dm.id.clone()));
    named(item, &dm.id, &dm.name, motion)
}

/// A folder's face: open, or closed holding its first four servers.
pub(super) fn folder_face<'a>(folder: &Folder, servers: &[&'a Server], open: bool) -> Face<'a> {
    let previews = servers
        .iter()
        .take(4)
        .map(|server| Preview {
            initial: &server.avatar.initial,
            picture: server.avatar.picture.clone(),
        })
        .collect();
    Face::Folder {
        open,
        tint: folder.tint,
        previews,
    }
}

fn folder_item<'a>(
    folder: &'a Folder,
    servers: &[&'a Server],
    aimed: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let face = folder_face(folder, servers, folder.open);
    let item = rail_item(face, motion)
        .aimed(aimed)
        .selected(folder.tile.selected)
        .unread(folder.tile.unread)
        .badge(&folder.tile.badge)
        .call(folder.call)
        .discord(folder.discord)
        .on_press(Intent::Folder(folder.id.clone()));
    named(item, &folder.id, &folder.name, motion)
}

fn server_item<'a>(server: &'a Server, aimed: bool, motion: Motion) -> Element<'a, Intent> {
    let item = rail_item(face(&server.avatar), motion)
        .aimed(aimed)
        .selected(server.tile.selected)
        .unread(server.tile.unread)
        .badge(&server.tile.badge)
        .call(server.call)
        .discord(server.discord)
        .fade(if server.unavailable { UNAVAILABLE } else { 1.0 })
        .on_press(Intent::Server(server.id.clone()));
    let text = if server.tip.is_empty() {
        &server.name
    } else {
        &server.tip
    };
    named(item, &server.id, text, motion)
}
