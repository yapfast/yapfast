//! The layers over the window, as `anomi_widgets::layer` stacks them: each
//! open [`Layer`] of the shell's view-model with its look, in the Slint
//! file's order, the zoom note and the files dragged over among them, and
//! the window's context menu (`vm.menu`) among the popups.
//!
//! The stack covers the whole window, so a spot an opener reported is
//! where its layer goes. The window's bar draws over every layer but the
//! popups ([`Layer::popup`]) and the context menu, which the Slint window
//! shows as `PopupWindow`s over its `WindowBar` (`ui/yapfast.slint:5610`).
//! What the Slint window shows below its bar (`y: root.title-height`)
//! starts below it.

mod content;

use iced::widget::{column, mouse_area, space};
use iced::{Fill, Length, Point, Rectangle, Size};

use anomi_theme::metrics;
use anomi_theme::motion::Pace;
use anomi_theme::prelude::*;
use anomi_vm::menu::{Align, Side, Spot};
use anomi_vm::shell::{self, Corner, Layer, Opened};
use anomi_vm::{Intent, Vm};
use anomi_widgets::layer::{self, Backdrop, Layers, Place};
use anomi_widgets::menu;
use anomi_widgets::menu::place::HEADER_CLEARANCE;

use super::notes;

/// The settings' and a full profile's backdrop (`ui/settings.slint:564`,
/// `ui/yapfast.slint:5054`).
const SHEET_DIM: f32 = 0.7;
/// A picture's or a video's (`ui/messages.slint:2641`, `:2746`).
const VIEWER_DIM: f32 = 0.9;
/// The call full screen is black (`ui/yapfast.slint:4598`).
const BLACK: f32 = 1.0;
/// A ringing call sits this far below the bar (`:4629`); the stream in a
/// corner this far from the conversation's edges (`:4567`).
const RINGING: f32 = 64.0;
const CORNER: f32 = 16.0;
/// The inbox opens this far under the bar, 8 px from the window's right
/// edge as every popover (`:5599`).
const INBOX_TOP: f32 = 4.0;

/// How a layer comes, where it goes and what's behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    /// A dialog in the middle over the dimmed window.
    Dialog,
    /// A card in the middle over a darker window: the settings, a profile;
    /// in the middle of the window below the bar where Slint's is: a
    /// server's settings, a channel's (`y: root.title-height`, `:4845`,
    /// `:4871`).
    Sheet { below_bar: bool },
    /// Over the whole window, below the bar where Slint's is: a picture,
    /// a video, the call full screen.
    Screen { dim: f32, below_bar: bool },
    /// From a spot, closing on a press outside: a popover, a card, a menu.
    Popover,
    /// A popover under a header's button, leaving 16 px under it rather
    /// than 8 (`root.height - self.y - 16px`, `:4894`-`:4967`).
    Header,
    /// From a spot, taking nothing from what's under it, at its pace.
    Floating(Pace),
}

fn look(layer: Layer) -> Look {
    match layer {
        // The stream in a corner arrives as a popover (`ui/voice.slint:698`),
        // a ringing call as a dialog (`Popup.dialog`, `:1227`).
        Layer::Pip => Look::Floating(Pace::Popover),
        Layer::IncomingCall => Look::Floating(Pace::Dialog),
        Layer::FullStage => Look::Screen {
            dim: BLACK,
            below_bar: false,
        },
        Layer::Viewer | Layer::Video => Look::Screen {
            dim: VIEWER_DIM,
            below_bar: true,
        },
        Layer::Settings | Layer::FullProfile => Look::Sheet { below_bar: false },
        Layer::ServerSettings | Layer::ChannelSettings => Look::Sheet { below_bar: true },
        Layer::Topic | Layer::Pins | Layer::Threads | Layer::AddPeople => Look::Header,
        Layer::RoleCard
        | Layer::Card
        | Layer::Profile
        | Layer::MessageCard
        | Layer::Inbox
        | Layer::Devices
        | Layer::Soundboard
        | Layer::Share
        | Layer::Account
        | Layer::Expressions
        | Layer::EmojiPicker
        | Layer::Connection
        | Layer::ParticipantMenu
        | Layer::BotSelect
        | Layer::MessageMenu => Look::Popover,
        Layer::AddFriend
        | Layer::Invite
        | Layer::Join
        | Layer::Event
        | Layer::Gift
        | Layer::OAuth
        | Layer::CreateChannel
        | Layer::ProfileEditor
        | Layer::Leave
        | Layer::Person
        | Layer::Status
        | Layer::Poll
        | Layer::NewDm
        | Layer::Reactions
        | Layer::Forward
        | Layer::LeavingLink
        | Layer::Switcher
        | Layer::BotModal
        | Layer::Mfa
        | Layer::SwitchCall => Look::Dialog,
    }
}

/// Where the window's parts are, for what opens by itself: its size, the
/// bar's height over it, the conversation's left edge and the width of the
/// thread docked beside it (none signed out or without one).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub window: Size,
    pub top: f32,
    pub left: f32,
    pub aside: f32,
}

