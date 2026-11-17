//! The bar's layout: three layers over the rail's ground, as `WindowBar`
//! stacks them (`ui/yapfast/window-bar.slint:74`): the place in the middle,
//! back and forward on the left, the inbox, help and the caption buttons on
//! the right, all under [`Drag`], which moves the window.

use iced::widget::svg;
use iced::widget::{container, row, stack, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chrome::{Chrome, Intent};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::HEIGHT;
use super::button::{Kind, button};
use super::drag::drag;
use super::ink::ink;

/// The middle's glyph and the gap to the place (`window-bar.slint:110`, `:109`).
const GLYPH: f32 = 16.0;
const GAP: f32 = 8.0;
/// What the middle leaves to the sides: half of 460, since Slint's group is
/// at most the bar's width minus 460 (`:108`); glyph, gap, label and the
/// tail, so a cut label is at most the bar's width minus 484.
const SIDES: f32 = 230.0;
/// The left group's inset and spacing (`:119`), the right group's inset
/// (`:127`).
const LEFT_INSET: f32 = 8.0;
const SPACING: f32 = 4.0;
const RIGHT_INSET: f32 = 12.0;

pub fn bar(chrome: &Chrome, motion: Motion) -> Element<'_, Intent> {
    let layers = stack![
        middle(chrome, motion),
        left(chrome, motion),
        right(chrome, motion)
    ]
    .width(Fill)
    .height(Fill);
    let bar = container(layers)
        .width(Fill)
        .height(HEIGHT)
        .style(catalog::container::rail);
    drag(bar, Intent::Move, Intent::ToggleMaximize).boxed()
}

/// The place, with its glyph, centred on the bar; the ink brightens (over
/// `QUICK`) while the window has the focus.
fn middle(chrome: &Chrome, motion: Motion) -> Element<'_, Intent> {
    let glyph = icon(Icon::MessageCircle, GLYPH).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().secondary),
    });
    let place = text(chrome.place.as_str())
        .size(typography::size::CAPTION)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::inherit);
    let place = ink(place.boxed(), chrome.focused, motion).boxed();
    container(row![glyph, place].spacing(GAP).align_y(Alignment::Center))
        .padding(Padding::from([0.0, SIDES]))
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .boxed()
}

/// Back and forward, which ask for the history's moves while it has them.
fn left(chrome: &Chrome, motion: Motion) -> Element<'_, Intent> {
    let back = button(Icon::ArrowLeft, Kind::Bar { active: false }, motion)
        .enabled(chrome.can_go_back)
        .on_press(Intent::Back);
    let forward = button(Icon::ArrowRight, Kind::Bar { active: false }, motion)
        .enabled(chrome.can_go_forward)
        .on_press(Intent::Forward);
    container(
        row![
            tip(
                back,
                tip_text(chrome.can_go_back, tr!("Back")),
                Side::Below,
                motion
            ),
            tip(
                forward,
                tip_text(
                    chrome.can_go_forward,
                    yapfast_lang::text_in("WindowBar", "Forward")
                ),
                Side::Below,
                motion,
            ),
        ]
        .spacing(SPACING),
    )
    .padding(Padding::default().left(LEFT_INSET))
    .height(Fill)
    .center_y(Fill)
    .boxed()
}

/// A disabled button shows no tip, as in Slint, where its touch area is off.
fn tip_text(enabled: bool, text: &'static str) -> &'static str {
    if enabled { text } else { "" }
}

/// The inbox and help for the account's screens, then the three caption
/// buttons at the window's edge.
fn right(chrome: &Chrome, motion: Motion) -> Element<'_, Intent> {
    let caption = |icon, kind, intent| button(icon, kind, motion).on_press(intent);
    let maximize = if chrome.maximized {
        Icon::Restore
    } else {
        Icon::Square
    };
    let mut buttons: iced::widget::Row<Element<'_, Intent>> = row![].align_y(Alignment::Center);
    if chrome.workspace {
        let inbox = button(
            Icon::Inbox,
            Kind::Bar {
                active: chrome.inbox_open,
            },
            motion,
        )
        .on_press(Intent::ToggleInbox);
        let help = button(Icon::HelpCircle, Kind::Bar { active: false }, motion)
            .on_press(Intent::OpenHelp);
        buttons = buttons.push(
            container(
                row![
                    tip(inbox, tr!("Inbox"), Side::Below, motion),
                    tip(help, tr!("Help"), Side::Below, motion),
                ]
                .spacing(SPACING),
            )
            .padding(Padding::default().right(RIGHT_INSET))
            .boxed(),
        );
    }
    buttons = buttons
        .push(caption(Icon::Minus, Kind::Caption, Intent::Minimize).boxed())
        .push(caption(maximize, Kind::Caption, Intent::ToggleMaximize).boxed())
        .push(caption(Icon::X, Kind::Close, Intent::Close).boxed());
    container(buttons)
        .height(Fill)
        .align_right(Fill)
        .center_y(Fill)
        .boxed()
}
