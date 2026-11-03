//! The editor of this account's Discord profile, as Discord's client has
//! it in two places: the main profile, from Edit Profile, with a display
//! name, pronouns, avatar, bio and a banner colour anyone may set and a
//! banner that takes Nitro; and the profile in one server, from a server's
//! "Edit server profile", with a nickname and pronouns anyone may set and
//! an avatar, banner and bio that take Nitro. See "Editing one's profile"
//! in docs/discord/people.md.

use super::*;
use crate::chat::{OwnProfile, Permission, PickedPicture, ProfileChange, ProfilePicture};

/// The longest name, pronouns and bio Discord takes, in characters: a
/// server nickname and a display name alike.
pub const NAME_LIMIT: usize = 32;
pub const PRONOUNS_LIMIT: usize = 40;
pub const BIO_LIMIT: usize = 190;

/// Where Discord's client sends someone to get Nitro.
const NITRO_PAGE: &str = "https://discord.com/nitro";

/// A text of the editor's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileText {
    /// The server nickname, or the display name.
    Name,
    Pronouns,
    Bio,
    /// The main profile's banner colour, as `#rrggbb`.
    Colour,
}

/// What becomes of an avatar or banner.
#[derive(Clone, Debug, Default)]
pub enum PictureEdit {
    /// It stays as Discord has it.
    #[default]
    Keep,
    /// It goes, and the fallback's shows: the main profile's in a server,
    /// Discord's default avatar or none in the main profile.
    Reset,
    New(PickedPicture),
}

/// A banner colour that isn't one, as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotAColour;

/// The editor of this account's profile.
#[derive(Clone, Debug, Default)]
pub struct ProfileEditor {
    /// The server's id and name, or none for the main profile.
    pub server: Option<String>,
    pub server_name: String,
    /// What Discord has; none while it sends it.
    pub profile: Option<OwnProfile>,
    /// As written, starting from the profile.
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    /// The banner colour, as `#rrggbb`, or empty for Discord's default.
    pub colour: String,
    pub avatar: PictureEdit,
    pub banner: PictureEdit,
    /// A picture being read, for the avatar or the banner.
    pub reading: Option<(ProfilePicture, PathBuf)>,
    /// Whether this account may change its nickname in the server.
    pub may_rename: bool,
    /// This account's Nitro, as `premium_type` gives it: 0 for none.
    pub premium: u8,
    pub saving: bool,
    pub error: Option<String>,
}

impl ProfileEditor {
    /// Waiting for Discord to send the profile.
    pub fn loading(&self) -> bool {
        self.profile.is_none() && self.error.is_none()
    }

    /// It edits the main profile, not one server's.
    pub fn main(&self) -> bool {
        self.server.is_none()
    }

    /// Whether this account has Nitro itself, not Basic or Classic, which a
    /// banner and a server's avatar and bio take: Discord's client checks
    /// `canUsePremiumProfileCustomization`.
    pub fn nitro(&self) -> bool {
        self.premium == 2
    }

    /// Whether it has a lesser Nitro, which Discord asks it to upgrade.
    pub fn upgrade(&self) -> bool {
        self.premium != 0 && !self.nitro()
    }

    /// Whether it may set an animated avatar, which Classic may too.
    pub fn animated(&self) -> bool {
        matches!(self.premium, 1 | 2)
    }

    /// Whether this account may change a text. Discord shows the banner
    /// colour only without Nitro, which sets a banner instead.
    pub fn may_write(&self, field: ProfileText) -> bool {
        match field {
            ProfileText::Name => self.main() || self.may_rename,
            ProfileText::Pronouns => true,
            ProfileText::Bio => self.main() || self.nitro(),
            ProfileText::Colour => self.main() && !self.nitro(),
        }
    }

    /// Whether this account may change a picture.
    pub fn may_picture(&self, kind: ProfilePicture) -> bool {
        match kind {
            ProfilePicture::Avatar => self.main() || self.nitro(),
            ProfilePicture::Banner => self.nitro(),
        }
    }

    /// The avatar or banner as it would be saved: its address, a new one's
    /// preview, or neither when the fallback's shows.
    pub fn picture(&self, kind: ProfilePicture) -> (Option<&str>, Option<&Arc<Picture>>) {
        let (edit, kept) = match kind {
            ProfilePicture::Avatar => (
                &self.avatar,
                self.profile.as_ref().and_then(|p| p.avatar.as_deref()),
            ),
            ProfilePicture::Banner => (
                &self.banner,
                self.profile.as_ref().and_then(|p| p.banner.as_deref()),
            ),
        };
        match edit {
            PictureEdit::Keep => (kept, None),
            PictureEdit::Reset => (None, None),
            PictureEdit::New(picked) => (None, Some(&picked.preview)),
        }
    }

    /// Whether there's an avatar or banner to reset or remove, as it stands
    /// or as chosen.
    pub fn resettable(&self, kind: ProfilePicture) -> bool {
        let (kept, new) = self.picture(kind);
        kept.is_some() || new.is_some()
    }

