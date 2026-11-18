//! The GIF picker's intents (`src/ui/gifs.rs:431`, `ui/gifs.slint:95`):
//! typing searches once it pauses for a quarter second, as Discord's client
//! does, Favorites filter at once; a tile sends its GIF or puts it in the
//! message; the grid's scroll asks for the pictures within a screen of the
//! view at the size they show at, and lets go of the rest.

use std::time::Duration;

use iced::Task;
use yapfast_core::application::{Action, GifView};

use anomi_vm::chat::gif_picker::{Intent, Page};
use anomi_vm::chat::pickers::Tab;
use anomi_vm::shell::Layer;

use super::{Due, showing};
use crate::app::{App, later};
use crate::message::Message;

/// The pause in typing a search waits for (`pause`, `ui/gifs.slint:98`).
const PAUSE: Duration = Duration::from_millis(250);

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Edit(text) => {
            app.vm.chat.pickers.gif.query = text;
            let shown = &mut app.shown.pickers;
            shown.typed += 1;
            later(PAUSE, Message::Pickers(Due::Search(shown.typed)))
        }
        Intent::Search(text) => {
            app.shown.pickers.typed += 1;
            app.vm.chat.pickers.gif.query.clone_from(&text);
            app.dispatch(Action::SearchGifs(text))
        }
        Intent::Back => {
            app.shown.pickers.typed += 1;
            app.vm.chat.pickers.gif.query.clear();
            app.dispatch(Action::ShowGifs(GifView::Home))
        }
        Intent::Show(page) => app.dispatch(Action::ShowGifs(view(page))),
        Intent::Category(term) => app.dispatch(Action::SearchGifCategory(term)),
        Intent::Pick { url, insert } => {
            let task = app.dispatch(Action::PickGif { url, insert });
            Task::batch([task, super::close(app, Layer::Expressions)])
        }
        Intent::Favorite(url) => app.dispatch(Action::FavoriteGif(url)),
        Intent::Shown { offset, height } => {
            app.vm.chat.pickers.gif.shown = (offset, height);
            ask(app)
        }
        Intent::Shift(held) => {
            app.vm.chat.pickers.gif.shift = held;
            Task::none()
        }
        Intent::Close => super::close(app, Layer::Expressions),
        intent => {
            tracing::debug!(?intent, "not the GIF picker's");
            Task::none()
        }
    }
}

fn view(page: Page) -> GifView {
    match page {
        Page::Home => GifView::Home,
        Page::Trending => GifView::Trending,
        Page::Favorites => GifView::Favorites,
        Page::Search => GifView::Search,
    }
}

/// The pause after the last of what was typed ended: it searches, unless
/// more was typed or the picker went meanwhile.
pub(super) fn search_due(app: &mut App, typed: u64) -> Task<Message> {
    if typed != app.shown.pickers.typed || !showing(app, Tab::Gifs) {
        return Task::none();
    }
    let query = app.vm.chat.pickers.gif.query.clone();
    app.dispatch(Action::SearchGifs(query))
}

/// Asks for the pictures of the tiles within a screen of the view, at the
/// size they show at on this display, and says whether the grid nears its
/// end; nothing when that is what was asked last.
pub(super) fn ask(app: &mut App) -> Task<Message> {
    app.shown.pickers.gifs.ask = false;
    if !showing(app, Tab::Gifs) {
        return Task::none();
    }
    let pickers = &app.vm.chat.pickers;
    let sight = pickers
        .gif
        .sight(pickers.shown(), anomi_widgets::picture::scale());
    let gifs = &mut app.shown.pickers.gifs;
    if gifs.asked.as_ref() == Some(&sight) {
        return Task::none();
    }
    gifs.asked = Some(sight.clone());
    app.dispatch(Action::GifsShown {
        shown: sight.asked,
        near_end: sight.near_end,
    })
}
