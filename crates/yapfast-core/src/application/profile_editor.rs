//! The editor of this account's Discord profile, as Discord's client has
//! it in two places: the main profile, from Edit Profile, with a display
//! name, pronouns, avatar, bio and a banner colour anyone may set and a
//! banner that takes Nitro; and the profile in one server, from a server's
//! "Edit server profile", with a nickname and pronouns anyone may set and
//! an avatar, banner and bio that take Nitro. See "Editing one's profile"
//! in docs/discord/people.md.

use super::*;
use crate::chat::{OwnProfile, Permission, PickedPicture, ProfileChange, ProfilePicture, Wishlist};

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
    /// The main profile's theme colours.
    ThemeTop,
    ThemeBottom,
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

/// What the editor held when it last saved, so that what Discord sends back
/// afterwards replaces only what wasn't changed since.
#[derive(Clone, Debug, Default)]
pub struct Sent {
    /// Saving went through; after a failure what Discord sends back is only
    /// what it has, and the editor keeps all that's written.
    pub saved: bool,
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    pub colour: String,
    pub theme: Option<[u32; 2]>,
    pub name_style: Option<crate::chat::NameStyle>,
    pub tag: Option<Option<String>>,
    /// [`ProfileEditor::picture_edits`] then.
    pub picture_edits: u32,
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
    /// The wishlist its tab shows, asked for when that tab first opens.
    pub wishlist: Wishlist,
    /// As written, starting from the profile.
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    /// The banner colour, as `#rrggbb`, or empty for Discord's default.
    pub colour: String,
    /// The theme colours and name colours as written, the name's font, and
    /// what Discord has of them, taken from the account's profile when it comes.
    pub theme_top: String,
    pub theme_bottom: String,
    pub name_font: Option<u8>,
    pub name_effect: u8,
    pub name_colours: Vec<u32>,
    pub kept_theme: Option<[u32; 2]>,
    pub kept_style: Option<crate::chat::NameStyle>,
    pub look_loaded: bool,
    /// The server tag chosen: none while it stays as it is, then the server's
    /// id, or none for no tag.
    pub chosen_tag: Option<Option<String>>,
    /// The server tag last saved here, which is no longer a change.
    pub kept_tag: Option<Option<String>>,
    /// Saved, or failed part way, and waiting for what Discord now has.
    pub refreshing: bool,
    /// What the last save sent.
    pub sent: Option<Sent>,
    /// How many times a picture was chosen, reset or put back, which says
    /// whether one changed since the save.
    pub picture_edits: u32,
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

    /// Asks Discord for the wishlist the profile says it has. Without a
    /// profile yet it stays loading, for [`Application::own_profile_loaded`];
    /// a profile without a wishlist has an empty one.
    fn ask_wishlist(&mut self) -> Vec<Effect> {
        let Some(profile) = self.profile.as_ref() else {
            self.wishlist = if self.loading() {
                Wishlist::Loading
            } else {
                Wishlist::Failed
            };
            return vec![];
        };
        let Some(place) = profile.fallback.wishlist_place.as_ref() else {
            self.wishlist = Wishlist::Loaded(Vec::new());
            return vec![];
        };
        let id = place.id.clone();
        self.wishlist = Wishlist::Loading;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::Wishlist { id },
        ))]
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
            ProfileText::ThemeTop | ProfileText::ThemeBottom => self.main() && self.nitro(),
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
        parse_colour(&self.colour)
    }

    /// The theme as written: none when both are empty; one colour alone is
    /// used for both.
    pub fn theme(&self) -> Result<Option<[u32; 2]>, NotAColour> {
        let (top, bottom) = (
            parse_colour(&self.theme_top)?,
            parse_colour(&self.theme_bottom)?,
        );
        Ok(match (top, bottom) {
            (Some(top), Some(bottom)) => Some([top, bottom]),
            (Some(one), None) | (None, Some(one)) => Some([one, one]),
            (None, None) => None,
        })
    }

    /// The name style as chosen: none with no font, effect or colour.
    pub fn name_style(&self) -> Option<crate::chat::NameStyle> {
        self.name_font.map(|font| {
            crate::chat::NameStyle::new(
                font,
                self.name_effect.max(1),
                if self.name_colours.is_empty() {
                    &[0]
                } else {
                    &self.name_colours
                },
            )
        })
    }

    /// Takes the theme and name style from the account's profile, once.
    pub fn fill_look(&mut self, profile: &crate::chat::Profile) {
        if self.look_loaded {
            return;
        }
        self.look_loaded = true;
        self.kept_theme = profile.theme;
        self.kept_style = profile.style.as_deref().copied();
        self.reset_look();
    }

    /// Puts the theme and name style back as Discord has them.
    fn reset_look(&mut self) {
        let [top, bottom] = self
            .kept_theme
            .map_or([None, None], |[top, bottom]| [Some(top), Some(bottom)]);
        self.theme_top = colour_text(top);
        self.theme_bottom = colour_text(bottom);
        let style = self.kept_style.as_ref();
        self.name_font = style.map(|style| style.font);
        self.name_effect = style.map_or(1, |style| style.effect);
        self.name_colours = style.map_or_else(Vec::new, |style| style.colors().to_vec());
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
        if self.refreshing {
            return None;
        }
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
            primary_server: self
                .chosen_tag
                .clone()
                .filter(|chosen| self.main() && Some(chosen) != self.kept_tag.as_ref()),
            theme: self
                .theme()
                .ok()
                .filter(|theme| self.may_write(ProfileText::ThemeTop) && *theme != self.kept_theme),
            name_style: {
                let style = self.name_style();
                (self.may_write(ProfileText::ThemeTop) && style != self.kept_style).then_some(style)
            },
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
            || change.accent.is_some()
            || change.theme.is_some()
            || change.primary_server.is_some()
            || change.name_style.is_some();
        some.then_some(change)
    }
}