    /// The banner colour as written: none for Discord's default, and an
    /// error when it isn't a colour.
    pub fn accent(&self) -> Result<Option<u32>, NotAColour> {
        let colour = self.colour.trim();
        if colour.is_empty() {
            return Ok(None);
        }
        let hex = colour.strip_prefix('#').unwrap_or(colour);
        let hex = match hex.len() {
            3 => hex.chars().flat_map(|digit| [digit, digit]).collect(),
            6 => hex.to_owned(),
            _ => return Err(NotAColour),
        };
        u32::from_str_radix(&hex, 16)
            .map(Some)
            .map_err(|_| NotAColour)
    }

    /// A text that's longer than Discord takes.
    pub fn too_long(&self) -> Option<ProfileText> {
        [
            (ProfileText::Name, &self.name, NAME_LIMIT),
            (ProfileText::Pronouns, &self.pronouns, PRONOUNS_LIMIT),
            (ProfileText::Bio, &self.bio, BIO_LIMIT),
        ]
        .into_iter()
        .find(|(_, text, limit)| text.chars().count() > *limit)
        .map(|(field, _, _)| field)
    }

    /// What saving would change; none when nothing does.
    pub fn change(&self) -> Option<ProfileChange> {
        let profile = self.profile.as_ref()?;
        let text = |field: ProfileText, written: &str, kept: &str| {
            let written = written.trim();
            (self.may_write(field) && written != kept).then(|| written.to_owned())
        };
        let picture = |kind: ProfilePicture, edit: &PictureEdit, kept: &Option<String>| {
            let change = match edit {
                PictureEdit::Keep => None,
                PictureEdit::Reset => kept.is_some().then_some(None),
                PictureEdit::New(picked) => Some(Some(picked.clone())),
            };
            change.filter(|_| self.may_picture(kind))
        };
        let change = ProfileChange {
            name: text(ProfileText::Name, &self.name, &profile.name),
            pronouns: text(ProfileText::Pronouns, &self.pronouns, &profile.pronouns),
            bio: text(ProfileText::Bio, &self.bio, &profile.bio),
            avatar: picture(ProfilePicture::Avatar, &self.avatar, &profile.avatar),
            banner: picture(ProfilePicture::Banner, &self.banner, &profile.banner),
            accent: self
                .accent()
                .ok()
                .filter(|accent| self.may_write(ProfileText::Colour) && *accent != profile.accent),
        };
        let some = change.name.is_some()
            || change.pronouns.is_some()
            || change.bio.is_some()
            || change.avatar.is_some()
            || change.banner.is_some()
            || change.accent.is_some();
        some.then_some(change)
    }
}

/// A colour as the editor writes it.
fn colour_text(accent: Option<u32>) -> String {
    accent
        .map(|accent| format!("#{accent:06x}"))
        .unwrap_or_default()
}

