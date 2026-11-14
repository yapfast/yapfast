//! Emoji and Stickers, as Discord's server settings have them: the
//! server's emoji, static and animated, each renamed or deleted at once,
//! and its stickers in boost levels' slots; a new one comes from a file
//! chosen, with no editor. Members with Manage Expressions may change
//! every one, those with Create Expressions only their own.

use super::*;
use crate::chat::{
    EmojiAction, GuildSettings, PickedPicture, ServerEmoji, ServerSticker, StickerAction,
    StickerForm,
};

/// Each kind's emoji slots without boosts, and with `MORE_EMOJI`.
pub const EMOJI_SLOTS: usize = 50;
const MORE_EMOJI_SLOTS: usize = 200;
/// The slots by boost level when Discord doesn't send `premium_features`.
const LEVEL_EMOJI_SLOTS: [usize; 4] = [50, 100, 150, 250];
pub const EMOJI_NAME_LIMIT: usize = 32;

/// Sticker slots up to each boost level, and level 3's with
/// `MORE_STICKERS`; the boosts each level takes.
pub const STICKER_SLOTS: [usize; 4] = [5, 15, 30, 60];
const MORE_STICKER_SLOTS: usize = 120;
pub const LEVEL_BOOSTS: [u32; 4] = [0, 2, 7, 14];
pub const STICKER_NAME_LIMIT: usize = 30;
pub const STICKER_DESCRIPTION_LIMIT: usize = 100;
/// Discord's API takes up to 200 characters of related emoji.
const STICKER_TAGS_LIMIT: usize = 200;

const NO_SLOTS: &str = "This server does not have any open emoji slots!";

/// Something done on Emoji. Upload Emoji chooses a file, which
/// [`Action::PickServerPicture`] reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmojiInput {
    Rename {
        id: String,
        name: String,
    },
    Delete(String),
    /// The new emoji's name, as typed.
    Name(String),
    Finish,
    Cancel,
}

/// Something done on Stickers. Upload Sticker opens the form, whose file
/// [`Action::PickServerPicture`] reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StickerInput {
    Upload,
    Edit(String),
    Name(String),
    Tags(String),
    Description(String),
    Save,
    Cancel,
    Delete(String),
}

/// A new emoji, as its dialog has it.
#[derive(Clone, Debug, Default)]
pub struct EmojiUpload {
    pub picture: Option<PickedPicture>,
    pub name: String,
    pub saving: bool,
    pub error: Option<String>,
}

impl EmojiUpload {
    pub fn animated(&self) -> bool {
        self.picture
            .as_ref()
            .is_some_and(|picture| picture.mime == "image/gif")
    }
}

/// Emoji, as Discord sent them.
#[derive(Clone, Debug)]
pub struct EmojiPage {
    /// In Discord's order, oldest first.
    pub emojis: Vec<ServerEmoji>,
    pub settings: GuildSettings,
    pub upload: Option<EmojiUpload>,
    /// Requests sent whose answers haven't come: the list waits for the
    /// last, so one answer doesn't undo another's change.
    pending: usize,
    /// The snapshot's emoji when the list came: a new list means
    /// GUILD_EMOJIS_UPDATE, and the page asks again, as Discord's does.
    #[cfg(feature = "discord")]
    pub(super) seen: Option<Arc<Vec<yapfast_discord::ready::Emoji>>>,
}

impl EmojiPage {
    pub fn new(emojis: Vec<ServerEmoji>, settings: GuildSettings) -> Self {
        Self {
            emojis,
            settings,
            upload: None,
            pending: 0,
            #[cfg(feature = "discord")]
            seen: None,
        }
    }

    /// Each kind's slots, as Discord's client counts them.
    pub fn slots(&self) -> usize {
        let base = if self.settings.has("MORE_EMOJI") {
            MORE_EMOJI_SLOTS
        } else {
            EMOJI_SLOTS
        };
        let boosted = match self.settings.extra_emoji_slots {
            Some(extra) => EMOJI_SLOTS + usize::from(extra),
            None => LEVEL_EMOJI_SLOTS[usize::from(self.settings.premium_tier.min(3))],
        };
        base.max(boosted)
    }

