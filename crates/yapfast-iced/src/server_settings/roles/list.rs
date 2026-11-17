//! The roles in their order: @everyone's card, each role's row with its
//! handle, mark and member count, and its More menu.

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, text, tooltip},
};
use yapfast_core::{
    application::{RoleInput, RoleLock, RoleRules, RolesPage, ServerSettings},
    chat::{RoleDetail, RoleManager},
    i18n::{self, tr},
};

use super::{
    BUTTON, CARD, CARD_ICON, DISABLED, DOT, Event, GLYPH, HANDLE, HANDLE_DOT, MARK, MEMBERS, MORE,
    ROW, SEARCH, input, local, lock_text, role_colour, search,
};
use crate::server_settings::Screen;
use crate::server_settings::View;
use crate::{
    app::Message,
    controls::{self, Check, Item, Kind, Light, action},
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    popover::{Popover, Side, Width},
    reorder::Reorder,
    style::{faded, rounded},
    widgets::{Hover, Pic},
};

impl<'a> View<'a> {
    /// The list: @everyone's card, the search and Create Role, and the
    /// roles in their order.
    pub(super) fn roles_list(
        self,
        core: &'a ServerSettings,
        page: &'a RolesPage,
        rules: &RoleRules,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ready = !core.saving;
        let listed = page.listed();
        let may_drag = page.search.is_empty() && ready;
        let first_movable = listed
            .iter()
            .take_while(|role| rules.lock(&role.id, role.position).is_some())
            .count();
        let header = row![
            Space::new().width(HANDLE),
            container(form::heading(
                theme,
                i18n::fill(tr("ROLES – {}"), &[&listed.len()])
            ))
            .width(Length::Fill),
            container(form::heading(theme, tr("MEMBERS"))).width(MEMBERS + 2.0 * BUTTON + HANDLE),
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
            list = list.push(form::note(theme, tr("No roles")));
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
            form::title(theme, tr("Roles")),
            form::note(
                theme,
                tr("Use roles to group your server members and assign permissions."),
            ),
            self.everyone_card(page.everyone()),
            row![
                search(
                    theme,
                    SEARCH,
                    tr("Search Roles"),
                    &page.search,
                    RoleInput::Search
                ),
                action(
                    theme,
                    tr("Create Role"),
                    Kind::Primary,
                    ready.then(|| input(RoleInput::Create)),
                )
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
            form::note(
                theme,
                tr(
                    "Members use the color of the highest role they have on this list. Drag roles to reorder them.",
                ),
            ),
            list.into(),
        ])
    }

    /// @everyone's card, which opens its permissions.
    pub(super) fn everyone_card(self, everyone: Option<&'a RoleDetail>) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let name = everyone.map_or("@everyone", |role| role.name.as_str());
        let field = theme.field;
        let (raised, lit) = (
            theme.raised,
            crate::theme::mix(theme.raised, theme.overlay, 0.9),
        );
        let face = row![
            container(self.cx.look.glyph(Icon::Users, 20, theme.secondary))
                .center(CARD_ICON)
                .style(move |_| rounded(field, CARD_ICON / 2.0)),
            column![
                text(tr("Default Permissions"))
                    .size(15)
                    .font(SEMIBOLD)
                    .color(theme.text),
                text(i18n::fill(
                    tr("{} • applies to all server members"),
                    &[&name]
                ))
                .size(13)
                .color(theme.secondary)
            ]
            .spacing(2)
            .width(Length::Fill),
            self.cx.look.glyph(Icon::ChevronRight, 18, theme.secondary),
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
    pub(super) fn role_row(
        self,
        role: &'a RoleDetail,
        lock: Option<RoleLock>,
        place: (usize, usize, usize),
        movable: bool,
        more_open: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut line = row![self.handle(movable), self.role_mark(role)]
            .spacing(10)
            .align_y(Alignment::Center);
        if lock.is_some() {
            line = line.push(tooltip(
                self.cx.look.glyph(Icon::Lock, 14, theme.muted),
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
            Some(RoleManager::Bot) => Some(tr("BOT")),
            Some(RoleManager::Booster) => Some(tr("BOOSTER")),
            Some(RoleManager::Integration) => Some(tr("INTEGRATION")),
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
        let edit = self
            .cx
            .look
            .icon_button(
                if lock.is_some() {
                    Icon::Eye
                } else {
                    Icon::Pencil
                },
                if lock.is_some() {
                    tr("View")
                } else {
                    tr("Edit")
                },
                Some(open.clone()),
            )
            .size(BUTTON, GLYPH)
            .fill(theme.raised)
            .radius(BUTTON / 2.0);
        let more = self
            .cx
            .look
            .icon_button(
                Icon::Ellipsis, // Its tip would cover the menu it opens.
                if more_open { "" } else { tr("More") },
                may_copy.then(|| local(Event::More(Some(role.id.clone())))),
            )
            .size(BUTTON, GLYPH)
            .fill(
                // Greyed while it can't be used.
                if may_copy {
                    theme.raised
                } else {
                    faded(theme.raised, DISABLED)
                },
            )
            .radius(BUTTON / 2.0);
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
                    self.cx.look.glyph(Icon::User, 16, theme.secondary)
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
    pub(super) fn handle(self, movable: bool) -> Element<'a, Message> {
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
        let muted = self.cx.look.theme.muted;
        Hover::new(dots(faded(muted, 0.5)), dots(muted)).into()
    }

    /// A role's mark: its icon, its emoji, or its colour's dot.
    pub(super) fn role_mark(self, role: &RoleDetail) -> Element<'a, Message> {
        if let Some(handle) = role
            .icon
            .as_deref()
            .and_then(|url| self.cx.picture(url, MARK as u32, MARK as u32, true))
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
    pub(super) fn role_menu(
        self,
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
            self.cx.look.menu_item(
                item(tr("Move Up"), false, movable && index > first_movable),
                Light::Pointer,
                local(Event::Moved(index, index.saturating_sub(1))),
            ),
            self.cx.look.menu_item(
                item(tr("Move Down"), false, movable && index + 1 < count),
                Light::Pointer,
                local(Event::Moved(index, index + 1)),
            ),
            self.cx.look.menu_item(
                item(tr("Duplicate Role"), false, true),
                Light::Pointer,
                input(RoleInput::Duplicate(id())),
            ),
            self.cx.look.menu_item(
                item(tr("Delete"), true, true),
                Light::Pointer,
                input(RoleInput::Delete(id())),
            ),
        ]
        .spacing(2);
        iced::widget::opaque(
            container(rows)
                .padding(6)
                .width(MORE)
                .style(controls::menu_panel(self.cx.look.theme)),
        )
    }
}