impl Application {
    /// Opens the editor of this account's main profile, as Edit Profile
    /// does, and asks Discord for it.
    pub(super) fn edit_profile(&mut self) -> Vec<Effect> {
        if self.state.discord_user.is_none() {
            return vec![];
        }
        self.state.profile_editor = Some(ProfileEditor {
            premium: self.premium_type(),
            ..Default::default()
        });
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::OwnProfile { server: None },
        ))]
    }

    /// Opens the editor of this account's profile in a Discord server, by
    /// its server id or one of its channels' room ids, and asks Discord for
    /// the profile.
    pub(super) fn edit_server_profile(&mut self, place: String) -> Vec<Effect> {
        let guild = discord::parse_server(&place)
            .or_else(|| discord::parse_room(&place).map(|(guild, _)| guild));
        let Some(server) = guild.map(discord::server_id) else {
            return vec![];
        };
        if !self
            .menu_choices(&server)
            .contains(&MenuChoice::EditServerProfile)
        {
            return vec![];
        }
        let server_name = self
            .state
            .servers
            .iter()
            .find(|known| known.id == server)
            .map(|known| known.name.clone())
            .unwrap_or_default();
        self.state.profile_editor = Some(ProfileEditor {
            may_rename: self.may(&server, Permission::ChangeNickname)
                || self.may(&server, Permission::ManageNicknames),
            premium: self.premium_type(),
            server: Some(server.clone()),
            server_name,
            ..Default::default()
        });
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::OwnProfile {
                server: Some(server),
            },
        ))]
    }

    /// This account's Nitro, as `premium_type` gives it: 0 for none.
    fn premium_type(&self) -> u8 {
        #[cfg(feature = "discord")]
        if let Some(account) = self
            .discord
            .as_ref()
            .and_then(|snapshot| snapshot.account.as_ref())
        {
            return account.premium_type;
        }
        0
    }

    pub(super) fn own_profile_loaded(
        &mut self,
        server: Option<String>,
        result: Result<OwnProfile, String>,
    ) {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| editor.server == server && editor.loading())
        else {
            return;
        };
        match result {
            Ok(profile) => {
                editor.name.clone_from(&profile.name);
                editor.pronouns.clone_from(&profile.pronouns);
                editor.bio.clone_from(&profile.bio);
                editor.colour = colour_text(profile.accent);
                editor.profile = Some(profile);
            }
            Err(error) => editor.error = Some(error),
        }
    }

    pub(super) fn write_profile(&mut self, field: ProfileText, text: String) {
        if let Some(editor) = self.state.profile_editor.as_mut() {
            *match field {
                ProfileText::Name => &mut editor.name,
                ProfileText::Pronouns => &mut editor.pronouns,
                ProfileText::Bio => &mut editor.bio,
                ProfileText::Colour => &mut editor.colour,
            } = text;
        }
    }

    /// Reads a picture chosen for the avatar or banner.
    pub(super) fn pick_profile_picture(
        &mut self,
        kind: ProfilePicture,
        path: PathBuf,
    ) -> Vec<Effect> {
        let Some(editor) = self.state.profile_editor.as_mut().filter(|editor| {
            editor.may_picture(kind) && editor.profile.is_some() && !editor.saving
        }) else {
            return vec![];
        };
        editor.reading = Some((kind, path.clone()));
        editor.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ProfilePicture { kind, path },
        ))]
    }

    pub(super) fn profile_picture_read(
        &mut self,
        kind: ProfilePicture,
        path: PathBuf,
        result: Result<PickedPicture, String>,
    ) {
        let Some(editor) = self.state.profile_editor.as_mut().filter(|editor| {
            editor
                .reading
                .as_ref()
                .is_some_and(|reading| *reading == (kind, path.clone()))
        }) else {
            return;
        };
        editor.reading = None;
        match result {
            // Discord's client offers Nitro instead of an animated avatar.
            Ok(picked)
                if kind == ProfilePicture::Avatar
                    && picked.mime == "image/gif"
                    && !editor.animated() =>
            {
                editor.error = Some("An animated avatar takes Nitro.".into());
            }
            Ok(picked) => match kind {
                ProfilePicture::Avatar => editor.avatar = PictureEdit::New(picked),
                ProfilePicture::Banner => editor.banner = PictureEdit::New(picked),
            },
            Err(error) => editor.error = Some(error),
        }
    }

    /// Goes back to the fallback's avatar or banner, as Discord's Reset
    /// Avatar in a server and Remove Avatar in the main profile do.
    pub(super) fn reset_profile_picture(&mut self, kind: ProfilePicture) {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| editor.may_picture(kind))
        else {
            return;
        };
        let kept = editor.profile.as_ref().and_then(|profile| match kind {
            ProfilePicture::Avatar => profile.avatar.as_ref(),
            ProfilePicture::Banner => profile.banner.as_ref(),
        });
        let edit = if kept.is_some() {
            PictureEdit::Reset
        } else {
            PictureEdit::Keep
        };
        match kind {
            ProfilePicture::Avatar => editor.avatar = edit,
            ProfilePicture::Banner => editor.banner = edit,
        }
    }

    /// Puts back what Discord has, dropping a picture being read.
    pub(super) fn undo_profile_edits(&mut self) {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| !editor.saving)
        else {
            return;
        };
        let Some(profile) = &editor.profile else {
            return;
        };
        editor.name.clone_from(&profile.name);
        editor.pronouns.clone_from(&profile.pronouns);
        editor.bio.clone_from(&profile.bio);
        editor.colour = colour_text(profile.accent);
        editor.avatar = PictureEdit::Keep;
        editor.banner = PictureEdit::Keep;
        editor.reading = None;
        editor.error = None;
    }

    pub(super) fn save_profile(&mut self) -> Vec<Effect> {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| !editor.saving && editor.reading.is_none())
        else {
            return vec![];
        };
        if editor.too_long().is_some()
            || (editor.may_write(ProfileText::Colour) && editor.accent().is_err())
        {
            return vec![];
        }
        let Some(change) = editor.change() else {
            // Nothing to save closes it, as nothing changed.
            self.state.profile_editor = None;
            return vec![];
        };
        editor.saving = true;
        editor.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SaveProfile {
                server: editor.server.clone(),
                change: Box::new(change),
            },
        ))]
    }

    pub(super) fn profile_saved(&mut self, server: Option<String>, result: Result<(), String>) {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| editor.saving && editor.server == server)
        else {
            return;
        };
        match result {
            // A server member's change comes back with GUILD_MEMBER_UPDATE,
            // the main profile's name and avatar with USER_UPDATE; the rest
            // shows in the profile, fetched again.
            Ok(()) => self.state.profile_editor = None,
            Err(error) => {
                editor.saving = false;
                editor.error = Some(error);
            }
        }
    }

    /// Opens Discord's Nitro page, from the editor's offer.
    pub(super) fn unlock_nitro(&self) -> Vec<Effect> {
        if self.state.profile_editor.is_none() {
            return vec![];
        }
        vec![Effect::OpenPage(NITRO_PAGE.into())]
    }
}
