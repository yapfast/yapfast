//! The profile editor (`ProfileEditorDialog`, `ui/server.slint:1585`): the
//! styles panel on the left, the profile's card in the middle with its
//! texts and pictures editable in place, P15's tabs beside it where there is
//! room, and Discord's bar under them once something changed. Task P14
//! (docs/iced/tasks/people.md).
//!
//! The shell draws the backdrop, Escape and the way in; the dialog draws its
//! own surface and sizes itself from the room it is given, as Slint's card
//! does: the main profile 958 px wide, 1199 with its panel, a server's 600.
//! While a theme colour's handle is open, P15's picker shows beside the
//! styles panel over a catcher that closes it ([`picker`]), and the server
//! tag's chooser under its chip ([`tag_menu`]).

pub mod colour;
mod customize;
pub mod name_style;
pub mod preview;
mod slots;
pub mod tag_menu;

use iced::widget::{
    Space, column, container, mouse_area, opaque, pin, responsive, row, space, stack,
};
use iced::{Background, Border, Fill, Length, Shadow, Size, Vector};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::people;
use anomi_vm::people::editor::{Editor, Intent as Edit};
use anomi_widgets::button::icon_button;
use anomi_widgets::menu::place::MARGIN;
use anomi_widgets::tip::Side;

pub use slots::card::Layout;
/// The fields' ids, for the app's `field::focus` as one opens.
pub use slots::fields::{BIO, COLOUR, NAME, PRONOUNS};
pub use tag_menu::SEARCH as TAG_SEARCH;

/// The picker's left edge from the card's: 8 px right of the 241 px panel
/// (`x: card.x + 249px`, `ui/server.slint:2323`).
const PICKER_X: f32 = 249.0;

/// The room the tabs need beside the profile: 340 for the card, 400 for them,
/// their words whole in Russian too.
const TABS_FROM: f32 = 760.0;

/// The profile editor, with the parts other tasks draw. The calls to them are
/// F01's wiring: keep them as the view grows.
pub fn view(editor: &Editor, motion: Motion) -> impl View<people::Intent> + '_ {
    responsive(move |room: Size| dialog(editor, room, motion))
}

/// The dialog's card in `room`.
fn dialog<'a>(editor: &'a Editor, room: Size, motion: Motion) -> Element<'a, people::Intent> {
    let main = editor.main;
    let side_out = main && editor.side_open;
    let folded = if main {
        (room.width - 80.0).min(958.0)
    } else {
        600.0_f32.min(room.width - 48.0)
    };
    let wide = if main {
        (room.width - 48.0).min(1199.0)
    } else {
        folded
    };
    let width = if side_out { wide } else { folded };
    let bar = if editor.changed || editor.saving {
        slots::bar::HEIGHT
    } else {
        0.0
    };
    let height = if main {
        (customize::HEIGHT.max(Layout::of(editor).head() + 16.0) + bar).min(room.height - 48.0)
    } else {
        620.0_f32
            .max(Layout::of(editor).head() + bar)
            .min(room.height - 48.0)
    };
    // Its top stays where it starts, so what's under the pointer stays put
    // as the bar or the banner colour opens; it rises only to keep its end
    // 24 px inside the window, which is `MARGIN` round the room. The main
    // profile's starts where its styles panel would be centred.
    let start = if main { customize::HEIGHT } else { 600.0 }.min(room.height - 32.0);
    let top = ((room.height - start) / 2.0)
        .min(room.height - 16.0 - height)
        .max(16.0);
    let radius = if main { 16.0 } else { 12.0 };
    let tabs_area = width - if side_out { 242.0 } else { 0.0 };
    let tabs = main && tabs_area >= TABS_FROM;
    // The card is as wide as the full profile's beside its tabs, its margin
    // added; it takes the whole area without them.
    let inner = if main { 8.0 } else { 0.0 };
    let profile_width = if tabs {
        Length::Fixed(340.0_f32.max(440.0_f32.min(tabs_area * 0.45)) + inner)
    } else {
        Length::Fill
    };
    let card = slots::card::view(editor, motion)
        .map(people::Intent::from)
        .boxed();
    let mut middle = row![
        container(card)
            .padding(iced::Padding {
                top: inner,
                bottom: inner,
                left: inner,
                right: if tabs { 0.0 } else { inner },
            })
            .width(profile_width)
            .height(Fill)
            .boxed()
    ]
    .height(Fill);
    if tabs {
        middle = middle.push(
            container(
                preview::view(&editor.preview, motion)
                    .map(people::Intent::from)
                    .boxed(),
            )
            .width(Fill)
            .height(Fill)
            .boxed(),
        );
    }
    let mut body = column![middle.height(Fill).boxed()];
    if let Some(unsaved) = slots::bar::view(editor, if main { 29.0 } else { 24.0 }, motion) {
        body = body.push(unsaved.map(people::Intent::from).boxed());
    }
    let mut line = row![].height(Fill);
    if main && side_out {
        line = line.push(
            customize::view(editor, motion)
                .map(people::Intent::from)
                .boxed(),
        );
        line = line.push(
            container(Space::new().width(1.0).height(Fill))
                .style(|theme: &Theme| container::Style {
                    background: Some(Background::Color(theme.palette().line)),
                    ..container::Style::default()
                })
                .boxed(),
        );
    }
    line = line.push(container(body).width(Fill).height(Fill).boxed());
    let surface = container(line)
        .width(width)
        .height(height)
        .clip(true)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sidebar)),
            border: Border::default()
                .rounded(radius)
                .width(1.0)
                .color(theme.palette().edge),
            shadow: Shadow {
                color: anomi_theme::rgba(0x00000066),
                offset: Vector::new(0.0, 12.0),
                blur_radius: 32.0,
            },
            ..container::Style::default()
        });
    // A press on the dialog that none of its parts took ends the editing,
    // and one beside it leaves, as the card's and the backdrop's touch
    // areas do (`ui/server.slint:1702`, `:1737`): the layer sees the whole
    // room as this dialog's.
    let surface = mouse_area(surface).on_press(people::Intent::from(Edit::Edit(None)));
    let placed = container(surface)
        .padding(iced::Padding::ZERO.top(top))
        .center_x(Fill)
        .height(Fill);
    let mut layers = stack![placed.boxed()].width(Fill).height(Fill);
    if main && editor.handle.is_some() {
        layers = layers.push(picker(editor, (room.width - width) / 2.0, room, motion));
    }
    if let Some(menu) = &editor.tag_menu {
        layers = layers.push(
            tag_menu::view(editor, menu, room, motion)
                .map(people::Intent::from)
                .boxed(),
        );
    }
    if main {
        layers = layers.push(
            close(width, top, tabs, room, motion)
                .map(people::Intent::from)
                .boxed(),
        );
        // The tab hides its far end under the card, as Slint's did.
        if !side_out {
            layers = layers.push_under(
                show_styles(width, top, room, motion)
                    .map(people::Intent::from)
                    .boxed(),
            );
        }
    }
    layers
        .push_under(
            mouse_area(space().width(Fill).height(Fill))
                .on_press(people::Intent::from(Edit::Close))
                .boxed(),
        )
        .boxed()
}

