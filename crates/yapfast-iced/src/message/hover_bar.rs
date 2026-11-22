//! A message's hover bar: quick reactions, Edit or Reply, Forward and More,
//! and where their menus open.

use iced::{
    Alignment, Element, Point,
    widget::{container, mouse_area, row},
};
use yapfast_core::chat::{self};

use crate::{app::Message, controls, menus::MenuAct, pictures::Icon};

use super::BAR_BUTTON;

/// Where the picker and the menu of row `index`'s message open, from
/// its hover bar or the keys: under the bar's right end.
pub fn bar_menu_at(ctx: crate::ctx::Ctx<'_>, index: usize) -> Point {
    let (lane, _) = ctx.lane_place(index);
    Point::new(
        lane.left + lane.width - 16.0 - 200.0,
        ctx.row_body_top(index) + 24.0,
    )
}

/// Whether the message `item`'s menu or reaction picker is open.
fn opened_from(ctx: crate::ctx::Ctx<'_>, item: &str) -> bool {
    ctx.menu
        .is_some_and(|menu| menu.item.as_deref() == Some(item))
        || ctx.picker.is_some_and(
            |picker| matches!(&picker.target, crate::picker::Target::React(on, _) if on == item),
        )
}

/// Whether the message `item`'s hover bar stays though the pointer has
/// left its row: while what it opened is open, its menu, its picker or
/// the question whether to delete it.
pub fn kept(ctx: crate::ctx::Ctx<'_>, item: &str) -> bool {
    opened_from(ctx, item) || ctx.state.confirm_delete.as_deref() == Some(item)
}

