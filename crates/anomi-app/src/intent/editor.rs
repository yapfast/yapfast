//! The profile editor's intents, its preview's and the name style window's
//! (`src/ui/callbacks.rs:1381`-`:1576`, and what the dialog's own
//! functions did in `ui/server.slint:1674`-`:1700`): a text written goes to
//! the core, which the editor's projection shows at once and Discord gets
//! only on Save; what's being edited, the boxes and pickers open, the tab
//! shown and the styles panel are the window's own.
//!
//! Leaving with changes asks, as Discord's editors do and the Slint app's
//! server and channel settings: the editor stays and its bar shakes red
//! until the changes are saved or reset ([`close`]).

mod menus;
mod pick;
mod style;
#[cfg(test)]
mod tests;

use iced::Task;
use yapfast_core::application::{Action, PersonAsk, ProfileText};
use yapfast_core::chat::ProfilePicture;

use anomi_people::editor::{BIO, COLOUR, NAME, PRONOUNS, TAG_SEARCH};
use anomi_vm::people;
use anomi_vm::people::editor::{Field, Handle, Intent as Edit, Picture, TagMenu};
use anomi_vm::people::editor_preview::{Colour, Intent as Preview, Page, format_hex};

use crate::app::App;
use crate::message::Message;

/// `Editor`, `EditorPreview` and `NameStyle` of `people::Intent`.
pub fn handle(app: &mut App, intent: people::Intent) -> Task<Message> {
    match intent {
        people::Intent::Editor(intent) => edit(app, intent),
        people::Intent::EditorPreview(intent) => preview(app, intent),
        people::Intent::NameStyle(intent) => style::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not the profile editor's");
            Task::none()
        }
    }
}

/// The core's name for a text of the editor's.
fn text(field: Field) -> ProfileText {
    match field {
        Field::Name => ProfileText::Name,
        Field::Pronouns => ProfileText::Pronouns,
        Field::Bio => ProfileText::Bio,
        Field::Colour => ProfileText::Colour,
    }
}

/// The core's name for a picture of the profile.
fn kind(picture: Picture) -> ProfilePicture {
    match picture {
        Picture::Avatar => ProfilePicture::Avatar,
        Picture::Banner => ProfilePicture::Banner,
    }
}

/// The text being edited is left (`stop-editing`).
fn stop_editing(app: &mut App) {
    app.vm.people.editor.editing = None;
}

