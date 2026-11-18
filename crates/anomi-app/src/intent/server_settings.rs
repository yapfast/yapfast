//! A server's and a channel's settings' intents as the core's actions
//! (task I20), ported from `bind` of `src/ui/server_settings.rs:1616` and
//! `src/ui/channel_settings.rs:367`. Nothing goes to Discord until Save but
//! what Discord's client saves as it's made (emoji, stickers, members,
//! invites, safety, bans); leaving with changes not saved shakes the bar,
//! which the core counts. What only the window keeps (a field written
//! before it's used, the row under the pointer, the role dragged) changes
//! the view-model alone.

mod channel;
mod expressions;
mod pages;
mod roles;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::Duration;

use iced::Task;
use yapfast_core::application::{Action, ChannelInput, EmojiInput, InviteInput, OwnershipInput};
use yapfast_core::application::{BanInput, MemberInput, RoleInput, StickerInput};
use yapfast_core::chat::ServerPicture;
use yapfast_lang::tr;

use anomi_vm::settings::Intent;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::{Message, Once};

/// How long the members' search waits for the typing to stop, as Discord's
/// client does.
const SEARCH_AFTER: Duration = Duration::from_millis(300);

/// A wait of these settings' ended.
#[derive(Clone, Debug)]
pub enum Due {
    /// The members' search typed, by its generation: sent unless typed
    /// again since.
    Search(u64),
}

/// The server and channel pages' intents.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Server(intent) => pages::server(app, intent),
        Intent::Engagement(intent) => pages::engagement(app, intent),
        Intent::Boosts(intent) => pages::boosts(app, intent),
        Intent::Safety(intent) => pages::safety(app, intent),
        Intent::Members(intent) => pages::members(app, intent),
        Intent::Invites(intent) => pages::invites(app, intent),
        Intent::Bans(intent) => pages::bans(app, intent),
        Intent::Audit(intent) => pages::audit(app, intent),
        Intent::Roles(intent) => roles::list(app, intent),
        Intent::RoleEditor(intent) => roles::editor(app, intent),
        Intent::Emoji(intent) => expressions::emoji(app, intent),
        Intent::Stickers(intent) => expressions::stickers(app, intent),
        Intent::Ownership(intent) => expressions::ownership(app, intent),
        Intent::Channel(intent) => channel::overview(app, intent),
        Intent::Permissions(intent) => channel::permissions(app, intent),
        Intent::CreateChannel(intent) => channel::create(app, intent),
        intent => {
            tracing::debug!(?intent, "not a server's or a channel's settings' intent");
            Task::none()
        }
    }
}

/// A wait ended.
pub fn due(app: &mut App, due: Due) -> Task<Message> {
    match due {
        Due::Search(generation) if generation == app.shown.server_settings.search => {
            let text = app.vm.settings.members.search.clone();
            app.dispatch(Action::Members(MemberInput::Search(text)))
        }
        Due::Search(_) => Task::none(),
    }
}

/// The members' search typed: it goes once the typing stops.
pub(super) fn search_later(app: &mut App) -> Task<Message> {
    let shown = &mut app.shown.server_settings;
    shown.search += 1;
    let due = Message::ServerSettings(Due::Search(shown.search));
    crate::app::later(SEARCH_AFTER, due)
}

/// Whether `layer` is one of these settings' and their dialogs'.
pub fn holds(layer: Layer) -> bool {
    matches!(
        layer,
        Layer::ServerSettings
            | Layer::SecurityActions
            | Layer::Ban
            | Layer::RoleConfirm
            | Layer::RoleAdding
            | Layer::EmojiUpload
            | Layer::StickerForm
            | Layer::Ownership
            | Layer::ChannelSettings
            | Layer::PermissionsConfirm
            | Layer::PermissionsAdd
            | Layer::CreateChannel
    )
}

