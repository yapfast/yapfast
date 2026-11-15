//! A Discord server's settings, as the Slint client's `ServerSettingsScreen`
//! (`ui/server_settings.slint` and the pages it holds): on the settings
//! frame (`sheet.rs`), the pages this account may change down the left
//! under Discord's groups, headed by the server's name, with Delete Server
//! last for its owner; the open page on the right, and Discord's bar over
//! its foot once its draft differs from what Discord has. Leaving the page
//! or the settings with changes shakes the bar instead.
//!
//! What each page holds, and what this account may do there, is the core's
//! (`ServerSettings`, opened by `Action::OpenServerSettings`); the screen
//! keeps only what's its own while it's open ([`Screen`]: an open
//! dropdown, a multi-line text, the bar's shake) and lets go of it, and of
//! the pages' pictures, when the settings close. Each page's view and
//! controls are in its module.

mod expressions;
mod moderation;
mod ownership;
pub(crate) mod people;
mod profile;
pub(crate) mod roles;

use std::path::PathBuf;

use iced::{
    Element, Length, Task,
    widget::{column, operation},
};
use yapfast_core::{
    application::{
        Action, AuditInput, BanInput, EmojiInput, InviteInput, MemberInput, OwnershipInput,
        RoleInput, ServerSettings, SettingsChoice, SettingsPage, SettingsText, SettingsToggle,
        StickerInput,
    },
    chat::{ServerPicture, SettingsSection},
};

use crate::{
    app::{App, Message},
    field::{self, Draft},
    form,
    pictures::Icon,
    sheet::{Nav, sheet},
};

/// The description's field on Server Profile.
const DESCRIPTION: &str = "server-description";

/// Which dropdown is open, at most one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    SystemChannel,
    AfkChannel,
    AfkTimeout,
    AuditUser,
    AuditAction,
    MembersSort,
    MembersPage,
    InviteAge,
    InviteUses,
    PauseHours,
}

/// The settings' own state while they're open.
#[derive(Default)]
pub struct Screen {
    /// The server whose settings these are.
    server: String,
    open: Option<Drop>,
    shake: form::Shake,
    /// Server Profile's description, which the field edits in place.
    description: Draft,
    /// Roles' own: the custom colour and the emoji being typed, a role's
    /// More menu, the row being dragged.
    roles: roles::Local,
    /// Members' row whose menu is open.
    people: people::Local,
    /// Emoji's own: a name being written, the list's viewport.
    expressions: expressions::Local,
    /// The open dialog's field that took the keys when it showed.
    focused: Option<&'static str>,
}

/// What's done in the settings.
#[derive(Clone, Debug)]
pub enum Event {
    Section(SettingsSection),
    /// Delete Server, from the column.
    DeleteServer,
    Close,
    Write(SettingsText, String),
    Description(field::Event),
    Toggle(SettingsToggle),
    Choose(SettingsChoice),
    Drop(Drop),
    /// A picture's button: the file dialog, then what was chosen.
    Pick(ServerPicture),
    Picked(ServerPicture, PathBuf),
    Remove(ServerPicture),
    Undo,
    Save,
    /// The bar's shake steps and its red ending.
    Shake(form::ShakeStep),
    Role(RoleInput),
    RoleLocal(roles::Event),
    Member(MemberInput),
    MemberLocal(people::Event),
    Invite(InviteInput),
    Ban(BanInput),
    Audit(AuditInput),
    Emoji(EmojiInput),
    Sticker(StickerInput),
    ExpressionLocal(expressions::Event),
    Ownership(OwnershipInput),
}

fn message(event: Event) -> Message {
    Message::ServerSettings(event)
}

impl App {
    /// The core's settings, while they're open.
    fn server_settings(&self) -> Option<&ServerSettings> {
        self.client.state().server_settings.as_ref()
    }

