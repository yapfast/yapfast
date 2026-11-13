//! A Discord server's settings, as Discord's client shows them: a page for
//! each part of the server, listed by what this account may change. Only
//! the open page's settings are held, from when it opens to when it
//! closes. Each page edits a draft against what Discord has; once the draft
//! differs, Discord's bar offers to reset or save it, and leaving the page
//! waits until one is chosen. See "Server settings" in docs/discord/server-settings.md.

use super::*;
use crate::{
    application::profile_editor::PictureEdit,
    chat::{
        AuditLog, AuditQuery, Ban, BansAction, GuildSettings, Permission, PickedPicture,
        ServerPicture, ServerSettingsChange, ServerSettingsPage, SettingsSection,
    },
};

mod audit_log;
mod bans;
mod boost;
mod engagement;
mod expressions;
mod invites;
mod members;
mod ownership;
mod people_input;
mod permissions;
mod profile;
mod role_input;
mod roles;
mod safety;

use audit_log::Known;
pub use audit_log::{
    AUDIT_ACTIONS, AUDIT_PAGE, AuditInput, AuditKind, AuditLine, AuditLogPage, AuditRow,
    Span as AuditSpan, Words as AuditWords,
};
pub use bans::{BANS_BATCH, BANS_PER_PAGE, BanInput, BansPage};
pub use boost::BoostPage;
pub use engagement::{AFK_TIMEOUTS, BOOSTED, EngagementPage, SETUP_TIPS, WELCOME, WELCOME_STICKER};
pub use expressions::{
    EMOJI_NAME_LIMIT, EmojiInput, EmojiPage, EmojiUpload, ExpressionRules, LEVEL_BOOSTS,
    STICKER_DESCRIPTION_LIMIT, STICKER_NAME_LIMIT, StickerCard, StickerDraft, StickerInput,
    StickersPage, emoji_name,
};
pub use invites::{InviteInput, InvitesPage, PAUSE_HOURS, SecurityDraft};
pub use members::{MEMBER_PAGE_SIZES, MEMBER_SORTS, MEMBERS_CHUNK, MemberInput, MembersPage};
pub use ownership::{DeleteAsk, Ownership, OwnershipInput, TransferAsk};
pub use permissions::{PermissionGroup, PermissionInfo, ROLE_PERMISSIONS};
pub use profile::{DESCRIPTION_LIMIT, ProfilePage, SettingsText};
pub use role_input::RoleInput;
pub use roles::{
    ADD_MEMBERS_LIMIT, AddMembers, DEFAULT_ROLE_COLOR, PermissionLock, ROLE_COLORS,
    ROLE_ICON_LIMIT, ROLE_NAME_LIMIT, RoleConfirm, RoleDraft, RoleLock, RoleRules, RoleTab,
    RolesPage,
};
pub use safety::{FILTER_ALL, HIGHEST_VERIFICATION, MfaRule, SafetyChoice, SafetyPage};

/// The settings of a Discord server, while they're open.
#[derive(Clone, Debug)]
pub struct ServerSettings {
    /// The server's id.
    pub server: String,
    pub section: SettingsSection,
    /// What the page shows; none while Discord sends it.
    pub page: Option<Page>,
    /// A picture being read for the page.
    pub reading: Option<(ServerPicture, PathBuf)>,
    /// Discord is taking the changes.
    pub saving: bool,
    pub error: Option<String>,
    /// How many times leaving was refused for unsaved changes: the bar
    /// shakes each time, as Discord's does.
    pub nudge: u32,
    /// Delete Server's or Transfer Ownership's dialog, while open.
    pub ownership: Option<Ownership>,
}

/// A page of the settings, holding what Discord has and the draft.
#[derive(Clone, Debug)]
pub enum Page {
    Profile(ProfilePage),
    Engagement(EngagementPage),
    BoostPerks(BoostPage),
    Emoji(Box<EmojiPage>),
    Stickers(Box<StickersPage>),
    Members(Box<MembersPage>),
    Roles(Box<RolesPage>),
    Invites(Box<InvitesPage>),
    Safety(SafetyPage),
    AuditLog(Box<AuditLogPage>),
    Bans(Box<BansPage>),
}

