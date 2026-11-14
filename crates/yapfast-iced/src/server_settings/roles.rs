//! Roles, as the Slint client's `RolesSection` (`ui/roles.slint`) with its
//! adapter's rules: the roles in their order, which dragging a row by its
//! handle (or its More menu's Move Up and Move Down) changes, and the
//! editor of one, with its Display, Permissions and Manage Members tabs;
//! Delete Role's and Remove member's question, and Add members, over the
//! settings. What may change is the core's (`RoleRules`): a role at or
//! above this account's highest is locked, with Discord's words why, as is
//! a permission it doesn't hold or would lose.

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, scrollable, text, tooltip},
};
use yapfast_core::{
    application::{
        ADD_MEMBERS_LIMIT, DEFAULT_ROLE_COLOR, PermissionLock, PictureEdit, ROLE_COLORS,
        ROLE_PERMISSIONS, RoleConfirm, RoleInput, RoleLock, RoleRules, RoleTab, RolesPage,
        ServerSettings, SettingsPage,
    },
    chat::{RoleDetail, RoleManager, RoleMember, ServerPicture},
};

use super::{Event as SettingsEvent, Screen, message};
use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Kind, Light, action, lights},
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    popover::{Popover, Side, Width},
    reorder::Reorder,
    style::{faded, hex, rounded, scrollbar},
    widgets::{Hover, Pic},
};

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

