//! Composer tools: Gift and Apps are icon buttons in Discord's places, and
//! both work in Yapfast. Gift lists the gifts the account bought and can
//! still give, and a chosen one puts its link in the message; Apps opens
//! the native command picker, or resumes the command being written. Opening
//! either never buys, authorizes, launches or sends a message. Yapfast has no
//! checkout, so it sells no gift, and activities need Discord's own web
//! client, so it has none.

use iced::{Element, Point, Task, widget::operation};
use yapfast_core::{
    application::Action, discord, i18n::tr, network::Network, storefront::GiftInventory,
};

use crate::{
    app::{App, Message, NOBODY},
    ctx::Ctx,
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
    widgets::Anchor,
};

const WIDTH: f32 = 320.0;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Gift,
    Apps,
}

/// Only a writable Discord chat offers these tools, never a message request.
pub fn available(ctx: Ctx<'_>) -> bool {
    ctx.state.can_write()
        && ctx.state.selected_room().is_some_and(|room| {
            room.network == Network::Discord && !room.request && channel_parts(&room.id).is_some()
        })
}

/// Whether Apps can open the command picker now: it clears nothing, so a
/// draft that is not a command being written waits to be sent or cleared.
fn can_browse(ctx: Ctx<'_>) -> bool {
    let state = ctx.state;
    state.command_draft().is_some() || state.slash_query().is_some() || state.draft().is_empty()
}

/// The gift's and the apps' icon buttons, 22 px glyphs in the secondary ink
/// like the picker's. Apps is dim, its tooltip saying why, while a draft
/// would be lost to the command picker.
pub fn button(ctx: Ctx<'_>, kind: Kind) -> Element<'static, Message> {
    let (icon, label, enabled) = match kind {
        Kind::Gift => (Icon::Gift, tr("Gift"), true),
        Kind::Apps if can_browse(ctx) => (Icon::Apps, tr("Apps"), true),
        Kind::Apps => (
            Icon::Apps,
            tr("Apps: finish or clear your message first"),
            false,
        ),
    };
    super::composer_button(
        ctx,
        icon,
        label,
        22,
        ctx.look.theme.secondary,
        enabled.then_some(Message::Write(super::Event::Tools(kind))),
    )
}

pub fn open(app: &mut App, kind: Kind) -> Task<Message> {
    if !available(app.ctx()) {
        return Task::none();
    }
    let Some(room) = app.client.state().selected.clone() else {
        return Task::none();
    };
    match kind {
        Kind::Apps => commands(app, &room),
        Kind::Gift => gifts(app, &room),
    }
}

/// The Gift menu: the gifts the account can give, read again as it opens.
fn gifts(app: &mut App, room: &str) -> Task<Message> {
    // A picker opened previously must not keep its search over the menu.
    let closed = if app.win.picker.is_open() {
        crate::picker::close_picker(app)
    } else {
        Task::none()
    };
    app.client.dispatch(Action::LoadGifts);
    let at = Point::new(
        crate::conversation::chat_right(app) - WIDTH - super::GUTTER,
        super::composer_top(app) - 8.0,
    );
    let rows = gift_rows(&app.client.state().gifts, room);
    app.win.menu = Some(Menu {
        gifts: true,
        ..Menu::new(at, rows).anchor(Anchor::Bottom).width(WIDTH)
    });
    Task::batch([closed, operation::focus(NOBODY), app.sync()])
}

/// The rows of the Gift menu for what has been read of the gifts: each gift
/// the account can still give, or the one line that says why there is none.
fn gift_rows(gifts: &GiftInventory, room: &str) -> Vec<MenuRow> {
    let line = |label: &'static str, subtitle: Option<String>| MenuRow {
        disabled: true,
        subtitle: subtitle.map(Into::into),
        ..MenuRow::new(MenuAct::GiftLink, label, Icon::Gift)
    };
    if !gifts.loaded {
        return vec![match &gifts.error {
            Some(error) => line(tr("Could not load your gifts"), Some(error.clone())),
            None => line(tr("Loading your gifts"), None),
        }];
    }
    if gifts.gifts.is_empty() {
        return vec![line(
            tr("You have no gifts to give"),
            Some(tr("Gifts bought in Discord's apps are listed here").to_owned()),
        )];
    }
    gifts
        .gifts
        .iter()
        .map(|gift| MenuRow {
            icon: Some(Icon::Gift),
            subtitle: Some(tr("Put its link in your message").into()),
            key: format!("{room} {}", gift.code),
            ..MenuRow::plain(MenuAct::GiftLink, gift.name.clone())
        })
        .collect()
}

