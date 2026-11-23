//! An app's select menu open: its options in a dropdown under the box that
//! opened it (over it when there's no room below), a search over them for
//! members, roles and channels, a tick by each one picked (a box of ticks
//! for a menu of several). A menu of one option takes the one clicked; one
//! of several takes what changed when it closes, by a press outside or
//! Escape. What it lists and sends is the core's (`OpenSelect`,
//! `ToggleSelect`, `SearchSelect`, `CloseSelect`).

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Element, Length, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::application::Action;

use crate::{
    app::Message,
    controls::field,
    ctx::Cx,
    fonts::MEDIUM,
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::scrollbar,
};
use yapfast_core::i18n::tr;

/// The search's field, a row's least height and the list's most.
const SEARCH: &str = "select-search";
const ROW: f32 = 40.0;
const MOST: f32 = 360.0;

#[derive(Clone, Debug)]
pub enum Event {
    Open(String, String),
    Toggle(usize),
    Search(String),
    Close,
}

/// What's done in a select menu: the core holds it all.
pub fn update(event: Event, cx: &mut Cx<'_>) -> Task<Message> {
    match event {
        Event::Open(item, custom_id) => {
            cx.dispatch(Action::OpenSelect { item, custom_id });
            let searchable = cx
                .state()
                .select_menu
                .as_ref()
                .is_some_and(|select| matches!(select.kind, 5..=8));
            if searchable {
                return iced::widget::operation::focus(SEARCH);
            }
        }
        Event::Toggle(index) => cx.dispatch(Action::ToggleSelect(index)),
        Event::Search(query) => cx.dispatch(Action::SearchSelect(query)),
        Event::Close => cx.dispatch(Action::CloseSelect),
    }
    Task::none()
}

/// The open select menu's options, under the box that opened it.
pub fn dropdown<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let state = ctx.state;
    let Some(select) = &state.select_menu else {
        return Space::new().into();
    };
    let several = select.max > 1;
    let query = select.query.to_lowercase();
    let mut list = column![].spacing(2);
    let mut shown = 0;
    for (index, choice) in select.choices.iter().enumerate() {
        if !query.is_empty()
            && !choice.label.to_lowercase().contains(&query)
            && !choice.description.to_lowercase().contains(&query)
        {
            continue;
        }
        shown += 1;
        let mut words = column![
            text(choice.label.clone())
                .size(15)
                .font(MEDIUM)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ];
        if !choice.description.is_empty() {
            words = words.push(
                text(choice.description.clone())
                    .size(13)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::None),
            );
        }
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if !choice.emoji.is_empty() {
            line = line.push(choice_emoji(ctx, &select.item, &choice.emoji));
        }
        line = line.push(container(words).width(Length::Fill).clip(true));
        if several {
            let (accent, edge) = (theme.accent, theme.secondary);
            let on = choice.selected;
            line = line.push(
                container(if on {
                    Element::from(ctx.look.glyph(Icon::Check, 14, theme.on_accent))
                } else {
                    Space::new().into()
                })
                .center(20)
                .style(move |_| container::Style {
                    background: on.then(|| accent.into()),
                    border: Border {
                        color: if on { accent } else { edge },
                        width: 2.0,
                        radius: 5.0.into(),
                    },
                    ..Default::default()
                }),
            );
        } else if choice.selected {
            line = line.push(ctx.look.glyph(Icon::Check, 18, theme.accent));
        }
        let (hover, sunken) = (theme.hover, theme.sunken);
        let picked = choice.selected && !several;
        list = list.push(
            button(line)
                .padding([6, 8])
                .width(Length::Fill)
                .height(Length::Shrink)
                .on_press(Message::BotSelect(Event::Toggle(index)))
                .style(crate::controls::lit_style(
                    picked.then_some(sunken),
                    hover,
                    4.0,
                )),
        );
    }
    let mut panel = column![].spacing(2).padding(4);
    if matches!(select.kind, 5..=8) {
        panel = panel.push(
            container(
                field(theme, SEARCH, tr("Search"), &select.query, false, true)
                    .on_input(|query| Message::BotSelect(Event::Search(query))),
            )
            .padding(4),
        );
    }
    if shown == 0 {
        panel = panel.push(
            container(
                text(tr("No matching options."))
                    .size(14)
                    .color(theme.secondary),
            )
            .center_x(Length::Fill)
            .height(ROW)
            .align_y(Alignment::Center),
        );
    }
    panel = panel.push(scroll_view(list).style(scrollbar(theme)));
    let surface = yapfast_core::themes::package::art::ArtSurface::Popout;
    let decorated =
        ctx.themes.decorates(&theme.id, surface) || theme.package_paint(surface.ground()).is_some();
    let (raised, edge) = (
        if decorated {
            iced::Color::TRANSPARENT
        } else {
            theme.raised
        },
        theme.edge,
    );
    let panel = crate::theme_art::surface(
        container(panel)
            .max_height(MOST)
            .width(Length::Fill)
            .clip(true)
            .style(move |_| crate::style::surface_panel(theme, "popout", raised, edge, 8.0)),
        ctx,
        surface,
    );
    iced::widget::opaque(panel)
}

/// An option's emoji: a Unicode one's picture, or one of the message's
/// custom emoji by `:name:`.
fn choice_emoji<'a>(ctx: crate::ctx::Ctx<'a>, item: &str, emoji: &str) -> Element<'a, Message> {
    let custom = ctx
        .state
        .message(item)
        .and_then(|message| message.emoji.iter().find(|custom| custom.text == emoji));
    match custom.and_then(|custom| ctx.picture(&custom.picture, 18, 18, false)) {
        Some(handle) => crate::widgets::Pic::square(handle, 18.0).into(),
        None if custom.is_some() => Space::new().width(18).height(18).into(),
        None => crate::suggest::emoji_face(ctx, emoji, 18),
    }
}

/// A menu's box, with its dropdown under it while it's the one open.
pub fn select_with_dropdown<'a>(
    ctx: crate::ctx::Ctx<'a>,
    boxed: Element<'a, Message>,
    item: &str,
    custom_id: &str,
) -> Element<'a, Message> {
    let open = ctx.state.select_menu.as_ref().filter(|select| {
        select.item == item && select.custom_id == custom_id && select.modal_field.is_none()
    });
    Popover::new(boxed, open.map(|_| dropdown(ctx)), Side::Below)
        .width(Width::Same)
        .gap(4.0)
        .outside(Message::BotSelect(Event::Close))
        .into()
}
