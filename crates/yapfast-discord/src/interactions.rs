//! Using a bot's message components and forms as this account, as Discord's
//! client does: the bodies of `POST /interactions`, which application a bot
//! message's controls belong to, and what a failed interaction's reason
//! says. See docs/discord/messages.md, "Bots' buttons, menus and forms".
//!
//! Every interaction starts from something the person did, a click or a
//! submit: nothing here retries or sends one on its own.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{Value, json};

/// How long an interaction stays pending without a word from the gateway,
/// once Discord took the request. Discord itself fails one whose app doesn't
/// answer in 3 seconds; this covers a gateway that dropped the answer, as
/// across a reconnect.
pub const UNANSWERED: Duration = Duration::from_secs(15);

/// The most bots whose application is kept.
const KEPT_APPS: usize = 256;

/// A use of a bot's control or form, as Yapfast sends it.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    /// A button clicked or options picked in a select menu under a message.
    Component {
        nonce: String,
        /// None in a DM.
        guild: Option<String>,
        channel: String,
        message: String,
        /// The message's flags, which Discord's client sends with it.
        message_flags: u64,
        /// The message's `application_id`, when Discord gave one.
        application: Option<String>,
        /// The message's author, the bot.
        author: String,
        /// 2 for a button, 3 and 5-8 for the select menus.
        component_type: u64,
        custom_id: String,
        /// The options picked, for a select menu.
        values: Option<Vec<String>>,
    },
    /// A form a bot opened, submitted.
    Modal {
        nonce: String,
        guild: Option<String>,
        channel: String,
        application: String,
        /// The id INTERACTION_MODAL_CREATE gave the form.
        id: String,
        custom_id: String,
        /// The form's components with what was entered, as Discord takes them.
        components: Value,
    },
}

impl Interaction {
    pub fn nonce(&self) -> &str {
        match self {
            Self::Component { nonce, .. } | Self::Modal { nonce, .. } => nonce,
        }
    }

    /// The body of `POST /interactions` in the gateway session `session`,
    /// with the application `app_ids` knows for a component's bot.
    pub fn body(&self, session: &str, app_ids: &AppIds) -> Value {
        match self {
            Self::Component {
                nonce,
                guild,
                channel,
                message,
                message_flags,
                application,
                author,
                component_type,
                custom_id,
                values,
            } => {
                let application = match application {
                    Some(application) => {
                        app_ids.learn(author, application);
                        application.clone()
                    }
                    None => app_ids.of(author),
                };
                let mut data = json!({"component_type": component_type, "custom_id": custom_id});
                if let Some(values) = values {
                    data["values"] = json!(values);
                }
                let mut body = json!({
                    "type": 3,
                    "nonce": nonce,
                    "application_id": application,
                    "channel_id": channel,
                    "message_flags": message_flags,
                    "message_id": message,
                    "session_id": session,
                    "data": data,
                });
                if let Some(guild) = guild {
                    body["guild_id"] = json!(guild);
                }
                body
            }
            Self::Modal {
                nonce,
                guild,
                channel,
                application,
                id,
                custom_id,
                components,
            } => {
                let mut body = json!({
                    "type": 5,
                    "nonce": nonce,
                    "application_id": application,
                    "channel_id": channel,
                    "session_id": session,
                    "data": {"id": id, "custom_id": custom_id, "components": components},
                });
                if let Some(guild) = guild {
                    body["guild_id"] = json!(guild);
                }
                body
            }
        }
    }
}

/// The application each bot user belongs to, as Discord said it: a message's
/// `application_id`, or the `application` of the bot's profile once someone
/// opened it. Yapfast asks for neither on its own. A bot not known here is
/// taken to be its own application, as an app's bot user has the
/// application's id: Discord's client does the same for a message without
/// `application_id` (from its web client; docs.discord.food doesn't say).
#[derive(Clone, Default, Debug)]
pub struct AppIds(Arc<Mutex<HashMap<String, String>>>);

impl AppIds {
    /// Keeps `application` as the bot `bot`'s.
    pub fn learn(&self, bot: &str, application: &str) {
        if bot.is_empty() || application.is_empty() {
            return;
        }
        let mut apps = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if apps.get(bot).map(String::as_str) == Some(application) {
            return;
        }
        // Few bots are used in a session; past the limit, start over.
        if apps.len() >= KEPT_APPS {
            apps.clear();
        }
        apps.insert(bot.to_owned(), application.to_owned());
    }

    /// The application of the bot `bot`.
    pub fn of(&self, bot: &str) -> String {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(bot)
            .cloned()
            .unwrap_or_else(|| bot.to_owned())
    }
}

/// What INTERACTION_FAILURE's `reason_code` means, in Discord's words.
pub fn failure_reason(code: u64) -> &'static str {
    match code {
        // TIMEOUT: the app didn't answer within 3 seconds.
        2 => "The application did not respond.",
        // UNKNOWN, and 3-20, which are about launching an activity.
        _ => "This interaction failed.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(application: Option<&str>) -> Interaction {
        Interaction::Component {
            nonce: "5".into(),
            guild: Some("1".into()),
            channel: "2".into(),
            message: "3".into(),
            message_flags: 0,
            application: application.map(str::to_owned),
            author: "7".into(),
            component_type: 2,
            custom_id: "ok".into(),
            values: None,
        }
    }

    #[test]
    fn a_click_is_sent_as_discords_client_sends_it() {
        let apps = AppIds::default();
        assert_eq!(
            click(None).body("s", &apps),
            json!({
                "type": 3, "nonce": "5", "guild_id": "1", "channel_id": "2",
                "message_id": "3", "message_flags": 0, "application_id": "7",
                "session_id": "s", "data": {"component_type": 2, "custom_id": "ok"},
            })
        );
    }

    #[test]
    fn a_bots_application_is_learned_from_what_discord_said() {
        let apps = AppIds::default();
        click(Some("9")).body("s", &apps);
        assert_eq!(click(None).body("s", &apps)["application_id"], "9");
        assert_eq!(apps.of("8"), "8");
    }

    #[test]
    fn failures_say_what_discords_client_says() {
        assert_eq!(failure_reason(2), "The application did not respond.");
        assert_eq!(failure_reason(1), "This interaction failed.");
        assert_eq!(failure_reason(17), "This interaction failed.");
    }

    #[test]
    fn a_form_is_submitted_with_its_id_and_values() {
        let form = Interaction::Modal {
            nonce: "5".into(),
            guild: None,
            channel: "2".into(),
            application: "9".into(),
            id: "11".into(),
            custom_id: "notes".into(),
            components: json!([{"type": 18, "component": {"type": 4, "custom_id": "a", "value": "hi"}}]),
        };
        let body = form.body("s", &AppIds::default());
        assert_eq!(body["type"], 5);
        assert!(body.get("guild_id").is_none());
        assert_eq!(body["data"]["id"], "11");
        assert_eq!(body["data"]["components"][0]["component"]["value"], "hi");
    }
}
