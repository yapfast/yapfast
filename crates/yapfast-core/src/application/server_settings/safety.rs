//! Safety Setup: what members must have done before they may talk, whose
//! images Discord filters, and whether moderators need two-factor
//! authentication. As in Discord's client, each choice is saved as it's
//! made, without the bar: the first two with `PATCH /guilds/{id}`, the
//! last with `POST /guilds/{id}/mfa`.

use crate::chat::{GuildSettings, ServerSettingsChange};

/// The highest verification level, a verified phone.
pub const HIGHEST_VERIFICATION: u8 = 4;
/// The explicit content filter's widest, everyone's images.
pub const FILTER_ALL: u8 = 2;

/// A choice on Safety Setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyChoice {
    /// 0 none to 4 a verified phone.
    Verification(u8),
    /// 0 no one's images, 1 those without roles, 2 everyone's.
    ContentFilter(u8),
    /// Moderators need two-factor authentication.
    RequireMfa(bool),
}

/// Who may change whether moderators need two-factor authentication, as
/// Discord's client decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MfaRule {
    /// The owner, with two-factor authentication on.
    May,
    /// The owner, without it.
    OwnerWithoutMfa,
    /// Anyone else.
    NotOwner,
}

/// Safety Setup, and a choice Discord is taking.
#[derive(Clone, Debug)]
pub struct SafetyPage {
    /// What Discord has.
    pub settings: GuildSettings,
    pub choosing: Option<SafetyChoice>,
}

impl SafetyPage {
    pub(super) fn new(settings: GuildSettings) -> Self {
        Self {
            settings,
            choosing: None,
        }
    }

    /// A Community server, where Discord takes away the lowest choices.
    pub fn community(&self) -> bool {
        self.settings.has("COMMUNITY")
    }

    /// The verification level shown: the one Discord is taking, or its own.
    pub fn verification(&self) -> u8 {
        match self.choosing {
            Some(SafetyChoice::Verification(level)) => level,
            _ => self.settings.verification_level,
        }
    }

    pub fn content_filter(&self) -> u8 {
        match self.choosing {
            Some(SafetyChoice::ContentFilter(level)) => level,
            _ => self.settings.explicit_content_filter,
        }
    }

    pub fn requires_mfa(&self) -> bool {
        match self.choosing {
            Some(SafetyChoice::RequireMfa(on)) => on,
            _ => self.settings.mfa_level == 1,
        }
    }

    /// Whether Discord offers a verification level: a Community server
    /// can't have none.
    pub fn offers_verification(&self, level: u8) -> bool {
        level <= HIGHEST_VERIFICATION && (level > 0 || !self.community())
    }

    /// Whether Discord offers a filter: a Community server filters
    /// everyone's.
    pub fn offers_filter(&self, level: u8) -> bool {
        level <= FILTER_ALL && (level == FILTER_ALL || !self.community())
    }

    /// Why the two-factor switch can't move, as Discord's client says it,
    /// if it can't: a Discoverable server keeps it on.
    pub fn mfa_lock(&self, rule: MfaRule) -> Option<&'static str> {
        if self.settings.mfa_level == 1 && self.settings.has("DISCOVERABLE") {
            return Some(yapfast_lang::tr!(
                "2FA is required while this server is in Server Discovery"
            ));
        }
        match rule {
            MfaRule::May => None,
            MfaRule::OwnerWithoutMfa => Some(yapfast_lang::tr!(
                "To change this setting, first enable 2FA on your account."
            )),
            MfaRule::NotOwner => Some(yapfast_lang::tr!(
                "This setting can only be changed by a server owner with 2FA enabled on their account."
            )),
        }
    }

    /// What a choice sends, if Discord offers it and it changes anything.
    pub(super) fn choose(
        &self,
        choice: SafetyChoice,
        rule: MfaRule,
    ) -> Option<ServerSettingsChange> {
        let settings = &self.settings;
        let guild = |verification_level, explicit_content_filter| ServerSettingsChange::Guild {
            system_channel: None,
            system_flags: None,
            default_notifications: None,
            afk_channel: None,
            afk_timeout: None,
            banner: None,
            splash: None,
            progress_bar: None,
            verification_level,
            explicit_content_filter,
        };
        match choice {
            SafetyChoice::Verification(level)
                if self.offers_verification(level) && level != settings.verification_level =>
            {
                Some(guild(Some(level), None))
            }
            SafetyChoice::ContentFilter(level)
                if self.offers_filter(level) && level != settings.explicit_content_filter =>
            {
                Some(guild(None, Some(level)))
            }
            SafetyChoice::RequireMfa(on)
                if self.mfa_lock(rule).is_none() && on != (settings.mfa_level == 1) =>
            {
                Some(ServerSettingsChange::MfaLevel(u8::from(on)))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_community_server_keeps_its_lowest_choices_out_of_reach() {
        let mut page = SafetyPage::new(GuildSettings {
            verification_level: 1,
            ..Default::default()
        });
        assert!(page.offers_verification(0) && page.offers_filter(0));
        page.settings.features = vec!["COMMUNITY".into()];
        assert!(!page.offers_verification(0) && page.offers_verification(4));
        assert!(!page.offers_filter(1) && page.offers_filter(2));
        assert!(
            page.choose(SafetyChoice::Verification(0), MfaRule::May)
                .is_none()
        );
        assert!(matches!(
            page.choose(SafetyChoice::Verification(3), MfaRule::May),
            Some(ServerSettingsChange::Guild {
                verification_level: Some(3),
                explicit_content_filter: None,
                ..
            })
        ));
    }

    #[test]
    fn only_an_owner_with_2fa_changes_whether_moderators_need_it() {
        let mut page = SafetyPage::new(GuildSettings::default());
        let on = SafetyChoice::RequireMfa(true);
        assert!(page.choose(on, MfaRule::NotOwner).is_none());
        assert!(page.choose(on, MfaRule::OwnerWithoutMfa).is_none());
        assert!(matches!(
            page.choose(on, MfaRule::May),
            Some(ServerSettingsChange::MfaLevel(1))
        ));
        page.settings.mfa_level = 1;
        page.settings.features = vec!["DISCOVERABLE".into()];
        assert_eq!(
            page.mfa_lock(MfaRule::May),
            Some("2FA is required while this server is in Server Discovery")
        );
    }
}