/// Escape, a press beside, or a dialog's X: each asks its page, which the
/// projection follows as the core lets go. A sticker being saved stays.
/// Delete Server and Transfer Ownership ask the core too, which keeps them
/// while Discord carries them out, as the Slint app's Escape did.
pub fn close(app: &mut App, layer: Layer) -> Task<Message> {
    let action = match layer {
        Layer::ServerSettings => {
            let rename = expressions::rename_written(app);
            return Task::batch([rename, app.dispatch(Action::CloseServerSettings)]);
        }
        Layer::SecurityActions => Action::Invites(InviteInput::CloseSecurity),
        Layer::Ban => Action::Bans(BanInput::Close),
        Layer::RoleConfirm => Action::Roles(RoleInput::Cancel),
        Layer::RoleAdding => Action::Roles(RoleInput::CloseAdding),
        Layer::EmojiUpload => Action::Emoji(EmojiInput::Cancel),
        Layer::StickerForm => {
            let form = app.vm.settings.stickers.form.as_ref();
            if form.is_some_and(|form| form.saving) {
                return Task::none();
            }
            Action::Stickers(StickerInput::Cancel)
        }
        Layer::Ownership => Action::Ownership(OwnershipInput::Cancel),
        Layer::ChannelSettings => Action::ChannelSettings(ChannelInput::Close),
        Layer::PermissionsConfirm => Action::ChannelSettings(ChannelInput::Cancel),
        Layer::PermissionsAdd => Action::ChannelSettings(ChannelInput::Adding(false)),
        Layer::CreateChannel => Action::CloseChannelDialog,
        layer => {
            app.vm.shell.close(layer);
            return Task::none();
        }
    };
    app.dispatch(action)
}

/// A layer of these settings played its way out: what it showed goes.
pub fn gone(app: &mut App, layer: Layer) {
    crate::project::server_settings::gone(&mut app.vm, &mut app.shown.server_settings, layer);
}

/// Opens the file dialog for a picture of the server's, where nothing is
/// read or saved meanwhile; the core reads the file chosen.
pub(super) fn pick(app: &mut App, kind: ServerPicture) -> Task<Message> {
    let ready = app
        .core
        .state
        .server_settings
        .as_ref()
        .is_some_and(|settings| !settings.saving && settings.reading.is_none());
    if !ready {
        return Task::none();
    }
    let title = match kind {
        ServerPicture::Icon => tr!("Change Server Icon"),
        ServerPicture::Banner => tr!("Upload Server Banner"),
        ServerPicture::Splash => tr!("Upload Invite Background"),
        ServerPicture::RoleIcon => tr!("Choose Role Icon"),
        ServerPicture::Emoji => tr!("Upload Emoji"),
        ServerPicture::Sticker => tr!("Upload Sticker"),
    };
    // Discord's stickers are PNG, APNG, JPEG or GIF.
    let kinds: &'static [&'static str] = if kind == ServerPicture::Sticker {
        &["png", "apng", "jpg", "jpeg", "gif"]
    } else {
        &["png", "jpg", "jpeg", "gif", "webp"]
    };
    let choose = move || {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter(tr!("Pictures"), kinds)
            .pick_file()
    };
    Task::perform(
        async move { tokio::task::spawn_blocking(choose).await.ok().flatten() },
        |path| path,
    )
    .and_then(move |path| Task::done(chosen(kind, path)))
}

/// The file chosen for `kind`, for the core to read.
pub(crate) fn chosen(kind: ServerPicture, path: PathBuf) -> Message {
    Message::Act(Once::new(Action::PickServerPicture { kind, path }))
}

/// A server's picture as the test session names it: `pick icon`,
/// `server-banner`, `splash`, `role-icon`, `emoji`, `sticker`.
pub(crate) fn picture(name: &str) -> Option<ServerPicture> {
    Some(match name {
        "icon" => ServerPicture::Icon,
        "server-banner" => ServerPicture::Banner,
        "splash" => ServerPicture::Splash,
        "role-icon" => ServerPicture::RoleIcon,
        "emoji" => ServerPicture::Emoji,
        "sticker" => ServerPicture::Sticker,
        _ => return None,
    })
}