/// A switch of a settings page: one of Engagement's system messages, by
/// its bit, or Boost Perks' progress bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsToggle {
    SystemMessage(u64),
    ProgressBar,
}

/// A choice from a settings page's lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsChoice {
    /// A text channel by its Discord id, or none.
    SystemChannel(Option<String>),
    /// 0 for all messages, 1 for only mentions.
    Notifications(u8),
    /// A voice channel by its Discord id, or none.
    AfkChannel(Option<String>),
    /// Seconds, one of [`AFK_TIMEOUTS`].
    AfkTimeout(u32),
    /// Safety Setup's, saved as it's made.
    Safety(SafetyChoice),
}

impl ServerSettings {
    /// Waiting for Discord to send the page.
    pub fn loading(&self) -> bool {
        self.page.is_none() && self.error.is_none()
    }

    /// Whether the draft differs from what Discord has.
    pub fn changed(&self) -> bool {
        self.change().is_some()
    }

    /// What saving would change.
    pub fn change(&self) -> Option<ServerSettingsChange> {
        match self.page.as_ref()? {
            Page::Profile(page) => page.change(),
            Page::Engagement(page) => page.change(),
            Page::BoostPerks(page) => page.change(),
            Page::Roles(page) => page.change(),
            // Saved as they're made, as in Discord's client.
            Page::Emoji(_)
            | Page::Stickers(_)
            | Page::Members(_)
            | Page::Invites(_)
            | Page::Safety(_)
            | Page::AuditLog(_)
            | Page::Bans(_) => None,
        }
    }

    /// Whether the page saves each change as it's made, without the bar.
    pub fn immediate(&self) -> bool {
        matches!(
            self.section,
            SettingsSection::Emoji
                | SettingsSection::Stickers
                | SettingsSection::Members
                | SettingsSection::Invites
                | SettingsSection::Safety
                | SettingsSection::AuditLog
                | SettingsSection::Bans
        )
    }

    /// Why the draft can't be saved as it is.
    pub fn problem(&self) -> Option<String> {
        match self.page.as_ref()? {
            Page::Profile(page) => page.problem(),
            Page::Roles(page) => page.problem(),
            Page::Engagement(_)
            | Page::BoostPerks(_)
            | Page::Emoji(_)
            | Page::Stickers(_)
            | Page::Members(_)
            | Page::Invites(_)
            | Page::Safety(_)
            | Page::AuditLog(_)
            | Page::Bans(_) => None,
        }
    }

    /// Whether the draft may be saved: it has no problem, and no picture is
    /// being read.
    pub fn ready(&self) -> bool {
        !self.saving && self.reading.is_none() && self.problem().is_none()
    }
}

impl Application {
    /// The pages of a Discord server's settings this account may open, in
    /// the order the sidebar lists them.
    pub fn settings_sections(&self, server: &str) -> Vec<SettingsSection> {
        let mut sections = vec![];
        if !self.may(server, Permission::Settings) {
            return sections;
        }
        if self.may(server, Permission::ManageServer) {
            sections.extend([
                SettingsSection::Profile,
                SettingsSection::Engagement,
                SettingsSection::BoostPerks,
            ]);
        }
        // Discord's Expression.
        if self.may(server, Permission::ManageExpressions)
            || self.may(server, Permission::CreateExpressions)
        {
            sections.extend([SettingsSection::Emoji, SettingsSection::Stickers]);
        }
        // Discord's People.
        let moderates = [
            Permission::ManageServer,
            Permission::Ban,
            Permission::Kick,
            Permission::Moderate,
            Permission::ManageRoles,
            Permission::ManageNicknames,
        ];
        if moderates.into_iter().any(|bit| self.may(server, bit)) {
            sections.push(SettingsSection::Members);
        }
        if self.may(server, Permission::ManageRoles) {
            sections.push(SettingsSection::Roles);
        }
        if self.may(server, Permission::ManageServer) {
            sections.push(SettingsSection::Invites);
        }
        // Discord's Moderation.
        if self.may(server, Permission::ManageServer) {
            sections.push(SettingsSection::Safety);
        }
        if self.may(server, Permission::ViewAuditLog) {
            sections.push(SettingsSection::AuditLog);
        }
        if self.may(server, Permission::Ban) || self.may(server, Permission::ViewAuditLog) {
            sections.push(SettingsSection::Bans);
        }
        sections
    }

