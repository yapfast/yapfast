//! A rail folder's menu, as Discord's (`discord-folder-menu.png` among the
//! owner's references): Mark Folder As Read, greyed while none of its
//! servers has anything to read; a line; Move…, whose submenu puts it at
//! the top or the bottom of the list; a line; Folder Settings; Close All
//! Folders, greyed while none is open. Marking read is the core's choice
//! (`MenuChoice::MarkRead`, which it offers only while there is something
//! to read); moving and the settings write the account's server list, so
//! they show for a Discord account where the rail lets its servers be
//! dragged; which folders are open is the window's own (`layout.json`).

use yapfast_core::application::{AppState, Application, MenuChoice};
use yapfast_lang::tr;

use anomi_vm::menu::Row;

use super::{Make, choice_id};
use crate::layout::Layout;

/// What the rows of a folder's menu report: Move's two, Folder Settings
/// and Close All Folders.
pub(crate) const TOP: &str = "folder-top";
pub(crate) const BOTTOM: &str = "folder-bottom";
pub(crate) const SETTINGS: &str = "folder-settings";
pub(crate) const CLOSE_ALL: &str = "close-folders";

/// Whether `id` names a folder on the rail.
pub(crate) fn is_folder(state: &AppState, id: &str) -> bool {
    state
        .servers
        .iter()
        .any(|server| server.folder.as_ref().is_some_and(|folder| folder.id == id))
}

/// Whether a folder of the rail is open.
pub(crate) fn any_open(state: &AppState, layout: &Layout) -> bool {
    state
        .servers
        .iter()
        .filter_map(|server| server.folder.as_ref())
        .any(|folder| layout.open_folders.contains(&folder.id))
}

/// The folder `id`'s menu.
pub(super) fn rows(core: &Application, layout: &Layout, make: &Make<'_>, id: &str) -> Vec<Row> {
    let state = &core.state;
    let unread = core.menu_choices(id).contains(&MenuChoice::MarkRead);
    let mark = choice_id(MenuChoice::MarkRead);
    let mut rows = vec![make.row(tr!("Mark folder as read"), mark).disabled(!unread)];
    // Discord's settings keep the folder, as they keep a server's place.
    if !state.preview && state.discord_user.is_some() {
        let ends = vec![
            make.row(tr!("Top of list"), TOP).plain(),
            make.row(tr!("Bottom of list"), BOTTOM).plain(),
        ];
        rows.push(Row::opens(tr!("Move…"), ends).separated());
        rows.push(make.row(tr!("Folder settings"), SETTINGS).separated());
    }
    let close = make
        .row(tr!("Close all folders"), CLOSE_ALL)
        .disabled(!any_open(state, layout));
    // Under a line of its own when nothing stands between it and Mark.
    rows.push(if rows.len() == 1 {
        close.separated()
    } else {
        close
    });
    rows.into_iter().map(Row::plain).collect()
}
