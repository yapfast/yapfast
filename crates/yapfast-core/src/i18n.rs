//! Interface strings in the chosen language: a small catalog, English complete
//! and other languages as far as translated, with English where a key has no
//! translation.

use std::sync::atomic::{AtomicU8, Ordering};

/// A language the interface can speak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    German,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::German];

    /// The language's name in itself, for the settings page.
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the language every [`tr`] answers in.
pub fn set_language(language: Language) {
    let index = Language::ALL
        .iter()
        .position(|&l| l == language)
        .unwrap_or(0);
    CURRENT.store(index as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    Language::ALL[usize::from(CURRENT.load(Ordering::Relaxed)).min(Language::ALL.len() - 1)]
}

/// The text for `key` in the current language.
pub fn tr(key: &str) -> &'static str {
    tr_in(language(), key)
}

/// The text for `key` in `language`, in English when it has none there; an
/// unknown key gives an empty text.
pub fn tr_in(language: Language, key: &str) -> &'static str {
    let found = match language {
        Language::English => None,
        Language::German => german(key),
    };
    found.or_else(|| english(key)).unwrap_or_default()
}

/// `message_hint` for `name`, such as `Message @sam`.
pub fn message_hint(name: &str) -> String {
    tr("message_hint").replace("{name}", name)
}

/// The composer's hint for a conversation: `Message @sam` in a DM, `Message
/// Group` in a group DM and `Message #general` in a channel; `named` is a
/// thread's or other room's own name, shown as it is.
pub fn composer_hint(name: &str, direct: bool, person: bool, channel: bool) -> String {
    let key = match (direct, person, channel) {
        (true, true, _) => "message_hint",
        (true, false, _) => "message_hint_group",
        (false, _, true) => "message_hint_channel",
        (false, _, false) => "message_hint_group",
    };
    tr(key).replace("{name}", name)
}

/// "1 mutual friend" or "5 mutual friends", in the current language.
pub fn mutual_friends(count: usize) -> String {
    let key = if count == 1 {
        "mutual_friend_one"
    } else {
        "mutual_friend_many"
    };
    tr(key).replace("{n}", &count.to_string())
}

/// "1 mutual server" or "2 mutual servers", in the current language.
pub fn mutual_servers(count: usize) -> String {
    let key = if count == 1 {
        "mutual_server_one"
    } else {
        "mutual_server_many"
    };
    tr(key).replace("{n}", &count.to_string())
}

#[cfg(test)]
pub(crate) const KEYS: &[&str] = &[
    "official_badge",
    "official_message",
    "wave",
    "inbox",
    "help",
    "download_apps",
    "nav_back",
    "nav_forward",
    "inbox_for_you",
    "inbox_unreads",
    "inbox_mentions",
    "inbox_empty",
    "inbox_friend_request",
    "inbox_mentioned",
    "start_voice_call",
    "start_video_call",
    "join_call",
    "message_hint_group",
    "message_hint_channel",
    "invite_people",
    "show_mutual",
    "roles",
    "connections",
    "note_title",
    "note_hint",
    "add_friend",
    "friends",
    "friend_sent",
    "accept_friend",
    "view_full_profile",
    "mutual_friend_one",
    "mutual_friend_many",
    "mutual_server_one",
    "mutual_server_many",
    "message_hint",
    "view_full_bio",
    "member_since",
    "tab_activity",
    "tab_mutual_friends",
    "tab_mutual_servers",
    "setting_date_format",
    "setting_time_zone",
    "setting_language",
    "date_dmy",
    "date_mdy",
    "date_ymd",
    "zone_system",
    "clock_today_at",
    "clock_yesterday_at",
    "clock_tomorrow_at",
    "clock_last_at",
    "clock_weekday_at",
    "clock_day_at",
    "composer_no_permission",
    "intro_welcome_server",
    "intro_server_start",
    "intro_welcome_channel",
    "intro_channel_start",
];