    /// Opens a Discord server's settings, by its server id or one of its
    /// channels' room ids, at their first page.
    pub(super) fn open_server_settings(&mut self, place: String) -> Vec<Effect> {
        let guild = discord::parse_server(&place)
            .or_else(|| discord::parse_room(&place).map(|(guild, _)| guild));
        let Some(server) = guild.map(discord::server_id) else {
            return vec![];
        };
        if !self
            .menu_choices(&server)
            .contains(&MenuChoice::ServerSettings)
        {
            return vec![];
        }
        let Some(&section) = self.settings_sections(&server).first() else {
            return vec![];
        };
        self.state.server_settings = Some(ServerSettings {
            server: server.clone(),
            section,
            page: None,
            reading: None,
            saving: false,
            error: None,
            nudge: 0,
            ownership: None,
        });
        load(server, section)
    }

    /// Goes to another page, unless the draft has changes, which the bar
    /// asks to reset or save first.
    pub(super) fn server_settings_section(&mut self, section: SettingsSection) -> Vec<Effect> {
        let Some(server) = self
            .state
            .server_settings
            .as_ref()
            .map(|settings| settings.server.clone())
        else {
            return vec![];
        };
        if !self.settings_sections(&server).contains(&section) {
            return vec![];
        }
        let Some(settings) = self.state.server_settings.as_mut() else {
            return vec![];
        };
        if settings.section == section {
            return vec![];
        }
        if settings.changed() || settings.saving {
            settings.nudge = settings.nudge.wrapping_add(1);
            return vec![];
        }
        settings.section = section;
        settings.ownership = None;
        settings.page = None;
        settings.reading = None;
        settings.error = None;
        load(server, section)
    }

    /// Closes the settings, unless the draft has changes.
    pub(super) fn close_server_settings(&mut self) {
        let Some(settings) = self.state.server_settings.as_mut() else {
            return;
        };
        if settings.changed() || settings.saving {
            settings.nudge = settings.nudge.wrapping_add(1);
            return;
        }
        // Not while Discord deletes the server or gives it away.
        if let Some(Ownership::Delete(DeleteAsk { sending: true, .. }))
        | Some(Ownership::Transfer(TransferAsk { sending: true, .. })) = &settings.ownership
        {
            return;
        }
        self.state.server_settings = None;
    }

    pub(super) fn server_settings_loaded(
        &mut self,
        server: String,
        section: SettingsSection,
        result: Result<ServerSettingsPage, String>,
    ) {
        let waiting = |settings: &ServerSettings| {
            settings.server == server && settings.section == section && settings.loading()
        };
        if !self.state.server_settings.as_ref().is_some_and(waiting) {
            return;
        }
        // Audit Log's entries are put into words as they arrive, naming
        // the channels and roles the server has.
        let result = result.map(|page| match page {
            ServerSettingsPage::AuditLog(log) if section == SettingsSection::AuditLog => {
                let mut page = AuditLogPage::default();
                let known = Known {
                    app: self,
                    server: &server,
                };
                page.take(&AuditQuery::default(), log, &known);
                Some(Page::AuditLog(Box::new(page)))
            }
            page => Page::new(section, page),
        });
        let Some(settings) = self.state.server_settings.as_mut() else {
            return;
        };
        match result {
            Ok(page) => settings.page = page,
            Err(error) => settings.error = Some(error),
        }
        #[cfg(feature = "discord")]
        self.follow_emojis();
    }

    /// Something done on Audit Log.
    pub(super) fn audit_log_input(&mut self, input: AuditInput) -> Vec<Effect> {
        let Some(settings) = self.state.server_settings.as_mut() else {
            return vec![];
        };
        let server = settings.server.clone();
        let Some(Page::AuditLog(page)) = settings.page.as_mut() else {
            return vec![];
        };
        let query = match input {
            AuditInput::User(user) => page.filter(user, page.action),
            AuditInput::Action(action) => page.filter(page.user.clone(), action),
            AuditInput::More => page.more(),
            AuditInput::Toggle(row) => {
                page.toggle(row);
                None
            }
        };
        query
            .map(|query| {
                Effect::Backend(Command::Discord(DiscordCommand::AuditLog { server, query }))
            })
            .into_iter()
            .collect()
    }

