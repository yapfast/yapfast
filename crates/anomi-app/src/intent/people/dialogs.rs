//! The dialogs about a person (`AnswerPerson`, `ChoosePersonOption`,
//! `ClosePersonDialog`, `skip-call-prompt-toggled`) and the friends picker
//! of a DM (`add-people-search`, `choose-person`, `confirm-add-people`,
//! `close-add-people`).

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::people::{add_people, person};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;

pub(super) fn person(app: &mut App, intent: person::Intent) -> Task<Message> {
    use person::Intent as I;
    match intent {
        I::Text(text) => {
            app.vm.people.person.text_now = text;
            Task::none()
        }
        I::Answer(text) if app.vm.people.person.may_answer() => {
            app.dispatch(Action::AnswerPerson(text))
        }
        I::Choose(index) => app.dispatch(Action::ChoosePersonOption(index)),
        I::Tick(skip) => app.dispatch(Action::SkipCallPrompt(skip)),
        I::Close => close(app),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The dialog closes: the core lets it go, and the projection closes it.
pub(crate) fn close(app: &mut App) -> Task<Message> {
    app.vm.shell.close(Layer::Person);
    if app.core.state.person_dialog.is_some() {
        app.dispatch(Action::ClosePersonDialog)
    } else {
        Task::none()
    }
}

pub(super) fn add_people(app: &mut App, intent: add_people::Intent) -> Task<Message> {
    use add_people::Intent as I;
    match intent {
        I::Query(query) => {
            app.vm.people.add_people.query.clone_from(&query);
            app.dispatch(Action::AddPeopleQuery(query))
        }
        I::Choose(user) => app.dispatch(Action::ChoosePerson(user)),
        I::Add if app.vm.people.add_people.may_add() => app.dispatch(Action::AddPeople),
        I::Close => super::super::header::close(app, Layer::AddPeople),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
