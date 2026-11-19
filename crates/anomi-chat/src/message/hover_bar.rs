//! The hover bar over the lit message, and the ground under every message.
//! The Slint side: `message-bar` and its actions (`ui/yapfast.slint:3951`-
//! `:4039`). On someone else's message: the three frequent reactions, a
//! line, the reaction picker, Reply, Forward and More; on one's own, Edit
//! where Reply was. Holding Shift swaps the frequent reactions for Pin,
//! Create Thread and, on one's own message, Reply, and More for a Delete
//! that doesn't ask. While its deletion waits, "Delete for everyone?" with
//! Delete and Cancel stands in the bar's place. [`frame`] is the ground.

mod frame;
mod raise;

pub use frame::{Ground, frame};

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Background, Border, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::message::{Bar, Intent, Message, Quick};
use anomi_vm::chat::{self};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::menu::opener;
use anomi_widgets::picture::picture;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use raise::{Place, raise};

/// The card: 3 px round its buttons, 8 px corners, a soft shadow below
/// (`:3964`-`:3965`, `:3973`).
const PADDING: f32 = 3.0;
const RADIUS: f32 = 8.0;
const SHADOW_BLUR: f32 = 10.0;
const SHADOW_Y: f32 = 2.0;
const SHADOW_ALPHA: f32 = 0x59 as f32 / 255.0;
/// A frequent reaction's button: 32 px, its emoji 20, 6 px corners, the
/// overlay at 12% under the pointer (`EmojiButton`, `ui/messages.slint:179`).
const EMOJI_BUTTON: f32 = 32.0;
const EMOJI: f32 = 20.0;
/// The text size whose Twemoji glyph is that big: the glyph runs to 72/53
/// of its text's size, as in the paragraph.
const EMOJI_TEXT: f32 = EMOJI * 53.0 / 72.0;
const EMOJI_RADIUS: f32 = 6.0;
const EMOJI_HOVER: f32 = 0.12;
/// The line after them: 1 by 20 px in a 9 px slot (`:4006`).
const LINE_SLOT: f32 = 9.0;
const LINE_HEIGHT: f32 = 20.0;
/// "Delete for everyone?": 8 px in, its parts 4 apart (`:3975`).
const CONFIRM_INSET: f32 = 8.0;
const CONFIRM_GAP: f32 = 4.0;
/// The message's padding over its text (`padding-top`, `ui/messages.slint:3085`).
pub const TOP: f32 = 2.0;

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::from(intent)
}

/// The bar's slot at the top of `message`'s column of text, `above` being
/// the space between the message and its row's top, `before` and `after`
/// the row's width round the column. Narrow, the bar leaves out the
/// frequent reactions, as Discord's compact one does, so it covers less of
/// the message.
pub fn view<'a>(
    message: &'a Message,
    bar: &'a Bar,
    above: f32,
    [before, after]: [f32; 2],
    motion: Motion,
) -> Element<'a, chat::Intent> {
    let key = message.key;
    let wide = card(key, bar, true, motion);
    let narrows = bar.may.react && !bar.shift && !bar.confirming && !bar.frequent.is_empty();
    let narrow = narrows.then(|| card(key, bar, false, motion));
    let place = Place {
        edge: message.edge,
        before,
        after,
        row_top: above + TOP,
        message_top: TOP,
    };
    raise(wide, narrow, place)
}

