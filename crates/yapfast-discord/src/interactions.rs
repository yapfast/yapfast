//! Using a bot's message components and forms, and apps' commands, as this
//! account, as Discord's client does: the bodies of `POST /interactions`,
//! which application a bot message's controls belong to, and what a failed
//! interaction's reason says. See docs/discord/messages.md, "Bots' buttons,
//! menus and forms" and "Sending apps' commands".
//!
//! Every interaction starts from something the person did, a click, a
//! submit, a command sent or a keystroke in an autocompleted option: nothing
//! here retries or sends one on its own.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};

/// How long an interaction stays pending without a word from the gateway,
/// once Discord took the request. Discord itself fails one whose app doesn't
/// answer in 3 seconds; this covers a gateway that dropped the answer, as
/// across a reconnect.
pub const UNANSWERED: Duration = Duration::from_secs(15);

/// How long typing in an autocompleted option pauses before the app is
/// asked for its choices: a keystroke within it asks instead.
pub const AUTOCOMPLETE_PAUSE: Duration = Duration::from_millis(300);

/// Where Discord's client says a command typed in the composer comes from:
/// docs.discord.food lists `slash_ui` as "executed from the chat bar".
const CHAT_BAR: &str = "slash_ui";

/// The application command option type of a file.
const ATTACHMENT: u64 = 11;

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
    /// An app's command sent, as typed in the composer or chosen from a
    /// message's or a person's menu.
    Command(CommandUse),
    /// An app asked for its choices for the option being typed.
    Autocomplete(CommandUse),
}

/// An app command with the options entered, to run or to complete.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandUse {
    pub nonce: String,
    /// None in a DM or group DM.
    pub guild: Option<String>,
    pub channel: String,
    /// The command's application.
    pub application: String,
    /// The command as Discord's command index gives it, which the body
    /// carries whole as `application_command`.
    pub command: Arc<Value>,
    /// The options as `/interactions` takes them: `{type, name, value}`
    /// each, a subcommand or group as `{type: 1 or 2, name, options}`. A
    /// file option's `value` is its index in `files`, as a string. For an
    /// autocomplete, the option being typed has `focused: true`.
    pub options: Value,
    /// The message or user a context-menu command acts on.
    pub target: Option<String>,
    /// The names of the files the options carry, sent as `files[n]`.
    pub files: Vec<String>,
}

impl Interaction {
    pub fn nonce(&self) -> &str {
        match self {
            Self::Component { nonce, .. } | Self::Modal { nonce, .. } => nonce,
            Self::Command(run) | Self::Autocomplete(run) => &run.nonce,
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
            Self::Command(run) => run.command_body(session),
            Self::Autocomplete(run) => run.autocomplete_body(session),
        }
    }
}

impl CommandUse {
    /// The body of `POST /interactions` that runs the command (type 2), as
    /// docs.discord.food's "Create Interaction" gives it. Its files go
    /// beside it in the same request, as `files[n]`.
    pub fn command_body(&self, session: &str) -> Value {
        let mut body = self.body(2, session, self.options.clone());
        if !self.files.is_empty() {
            body["data"]["attachments"] = self
                .files
                .iter()
                .enumerate()
                .map(|(index, name)| json!({"id": index.to_string(), "filename": name}))
                .collect();
        }
        // A command typed in the composer; what a context menu sends is
        // phase 2's, and not known yet.
        if self.target.is_none() {
            body["analytics_location"] = json!(CHAT_BAR);
        }
        body
    }

    /// The body of `POST /interactions` that asks the app for its choices
    /// for the focused option (type 4): the same data, without files. The
    /// focused option's value is always a string, whatever its type, and
    /// file options are left out.
    pub fn autocomplete_body(&self, session: &str) -> Value {
        self.body(4, session, typed_so_far(&self.options))
    }

    fn body(&self, kind: u64, session: &str, options: Value) -> Value {
        let command = &*self.command;
        let command_type = command["type"].as_u64().unwrap_or(1);
        let mut data = json!({
            "version": command["version"],
            "id": command["id"],
            "name": command["name"],
            "type": command_type,
            "application_command": command,
        });
        // Options belong to a chat command alone; a context-menu command
        // names its target instead.
        if command_type == 1 {
            data["options"] = if options.is_array() {
                options
            } else {
                json!([])
            };
        }
        if let Some(target) = &self.target {
            data["target_id"] = json!(target);
        }
        let mut body = json!({
            "type": kind,
            "nonce": self.nonce,
            "application_id": self.application,
            "channel_id": self.channel,
            "session_id": session,
            "data": data,
        });
        if let Some(guild) = &self.guild {
            body["guild_id"] = json!(guild);
        }
        body
    }
}

