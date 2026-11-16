//! Roles, as the Slint client's `RolesSection` (`ui/roles.slint`) with its
//! adapter's rules: the roles in their order, which dragging a row by its
//! handle (or its More menu's Move Up and Move Down) changes, and the
//! editor of one, with its Display, Permissions and Manage Members tabs;
//! Delete Role's and Remove member's question, and Add members, over the
//! settings. What may change is the core's (`RoleRules`): a role at or
//! above this account's highest is locked, with Discord's words why, as is
//! a permission it doesn't hold or would lose.

use iced::{
    Color, Element, Length, Task,
    widget::{Space, column, row, text},
};

use yapfast_core::application::{
    DEFAULT_ROLE_COLOR, RoleConfirm, RoleInput, RoleLock, RoleTab, RolesPage, ServerSettings,
    SettingsPage,
};

use super::{Event as SettingsEvent, Screen, message};
use crate::server_settings::View;
use crate::{
    app::Message,
    controls::{self, Kind, action},
    ctx::Cx,
    fonts::BOLD,
    style::hex,
};

mod editor;
mod list;
mod members;
use editor::hex_colour;
pub use members::focus_candidates;

/// The fields' ids.
const SEARCH: &str = "roles-search";
const NAME: &str = "role-name";
const HEX: &str = "role-hex";
const EMOJI: &str = "role-emoji";
const PERMISSION_SEARCH: &str = "role-permission-search";
const MEMBER_SEARCH: &str = "role-member-search";
const CANDIDATES: &str = "role-candidates";
/// A row of the list, its handle's strip and mark, and the members' column
/// with the two buttons after it.
const ROW: f32 = 52.0;
const HANDLE: f32 = 20.0;
const MARK: f32 = 20.0;
const DOT: f32 = 14.0;
const HANDLE_DOT: f32 = 3.0;
const MEMBERS: f32 = 80.0;
const BUTTON: f32 = 32.0;
const GLYPH: u32 = 20;
/// How much of a greyed More button shows.
const DISABLED: f32 = 0.4;
/// The More menu's width.
const MORE: f32 = 180.0;
/// @everyone's card.
const CARD: f32 = 64.0;
const CARD_ICON: f32 = 36.0;
/// The editor's tabs and their underline.
const TAB: f32 = 36.0;
const UNDERLINE: f32 = 2.0;
/// The colours: Discord's default, the swatches, the custom one, its field.
const DEFAULT_SWATCH: (f32, f32) = (66.0, 50.0);
const SWATCH: f32 = 20.0;
const CUSTOM: f32 = 50.0;
const SWATCH_GAP: f32 = 10.0;
const SMALL_FIELD: f32 = 110.0;
/// The role icon's box and what's in it.
const ICON_BOX: f32 = 64.0;
const ICON_SHOWN: f32 = 48.0;
/// A member on Manage Members and in Add members.
const MEMBER: f32 = 44.0;
const MEMBER_AVATAR: f32 = 32.0;
const CANDIDATE_AVATAR: f32 = 28.0;
/// The dialogs, and the room Add members leaves above and below it.
const DIALOG: f32 = 440.0;
const ADD_HEIGHT: f32 = 520.0;
const ADD_ROOM: f32 = 80.0;

/// Roles' own state on the screen.
#[derive(Default)]
pub struct Local {
    /// The role whose More menu is open.
    more: Option<String>,
    /// The custom colour being typed, for the role it's typed for; it
    /// goes once applied.
    hex: Option<(String, String)>,
    /// An emoji being typed for the icon.
    emoji: String,
}

impl Local {
    /// Closes the More menu, if it was open.
    pub fn close_menu(&mut self) -> bool {
        self.more.take().is_some()
    }
}

/// What's done on Roles that's the screen's own.
#[derive(Clone, Debug)]
pub enum Event {
    More(Option<String>),
    Hex(String),
    ApplyHex,
    Emoji(String),
    UseEmoji,
    /// A row dragged, or moved from its menu, from one place to another.
    Moved(usize, usize),
}

fn local(event: Event) -> Message {
    message(SettingsEvent::RoleLocal(event))
}

fn input(input: RoleInput) -> Message {
    message(SettingsEvent::Role(input))
}

/// The pictures Roles shows: role icons, the members' avatars.
pub fn pictures(page: &RolesPage) -> Vec<String> {
    let mut urls: Vec<String> = page
        .listed()
        .iter()
        .filter_map(|role| role.icon.clone())
        .collect();
    urls.extend(page.open_role().and_then(|role| role.icon.clone()));
    urls.extend(
        page.shown_members()
            .iter()
            .map(|member| member.avatar.clone()),
    );
    if let Some(adding) = &page.adding {
        urls.extend(adding.found.iter().map(|member| member.avatar.clone()));
        urls.extend(adding.chosen.iter().map(|member| member.avatar.clone()));
    }
    urls.retain(|url| !url.is_empty());
    urls
}

/// A role's colour as Roles shows it: Discord's grey for none.
pub(crate) fn role_colour(colour: u32) -> Color {
    hex(if colour == 0 {
        DEFAULT_ROLE_COLOR
    } else {
        colour
    })
}