fn edit(app: &mut App, intent: Edit) -> Task<Message> {
    let editor = &mut app.vm.people.editor;
    match intent {
        Edit::Write(field, text_written) => app.dispatch(Action::WriteProfile {
            field: text(field),
            text: text_written,
        }),
        // A text becomes a field with the keys in it (`edit`); `None`, as a
        // press on the dialog away from its parts, leaves it and the banner
        // colour's box (`stop-editing`).
        Edit::Edit(field) => {
            editor.editing = field;
            editor.colour_open = false;
            editor.handle = None;
            let Some(field) = field else {
                return Task::none();
            };
            anomi_widgets::field::focus(match field {
                Field::Name => NAME,
                Field::Pronouns => PRONOUNS,
                Field::Bio => BIO,
                Field::Colour => COLOUR,
            })
        }
        Edit::ColourBox(open) => {
            editor.editing = None;
            editor.colour_open = open;
            if open {
                anomi_widgets::field::focus(COLOUR)
            } else {
                Task::none()
            }
        }
        Edit::Pick(picture) => {
            stop_editing(app);
            pick::choose(app, kind(picture))
        }
        Edit::Reset(picture) => {
            stop_editing(app);
            app.dispatch(Action::ResetProfilePicture(kind(picture)))
        }
        Edit::PictureMenu(picture, spot) => menus::picture(app, picture, spot),
        // Its search has the keys as it opens.
        Edit::TagMenu(spot) => {
            editor.editing = None;
            editor.tag_menu = Some(TagMenu {
                at: spot.anchor,
                search: String::new(),
            });
            anomi_widgets::field::focus(TAG_SEARCH)
        }
        Edit::TagSearch(search) => {
            if let Some(menu) = &mut app.vm.people.editor.tag_menu {
                menu.search = search;
            }
            Task::none()
        }
        Edit::CloseTagMenu => {
            app.vm.people.editor.tag_menu = None;
            Task::none()
        }
        Edit::ChooseTag(server) => {
            app.vm.people.editor.tag_menu = None;
            app.dispatch(Action::ChooseServerTag(server))
        }
        Edit::MoreMenu(spot) => menus::more(app, spot),
        Edit::CopyId => {
            let id = app.core.state.discord_user.clone().unwrap_or_default();
            if id.is_empty() {
                return Task::none();
            }
            iced::clipboard::write(id).discard()
        }
        Edit::Handle(at, y) => {
            editor.editing = None;
            if editor.handle == Some(at) {
                editor.handle = None;
                return Task::none();
            }
            let start = app
                .core
                .state
                .profile_editor
                .as_ref()
                .map_or("", |core| match at {
                    Handle::Top => core.theme_top.as_str(),
                    Handle::Bottom => core.theme_bottom.as_str(),
                });
            editor.preview.colour = Colour::start(start);
            editor.handle = Some(at);
            editor.handle_y = y;
            Task::none()
        }
        Edit::CloseHandle => {
            editor.handle = None;
            Task::none()
        }
        Edit::OpenStyle => {
            stop_editing(app);
            style::open(app);
            Task::none()
        }
        Edit::EditStatus => {
            stop_editing(app);
            super::status::open(app)
        }
        // One's own note, in the dialog about a person (`edit-own-note`).
        Edit::EditNote => {
            stop_editing(app);
            let Some(user) = app.core.state.discord_user.clone() else {
                return Task::none();
            };
            app.dispatch(Action::AskPerson {
                user,
                ask: PersonAsk::Note,
                location: "ContextMenu",
            })
        }
        Edit::OpenLink(url) => crate::platform::links::follow(app, &url, false),
        Edit::UnlockNitro => app.dispatch(Action::UnlockNitro),
        Edit::Side(open) => {
            editor.side_open = open;
            Task::none()
        }
        Edit::Undo => {
            editor.editing = None;
            editor.colour_open = false;
            editor.handle = None;
            app.dispatch(Action::UndoProfileEdits)
        }
        Edit::Save => {
            stop_editing(app);
            app.dispatch(Action::SaveProfile)
        }
        Edit::Close => close(app),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The preview's tabs and a theme colour's picker.
fn preview(app: &mut App, intent: Preview) -> Task<Message> {
    let editor = &mut app.vm.people.editor;
    match intent {
        Preview::Choose(page) => {
            editor.preview.page = page;
            // The wishlist is asked for as its tab first opens.
            if page == Page::Wishlist {
                app.dispatch(Action::OpenWishlist)
            } else {
                Task::none()
            }
        }
        Preview::Link(url) => crate::platform::links::follow(app, &url, false),
        // The picker's colour, given one, is the open handle's.
        Preview::Colour(change) => {
            let Some(handle) = editor.handle else {
                return Task::none();
            };
            let Some(colour) = editor.preview.colour.apply(change) else {
                return Task::none();
            };
            let field = match handle {
                Handle::Top => ProfileText::ThemeTop,
                Handle::Bottom => ProfileText::ThemeBottom,
            };
            app.dispatch(Action::WriteProfile {
                field,
                text: format!("#{}", format_hex(colour)),
            })
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Escape or a press beside the dialog: the name style window over it
/// goes first, then a theme colour's picker or the tags' chooser, then what's being edited
/// (`ui/server.slint:1741`); then the editor, as [`close`] lets it.
pub(crate) fn escape(app: &mut App) -> Task<Message> {
    let people = &mut app.vm.people;
    if people.name_style.open {
        return style::handle(app, people::name_style::Intent::Close);
    }
    let editor = &mut people.editor;
    if editor.handle.is_some() || editor.tag_menu.is_some() {
        editor.handle = None;
        editor.tag_menu = None;
        return Task::none();
    }
    if editor.editing.is_some() || editor.colour_open {
        editor.editing = None;
        editor.colour_open = false;
        return Task::none();
    }
    close(app)
}

/// The editor's X, Escape or a press beside it: the core lets the editor
/// go and the projection closes it; with changes not saved, it stays and
/// its bar shakes, as Discord's does, until they're saved or reset.
pub(crate) fn close(app: &mut App) -> Task<Message> {
    let editor = &mut app.vm.people.editor;
    if editor.changed || editor.saving {
        editor.editing = None;
        editor.handle = None;
        editor.nudge = editor.nudge.wrapping_add(1);
        return Task::none();
    }
    app.dispatch(Action::CloseProfileEditor)
}