/// A theme colour's picker, `left` being the card's: its middle on the
/// clicked tile's, kept 8 px inside the window, over a catcher that closes
/// it (`ui/server.slint:2321`). The layer lays the dialog out `MARGIN` in
/// from the window, where `handle_y` is measured from.
fn picker<'a>(
    editor: &'a Editor,
    left: f32,
    room: Size,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let middle = editor.handle_y - MARGIN;
    let y = (middle - colour::HEIGHT / 2.0)
        .min(room.height - colour::HEIGHT - 8.0)
        .max(8.0);
    let card = colour::view(&editor.preview.colour, motion).map(people::Intent::from);
    stack![
        mouse_area(space().width(Fill).height(Fill))
            .on_press(people::Intent::from(Edit::CloseHandle)),
        pin(opaque(card)).x(left + PICKER_X).y(y),
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// The main profile closes from the end of its tabs' row, or, with no tabs,
/// from the banner's corner as a server profile does.
fn close<'a>(width: f32, top: f32, tabs: bool, room: Size, motion: Motion) -> Element<'a, Edit> {
    let left = (room.width - width) / 2.0;
    let side = if tabs { 32.0 } else { 36.0 };
    let button = icon_button(Icon::X, tr_close(), motion)
        .size(side)
        .glyph(18.0)
        .radius(side / 2.0)
        .on_press(Edit::Close);
    let button = if tabs {
        button.view().boxed()
    } else {
        button
            .fill(|_| anomi_theme::rgba(0x000000b3))
            .tint(|_| iced::Color::WHITE)
            .view()
            .boxed()
    };
    let (x, y) = if tabs {
        (left + width - 24.0 - side + 7.0, top + 7.0)
    } else {
        (left + width - 16.0 - side, top + 16.0)
    };
    iced::widget::pin(button).x(x).y(y).boxed()
}

fn tr_close() -> &'static str {
    yapfast_lang::tr!("Close")
}

/// Show Styles: a tab on the card's left edge, once the panel has gone.
fn show_styles<'a>(width: f32, top: f32, room: Size, motion: Motion) -> Element<'a, Edit> {
    let left = (room.width - width) / 2.0;
    let tab = icon_button(Icon::Brush, yapfast_lang::tr!("Show Styles"), motion)
        .size(36.0)
        .glyph(18.0)
        .radius(10.0)
        .tip_side(Side::Right)
        .fill(|_| iced::Color::BLACK)
        .tint(|_| iced::Color::WHITE)
        .on_press(Edit::Side(true))
        .view();
    iced::widget::pin(tab)
        .x(left - left.min(30.0))
        .y(top + 40.0)
        .boxed()
}

/// Its shots for `anomi-shots`: the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    vec![crate::shots::Shot {
        area: "profile",
        name: "editor",
        size: crate::shots::GOLDEN,
        theme: "bonfire",
        vm: slots::shot::editor,
    }]
}
