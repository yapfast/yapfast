//! The editor's menus, as the window's menu with rows carrying their
//! intents (the `avatar-menu`, `banner-menu`, `tag-menu` and `more-menu`
//! popups, `ui/server.slint:1845`, `:1941`, `:2077`, `:2144`).

use iced::Task;

use anomi_vm::icon::Icon;
use anomi_vm::menu::{Menu, Row, Spot};
use anomi_vm::people::editor::{Intent as Edit, Picture};
use yapfast_lang::tr;

use crate::app::App;
use crate::message::Message;

fn row(text: &str, intent: Edit) -> Row {
    Row::new(
        text,
        anomi_vm::Intent::from(anomi_vm::people::Intent::from(intent)),
    )
}

fn show(app: &mut App, spot: Spot, rows: Vec<Row>) -> Task<Message> {
    crate::intent::menus::show(app, Menu::new(0, spot, rows));
    Task::none()
}

/// A picture there is to change or remove: Change, and Remove in the main
/// profile or Reset in a server's, which goes back to the main profile's.
pub(super) fn picture(app: &mut App, picture: Picture, spot: Spot) -> Task<Message> {
    app.vm.people.editor.editing = None;
    let main = app.vm.people.editor.main;
    let (change, undo) = match (picture, main) {
        (Picture::Avatar, true) => (tr!("Change Avatar"), tr!("Remove Avatar")),
        (Picture::Avatar, false) => (tr!("Change Avatar"), tr!("Reset Avatar")),
        (Picture::Banner, true) => (tr!("Change Banner"), tr!("Remove Banner")),
        (Picture::Banner, false) => (tr!("Change Banner"), tr!("Reset Banner")),
    };
    let rows = vec![
        row(change, Edit::Pick(picture)).icon(Icon::Image),
        row(undo, Edit::Reset(picture)).icon(Icon::X).danger(),
    ];
    show(app, spot, rows)
}

/// The server tag to wear: none, or one of the servers that have one, the
/// one chosen marked.
pub(super) fn tags(app: &mut App, spot: Spot) -> Task<Message> {
    let editor = &mut app.vm.people.editor;
    editor.editing = None;
    let chosen = editor.tag_chosen.as_str();
    let mut rows = vec![row(tr!("No Server Tag"), Edit::ChooseTag(String::new())).plain()];
    rows.extend(editor.tags.iter().map(|choice| {
        row(&choice.name, Edit::ChooseTag(choice.id.clone()))
            .subtitle(choice.tag.as_str())
            .checked(choice.id == chosen)
            .plain()
    }));
    show(app, spot, rows)
}

/// The card's More: Copy User ID.
pub(super) fn more(app: &mut App, spot: Spot) -> Task<Message> {
    app.vm.people.editor.editing = None;
    let rows = vec![row(tr!("Copy User ID"), Edit::CopyId).icon(Icon::Copy)];
    show(app, spot, rows)
}