    /// The audit log page asked for, unless another was asked for since.
    pub(super) fn audit_log_loaded(
        &mut self,
        server: String,
        query: AuditQuery,
        result: Result<AuditLog, String>,
    ) {
        let asked = |settings: &ServerSettings| {
            settings.server == server
                && matches!(&settings.page, Some(Page::AuditLog(page))
                    if page.asking.as_ref() == Some(&query))
        };
        let Some(settings) = self.state.server_settings.as_mut().filter(|s| asked(s)) else {
            return;
        };
        // Taken out while it's put into words, which reads the server.
        let Some(Page::AuditLog(mut page)) = settings.page.take() else {
            return;
        };
        match result {
            Ok(log) => {
                let known = Known {
                    app: self,
                    server: &server,
                };
                page.take(&query, log, &known);
            }
            Err(_) => page.refused(),
        }
        if let Some(settings) = self.state.server_settings.as_mut() {
            settings.page = Some(Page::AuditLog(page));
        }
    }

    /// Changes a text of the page's draft.
    pub(super) fn write_server_setting(&mut self, field: SettingsText, text: String) {
        if let Some(Page::Profile(page)) = self.editable_page() {
            page.write(field, text);
        }
    }

    /// Flips a switch of the page's draft.
    pub(super) fn toggle_server_setting(&mut self, toggle: SettingsToggle) {
        match (self.editable_page(), toggle) {
            (Some(Page::Engagement(page)), SettingsToggle::SystemMessage(bit)) => page.toggle(bit),
            (Some(Page::BoostPerks(page)), SettingsToggle::ProgressBar) => {
                page.progress_bar = !page.progress_bar;
            }
            _ => {}
        }
    }

    /// Takes a choice from one of the page's lists; Safety Setup's goes
    /// to Discord at once.
    pub(super) fn choose_server_setting(&mut self, choice: SettingsChoice) -> Vec<Effect> {
        if let SettingsChoice::Safety(choice) = choice {
            return self.choose_safety(choice);
        }
        let Some(Page::Engagement(page)) = self.editable_page() else {
            return vec![];
        };
        match choice {
            SettingsChoice::SystemChannel(channel) => page.system_channel = channel,
            SettingsChoice::Notifications(level) if level <= 1 => {
                page.default_notifications = level;
            }
            SettingsChoice::AfkChannel(channel) => page.afk_channel = channel,
            // Discord greys the timeout out without a channel.
            SettingsChoice::AfkTimeout(seconds)
                if AFK_TIMEOUTS.contains(&seconds) && page.afk_channel.is_some() =>
            {
                page.afk_timeout = seconds;
            }
            _ => {}
        }
        vec![]
    }

    /// Who may change whether a server's moderators need two-factor
    /// authentication: its owner, with it on.
    pub fn mfa_rule(&self, server: &str) -> MfaRule {
        #[cfg(feature = "discord")]
        if let Some((known, _, me)) = self.discord_place(server)
            && known.owner_id.as_deref() == Some(me)
        {
            let on = self
                .discord
                .as_ref()
                .and_then(|snapshot| snapshot.account.as_ref())
                .is_some_and(|account| account.mfa_enabled);
            return if on {
                MfaRule::May
            } else {
                MfaRule::OwnerWithoutMfa
            };
        }
        let _ = server;
        MfaRule::NotOwner
    }

