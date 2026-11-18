//! What the screens ask, handled: each area's intents in its own module,
//! which either changes the view-model's own state (a filter's text) or
//! hands the core an `Action`, as `src/ui/callbacks.rs` does for the Slint
//! window.
//!
//! [`handle`] routes every area to the module of the I task that handles it
//! (docs/iced/tasks/integration.md); until that task, the module logs what
//! it gets. An intent another area acts on is routed to that area's module
//! here, ahead of its own area's arm (the header's member list switch goes
//! to the shell, which keeps it in the layout). An area's own `Intent` is `#[non_exhaustive]`, so a handler
//! matches it with a wildcard arm and a screen's task can add a variant
//! without breaking this crate.

pub mod account;
mod chat;
mod edit;
mod editor;
mod forward;
mod header;
mod inbox;
mod menus;
mod people;
mod places;
mod rail;
mod server_settings;
mod settings;
mod shell;
mod sidebar;
mod sign_in;
pub mod status;
mod voice;

use iced::Task;

use anomi_vm::Intent;
use anomi_vm::header::Intent as Header;
use anomi_vm::people::Intent as People;
use anomi_vm::settings::Intent as Settings;

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Shell(intent) => shell::handle(app, intent),
        Intent::Chrome(intent) => crate::platform::window::handle(app, intent),
        Intent::Menu(intent) => menus::handle(app, intent),
        Intent::Rail(intent) => rail::handle(app, intent),
        Intent::Sidebar(intent) => sidebar::handle(app, intent),
        Intent::Browse(intent) => sidebar::browse(app, intent),
        Intent::Account(intent) => account::handle(app, intent),
        // The two switches the layout keeps.
        Intent::Header(Header::Members) => shell::toggle_members(app),
        Intent::Header(Header::ProfilePanel) => shell::toggle_profile(app),
        Intent::Header(intent) => header::handle(app, intent),
        Intent::Inbox(intent) => inbox::handle(app, intent),
        Intent::SignIn(intent) => sign_in::handle(app, intent),
        intent @ (Intent::Switcher(_) | Intent::NewDm(_) | Intent::Places(_)) => {
            places::handle(app, intent)
        }
        Intent::Chat(intent) => chat::handle(app, intent),
        Intent::People(People::Status(intent)) => status::handle(app, intent),
        Intent::People(
            intent @ (People::Editor(_) | People::EditorPreview(_) | People::NameStyle(_)),
        ) => editor::handle(app, intent),
        Intent::People(intent) => people::handle(app, intent),
        Intent::Voice(intent) => voice::handle(app, intent),
        Intent::Settings(Settings::Mfa(intent)) => sign_in::mfa(app, intent),
        Intent::Settings(
            intent @ (Settings::Server(_)
            | Settings::Engagement(_)
            | Settings::Boosts(_)
            | Settings::Members(_)
            | Settings::Invites(_)
            | Settings::Safety(_)
            | Settings::Bans(_)
            | Settings::Audit(_)
            | Settings::Roles(_)
            | Settings::RoleEditor(_)
            | Settings::Emoji(_)
            | Settings::Stickers(_)
            | Settings::Ownership(_)
            | Settings::Channel(_)
            | Settings::Permissions(_)
            | Settings::CreateChannel(_)),
        ) => server_settings::handle(app, intent),
        Intent::Settings(intent) => settings::handle(app, intent),
    }
}