/// A Gift menu that is open follows the gifts as they are read: loading,
/// then the list, or why there is none.
pub fn gifts_sync(app: &mut App) {
    let Some(menu) = app.win.menu.as_ref().filter(|menu| menu.gifts) else {
        return;
    };
    let Some(room) = app.client.state().selected.clone() else {
        return;
    };
    let rows = gift_rows(&app.client.state().gifts, &room);
    let same = rows.len() == menu.rows.len()
        && rows.iter().zip(&menu.rows).all(|(new, old)| {
            (&new.label, &new.key, &new.subtitle, new.disabled)
                == (&old.label, &old.key, &old.subtitle, old.disabled)
        });
    if same {
        return;
    }
    if let Some(menu) = &mut app.win.menu {
        menu.lit = menu.lit.filter(|lit| *lit < rows.len());
        menu.rows = rows;
        menu.fits = Default::default();
    }
}

/// Puts the link of the gift in `key` (a room, a space, then its code) in
/// the message being written, where the caret is, for the person to send.
/// A gift of another room's menu, or in a room that no longer offers the
/// tools, goes nowhere.
pub fn give(app: &mut App, key: &str) -> Task<Message> {
    let Some((room, code)) = key.split_once(' ') else {
        return super::refocus(app);
    };
    if app.client.state().selected.as_deref() != Some(room) || !available(app.ctx()) {
        return super::refocus(app);
    }
    let draft = super::writing(app.ctx(), super::Writing::Draft);
    let text = draft.text();
    let caret = draft.caret().min(text.len());
    let apart = text[..caret]
        .chars()
        .next_back()
        .is_some_and(|before| !before.is_whitespace());
    let link = format!(
        "{}https://discord.gift/{code}",
        if apart { " " } else { "" }
    );
    app.composer.writing_last = super::Writing::Draft;
    super::insert(app, super::Writing::Draft, &link);
    super::refocus(app)
}

/// Borrow the ids on the view path, without allocating an address per frame.
fn channel_parts(room: &str) -> Option<(&str, &str)> {
    let (guild, channel) = discord::parse_room(room)?;
    let snowflake =
        |id: &str| !id.is_empty() && id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit());
    ((guild == discord::DM_GUILD || snowflake(guild)) && snowflake(channel))
        .then_some((guild, channel))
}

/// This reuses the command picker, including its permissions and validation.
/// Recheck the room and draft: either may have changed since the menu opened.
pub fn commands(app: &mut App, room: &str) -> Task<Message> {
    let state = app.client.state();
    if state.selected.as_deref() != Some(room) || !available(app.ctx()) {
        return super::refocus(app);
    }
    if state.command_draft().is_some() || state.slash_query().is_some() {
        app.composer.writing_last = super::Writing::Draft;
        return operation::focus(super::COMPOSER);
    }
    if !state.draft().is_empty() {
        return super::refocus(app);
    }
    app.composer.suggest.set_index(0);
    app.composer.suggest.command_offset = 0.0;
    app.composer.writing_last = super::Writing::Draft;
    app.client.dispatch(Action::SlashQuery(String::new()));
    Task::batch([app.sync(), operation::focus(super::COMPOSER)])
}
