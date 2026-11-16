//! A Discord channel's or category's settings, as the Slint client's
//! `ChannelSettingsScreen` (`ui/channel_settings.slint`): on the settings
//! frame (`sheet.rs`), the channel's name over Overview and Permissions,
//! with Delete Channel (or Category) after a line where the channel's menu
//! offers it; Overview's name, a text channel's topic, slowmode and age
//! restriction; Permissions' sync with its category, Private Channel, and
//! the overwrites beside the chosen one's permissions, each denied, left
//! to the roles or allowed. Discord's bar saves the page's draft, and
//! leaving with changes shakes it instead.
//!
//! What it holds and may change is the core's (`ChannelSettingsScreen`,
//! opened by `Action::OpenChannelSettings` from the channel's or the
//! category's menu); the screen keeps only its own while it's open
//! ([`Screen`]: the bar's shake, whether Add's search had the keys) and
//! lets go of it when the settings close.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, text},
};
use yapfast_core::{
    application::{
        Action, ChannelInput, ChannelSection, ChannelSettingsScreen, OverwriteConfirm,
        OverwriteState, Overwrites, SLOWMODES, channel_groups, channel_permission,
    },
    chat::{ChannelKind, ChannelOverwrite, RoleMember, ServerRole},
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action},
    fonts::{BOLD, SEMIBOLD},
    form::{self, Tri},
    pictures::Icon,
    server_settings::roles::role_colour,
    sheet::{Nav, sheet},
    style::{rounded, scrollbar},
};

/// The fields' ids.
const NAME: &str = "channel-name";
const TOPIC: &str = "channel-topic";
const ADD_SEARCH: &str = "channel-add-search";
/// Slowmode's steps as Overview names them, as [`SLOWMODES`] lists them.
const SLOWMODE_NAMES: [&str; 14] = [
    "Off", "5s", "10s", "15s", "30s", "1m", "2m", "5m", "10m", "15m", "30m", "1h", "2h", "6h",
];
/// Discord's words for a permission this account can't change.
const NOT_HELD: &str = "You cannot modify this permission because none of your roles have it.";
/// Overview's groups are further apart than a form's.
const OVERVIEW_GAP: f32 = 20.0;
const STEP: f32 = 32.0;
const STEP_GLYPH: u32 = 18;
const SLOWMODE_SHOWN: f32 = 48.0;
/// Permissions' list of roles and members, a row of it, its mark.
const OVERWRITES: f32 = 180.0;
const OVERWRITE_HEAD: f32 = 28.0;
const OVERWRITE_ROW: f32 = 32.0;
const OVERWRITE_DOT: f32 = 10.0;
const ADD_BUTTON: f32 = 28.0;
const ADD_GLYPH: u32 = 16;
/// The dialogs: the question's width, and Add's size and its rows.
const CONFIRM: f32 = 440.0;
const ADD_WIDTH: f32 = 400.0;
const ADD_HEIGHT: f32 = 480.0;
const ADD_ROOM: f32 = 80.0;
const ROLE_ROW: f32 = 36.0;
const MEMBER_ROW: f32 = 40.0;
const MEMBER_AVATAR: f32 = 26.0;

/// The settings' own state while they're open.
#[derive(Default)]
pub struct Screen {
    room: String,
    shake: form::Shake,
    /// Add's search took the keys as it opened.
    adding: bool,
}

/// What's done in the settings.
#[derive(Clone, Debug)]
pub enum Event {
    Input(ChannelInput),
    /// The bar's shake steps and its red ending.
    Shake(form::ShakeStep),
}

fn shake(step: form::ShakeStep) -> Message {
    Message::ChannelSettings(Event::Shake(step))
}

fn input(input: ChannelInput) -> Message {
    Message::ChannelSettings(Event::Input(input))
}

