//! The threads popover (`ThreadsPopover`: `search`, `create`, `open`,
//! `more-threads`, `close`; `src/ui/callbacks.rs:1643`-`:1646`, `:1765`)
//! and the "New Thread" form (`NewThreadPanel`: `:1677`-`:1709`, `:4339`).

use iced::Task;
use yapfast_core::application::{Action, ThreadFrom};

use anomi_vm::chat::new_thread::Intent as NewIntent;
use anomi_vm::chat::threads::{Intent, ThreadFrom as From};
use anomi_vm::shell::Layer;
use anomi_widgets::focus;

use crate::app::App;
use crate::intent::chat::open_link;
use crate::message::Message;

pub(super) fn popover(app: &mut App, intent: Intent) -> Task<Message> {
    let threads = &mut app.vm.chat.threads;
    match intent {
        // The cross, or the field emptied: the list again.
        Intent::Edit(text) => {
            let emptied = text.is_empty() && !threads.query.is_empty();
            threads.query = text;
            if emptied {
                return app.dispatch(Action::SearchThreads(String::new()));
            }
            Task::none()
        }
        Intent::Search(text) => app.dispatch(Action::SearchThreads(text)),
        // As Discord's: in place of the channel, or with Shift beside it;
        // opening first, as it may use what the list found.
        Intent::Open { room, beside } => {
            let opened = app.dispatch(if beside {
                Action::OpenAside(room)
            } else {
                Action::Open(room)
            });
            Task::batch([opened, crate::intent::header::close(app, Layer::Threads)])
        }
        // Only a scroll of the person's near the end asks for more, and
        // only while more is wanted and none is on its way.
        Intent::Scrolled { offset, near_end } => {
            if offset > 0.0 && near_end && threads.more && !threads.loading {
                return app.dispatch(Action::MoreThreads);
            }
            Task::none()
        }
        Intent::Focus(focused) => {
            threads.focused = focused;
            Task::none()
        }
        // The form opens beside the channel; the core closes the list.
        Intent::Create(from) => app.dispatch(Action::NewThread {
            message: None,
            from: match from {
                From::List => ThreadFrom::List,
                From::ListEmpty => ThreadFrom::ListEmpty,
            },
        }),
        Intent::Close => crate::intent::header::close(app, Layer::Threads),
        intent => {
            tracing::debug!(?intent, "not the threads popover's");
            Task::none()
        }
    }
}

pub(super) fn new(app: &mut App, intent: NewIntent) -> Task<Message> {
    match intent {
        NewIntent::Name(name) => app.dispatch(Action::WriteNewThread {
            name: Some(name),
            body: None,
        }),
        NewIntent::ToBody => focus::to(focus::ASIDE_COMPOSER),
        NewIntent::Body { text, .. } => app.dispatch(Action::WriteNewThread {
            name: None,
            body: Some(text),
        }),
        NewIntent::Focus(focused) => {
            app.vm.chat.new_thread.focused = focused;
            // Typing anywhere comes here now (`aside-composer-last`).
            if focused {
                app.aside_last = true;
            }
            Task::none()
        }
        NewIntent::Private(private) => app.dispatch(Action::SetThreadPrivate(private)),
        NewIntent::Create => app.dispatch(Action::CreateThread),
        NewIntent::Close => app.dispatch(Action::CloseNewThread),
        NewIntent::Leave => app.dispatch(Action::LeaveNewThread),
        NewIntent::Stay => app.dispatch(Action::StayInNewThread),
        NewIntent::Jump { room, item } => app.dispatch(Action::JumpInRoom { room, item }),
        NewIntent::Link(link) => open_link(app, link),
        intent => {
            tracing::debug!(?intent, "not the new thread's");
            Task::none()
        }
    }
}