/// A colour written as hex, "#rrggbb" or without its "#".
fn hex_colour(text: &str) -> Option<u32> {
    let hex = text.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
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
fn roles(app: &App) -> Option<&RolesPage> {
    match app.client.state().server_settings.as_ref()?.page.as_ref()? {
        SettingsPage::Roles(page) => Some(page),
        _ => None,
    }
}

impl App {
    /// What's done on Roles that's the screen's own.
    pub(super) fn roles_local(&mut self, event: Event) -> Task<Message> {
        let open = roles(self).and_then(|page| page.open.as_ref().map(|draft| draft.id.clone()));
        let moved = match &event {
            Event::Moved(from, _) => {
                roles(self).and_then(|page| page.listed().get(*from).map(|role| role.id.clone()))
            }
            _ => None,
        };
        let Some(screen) = self.server_screen.as_mut() else {
            return Task::none();
        };
        let local = &mut screen.roles;
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
        self.server_settings_event(SettingsEvent::Role(action))
    }

    /// Roles: the list, or the editor of the open role. Its Permissions
    /// tab fills the frame's height, scrolling its own list.
    pub(super) fn roles_page<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        screen: &'a Screen,
    ) -> (Element<'a, Message>, bool) {
        let rules = self.client.app.role_rules(&core.server);
        match (&page.open, page.open_role()) {
            (Some(draft), Some(role)) => (
                self.role_editor(core, page, role, &rules, screen),
                draft.tab == RoleTab::Permissions,
            ),
            _ => (self.roles_list(core, page, &rules, screen), false),
        }
    }

    /// The list: @everyone's card, the search and Create Role, and the
    /// roles in their order.
    fn roles_list<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        rules: &RoleRules,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let ready = !core.saving;
        let listed = page.listed();
        let may_drag = page.search.is_empty() && ready;
        let first_movable = listed
            .iter()
            .take_while(|role| rules.lock(&role.id, role.position).is_some())
            .count();
        let header = row![
            Space::new().width(HANDLE),
            container(form::heading(theme, format!("ROLES – {}", listed.len())))
                .width(Length::Fill),
            container(form::heading(theme, "MEMBERS")).width(MEMBERS + 2.0 * BUTTON + HANDLE),
        ]
        .spacing(10)
        .padding(Padding {
            left: 4.0,
            right: 8.0,
            bottom: 6.0,
            top: 0.0,
        });
        let mut list = column![header, form::rule(theme)];
        if listed.is_empty() {
            list = list.push(form::note(theme, "No roles"));
        }
        let count = listed.len();
        let mut rows = Vec::with_capacity(count);
        let mut movable = Vec::with_capacity(count);
        for (index, role) in listed.iter().enumerate() {
            let lock = rules.lock(&role.id, role.position);
            let can_move = may_drag && lock.is_none();
            movable.push(can_move);
            rows.push(self.role_row(
                role,
                lock,
                (index, count, first_movable),
                can_move,
                screen.roles.more.as_deref() == Some(role.id.as_str()),
            ));
        }
        list = list.push(
            Reorder::new(rows, movable, HANDLE + 4.0, |from, to| {
                local(Event::Moved(from, to))
            })
            .lifted(theme.raised, 6.0),
        );
        form::page([
            form::title(theme, "Roles"),
            form::note(
                theme,
                "Use roles to group your server members and assign permissions.",
            ),
            self.everyone_card(page.everyone()),
            row![
                search(
                    theme,
                    SEARCH,
                    "Search Roles",
                    &page.search,
                    RoleInput::Search
                ),
                action(
                    theme,
                    "Create Role",
                    Kind::Primary,
                    ready.then(|| input(RoleInput::Create)),
                )
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
            form::note(
                theme,
                "Members use the color of the highest role they have on this list. Drag roles to reorder them.",
            ),
            list.into(),
        ])
    }

    /// @everyone's card, which opens its permissions.
    fn everyone_card<'a>(&'a self, everyone: Option<&'a RoleDetail>) -> Element<'a, Message> {
        let theme = &self.theme;
        let name = everyone.map_or("@everyone", |role| role.name.as_str());
        let field = theme.field;
        let (raised, lit) = (
            theme.raised,
            crate::theme::mix(theme.raised, theme.overlay, 0.9),
        );
        let face = row![
            container(self.glyph(Icon::Users, 20, theme.secondary))
                .center(CARD_ICON)
                .style(move |_| rounded(field, CARD_ICON / 2.0)),
            column![
                text("Default Permissions")
                    .size(15)
                    .font(SEMIBOLD)
                    .color(theme.text),
                text(format!("{name} • applies to all server members"))
                    .size(13)
                    .color(theme.secondary)
            ]
            .spacing(2)
            .width(Length::Fill),
            self.glyph(Icon::ChevronRight, 18, theme.secondary),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 16])
        .width(Length::Fill)
        .height(CARD)
        .on_press_maybe(everyone.map(|role| input(RoleInput::Open(Some(role.id.clone())))))
        .style(controls::lit_style(Some(raised), lit, 8.0))
        .into()
    }

    /// A role in the list: its handle, mark, lock, name and badge, how many
    /// hold it, and Edit (or View) and More. `place` is its place, how many
    /// there are, and the first that may move.
    fn role_row<'a>(
        &'a self,
        role: &'a RoleDetail,
        lock: Option<RoleLock>,
        place: (usize, usize, usize),
        movable: bool,
        more_open: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut line = row![self.handle(movable), self.role_mark(role)]
            .spacing(10)
            .align_y(Alignment::Center);
        if lock.is_some() {
            line = line.push(tooltip(
                self.glyph(Icon::Lock, 14, theme.muted),
                controls::tip(theme, lock_text(lock)),
                tooltip::Position::Top,
            ));
        }
        let mut name = row![
            text(role.name.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let badge = match role.managed {
            Some(RoleManager::Bot) => Some("BOT"),
            Some(RoleManager::Booster) => Some("BOOSTER"),
            Some(RoleManager::Integration) => Some("INTEGRATION"),
            None => None,
        };
        if let Some(badge) = badge {
            let accent = theme.accent;
            name = name.push(
                container(text(badge).size(10).font(BOLD).color(theme.on_accent))
                    .padding([0, 4])
                    .height(16)
                    .align_y(Alignment::Center)
                    .style(move |_| rounded(accent, 3)),
            );
        }
        let may_copy = lock.is_none() && role.managed.is_none();
        let open = input(RoleInput::Open(Some(role.id.clone())));
        let edit = self.filled_icon_button(
            if lock.is_some() {
                Icon::Eye
            } else {
                Icon::Pencil
            },
            if lock.is_some() { "View" } else { "Edit" },
            BUTTON,
            GLYPH,
            theme.raised,
            BUTTON / 2.0,
            Some(open.clone()),
        );
        let more = self.filled_icon_button(
            Icon::Ellipsis,
            // Its tip would cover the menu it opens.
            if more_open { "" } else { "More" },
            BUTTON,
            GLYPH,
            // Greyed as the Slint client's, which fades the whole button.
            if may_copy {
                theme.raised
            } else {
                faded(theme.raised, DISABLED)
            },
            BUTTON / 2.0,
            may_copy.then(|| local(Event::More(Some(role.id.clone())))),
        );
        let menu = more_open.then(|| self.role_menu(role, place, movable));
        let more = Popover::new(more, menu, Side::Below)
            .width(Width::Own(MORE))
            .right()
            .gap(4.0)
            .outside(local(Event::More(None)));
        line = line.push(container(name).width(Length::Fill).clip(true));
        line = line.push(
            container(
                row![
                    text(role.members.to_string())
                        .size(14)
                        .color(theme.secondary),
                    self.glyph(Icon::User, 16, theme.secondary)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .width(MEMBERS),
        );
        line = line.push(edit).push(more);
        let hover = theme.wash(0.05);
        button(
            container(line)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding(Padding {
            left: 4.0,
            right: 8.0,
            ..Padding::ZERO
        })
        .width(Length::Fill)
        .height(ROW)
        .on_press(open)
        .style(controls::lit_style(None, hover, 6.0))
        .into()
    }

    /// A row's handle to drag it by: two columns of three dots, brighter
    /// under the pointer; nothing for a role that can't move.
    fn handle(&self, movable: bool) -> Element<'_, Message> {
        if !movable {
            return Space::new().width(HANDLE).into();
        }
        let dots = |ink: Color| {
            let dot = move || {
                container(Space::new())
                    .width(HANDLE_DOT)
                    .height(HANDLE_DOT)
                    .style(move |_| rounded(ink, HANDLE_DOT / 2.0))
            };
            let three = || column![dot(), dot(), dot()].spacing(HANDLE_DOT);
            container(row![three(), three()].spacing(HANDLE_DOT))
                .padding(Padding {
                    left: 2.0 * HANDLE_DOT,
                    ..Padding::ZERO
                })
                .width(HANDLE)
                .height(ROW)
                .align_y(Alignment::Center)
        };
        let muted = self.theme.muted;
        Hover::new(dots(faded(muted, 0.5)), dots(muted)).into()
    }

    /// A role's mark: its icon, its emoji, or its colour's dot.
    fn role_mark(&self, role: &RoleDetail) -> Element<'_, Message> {
        if let Some(handle) = role
            .icon
            .as_deref()
            .and_then(|url| self.picture(url, MARK as u32, MARK as u32, true))
        {
            return Pic::square(handle, MARK).into();
        }
        if let Some(emoji) = &role.unicode_emoji {
            return container(text(emoji.clone()).size(16)).center(MARK).into();
        }
        let colour = role_colour(role.color);
        container(
            container(Space::new())
                .width(DOT)
                .height(DOT)
                .style(move |_| rounded(colour, DOT / 2.0)),
        )
        .center(MARK)
        .into()
    }

    /// A role's More menu: Move Up, Move Down, Duplicate Role, Delete.
    fn role_menu<'a>(
        &'a self,
        role: &'a RoleDetail,
        (index, count, first_movable): (usize, usize, usize),
        movable: bool,
    ) -> Element<'a, Message> {
        let item = |label: &'static str, danger: bool, enabled: bool| Item {
            label: label.into(),
            icon: None,
            danger,
            slot: false,
            disabled: !enabled,
            subtitle: None,
            check: Check::None,
        };
        let id = || role.id.clone();
        let rows = column![
            self.menu_item(
                item("Move Up", false, movable && index > first_movable),
                Light::Pointer,
                local(Event::Moved(index, index.saturating_sub(1))),
            ),
            self.menu_item(
                item("Move Down", false, movable && index + 1 < count),
                Light::Pointer,
                local(Event::Moved(index, index + 1)),
            ),
            self.menu_item(
                item("Duplicate Role", false, true),
                Light::Pointer,
                input(RoleInput::Duplicate(id())),
            ),
            self.menu_item(
                item("Delete", true, true),
                Light::Pointer,
                input(RoleInput::Delete(id())),
            ),
        ]
        .spacing(2);
        iced::widget::opaque(
            container(rows)
                .padding(6)
                .width(MORE)
                .style(controls::menu_panel(&self.theme)),
        )
    }

    /// The editor: Back, its title, why it's locked or what manages it,
    /// the tabs and the open tab.
    fn role_editor<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        rules: &RoleRules,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let lock = rules.lock(&role.id, role.position);
        let everyone = page.is_everyone(&role.id);
        let ready = !core.saving;
        let editable = ready && lock.is_none();
        let (secondary, ink) = (theme.secondary, theme.text);
        let back = button(
            container(
                row![
                    self.glyph(Icon::ChevronLeft, 16, theme.secondary),
                    text("Back").size(14).font(SEMIBOLD)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .height(Length::Fill)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .height(28)
        .on_press(input(RoleInput::Open(None)))
        .style(controls::ink_style(secondary, ink));
        let title = if everyone {
            "Default Permissions".to_owned()
        } else {
            format!("Edit Role — {}", role.name)
        };
        let notice = match (lock, role.managed) {
            (Some(_), _) => lock_text(lock),
            (None, Some(RoleManager::Booster)) => {
                "This role is automatically managed by Discord for Server Boosting. It cannot be manually assigned to members or deleted."
            }
            (None, Some(_)) => "This role is automatically managed by an integration.",
            (None, None) => "",
        };
        let mut parts: Vec<Element<'a, Message>> = vec![back.into(), form::title(theme, title)];
        if !notice.is_empty() {
            let raised = theme.raised;
            parts.push(
                container(text(notice).size(14).color(theme.secondary))
                    .padding([10, 12])
                    .width(Length::Fill)
                    .style(move |_| rounded(raised, 6))
                    .into(),
            );
        }
        let tab = |label: String, which: RoleTab, enabled: bool| {
            self.role_tab(
                label,
                draft.tab == which,
                enabled.then(|| input(RoleInput::Tab(which))),
            )
        };
        parts.push(
            row![
                tab("Display".into(), RoleTab::Display, !everyone),
                tab("Permissions".into(), RoleTab::Permissions, true),
                tab(
                    format!("Manage Members ({})", role.members),
                    RoleTab::Members,
                    !everyone
                ),
            ]
            .spacing(24)
            .into(),
        );
        parts.push(form::rule(theme));
        parts.push(match draft.tab {
            RoleTab::Display => self.role_display(core, page, role, rules, editable, screen),
            RoleTab::Permissions => {
                self.role_permissions(page, role, rules, editable, lock.is_some(), notice)
            }
            RoleTab::Members => self.role_members(page, role, lock, everyone, ready),
        });
        column(parts)
            .spacing(form::GAP)
            .width(Length::Fill)
            .height(if draft.tab == RoleTab::Permissions {
                Length::Fill
            } else {
                Length::Shrink
            })
            .into()
    }

    /// A tab of the editor: its name, with the accent's line under it while
    /// it's open; greyed without a message.
    fn role_tab<'a>(
        &self,
        label: String,
        open: bool,
        press: Option<Message>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let enabled = press.is_some();
        let (faint, ink, rest, accent) = (theme.faint, theme.text, theme.secondary, theme.accent);
        // The line takes the tab's width: a stack is as wide as its first
        // layer, its name.
        button(iced::widget::stack![
            container(text(label).size(15).font(SEMIBOLD))
                .padding(Padding {
                    top: 6.0,
                    left: 4.0,
                    right: 4.0,
                    ..Padding::ZERO
                })
                .height(TAB),
            container(controls::bar(
                if open { accent } else { Color::TRANSPARENT },
                Length::Fill,
                UNDERLINE
            ))
            .height(Length::Fill)
            .align_bottom(Length::Fill),
        ])
        .padding(0)
        .height(TAB)
        .on_press_maybe(press)
        .style(move |_, status| button::Style {
            text_color: if !enabled {
                faint
            } else if open || lights(status) {
                ink
            } else {
                rest
            },
            ..Default::default()
        })
        .into()
    }

    /// Display: the name, the colour, the icon and the two switches.
    fn role_display<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        rules: &RoleRules,
        editable: bool,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let name = form::entry(theme, NAME, "", &draft.name)
            .on_input_maybe(editable.then_some(|text| input(RoleInput::Name(text))));
        column![
            form::group([form::heading(theme, "ROLE NAME"), name.into()]),
            form::group([
                form::heading(theme, "ROLE COLOR"),
                form::note(
                    theme,
                    "Members use the color of the highest role they have on the roles list."
                ),
                self.role_colours(draft.color, &draft.id, editable, screen),
            ]),
            self.role_icon(core, page, role, rules, editable, screen),
            form::switch(
                "Display role members separately from online members",
                draft.hoist,
                editable.then(|| input(RoleInput::Hoist)),
            )
            .view(self),
            form::switch(
                "Allow anyone to @mention this role",
                draft.mentionable,
                editable.then(|| input(RoleInput::Mentionable)),
            )
            .description(
                "Note: Members with the \"Mention @everyone, @here, and All Roles\" permission will always be able to ping this role.",
            )
            .view(self),
        ]
        .spacing(20)
        .into()
    }

    /// The colours: Discord's default, its colours in a bright row and a
    /// dark one, and any other as hex, applied on Enter.
    fn role_colours<'a>(
        &'a self,
        colour: u32,
        role: &str,
        editable: bool,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let is_default = colour == 0 || colour == DEFAULT_ROLE_COLOR;
        let swatch = |ground: Color,
                      (width, height): (f32, f32),
                      chosen: bool,
                      tick: u32,
                      press: Option<Message>|
         -> Element<'a, Message> {
            let face: Element<'a, Message> = if chosen {
                self.glyph(Icon::Check, tick, Color::WHITE).into()
            } else {
                Space::new().into()
            };
            button(container(face).center_x(width).center_y(height))
                .padding(0)
                .width(width)
                .height(height)
                .on_press_maybe(press)
                .style(move |_, _| button::Style {
                    background: Some(ground.into()),
                    border: Border::default().rounded(4),
                    ..Default::default()
                })
                .into()
        };
        let pick = |picked: u32| editable.then(|| input(RoleInput::Color(picked)));
        let swatches = |colours: &[u32]| {
            row(colours.iter().map(|picked| {
                swatch(
                    hex(*picked),
                    (SWATCH, SWATCH),
                    colour == *picked,
                    14,
                    pick(*picked),
                )
            }))
            .spacing(SWATCH_GAP)
        };
        let custom = !is_default && !ROLE_COLORS.contains(&colour);
        let typed = screen
            .roles
            .hex
            .as_ref()
            .filter(|(id, _)| id == role)
            .map(|(_, text)| text.clone());
        let shown = typed.unwrap_or_else(|| {
            format!(
                "#{:06x}",
                if colour == 0 {
                    DEFAULT_ROLE_COLOR
                } else {
                    colour
                }
            )
        });
        let field = controls::field(theme, HEX, "#99aab5", &shown, false, editable)
            .size(14)
            .padding([9.0, 10.0])
            .width(SMALL_FIELD);
        let hex_field: Element<'a, Message> = if editable {
            field
                .on_input(|text| local(Event::Hex(text)))
                .on_submit(local(Event::ApplyHex))
                .into()
        } else {
            field.into()
        };
        row![
            swatch(
                hex(DEFAULT_ROLE_COLOR),
                DEFAULT_SWATCH,
                is_default,
                20,
                pick(DEFAULT_ROLE_COLOR)
            ),
            column![swatches(&ROLE_COLORS[..10]), swatches(&ROLE_COLORS[10..])].spacing(SWATCH_GAP),
            swatch(role_colour(colour), (CUSTOM, CUSTOM), custom, 20, None),
            hex_field,
        ]
        .spacing(SWATCH_GAP)
        .align_y(Alignment::Center)
        .into()
    }
}