    /// Takes in the core's settings: the screen comes with them and goes
    /// with them, and the bar shakes when leaving was refused.
    pub fn server_settings_sync(&mut self) -> Task<Message> {
        let Some(core) = self.client.state().server_settings.as_ref() else {
            self.server_screen = None;
            return Task::none();
        };
        let fresh = self
            .server_screen
            .as_ref()
            .is_none_or(|screen| screen.server != core.server);
        if fresh {
            self.server_screen = Some(Box::new(Screen {
                server: core.server.clone(),
                ..Screen::default()
            }));
        }
        let nudge = core.nudge;
        let description = match &core.page {
            Some(SettingsPage::Profile(page)) => Some(page.description.clone()),
            _ => None,
        };
        // A dialog's field takes the keys when it shows: Upload Emoji's
        // name, the server's name to delete it, the emailed code.
        let field = core
            .ownership
            .as_ref()
            .and_then(ownership::dialog_field)
            .or(match &core.page {
                Some(SettingsPage::Emoji(page)) => expressions::dialog_field(page),
                _ => None,
            });
        let Some(screen) = self.server_screen.as_mut() else {
            return Task::none();
        };
        // The description's field follows the core, as after Reset.
        if let Some(description) = description
            && screen.description.text() != description
        {
            screen.description.reset(&description);
        }
        let mut tasks = vec![];
        if screen.focused != field {
            screen.focused = field;
            tasks.extend(field.map(|id| operation::focus(iced::widget::Id::new(id))));
        }
        tasks.push(
            screen
                .shake
                .start(nudge, |step| message(Event::Shake(step))),
        );
        Task::batch(tasks)
    }

    /// The pictures the open page shows.
    pub fn server_settings_pictures(&self) -> Vec<String> {
        let Some(core) = self.server_settings() else {
            return Vec::new();
        };
        let mut urls = Vec::new();
        match &core.page {
            Some(SettingsPage::Profile(page)) => {
                urls.extend(page.icon().0.map(str::to_owned));
            }
            Some(SettingsPage::BoostPerks(page)) => {
                for kind in [ServerPicture::Banner, ServerPicture::Splash] {
                    urls.extend(page.picture(kind).0.map(str::to_owned));
                }
            }
            Some(SettingsPage::Roles(page)) => urls.extend(roles::pictures(page)),
            Some(SettingsPage::Members(page)) => {
                if let Some(screen) = &self.server_screen {
                    urls.extend(people::member_pictures(
                        page,
                        &screen.people,
                        self.window.height,
                    ));
                }
            }
            Some(SettingsPage::Invites(page)) => urls.extend(people::invite_pictures(page)),
            Some(SettingsPage::Bans(page)) => urls.extend(moderation::ban_pictures(page)),
            Some(SettingsPage::AuditLog(page)) => urls.extend(moderation::audit_pictures(page)),
            Some(SettingsPage::Emoji(page)) => {
                if let Some(screen) = &self.server_screen {
                    urls.extend(expressions::emoji_pictures(
                        page,
                        &screen.expressions,
                        self.window.height,
                    ));
                }
            }
            Some(SettingsPage::Stickers(page)) => urls.extend(expressions::sticker_pictures(page)),
            _ => {}
        }
        if let Some(ownership) = &core.ownership {
            urls.extend(ownership::pictures(self.client.state(), ownership));
        }
        urls
    }

