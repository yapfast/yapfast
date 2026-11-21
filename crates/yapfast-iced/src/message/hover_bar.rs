//! A message's hover bar: quick reactions, Edit or Reply, Forward and More,
//! and where their menus open.

use iced::{
    Alignment, Element, Point,
    widget::{container, mouse_area, row},
};
use yapfast_core::chat::{self};

use crate::{app::Message, controls, menus::MenuAct, pictures::Icon};

use super::BAR_BUTTON;
use yapfast_core::i18n::tr;

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
    let button = |icon: Icon, label: &'static str, message: Message| {
        ctx.look
            .icon_button(icon, label, message)
            .size(BAR_BUTTON, 20)
    };
    let mut buttons = row![].align_y(Alignment::Center);
    if quick && message.can_pin {
        buttons = buttons.push(if message.pinned {
            button(Icon::PinOff, tr("Unpin"), act(MenuAct::Pin))
        } else {
            button(Icon::Pin, tr("Pin"), act(MenuAct::Pin))
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
            tr("Create Thread"),
            act(MenuAct::NewThread),
        ));
    }
    if quick && message.own && message.can_edit && message.can_reply && writes {
        buttons = buttons.push(button(Icon::Reply, tr("Reply"), act(MenuAct::Reply)));
    }
    if message.can_react && super::reaction_chips::reacts(ctx, index) == chat::Reacts::Any {
        if !quick {
            for key in ctx.app.frequent_reactions(quick_reactions) {
                buttons = buttons.push(crate::suggest::emoji_button(
                    ctx,
                    &key,
                    20,
                    BAR_BUTTON,
                    (None, theme.wash(0.1)),
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
            tr("Add reaction"),
            act(MenuAct::AddReaction),
        ));
    }
    if message.own && message.can_edit && writes {
        buttons = buttons.push(button(Icon::Pencil, tr("Edit"), act(MenuAct::Edit)));
    } else if message.can_reply && writes {
        buttons = buttons.push(button(Icon::Reply, tr("Reply"), act(MenuAct::Reply)));
    }
    // Beside Reply, as in Discord, whether or not Shift is held.
    if message.can_forward && crate::forward::allowed(ctx.state) {
        buttons = buttons.push(button(Icon::Forward, tr("Forward"), act(MenuAct::Forward)));
    }
    buttons = buttons.push(if deletes {
        ctx.look.face_button(
            ctx.look.glyph(Icon::Trash, 20, theme.danger),
            BAR_BUTTON,
            tr("Delete"),
            act(MenuAct::DeleteNow),
        )
    } else {
        button(
            Icon::Ellipsis,
            tr("More"),
            Message::Timeline(crate::timeline::Event::RowMenu(shared.clone(), at)),
        )
    });
    let (ground, edge) = (theme.raised, theme.edge);
    let bar = mouse_area(container(buttons).padding(3).style(move |_| {
        crate::style::outlined(ground, edge, 8.0).shadow(crate::style::hover_bar_shadow())
    }))
    .on_enter(Message::Timeline(crate::timeline::Event::PointRow(
        shared, true,
    )));
    // The pointer on the bar is on its message, though the bar floats
    // over the row above: the bar keeps its moves from that row, and
    // from its own row's saying the pointer left. Not while a drag
    // selects, whose moves the text under it follows.
    Some(if ctx.lanes.timeline.selecting {
        bar.into()
    } else {
        crate::widgets::Holds::new(bar).into()
    })
}