/// The tri-state's choice for a permission's state, and back.
fn tri(state: OverwriteState) -> Tri {
    match state {
        OverwriteState::Deny => Tri::Deny,
        OverwriteState::Neutral => Tri::Pass,
        OverwriteState::Allow => Tri::Allow,
    }
}

fn overwrite_state(tri: Tri) -> OverwriteState {
    match tri {
        Tri::Deny => OverwriteState::Deny,
        Tri::Pass => OverwriteState::Neutral,
        Tri::Allow => OverwriteState::Allow,
    }
}

impl App {
    fn channel_core(&self) -> Option<&ChannelSettingsScreen> {
        self.client.state().channel_settings.as_ref()
    }

    /// Takes in the core's settings: the screen comes and goes with them,
    /// the bar shakes when leaving was refused, and Add's search takes the
    /// keys as it opens.
    pub fn channel_settings_sync(&mut self) -> Task<Message> {
        let Some(core) = self.client.state().channel_settings.as_ref() else {
            self.channel_screen = None;
            return Task::none();
        };
        let (room, nudge) = (core.room.clone(), core.nudge);
        let adding = core.permissions.adding.is_some();
        let mut tasks = vec![];
        if self
            .channel_screen
            .as_ref()
            .is_none_or(|screen| screen.room != room)
        {
            self.channel_screen = Some(Box::new(Screen {
                room,
                ..Screen::default()
            }));
            // The composer lets go of the keys under the settings.
            tasks.push(operation::focus(crate::app::NOBODY));
        }
        let Some(screen) = self.channel_screen.as_mut() else {
            return Task::none();
        };
        if screen.adding != adding {
            screen.adding = adding;
            if adding {
                tasks.push(operation::focus(iced::widget::Id::new(ADD_SEARCH)));
            }
        }
        tasks.push(screen.shake.start(nudge, shake));
        Task::batch(tasks)
    }

    /// The avatars Add's members show.
    pub fn channel_settings_pictures(&self) -> Vec<String> {
        self.channel_core()
            .and_then(|core| core.permissions.adding.as_ref())
            .into_iter()
            .flat_map(|adding| &adding.found)
            .map(|member| member.avatar.clone())
            .filter(|url| !url.is_empty())
            .collect()
    }

    pub fn channel_settings_event(&mut self, event: Event) -> Task<Message> {
        if self.channel_screen.is_none() {
            return Task::none();
        }
        match event {
            Event::Input(input) => {
                let page = matches!(input, ChannelInput::Section(_));
                self.client.dispatch(Action::ChannelSettings(input));
                let mut tasks = vec![self.sync()];
                if page {
                    tasks.push(operation::snap_to(
                        iced::widget::Id::new(crate::sheet::PAGE),
                        iced::widget::scrollable::RelativeOffset::START,
                    ));
                }
                if self.channel_screen.is_none() {
                    tasks.push(self.refocus());
                }
                Task::batch(tasks)
            }
            Event::Shake(step) => self
                .channel_screen
                .as_mut()
                .map_or_else(Task::none, |screen| screen.shake.step(step, shake)),
        }
    }

    /// Escape in the settings: Add's picker closes, then the question, then
    /// the settings (unless they have changes, which shakes the bar).
    pub fn channel_settings_escape(&mut self) -> Task<Message> {
        let Some(core) = self.channel_core() else {
            return Task::none();
        };
        let close = if core.permissions.adding.is_some() {
            ChannelInput::Adding(false)
        } else if core.permissions.confirm.is_some() {
            ChannelInput::Cancel
        } else {
            ChannelInput::Close
        };
        self.channel_settings_event(Event::Input(close))
    }

    /// The channel as its settings name it: a text channel with its `#`.
    fn channel_label(&self, core: &ChannelSettingsScreen) -> String {
        let title = self.client.app.channel_title(&core.room);
        if core.kind == ChannelKind::Text {
            format!("#{title}")
        } else {
            title
        }
    }