/// Options as an autocomplete sends them: without files, and with the
/// focused one's value as a string.
fn typed_so_far(options: &Value) -> Value {
    let Some(options) = options.as_array() else {
        return json!([]);
    };
    options
        .iter()
        .filter(|option| option["type"].as_u64() != Some(ATTACHMENT))
        .map(|option| {
            let mut option = option.clone();
            if option.get("options").is_some() {
                option["options"] = typed_so_far(&option["options"]);
            } else if option["focused"] == true {
                let value = match &option["value"] {
                    Value::String(_) => None,
                    Value::Null => Some(String::new()),
                    other => Some(other.to_string()),
                };
                if let Some(value) = value {
                    option["value"] = json!(value);
                }
            }
            option
        })
        .collect()
}

/// Which autocomplete is the newest: only its answer is wanted, and one
/// still waiting out [`AUTOCOMPLETE_PAUSE`] when a newer one starts is never
/// sent.
#[derive(Clone, Default, Debug)]
pub struct Autocompletes(Arc<AtomicU64>);

impl Autocompletes {
    /// Starts an autocomplete, which is the newest until the next starts.
    pub fn start(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether the autocomplete `start` gave `turn` is still the newest.
    pub fn newest(&self, turn: u64) -> bool {
        self.0.load(Ordering::SeqCst) == turn
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

    fn echo(options: Value, files: &[&str]) -> CommandUse {
        CommandUse {
            nonce: "5".into(),
            guild: Some("1".into()),
            channel: "2".into(),
            application: "9".into(),
            command: Arc::new(json!({
                "id": "30", "application_id": "9", "version": "31", "type": 1,
                "name": "echo", "description": "Says it back",
            })),
            options,
            target: None,
            files: files.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn a_command_is_sent_with_its_index_entry_and_options() {
        let run = echo(json!([{"type": 3, "name": "text", "value": "hi"}]), &[]);
        assert_eq!(
            Interaction::Command(run.clone()).body("s", &AppIds::default()),
            json!({
                "type": 2, "nonce": "5", "application_id": "9", "guild_id": "1",
                "channel_id": "2", "session_id": "s", "analytics_location": "slash_ui",
                "data": {
                    "version": "31", "id": "30", "name": "echo", "type": 1,
                    "application_command": *run.command,
                    "options": [{"type": 3, "name": "text", "value": "hi"}],
                },
            })
        );
    }

    #[test]
    fn a_subcommand_nests_and_a_file_is_named_by_its_index() {
        let options = json!([{"type": 1, "name": "set", "options": [
            {"type": 11, "name": "file", "value": "0"},
        ]}]);
        let body = echo(options.clone(), &["a.png"]).command_body("s");
        assert_eq!(body["data"]["options"], options);
        assert_eq!(
            body["data"]["attachments"],
            json!([{"id": "0", "filename": "a.png"}])
        );
    }

    #[test]
    fn a_context_menu_command_names_its_target_without_options() {
        let mut run = echo(json!([]), &[]);
        run.command = Arc::new(json!({"id": "40", "version": "41", "type": 3, "name": "Quote"}));
        run.target = Some("77".into());
        let body = run.command_body("s");
        assert_eq!(body["data"]["target_id"], "77");
        assert!(body["data"].get("options").is_none());
        assert!(body.get("analytics_location").is_none());
    }

    #[test]
    fn an_autocomplete_sends_the_focused_value_as_text_and_no_files() {
        let run = echo(
            json!([
                {"type": 4, "name": "times", "value": 3, "focused": true},
                {"type": 11, "name": "file", "value": "0"},
            ]),
            &["a.png"],
        );
        let body = run.autocomplete_body("s");
        assert_eq!(body["type"], 4);
        assert_eq!(
            body["data"]["options"],
            json!([{"type": 4, "name": "times", "value": "3", "focused": true}])
        );
        assert!(body["data"].get("attachments").is_none());
        assert!(body.get("analytics_location").is_none());
    }

    #[test]
    fn only_the_newest_autocomplete_is_wanted() {
        let autocompletes = Autocompletes::default();
        let first = autocompletes.start();
        assert!(autocompletes.newest(first));
        let second = autocompletes.start();
        assert!(!autocompletes.newest(first));
        assert!(autocompletes.newest(second));
    }
}