/// A message's hover bar: the three reactions this device uses most and Add
/// reaction, Edit on one's own message or Reply on someone else's, Forward,
/// and More, which opens its menu. Holding Shift swaps the reactions for
/// Pin, Create Thread (where one may start and the message has none) and,
/// on one's own message, Reply, and More for a Delete that doesn't
/// ask, as Discord's client does; not while the message's menu or picker is
/// open.
pub fn hover_bar<'a>(ctx: crate::ctx::Ctx<'a>, index: usize) -> Option<Element<'a, Message>> {
    let message = ctx.row_message(index)?;
    if message.deleted || message.delivery != chat::Delivery::default() {
        return None;
    }
    let theme = ctx.look.theme;
    let at = bar_menu_at(ctx, index);
    // The thread beside the channel is narrower: two quick reactions.
    let quick_reactions = if index >= crate::timeline::ASIDE {
        2
    } else {
        3
    };
    let item = &message.item;
    let shared = ctx.row_item(index);
    let writes = if index >= crate::timeline::ASIDE {
        crate::thread::aside_writes(ctx)
    } else {
        crate::composer::composer_look(ctx).enabled
    };
    let quick = ctx.modifiers.shift() && !opened_from(ctx, item);
    let deletes = quick && message.can_delete && writes;
    let act = |act: MenuAct| Message::Menu(crate::menus::Event::Act(act, item.clone(), at));
    let hover_wash = theme.token_colour("actions-hover", theme.wash(0.1));
    let button = |icon: Icon, label: &'static str, message: Message| {
        ctx.look
            .icon_button(icon, label, message)
            .size(BAR_BUTTON, 20)
            .hover_wash(hover_wash)
    };
    let mut buttons = row![].align_y(Alignment::Center);
    if quick && message.can_pin {
        buttons = buttons.push(if message.pinned {
            button(Icon::PinOff, "Unpin", act(MenuAct::Pin))
        } else {
            button(Icon::Pin, "Pin", act(MenuAct::Pin))
        });
    }
    // Not in the thread beside, where no thread starts.
    if quick
        && index < crate::timeline::ASIDE
        && ctx.app.can_start_threads()
        && message.can_reply
        && message.thread.is_none()
    {
        buttons = buttons.push(button(
            Icon::Thread,
            "Create Thread",
            act(MenuAct::NewThread),
        ));
    }
    if quick && message.own && message.can_edit && message.can_reply && writes {
        buttons = buttons.push(button(Icon::Reply, "Reply", act(MenuAct::Reply)));
    }
    if message.can_react && super::reaction_chips::reacts(ctx, index) == chat::Reacts::Any {
        if !quick {
            for key in ctx.app.frequent_reactions(quick_reactions) {
                buttons = buttons.push(crate::suggest::emoji_button(
                    ctx,
                    &key,
                    20,
                    BAR_BUTTON,
                    (None, hover_wash),
                    Message::Menu(crate::menus::Event::React(item.clone(), key.clone())),
                ));
            }
            buttons = buttons.push(
                container(controls::bar(theme.line, 1, 20))
                    .width(9)
                    .center_x(9),
            );
        }
        buttons = buttons.push(button(
            Icon::SmilePlus,
            "Add reaction",
            act(MenuAct::AddReaction),
        ));
    }
    if message.own && message.can_edit && writes {
        buttons = buttons.push(button(Icon::Pencil, "Edit", act(MenuAct::Edit)));
    } else if message.can_reply && writes {
        buttons = buttons.push(button(Icon::Reply, "Reply", act(MenuAct::Reply)));
    }
    // Beside Reply, as in Discord, whether or not Shift is held.
    if message.can_forward && crate::forward::allowed(ctx.state) {
        buttons = buttons.push(button(Icon::Forward, "Forward", act(MenuAct::Forward)));
    }
    buttons = buttons.push(if deletes {
        ctx.look.face_button(
            ctx.look.glyph(Icon::Trash, 20, theme.danger),
            BAR_BUTTON,
            "Delete",
            act(MenuAct::DeleteNow),
        )
    } else {
        button(
            Icon::Ellipsis,
            "More",
            Message::Timeline(crate::timeline::Event::RowMenu(shared.clone(), at)),
        )
    });
    let surface = yapfast_core::themes::package::art::ArtSurface::Actions;
    let decorated =
        ctx.themes.decorates(&theme.id, surface) || theme.package_paint("actions").is_some();
    let ground = if decorated {
        iced::Color::TRANSPARENT
    } else {
        theme.raised
    };
    let edge = theme.edge;
    let custom_outline = theme.outline("actions").is_some();
    // As Slint's `ActionsFrame`: a solid outline is the frame's own
    // border, under its art (Zine's tape strip covers it); only a dashed
    // or dotted one is drawn over everything.
    let solid = ctx
        .themes
        .package_layout(&theme.id)
        .and_then(|layout| {
            layout
                .borders
                .get(&yapfast_core::themes::layout::BorderPlace::Actions)
        })
        .filter(|border| border.stroke == yapfast_core::themes::layout::Stroke::Solid)
        .copied();
    // Under a theme's art a solid outline doesn't show (Zine's tape strip
    // covers it); without art it's the frame's edge (Phosphor's green).
    let has_art = !ctx.themes.art(&theme.id, surface).is_empty();
    let solid_edge = solid.filter(|_| !has_art).map(|solid| {
        let mut border = theme.surface_border("actions", 8.0, 1.0, edge);
        border.width = solid.width.unwrap_or(border.width);
        border.color = solid
            .colour
            .map(crate::themes::argb)
            .unwrap_or(border.color);
        border.radius = theme
            .surface_radius("actions", 8.0)
            .min((BAR_BUTTON + 6.0) / 2.0)
            .into();
        border
    });
    let look = theme.part("timeline.actions", 0);
    let action = container(buttons).padding(3).style(move |_| {
        let mut style = crate::style::surface_panel(theme, "actions", ground, edge, 8.0);
        if custom_outline {
            style.border.width = 0.0;
        }
        if let Some(border) = solid_edge {
            style.border = border;
        }
        // A dressed bar's ground and art lie under this see-through face:
        // its shadow goes under them (below), not over them through it
        // (Bubblegum's hard purple shadow filled its pink pill).
        style.shadow = if decorated {
            iced::Shadow::default()
        } else {
            theme.surface_shadow("actions", crate::style::hover_bar_shadow())
        };
        yapfast_iced_kit::parts::style(look.as_ref(), style)
    });
    let action = yapfast_iced_kit::parts::show(theme, "timeline.actions", 0, action);
    let action = crate::theme_art::surface(action, ctx, surface);
    let action: Element<'a, Message> = if decorated {
        let shadow = theme.surface_shadow("actions", crate::style::hover_bar_shadow());
        let paint = theme.surface_paint("actions");
        // Held to half the bar's height: past it iced draws a bar shape.
        let radius = theme
            .surface_radius("actions", 8.0)
            .min((BAR_BUTTON + 6.0) / 2.0);
        container(action)
            .style(move |_| {
                yapfast_iced_kit::parts::style(
                    look.as_ref(),
                    iced::widget::container::Style {
                        background: Some(paint),
                        border: iced::Border::default().rounded(radius),
                        shadow,
                        ..Default::default()
                    },
                )
            })
            .into()
    } else {
        action
    };
    if decorated && solid.is_some() {
        return Some(finish(ctx, shared, action));
    }
    let action = crate::theme_art::outlined(
        action,
        ctx,
        yapfast_core::themes::layout::BorderPlace::Actions,
        "actions",
        8.0,
        1.0,
        edge,
    );
    Some(finish(ctx, shared, action))
}

/// The bar under the pointer: its moves held from the rows round it.
fn finish<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shared: std::sync::Arc<str>,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    let bar = mouse_area(action).on_enter(Message::Timeline(crate::timeline::Event::PointRow(
        shared, true,
    )));
    // The pointer on the bar is on its message, though the bar floats
    // over the row above: the bar keeps its moves from that row, and
    // from its own row's saying the pointer left. Not while a drag
    // selects, whose moves the text under it follows.
    if ctx.lanes.timeline.selecting {
        bar.into()
    } else {
        crate::widgets::Holds::new(bar).into()
    }
}
