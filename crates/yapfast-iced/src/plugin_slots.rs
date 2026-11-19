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
    widget::{Space, column, container, row, text},
};
use yapfast_core::plugins::slots::{Slot, Slots};

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
        let plugin = &slots.names()[drawn.plugin].id;
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::Banner, subject),
        );
        shown = shown.push(notice_card(theme, plugin, Slot::Banner, body));
        any = true;
    }
    any.then(|| shown.into())
}

/// The dialogs plugins ask, over everything else, one each.
pub fn dialog<'a>(
    theme: &'a Theme,
    slots: &'a Slots,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(12).width(Length::Fixed(440.0));
    let mut any = false;
    for drawn in slots.drawn(Slot::Dialog) {
        let plugin = &slots.names()[drawn.plugin].id;
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::Dialog, subject),
        );
        shown = shown.push(notice_card(theme, plugin, Slot::Dialog, body));
        any = true;
    }
    if !any {
        return None;
    }
    let ground = theme.panel;
    let radius = crate::sheet::RADIUS;
    Some(
        container(
            container(shown)
                .padding(20)
                .style(move |_| style::rounded(ground, radius)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(
                iced::Color {
                    a: 0.6,
                    ..iced::Color::BLACK
                }
                .into(),
            ),
            ..container::Style::default()
        })
        .into(),
    )
}

/// The toasts in the window's corner, each gone when it says so.
pub fn toasts<'a>(theme: &'a Theme, slots: &'a Slots) -> Option<Element<'a, Message>> {
    let mut shown = column![].spacing(8).width(Length::Fixed(320.0));
    let mut any = false;
    for drawn in slots.drawn(Slot::Toast) {
        let plugin = &slots.names()[drawn.plugin].id;
        let body = plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::Toast, ""),
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
        let plugin = &slots.names()[drawn.plugin].id;
        buttons = buttons.push(plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::Composer, subject),
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
        let plugin = &slots.names()[drawn.plugin].id;
        buttons = buttons.push(plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::HoverBar, item),
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
        let plugin = &slots.names()[drawn.plugin].id;
        shown = shown.push(plugin_tree::draw(
            theme,
            &drawn.tree,
            &NO_DRAFTS,
            acted(plugin, Slot::ProfileBadges, person),
        ));
        any = true;
    }
    any.then(|| shown.into())
}

/// A notice, its plugin named so it can't pass for Yapfast's own, and the
/// button that closes it.
fn notice_card<'a>(
    theme: &'a Theme,
    plugin: &'a str,
    slot: Slot,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let ground = theme.raised;
    let name = theme.muted;
    let close: Element<'a, Message> = controls::action(
        theme,
        "Dismiss",
        ActionKind::Ghost,
        Some(Message::Plugin(yapfast_core::plugins::Input::Dismissed {
            plugin: plugin.to_owned(),
            slot,
        })),
    )
    .small()
    .into();
    container(
        column![
            row![
                text(plugin.to_owned()).size(11).font(BOLD).color(name),
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
