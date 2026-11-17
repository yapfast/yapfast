//! The expression picker above the composer, with its tabs. Task C18
//! (docs/iced/tasks/chat.md), porting the popup (`expressions`,
//! `ui/yapfast.slint:2543`-`:2645`) and `PickerTab` (`ui/gifs.slint:235`),
//! as `open-expressions` and `show-expression-tab` drive them.
//!
//! A raised card with its edge and shadow, the tabs GIFs, Emoji and Stickers
//! along its top, and the picker of the one chosen under them: [`gif`]'s,
//! [`emoji`]'s `bare` and [`sticker`]'s. The shell opens and places it over
//! the composer's emoji button and closes it on Escape or a press outside.
//! The card's left edge drags it wider ([`gif::edge`]); the width it takes
//! and the room it has are the view-model's ([`Pickers::shown`]).

pub mod emoji;
pub mod gif;
pub mod sticker;

use iced::widget::{button, column, container, row, space, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::pickers::{Intent, Pickers, Tab};
use anomi_vm::chat::{self};
use yapfast_lang::tr;

/// The card's corners and shadow, as the emoji popup's
/// (`border-radius: 10px`, `drop-shadow-blur: 24px`, `#00000080`, down 8 px;
/// `ui/yapfast.slint:2564`-`:2567`).
const RADIUS: f32 = 10.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_ALPHA: f32 = 0.5;
const SHADOW_DROP: f32 = 8.0;
/// Above the tabs 16 px, between them and the picker 12, in from the left
/// 16, 8 between tabs (`:2569`-`:2572`).
const TOP: f32 = 16.0;
const SPACING: f32 = 12.0;
const TABS_IN: f32 = 16.0;
const TABS_GAP: f32 = 8.0;
/// A tab: 30 px tall, 12 px of room either side of its words, 6 px corners,
/// 14 px semibold words (`PickerTab`, `:248`-`:253`).
const TAB_HEIGHT: f32 = 30.0;
const TAB_PAD: f32 = 12.0;
const TAB_RADIUS: f32 = 6.0;

/// The card, `Pickers::shown` wide and `Pickers::height` tall.
pub fn view(pickers: &Pickers, motion: Motion) -> impl View<chat::Intent> + '_ {
    let shown = pickers.shown();
    let picker: Element<'_, chat::Intent> = match pickers.tab {
        Tab::Gifs => gif::view(&pickers.gif, shown, motion)
            .map(chat::Intent::from)
            .boxed(),
        Tab::Emoji => emoji::bare(&pickers.emoji, motion)
            .map(chat::Intent::from)
            .boxed(),
        Tab::Stickers => sticker::bare(&pickers.stickers, motion)
            .map(chat::Intent::from)
            .boxed(),
    };
    let body = column![tabs(pickers.tab), picker]
        .spacing(SPACING)
        .padding(Padding::ZERO.top(TOP))
        .width(Fill)
        .height(Fill);
    let card = container(body)
        .width(shown)
        .height(pickers.height())
        .style(card);
    // The edge lies over the content and the strip over the edge, as Slint
    // draws a border over what its rectangle holds.
    let outline = container(space()).width(Fill).height(Fill).style(outline);
    let strip = gif::edge(
        shown,
        |wanted| chat::Intent::from(Intent::Drag(wanted)),
        |width| chat::Intent::from(Intent::Chosen(width)),
    );
    stack![card, outline, strip]
}

/// GIFs, Emoji and Stickers; the one shown is marked.
fn tabs(shown: Tab) -> Element<'static, chat::Intent> {
    let tab = |tab: Tab| {
        let label = match tab {
            Tab::Gifs => tr!("GIFs"),
            Tab::Emoji => tr!("Emoji"),
            Tab::Stickers => tr!("Stickers"),
        };
        let selected = tab == shown;
        button(
            container(
                text(label)
                    .size(typography::size::SMALL)
                    .font(typography::SEMIBOLD),
            )
            .center_y(TAB_HEIGHT)
            .padding(Padding::ZERO.left(TAB_PAD).right(TAB_PAD)),
        )
        .padding(0)
        .on_press(chat::Intent::from(Intent::Tab(tab)))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let ground = if selected {
                Some(palette.active)
            } else if hovered {
                Some(palette.hover)
            } else {
                None
            };
            button::Style {
                background: ground.map(Background::Color),
                text_color: if selected || hovered {
                    palette.text
                } else {
                    palette.secondary
                },
                border: Border::default().rounded(TAB_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
    };
    container(
        row(Tab::ALL.into_iter().map(tab))
            .spacing(TABS_GAP)
            .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.left(TABS_IN))
    .width(Fill)
    .boxed()
}

fn card(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().raised.into()),
        border: Border::default().rounded(RADIUS),
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

fn outline(theme: &Theme) -> container::Style {
    container::Style {
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(theme.palette().edge),
        ..container::Style::default()
    }
}

#[cfg(feature = "shots")]
mod shots {
    //! The expression picker's shots of its Emoji and Stickers tabs; the
    //! GIFs tab's are [`gif::shots`](super::gif::shots).

    use iced::{Point, Rectangle, Size};

    use anomi_vm::Vm;
    use anomi_vm::fixtures::chat::pickers;
    use anomi_vm::fixtures::shell;
    use anomi_vm::menu::{Align, Spot};
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    /// `pickers` over the composer's emoji button (a 32 px icon button), its
    /// right edge on the button's and 8 px over the composer
    /// (`ui/yapfast.slint:2537`), the composer showing its GIF and sticker
    /// buttons as in `picker/gif.png`. The GIFs tab's shots open it too.
    pub(super) fn open(pickers: anomi_vm::chat::pickers::Pickers) -> Vm {
        let mut vm = shell::server_home();
        vm.chat.pickers = pickers;
        vm.chat.composer.gifs = true;
        vm.chat.composer.stickers = true;
        let button = Rectangle::new(Point::new(1087.0, 795.0), Size::new(32.0, 32.0));
        let spot = Spot::above(button).align(Align::End).gap(8.0);
        shell::open_at(vm, Layer::Expressions, spot)
    }

    pub fn shots() -> Vec<Shot> {
        let shot = |name, vm: fn() -> Vm| Shot {
            area: "picker",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        };
        vec![
            shot("tabs_emoji", || open(pickers::emoji())),
            shot("tabs_stickers", || open(pickers::stickers())),
        ]
    }
}
#[cfg(feature = "shots")]
pub use shots::shots;
