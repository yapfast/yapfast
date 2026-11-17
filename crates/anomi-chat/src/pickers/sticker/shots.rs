//! The sticker picker's shots, named after the golden screenshot
//! (`picker/sticker.png`) and its other states. Its animated stickers play
//! the fake's bouncing GIF.
//!
//! The golden shot is the picker above the composer on its Stickers tab,
//! which C18's `pickers::view` draws around [`super::bare`] from
//! `vm.chat.pickers.stickers`: each shot opens that layer over the composer's
//! sticker button, on the third tab, with the sample in it.

use iced::widget::image;
use iced::{Point, Rectangle, Size};

use anomi_vm::Vm;
use anomi_vm::chat::pickers::Tab;
use anomi_vm::chat::sticker_picker::StickerPicker;
use anomi_vm::fixtures::chat::sticker_picker as sample;
use anomi_vm::fixtures::shell;
use anomi_vm::menu::{Align, Spot};
use anomi_vm::shell::Layer;
use anomi_widgets::picture::sample::bouncing;

use crate::shots::{GOLDEN, Shot};

/// The fake's animated stickers (`/stickers/{id}.gif`): its bouncing dot.
/// The fake's Lottie stickers (`testing/fixtures/sticker.json`) draw a 24 px
/// red square at half strength, 12 px in from the corner, on nothing.
fn moving(id: &str) -> image::Handle {
    if id == sample::LOTTIE || id == sample::PUFF {
        return lottie();
    }
    bouncing(96, 10, [124, 196, 255])
}

fn lottie() -> image::Handle {
    let mut pixels = vec![0u8; 96 * 96 * 4];
    for y in 12..36 {
        for x in 12..36 {
            let at = (y * 96 + x) * 4;
            pixels[at..at + 4].copy_from_slice(&[255, 0, 0, 128]);
        }
    }
    image::Handle::from_rgba(96, 96, pixels)
}

/// `picker` over the composer's sticker button, its right edge on the emoji
/// button's and 8 px over the composer (`ui/yapfast.slint:2537`-`:2538`).
fn above_composer(picker: StickerPicker) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.pickers.tab = Tab::Stickers;
    vm.chat.pickers.stickers = picker;
    let button = Rectangle::new(Point::new(1087.0, 795.0), Size::new(32.0, 32.0));
    let spot = Spot::above(button).align(Align::End).gap(8.0);
    shell::open_at(vm, Layer::Expressions, spot)
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
        shot("sticker", || above_composer(sample::picker_with(moving))),
        shot("sticker_locked", || {
            above_composer(sample::searched_locked_with("sticker_", moving))
        }),
        shot("sticker_favorites", || {
            above_composer(sample::favorited_with(moving))
        }),
        shot("sticker_search", || {
            above_composer(sample::searched_with("wa", moving))
        }),
        shot("sticker_nothing", || above_composer(sample::nothing())),
        shot("sticker_failed", || above_composer(sample::failed())),
        shot("sticker_hovered", || {
            above_composer(sample::hovered_with(moving))
        }),
        shot("sticker_waiting", || above_composer(sample::waiting())),
    ]
}
