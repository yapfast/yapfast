//! A card's buttons and its menu: `CardButtonView` (`ui/messages.slint:2225`)
//! and the row of `BotSelectBox` (C20's box).

use iced::Length;
use iced::widget::container;

use anomi_theme::prelude::*;
use anomi_vm::chat::components::{Button, Components, Intent, Select, Style};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

/// The menu's widest: `max-width: 400px`.
const SELECT_WIDTH: f32 = 400.0;
/// Its dropdown's distance under it, as under a form's menu (C20's).
const DROP_GAP: f32 = 4.0;

/// An app's button: invokes its component, opens a link, or views its offer.
/// A control waits for its bot while the message's `busy`.
pub fn card_button<'a>(
    components: &'a Components,
    button: &'a Button,
    motion: Motion,
) -> Element<'a, Intent> {
    let key = components.key;
    let premium = button.style == Style::Premium;
    let waiting = !button.custom_id.is_empty() && components.busy;
    let offer = !premium || !components.app_id.is_empty();
    let kind = match button.style {
        Style::Primary => Kind::Primary,
        Style::Success => Kind::Success,
        Style::Danger => Kind::Danger,
        Style::Secondary | Style::Link | Style::Premium => Kind::Secondary,
    };
    let text: &str = if premium {
        tr!("Open in Discord")
    } else {
        &button.label
    };
    let mut face = action_button(text, kind, motion).small();
    if premium {
        face = face.leading(Icon::Shop);
    }
    // A link's arrow follows its label, in lines.
    if button.style == Style::Link {
        face = face.trailing(Icon::ArrowUpRight);
    }
    let intent = match button.address(&components.app_id) {
        None => Intent::Use(key, button.custom_id.clone()),
        Some(url) => Intent::Open(key, url),
    };
    face.on_press(intent)
        .enabled(button.enabled && !waiting && offer)
        .boxed()
}

/// A menu shown by what it picked or its placeholder, at most 400 wide: a
/// click opens its dropdown under it (the shell's, by `Intent::OpenSelect`).
/// Its box is C20's.
pub fn select_box<'a>(components: &'a Components, select: &'a Select) -> Element<'a, Intent> {
    let key = components.key;
    let id = select.id.as_str();
    let field = crate::bot_forms::select_box(
        &select.text,
        select.picked,
        select.enabled,
        components.busy,
        move |bounds| Intent::OpenSelect(key, id.to_owned(), Spot::below(bounds).gap(DROP_GAP)),
    );
    container(field)
        .width(Length::Fill.max(SELECT_WIDTH))
        .boxed()
}