    pub fn count(&self, animated: bool) -> usize {
        self.emojis
            .iter()
            .filter(|emoji| emoji.animated == animated)
            .count()
    }

    pub fn free(&self, animated: bool) -> usize {
        self.slots().saturating_sub(self.count(animated))
    }

    /// A list Discord sent, unless more answers are still to come.
    fn answered(&mut self, emojis: Option<Vec<ServerEmoji>>) {
        self.pending = self.pending.saturating_sub(1);
        if let Some(emojis) = emojis.filter(|_| self.pending == 0) {
            self.emojis = emojis;
        }
    }
}

/// An emoji's name as Discord's client makes it: only letters, digits and
/// `_`, at most 32, and at least 2, padded with `_`.
pub fn emoji_name(name: &str) -> String {
    let mut name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(EMOJI_NAME_LIMIT)
        .collect();
    while name.len() < 2 {
        name.push('_');
    }
    name
}

/// A sticker's form, while it's open.
#[derive(Clone, Debug, Default)]
pub struct StickerDraft {
    /// The sticker edited; none for a new one.
    pub editing: Option<String>,
    pub form: StickerForm,
    pub file: Option<PickedPicture>,
    /// The chosen file's name.
    pub file_name: String,
    pub saving: bool,
    pub error: Option<String>,
}

impl StickerDraft {
    /// Whether it may be sent, as Discord's form decides: a name of 2 to
    /// 30, a related emoji, a description empty or of 2 to 100, and a file
    /// for a new one.
    pub fn ready(&self) -> bool {
        let form = &self.form;
        let name = form.name.trim().chars().count();
        let description = form.description.trim().chars().count();
        !self.saving
            && (2..=STICKER_NAME_LIMIT).contains(&name)
            && !form.tags.trim().is_empty()
            && (description == 0 || (2..=STICKER_DESCRIPTION_LIMIT).contains(&description))
            && (self.editing.is_some() || self.file.is_some())
    }
}

/// Stickers, as Discord sent them.
#[derive(Clone, Debug)]
pub struct StickersPage {
    /// In Discord's order, which fills the levels' slots.
    pub stickers: Vec<ServerSticker>,
    pub settings: GuildSettings,
    pub draft: Option<StickerDraft>,
    pending: usize,
}

/// A boost level's card on Stickers: the stickers in its slots, how many
/// it has, and whether the server's level unlocks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerCard<'a> {
    pub level: usize,
    pub stickers: &'a [ServerSticker],
    pub slots: usize,
    pub unlocked: bool,
    /// The next sticker goes here.
    pub next: bool,
}

impl StickersPage {
    pub fn new(stickers: Vec<ServerSticker>, settings: GuildSettings) -> Self {
        Self {
            stickers,
            settings,
            draft: None,
            pending: 0,
        }
    }

    /// The server's boost level, which `MORE_STICKERS` takes to 3.
    pub fn level(&self) -> usize {
        if self.settings.has("MORE_STICKERS") {
            3
        } else {
            usize::from(self.settings.premium_tier.min(3))
        }
    }

    /// The slots up to `level`.
    fn up_to(&self, level: usize) -> usize {
        if level == 3 && self.settings.has("MORE_STICKERS") {
            MORE_STICKER_SLOTS
        } else {
            STICKER_SLOTS[level]
        }
    }

    pub fn slots(&self) -> usize {
        self.up_to(self.level())
    }

    pub fn full(&self) -> bool {
        self.stickers.len() >= self.slots()
    }

