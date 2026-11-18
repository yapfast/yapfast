//! Dragging in a server's channel list (`src/ui/callbacks.rs:2945`-`:3024`):
//! a Discord channel or category to a new place, by the places the list
//! numbered; someone in a voice channel onto another, which moves them as
//! their menu's Move to does. What's dragged is kept in
//! `project::sidebar::drags::Drags`, which each refresh shows.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::sidebar::channels::{Arrange, Move};

use crate::app::App;
use crate::message::Message;
use crate::project::sidebar::places;

pub fn arrange(app: &mut App, arrange: Arrange) -> Task<Message> {
    match arrange {
        Arrange::Start(place) => {
            let room = place
                .checked_sub(1)
                .and_then(|at| places(&app.shown.sidebar).get(at as usize))
                .cloned();
            app.shown.sidebar.drags.arranging = room;
            app.refresh();
            Task::none()
        }
        // The list draws where it would land; the core is asked as it drops.
        Arrange::Over(_) => Task::none(),
        Arrange::Drop(place) => {
            let room = app.shown.sidebar.drags.arranging.take();
            let shown = places(&app.shown.sidebar);
            let landing = room.as_ref().and_then(|room| {
                let dragged = shown.iter().position(|known| known == room)?;
                let onto = (place as usize).checked_sub(1)?;
                app.core.landing(shown, dragged, onto)
            });
            match (room, landing) {
                (Some(room), Some(landing)) => app.dispatch(Action::MoveChannel { room, landing }),
                _ => {
                    app.refresh();
                    Task::none()
                }
            }
        }
        Arrange::Stop => {
            app.shown.sidebar.drags.arranging = None;
            app.refresh();
            Task::none()
        }
    }
}

pub fn moving(app: &mut App, moving: Move) -> Task<Message> {
    let drags = &mut app.shown.sidebar.drags;
    match moving {
        // Not while a channel is dragged.
        Move::Start { channel, user } => {
            if drags.arranging.is_none() {
                drags.moving = Some((channel, user));
                drags.over = None;
            }
        }
        Move::Over(room) => drags.over = room,
        Move::Drop(to) => {
            let action = drags
                .moving
                .as_ref()
                .and_then(|(place, user)| app.core.move_to(place, user, &to));
            app.shown.sidebar.drags.stop_moving();
            if let Some(action) = action {
                return app.dispatch(action);
            }
        }
        Move::Stop => drags.stop_moving(),
    }
    app.refresh();
    Task::none()
}
