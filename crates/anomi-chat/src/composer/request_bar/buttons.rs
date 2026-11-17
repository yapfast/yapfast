//! The buttons at the field's sides: the plus before it, and after it the
//! GIFs, the stickers and the emoji (`ui/yapfast.slint:4190`-`:4300`). They
//! stay on the field's last line as it grows, 14 px above the frame's
//! bottom.

use iced::widget::{container, row};
use iced::{Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::Palette;
use anomi_theme::prelude::*;
use anomi_vm::chat::composer::{Composer, Expression, Intent};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::composer::Expressions;
use anomi_widgets::menu::opener;

/// `composer-button-inset`: the room under a button.
const INSET: f32 = 14.0;
/// The plus is Discord's plain one, 27 px; the others 22.
const PLUS: f32 = 27.0;
const GLYPH: f32 = 22.0;

/// What Ctrl+G and Ctrl+E open, as the composer's intent says it.
pub fn expression(which: Expressions) -> Expression {
    match which {
        Expressions::Gifs => Expression::Gifs,
        Expressions::Emoji => Expression::Emoji,
    }
}

/// A button's place: on the last line.
fn last<'a>(button: impl View<Intent> + 'a) -> Element<'a, Intent> {
    container(button)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: INSET,
            left: 0.0,
        })
        .align_y(alignment::Vertical::Bottom)
        .boxed()
}

/// The plus: Discord's menu of uploading, starting a thread and creating a
/// poll where the room has more than files, else the file picker itself.
pub fn plus<'a>(vm: &'a Composer, motion: Motion) -> Element<'a, Intent> {
    let more = vm.more;
    let label = if more.menu() {
        tr!("More options")
    } else {
        tr!("Attach files")
    };
    let button = icon_button(Icon::Plus, label, motion)
        .glyph(PLUS)
        .enabled(more.any());
    if more.menu() {
        // The menu opens above it, 8 px off (`menu-y - height - 8px`), where
        // the window says the plus is.
        last(opener(button.view()).on_press(|button| {
            Intent::More(Spot {
                gap: 8.0,
                ..Spot::above(button)
            })
        }))
    } else {
        last(button.on_press(Intent::Attach).view())
    }
}

/// The GIFs, the stickers where the room has them, and the emoji.
pub fn expressions<'a>(vm: &'a Composer, motion: Motion) -> Element<'a, Intent> {
    let tint = |open: bool| -> fn(&Palette) -> iced::Color {
        if open { |p| p.text } else { |p| p.secondary }
    };
    let button = |symbol: Icon, label: &'a str, which: Expression| {
        last(
            icon_button(symbol, label, motion)
                .glyph(GLYPH)
                .enabled(vm.enabled)
                .tint(tint(vm.open == Some(which)))
                .on_press(Intent::Expressions(which))
                .view(),
        )
    };
    let mut buttons = row![].spacing(6);
    if vm.gifs {
        buttons = buttons.push(button(Icon::Gif, tr!("Open GIF picker"), Expression::Gifs));
    }
    if vm.stickers {
        buttons = buttons.push(button(
            Icon::Sticker,
            tr!("Open sticker picker"),
            Expression::Stickers,
        ));
    }
    buttons
        .push(button(Icon::Smile, tr!("Add an emoji"), Expression::Emoji))
        .boxed()
}
