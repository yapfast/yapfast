//! The places a plugin puts something in the window: a notice over the
//! conversation, a toast in a corner, a dialog over everything, buttons beside
//! the composer, buttons on a message's hover bar and badges on a person.
//!
//! What is drawn comes from `yapfast_core::plugins::slots` (`AppState::
//! plugin_slots`), and what the person does with it goes back as
//! `Action::PluginInput`, which the application hands to the runtime that
//! owns the plugin.

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, row, scrollable, text, tooltip},
};
use yapfast_core::plugins::slots::{Slot, Slots};
use yapfast_core::plugins::ui::PluginName;

use crate::{
    app::Message,
    controls::{self, Kind as ActionKind},
    fonts::BOLD,
    plugin_tree, style,
    theme::Theme,
};

/// A slot's cells hold no text before the person types it.
static NO_DRAFTS: std::sync::LazyLock<std::collections::BTreeMap<String, String>> =
    std::sync::LazyLock::new(std::collections::BTreeMap::new);

/// What the person did with a plugin's drawing, as the message that carries
/// it to the runtime.
fn acted(plugin: &str, slot: Slot, subject: &str) -> impl Fn(plugin_tree::Act) -> Message {
    let plugin = plugin.to_owned();
    let subject = subject.to_owned();
    move |act| {
        let (id, value) = match act {
            plugin_tree::Act::Set { id, value } | plugin_tree::Act::Edit { id, value } => {
                (id, value)
            }
        };
        Message::Plugin(yapfast_core::plugins::Input::Widget {
            plugin: plugin.clone(),
            slot,
            id,
            value,
            subject: subject.clone(),
        })
    }
}

/// A notice a plugin puts over the conversation, with the button that closes
/// it.
pub fn banner<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(8).width(Length::Fill);
    let mut any = false;
    for drawn in slots.drawn(Slot::Banner) {
        let plugin = &slots.names()[drawn.plugin];
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(&plugin.id, Slot::Banner, subject),
        );
        shown = shown.push(notice_card(theme, plugin, Slot::Banner, body));
        any = true;
    }
    any.then(|| shown.into())
}

/// The dialogs plugins ask, one card each, each naming its plugin, in the
/// kit's dialog frame. They scroll past a height, so no tree can push
/// Dismiss off the window, and a click outside or Escape dismisses the first.
pub fn dialog<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(12).width(Length::Fill);
    let mut first = None;
    for drawn in slots.drawn(Slot::Dialog) {
        let plugin = &slots.names()[drawn.plugin];
        first.get_or_insert(plugin.id.as_str());
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(&plugin.id, Slot::Dialog, subject),
        );
        shown = shown.push(notice_card(theme, plugin, Slot::Dialog, body));
    }
    let first = first?;
    let body = container(scrollable(shown).height(Length::Shrink))
        .padding(16)
        .max_height(DIALOG_HEIGHT);
    Some(crate::dialog::dialog(
        theme,
        body.into(),
        crate::dialog::CONFIRM_WIDTH,
        dismissed(first, Slot::Dialog),
    ))
}

/// The tallest a plugin's dialog draws before it scrolls.
const DIALOG_HEIGHT: f32 = 520.0;

/// What closing a plugin's notice or dialog tells it.
pub fn dismissed(plugin: &str, slot: Slot) -> Message {
    Message::Plugin(yapfast_core::plugins::Input::Dismissed {
        plugin: plugin.to_owned(),
        slot,
    })
}

/// The toasts in the window's corner, each gone when it says so.
pub fn toasts<'a>(theme: &'a Theme, slots: &'a Slots) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(8).width(Length::Fixed(320.0));
    let mut any = false;
    for drawn in slots.drawn(Slot::Toast) {
        let plugin = &slots.names()[drawn.plugin];
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(&plugin.id, Slot::Toast, ""),
        );
        shown = shown.push(notice_card(theme, plugin, Slot::Toast, body));
        any = true;
    }
    any.then(|| shown.into())
}

/// The buttons a plugin adds beside the composer's, for the open
/// conversation.
pub fn composer_buttons<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let mut buttons = row![].spacing(4).align_y(Alignment::Center);
    let mut any = false;
    for drawn in slots.drawn(Slot::Composer) {
        let plugin = &slots.names()[drawn.plugin];
        buttons = buttons.push(named(
            theme,
            plugin,
            plugin_tree::draw(
                theme,
                &drawn.tree,
                &NO_DRAFTS,
                acted(&plugin.id, Slot::Composer, subject),
            ),
        ));
        any = true;
    }
    any.then(|| buttons.into())
}

/// The buttons a plugin adds to a message's hover bar, before More, for the
/// message `item` of the open conversation.
pub fn hover_bar<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    item: &'a str,
) -> Option<Element<'a, Message>> {
    let mut buttons = row![].spacing(2).align_y(Alignment::Center);
    let mut any = false;
    for drawn in slots.drawn(Slot::HoverBar) {
        let plugin = &slots.names()[drawn.plugin];
        buttons = buttons.push(named(
            theme,
            plugin,
            plugin_tree::draw(
                theme,
                &drawn.tree,
                &NO_DRAFTS,
                acted(&plugin.id, Slot::HoverBar, item),
            ),
        ));
        any = true;
    }
    any.then(|| buttons.into())
}

/// What plugins add to a person's card, drawn under it.
pub fn badges<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    person: &'a str,
) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(4).width(Length::Fill);
    let mut any = false;
    for drawn in slots.drawn(Slot::ProfileBadges) {
        let plugin = &slots.names()[drawn.plugin];
        // Each plugin's badges sit under its name, so a badge that says
        // "Staff" reads as that plugin's word and not Yapfast's or Discord's.
        shown = shown.push(
            column![
                text(format!("From {}", plugin.name))
                    .size(11)
                    .font(BOLD)
                    .color(theme.muted),
                plugin_tree::draw(
                    theme,
                    &drawn.tree,
                    &NO_DRAFTS,
                    acted(&plugin.id, Slot::ProfileBadges, person),
                ),
            ]
            .spacing(2),
        );
        any = true;
    }
    any.then(|| shown.into())
}

/// A plugin's buttons, with a tooltip that names the plugin, so a button that
/// says "Report" can't pass for Yapfast's own.
fn named<'a>(
    theme: &'a Theme,
    plugin: &PluginName,
    drawn: Element<'a, Message>,
) -> Element<'a, Message> {
    tooltip(
        drawn,
        controls::tip(theme, from(plugin)),
        tooltip::Position::Top,
    )
    .into()
}

/// How a plugin's drawing is labelled: its name and id.
pub fn from(plugin: &PluginName) -> String {
    format!("{} \u{b7} plugin {}", plugin.name, plugin.id)
}

/// A notice, its plugin named so it can't pass for Yapfast's own, and the
/// button that closes it.
fn notice_card<'a>(
    theme: &'a Theme,
    plugin: &PluginName,
    slot: Slot,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let ground = theme.raised;
    let name = theme.muted;
    let close: Element<'a, Message> = controls::action(
        theme,
        "Dismiss",
        ActionKind::Ghost,
        Some(dismissed(&plugin.id, slot)),
    )
    .small()
    .into();
    container(
        column![
            row![
                text(from(plugin)).size(11).font(BOLD).color(name),
                Space::new().width(Length::Fill),
                close
            ]
            .align_y(Alignment::Center),
            body,
        ]
        .spacing(6),
    )
    .padding(10)
    .width(Length::Fill)
    .style(move |_| style::rounded(ground, 8))
    .into()
}
