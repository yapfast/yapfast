//! A message's menu, made as it opens from the core's message: what this
//! account may do with it (`MessageMenu`'s conditions,
//! `ui/messages.slint:1394`, with the window's `open-message-menu`,
//! `ui/yapfast.slint:2460`), the four reactions used most over its rows,
//! and the open channel's apps' message commands. The rows are
//! `anomi_chat::menu::build`'s, as they carry the screen's words.

use iced::{Point, Rectangle, Size};
use yapfast_core::application::{Application, CommandTarget};
use yapfast_core::chat::{ChatMessage, Delivery, Reacts};

use anomi_vm::Vm;
use anomi_vm::chat::emoji_picker::{Custom, Emoji};
use anomi_vm::chat::menu::{App, Facts, Media, Menu};
use anomi_vm::kind::Expected;
use anomi_vm::menu::{Align, Side, Spot};
use anomi_vm::shell::Layer;

use crate::project::chat::message::{EPHEMERAL, LOADING, Look, frequent};

/// The quick reactions over the rows (`menu-reactions`).
const QUICK: usize = 4;
/// The menu's least width (`max(224px, ..)`), the Apps submenu's with the
/// space before it (`4px + apps-width`), and the edge it keeps from the
/// window's sides.
const NARROWEST: f32 = 224.0;
const APPS: f32 = 4.0 + 224.0;
const EDGE: f32 = 8.0;
/// Between a button and the menu beside it.
const BESIDE: f32 = 8.0;
/// The room a menu at the pointer wants under its bottom to open below
/// it (`anchor-y + 4px + self.height`).
const BELOW: f32 = 4.0;

/// The menu of `message`, its row `key`, opened on its picture or video
/// `media`; and whether its Apps are still to come.
pub(crate) fn build(
    core: &Application,
    look: &Look,
    message: &ChatMessage,
    key: u64,
    media: Option<Media>,
) -> (Menu, bool) {
    let state = &core.state;
    let (link, id) = state.message_link(&message.item).unwrap_or_default();
    let address = media
        .and_then(|media| message.attachments.get(media.index)?.url.as_deref())
        .unwrap_or_default()
        .to_owned();
    let facts = Facts {
        can_react: message.can_react && look.reacts == Reacts::Any,
        has_reactions: !message.reactions.is_empty(),
        can_edit: message.can_edit,
        can_delete: message.can_delete,
        can_reply: message.can_reply,
        can_forward: message.can_forward,
        can_pin: message.can_pin,
        pinned: message.pinned,
        can_start_thread: look.threads,
        thread: message.thread.as_ref().map(|thread| thread.room.clone()),
        composer_enabled: look.writes,
        forward_allowed: look.forwards,
        delivered: message.delivery == Delivery::Sent,
        body: message.body.clone(),
        selection: String::new(),
        can_end_poll: message.poll.as_ref().is_some_and(|poll| poll.can_end),
        can_mark_unread: core.marks_unread(&message.item),
        link,
        id,
        media,
        address,
        preview: look.preview,
    };
    let reactions = if facts.can_react {
        reactions(core)
    } else {
        Vec::new()
    };
    let takes = takes_apps(message);
    let mut menu = Menu {
        key,
        facts,
        reactions,
        apps: if takes { apps(core) } else { Vec::new() },
        ..Menu::default()
    };
    let due = takes && !apps_loaded(core);
    anomi_chat::menu::build(&mut menu);
    (menu, due)
}

/// A Discord message sent or got, not one still sending, an answer only
/// this account sees, or Yapfast's "Sending command..." (`has-apps`).
fn takes_apps(message: &ChatMessage) -> bool {
    message.item.starts_with("d:")
        && message.delivery == Delivery::Sent
        && message.flags & (EPHEMERAL | LOADING) == 0
}

/// Whether the open room's command index came.
fn apps_loaded(core: &Application) -> bool {
    let state = &core.state;
    state
        .selected
        .as_deref()
        .is_some_and(|room| state.app_commands.index(room).is_some())
}

/// The open room's apps' message commands.
fn apps(core: &Application) -> Vec<App> {
    let Some(room) = core.state.selected.as_deref() else {
        return Vec::new();
    };
    core.commands_for_target(room, CommandTarget::Message)
        .into_iter()
        .map(|command| App {
            id: command.command,
            name: command.name,
            app: command.app_name,
        })
        .collect()
}

/// The menu opened before the room's apps came: its Apps go in once they
/// have, in their place among the rows (which name their message and
/// choice, not their place), and the menu, at `spot` in a window of
/// `window`, makes room for them. Whether they came.
pub(super) fn apps_came(core: &Application, vm: &mut Vm, spot: Option<(Spot, Size)>) -> bool {
    if !apps_loaded(core) {
        return false;
    }
    let menu = &mut vm.chat.menu;
    menu.apps = apps(core);
    if menu.apps.is_empty() {
        return true;
    }
    anomi_chat::menu::build(menu);
    if let Some((spot, window)) = spot {
        let (at, flip) = place(spot, menu, window);
        menu.flip = flip;
        if vm.shell.is_open(Layer::MessageMenu) {
            vm.shell.open_at(Layer::MessageMenu, at);
        }
    }
    true
}

/// Where the menu's card goes for `spot`, and whether its Apps open on its
/// left, as the Slint window's `message-menu` places it: at the pointer,
/// below it or else above it, or else at the window's top; from a button,
/// left of it level with its top, or right of it without room, kept in the
/// window. With Apps the card is wider by their room, on the menu's right,
/// or its left near the window's right edge, the menu staying put. The
/// spot it gets is the card's top left corner.
pub(crate) fn place(spot: Spot, menu: &Menu, window: Size) -> (Spot, bool) {
    let width = anomi_widgets::menu::width(&menu.rows, NARROWEST);
    let height = anomi_chat::menu::size(menu).height;
    let apps = !menu.apps.is_empty();
    let anchor = spot.anchor;
    let pointer = anchor.width == 0.0 && anchor.height == 0.0;
    let fit = |x: f32| x.min(window.width - width - EDGE).max(EDGE);
    let left = if pointer {
        fit(anchor.x)
    } else if anchor.x - width - BESIDE >= EDGE {
        anchor.x - width - BESIDE
    } else {
        fit(anchor.x + anchor.width + BESIDE)
    };
    let flip = apps && left + width + APPS > window.width - EDGE;
    let x = if flip { left - APPS } else { left };
    let y = if !pointer {
        anchor.y.min(window.height - height - EDGE).max(EDGE)
    } else if anchor.y + BELOW + height <= window.height - EDGE {
        anchor.y
    } else {
        (anchor.y - height - BELOW).max(EDGE)
    };
    // Beside a point, its top left corner there: the card is laid out
    // whole, as tall as it is.
    let at = Spot {
        anchor: Rectangle::new(Point::new(x, y), Size::ZERO),
        side: Side::Right,
        align: Align::Start,
        gap: 0.0,
    };
    (at, flip)
}

/// The reactions this device uses most, as the picker's emoji: Unicode's
/// by their glyph, a server's with its picture once it came.
fn reactions(core: &Application) -> Vec<Emoji> {
    frequent(core, QUICK)
        .into_iter()
        .map(|quick| {
            Emoji::unicode(&quick.key).unwrap_or_else(|| {
                Emoji::Custom(Box::new(Custom {
                    picture: quick.picture.map_or(Expected::Waiting, Expected::Ready),
                    key: quick.key,
                    usable: true,
                }))
            })
        })
        .collect()
}