/// A role's lock in Discord's words.
pub(crate) fn lock_text(lock: Option<RoleLock>) -> &'static str {
    match lock {
        Some(RoleLock::Highest) => {
            "Role is locked because it is your highest ranked role. Please ask a higher rank or Server Owner for help."
        }
        Some(RoleLock::Higher) => {
            "Role is locked because it is a higher rank than your highest role."
        }
        None => "",
    }
}

/// A search field of the page's, as the Slint client's 14 px `TextField`,
/// 40 px tall.
fn search<'a>(
    theme: &crate::theme::Theme,
    id: &'static str,
    placeholder: &'a str,
    value: &'a str,
    on: impl Fn(String) -> RoleInput + 'a,
) -> Element<'a, Message> {
    controls::field(theme, id, placeholder, value, false, true)
        .size(14)
        .padding([10.9, 12.0])
        .on_input(move |text| input(on(text)))
        .into()
}

/// The open Roles, if it's showing.
fn roles(state: &yapfast_core::application::AppState) -> Option<&RolesPage> {
    match state.server_settings.as_ref()?.page.as_ref()? {
        SettingsPage::Roles(page) => Some(&**page),
        _ => None,
    }
}

impl super::Screen {
    /// What's done on Roles that's the screen's own.
    pub(super) fn roles_local(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let state = cx.state();
        let open = roles(state).and_then(|page| page.open.as_ref().map(|draft| draft.id.clone()));
        let moved = match &event {
            Event::Moved(from, _) => {
                roles(state).and_then(|page| page.listed().get(*from).map(|role| role.id.clone()))
            }
            _ => None,
        };
        let local = &mut self.roles;
        let action = match event {
            Event::More(role) => {
                local.more = if local.more == role { None } else { role };
                return Task::none();
            }
            Event::Hex(text) => {
                local.hex = open.map(|role| (role, text));
                return Task::none();
            }
            Event::ApplyHex => {
                let Some(colour) = local.hex.take().and_then(|(_, text)| hex_colour(&text)) else {
                    return Task::none();
                };
                RoleInput::Color(colour)
            }
            Event::Emoji(text) => {
                local.emoji = text;
                return Task::none();
            }
            Event::UseEmoji => {
                let emoji = std::mem::take(&mut local.emoji);
                if emoji.trim().is_empty() {
                    return Task::none();
                }
                RoleInput::Emoji(emoji)
            }
            Event::Moved(_, to) => {
                local.more = None;
                let Some(role) = moved else {
                    return Task::none();
                };
                RoleInput::Move { role, to }
            }
        };
        self.update(SettingsEvent::Role(action), cx)
    }
}

impl<'a> View<'a> {
    /// Roles: the list, or the editor of the open role. Its Permissions
    /// tab fills the frame's height, scrolling its own list.
    pub(super) fn roles_page(
        self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        screen: &'a Screen,
    ) -> (Element<'a, Message>, bool) {
        let rules = self.cx.app.role_rules(&core.server);
        match (&page.open, page.open_role()) {
            (Some(draft), Some(role)) => (
                self.role_editor(core, page, role, &rules, screen),
                draft.tab == RoleTab::Permissions,
            ),
            _ => (self.roles_list(core, page, &rules, screen), false),
        }
    }
}

impl<'a> View<'a> {
    /// Delete Role's or Remove member's question, and Add members, over
    /// the settings.
    pub(super) fn roles_dialogs(
        self,
        page: &'a RolesPage,
        _screen: &'a Screen,
    ) -> Vec<Element<'a, Message>> {
        let mut dialogs = Vec::new();
        if let Some(confirm) = &page.confirm {
            dialogs.push(self.role_confirm(page, confirm));
        }
        if page.adding.is_some() {
            dialogs.push(self.add_members(page));
        }
        dialogs
    }

    /// What Roles asks before deleting a role or taking it from someone.
    fn role_confirm(self, page: &'a RolesPage, confirm: &'a RoleConfirm) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (title, question, answer) = match confirm {
            RoleConfirm::Delete(id) => {
                let name = page.role(id).map_or("", |role| role.name.as_str());
                (
                    "Delete Role",
                    format!(
                        "Are you sure you want to delete the {name} role? This action cannot be undone."
                    ),
                    "Delete",
                )
            }
            RoleConfirm::RemoveMember(member) => {
                let role = page.open_role().map_or("", |role| role.name.as_str());
                (
                    "Remove member",
                    format!("Remove {} from role {role}?", member.name),
                    "Remove",
                )
            }
        };
        let body = column![
            text(title).size(20).font(BOLD).color(theme.text),
            text(question).size(15).color(theme.secondary),
            row![
                Space::new().width(Length::Fill),
                action(theme, "Cancel", Kind::Ghost, Some(input(RoleInput::Cancel))),
                action(theme, answer, Kind::Danger, Some(input(RoleInput::Confirm))),
            ]
            .spacing(8)
        ]
        .spacing(12)
        .padding(16);
        crate::dialog::panel(theme, body.into(), DIALOG, None, input(RoleInput::Cancel))
    }
}