/// A colour as written, as `#rgb` or `#rrggbb`: none when empty.
fn parse_colour(text: &str) -> Result<Option<u32>, NotAColour> {
    let colour = text.trim();
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
        // The theme and name style come with the profile it reads.
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

    /// Opens the editor's wishlist tab: asks Discord the first time, and again
    /// after a failure. While the profile is still on its way, that waits for
    /// it, which says where the wishlist is.
    pub(super) fn open_wishlist(&mut self) -> Vec<Effect> {
        let Some(editor) = self.state.profile_editor.as_mut() else {
            return vec![];
        };
        match editor.wishlist {
            Wishlist::Unasked | Wishlist::Failed | Wishlist::Hidden => editor.ask_wishlist(),
            Wishlist::Loading | Wishlist::Loaded(_) => vec![],
        }
    }

    pub(super) fn wishlist_loaded(&mut self, id: &str, wishlist: Wishlist) {
        self.profile_wishlist_loaded(id, &wishlist);
        let Some(editor) = self.state.profile_editor.as_mut() else {
            return;
        };
        let asked = editor
            .profile
            .as_ref()
            .and_then(|profile| profile.fallback.wishlist_place.as_ref())
            .is_some_and(|place| place.id == id);
        if asked {
            editor.wishlist = wishlist;
        }
    }

    pub(super) fn own_profile_loaded(
        &mut self,
        server: Option<String>,
        result: Result<OwnProfile, String>,
    ) -> Vec<Effect> {
        let Some(editor) =
            self.state.profile_editor.as_mut().filter(|editor| {
                editor.server == server && (editor.loading() || editor.refreshing)
            })
        else {
            return vec![];
        };
        let refreshed = std::mem::take(&mut editor.refreshing);
        // After a save, what was written since stays, and so does all that's
        // written when the save failed part way.
        let sent = if refreshed { editor.sent.take() } else { None };
        let unchanged = |written: &str, sent: Option<&str>| match sent {
            None => !refreshed,
            Some(sent) => written == sent,
        };
        let kept = sent.as_ref().filter(|sent| sent.saved);
        if kept.is_some_and(|sent| sent.picture_edits == editor.picture_edits) {
            editor.avatar = PictureEdit::Keep;
            editor.banner = PictureEdit::Keep;
        }
        match result {
            // What was saved stays in the editor if it can't be read back.
            Err(error) if refreshed => {
                tracing::warn!(error, "could not read back the saved profile");
            }
            Ok(profile) => {
                if unchanged(&editor.name, kept.map(|sent| sent.name.as_str())) {
                    editor.name.clone_from(&profile.name);
                }
                if unchanged(&editor.pronouns, kept.map(|sent| sent.pronouns.as_str())) {
                    editor.pronouns.clone_from(&profile.pronouns);
                }
                if unchanged(&editor.bio, kept.map(|sent| sent.bio.as_str())) {
                    editor.bio.clone_from(&profile.bio);
                }
                if unchanged(&editor.colour, kept.map(|sent| sent.colour.as_str())) {
                    editor.colour = colour_text(profile.accent);
                }
                // The theme and name style are the main profile's, which is
                // the fallback in a server.
                editor.fill_look(&profile.fallback);
                let mut profile = profile;
                // Read back after a save, its board keeps the names it has.
                if let Some(old) = editor
                    .profile
                    .as_mut()
                    .map(|old| &mut old.fallback)
                    .filter(|old| same_board(old, &profile.fallback))
                {
                    keep_board(&mut profile.fallback, old);
                }
                editor.profile = Some(profile);
            }
            Err(error) => editor.error = Some(error),
        }
        let mut effects = vec![];
        // The board's games are named once, as the full profile's are.
        if let Some(fallback) = editor
            .profile
            .as_mut()
            .map(|profile| &mut profile.fallback)
            .filter(|fallback| !fallback.board_asked && names_games(&fallback.widgets))
        {
            fallback.board_asked = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ProfileBoard {
                    user: fallback.id.clone(),
                    widgets: fallback.widgets.clone(),
                },
            )));
        }
        // The tab opened before the profile came, so its asking is due.
        if !refreshed && editor.wishlist == Wishlist::Loading {
            effects.extend(editor.ask_wishlist());
        }
        effects
    }

    /// Sets the name style its window chose; Discord's usual font with its
    /// usual white is no style at all. Colours are hex, joined by commas.
    pub(super) fn choose_name_style(&mut self, font: u8, effect: u8, colours: &str) {
        let Some(editor) = self.state.profile_editor.as_mut() else {
            return;
        };
        let colours: Vec<u32> = colours
            .split(',')
            .filter_map(|colour| parse_colour(colour).ok().flatten())
            .collect();
        if font == 11 && effect <= 1 && colours == [0xf0_f0_f2] {
            editor.name_font = None;
            editor.name_effect = 1;
            editor.name_colours.clear();
        } else {
            editor.name_font = Some(font);
            editor.name_effect = effect.max(1);
            editor.name_colours = colours;
        }
    }

    /// Picks the server whose tag to wear; an empty id for no tag.
    pub(super) fn choose_server_tag(&mut self, server: String) {
        if let Some(editor) = self.state.profile_editor.as_mut() {
            editor.chosen_tag = Some(Some(server).filter(|server| !server.is_empty()));
        }
    }

    pub(super) fn write_profile(&mut self, field: ProfileText, text: String) {
        if let Some(editor) = self.state.profile_editor.as_mut() {
            *match field {
                ProfileText::Name => &mut editor.name,
                ProfileText::Pronouns => &mut editor.pronouns,
                ProfileText::Bio => &mut editor.bio,
                ProfileText::Colour => &mut editor.colour,
                ProfileText::ThemeTop => &mut editor.theme_top,
                ProfileText::ThemeBottom => &mut editor.theme_bottom,
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
            Ok(picked) => {
                editor.picture_edits += 1;
                match kind {
                    ProfilePicture::Avatar => editor.avatar = PictureEdit::New(picked),
                    ProfilePicture::Banner => editor.banner = PictureEdit::New(picked),
                }
            }
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
        editor.picture_edits += 1;
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
        editor.reset_look();
        editor.chosen_tag = None;
        editor.avatar = PictureEdit::Keep;
        editor.banner = PictureEdit::Keep;
        editor.picture_edits += 1;
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
            || (editor.may_write(ProfileText::ThemeTop) && editor.theme().is_err())
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
        editor.sent = Some(Sent {
            saved: false,
            name: editor.name.clone(),
            pronouns: editor.pronouns.clone(),
            bio: editor.bio.clone(),
            colour: editor.colour.clone(),
            theme: editor.theme().ok().flatten(),
            name_style: editor.name_style(),
            tag: editor.chosen_tag.clone(),
            picture_edits: editor.picture_edits,
        });
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SaveProfile {
                server: editor.server.clone(),
                change: Box::new(change),
            },
        ))]
    }

    pub(super) fn profile_saved(
        &mut self,
        server: Option<String>,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let Some(editor) = self
            .state
            .profile_editor
            .as_mut()
            .filter(|editor| editor.saving && editor.server == server)
        else {
            return vec![];
        };
        editor.saving = false;
        editor.refreshing = true;
        let sent = editor.sent.get_or_insert_default();
        match result {
            // It stays open, with nothing left to save. A server member's
            // change comes back with GUILD_MEMBER_UPDATE, the main profile's
            // name and avatar with USER_UPDATE; the rest, its tag, name style
            // and theme among them, shows in the profile, fetched again. What
            // was written since the save stays a change.
            Ok(()) => {
                sent.saved = true;
                let sent = sent.clone();
                editor.error = None;
                editor.kept_tag = sent.tag.clone();
                editor.kept_theme = sent.theme;
                editor.kept_style = sent.name_style;
                if let Some(profile) = editor.profile.as_mut() {
                    profile.name = sent.name.trim().to_owned();
                    profile.pronouns = sent.pronouns.trim().to_owned();
                    profile.bio = sent.bio.trim().to_owned();
                    profile.accent = parse_colour(&sent.colour).ok().flatten();
                }
            }
            // A save is up to three requests, and those before the one that
            // failed went through. What Discord now has is asked for again,
            // as in the kept profiles, which the service dropped; the editor
            // keeps all that's written, so what didn't go through is still a
            // change.
            Err(error) => editor.error = Some(error),
        }
        // The account's card asks again as it next opens: the save dropped
        // the profile the service kept.
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::OwnProfile { server },
        ))]
    }

    /// Opens Discord's Nitro page, from the editor's offer.
    pub(super) fn unlock_nitro(&self) -> Vec<Effect> {
        if self.state.profile_editor.is_none() {
            return vec![];
        }
        vec![Effect::OpenPage(NITRO_PAGE.into())]
    }
}
