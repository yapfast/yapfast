//! Composer tools: Gift and Apps are icon buttons in Discord's places, with
//! Yapfast's menus under them. Commands stay in Yapfast; purchases, app installation and activities open Discord in the
//! browser. Opening a menu never buys, authorizes, launches or sends a message.

use iced::{Element, Point, Task, widget::operation};
use yapfast_core::{
    application::{Action, network_link},
    discord,
    i18n::tr,
    network::Network,
};

use crate::{
    app::{App, Message, NOBODY},
    ctx::Ctx,
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
    widgets::Anchor,
};

const WIDTH: f32 = 320.0;
const APP_DIRECTORY: &str = "https://discord.com/discovery/applications";
const GIFT_GUIDE: &str = "https://support.discord.com/hc/en-us/articles/360020877112-Nitro-Gifting";

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

/// The gift's and the apps' icon buttons, 22 px glyphs in the secondary ink
/// like the picker's: the menus they open say these are Yapfast's.
pub fn button(ctx: Ctx<'_>, kind: Kind) -> Element<'static, Message> {
    let (icon, label) = match kind {
        Kind::Gift => (Icon::Gift, tr("Gift")),
        Kind::Apps => (Icon::Apps, tr("Apps")),
    };
    super::composer_button(
        ctx,
        icon,
        label,
        22,
        ctx.look.theme.secondary,
        Some(Message::Write(super::Event::Tools(kind))),
    )
}

pub fn open(app: &mut App, kind: Kind) -> Task<Message> {
    if !available(app.ctx()) {
        return Task::none();
    }
    let Some(room) = app.client.state().selected.as_deref() else {
        return Task::none();
    };
    let Some(page) = channel_page(room) else {
        return Task::none();
    };
    let rows = match kind {
        Kind::Gift => vec![
            browser_row(
                "Open chat for gifting",
                "Browser: choose Gift Nitro on discord.com",
                page,
            ),
            browser_row(
                "Nitro gifting guide",
                "Browser: Discord's purchase instructions",
                GIFT_GUIDE.to_owned(),
            ),
        ],
        Kind::Apps => {
            let state = app.client.state();
            let resuming = state.command_draft().is_some() || state.slash_query().is_some();
            let can_browse = resuming || state.draft().is_empty();
            vec![
                MenuRow {
                    disabled: !can_browse,
                    subtitle: Some(
                        if resuming {
                            "Native: resume your command in Yapfast"
                        } else if can_browse {
                            "Native: choose a slash command"
                        } else {
                            "Finish or clear your draft first"
                        }
                        .into(),
                    ),
                    ..MenuRow::new(
                        MenuAct::ComposerCommands,
                        "Commands in Yapfast",
                        Icon::Slash,
                    )
                    .key(room.to_owned())
                },
                MenuRow {
                    separated: true,
                    ..browser_row(
                        "Open chat for activities",
                        "Browser: launch and authorize on discord.com",
                        page,
                    )
                },
                browser_row(
                    "Find apps on discord.com",
                    "Browser: browse, install and authorize there",
                    APP_DIRECTORY.to_owned(),
                ),
            ]
        }
    };
    // A picker opened previously must not keep its search over the menu.
    let closed = if app.win.picker.is_open() {
        crate::picker::close_picker(app)
    } else {
        Task::none()
    };
    let at = Point::new(
        crate::conversation::chat_right(app) - WIDTH - super::GUTTER,
        super::composer_top(app) - 8.0,
    );
    app.win.menu = Some(Menu::new(at, rows).anchor(Anchor::Bottom).width(WIDTH));
    Task::batch([closed, operation::focus(NOBODY)])
}

fn browser_row(label: &'static str, subtitle: &'static str, page: String) -> MenuRow {
    MenuRow {
        subtitle: Some(subtitle.into()),
        ..MenuRow::new(MenuAct::OpenTextLink, label, Icon::Link).key(page)
    }
}

/// Validate the existing room-link format before offering a browser handoff.
/// This accepts DMs, groups, server channels and threads, with no query flags.
pub(super) fn channel_page(room: &str) -> Option<String> {
    channel_parts(room)?;
    Some(network_link(room))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_handoffs_use_only_discord_channel_pages() {
        for (room, page) in [
            ("discord:10:11", "https://discord.com/channels/10/11"),
            ("discord:@me:20", "https://discord.com/channels/@me/20"),
        ] {
            assert_eq!(channel_page(room).as_deref(), Some(page));
        }
        for room in [
            "!room:example.org",
            "discord:10",
            "discord:10:11?launch=true",
            "discord:10:11/12",
            "discord:@favorites:20",
        ] {
            assert!(channel_page(room).is_none(), "{room}");
        }
    }
}
