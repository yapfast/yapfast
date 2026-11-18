//! Where the pickers open from, and their tabs: the composer's buttons and
//! Ctrl+G, Ctrl+E (`expressions(tab)`, `open-expressions`,
//! `ui/yapfast.slint:4250`, `:2504`), a message's Add Reaction, its reaction
//! picker and the keys' (`open-picker`, `:2249`), the emoji button of a
//! message edited in place (`open-edit-picker`, `:2245`), the custom
//! status's and a poll's answer's (`pick-emoji-beside`).
//!
//! Each tab of the picker above the composer holds what it lists while it
//! shows: the core opens the GIFs or the stickers as their tab shows and
//! lets go of them as another does (`show-expression-tab`, `:2515`).

use iced::{Point, Rectangle, Size, Task};
use yapfast_core::application::Action;

use anomi_vm::chat::composer::Expression;
use anomi_vm::chat::emoji_picker::{EmojiPicker, Purpose};
use anomi_vm::chat::gif_picker::GifPicker;
use anomi_vm::chat::pickers::{Tab, WIDTH};
use anomi_vm::chat::sticker_picker::StickerPicker;
use anomi_vm::menu::{Align, Spot};
use anomi_vm::shell::Layer;
use anomi_widgets::field;

use super::{Due, anchor, emoji, fit};
use crate::app::App;
use crate::message::Message;
use crate::project::chat::pickers::Target;

/// What the picker above the composer leaves between itself and the
/// composer's top, and the emoji popup between itself and what opened it
/// (`- 8px`, `+ 4px`, `ui/yapfast.slint:2546`, `:2651`).
const OVER_COMPOSER: f32 = 8.0;
const BESIDE: f32 = 4.0;
/// The room the picker leaves: right of the 72 px server rail and 8 px
/// more, or 8 px from the window's edge (`picker-room`, `:2207`).
const FROM_EDGE: f32 = 88.0;
const FROM_BUTTON: f32 = 80.0;

fn tab_of(which: Expression) -> Tab {
    match which {
        Expression::Gifs => Tab::Gifs,
        Expression::Emoji => Tab::Emoji,
        Expression::Stickers => Tab::Stickers,
    }
}

/// A button of the composer's, or Ctrl+G or Ctrl+E: its picker opens over
/// the composer, once the window said where it is; the one open closes.
pub(crate) fn from_composer(app: &mut App, which: Expression) -> Task<Message> {
    let composer = &app.vm.chat.composer;
    if which == Expression::Gifs && !composer.gifs {
        return Task::none();
    }
    if composer.open == Some(which) && app.vm.shell.is_open(Layer::Expressions) {
        return super::close(app, Layer::Expressions);
    }
    anchor::find(composer.room).map(move |found| Message::Pickers(Due::Anchored { which, found }))
}

/// The window said where the composer is: the picker opens there, the
/// three tabs where GIFs may be picked, else the emoji alone.
pub(crate) fn anchored(
    app: &mut App,
    which: Expression,
    found: [Option<Rectangle>; 2],
) -> Task<Message> {
    let [Some(buttons), Some(frame)] = found else {
        return Task::none();
    };
    let anchor = Rectangle::new(
        Point::new(buttons.x, frame.y),
        Size::new(buttons.width, 0.0),
    );
    if !app.vm.chat.composer.gifs {
        let spot = Spot::above(anchor).align(Align::End).gap(BESIDE);
        return popup(app, Purpose::Composer, Target::None, spot);
    }
    let tab = tab_of(which);
    app.vm.chat.composer.open = Some(which);
    if app.vm.shell.is_open(Layer::Expressions) {
        return self::tab(app, tab);
    }
    let room = (app.size.width - FROM_EDGE).min(buttons.x + buttons.width - FROM_BUTTON);
    let pickers = &mut app.vm.chat.pickers;
    pickers.room = room.max(WIDTH);
    pickers.above = frame.y;
    pickers.width = app.layout.picker_width.unwrap_or(WIDTH);
    pickers.tab = tab;
    pickers.emoji.purpose = Purpose::Composer;
    fit(app);
    app.shown.pickers.target = Target::None;
    let spot = Spot::above(anchor).align(Align::End).gap(OVER_COMPOSER);
    app.vm.shell.open_at(Layer::Expressions, spot);
    enter(app, tab)
}