    pub fn server_settings_event(&mut self, event: Event) -> Task<Message> {
        if self.server_screen.is_none() {
            return Task::none();
        }
        let action = match event {
            Event::Section(section) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = None;
                }
                self.client.dispatch(Action::ServerSettingsSection(section));
                let top = operation::snap_to(
                    iced::widget::Id::new(crate::sheet::PAGE),
                    iced::widget::scrollable::RelativeOffset::START,
                );
                return Task::batch([top, self.sync()]);
            }
            Event::DeleteServer => Action::Ownership(OwnershipInput::Delete),
            Event::Close => {
                self.client.dispatch(Action::CloseServerSettings);
                let task = self.sync();
                if self.server_screen.is_none() {
                    return Task::batch([task, self.refocus()]);
                }
                return task;
            }
            Event::Write(field, text) => Action::WriteServerSetting { field, text },
            Event::Description(event) => match event {
                field::Event::Edited(text, _) => Action::WriteServerSetting {
                    field: SettingsText::Description,
                    text,
                },
                _ => return Task::none(),
            },
            Event::Toggle(toggle) => Action::ToggleServerSetting(toggle),
            Event::Choose(choice) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = None;
                }
                Action::ChooseServerSetting(choice)
            }
            Event::Drop(drop) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = (screen.open != Some(drop)).then_some(drop);
                }
                return Task::none();
            }
            Event::Pick(kind) => return pick(kind),
            Event::Picked(kind, path) => Action::PickServerPicture { kind, path },
            Event::Remove(kind) => Action::RemoveServerPicture(kind),
            Event::Undo => Action::UndoServerSettings,
            Event::Save => Action::SaveServerSettings,
            Event::Shake(step) => {
                return self
                    .server_screen
                    .as_mut()
                    .map_or_else(Task::none, |screen| {
                        screen.shake.step(step, |step| message(Event::Shake(step)))
                    });
            }
            Event::Role(input) => {
                // Add members' search takes the keys as it opens.
                let focus = matches!(input, RoleInput::AddMembers);
                // A choice from a row's More menu is the end of it.
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.roles.close_menu();
                }
                self.client.dispatch(Action::Roles(input));
                let task = self.sync();
                return if focus {
                    Task::batch([task, roles::focus_candidates()])
                } else {
                    task
                };
            }
            Event::RoleLocal(event) => return self.roles_local(event),
            Event::Member(input) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = None;
                }
                Action::Members(input)
            }
            Event::MemberLocal(event) => return self.members_local(event),
            Event::Invite(input) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = None;
                }
                Action::Invites(input)
            }
            Event::Ban(input) => Action::Bans(input),
            Event::Audit(input) => {
                if let Some(screen) = self.server_screen.as_mut() {
                    screen.open = None;
                }
                Action::AuditLog(input)
            }
            Event::Emoji(input) => Action::Emoji(input),
            Event::Sticker(input) => Action::Stickers(input),
            Event::ExpressionLocal(event) => return self.expressions_local(event),
            Event::Ownership(input) => Action::Ownership(input),
        };
        self.client.dispatch(action);
        self.sync()
    }

    /// Escape in the settings: an open dropdown or menu closes, then a
    /// dialog over the page, then the settings (unless they have changes,
    /// which shakes the bar).
    pub fn server_settings_escape(&mut self) -> Task<Message> {
        // A member's menu or profile, from Members, is over the page.
        if self.menu.take().is_some() {
            return Task::none();
        }
        if self
            .profiles
            .shown
            .as_ref()
            .is_some_and(|shown| shown.look.view.large)
        {
            return self.profile_event(crate::profile::Event::Close);
        }
        let Some(screen) = self.server_screen.as_mut() else {
            return Task::none();
        };
        if screen.open.take().is_some() || screen.roles.close_menu() {
            return Task::none();
        }
        if let Some(input) = self.server_dialog_cancel() {
            return self.server_settings_event(input);
        }
        self.server_settings_event(Event::Close)
    }

    /// What closes the dialog open over the page, if one is.
    fn server_dialog_cancel(&self) -> Option<Event> {
        let core = self.server_settings()?;
        if core.ownership.is_some() {
            return Some(Event::Ownership(OwnershipInput::Cancel));
        }
        match core.page.as_ref()? {
            SettingsPage::Roles(page) if page.confirm.is_some() => {
                Some(Event::Role(RoleInput::Cancel))
            }
            SettingsPage::Roles(page) if page.adding.is_some() => {
                Some(Event::Role(RoleInput::CloseAdding))
            }
            SettingsPage::Bans(page) if page.open.is_some() => Some(Event::Ban(BanInput::Close)),
            SettingsPage::Invites(page) if page.security.is_some() => {
                Some(Event::Invite(InviteInput::CloseSecurity))
            }
            SettingsPage::Emoji(page) if page.upload.is_some() => {
                Some(Event::Emoji(EmojiInput::Cancel))
            }
            SettingsPage::Stickers(page) if page.draft.is_some() => {
                Some(Event::Sticker(StickerInput::Cancel))
            }
            _ => None,
        }
    }

    /// The settings over the dimmed window, with what's open over them.
    pub fn server_settings_view(&self) -> Option<Element<'_, Message>> {
        let core = self.server_settings()?;
        let screen = self.server_screen.as_deref()?;
        let theme = &self.theme;
        let mut page = column![].spacing(form::GAP).width(Length::Fill);
        let mut fill = false;
        if core.loading() {
            page = page.push(form::note(theme, "Loading..."));
        }
        if let Some(shown) = &core.page {
            let (view, fills) = match shown {
                SettingsPage::Profile(page) => {
                    (self.server_profile_page(core, page, screen), false)
                }
                SettingsPage::Engagement(page) => (self.engagement_page(core, page, screen), false),
                SettingsPage::BoostPerks(page) => (self.boost_page(core, page), false),
                SettingsPage::Roles(page) => self.roles_page(core, page, screen),
                SettingsPage::Safety(page) => (self.safety_page(core, page), false),
                SettingsPage::Bans(page) => (self.bans_page(core, page), true),
                SettingsPage::AuditLog(page) => (self.audit_page(page, screen), true),
                SettingsPage::Members(page) => (self.members_page(core, page, screen), true),
                SettingsPage::Invites(page) => (self.invites_page(core, page, screen), true),
                SettingsPage::Emoji(page) => (self.emoji_page(core, page, screen), true),
                SettingsPage::Stickers(page) => (self.stickers_page(core, page), false),
            };
            fill = fills;
            page = page.push(view);
        }
        if let Some(problem) = core.problem() {
            page = page.push(form::error(theme, problem));
        }
        if let Some(error) = &core.error {
            page = page.push(form::error(theme, error.as_str()));
        }
        let bar = ((core.changed() || core.saving) && !core.immediate()).then(|| {
            form::Unsaved {
                pad: 12.0,
                radius: crate::sheet::BAR_RADIUS,
                saving: core.saving,
                may_reset: true,
                may_save: core.ready(),
                shake: screen.shake,
                undo: message(Event::Undo),
                save: message(Event::Save),
            }
            .view(theme)
        });
        let frame = sheet(
            self.server_nav(core),
            page,
            message(Event::Close),
            "Server settings",
        )
        .bar(bar)
        .fill(fill)
        .view(self);
        let mut layers = iced::widget::stack![frame];
        for dialog in self.server_dialogs(core, screen) {
            layers = layers.push(dialog);
        }
        Some(layers.into())
    }

    /// The dialogs a page opens over the settings.
    fn server_dialogs<'a>(
        &'a self,
        core: &'a ServerSettings,
        screen: &'a Screen,
    ) -> Vec<Element<'a, Message>> {
        let mut dialogs = Vec::new();
        match &core.page {
            Some(SettingsPage::Roles(page)) => dialogs.extend(self.roles_dialogs(page, screen)),
            Some(SettingsPage::Bans(page)) => dialogs.extend(self.ban_dialog(core, page)),
            Some(SettingsPage::Invites(page)) => dialogs.extend(self.security_dialog(page, screen)),
            Some(SettingsPage::Emoji(page)) => dialogs.extend(self.emoji_dialog(page)),
            Some(SettingsPage::Stickers(page)) => dialogs.extend(self.sticker_dialog(core, page)),
            _ => {}
        }
        if let Some(ownership) = &core.ownership {
            dialogs.push(self.ownership_dialog(&core.server, ownership));
        }
        dialogs
    }

    /// The pages' column: the server's name over the pages this account may
    /// change, under Discord's Expression, People and Moderation, then
    /// Delete Server after a line for its owner. A heading with nothing
    /// under it goes, as the server's name does for someone who may only
    /// manage roles.
    fn server_nav<'a>(&'a self, core: &'a ServerSettings) -> Vec<Nav<'a>> {
        let app = &self.client.app;
        let name = app
            .state
            .servers
            .iter()
            .find(|known| known.id == core.server)
            .map(|known| known.name.to_uppercase())
            .unwrap_or_default();
        let mut rows = vec![Nav::Heading(name.into())];
        let mut group = "";
        for section in app.settings_sections(&core.server) {
            let heading = match section {
                SettingsSection::Emoji | SettingsSection::Stickers => "EXPRESSION",
                SettingsSection::Members | SettingsSection::Roles | SettingsSection::Invites => {
                    "PEOPLE"
                }
                SettingsSection::Safety | SettingsSection::AuditLog | SettingsSection::Bans => {
                    "MODERATION"
                }
                _ => "",
            };
            if heading != group {
                group = heading;
                rows.push(Nav::Heading(heading.into()));
            }
            let (label, icon) = section_look(section);
            rows.push(Nav::Page {
                label: label.into(),
                icon,
                selected: core.section == section,
                danger: false,
                press: message(Event::Section(section)),
            });
        }
        if app.may_give_up(&core.server) {
            rows.push(Nav::Line);
            rows.push(Nav::Page {
                label: "Delete Server".into(),
                icon: Icon::Trash,
                selected: false,
                danger: true,
                press: message(Event::DeleteServer),
            });
        }
        let mut kept: Vec<Nav<'a>> = Vec::with_capacity(rows.len());
        for row in rows {
            let heading = matches!(row, Nav::Heading(_) | Nav::Line);
            if heading
                && kept
                    .last()
                    .is_some_and(|last| matches!(last, Nav::Heading(_)))
            {
                kept.pop();
            }
            kept.push(row);
        }
        if kept
            .last()
            .is_some_and(|last| matches!(last, Nav::Heading(_) | Nav::Line))
        {
            kept.pop();
        }
        kept
    }
}