fn english(key: &str) -> Option<&'static str> {
    Some(match key {
        "show_mutual" => "Show mutual friends and servers in the full profile",
        "roles" => "Roles",
        "connections" => "Connections",
        "note_title" => "Note (only visible to you)",
        "note_hint" => "Click to add a note",
        "add_friend" => "Add Friend",
        "friends" => "Friends",
        "friend_sent" => "Friend Request Sent",
        "accept_friend" => "Accept Friend Request",
        "view_full_profile" => "View Full Profile",
        "mutual_friend_one" => "{n} mutual friend",
        "mutual_friend_many" => "{n} mutual friends",
        "mutual_server_one" => "{n} mutual server",
        "mutual_server_many" => "{n} mutual servers",
        "invite_people" => "Invite people",
        "message_hint_group" => "Message {name}",
        "message_hint_channel" => "Message #{name}",
        "start_voice_call" => "Start Voice Call",
        "start_video_call" => "Start Video Call",
        "join_call" => "Join the call",
        "inbox" => "Inbox",
        "help" => "Help",
        "download_apps" => "Download Apps",
        "nav_back" => "Back",
        "nav_forward" => "Forward",
        "inbox_for_you" => "For You",
        "inbox_unreads" => "Unreads",
        "inbox_mentions" => "Mentions",
        "inbox_empty" => "Nothing here right now.",
        "inbox_friend_request" => "sent you a friend request",
        "inbox_mentioned" => "mentions waiting",
        "official_badge" => "OFFICIAL",
        "official_message" => "Official Discord Message",
        "wave" => "Wave",
        "message_hint" => "Message @{name}",
        "view_full_bio" => "View full bio",
        "member_since" => "Member since",
        "tab_activity" => "Activity",
        "tab_mutual_friends" => "Mutual friends",
        "tab_mutual_servers" => "Mutual servers",
        "setting_date_format" => "DATE FORMAT",
        "setting_time_zone" => "TIME ZONE",
        "setting_language" => "LANGUAGE",
        "date_dmy" => "Day/Month/Year",
        "date_mdy" => "Month/Day/Year",
        "date_ymd" => "Year-Month-Day",
        "zone_system" => "System default",
        "clock_today_at" => "Today at {time}",
        "clock_yesterday_at" => "Yesterday at {time}",
        "clock_tomorrow_at" => "Tomorrow at {time}",
        "clock_last_at" => "Last {weekday} at {time}",
        "clock_weekday_at" => "{weekday} at {time}",
        "clock_day_at" => "{day} at {time}",
        "composer_no_permission" => "You do not have permission to send messages in this channel.",
        "intro_welcome_server" => "Welcome to",
        "intro_server_start" => "This is the beginning of this server.",
        "intro_welcome_channel" => "Welcome to #{name}!",
        "intro_channel_start" => "This is the start of the #{name} channel.",
        _ => return None,
    })
}

