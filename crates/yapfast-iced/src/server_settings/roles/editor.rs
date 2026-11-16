//! One role's editor: its tabs, Display (name, colours, icon) and
//! Permissions.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, text, tooltip},
};
use yapfast_core::{
    application::{
        DEFAULT_ROLE_COLOR, PermissionLock, PictureEdit, ROLE_COLORS, ROLE_PERMISSIONS, RoleInput,
        RoleRules, RoleTab, RolesPage, ServerSettings,
    },
    chat::{RoleDetail, RoleManager, ServerPicture},
};

use super::{
    CUSTOM, DEFAULT_SWATCH, EMOJI, Event, HEX, ICON_BOX, ICON_SHOWN, NAME, PERMISSION_SEARCH,
    SMALL_FIELD, SWATCH, SWATCH_GAP, TAB, UNDERLINE, input, local, lock_text, role_colour, search,
};
use crate::server_settings::{Event as SettingsEvent, Screen, message};
use crate::{
    app::{App, Message},
    controls::{self, Kind, action, lights},
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    style::{hex, rounded, scrollbar},
    widgets::Pic,
};

/// A colour written as hex, "#rrggbb" or without its "#".
pub(super) fn hex_colour(text: &str) -> Option<u32> {
    let hex = text.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

impl App {
    /// The editor: Back, its title, why it's locked or what manages it,
    /// the tabs and the open tab.
    pub(super) fn role_editor<'a>(
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
    pub(super) fn role_tab<'a>(
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
    pub(super) fn role_display<'a>(
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
            .view(self.look()),
            form::switch(
                "Allow anyone to @mention this role",
                draft.mentionable,
                editable.then(|| input(RoleInput::Mentionable)),
            )
            .description(
                "Note: Members with the \"Mention @everyone, @here, and All Roles\" permission will always be able to ping this role.",
            )
            .view(self.look()),
        ]
        .spacing(20)
        .into()
    }

    /// The colours: Discord's default, its colours in a bright row and a
    /// dark one, and any other as hex, applied on Enter.
    pub(super) fn role_colours<'a>(
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

    /// The icon: the picture, the emoji or a placeholder in its box, what
    /// unlocks it, Choose Image and Remove Icon, and an emoji typed in.
    pub(super) fn role_icon<'a>(
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
    pub(super) fn role_permissions<'a>(
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
                let mut entry = column![switch.view(self.look())].spacing(4);
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
            scroll_view(rows)
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
}