/// A page's name and icon in the column.
fn section_look(section: SettingsSection) -> (&'static str, Icon) {
    match section {
        SettingsSection::Profile => ("Server Profile", Icon::Pencil),
        SettingsSection::Engagement => ("Engagement", Icon::Bell),
        SettingsSection::BoostPerks => ("Boost Perks", Icon::Pin),
        SettingsSection::Members => ("Members", Icon::Users),
        SettingsSection::Roles => ("Roles", Icon::Shield),
        SettingsSection::Invites => ("Invites", Icon::Chain),
        SettingsSection::Safety => ("Safety Setup", Icon::Lock),
        SettingsSection::AuditLog => ("Audit Log", Icon::History),
        SettingsSection::Bans => ("Bans", Icon::Gavel),
        SettingsSection::Emoji => ("Emoji", Icon::Smile),
        SettingsSection::Stickers => ("Stickers", Icon::StickyNote),
    }
}

/// The system's file dialog for one of the server's pictures, in the
/// Slint client's words.
fn pick(kind: ServerPicture) -> Task<Message> {
    Task::perform(
        async move {
            rfd::AsyncFileDialog::new()
                .set_title(match kind {
                    ServerPicture::Icon => "Change Server Icon",
                    ServerPicture::Banner => "Upload Server Banner",
                    ServerPicture::Splash => "Upload Invite Background",
                    ServerPicture::RoleIcon => "Choose Role Icon",
                    ServerPicture::Emoji => "Upload Emoji",
                    ServerPicture::Sticker => "Upload Sticker",
                })
                .add_filter(
                    "Pictures",
                    // Discord's stickers are PNG, APNG, JPEG or GIF.
                    if kind == ServerPicture::Sticker {
                        &["png", "apng", "jpg", "jpeg", "gif"][..]
                    } else {
                        &["png", "jpg", "jpeg", "gif", "webp"][..]
                    },
                )
                .pick_file()
                .await
                .map(|file| file.path().to_owned())
        },
        move |path| match path {
            Some(path) => message(Event::Picked(kind, path)),
            None => Message::Nothing,
        },
    )
}

/// A picture of the server's in a box: the one Discord has by its address,
/// or a new one's preview, cut to cover `width` by `height` with round
/// corners; `empty` without either.
fn picture_box<'a>(
    app: &'a App,
    url: Option<&str>,
    new: Option<&std::sync::Arc<yapfast_core::chat::Picture>>,
    width: f32,
    height: f32,
    radius: f32,
    empty: Element<'a, Message>,
) -> Element<'a, Message> {
    let handle = match (new, url) {
        (Some(picture), _) => app.fit(picture, width as u32, height as u32, true),
        (None, Some(url)) => app.picture(url, width as u32, height as u32, true),
        (None, None) => None,
    };
    match handle {
        Some(handle) => crate::widgets::Pic::new(handle, iced::Size::new(width, height))
            .radius(radius)
            .into(),
        None => {
            let sunken = app.theme.sunken;
            iced::widget::container(empty)
                .center_x(width)
                .center_y(height)
                .style(move |_| crate::style::rounded(sunken, radius))
                .into()
        }
    }
}