    /// The settings over the dimmed window, with what's open over them.
    pub fn channel_settings_view(&self) -> Option<Element<'_, Message>> {
        let core = self.channel_core()?;
        let screen = self.channel_screen.as_deref()?;
        let theme = &self.theme;
        let app = &self.client.app;
        let label = self.channel_label(core);
        let mut nav = vec![Nav::Heading(label.to_uppercase().into())];
        for section in app.channel_sections(&core.room) {
            let (words, icon) = match section {
                ChannelSection::Overview => ("Overview", Icon::Pencil),
                ChannelSection::Permissions => ("Permissions", Icon::Shield),
            };
            nav.push(Nav::Page {
                label: words.into(),
                icon,
                selected: core.section == section,
                danger: false,
                press: input(ChannelInput::Section(section)),
            });
        }
        // Delete last, after a line, as Discord's sidebar has it.
        if app.may_delete_channel(&core.room) {
            nav.push(Nav::Line);
            nav.push(Nav::Page {
                label: if core.kind == ChannelKind::Category {
                    "Delete Category"
                } else {
                    "Delete Channel"
                }
                .into(),
                icon: Icon::Trash,
                selected: false,
                danger: true,
                press: input(ChannelInput::Delete),
            });
        }
        let loading = core.section == ChannelSection::Overview
            && core.overview.is_none()
            && core.error.is_none();
        let permissions = !loading && core.section == ChannelSection::Permissions;
        let mut page = column![].spacing(form::GAP).width(Length::Fill);
        if loading {
            page = page.push(form::note(theme, "Loading..."));
        } else if permissions {
            page = page.push(self.channel_permissions(core));
        } else {
            page = page.push(self.channel_overview(core));
        }
        if let Some(problem) = core.problem() {
            page = page.push(form::error(theme, problem));
        }
        if let Some(error) = &core.error {
            page = page.push(form::error(theme, error.as_str()));
        }
        let bar = (core.changed() || core.saving).then(|| {
            form::Unsaved {
                pad: 12.0,
                radius: crate::sheet::BAR_RADIUS,
                saving: core.saving,
                may_reset: true,
                may_save: core.ready() && app.state.discord_online,
                shake: screen.shake,
                undo: input(ChannelInput::Undo),
                save: input(ChannelInput::Save),
            }
            .view(theme)
        });
        let frame = sheet(nav, page, input(ChannelInput::Close), "Channel settings")
            .bar(bar)
            .fill(permissions)
            .view(self.look());
        let mut layers = iced::widget::stack![frame];
        if let Some(confirm) = &core.permissions.confirm {
            layers = layers.push(self.overwrite_confirm(core, confirm, label));
        }
        if core.permissions.adding.is_some() {
            layers = layers.push(self.add_overwrite(core));
        }
        Some(layers.into())
    }

    /// Overview: the name, and a text channel's topic, slowmode and age
    /// restriction.
    fn channel_overview<'a>(&'a self, core: &'a ChannelSettingsScreen) -> Element<'a, Message> {
        let theme = &self.theme;
        let Some((_, draft)) = &core.overview else {
            return Space::new().into();
        };
        let ready = !core.saving;
        let category = core.kind == ChannelKind::Category;
        let mut parts = vec![
            form::title(theme, "Overview"),
            form::group([
                form::heading(
                    theme,
                    if category {
                        "CATEGORY NAME"
                    } else {
                        "CHANNEL NAME"
                    },
                ),
                form::entry(theme, NAME, "", &draft.name)
                    .on_input_maybe(ready.then_some(|text| input(ChannelInput::Name(text))))
                    .into(),
            ]),
        ];
        if core.kind == ChannelKind::Text {
            let step = SLOWMODES
                .iter()
                .position(|seconds| *seconds == draft.slowmode)
                .unwrap_or(0);
            let to = |index: usize| input(ChannelInput::Slowmode(SLOWMODES[index]));
            parts.extend([
                form::group([
                    form::heading(theme, "CHANNEL TOPIC"),
                    form::entry(
                        theme,
                        TOPIC,
                        "Let everyone know how to use this channel!",
                        &draft.topic,
                    )
                    .on_input_maybe(ready.then_some(|text| input(ChannelInput::Topic(text))))
                    .into(),
                ]),
                form::group([
                    form::heading(theme, "SLOWMODE"),
                    row![
                        self.look().icon_button(Icon::ChevronLeft, "Shorter slowmode", Some(to(step.saturating_sub(1)))).size(STEP, STEP_GLYPH).fill(iced::Color::TRANSPARENT),
                        container(
                            text(SLOWMODE_NAMES[step.min(SLOWMODE_NAMES.len() - 1)])
                                .size(15)
                                .color(theme.text)
                        )
                        .center_x(SLOWMODE_SHOWN),
                        self.look().icon_button(Icon::ChevronRight, "Longer slowmode", Some(to((step + 1).min(SLOWMODES.len() - 1)))).size(STEP, STEP_GLYPH).fill(iced::Color::TRANSPARENT),
                    ]
                    .spacing(form::TIGHT)
                    .align_y(Alignment::Center)
                    .into(),
                    form::note(
                        theme,
                        "Members will be restricted to sending one message and creating one thread per this interval, unless they have Bypass Slowmode, Manage Channel or Manage Messages permissions.",
                    ),
                ]),
                form::switch(
                    "Age-Restricted Channel",
                    draft.nsfw,
                    ready.then(|| input(ChannelInput::Nsfw(!draft.nsfw))),
                )
                .description(
                    "Users will need to confirm they are of over legal age to view the content in this channel. Age-restricted channels are exempt from the explicit content filter.",
                )
                .view(self.look()),
            ]);
        }
        column(parts)
            .spacing(OVERVIEW_GAP)
            .width(Length::Fill)
            .into()
    }

    /// Permissions: the sync notice, Private Channel, and the overwrites
    /// beside the shown one's permissions.
    fn channel_permissions<'a>(&'a self, core: &'a ChannelSettingsScreen) -> Element<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let room = &core.room;
        let page = &core.permissions;
        let rules = app.overwrite_rules(room);
        let category = core.kind == ChannelKind::Category;
        let voice = core.kind == ChannelKind::Voice;
        let may_edit = app
            .channel_sections(room)
            .contains(&ChannelSection::Permissions);
        let ready = may_edit && !core.saving;
        let mut parts = column![
            form::title(
                theme,
                if category {
                    "Category Settings"
                } else {
                    "Channel Permissions"
                }
            ),
            form::note(
                theme,
                if category {
                    "Use permissions to customize who can do what in this category."
                } else {
                    "Use permissions to customize who can do what in this channel."
                }
            ),
        ]
        .spacing(form::GAP)
        .height(Length::Fill);
        if !category && let Some((_, name)) = app.sync_category(room) {
            let synced = app.synced(room);
            let mut notice = row![
                text(format!(
                    "{}{name}",
                    if synced {
                        "Permissions synced with category: "
                    } else {
                        "Permissions not synced with category: "
                    }
                ))
                .size(14)
                .color(theme.secondary)
                .width(Length::Fill)
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            if !synced {
                notice = notice.push(action(
                    theme,
                    "Sync Now",
                    Kind::Primary,
                    ready.then(|| input(ChannelInput::Sync)),
                ));
            }
            let raised = theme.raised;
            parts = parts.push(
                container(notice)
                    .padding(Padding {
                        top: 10.0,
                        right: 10.0,
                        bottom: 10.0,
                        left: 12.0,
                    })
                    .width(Length::Fill)
                    .style(move |_| rounded(raised, 6)),
            );
        }
        // Discord's private channel takes these from @everyone.
        let access = if voice {
            (1 << 10) | (1 << 20)
        } else {
            1 << 10
        };
        let private = page.private(access);
        let locked = (0..64).any(|bit| access & (1u64 << bit) != 0 && rules.locked(bit));
        let mut switch = form::switch(
            if category {
                "Private Category"
            } else {
                "Private Channel"
            },
            private,
            (ready && !locked).then(|| input(ChannelInput::Private(!private))),
        )
        .description(if category {
            "By making a category private, only select members and roles will be able to view this category. Linked channels in this category will automatically match to this setting."
        } else if voice {
            "By making a channel private, only select members and roles will have access to view or connect to this channel."
        } else {
            "By making a channel private, only select members and roles will be able to view this channel."
        });
        if locked {
            switch = switch.locked(NOT_HELD);
        }
        let raised = theme.raised;
        parts = parts
            .push(
                container(switch.view(self.look()))
                    .padding(14)
                    .width(Length::Fill)
                    .style(move |_| rounded(raised, 8)),
            )
            .push(form::heading(theme, "ADVANCED PERMISSIONS"));
        let shown = page.shown();
        let everyone = page.is_everyone(&shown);
        let set = move |bit: u32| {
            move |choice: Tri| input(ChannelInput::Set(bit, overwrite_state(choice)))
        };
        let mut list = column![];
        for (heading, bits) in channel_groups(core.kind) {
            list = list.push(
                container(form::heading(theme, heading.to_uppercase())).padding(Padding {
                    top: 12.0,
                    ..Padding::ZERO
                }),
            );
            for &bit in *bits {
                let (title, description) = channel_permission(bit, core.kind, everyone);
                let chose = set(bit);
                let on: Option<&dyn Fn(Tri) -> Message> = ready.then_some(&chose);
                list = list.push(
                    container(form::tri_line(
                        self.look(),
                        title,
                        description,
                        tri(Overwrites::state(&shown, bit)),
                        rules.locked(bit).then_some(NOT_HELD),
                        on,
                    ))
                    .padding(Padding {
                        top: 4.0,
                        ..Padding::ZERO
                    }),
                );
            }
        }
        let mut right = column![
            scroll_view(list)
                .style(scrollbar(theme))
                .height(Length::Fill)
        ]
        .spacing(form::TIGHT)
        .width(Length::Fill);
        if !everyone {
            right = right.push(action(
                theme,
                format!("Remove {}", app.overwrite_name(room, &shown)),
                Kind::Danger,
                ready.then(|| input(ChannelInput::Remove)),
            ));
        }
        parts
            .push(
                row![self.overwrite_list(core, ready).width(OVERWRITES), right]
                    .spacing(form::GAP)
                    .height(Length::Fill),
            )
            .into()
    }

    /// The roles and members with an overwrite, the shown one lit, under
    /// their heading and Add.
    fn overwrite_list<'a>(
        &'a self,
        core: &'a ChannelSettingsScreen,
        ready: bool,
    ) -> iced::widget::Column<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let page = &core.permissions;
        let roles = app.server_roles(&core.room);
        let head = row![
            text("ROLES/MEMBERS")
                .size(12)
                .font(BOLD)
                .color(theme.muted)
                .width(Length::Fill),
            self.look()
                .icon_button(
                    Icon::CirclePlus,
                    "Add a role or member",
                    ready.then(|| input(ChannelInput::Adding(true)))
                )
                .size(ADD_BUTTON, ADD_GLYPH)
                .fill(iced::Color::TRANSPARENT)
        ]
        .height(OVERWRITE_HEAD)
        .align_y(Alignment::Center);
        let rows = column(app.listed_overwrites().into_iter().map(|overwrite| {
            let name = app.overwrite_name(&core.room, &overwrite);
            let colour = if page.is_everyone(&overwrite) {
                None
            } else {
                role_of(&roles, &overwrite.id).and_then(|role| role.color)
            };
            self.overwrite_row(&overwrite, name, colour, overwrite.id == page.selected)
        }));
        column![
            head,
            scroll_view(rows)
                .style(scrollbar(theme))
                .height(Length::Fill)
        ]
        .spacing(4)
    }

    /// A role's or member's row: its colour's dot or a person, its name.
    fn overwrite_row<'a>(
        &'a self,
        overwrite: &ChannelOverwrite,
        name: String,
        colour: Option<u32>,
        selected: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mark: Element<'a, Message> = if overwrite.member {
            self.glyph(Icon::User, 14, theme.muted).into()
        } else {
            dot(role_colour(colour.unwrap_or(0)))
        };
        let (active, hover) = (theme.active, theme.hover);
        button(
            container(
                row![
                    mark,
                    text(name)
                        .size(14)
                        .color(if selected {
                            theme.text
                        } else {
                            theme.secondary
                        })
                        .wrapping(text::Wrapping::None)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true),
        )
        .padding([0, 8])
        .width(Length::Fill)
        .height(OVERWRITE_ROW)
        .on_press(input(ChannelInput::Select(overwrite.id.clone())))
        .style(controls::lit_style(
            selected.then_some(active),
            if selected { active } else { hover },
            4.0,
        ))
        .into()
    }

    /// What Permissions asks before removing an overwrite or syncing, or
    /// tells when a change would take a permission from this account.
    fn overwrite_confirm<'a>(
        &'a self,
        core: &'a ChannelSettingsScreen,
        confirm: &'a OverwriteConfirm,
        channel: String,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let category = app
            .sync_category(&core.room)
            .map(|(_, name)| name)
            .unwrap_or_default();
        let (title, question) = match confirm {
            OverwriteConfirm::Remove(id) => {
                let listed = app.listed_overwrites();
                let overwrite = listed
                    .iter()
                    .find(|known| known.id == *id)
                    .cloned()
                    .unwrap_or_else(|| ChannelOverwrite {
                        id: id.clone(),
                        ..Default::default()
                    });
                (
                    "Delete Permission Settings",
                    format!(
                        "Are you sure you want to delete {} permissions? This action cannot be undone.",
                        app.overwrite_name(&core.room, &overwrite)
                    ),
                )
            }
            OverwriteConfirm::Sync => (
                "Sync permissions?",
                format!("Would you like to sync {channel} permissions with {category}?"),
            ),
            OverwriteConfirm::DenySelf(name) => (
                "Pretty sure you don't want to do this",
                format!(
                    "You cannot deny this permission to {name} because it will deny it to you as well. Allow another role or yourself before trying again."
                ),
            ),
        };
        let cancel = input(ChannelInput::Cancel);
        let mut answers = row![Space::new().width(Length::Fill)].spacing(form::TIGHT);
        match confirm {
            OverwriteConfirm::DenySelf(_) => {
                answers = answers.push(action(theme, "Okay", Kind::Primary, Some(cancel.clone())));
            }
            OverwriteConfirm::Remove(_) => {
                answers = answers
                    .push(action(theme, "Cancel", Kind::Ghost, Some(cancel.clone())))
                    .push(action(
                        theme,
                        "Delete",
                        Kind::Danger,
                        Some(input(ChannelInput::Confirm)),
                    ));
            }
            OverwriteConfirm::Sync => {
                answers = answers
                    .push(action(
                        theme,
                        "Keep Current Permissions",
                        Kind::Ghost,
                        Some(cancel.clone()),
                    ))
                    .push(action(
                        theme,
                        "Sync Permissions",
                        Kind::Primary,
                        Some(input(ChannelInput::Confirm)),
                    ));
            }
        }
        let body = column![
            text(title).size(20).font(BOLD).color(theme.text),
            text(question).size(15).color(theme.secondary),
            answers
        ]
        .spacing(12)
        .padding(16);
        crate::dialog::panel(theme, body.into(), CONFIRM, None, cancel)
    }

    /// Add's picker: the roles without an overwrite, then the members
    /// found by what's typed; one chosen is added at once.
    fn add_overwrite<'a>(&'a self, core: &'a ChannelSettingsScreen) -> Element<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let Some(adding) = &core.permissions.adding else {
            return Space::new().into();
        };
        let listed = app.listed_overwrites();
        let free = |id: &str| !listed.iter().any(|known| known.id == id);
        let query = adding.query.trim().to_lowercase();
        let roles: Vec<ServerRole> = app
            .server_roles(&core.room)
            .into_iter()
            .filter(|role| free(&role.id))
            .filter(|role| query.is_empty() || role.name.to_lowercase().contains(&query))
            .collect();
        let members: Vec<&RoleMember> = adding
            .found
            .iter()
            .filter(|member| free(&member.id))
            .collect();
        let mut list = column![];
        if !roles.is_empty() {
            list = list.push(form::heading(theme, "ROLES"));
        }
        for role in roles {
            list = list.push(
                self.add_row(
                    row![
                        dot(role_colour(role.color.unwrap_or(0))),
                        text(role.name)
                            .size(15)
                            .color(theme.text)
                            .wrapping(text::Wrapping::None)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                    ROLE_ROW,
                    Padding {
                        left: 8.0,
                        ..Padding::ZERO
                    },
                    input(ChannelInput::Add(role.id, None)),
                ),
            );
        }
        if !members.is_empty() {
            list = list.push(form::heading(theme, "MEMBERS"));
        }
        for member in members {
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
            list = list.push(
                self.add_row(
                    row![
                        self.avatar(
                            Some(member.avatar.as_str()).filter(|url| !url.is_empty()),
                            &member.name,
                            MEMBER_AVATAR,
                            MEMBER_AVATAR / 2.0,
                            1.0,
                        ),
                        container(names).width(Length::Fill).clip(true)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                    MEMBER_ROW,
                    Padding {
                        left: 6.0,
                        ..Padding::ZERO
                    },
                    input(ChannelInput::Add(member.id.clone(), Some(member.clone()))),
                ),
            );
        }
        let close = input(ChannelInput::Adding(false));
        let body = column![
            text("Add:").size(20).font(BOLD).color(theme.text),
            controls::field(theme, ADD_SEARCH, "Role/Member", &adding.query, false, true)
                .size(14)
                .padding([10.9, 12.0])
                .on_input(|text| input(ChannelInput::FindMembers(text))),
            scroll_view(list)
                .style(scrollbar(theme))
                .height(Length::Fill),
            row![
                Space::new().width(Length::Fill),
                action(theme, "Cancel", Kind::Ghost, Some(close.clone()))
            ]
        ]
        .spacing(12)
        .padding(16)
        .height(Length::Fill);
        let height = ADD_HEIGHT.min(self.window.height - ADD_ROOM);
        crate::dialog::panel(theme, body.into(), ADD_WIDTH, Some(height), close)
    }

    /// A row of Add's picker, lit under the pointer.
    fn add_row<'a>(
        &self,
        face: Element<'a, Message>,
        height: f32,
        padding: Padding,
        press: Message,
    ) -> Element<'a, Message> {
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding(padding)
        .width(Length::Fill)
        .height(height)
        .on_press(press)
        .style(controls::lit_style(None, self.theme.hover, 4.0))
        .into()
    }
}

/// A role among the server's, by id.
fn role_of<'a>(roles: &'a [ServerRole], id: &str) -> Option<&'a ServerRole> {
    roles.iter().find(|role| role.id == id)
}

/// A role's colour as a dot.
fn dot<'a>(colour: iced::Color) -> Element<'a, Message> {
    container(Space::new())
        .width(OVERWRITE_DOT)
        .height(OVERWRITE_DOT)
        .style(move |_| rounded(colour, OVERWRITE_DOT / 2.0))
        .into()
}
