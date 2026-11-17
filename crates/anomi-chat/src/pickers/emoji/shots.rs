//! The emoji picker's shots, named after the golden screenshots
//! (`picker/reaction_emoji.png`, `picker/composer_emoji.png`), and its other
//! states. Its animated emoji play the fake's bouncing GIF.
//!
//! The composer's opens this popup over the composer's emoji button until
//! the picker above the composer (C18) has its tabs; that one then shows
//! [`super::bare`] under them, as the golden shot does.

use iced::widget::image;
use iced::{Point, Rectangle, Size};

use anomi_vm::Vm;
use anomi_vm::chat::emoji_picker::EmojiPicker;
use anomi_vm::fixtures::chat::emoji_picker as sample;
use anomi_vm::fixtures::shell;
use anomi_vm::menu::{Align, Spot};
use anomi_vm::shell::Layer;
use anomi_widgets::picture::sample::bouncing;

use crate::shots::{GOLDEN, Shot};

/// The fake's animated emoji (`/emojis/{id}.gif`): its bouncing dot.
fn moving(_id: &str) -> image::Handle {
    bouncing(96, 10, [124, 196, 255])
}

/// `picker` in the popup, opened from a message's menu right-clicked at
/// (700, 300): 4 px under where the menu was (`anchor-y + 4px`,
/// `ui/yapfast.slint:2651`).
fn at_message(picker: EmojiPicker) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.emoji_picker = picker;
    let spot = Spot::pointer(Point::new(700.0, 300.0)).gap(4.0);
    shell::open_at(vm, Layer::EmojiPicker, spot)
}

fn reaction() -> Vm {
    at_message(sample::reaction_with(moving))
}

/// Over the composer's emoji button, its right edge on the button's and
/// 8 px over the composer (`ui/yapfast.slint:2537`-`:2538`).
fn composer() -> Vm {
    let mut vm = shell::server_home();
    vm.chat.emoji_picker = sample::composer_with(moving);
    let button = Rectangle::new(Point::new(1089.0, 795.0), Size::new(28.0, 28.0));
    let spot = Spot::above(button).align(Align::End).gap(8.0);
    shell::open_at(vm, Layer::EmojiPicker, spot)
}

fn searched() -> Vm {
    at_message(sample::searched("par"))
}

fn nothing() -> Vm {
    at_message(sample::nothing())
}

fn hovered() -> Vm {
    at_message(sample::hovered())
}

fn waiting() -> Vm {
    at_message(sample::waiting())
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "picker",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("reaction_emoji", reaction),
        shot("composer_emoji", composer),
        shot("emoji_search", searched),
        shot("emoji_nothing", nothing),
        shot("emoji_hovered", hovered),
        shot("emoji_waiting", waiting),
    ]
}