    /// Sends a choice on Safety Setup, which shows while Discord takes it.
    fn choose_safety(&mut self, choice: SafetyChoice) -> Vec<Effect> {
        let Some(server) = self
            .state
            .server_settings
            .as_ref()
            .map(|settings| settings.server.clone())
        else {
            return vec![];
        };
        let rule = self.mfa_rule(&server);
        let Some(settings) = self.state.server_settings.as_mut() else {
            return vec![];
        };
        let Some(Page::Safety(page)) = settings.page.as_mut().filter(|_| !settings.saving) else {
            return vec![];
        };
        let Some(change) = page.choose(choice, rule) else {
            return vec![];
        };
        page.choosing = Some(choice);
        settings.saving = true;
        settings.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SaveServerSettings {
                server,
                change: Box::new(change),
            },
        ))]
    }

    /// Something done on Bans.
    pub(super) fn ban_input(&mut self, input: BanInput) -> Vec<Effect> {
        let Some(settings) = self.state.server_settings.as_ref() else {
            return vec![];
        };
        let server = settings.server.clone();
        let may_unban = self.may(&server, Permission::Ban);
        let Some(Page::Bans(page)) = self
            .state
            .server_settings
            .as_mut()
            .and_then(|settings| settings.page.as_mut())
        else {
            return vec![];
        };
        let action = match input {
            BanInput::Query(query) => {
                page.write(query);
                None
            }
            BanInput::Search => page.search().map(BansAction::Search),
            BanInput::Page(to) => page.go(to).map(BansAction::More),
            BanInput::Open(user) => {
                if page.bans.iter().any(|ban| ban.user.id == user) {
                    page.close();
                    page.open = Some(user);
                }
                None
            }
            BanInput::Close => {
                page.close();
                None
            }
            BanInput::Unban => {
                let user = page.open.clone().filter(|_| may_unban && !page.unbanning);
                if user.is_some() {
                    page.unbanning = true;
                    page.open_error = None;
                }
                user.map(BansAction::Unban)
            }
        };
        action
            .map(|action| {
                Effect::Backend(Command::Discord(DiscordCommand::Bans { server, action }))
            })
            .into_iter()
            .collect()
    }

    /// The open Bans, if it's this server's.
    fn bans_page(&mut self, server: &str) -> Option<&mut BansPage> {
        let settings = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server)?;
        match settings.page.as_mut()? {
            Page::Bans(page) => Some(page),
            _ => None,
        }
    }

    pub(super) fn bans_loaded(
        &mut self,
        server: String,
        searched: bool,
        result: Result<Vec<Ban>, String>,
    ) {
        let error = match result {
            Ok(bans) => {
                if let Some(page) = self.bans_page(&server) {
                    page.take(bans, searched);
                }
                return;
            }
            Err(error) => error,
        };
        if let Some(page) = self.bans_page(&server) {
            page.fetching = false;
            if let Some(settings) = self.state.server_settings.as_mut() {
                settings.error = Some(error);
            }
        }
    }

    pub(super) fn unbanned(&mut self, server: String, user: String, result: Result<(), String>) {
        let Some(page) = self.bans_page(&server) else {
            return;
        };
        match result {
            Ok(()) => {
                let ban = page.bans.iter().find(|ban| ban.user.id == user).cloned();
                if let Some(ban) = ban {
                    page.follow(ban, false);
                }
            }
            Err(error) if page.open.as_ref() == Some(&user) => {
                page.unbanning = false;
                page.open_error = Some(error);
            }
            Err(_) => {}
        }
    }

    /// A ban made or lifted, for an open Bans.
    pub(super) fn server_ban(&mut self, server: String, ban: Ban, banned: bool) {
        if let Some(page) = self.bans_page(&server) {
            page.follow(ban, banned);
        }
    }

    /// What this account may do on the open settings' Roles.
    fn settings_rules(&self) -> RoleRules {
        self.state
            .server_settings
            .as_ref()
            .map(|settings| self.role_rules(&settings.server))
            .unwrap_or_default()
    }

    /// The open page, unless Discord is taking its changes.
    fn editable_page(&mut self) -> Option<&mut Page> {
        self.state
            .server_settings
            .as_mut()
            .filter(|settings| !settings.saving)?
            .page
            .as_mut()
    }

    /// Reads a picture chosen for the server: its icon on Server Profile,
    /// its banner or invite background on Boost Perks, once its level
    /// unlocks it.
    pub(super) fn pick_server_picture(
        &mut self,
        kind: ServerPicture,
        path: PathBuf,
    ) -> Vec<Effect> {
        let rules = self.settings_rules();
        let creates = self.state.server_settings.as_ref().is_some_and(|settings| {
            self.may(&settings.server, Permission::CreateExpressions)
                || self.may(&settings.server, Permission::ManageExpressions)
        });
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| !settings.saving)
        else {
            return vec![];
        };
        let fits = match (&settings.page, kind) {
            (Some(Page::Profile(_)), ServerPicture::Icon) => true,
            (Some(Page::BoostPerks(page)), ServerPicture::Banner | ServerPicture::Splash) => {
                page.unlocked(kind)
            }
            (Some(Page::Roles(page)), ServerPicture::RoleIcon) => page.icon_editable(&rules),
            (Some(Page::Emoji(page)), ServerPicture::Emoji) => {
                creates && page.upload.is_none() && (page.free(false) > 0 || page.free(true) > 0)
            }
            (Some(Page::Stickers(page)), ServerPicture::Sticker) => page
                .draft
                .as_ref()
                .is_some_and(|draft| draft.editing.is_none() && !draft.saving),
            _ => false,
        };
        if !fits {
            return vec![];
        }
        settings.reading = Some((kind, path.clone()));
        settings.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ServerPicture { kind, path },
        ))]
    }

    pub(super) fn server_picture_read(
        &mut self,
        kind: ServerPicture,
        path: PathBuf,
        result: Result<PickedPicture, String>,
    ) {
        let Some(server) = self
            .state
            .server_settings
            .as_ref()
            .map(|settings| settings.server.clone())
        else {
            return;
        };
        // An animated icon takes level 1, as Discord's ANIMATED_ICON.
        let animated_icon = self.premium_tier(&server) >= 1;
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.reading.as_ref() == Some(&(kind, path.clone())))
        else {
            return;
        };
        settings.reading = None;
        let picked = match result {
            Ok(picked) => picked,
            Err(error) => {
                match &mut settings.page {
                    Some(Page::Stickers(page)) if page.draft.is_some() => {
                        if let Some(draft) = &mut page.draft {
                            draft.error = Some(error);
                        }
                    }
                    _ => settings.error = Some(error),
                }
                return;
            }
        };
        let gif = picked.mime == "image/gif";
        settings.error = match (&mut settings.page, kind) {
            (Some(Page::Profile(_)), ServerPicture::Icon) if gif && !animated_icon => {
                Some("An animated icon unlocks at boost level 1.".to_owned())
            }
            (Some(Page::Profile(page)), ServerPicture::Icon) => {
                page.icon = PictureEdit::New(picked);
                None
            }
            (Some(Page::BoostPerks(page)), ServerPicture::Banner | ServerPicture::Splash)
                if gif && !page.animated(kind) =>
            {
                Some(
                    match kind {
                        ServerPicture::Banner => "An animated banner unlocks at boost level 3.",
                        _ => "An invite background can't be animated.",
                    }
                    .to_owned(),
                )
            }
            (Some(Page::BoostPerks(page)), ServerPicture::Banner | ServerPicture::Splash) => {
                if let Some(edit) = page.edit(kind) {
                    *edit = PictureEdit::New(picked);
                }
                None
            }
            (Some(Page::Roles(_)), ServerPicture::RoleIcon) if gif => {
                Some("A role icon can't be animated.".to_owned())
            }
            (Some(Page::Roles(_)), ServerPicture::RoleIcon)
                if picked.data.len() > ROLE_ICON_LIMIT =>
            {
                Some("Role icons must be under 256 KB.".to_owned())
            }
            (Some(Page::Roles(page)), ServerPicture::RoleIcon) => {
                if let Some(draft) = &mut page.open {
                    draft.icon = PictureEdit::New(picked);
                    draft.emoji = None;
                }
                None
            }
            // The dialog opens with the picture, named after its file.
            (Some(Page::Emoji(page)), ServerPicture::Emoji) => {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                page.upload = Some(EmojiUpload {
                    name: emoji_name(&stem),
                    picture: Some(picked),
                    ..EmojiUpload::default()
                });
                None
            }
            (Some(Page::Stickers(page)), ServerPicture::Sticker) => {
                if let Some(draft) = &mut page.draft {
                    draft.file = Some(picked);
                    draft.file_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    draft.error = None;
                }
                None
            }
            _ => None,
        };
    }

    /// Removes the server's icon, banner or invite background, as the
    /// pages' Remove buttons do.
    pub(super) fn remove_server_picture(&mut self, kind: ServerPicture) {
        let rules = self.settings_rules();
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| !settings.saving)
        else {
            return;
        };
        if settings
            .reading
            .as_ref()
            .is_some_and(|(reading, _)| *reading == kind)
        {
            settings.reading = None;
        }
        let (edit, kept) = match (&mut settings.page, kind) {
            (Some(Page::Profile(page)), ServerPicture::Icon) => {
                (&mut page.icon, page.original.icon.is_some())
            }
            (Some(Page::BoostPerks(page)), ServerPicture::Banner) => {
                (&mut page.banner, page.original.banner.is_some())
            }
            (Some(Page::BoostPerks(page)), ServerPicture::Splash) => {
                (&mut page.splash, page.original.splash.is_some())
            }
            (Some(Page::Roles(page)), ServerPicture::RoleIcon) => {
                let kept = page
                    .open_role()
                    .is_some_and(|role| role.icon.is_some() || role.unicode_emoji.is_some());
                if !page.icon_editable(&rules) {
                    return;
                }
                let Some(draft) = &mut page.open else {
                    return;
                };
                draft.emoji = None;
                (&mut draft.icon, kept)
            }
            _ => return,
        };
        *edit = if kept {
            PictureEdit::Reset
        } else {
            PictureEdit::Keep
        };
    }

    /// Puts back what Discord has, as the bar's Reset does.
    pub(super) fn undo_server_settings(&mut self) {
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| !settings.saving)
        else {
            return;
        };
        settings.error = None;
        settings.reading = None;
        match &mut settings.page {
            Some(Page::Profile(page)) => page.undo(),
            Some(Page::Engagement(page)) => *page = EngagementPage::new(page.original.clone()),
            Some(Page::BoostPerks(page)) => *page = BoostPage::new(page.original.clone()),
            Some(Page::Roles(page)) => page.undo(),
            Some(
                Page::Emoji(_)
                | Page::Stickers(_)
                | Page::Members(_)
                | Page::Invites(_)
                | Page::Safety(_)
                | Page::AuditLog(_)
                | Page::Bans(_),
            )
            | None => {}
        }
    }

    /// Sends what changed, as the bar's Save Changes does.
    pub(super) fn save_server_settings(&mut self) -> Vec<Effect> {
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.ready())
        else {
            return vec![];
        };
        let Some(change) = settings.change() else {
            return vec![];
        };
        settings.saving = true;
        settings.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SaveServerSettings {
                server: settings.server.clone(),
                change: Box::new(change),
            },
        ))]
    }

    /// Discord took the changes, giving the page as it now stands, or
    /// refused them; the draft stays to try again.
    pub(super) fn server_settings_saved(
        &mut self,
        server: String,
        result: Result<ServerSettingsPage, String>,
    ) {
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.saving && settings.server == server)
        else {
            return;
        };
        settings.saving = false;
        match result {
            Ok(page) => {
                let new = Page::new(settings.section, page);
                settings.page = match (new, settings.page.take()) {
                    (Some(Page::Roles(new)), Some(Page::Roles(was))) => {
                        Some(Page::Roles(Box::new((*new).carry(*was))))
                    }
                    (new, _) => new,
                };
            }
            Err(error) => {
                if let Some(Page::Safety(page)) = &mut settings.page {
                    page.choosing = None;
                }
                settings.error = Some(error);
            }
        }
    }

    /// A server's settings changed elsewhere, or by this account's save:
    /// the open page takes what its draft didn't change.
    pub(super) fn server_settings_changed(&mut self, server: String, changed: GuildSettings) {
        let Some(settings) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server && !settings.saving)
        else {
            return;
        };
        match &mut settings.page {
            Some(Page::Engagement(page)) => page.follow(changed),
            Some(Page::BoostPerks(page)) => page.follow(changed),
            Some(Page::Safety(page)) => page.settings = changed,
            Some(Page::Invites(page)) => page.settings = changed,
            Some(
                Page::Profile(_)
                | Page::Emoji(_)
                | Page::Stickers(_)
                | Page::Members(_)
                | Page::Roles(_)
                | Page::AuditLog(_)
                | Page::Bans(_),
            )
            | None => {}
        }
    }

    /// Keeps the open settings to what this account may still change, as
    /// a new snapshot says: they close once it may change nothing, or the
    /// server is gone, and the page follows a name or icon changed
    /// elsewhere that the draft didn't.
    #[cfg(feature = "discord")]
    pub(super) fn follow_server_settings(&mut self) -> Vec<Effect> {
        let Some(server) = self
            .state
            .server_settings
            .as_ref()
            .map(|settings| settings.server.clone())
        else {
            return vec![];
        };
        let sections = self.settings_sections(&server);
        let known = self.state.servers.iter().find(|known| known.id == server);
        let (name, icon) = match known {
            Some(known) => (known.name.clone(), known.icon.clone()),
            None => (String::new(), None),
        };
        let Some(settings) = self.state.server_settings.as_mut() else {
            return vec![];
        };
        if known.is_none() || !sections.contains(&settings.section) {
            self.state.server_settings = None;
            return vec![];
        }
        if let Some(Page::Profile(page)) = &mut settings.page {
            page.follow(name, icon);
        }
        self.follow_emojis()
    }

    /// The text channels a server's system messages may go to, or its
    /// voice channels for the inactive channel: each Discord id and name,
    /// in the sidebar's order.
    pub fn settings_channels(&self, server: &str, voice: bool) -> Vec<(String, String)> {
        #[cfg(feature = "discord")]
        if let Some((known, _, _)) = self.discord_place(server) {
            let channels = if voice {
                &known.voice_channels
            } else {
                &known.text_channels
            };
            let mut found: Vec<_> = channels
                .iter()
                .filter(|channel| voice || (channel.forum.is_none() && !channel.announcement))
                .collect();
            found.sort_by_key(|channel| (channel.position, channel.id.clone()));
            return found
                .into_iter()
                .map(|channel| (channel.id.clone(), channel.name.clone()))
                .collect();
        }
        let _ = (server, voice);
        vec![]
    }
}

