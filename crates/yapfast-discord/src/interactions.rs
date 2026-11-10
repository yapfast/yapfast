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
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::api::Upload;

/// How long an interaction stays pending without a word from the gateway,
/// once Discord took the request. Discord itself fails one whose app doesn't
/// answer in 3 seconds; this covers a gateway that dropped the answer, as
/// across a reconnect.
pub const UNANSWERED: Duration = Duration::from_secs(15);

/// How long typing in an autocompleted option pauses before the app is
/// asked again for its choices, as Discord's client debounces it: the first
/// keystroke asks at once, the others once typing stops for this long.
pub const AUTOCOMPLETE_PAUSE: Duration = Duration::from_millis(500);

/// Where Discord's client says a command comes from: docs.discord.food lists
/// `slash_ui` as "executed from the chat bar", and the client sends it for a
/// command chosen from a message's or a person's menu too.
const CHAT_BAR: &str = "slash_ui";

/// The application command option type of a file.
const ATTACHMENT: u64 = 11;

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
        /// The message's `application_id`, or else its author's id, which a
        /// bot's user shares with its app, as Discord's client sends it.
        application: String,
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
        /// A file upload's `values` are its files' places in `files`, as
        /// `0` for the first.
        components: Value,
        /// The names of the files its file uploads carry, uploaded before
        /// it's sent.
        files: Vec<String>,
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
    /// file option's `value` is its index in `files`, a number. For an
    /// autocomplete, the option being typed has `focused: true`.
    pub options: Value,
    /// The message or user a context-menu command acts on.
    pub target: Option<String>,
    /// The names of the files the options carry, uploaded before it's sent.
    pub files: Vec<String>,
}

impl Interaction {
    pub fn nonce(&self) -> &str {
        match self {
            Self::Component { nonce, .. } | Self::Modal { nonce, .. } => nonce,
            Self::Command(run) | Self::Autocomplete(run) => &run.nonce,
        }
    }

    /// The channel it's used in, where its files are uploaded.
    pub fn channel(&self) -> &str {
        match self {
            Self::Component { channel, .. } | Self::Modal { channel, .. } => channel,
            Self::Command(run) | Self::Autocomplete(run) => &run.channel,
        }
    }

    /// The names of the files it carries, to upload before it's sent.
    pub fn files(&self) -> &[String] {
        match self {
            Self::Component { .. } | Self::Autocomplete(_) => &[],
            Self::Modal { files, .. } => files,
            Self::Command(run) => &run.files,
        }
    }

    /// Whether it goes again after a 429, as Discord's client sends a
    /// command or a form again; a click or an autocomplete it drops.
    pub fn again_when_limited(&self) -> bool {
        matches!(self, Self::Modal { .. } | Self::Command(_))
    }