/// A tab of the picker above the composer: the one shown lets go of what
/// it listed, and the other lists its own.
pub(crate) fn tab(app: &mut App, tab: Tab) -> Task<Message> {
    let current = app.vm.chat.pickers.tab;
    if current == tab {
        return Task::none();
    }
    let left = leave(app, current);
    app.vm.chat.composer.open = Some(match tab {
        Tab::Gifs => Expression::Gifs,
        Tab::Emoji => Expression::Emoji,
        Tab::Stickers => Expression::Stickers,
    });
    // What the tab left showed goes now; the core let go of it already.
    let pickers = &mut app.vm.chat.pickers;
    pickers.tab = tab;
    pickers.gif = GifPicker::default();
    pickers.stickers = StickerPicker::default();
    pickers.emoji = EmojiPicker {
        purpose: Purpose::Composer,
        ..EmojiPicker::default()
    };
    let shown = &mut app.shown.pickers;
    shown.gifs = Default::default();
    shown.stickers = Default::default();
    fit(app);
    Task::batch([left, enter(app, tab)])
}

/// The core lets go of what `tab` listed.
pub(crate) fn leave(app: &mut App, tab: Tab) -> Task<Message> {
    match tab {
        Tab::Gifs => app.dispatch(Action::CloseGifs),
        Tab::Stickers => app.dispatch(Action::CloseStickers),
        Tab::Emoji => emoji::let_go(app),
    }
}

/// `tab` lists what it shows and its search takes the keys.
fn enter(app: &mut App, tab: Tab) -> Task<Message> {
    let task = match tab {
        Tab::Gifs => app.dispatch(Action::OpenGifs),
        Tab::Stickers => app.dispatch(Action::OpenStickers),
        Tab::Emoji => emoji::build(app),
    };
    let field = match tab {
        Tab::Gifs => anomi_chat::pickers::gif::FIELD,
        Tab::Emoji => anomi_chat::pickers::emoji::FIELD,
        Tab::Stickers => anomi_chat::pickers::sticker::FIELD,
    };
    Task::batch([task, field::focus(field)])
}

/// The emoji popup at `spot`, for `purpose`.
fn popup(app: &mut App, purpose: Purpose, target: Target, spot: Spot) -> Task<Message> {
    app.vm.chat.emoji_picker = EmojiPicker {
        purpose,
        columns: anomi_vm::chat::emoji_picker::columns(anomi_vm::chat::emoji_picker::POPUP_WIDTH),
        ..EmojiPicker::default()
    };
    app.shown.pickers.target = target;
    app.vm.shell.open_at(Layer::EmojiPicker, spot);
    let task = emoji::build(app);
    Task::batch([task, field::focus(anomi_chat::pickers::emoji::FIELD)])
}

/// A message's reaction picker, by its row: from its menu's Add Reaction at
/// the menu's spot, its hover bar's button or the keys.
pub(crate) fn for_message(app: &mut App, key: u64, spot: Spot) -> Task<Message> {
    let Some(item) = (app.shown.timeline.message(key)).map(|message| message.item.clone()) else {
        return Task::none();
    };
    popup(app, Purpose::Reaction, Target::Message(item), spot)
}

/// The emoji button of the message edited in place: what is picked goes in
/// at its caret.
pub(crate) fn for_edit(app: &mut App, spot: Spot) -> Task<Message> {
    popup(app, Purpose::Edit, Target::None, spot)
}

/// The custom status's emoji button: the dialog's draft takes what is
/// picked (`intent::status::picked`).
pub(crate) fn for_status(app: &mut App, spot: Spot) -> Task<Message> {
    popup(app, Purpose::Status, Target::None, spot)
}

/// The emoji button of the poll being written's answer `index`.
pub(crate) fn for_poll(app: &mut App, index: usize, spot: Spot) -> Task<Message> {
    popup(app, Purpose::Poll, Target::Answer(index), spot)
}

/// Whether the reaction picker is open for `item` (a test's question).
#[cfg(test)]
pub(crate) fn reacting_to(app: &App) -> Option<&str> {
    match &app.shown.pickers.target {
        Target::Message(item) if app.vm.shell.is_open(Layer::EmojiPicker) => Some(item),
        _ => None,
    }
}