fn german(key: &str) -> Option<&'static str> {
    Some(match key {
        "show_mutual" => "Gemeinsame Freunde und Server im vollständigen Profil anzeigen",
        "roles" => "Rollen",
        "connections" => "Verknüpfungen",
        "note_title" => "Notiz (nur für dich sichtbar)",
        "note_hint" => "Hier klicken, um eine Notiz hinzuzufügen",
        "add_friend" => "Freund hinzufügen",
        "friends" => "Freunde",
        "friend_sent" => "Freundschaftsanfrage gesendet",
        "accept_friend" => "Freundschaftsanfrage annehmen",
        "view_full_profile" => "Vollständiges Profil anzeigen",
        "mutual_friend_one" => "{n} gemeinsamer Freund",
        "mutual_friend_many" => "{n} gemeinsame Freunde",
        "mutual_server_one" => "{n} gemeinsamer Server",
        "mutual_server_many" => "{n} gemeinsame Server",
        "invite_people" => "Mitglieder einladen",
        "message_hint_group" => "Nachricht an {name}",
        "message_hint_channel" => "Nachricht an #{name}",
        "start_voice_call" => "Sprachanruf starten",
        "start_video_call" => "Videoanruf starten",
        "join_call" => "Dem Anruf beitreten",
        "inbox" => "Posteingang",
        "help" => "Hilfe",
        "download_apps" => "Apps herunterladen",
        "nav_back" => "Zurück",
        "nav_forward" => "Vorwärts",
        "inbox_for_you" => "Für dich",
        "inbox_unreads" => "Ungelesen",
        "inbox_mentions" => "Erwähnungen",
        "inbox_empty" => "Gerade nichts zu sehen.",
        "inbox_friend_request" => "hat dir eine Freundschaftsanfrage gesendet",
        "inbox_mentioned" => "Erwähnungen warten",
        "official_badge" => "OFFIZIELL",
        "official_message" => "Offizielle Discord-Nachricht",
        "wave" => "Winken",
        "message_hint" => "Nachricht an @{name}",
        "view_full_bio" => "Vollständige Bio anzeigen",
        "member_since" => "Mitglied seit",
        "tab_activity" => "Aktivität",
        "tab_mutual_friends" => "Gemeinsame Freunde",
        "tab_mutual_servers" => "Gemeinsame Server",
        "setting_date_format" => "DATUMSFORMAT",
        "setting_time_zone" => "ZEITZONE",
        "setting_language" => "SPRACHE",
        "date_dmy" => "Tag/Monat/Jahr",
        "date_mdy" => "Monat/Tag/Jahr",
        "date_ymd" => "Jahr-Monat-Tag",
        "zone_system" => "Systemstandard",
        "clock_today_at" => "Heute um {time} Uhr",
        "clock_yesterday_at" => "Gestern um {time} Uhr",
        "clock_tomorrow_at" => "Morgen um {time} Uhr",
        "clock_last_at" => "Letzten {weekday} um {time} Uhr",
        "clock_weekday_at" => "{weekday} um {time} Uhr",
        "clock_day_at" => "{day} um {time} Uhr",
        "composer_no_permission" => {
            "Du hast keine Berechtigung, in diesem Kanal Nachrichten zu versenden."
        }
        "intro_welcome_server" => "Willkommen bei",
        "intro_server_start" => "Das ist der Anfang dieses Servers.",
        "intro_welcome_channel" => "Willkommen in #{name}!",
        "intro_channel_start" => "Das ist der Anfang des Kanals #{name}.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_german_key_exists_in_english() {
        for key in KEYS {
            assert!(english(key).is_some(), "English lacks {key}");
            if let Some(text) = german(key) {
                assert!(!text.is_empty(), "German {key} is empty");
            }
        }
    }

    #[test]
    fn missing_translations_fall_back_to_english() {
        assert_eq!(tr_in(Language::German, "no_such_key"), "");
        assert_eq!(
            tr_in(Language::German, "view_full_bio"),
            "Vollständige Bio anzeigen"
        );
        assert_eq!(tr_in(Language::English, "tab_activity"), "Activity");
    }

    #[test]
    fn language_is_process_wide_and_fills_names_in() {
        // The process-wide language is left alone, since other tests read it.
        assert_eq!(
            tr_in(Language::German, "message_hint").replace("{name}", "sam"),
            "Nachricht an @sam"
        );
        assert_eq!(message_hint("sam"), "Message @sam");
        assert_eq!(composer_hint("sam", true, true, false), "Message @sam");
        assert_eq!(composer_hint("Crew", true, false, false), "Message Crew");
        assert_eq!(composer_hint("dev", false, false, true), "Message #dev");
        assert_eq!(
            tr_in(Language::German, "message_hint_channel").replace("{name}", "dev"),
            "Nachricht an #dev"
        );
        assert_eq!(
            tr_in(Language::German, "mutual_friend_many").replace("{n}", "5"),
            "5 gemeinsame Freunde"
        );
        assert_eq!(
            tr_in(Language::English, "mutual_server_one").replace("{n}", "1"),
            "1 mutual server"
        );
    }
}