    /// The body of `POST /interactions` in the gateway session `session`,
    /// with [`Self::files`] uploaded as `uploads`.
    pub fn body(&self, session: &str, uploads: &[Upload]) -> Value {
        match self {
            Self::Component {
                nonce,
                guild,
                channel,
                message,
                message_flags,
                application,
                component_type,
                custom_id,
                values,
            } => {
                let mut data = json!({"component_type": component_type, "custom_id": custom_id});
                if let Some(values) = values {
                    // Discord's client names a menu's kind beside its picks.
                    data["type"] = json!(component_type);
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
                files,
            } => {
                let mut body = json!({
                    "type": 5,
                    "nonce": nonce,
                    "application_id": application,
                    "channel_id": channel,
                    "session_id": session,
                    "data": {"id": id, "custom_id": custom_id, "components": components},
                });
                // As a command's: each file named by its place among them.
                if !files.is_empty() {
                    body["data"]["attachments"] = attachments(uploads);
                }
                if let Some(guild) = guild {
                    body["guild_id"] = json!(guild);
                }
                body
            }
            Self::Command(run) => run.command_body(session, uploads),
            Self::Autocomplete(run) => run.autocomplete_body(session),
        }
    }
}

impl CommandUse {
    /// The body of `POST /interactions` that runs the command (type 2), as
    /// docs.discord.food's "Create Interaction" gives it, with its files
    /// uploaded first as `uploads`, as Discord's client does.
    pub fn command_body(&self, session: &str, uploads: &[Upload]) -> Value {
        let mut body = self.body(2, session, self.options.clone());
        if !uploads.is_empty() {
            body["data"]["attachments"] = attachments(uploads);
        }
        body["analytics_location"] = json!(CHAT_BAR);
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
        // Discord's client sends `options`, empty for a context-menu
        // command, and the server a command was made for.
        let mut data = json!({
            "version": command["version"],
            "id": command["id"],
            "name": command["name"],
            "type": command_type,
            "options": if options.is_array() { options } else { json!([]) },
            "application_command": command,
        });
        if let Some(guild) = command["guild_id"].as_str() {
            data["guild_id"] = json!(guild);
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

/// The uploaded files as `data.attachments` names them, each by its place,
/// `"0"` for the first, as an option's or a file upload's value names it.
fn attachments(uploads: &[Upload]) -> Value {
    uploads
        .iter()
        .enumerate()
        .map(|(index, upload)| {
            json!({
                "id": index.to_string(),
                "filename": upload.filename,
                "uploaded_filename": upload.uploaded_filename,
            })
        })
        .collect()
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

/// Which autocomplete is the newest, whose answer alone is wanted, and when
/// the next may go: the first keystroke after a pause asks at once, and a
/// keystroke within [`AUTOCOMPLETE_PAUSE`] of another asks once typing stops
/// for that long, as Discord's client's debounce (leading and trailing).
#[derive(Clone, Default, Debug)]
pub struct Autocompletes(Arc<Mutex<Turns>>);

#[derive(Default, Debug)]
struct Turns {
    newest: u64,
    /// Until when keystrokes wait, pushed on by each.
    quiet_until: Option<Instant>,
}

impl Autocompletes {
    fn turns(&self) -> std::sync::MutexGuard<'_, Turns> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts an autocomplete at `now`, which is the newest until the next
    /// starts, and says whether it goes at once.
    pub fn start(&self, now: Instant) -> (u64, bool) {
        let mut turns = self.turns();
        turns.newest += 1;
        let at_once = turns.quiet_until.is_none_or(|until| until <= now);
        turns.quiet_until = Some(now + AUTOCOMPLETE_PAUSE);
        (turns.newest, at_once)
    }

    /// When one that waits may go, if it's still the newest by then.
    pub fn due(&self) -> Option<Instant> {
        self.turns().quiet_until
    }

    /// One that waited went: the next keystroke asks at once again.
    pub fn went(&self, turn: u64) {
        let mut turns = self.turns();
        if turns.newest == turn {
            turns.quiet_until = None;
        }
    }

    /// Whether the autocomplete `start` gave `turn` is still the newest.
    pub fn newest(&self, turn: u64) -> bool {
        self.turns().newest == turn
    }
}

/// What INTERACTION_FAILURE's `reason_code` means, in Discord's words.
pub fn failure_reason(code: u64) -> &'static str {
    match code {
        // TIMEOUT: the app didn't answer within 3 seconds.
        2 => "The application did not respond",
        // UNKNOWN, and 3-20, which are about launching an activity.
        _ => "This interaction failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(values: Option<Vec<String>>) -> Interaction {
        Interaction::Component {
            nonce: "5".into(),
            guild: Some("1".into()),
            channel: "2".into(),
            message: "3".into(),
            message_flags: 0,
            application: "7".into(),
            component_type: if values.is_some() { 3 } else { 2 },
            custom_id: "ok".into(),
            values,
        }
    }

    #[test]
    fn a_click_is_sent_as_discords_client_sends_it() {
        assert_eq!(
            click(None).body("s", &[]),
            json!({
                "type": 3, "nonce": "5", "guild_id": "1", "channel_id": "2",
                "message_id": "3", "message_flags": 0, "application_id": "7",
                "session_id": "s", "data": {"component_type": 2, "custom_id": "ok"},
            })
        );
        // A menu's picks go with its kind beside them.
        assert_eq!(
            click(Some(vec!["a".into()])).body("s", &[])["data"],
            json!({"component_type": 3, "custom_id": "ok", "type": 3, "values": ["a"]})
        );
    }

    #[test]
    fn failures_say_what_discords_client_says() {
        assert_eq!(failure_reason(2), "The application did not respond");
        assert_eq!(failure_reason(1), "This interaction failed");
        assert_eq!(failure_reason(17), "This interaction failed");
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
            files: Vec::new(),
        };
        let body = form.body("s", &[]);
        assert_eq!(body["type"], 5);
        assert!(body.get("guild_id").is_none());
        assert_eq!(body["data"]["id"], "11");
        assert_eq!(body["data"]["components"][0]["component"]["value"], "hi");
        assert!(body["data"].get("attachments").is_none());
    }

    #[test]
    fn a_forms_files_are_named_by_their_place() {
        let form = Interaction::Modal {
            nonce: "5".into(),
            guild: Some("1".into()),
            channel: "2".into(),
            application: "9".into(),
            id: "11".into(),
            custom_id: "proof".into(),
            components: json!([{"type": 18, "component": {"type": 19, "custom_id": "shots", "values": [0, 1]}}]),
            files: vec!["a.png".into(), "b.txt".into()],
        };
        assert_eq!(form.files(), ["a.png", "b.txt"]);
        let body = form.body("s", &[upload("a.png"), upload("b.txt")]);
        assert_eq!(
            body["data"]["attachments"],
            json!([
                {"id": "0", "filename": "a.png", "uploaded_filename": "uploads/a.png"},
                {"id": "1", "filename": "b.txt", "uploaded_filename": "uploads/b.txt"},
            ])
        );
    }

    fn upload(name: &str) -> Upload {
        Upload {
            filename: name.into(),
            uploaded_filename: format!("uploads/{name}"),
        }
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
            Interaction::Command(run.clone()).body("s", &[]),
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
            {"type": 11, "name": "file", "value": 0},
        ]}]);
        let body = echo(options.clone(), &["a.png"]).command_body("s", &[upload("a.png")]);
        assert_eq!(body["data"]["options"], options);
        assert_eq!(
            body["data"]["attachments"],
            json!([{"id": "0", "filename": "a.png", "uploaded_filename": "uploads/a.png"}])
        );
    }

    #[test]
    fn a_context_menu_command_names_its_target_and_no_options() {
        let mut run = echo(Value::Null, &[]);
        run.command = Arc::new(json!({"id": "40", "version": "41", "type": 3, "name": "Quote"}));
        run.target = Some("77".into());
        let body = run.command_body("s", &[]);
        assert_eq!(body["data"]["target_id"], "77");
        assert_eq!(body["data"]["options"], json!([]));
        assert_eq!(body["analytics_location"], "slash_ui");
        assert!(body["data"].get("guild_id").is_none());
    }

    #[test]
    fn a_servers_own_command_names_its_server() {
        let mut run = echo(json!([]), &[]);
        run.command = Arc::new(
            json!({"id": "30", "version": "31", "type": 1, "name": "echo", "guild_id": "1"}),
        );
        assert_eq!(run.command_body("s", &[])["data"]["guild_id"], "1");
    }

    #[test]
    fn an_autocomplete_sends_the_focused_value_as_text_and_no_files() {
        let run = echo(
            json!([
                {"type": 4, "name": "times", "value": 3, "focused": true},
                {"type": 11, "name": "file", "value": 0},
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
    fn autocompletes_go_first_and_last_and_only_the_newest_is_wanted() {
        let autocompletes = Autocompletes::default();
        let now = Instant::now();
        let (first, at_once) = autocompletes.start(now);
        assert!(at_once && autocompletes.newest(first));
        let (second, at_once) = autocompletes.start(now + Duration::from_millis(100));
        assert!(!at_once, "within the pause it waits");
        assert!(!autocompletes.newest(first) && autocompletes.newest(second));
        assert_eq!(
            autocompletes.due(),
            Some(now + Duration::from_millis(100) + AUTOCOMPLETE_PAUSE)
        );
        autocompletes.went(second);
        let (_, at_once) = autocompletes.start(now + Duration::from_millis(700));
        assert!(at_once, "after one went, the next goes at once");
        let (_, at_once) = autocompletes.start(now + Duration::from_secs(5));
        assert!(at_once, "and after a pause");
    }
}