impl App {
    /// The icon: the picture, the emoji or a placeholder in its box, what
    /// unlocks it, Choose Image and Remove Icon, and an emoji typed in.
    fn role_icon<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        rules: &RoleRules,
        editable: bool,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let reading = core.reading.is_some();
        let unlocked = page.original.role_icons;
        let usable = editable && page.icon_editable(rules);
        let side_px = ICON_SHOWN as u32;
        let handle = match &draft.icon {
            PictureEdit::Keep => role
                .icon
                .as_deref()
                .and_then(|url| self.picture(url, side_px, side_px, true)),
            PictureEdit::Reset => None,
            PictureEdit::New(picked) => self.fit(&picked.preview, side_px, side_px, true),
        };
        let emoji = draft.emoji.as_deref().or(match draft.icon {
            PictureEdit::Keep => role.unicode_emoji.as_deref(),
            _ => None,
        });
        let has_icon = handle.is_some() || emoji.is_some();
        let shown: Element<'a, Message> = match (handle, emoji) {
            (Some(handle), _) => Pic::square(handle, ICON_SHOWN).into(),
            (None, Some(emoji)) => text(emoji.to_owned()).size(36).into(),
            (None, None) => self.glyph(Icon::Image, 24, theme.muted).into(),
        };
        let sunken = theme.sunken;
        let mut side = column![].spacing(form::TIGHT);
        if !unlocked {
            side = side.push(form::note(theme, "Unlocks at Level 2"));
        }
        if reading {
            side = side.push(form::note(theme, "Reading the picture..."));
        }
        let mut buttons = vec![
            action(
                theme,
                "Choose Image",
                Kind::Plain,
                (usable && !reading).then(|| message(SettingsEvent::Pick(ServerPicture::RoleIcon))),
            )
            .into(),
        ];
        if has_icon {
            buttons.push(
                action(
                    theme,
                    "Remove Icon",
                    Kind::Ghost,
                    usable.then(|| message(SettingsEvent::Remove(ServerPicture::RoleIcon))),
                )
                .into(),
            );
        }
        side = side.push(form::buttons(buttons));
        let typed = screen.roles.emoji.as_str();
        let field = controls::field(theme, EMOJI, "Emoji", typed, false, usable)
            .size(14)
            .padding([9.0, 10.0])
            .width(SMALL_FIELD);
        let field: Element<'a, Message> = if usable {
            field
                .on_input(|text| local(Event::Emoji(text)))
                .on_submit(local(Event::UseEmoji))
                .into()
        } else {
            field.into()
        };
        side = side.push(form::buttons([
            field,
            action(
                theme,
                "Use Emoji",
                Kind::Ghost,
                (usable && !typed.is_empty()).then(|| local(Event::UseEmoji)),
            )
            .into(),
        ]));
        form::group([
            form::heading(theme, "ROLE ICON"),
            form::note(
                theme,
                "Upload an image under 256 KB or use an emoji. We recommend at least 64x64 pixels. Members will see the icon for their highest role if they have multiple roles.",
            ),
            row![
                container(shown)
                    .center(ICON_BOX)
                    .style(move |_| rounded(sunken, 8)),
                side
            ]
            .spacing(form::GAP)
            .align_y(Alignment::Center)
            .into(),
        ])
    }

    /// Permissions: the search, Clear permissions, and each group's
    /// switches the search finds, in a list of their own.
    fn role_permissions<'a>(
        &'a self,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        rules: &RoleRules,
        editable: bool,
        locked: bool,
        notice: &'static str,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let clearable = (0..64)
            .map(|bit| 1u64 << bit)
            .filter(|bit| draft.permissions & bit != 0)
            .all(|bit| rules.permission_lock(&role.id, bit, true).is_none());
        let may_clear = draft.permissions != 0 && clearable;
        let clear: Element<'a, Message> = action(
            theme,
            "Clear permissions",
            Kind::Ghost,
            (editable && may_clear).then(|| input(RoleInput::ClearPermissions)),
        )
        .into();
        let clear = if clearable {
            clear
        } else {
            tooltip(
                clear,
                controls::tip(
                    theme,
                    "You cannot clear permissions because it would remove one or more permissions from you.",
                ),
                tooltip::Position::Top,
            )
            .into()
        };
        let query = page.permission_search.to_lowercase();
        let mut rows = column![].padding(Padding {
            right: 12.0,
            ..Padding::ZERO
        });
        let mut any = false;
        for group in &ROLE_PERMISSIONS {
            let shown: Vec<_> = group
                .permissions
                .iter()
                .filter(|info| page.original.community || !info.community)
                .filter(|info| {
                    query.is_empty()
                        || info.title.to_lowercase().contains(&query)
                        || info.description.to_lowercase().contains(&query)
                })
                .collect();
            if shown.is_empty() {
                continue;
            }
            any = true;
            rows = rows.push(
                container(form::heading(theme, group.heading.to_uppercase())).padding(Padding {
                    top: 16.0,
                    bottom: 8.0,
                    ..Padding::ZERO
                }),
            );
            for info in shown {
                let bit = 1u64 << info.bit;
                let on = draft.permissions & bit != 0;
                let why = match rules.permission_lock(&role.id, bit, on) {
                    Some(PermissionLock::NotHeld) => {
                        "You cannot modify this permission because none of your roles have it."
                    }
                    Some(PermissionLock::WouldLose) => {
                        "You cannot modify this permission because removing it would remove it from you."
                    }
                    None => "",
                };
                let switch = form::switch(
                    info.title,
                    on,
                    (editable && why.is_empty()).then(|| input(RoleInput::Permission(bit))),
                )
                .description(info.description)
                .locked(if locked { notice } else { why });
                let mut entry = column![switch.view(self)].spacing(4);
                if let Some(warning) = info.warning {
                    entry = entry.push(text(warning).size(14).font(BOLD).color(theme.text));
                }
                entry = entry.push(form::rule(theme));
                rows = rows.push(container(entry).padding(Padding {
                    top: 4.0,
                    bottom: 12.0,
                    ..Padding::ZERO
                }));
            }
        }
        let list: Element<'a, Message> = if any {
            scrollable(rows)
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        } else {
            form::note(theme, "No permissions found")
        };
        column![
            row![
                search(
                    theme,
                    PERMISSION_SEARCH,
                    "Search permissions",
                    &page.permission_search,
                    RoleInput::PermissionSearch
                ),
                clear
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            list
        ]
        .spacing(form::GAP)
        .height(Length::Fill)
        .into()
    }

    /// Manage Members: the search, Add Members, and those who hold it,
    /// each with the button that takes it from them.
    fn role_members<'a>(
        &'a self,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        lock: Option<RoleLock>,
        everyone: bool,
        ready: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let may_add = lock.is_none() && !everyone && role.managed.is_none();
        let mut parts = column![
            row![
                search(
                    theme,
                    MEMBER_SEARCH,
                    "Search Members",
                    &draft.member_search,
                    RoleInput::MemberSearch
                ),
                action(
                    theme,
                    "Add Members",
                    Kind::Primary,
                    (ready && may_add).then(|| input(RoleInput::AddMembers)),
                )
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        let not_all = draft.member_search.is_empty()
            && matches!(&draft.members, Some(Ok(members)) if (members.len() as u64) < role.members);
        if not_all {
            parts = parts.push(form::note(
                theme,
                "Not all members are shown, use Search to find specific members",
            ));
        }
        let shown = page.shown_members();
        match &draft.members {
            None => parts = parts.push(form::note(theme, "Loading...")),
            Some(Err(error)) => {
                parts = parts.push(text(error.as_str()).size(14).color(theme.danger));
            }
            Some(Ok(_)) if shown.is_empty() => {
                parts = parts.push(form::note(theme, "No members were found."));
            }
            Some(Ok(_)) => {}
        }
        let removable = ready && may_add;
        for member in shown {
            parts = parts.push(self.role_member(member, removable));
        }
        parts.into()
    }

    /// A member on Manage Members: avatar, names, and the X that takes the
    /// role from them.
    fn role_member<'a>(&'a self, member: &'a RoleMember, removable: bool) -> Element<'a, Message> {
        row![
            self.avatar(
                Some(member.avatar.as_str()).filter(|url| !url.is_empty()),
                &member.name,
                MEMBER_AVATAR,
                MEMBER_AVATAR / 2.0,
                1.0
            ),
            self.member_names(member),
            self.filled_icon_button(
                Icon::X,
                if removable {
                    "Remove member"
                } else {
                    "Members cannot be manually added or removed from this role"
                },
                BUTTON,
                GLYPH,
                Color::TRANSPARENT,
                6.0,
                removable.then(|| input(RoleInput::RemoveMember(member.id.clone()))),
            ),
        ]
        .spacing(10)
        .padding(Padding {
            right: 4.0,
            ..Padding::ZERO
        })
        .height(MEMBER)
        .align_y(Alignment::Center)
        .into()
    }

    /// A member's name and username on one line.
    fn member_names<'a>(&self, member: &'a RoleMember) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut names = row![
            text(member.name.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(username) = &member.username {
            names = names.push(
                text(username.as_str())
                    .size(14)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        container(names).width(Length::Fill).clip(true).into()
    }

    /// Delete Role's or Remove member's question, and Add members, over
    /// the settings.
    pub(super) fn roles_dialogs<'a>(
        &'a self,
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
    fn role_confirm<'a>(
        &'a self,
        page: &'a RolesPage,
        confirm: &'a RoleConfirm,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
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
        crate::dialogs::panel(theme, body.into(), DIALOG, None, input(RoleInput::Cancel))
    }

    /// Add members: a search of the server's members, each with a box to
    /// pick them, up to thirty, then Add.
    fn add_members<'a>(&'a self, page: &'a RolesPage) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some(adding) = &page.adding else {
            return Space::new().into();
        };
        let role = page.open.as_ref().map_or("", |draft| draft.name.as_str());
        let chosen = |id: &str| adding.chosen.iter().any(|member| member.id == id);
        let extra = adding
            .chosen
            .iter()
            .filter(|member| !adding.found.iter().any(|found| found.id == member.id));
        let list = column(
            adding
                .found
                .iter()
                .chain(extra)
                .map(|member| self.candidate(member, chosen(&member.id))),
        );
        let picked = adding.chosen.len();
        let height = ADD_HEIGHT.min(self.window.height - ADD_ROOM);
        let body = column![
            text("Add members").size(20).font(BOLD).color(theme.text),
            text(format!("Select up to 30 members to add to role {role}"))
                .size(14)
                .color(theme.secondary),
            search(
                theme,
                CANDIDATES,
                "Search members",
                &adding.query,
                RoleInput::FindCandidates
            ),
            scrollable(list)
                .style(scrollbar(theme))
                .height(Length::Fill),
            row![
                Space::new().width(Length::Fill),
                action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    Some(input(RoleInput::CloseAdding))
                ),
                action(
                    theme,
                    "Add",
                    Kind::Primary,
                    (picked > 0 && picked <= ADD_MEMBERS_LIMIT)
                        .then(|| input(RoleInput::AddPicked)),
                ),
            ]
            .spacing(8)
        ]
        .spacing(12)
        .padding(16)
        .height(Length::Fill);
        crate::dialogs::panel(
            theme,
            body.into(),
            DIALOG,
            Some(height),
            input(RoleInput::CloseAdding),
        )
    }

    /// Someone in Add members: their box, avatar and names; a click picks
    /// them or puts them back.
    fn candidate<'a>(&'a self, member: &'a RoleMember, chosen: bool) -> Element<'a, Message> {
        let lit = self.theme.wash(0.06);
        let face = row![
            self.checkbox(chosen, false),
            self.avatar(
                Some(member.avatar.as_str()).filter(|url| !url.is_empty()),
                &member.name,
                CANDIDATE_AVATAR,
                CANDIDATE_AVATAR / 2.0,
                1.0
            ),
            self.member_names(member),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 6])
        .width(Length::Fill)
        .height(MEMBER)
        .on_press(input(RoleInput::Pick(member.id.clone())))
        .style(controls::lit_style(None, lit, 4.0))
        .into()
    }
}

/// Add members' search, which takes the keys as it opens.
pub fn focus_candidates() -> Task<Message> {
    operation::focus(iced::widget::Id::new(CANDIDATES))
}
