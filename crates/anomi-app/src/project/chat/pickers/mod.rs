//! The pickers from the core's state: the emoji picker (`show_emoji`,
//! `src/ui/callbacks.rs:426`, with `src/ui/emoji_grid.rs`), the GIFs
//! (`show_gifs`, `src/ui/gifs.rs`) and the stickers (`show_stickers`,
//! `src/ui/stickers.rs`), and the pictures that come for them
//! (`show_emoji_pictures` and `show_picker_pictures`,
//! `src/ui/refresh.rs:487`, `:510`).
//!
//! A closed picker holds nothing: its sections, tiles and the addresses
//! kept for them go as it plays its way out ([`gone`]), and [`show`]
//! returns at once. While one is open, its emoji are built as it opens and
//! as its search or its stars change (the intents,
//! `crate::intent::chat::pickers`); the GIFs and stickers as the core's
//! lists of them change. A picture that comes fills the rows near the view
//! in place: no row is built again for it, and nothing else in the window.

pub(crate) mod emoji;
pub(crate) mod gifs;
pub(crate) mod stickers;
#[cfg(test)]
mod tests;

use yapfast_core::application::Application;

use anomi_vm::Vm;
use anomi_vm::chat::emoji_picker::EmojiPicker;
use anomi_vm::chat::pickers::Tab;
use anomi_vm::shell::Layer;

/// What a picked emoji goes to besides its purpose
/// (`EmojiPicker::purpose`): the message a reaction is for, a poll's answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Target {
    #[default]
    None,
    /// A message, by its item.
    Message(String),
    /// The poll being written's answer, by its place.
    Answer(usize),
}

/// What the open pickers hold beside their view-models.
#[derive(Default)]
pub(crate) struct Shown {
    pub(crate) emoji: emoji::Held,
    pub(crate) gifs: gifs::Shown,
    pub(crate) stickers: stickers::Shown,
    pub(crate) target: Target,
    /// The searches typed into the GIFs' field: the last goes once typing
    /// pauses (`pause`, `ui/gifs.slint:98`).
    pub(crate) typed: u64,
}

/// The emoji picker that shows: the popup, or the Emoji tab of the picker
/// above the composer. None while neither does.
pub(crate) fn emoji_shown(vm: &mut Vm) -> Option<&mut EmojiPicker> {
    if vm.shell.is_open(Layer::EmojiPicker) {
        return Some(&mut vm.chat.emoji_picker);
    }
    let tab = vm.shell.is_open(Layer::Expressions) && vm.chat.pickers.tab == Tab::Emoji;
    tab.then_some(&mut vm.chat.pickers.emoji)
}

/// Shows what came for the picker that shows: the pictures of its emoji,
/// GIFs or stickers, and the GIFs and stickers the core listed again.
/// Nothing while no picker shows.
pub(crate) fn show(core: &Application, vm: &mut Vm, shown: &mut Shown) {
    let state = &core.state;
    if let Some(picker) = emoji_shown(vm) {
        emoji::fill(state, picker, &mut shown.emoji);
        return;
    }
    if !vm.shell.is_open(Layer::Expressions) {
        return;
    }
    let pickers = &mut vm.chat.pickers;
    match pickers.tab {
        Tab::Gifs => gifs::show(state, &mut pickers.gif, &mut shown.gifs),
        Tab::Stickers => stickers::show(core, &mut pickers.stickers, &mut shown.stickers),
        Tab::Emoji => {}
    }
}

/// A picker played its way out: everything it held goes, its view-model's
/// rows and what was kept for them, unless another picker shows now.
pub(crate) fn gone(vm: &mut Vm, shown: &mut Shown) {
    let popup = vm.shell.is_open(Layer::EmojiPicker);
    let expressions = vm.shell.is_open(Layer::Expressions);
    if !popup {
        vm.chat.emoji_picker = EmojiPicker::default();
    }
    if !expressions {
        let pickers = &mut vm.chat.pickers;
        pickers.emoji = EmojiPicker::default();
        pickers.gif = Default::default();
        pickers.stickers = Default::default();
        shown.gifs = Default::default();
        shown.stickers = Default::default();
    }
    if !popup && !expressions {
        *shown = Shown::default();
    }
}
