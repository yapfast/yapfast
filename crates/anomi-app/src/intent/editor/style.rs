//! The Display Name Style window over the editor (`NameStyleWindow`,
//! `ui/server.slint:2330`): it opens on the style as written, on the
//! editor's card; it keeps its own choices (`NameStyle::update`), and Apply
//! hands them to the core with the editor's other changes
//! (`choose-name-style`, `src/ui/callbacks.rs:1426`), which saves them only
//! with Save.

use iced::Task;
use yapfast_core::application::{Action, ProfileText};

use anomi_vm::people::name_style::{Intent, NameStyle};

use crate::app::App;
use crate::message::Message;

/// Opens the window, where this account may style its name.
pub(super) fn open(app: &mut App) {
    let Some(editor) = app
        .core
        .state
        .profile_editor
        .as_ref()
        .filter(|editor| editor.may_write(ProfileText::ThemeTop))
    else {
        return;
    };
    let key = editor
        .name_colours
        .iter()
        .map(|colour| format!("{colour:06x}"))
        .collect::<Vec<_>>()
        .join(",");
    let font = editor.name_font.unwrap_or(11);
    let card = app.vm.people.editor.card.clone();
    app.vm.people.name_style = NameStyle::opened(card, font, editor.name_effect, &key);
}

/// What the window asks.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let window = &mut app.vm.people.name_style;
    window.update(&intent);
    match intent {
        Intent::Apply {
            font,
            effect,
            colours,
        } => app.dispatch(Action::ChooseNameStyle {
            font,
            effect: effect.max(1),
            colours,
        }),
        // Gone: the card it held goes with it.
        Intent::Gone if !window.open => {
            *window = NameStyle::default();
            Task::none()
        }
        _ => Task::none(),
    }
}