    /// Discord's four cards, Free Slots and Levels 1 to 3, each holding the
    /// stickers in its slots by their place in the list.
    pub fn cards(&self) -> Vec<StickerCard<'_>> {
        let level = self.level();
        let count = self.stickers.len();
        (0..4)
            .map(|card| {
                let start = if card == 0 { 0 } else { self.up_to(card - 1) };
                let end = self.up_to(card);
                let unlocked = card <= level;
                StickerCard {
                    level: card,
                    stickers: &self.stickers[start.min(count)..end.min(count)],
                    slots: end - start,
                    unlocked,
                    next: unlocked && (start..end).contains(&count),
                }
            })
            .collect()
    }

    /// A field of the open form, as typed, cut to its field's length.
    fn write(&mut self, field: fn(&mut StickerForm) -> &mut String, text: String, limit: usize) {
        if let Some(draft) = self.draft.as_mut().filter(|draft| !draft.saving) {
            *field(&mut draft.form) = text.chars().take(limit).collect();
        }
    }

    fn answered(&mut self, stickers: Option<Vec<ServerSticker>>) {
        self.pending = self.pending.saturating_sub(1);
        if let Some(stickers) = stickers.filter(|_| self.pending == 0) {
            self.follow(stickers);
        }
    }

    /// A new list, keeping who uploaded each sticker it already knew:
    /// GUILD_STICKERS_UPDATE doesn't say.
    pub(super) fn follow(&mut self, mut stickers: Vec<ServerSticker>) {
        for sticker in stickers
            .iter_mut()
            .filter(|sticker| sticker.user_id.is_none())
        {
            if let Some(known) = self.stickers.iter().find(|known| known.id == sticker.id) {
                sticker.user_id = known.user_id.clone();
                sticker.user = known.user.clone();
            }
        }
        self.stickers = stickers;
    }
}

/// Whether this account may change an emoji or sticker uploaded by
/// `uploader`: any with Manage Expressions, its own with Create
/// Expressions.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExpressionRules<'a> {
    pub manage: bool,
    pub create: bool,
    pub me: Option<&'a str>,
}

impl ExpressionRules<'_> {
    pub fn may_change(&self, uploader: Option<&str>) -> bool {
        self.manage || (self.create && uploader.is_some() && uploader == self.me)
    }
}