/// The window's base with every layer that shows over it, and the window's
/// `bar` over all but the popups and the context menu.
///
/// The bar lies between two stacks: the popups' over the rest's, which is
/// its base. `anomi_widgets::layer` has no place for a strip across the
/// window that leaves the rest to what's under it (a layer filling the
/// window keeps the pointer from all under it). Stacked so, the popups'
/// stack hands the keys, the pointer and the focus on to the rest only
/// while no popup is open, so Escape still closes the top layer only and
/// focus stays in it, as in one stack.
pub fn stack<'a>(
    vm: &'a Vm,
    motion: Motion,
    frame: Frame,
    base: impl View<Intent> + 'a,
    bar: Element<'a, Intent>,
) -> Layers<'a, Intent> {
    let mut rest = layer::layers(base, motion);
    let mut notes_placed = false;
    for opened in vm.shell.layers.iter().filter(|o| !o.layer.popup()) {
        if !notes_placed && opened.layer > Layer::LeavingLink {
            rest = notes(rest, vm, motion, frame);
            notes_placed = true;
        }
        rest = rest.push(one(vm, motion, frame, opened));
    }
    if !notes_placed {
        rest = notes(rest, vm, motion, frame);
    }
    let under = iced::widget::stack![rest, bar].width(Fill).height(Fill);
    let mut popups = layer::layers(under, motion);
    let mut menu_placed = false;
    for opened in vm.shell.layers.iter().filter(|o| o.layer.popup()) {
        if !menu_placed && opened.layer >= Layer::Account {
            popups = popups.push_maybe(menu::layer(&vm.menu, motion));
            menu_placed = true;
        }
        popups = popups.push(one(vm, motion, frame, opened));
    }
    if !menu_placed {
        popups = popups.push_maybe(menu::layer(&vm.menu, motion));
    }
    popups
}

fn notes<'a>(
    stack: Layers<'a, Intent>,
    vm: &'a Vm,
    motion: Motion,
    frame: Frame,
) -> Layers<'a, Intent> {
    stack
        .push_maybe(notes::zoom(vm, frame))
        .push_maybe(notes::files(vm, motion, frame))
}

/// One open layer, with its look.
fn one<'a>(vm: &'a Vm, motion: Motion, frame: Frame, opened: &Opened) -> layer::Layer<'a, Intent> {
    let content = content::content(vm, motion, opened.layer);
    let look = look(opened.layer);
    let close = Intent::Shell(shell::Intent::Close(opened.layer));
    let content: Element<'a, Intent> = match look {
        Look::Screen {
            below_bar: true, ..
        } => column![space().height(frame.top), content].boxed(),
        Look::Sheet { below_bar: true } => sheet_below_bar(content, frame.top, close.clone()),
        _ => content,
    };
    let card = layer::layer(opened.layer, content)
        .open(opened.open)
        .on_close(close)
        .on_gone(Intent::Shell(shell::Intent::Gone(opened.layer)));
    match look {
        Look::Dialog => card.dialog(),
        Look::Sheet { .. } => card.pace(Pace::Dialog).backdrop(Backdrop::Dim(SHEET_DIM)),
        Look::Screen { dim, .. } => card
            .pace(Pace::Dialog)
            .place(Place::Fill)
            .backdrop(Backdrop::Dim(dim)),
        Look::Popover => card.place(spot(vm, frame, opened).map_or(Place::Centre, Place::At)),
        Look::Header => card
            .place(spot(vm, frame, opened).map_or(Place::Centre, Place::At))
            .bottom_clearance(HEADER_CLEARANCE),
        Look::Floating(pace) => card
            .pace(pace)
            .place(spot(vm, frame, opened).map_or(Place::Centre, Place::At))
            .backdrop(Backdrop::None),
    }
}

/// A sheet in the middle of the window below the bar: the bar's height
/// over its card moves it down by half that and takes that from its room.
/// That strip is the backdrop's in Slint, so a press on it closes the
/// sheet; it is as wide as the card, unless the card fills the window.
fn sheet_below_bar<'a>(
    content: Element<'a, Intent>,
    top: f32,
    close: Intent,
) -> Element<'a, Intent> {
    let fills = iced::advanced::widget::Widget::size(&content)
        .width
        .is_fill();
    let strip = mouse_area(space().width(Fill).height(top)).on_press(close);
    let sheet = column![strip, content];
    if fills {
        sheet.boxed()
    } else {
        sheet.width(Length::Shrink).boxed()
    }
}

/// Where a layer opens from: the spot it was opened at, or its own.
fn spot(vm: &Vm, frame: Frame, opened: &Opened) -> Option<Spot> {
    let own = |x: f32, y: f32, side, align| Spot {
        anchor: Rectangle::new(Point::new(x, y), Size::ZERO),
        side,
        align,
        gap: 0.0,
    };
    let window = frame.window;
    opened.at.or_else(|| match opened.layer {
        Layer::IncomingCall => Some(own(
            window.width / 2.0,
            frame.top + RINGING,
            Side::Below,
            Align::Centre,
        )),
        Layer::Inbox => Some(own(
            window.width - menu::place::MARGIN,
            frame.top + INBOX_TOP,
            Side::Below,
            Align::End,
        )),
        Layer::Pip => Some(corner(vm.shell.pip, frame)),
        _ => None,
    })
}

/// The corner a floating stream keeps to, inside the conversation below
/// its header.
fn corner(corner: Corner, frame: Frame) -> Spot {
    let x = if corner.left {
        frame.left + CORNER
    } else {
        frame.window.width - CORNER
    };
    let y = if corner.bottom {
        frame.window.height - CORNER
    } else {
        frame.top + metrics::HEADER_HEIGHT + CORNER
    };
    Spot {
        anchor: Rectangle::new(Point::new(x, y), Size::ZERO),
        side: if corner.bottom {
            Side::Above
        } else {
            Side::Below
        },
        align: if corner.left {
            Align::Start
        } else {
            Align::End
        },
        gap: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ringing_call_arrives_as_a_dialog_and_the_stream_as_a_popover() {
        assert_eq!(look(Layer::IncomingCall), Look::Floating(Pace::Dialog));
        assert_eq!(look(Layer::Pip), Look::Floating(Pace::Popover));
    }

    #[test]
    fn the_cards_open_where_they_were_opened() {
        assert_eq!(look(Layer::RoleCard), Look::Popover);
        assert_eq!(look(Layer::Card), Look::Popover);
    }
}