fn card<'a>(key: u64, bar: &'a Bar, frequent: bool, motion: Motion) -> Element<'a, chat::Intent> {
    let content = if bar.confirming {
        confirm(key, motion)
    } else {
        actions(key, bar, frequent, motion)
    };
    container(content)
        .padding(PADDING)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.raised)),
                border: Border {
                    color: palette.edge,
                    width: 1.0,
                    radius: RADIUS.into(),
                },
                shadow: Shadow {
                    color: iced::Color::BLACK.scale_alpha(SHADOW_ALPHA),
                    offset: Vector::new(0.0, SHADOW_Y),
                    blur_radius: SHADOW_BLUR,
                },
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The actions, the frequent reactions among them while `frequent`.
fn actions<'a>(
    key: u64,
    bar: &'a Bar,
    frequent: bool,
    motion: Motion,
) -> Element<'a, chat::Intent> {
    let (may, shift) = (bar.may, bar.shift);
    let action = |icon: Icon, label: &'static str, intent: Intent| -> Element<'a, chat::Intent> {
        icon_button(icon, label, motion)
            .on_press(ask(intent))
            .view()
            .boxed()
    };
    let mut parts = row![].align_y(Alignment::Center);
    if shift && may.pin {
        parts = parts.push(if bar.pinned {
            action(Icon::PinOff, tr!("Unpin"), Intent::Pin(key))
        } else {
            action(Icon::Pin, tr!("Pin"), Intent::Pin(key))
        });
    }
    if shift && may.thread {
        parts = parts.push(action(
            Icon::Thread,
            tr!("Create Thread"),
            Intent::NewThread(key),
        ));
    }
    if shift && may.edit && may.reply {
        parts = parts.push(action(Icon::Reply, tr!("Reply"), Intent::Reply(key)));
    }
    if may.react {
        if !shift && frequent {
            for quick in &bar.frequent {
                parts = parts.push(reaction(key, quick, motion));
            }
            parts = parts.push(line());
        }
        let picker = icon_button(Icon::SmilePlus, tr!("Add reaction"), motion).view();
        let picker =
            opener(picker).on_press(move |button| ask(Intent::Picker(key, Spot::beside(button))));
        parts = parts.push(picker.boxed());
    }
    if may.edit {
        parts = parts.push(action(Icon::Pencil, tr!("Edit"), Intent::Edit(key)));
    } else if may.reply {
        parts = parts.push(action(Icon::Reply, tr!("Reply"), Intent::Reply(key)));
    }
    if may.forward {
        parts = parts.push(action(
            Icon::Forward,
            tr!("YapfastWindow" => "Forward"),
            Intent::Forward(key),
        ));
    }
    if shift && may.delete {
        let delete = icon_button(Icon::Trash, tr!("Delete"), motion).danger(true);
        parts = parts.push(delete.on_press(ask(Intent::DeleteNow(key))).view().boxed());
    } else {
        let more = icon_button(Icon::Ellipsis, tr!("More"), motion).view();
        let more =
            opener(more).on_press(move |button| ask(Intent::Menu(key, Spot::beside(button))));
        parts = parts.push(more.boxed());
    }
    parts.boxed()
}

/// One of the frequent reactions: its emoji on a button that lifts under
/// the pointer, named in its tooltip.
fn reaction<'a>(key: u64, quick: &'a Quick, motion: Motion) -> Element<'a, chat::Intent> {
    let face: Element<'a, chat::Intent> = match &quick.picture {
        Some(still) => picture(still.clone()).side(EMOJI).motion(motion).boxed(),
        None => text(quick.key.as_str()).size(EMOJI_TEXT).boxed(),
    };
    let pressed = button(container(face).center(Length::Fixed(EMOJI_BUTTON)))
        .padding(0)
        .on_press(ask(Intent::React(key, quick.key.clone())))
        .style(|theme: &Theme, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| Background::Color(theme.palette().lift(EMOJI_HOVER))),
            border: Border::default().rounded(EMOJI_RADIUS),
            ..button::Style::default()
        });
    tip(pressed, quick.name.as_str(), Side::Above, motion).boxed()
}

/// The line between the frequent reactions and the rest.
fn line<'a>() -> Element<'a, chat::Intent> {
    let rule = container(space().width(1.0).height(LINE_HEIGHT))
        .style(|theme: &Theme| container::Style::default().background(theme.palette().line));
    container(rule).center_x(LINE_SLOT).boxed()
}

/// "Delete for everyone?" with Delete and Cancel.
fn confirm<'a>(key: u64, motion: Motion) -> Element<'a, chat::Intent> {
    row![
        text(tr!("Delete for everyone?"))
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        action_button(tr!("Delete"), Kind::Danger, motion)
            .small()
            .on_press(ask(Intent::ConfirmDelete(key))),
        action_button(tr!("Cancel"), Kind::Ghost, motion)
            .small()
            .on_press(ask(Intent::CancelDelete(key))),
    ]
    .spacing(CONFIRM_GAP)
    .padding(Padding::default().left(CONFIRM_INSET))
    .align_y(Alignment::Center)
    .boxed()
}