impl Application {
    /// What this account may do on a server's Emoji and Stickers.
    pub fn expression_rules(&self, server: &str) -> ExpressionRules<'_> {
        ExpressionRules {
            manage: self.may(server, Permission::ManageExpressions),
            create: self.may(server, Permission::CreateExpressions),
            me: self.state.discord_user.as_deref(),
        }
    }

    /// Something done on Emoji.
    pub(in crate::application) fn emoji_input(&mut self, input: EmojiInput) -> Vec<Effect> {
        let Some(server) = self.settings_server() else {
            return vec![];
        };
        let owners = |page: &EmojiPage, id: &str| {
            page.emojis
                .iter()
                .find(|emoji| emoji.id == id)
                .map(|emoji| emoji.user.as_ref().map(|user| user.id.clone()))
        };
        let rules = self.expression_rules(&server);
        let (manage, create, me) = (rules.manage, rules.create, rules.me.map(str::to_owned));
        let may = |owner: Option<Option<String>>| {
            let rules = ExpressionRules {
                manage,
                create,
                me: me.as_deref(),
            };
            owner.is_some_and(|owner| rules.may_change(owner.as_deref()))
        };
        let Some(page) = self.emoji_page() else {
            return vec![];
        };
        let action = match input {
            EmojiInput::Rename { id, name } => {
                let name = emoji_name(name.trim());
                let Some(emoji) = page
                    .emojis
                    .iter_mut()
                    .find(|emoji| emoji.id == id && emoji.name != name)
                else {
                    return vec![];
                };
                if !may(Some(emoji.user.as_ref().map(|user| user.id.clone()))) {
                    return vec![];
                }
                emoji.name = name.clone();
                EmojiAction::Rename { id, name }
            }
            EmojiInput::Delete(id) => {
                if !may(owners(page, &id)) {
                    return vec![];
                }
                // Gone at once, as in Discord's client.
                page.emojis.retain(|emoji| emoji.id != id);
                EmojiAction::Delete(id)
            }
            EmojiInput::Name(name) => {
                if let Some(upload) = page.upload.as_mut().filter(|upload| !upload.saving) {
                    // Discord's field makes spaces `_` as they're typed.
                    upload.name = name.replace(char::is_whitespace, "_");
                }
                return vec![];
            }
            EmojiInput::Cancel => {
                if page.upload.as_ref().is_some_and(|upload| !upload.saving) {
                    page.upload = None;
                }
                return vec![];
            }
            EmojiInput::Finish => {
                let free = page
                    .upload
                    .as_ref()
                    .map(|upload| page.free(upload.animated()));
                let Some(upload) = page
                    .upload
                    .as_mut()
                    .filter(|upload| !upload.saving && upload.name.chars().count() >= 2)
                else {
                    return vec![];
                };
                let Some(picture) = upload.picture.clone() else {
                    return vec![];
                };
                if free == Some(0) {
                    upload.error = Some(NO_SLOTS.to_owned());
                    return vec![];
                }
                upload.name = emoji_name(&upload.name);
                upload.saving = true;
                upload.error = None;
                EmojiAction::Upload {
                    name: upload.name.clone(),
                    picture,
                }
            }
        };
        page.pending += 1;
        vec![Effect::Backend(Command::Discord(DiscordCommand::Emoji {
            server,
            action,
        }))]
    }

    pub(in crate::application) fn emojis_changed(
        &mut self,
        server: String,
        upload: bool,
        result: Result<(), String>,
        emojis: Option<Vec<ServerEmoji>>,
    ) {
        if self.settings_server().as_deref() != Some(&server) {
            return;
        }
        #[cfg(feature = "discord")]
        let seen = self
            .discord_place(&server)
            .map(|(known, _, _)| known.emojis.clone());
        let mut error = None;
        let Some(page) = self.emoji_page() else {
            return;
        };
        #[cfg(feature = "discord")]
        if emojis.is_some() {
            page.seen = seen;
        }
        page.answered(emojis);
        match (upload, result) {
            (true, Ok(())) => page.upload = None,
            (true, Err(why)) => {
                if let Some(upload) = &mut page.upload {
                    upload.saving = false;
                    upload.error = Some(why);
                }
            }
            (false, result) => error = result.err(),
        }
        if let Some(settings) = &mut self.state.server_settings {
            settings.error = error;
        }
    }

    /// Something done on Stickers.
    pub(in crate::application) fn sticker_input(&mut self, input: StickerInput) -> Vec<Effect> {
        let Some(server) = self.settings_server() else {
            return vec![];
        };
        let rules = self.expression_rules(&server);
        let (manage, create, me) = (rules.manage, rules.create, rules.me.map(str::to_owned));
        let rules = ExpressionRules {
            manage,
            create,
            me: me.as_deref(),
        };
        let Some(page) = self.stickers_page() else {
            return vec![];
        };
        let owner = |page: &StickersPage, id: &str| {
            page.stickers
                .iter()
                .find(|sticker| sticker.id == id)
                .map(|sticker| sticker.user_id.clone())
        };
        let saving = page.draft.as_ref().is_some_and(|draft| draft.saving);
        let action = match input {
            StickerInput::Upload => {
                if (manage || create) && !page.full() && !saving {
                    page.draft = Some(StickerDraft::default());
                }
                return vec![];
            }
            StickerInput::Edit(id) => {
                let may = owner(page, &id).is_some_and(|owner| rules.may_change(owner.as_deref()));
                let sticker = page.stickers.iter().find(|sticker| sticker.id == id);
                if let Some(sticker) = sticker.filter(|_| may && !saving) {
                    page.draft = Some(StickerDraft {
                        editing: Some(id),
                        form: sticker.form.clone(),
                        ..StickerDraft::default()
                    });
                }
                return vec![];
            }
            StickerInput::Name(text) => {
                page.write(|form| &mut form.name, text, STICKER_NAME_LIMIT);
                return vec![];
            }
            StickerInput::Tags(text) => {
                page.write(|form| &mut form.tags, text, STICKER_TAGS_LIMIT);
                return vec![];
            }
            StickerInput::Description(text) => {
                page.write(
                    |form| &mut form.description,
                    text,
                    STICKER_DESCRIPTION_LIMIT,
                );
                return vec![];
            }
            StickerInput::Cancel => {
                if !saving {
                    page.draft = None;
                }
                return vec![];
            }
            StickerInput::Save => {
                let Some(draft) = page.draft.as_mut().filter(|draft| draft.ready()) else {
                    return vec![];
                };
                draft.saving = true;
                draft.error = None;
                let form = StickerForm {
                    name: draft.form.name.trim().to_owned(),
                    tags: draft.form.tags.trim().to_owned(),
                    description: draft.form.description.trim().to_owned(),
                };
                match (&draft.editing, &draft.file) {
                    (Some(id), _) => StickerAction::Edit {
                        id: id.clone(),
                        form,
                    },
                    (None, Some(file)) => StickerAction::Upload {
                        form,
                        file: file.clone(),
                    },
                    (None, None) => return vec![],
                }
            }
            StickerInput::Delete(id) => {
                if !owner(page, &id).is_some_and(|owner| rules.may_change(owner.as_deref())) {
                    return vec![];
                }
                page.stickers.retain(|sticker| sticker.id != id);
                StickerAction::Delete(id)
            }
        };
        page.pending += 1;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::Stickers {
                server,
                action: Box::new(action),
            },
        ))]
    }

    pub(in crate::application) fn stickers_changed(
        &mut self,
        server: String,
        form: bool,
        result: Result<(), String>,
        stickers: Option<Vec<ServerSticker>>,
    ) {
        if self.settings_server().as_deref() != Some(&server) {
            return;
        }
        let mut error = None;
        let Some(page) = self.stickers_page() else {
            return;
        };
        page.answered(stickers);
        match (form, result) {
            (true, Ok(())) => page.draft = None,
            (true, Err(why)) => {
                if let Some(draft) = &mut page.draft {
                    draft.saving = false;
                    draft.error = Some(why);
                }
            }
            (false, result) => error = result.err(),
        }
        if let Some(settings) = &mut self.state.server_settings {
            settings.error = error;
        }
    }

    /// A server's stickers changed elsewhere, or by this account.
    pub(in crate::application) fn server_stickers(
        &mut self,
        server: String,
        stickers: Vec<ServerSticker>,
    ) {
        self.follow_picker_stickers(&server, &stickers);
        if self.settings_server().as_deref() != Some(&server) {
            return;
        }
        if let Some(page) = self.stickers_page().filter(|page| page.pending == 0) {
            page.follow(stickers);
        }
    }

    /// Emoji asks Discord again once the snapshot's emoji changed, as
    /// GUILD_EMOJIS_UPDATE changes them, unless its own requests will
    /// bring the list anyway. A page just loaded only notes them.
    #[cfg(feature = "discord")]
    pub(super) fn follow_emojis(&mut self) -> Vec<Effect> {
        let Some(server) = self.settings_server() else {
            return vec![];
        };
        let Some(now) = self
            .discord_place(&server)
            .map(|(known, _, _)| known.emojis.clone())
        else {
            return vec![];
        };
        let Some(page) = self.emoji_page() else {
            return vec![];
        };
        let changed = page
            .seen
            .as_ref()
            .is_some_and(|seen| !Arc::ptr_eq(seen, &now));
        if page.seen.is_none() || page.pending > 0 {
            page.seen = Some(now);
            return vec![];
        }
        if !changed {
            return vec![];
        }
        page.seen = Some(now);
        page.pending += 1;
        vec![Effect::Backend(Command::Discord(DiscordCommand::Emoji {
            server,
            action: EmojiAction::Reload,
        }))]
    }

    fn settings_server(&self) -> Option<String> {
        self.state
            .server_settings
            .as_ref()
            .map(|settings| settings.server.clone())
    }

    fn emoji_page(&mut self) -> Option<&mut EmojiPage> {
        match self.state.server_settings.as_mut()?.page.as_mut()? {
            Page::Emoji(page) => Some(page),
            _ => None,
        }
    }

    fn stickers_page(&mut self) -> Option<&mut StickersPage> {
        match self.state.server_settings.as_mut()?.page.as_mut()? {
            Page::Stickers(page) => Some(page),
            _ => None,
        }
    }
}