impl Page {
    fn new(section: SettingsSection, page: ServerSettingsPage) -> Option<Self> {
        Some(match (section, page) {
            (SettingsSection::Profile, ServerSettingsPage::Profile(profile)) => {
                Self::Profile(ProfilePage::new(profile))
            }
            (SettingsSection::Engagement, ServerSettingsPage::Guild(settings)) => {
                Self::Engagement(EngagementPage::new(settings))
            }
            (SettingsSection::BoostPerks, ServerSettingsPage::Guild(settings)) => {
                Self::BoostPerks(BoostPage::new(settings))
            }
            (SettingsSection::Emoji, ServerSettingsPage::Emoji { emojis, settings }) => {
                Self::Emoji(Box::new(EmojiPage::new(emojis, settings)))
            }
            (SettingsSection::Stickers, ServerSettingsPage::Stickers { stickers, settings }) => {
                Self::Stickers(Box::new(StickersPage::new(stickers, settings)))
            }
            (SettingsSection::Roles, ServerSettingsPage::Roles(roles)) => {
                Self::Roles(Box::new(RolesPage::new(roles)))
            }
            (SettingsSection::Safety, ServerSettingsPage::Guild(settings)) => {
                Self::Safety(SafetyPage::new(settings))
            }
            (SettingsSection::Members, ServerSettingsPage::Members { members, total }) => {
                Self::Members(Box::new(MembersPage::new(members, total)))
            }
            (SettingsSection::Invites, ServerSettingsPage::Invites { invites, settings }) => {
                Self::Invites(Box::new(InvitesPage::new(invites, settings)))
            }
            (SettingsSection::Bans, ServerSettingsPage::Bans(bans)) => {
                Self::Bans(Box::new(BansPage::new(bans)))
            }
            _ => return None,
        })
    }
}

/// Asks Discord for a page.
fn load(server: String, section: SettingsSection) -> Vec<Effect> {
    vec![Effect::Backend(Command::Discord(
        DiscordCommand::ServerSettings { server, section },
    ))]
}
