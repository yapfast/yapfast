//! The `/` picker in the composer and the app command being written there:
//! which commands fit the channel, the options as the person fills them, and
//! the checks Discord leaves to the client before a command goes. See
//! docs/discord/messages.md, "Apps' commands".
//!
//! A command is written one option at a time. The composer's text is the
//! focused option's, so the draft of a room in command mode is that text,
//! never saved; Tab moves between options, and Enter runs the command once
//! every option passes its checks. docs.discord.food says these checks are
//! the client's alone ("This validation is client-side only").
//!
//! The index is asked for only once the person types `/`, and kept, by
//! application/commands.rs, which also says which of its commands the person
//! may use; autocomplete only on a keystroke in an option that has it; a
//! command runs only on Enter, or from a menu's "Apps" (application/commands.rs).

use std::cell::OnceCell;

use serde_json::{Value, json};

use super::*;
use crate::chat::CommandRun;

// Option types, as docs.discord.food lists them
// (interactions/application-commands, "Application Command Option Type").
const SUB_COMMAND: u64 = 1;
const SUB_COMMAND_GROUP: u64 = 2;
const STRING: u64 = 3;
const INTEGER: u64 = 4;
const BOOLEAN: u64 = 5;
const USER: u64 = 6;
const CHANNEL: u64 = 7;
const ROLE: u64 = 8;
const MENTIONABLE: u64 = 9;
const NUMBER: u64 = 10;
const ATTACHMENT: u64 = 11;

/// How many of an app's autocomplete answers a command being written keeps.
const ANSWERS_KEPT: usize = 32;

/// How many people, roles or channels an option offers at once.
const OPTION_SUGGESTIONS: usize = 10;

/// The picker and the command being written in a channel's composer.
#[derive(Default)]
pub struct Slash {
    /// The room the picker is open in, and what follows the `/`.
    query: Option<(String, String)>,
    /// The picker was closed with Escape; it stays closed until the draft
    /// no longer starts with `/`.
    dismissed: bool,
    /// The command being written.
    command: Option<CommandDraft>,
    /// Commands sent, until their answers come.
    sent: Vec<SentCommand>,
}

/// A command sent, which its channel shows as "Sending command…" until the
/// app's answer comes, or, when it failed, as why, with Retry.
struct SentCommand {
    run: CommandRun,
    app_name: String,
    app_icon: Option<String>,
    /// The interaction's id, which its answer names in
    /// `interaction_metadata`, once Discord gave it.
    interaction: Option<String>,
    /// Why it failed.
    failed: Option<String>,
}

/// What a command row's item starts with.
const SENT_COMMAND: &str = "c:";

/// A command picked from the picker, with what was entered for its options.
pub struct CommandDraft {
    room: String,
    application: String,
    app_name: String,
    app_icon: Option<String>,
    /// The command as the index gives it, sent along with it.
    command: Arc<Value>,
    /// Its name with its subcommand's, as "config set".
    pub name: String,
    /// The subcommand group and subcommand chosen, outermost first.
    path: Vec<String>,
    pub options: Vec<OptionDraft>,
    /// The option the composer writes.
    pub focused: usize,
    /// Enter was pressed with an option missing or wrong: every option shows
    /// what's wrong with it, the required ones too.
    tried: bool,
    /// The newest autocomplete asked, by nonce, with the option and the
    /// text it asked for; older answers are dropped.
    autocomplete: Option<(String, usize, String)>,
    /// The app's answers so far, by option and text, newest last: text
    /// answered before isn't asked again, as Discord's client keeps them.
    answered: Vec<(usize, String, Vec<Suggestion>)>,
    /// What the app suggested for the focused option.
    suggested: Vec<Suggestion>,
    /// The focused option's suggestions were closed with Escape.
    choices_dismissed: bool,
}

/// An option of the command being written.
pub struct OptionDraft {
    /// The option as the index gives it.
    spec: Value,
    pub name: String,
    pub description: String,
    pub kind: u64,
    pub required: bool,
    /// What the composer shows for it: typed, or a choice's name.
    pub text: String,
    /// The choice taken, by the text it put in, and its value.
    chosen: Option<(String, Value)>,
    /// A file for an attachment option.
    pub file: Option<PathBuf>,
    /// For a channel option with `channel_types`, the channels of those
    /// types, by id, as the command was picked; none for any channel.
    channels: Option<HashSet<String>>,
}

impl OptionDraft {
    pub fn is_attachment(&self) -> bool {
        self.kind == ATTACHMENT
    }

    /// What its pill shows: the text, or a file's name.
    pub fn shown(&self) -> String {
        match &self.file {
            Some(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => self.text.clone(),
        }
    }
}

/// A row of the list above the composer: a command the picker offers, or a
/// value the focused option may take.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandRow {
    /// The app's name, for a command; nothing for a value.
    pub app: String,
    /// The app's icon or the person's avatar, by address.
    pub icon: Option<String>,
    pub name: String,
    pub description: String,
}

/// A value an option may take, from its choices, the app's autocomplete,
/// or the people, roles and channels a mention would offer.
#[derive(Clone, Debug, PartialEq)]
struct Suggestion {
    name: String,
    detail: String,
    picture: Option<String>,
    value: Value,
}

fn kind(value: &Value) -> u64 {
    value.get("type").and_then(Value::as_u64).unwrap_or(0)
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn list<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value
        .get(field)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn command_nonce() -> String {
    #[cfg(feature = "discord")]
    {
        yapfast_discord::message::nonce()
    }
    #[cfg(not(feature = "discord"))]
    {
        uuid::Uuid::new_v4().simple().to_string()
    }
}

/// What a `<@id>`, `<@&id>` or `<#id>` mention points at.
fn mention_id(mention: &str) -> Option<&str> {
    let inner = mention.strip_prefix('<')?.strip_suffix('>')?;
    inner
        .strip_prefix("@&")
        .or_else(|| inner.strip_prefix('@'))
        .or_else(|| inner.strip_prefix('#'))
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Numbers as Discord prints them: whole ones without a point.
fn number(value: &Value) -> String {
    match value.as_f64() {
        Some(float) if float.fract() == 0.0 && float.abs() < 1e15 => format!("{}", float as i64),
        Some(float) => float.to_string(),
        None => value.to_string(),
    }
}

impl AppState {
    /// The command being written in the selected room's composer.
    pub fn command_draft(&self) -> Option<&CommandDraft> {
        let room = self.selected.as_deref()?;
        self.slash
            .command
            .as_ref()
            .filter(|command| command.room == room)
    }

    /// The `/` picker is open over the selected room's composer, with what
    /// follows the `/`.
    pub fn slash_query(&self) -> Option<&str> {
        let room = self.selected.as_deref()?;
        match &self.slash.query {
            Some((at, query))
                if at == room && !self.slash.dismissed && self.draft_in(room).starts_with('/') =>
            {
                Some(query)
            }
            _ => None,
        }
    }

    /// The values the focused option may take, matching what was typed.
    fn option_suggestions(&self) -> Vec<Suggestion> {
        self.option_suggestions_among(&OnceCell::new())
    }

    /// [`Self::option_suggestions`], with the people, roles and channels a
    /// mention would offer worked out at most once into `known`.
    fn option_suggestions_among(&self, known: &OnceCell<Vec<Candidate>>) -> Vec<Suggestion> {
        let Some(command) = self.command_draft() else {
            return vec![];
        };
        let Some(option) = command.options.get(command.focused) else {
            return vec![];
        };
        if command.choices_dismissed
            || option
                .chosen
                .as_ref()
                .is_some_and(|(text, _)| text == &option.text)
        {
            return vec![];
        }
        let typed = option.text.trim().to_lowercase();
        let choices = list(&option.spec, "choices");
        if !choices.is_empty() {
            return choices
                .iter()
                .filter(|choice| text(choice, "name").to_lowercase().contains(&typed))
                .map(|choice| Suggestion {
                    name: text(choice, "name").to_owned(),
                    detail: String::new(),
                    picture: None,
                    value: choice.get("value").cloned().unwrap_or(Value::Null),
                })
                .collect();
        }
        if option.spec.get("autocomplete") == Some(&Value::Bool(true)) {
            return command.suggested.clone();
        }
        match option.kind {
            BOOLEAN => [("True", true), ("False", false)]
                .into_iter()
                .filter(|(name, _)| name.to_lowercase().starts_with(&typed))
                .map(|(name, value)| Suggestion {
                    name: name.into(),
                    detail: String::new(),
                    picture: None,
                    value: Value::Bool(value),
                })
                .collect(),
            USER | ROLE | MENTIONABLE | CHANNEL => {
                let candidates: Vec<&Candidate> = known
                    .get_or_init(|| self.mention_candidates())
                    .iter()
                    .filter(|candidate| takes(option, candidate))
                    .collect();
                let typed = typed.trim_start_matches(['@', '#']);
                mention::matching(&candidates, typed, OPTION_SUGGESTIONS)
                    .into_iter()
                    .filter_map(|candidate| {
                        Some(Suggestion {
                            name: candidate.name.clone(),
                            detail: candidate.username.clone().unwrap_or_default(),
                            picture: candidate.avatar.clone(),
                            value: Value::String(mention_id(&candidate.mention)?.to_owned()),
                        })
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// The option's value as `/interactions` takes it, none when it's
    /// empty, or why it can't be sent. The wording is Yapfast's: Discord's
    /// own is not documented.
    fn option_value(&self, option: &OptionDraft) -> Result<Option<Value>, String> {
        self.option_value_among(option, &OnceCell::new())
    }

    /// [`Self::option_value`], with the people, roles and channels a
    /// mention would offer worked out at most once into `known`.
    fn option_value_among(
        &self,
        option: &OptionDraft,
        known: &OnceCell<Vec<Candidate>>,
    ) -> Result<Option<Value>, String> {
        if option.kind == ATTACHMENT {
            return Ok(option.file.as_ref().map(|_| Value::Null));
        }
        if let Some((text, value)) = &option.chosen
            && text == &option.text
        {
            return Ok(Some(value.clone()));
        }
        let typed = option.text.trim();
        if typed.is_empty() {
            return Ok(None);
        }
        let choices = list(&option.spec, "choices");
        if !choices.is_empty() {
            return choices
                .iter()
                .find(|choice| text(choice, "name").eq_ignore_ascii_case(typed))
                .and_then(|choice| choice.get("value").cloned())
                .map(Some)
                .ok_or_else(|| "Pick one of the choices".into());
        }
        let bound = |field: &str| option.spec.get(field).and_then(Value::as_f64);
        let in_range = |value: f64| -> Result<(), String> {
            if let Some(min) = bound("min_value").filter(|min| value < *min) {
                return Err(format!("Must be at least {}", number(&json!(min))));
            }
            if let Some(max) = bound("max_value").filter(|max| value > *max) {
                return Err(format!("Must be at most {}", number(&json!(max))));
            }
            Ok(())
        };
        match option.kind {
            STRING => {
                let length = typed.chars().count() as f64;
                if let Some(min) = bound("min_length").filter(|min| length < *min) {
                    return Err(format!("Must be at least {min} characters"));
                }
                if let Some(max) = bound("max_length").filter(|max| length > *max) {
                    return Err(format!("Must be at most {max} characters"));
                }
                Ok(Some(Value::String(typed.to_owned())))
            }
            INTEGER => {
                let value: i64 = typed
                    .parse()
                    .map_err(|_| "Must be a whole number".to_owned())?;
                in_range(value as f64)?;
                Ok(Some(json!(value)))
            }
            NUMBER => {
                let parsed: f64 = typed
                    .parse()
                    .ok()
                    .filter(|value: &f64| value.is_finite())
                    .ok_or_else(|| "Must be a number".to_owned())?;
                in_range(parsed)?;
                Ok(Some(json!(parsed)))
            }
            BOOLEAN => match typed.to_lowercase().as_str() {
                "true" => Ok(Some(Value::Bool(true))),
                "false" => Ok(Some(Value::Bool(false))),
                _ => Err("Pick True or False".into()),
            },
            USER | ROLE | MENTIONABLE | CHANNEL => {
                // A mention or an id pasted in names one Yapfast may not
                // have listed.
                let id = mention_id(typed)
                    .or_else(|| {
                        typed
                            .bytes()
                            .all(|byte| byte.is_ascii_digit())
                            .then_some(typed)
                    })
                    .filter(|id| id.len() >= 17);
                if let Some(id) = id {
                    return Ok(Some(Value::String(id.to_owned())));
                }
                let name = typed.trim_start_matches(['@', '#']);
                known
                    .get_or_init(|| self.mention_candidates())
                    .iter()
                    .filter(|candidate| takes(option, candidate))
                    .find(|candidate| candidate.name.eq_ignore_ascii_case(name))
                    .and_then(|candidate| mention_id(&candidate.mention).map(str::to_owned))
                    .map(|id| Some(Value::String(id)))
                    .ok_or_else(|| {
                        match option.kind {
                            USER => "Pick a member",
                            ROLE => "Pick a role",
                            CHANNEL => "Pick a channel",
                            _ => "Pick a member or a role",
                        }
                        .into()
                    })
            }
            _ => Err("Yapfast can't fill this option yet".into()),
        }
    }

    /// What's wrong with each option, for its pill: a value that can't be
    /// sent, or, once Enter was pressed, a required one left empty. The
    /// people and roles an option may name are listed once for all.
    pub fn option_errors(&self, command: &CommandDraft) -> Vec<Option<String>> {
        let known = OnceCell::new();
        command
            .options
            .iter()
            .map(|option| match self.option_value_among(option, &known) {
                Err(error) => Some(error),
                Ok(None) if option.required && command.tried => Some("Required".into()),
                Ok(_) => None,
            })
            .collect()
    }
}

/// Whether a mention candidate fits a user, role, mentionable or channel
/// option: a channel option with `channel_types` takes the channels of
/// those types.
fn takes(option: &OptionDraft, candidate: &Candidate) -> bool {
    let role = candidate.group && candidate.mention.starts_with("<@&");
    let person = !candidate.group;
    match option.kind {
        USER => person,
        ROLE => role,
        MENTIONABLE => person || role,
        CHANNEL => {
            candidate.channel
                && option.channels.as_ref().is_none_or(|ids| {
                    mention_id(&candidate.mention).is_some_and(|id| ids.contains(id))
                })
        }
        _ => false,
    }
}

impl Application {
    /// What the picker says above `rows`, [`Self::command_rows`]'s, or in
    /// their place: what it lists, that the commands are on their way, or
    /// why they aren't.
    pub fn command_heading(&self, rows: &[CommandRow]) -> String {
        let state = &self.state;
        if let Some(command) = state.command_draft() {
            return command
                .options
                .get(command.focused)
                .map(|option| option.name.to_uppercase())
                .unwrap_or_default();
        }
        let (Some(room), Some(_)) = (state.selected.as_deref(), state.slash_query()) else {
            return String::new();
        };
        if state.app_commands.loading(room) {
            return "Loading commands…".into();
        }
        if let Some(error) = &state.app_commands.error {
            return error.clone();
        }
        if rows.is_empty() {
            "No commands match".into()
        } else {
            "COMMANDS".into()
        }
    }

    /// The rows the list above the composer shows: the commands the picker
    /// offers, or the values the focused option may take.
    pub fn command_rows(&self) -> Vec<CommandRow> {
        let state = &self.state;
        if state.command_draft().is_some() {
            return state
                .option_suggestions_among(&OnceCell::new())
                .into_iter()
                .map(|suggestion| CommandRow {
                    app: String::new(),
                    icon: suggestion.picture,
                    name: suggestion.name,
                    description: suggestion.detail,
                })
                .collect();
        }
        let (Some(room), Some(query)) = (state.selected.as_deref(), state.slash_query()) else {
            return vec![];
        };
        self.slash_commands(room, query)
            .into_iter()
            .map(|command| CommandRow {
                app: command.app_name,
                icon: command.app_icon,
                name: command.name,
                description: command.description,
            })
            .collect()
    }

    /// The selected room's draft changed: opens or follows the `/` picker,
    /// or, with a command being written, changes its focused option. Gives
    /// back `None` when the draft is the composer's ordinary text.
    pub(super) fn slash_draft(&mut self, room: &str, body: &str) -> Option<Vec<Effect>> {
        if self
            .state
            .slash
            .command
            .as_ref()
            .is_some_and(|command| command.room == room)
        {
            return Some(self.edit_option(room, body));
        }
        let writing = Network::of(room) == Network::Discord
            && !matches!(self.state.compose.get(room), Some(Compose::Edit { .. }))
            && !self.state.preview;
        let query = body.strip_prefix('/').filter(|_| writing);
        let Some(query) = query else {
            self.state.slash.query = None;
            self.state.slash.dismissed = false;
            return None;
        };
        if query.contains(char::is_whitespace) && !self.state.app_commands.loading(room) {
            // A command's whole name and a space picks it, as in Discord's
            // client, the rest going to its first option; a name a
            // subcommand's starts keeps the picker; anything else is text.
            if let Some((picked, rest)) = self.named_command(room, query) {
                let mut effects = self.write_command(room.to_owned(), picked);
                let takes_text = self
                    .state
                    .command_draft()
                    .is_some_and(|command| !command.options.is_empty());
                if takes_text && !rest.is_empty() {
                    effects.extend(self.set_draft(rest));
                }
                return Some(effects);
            }
            let lower = query.to_lowercase();
            if !self
                .slash_commands(room, "")
                .iter()
                .any(|command| command.name.to_lowercase().starts_with(&lower))
            {
                self.state.slash.query = None;
                self.state.slash.dismissed = false;
                return None;
            }
        }
        Some(self.follow_slash(room.to_owned(), query.to_owned()))
    }

    /// The command whose whole name `query` starts with, followed by a
    /// space, and what follows the space. The longest name wins, so
    /// `config set` over `config`.
    fn named_command(&self, room: &str, query: &str) -> Option<(SlashCommand, String)> {
        let picked = self
            .slash_commands(room, "")
            .into_iter()
            .filter(|command| {
                let len = command.name.len();
                query
                    .get(..len)
                    .is_some_and(|head| head.to_lowercase() == command.name.to_lowercase())
                    && query[len..].starts_with(char::is_whitespace)
            })
            .max_by_key(|command| command.name.len())?;
        let rest = query[picked.name.len()..].trim_start().to_owned();
        Some((picked, rest))
    }

    /// The `/` picker is open in `room`.
    pub(super) fn slash_open_in(&self, room: &str) -> bool {
        self.state
            .slash
            .query
            .as_ref()
            .is_some_and(|(at, _)| at == room)
    }

    /// The draft is `/` and `query` after it: the picker follows it, and
    /// the room's commands are asked for unless they're kept or on their
    /// way.
    fn follow_slash(&mut self, room: String, query: String) -> Vec<Effect> {
        self.state.slash.query = Some((room, query));
        self.ask_command_index().into_iter().collect()
    }

    /// [`Action::SlashQuery`]: as typing `/` and `query` would.
    pub(super) fn slash_query_action(&mut self, query: String) -> Vec<Effect> {
        self.set_draft(format!("/{query}"))
    }

    /// [`Event::AutocompleteChoices`]: the app's suggestions for the focused
    /// option, if they answer the newest keystroke.
    pub(super) fn slash_autocomplete(&mut self, nonce: &str, choices: &Value) -> Vec<Effect> {
        let Some(command) = self.state.slash.command.as_mut() else {
            return vec![];
        };
        let Some((_, option, typed)) = command.autocomplete.take_if(|(asked, ..)| asked == nonce)
        else {
            return vec![];
        };
        command.suggested = choices
            .as_array()
            .into_iter()
            .flatten()
            .map(|choice| Suggestion {
                name: choice
                    .get("name_localized")
                    .and_then(Value::as_str)
                    .unwrap_or(text(choice, "name"))
                    .to_owned(),
                detail: String::new(),
                picture: None,
                value: choice.get("value").cloned().unwrap_or(Value::Null),
            })
            .collect();
        if command.answered.len() >= ANSWERS_KEPT {
            command.answered.remove(0);
        }
        command
            .answered
            .push((option, typed, command.suggested.clone()));
        vec![]
    }

    /// [`Action::PickCommand`]: a command the picker offers, which the
    /// composer then writes, or a value for the focused option.
    pub(super) fn pick_command(&mut self, index: usize) -> Vec<Effect> {
        if self.state.command_draft().is_some() {
            let Some(suggestion) = self.state.option_suggestions().into_iter().nth(index) else {
                return vec![];
            };
            let Some(command) = self.state.slash.command.as_mut() else {
                return vec![];
            };
            let focused = command.focused;
            let Some(option) = command.options.get_mut(focused) else {
                return vec![];
            };
            option.text = suggestion.name.clone();
            option.chosen = Some((suggestion.name, suggestion.value));
            let next = (focused + 1).min(command.options.len() - 1);
            return self.focus_option(next);
        }
        let (Some(room), Some(query)) = (
            self.state.selected.clone(),
            self.state.slash_query().map(str::to_owned),
        ) else {
            return vec![];
        };
        let Some(picked) = self.slash_commands(&room, &query).into_iter().nth(index) else {
            return vec![];
        };
        self.write_command(room, picked)
    }

    /// Starts writing `picked` in `room`'s composer, its options as pills.
    fn write_command(&mut self, room: String, picked: SlashCommand) -> Vec<Effect> {
        let Some((command, options)) = self.slash_command_spec(&room, &picked) else {
            return vec![];
        };
        let draft = CommandDraft {
            room: room.clone(),
            application: picked.app,
            app_name: picked.app_name,
            app_icon: picked.app_icon,
            command,
            name: picked.name,
            path: picked.path,
            options: options
                .into_iter()
                .map(|spec| OptionDraft {
                    name: text(&spec, "name").to_owned(),
                    description: text(&spec, "description").to_owned(),
                    kind: kind(&spec),
                    required: spec.get("required") == Some(&Value::Bool(true)),
                    text: String::new(),
                    chosen: None,
                    file: None,
                    channels: (kind(&spec) == CHANNEL)
                        .then(|| {
                            let types: Vec<u64> = list(&spec, "channel_types")
                                .iter()
                                .filter_map(Value::as_u64)
                                .collect();
                            self.select_channels(&room, &types).map(|ids| {
                                ids.into_iter().map(str::to_owned).collect::<HashSet<_>>()
                            })
                        })
                        .flatten(),
                    spec,
                })
                .collect(),
            focused: 0,
            tried: false,
            autocomplete: None,
            answered: Vec::new(),
            suggested: Vec::new(),
            choices_dismissed: false,
        };
        self.state.slash.command = Some(draft);
        self.state.slash.query = None;
        self.state.slash.dismissed = false;
        self.state.drafts.insert(room.clone(), String::new());
        // The `/word` typed to find it is not a draft to keep.
        vec![
            Effect::Backend(Command::Typing {
                room: room.clone(),
                typing: false,
            }),
            Effect::Backend(Command::SaveDraft {
                room,
                body: String::new(),
            }),
        ]
    }

    /// [`Action::FocusCommandOption`]: the composer writes that option.
    pub(super) fn focus_option(&mut self, index: usize) -> Vec<Effect> {
        let Some(command) = self.state.slash.command.as_mut() else {
            return vec![];
        };
        let Some(option) = command.options.get(index) else {
            return vec![];
        };
        let text = option.text.clone();
        command.focused = index;
        command.autocomplete = None;
        command.suggested.clear();
        command.choices_dismissed = false;
        let room = command.room.clone();
        self.state.drafts.insert(room, text);
        vec![]
    }

    /// The composer's text changed with a command being written: it's the
    /// focused option's, and an option with autocomplete asks the app.
    fn edit_option(&mut self, room: &str, body: &str) -> Vec<Effect> {
        self.state.drafts.insert(room.to_owned(), body.to_owned());
        {
            let Some(command) = self.state.slash.command.as_mut() else {
                return vec![];
            };
            let Some(option) = command.options.get_mut(command.focused) else {
                return vec![];
            };
            if option.text == body {
                return vec![];
            }
            option.text = body.to_owned();
            command.choices_dismissed = false;
            if matches!(option.kind, USER | MENTIONABLE) {
                return self.search_option_members(room, body);
            }
            if option.spec.get("autocomplete") != Some(&Value::Bool(true)) || self.state.preview {
                return vec![];
            }
        }
        let Some(command) = self.state.slash.command.as_mut() else {
            return vec![];
        };
        let (focused, typed) = (command.focused, body.to_owned());
        if let Some((.., answer)) = command
            .answered
            .iter()
            .find(|(option, text, _)| *option == focused && *text == typed)
        {
            command.suggested = answer.clone();
            command.autocomplete = None;
            return vec![];
        }
        let Some(command) = self.state.slash.command.as_ref() else {
            return vec![];
        };
        // Autocomplete always gets the focused option as a string; the
        // others go as they would be sent, attachments left out.
        let mut options = Vec::new();
        for (index, option) in command.options.iter().enumerate() {
            if index == command.focused {
                options.push(json!({
                    "type": option.kind, "name": option.name,
                    "value": option.text, "focused": true,
                }));
            } else if option.kind != ATTACHMENT
                && let Ok(Some(value)) = self.state.option_value(option)
            {
                options.push(json!({"type": option.kind, "name": option.name, "value": value}));
            }
        }
        let nonce = command_nonce();
        let run = CommandRun {
            nonce: nonce.clone(),
            room: room.to_owned(),
            application: command.application.clone(),
            command: command.command.clone(),
            options: nest(&command.path, options),
            target: None,
            files: Vec::new(),
        };
        if let Some(command) = self.state.slash.command.as_mut() {
            command.autocomplete = Some((nonce, focused, typed));
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::Autocomplete(Box::new(run)),
        ))]
    }

    /// A server's members named as typed in a user or mentionable option,
    /// asked of Discord as a mention's are, since a server lists only some:
    /// unless the option already offers enough, or that was asked.
    fn search_option_members(&mut self, room: &str, typed: &str) -> Vec<Effect> {
        let query = typed.trim().trim_start_matches('@');
        let in_server =
            discord::parse_room(room).is_some_and(|(guild, _)| guild != discord::DM_GUILD);
        if self.state.preview
            || !in_server
            || query.is_empty()
            || mention_id(query).is_some()
            || self.state.option_suggestions().len() >= OPTION_SUGGESTIONS
            || !self.searched.insert(query.to_lowercase())
        {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SearchMembers {
                room: room.to_owned(),
                query: query.to_owned(),
            },
        ))]
    }

    /// [`Action::SetCommandOption`]: a file for an attachment option, or
    /// text for any other, as typed.
    pub(super) fn set_command_option(&mut self, index: usize, value: String) -> Vec<Effect> {
        let Some(command) = self.state.slash.command.as_mut() else {
            return vec![];
        };
        let Some(option) = command.options.get_mut(index) else {
            return vec![];
        };
        if option.kind == ATTACHMENT {
            option.file = (!value.is_empty()).then(|| PathBuf::from(value));
            return vec![];
        }
        option.text = value.clone();
        option.chosen = None;
        if command.focused == index {
            let room = command.room.clone();
            self.state.drafts.insert(room, value);
        }
        vec![]
    }

    /// [`Action::CloseCommand`]: Escape closes the list above the composer,
    /// then stops writing the command; with `back`, as Backspace in an
    /// empty first option, the command's name goes back in the composer.
    pub(super) fn close_command(&mut self, back: bool) -> Vec<Effect> {
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        if self.state.command_draft().is_none() {
            if self.state.slash_query().is_some() {
                self.state.slash.dismissed = true;
            }
            return vec![];
        }
        let shows_choices = !self.state.option_suggestions().is_empty();
        if !back
            && shows_choices
            && let Some(command) = self.state.slash.command.as_mut()
        {
            command.choices_dismissed = true;
            return vec![];
        }
        let Some(command) = self.state.slash.command.take() else {
            return vec![];
        };
        if back {
            let body = format!("/{}", command.name);
            self.state.drafts.insert(room.clone(), body.clone());
            self.state.slash.query = Some((room, command.name));
        } else {
            self.state.drafts.insert(room, String::new());
        }
        vec![]
    }

    /// Enter with a command being written: it runs when every option can
    /// be sent and none required is missing; otherwise each pill says
    /// what's wrong, and the first wrong one takes the composer.
    pub(super) fn run_written_command(&mut self, room: &str) -> Vec<Effect> {
        let Some(command) = self.state.command_draft() else {
            return vec![];
        };
        let known = OnceCell::new();
        let wrong = command.options.iter().position(|option| {
            match self.state.option_value_among(option, &known) {
                Err(_) => true,
                Ok(value) => value.is_none() && option.required,
            }
        });
        if let Some(wrong) = wrong {
            if let Some(command) = self.state.slash.command.as_mut() {
                command.tried = true;
            }
            return self.focus_option(wrong);
        }
        let mut options = Vec::new();
        let mut files = Vec::new();
        for option in &command.options {
            let value = if option.kind == ATTACHMENT {
                match &option.file {
                    // Its index among the files, a number, as Discord's
                    // client names it.
                    Some(path) => {
                        files.push(path.clone());
                        json!(files.len() - 1)
                    }
                    None => continue,
                }
            } else {
                match self.state.option_value_among(option, &known) {
                    Ok(Some(value)) => value,
                    _ => continue,
                }
            };
            options.push(json!({"type": option.kind, "name": option.name, "value": value}));
        }
        let run = CommandRun {
            // send_command gives it one.
            nonce: String::new(),
            room: room.to_owned(),
            application: command.application.clone(),
            command: command.command.clone(),
            options: nest(&command.path, options),
            target: None,
            files,
        };
        let Some(command) = self.state.slash.command.take() else {
            return vec![];
        };
        self.state.drafts.insert(room.to_owned(), String::new());
        self.start_command(run, command.app_name, command.app_icon)
    }

    /// Sends `run`, from `app_name`'s app, and shows it as sending until
    /// its answer comes: a command written in the composer, or one chosen
    /// from a menu's "Apps".
    pub(super) fn start_command(
        &mut self,
        run: CommandRun,
        app_name: String,
        app_icon: Option<String>,
    ) -> Vec<Effect> {
        if self.state.preview {
            return vec![];
        }
        self.state.slash.sent.push(SentCommand {
            run,
            app_name,
            app_icon,
            interaction: None,
            failed: None,
        });
        self.send_command(self.state.slash.sent.len() - 1)
    }

    /// Sends the sent command at `index`, again for a retry, with a new
    /// nonce, and shows it as sending.
    fn send_command(&mut self, index: usize) -> Vec<Effect> {
        let mut nonce = command_nonce();
        // A retry, or a control's click, within the same millisecond takes
        // the next one.
        while self.state.interactions.is_pending(&nonce)
            || self
                .state
                .slash
                .sent
                .iter()
                .any(|sent| sent.run.nonce == nonce)
        {
            nonce = match nonce.parse::<u64>() {
                Ok(number) => (number + 1).to_string(),
                Err(_) => command_nonce(),
            };
        }
        let sent = &mut self.state.slash.sent[index];
        sent.run.nonce = nonce.clone();
        sent.interaction = None;
        sent.failed = None;
        let run = sent.run.clone();
        // A form it answers with opens as a control's does.
        let guild = discord::parse_room(&run.room)
            .map(|(guild, _)| guild)
            .filter(|guild| *guild != discord::DM_GUILD)
            .map(str::to_owned);
        self.state.interactions.command_sent(nonce, guild);
        self.show_sent_commands();
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::RunCommand(Box::new(run)),
        ))]
    }

    /// Retry on a command that failed, by its row's item.
    pub(super) fn retry_command(&mut self, item: &str) -> Vec<Effect> {
        let nonce = item.strip_prefix(SENT_COMMAND).unwrap_or_default();
        match self
            .state
            .slash
            .sent
            .iter()
            .position(|sent| sent.run.nonce == nonce && sent.failed.is_some())
        {
            Some(index) => self.send_command(index),
            None => vec![],
        }
    }

    /// Cancel on a command that failed: its row goes.
    pub(super) fn cancel_command(&mut self, item: &str) {
        let nonce = item.strip_prefix(SENT_COMMAND).unwrap_or_default();
        self.state
            .slash
            .sent
            .retain(|sent| sent.run.nonce != nonce || sent.failed.is_none());
        self.show_sent_commands();
    }

    /// [`Event::InteractionCreated`]: the id its answer will name.
    pub(super) fn command_created(&mut self, nonce: &str, id: String) {
        if let Some(sent) = self.sent_command(nonce) {
            sent.interaction = Some(id);
        }
    }

    /// Discord's word on the command `nonce`: answered, when its row goes,
    /// or failed with `failure`, or neither in time when `settled`.
    pub(super) fn command_answered(&mut self, nonce: &str, failure: Option<String>, settled: bool) {
        let Some(sent) = self.sent_command(nonce) else {
            return;
        };
        match failure {
            Some(reason) => sent.failed = Some(reason),
            None if settled => {
                sent.failed = Some(
                    "Discord didn't confirm this command was sent. Check for the app's answer before sending it again."
                        .into(),
                );
            }
            None => self.state.slash.sent.retain(|sent| sent.run.nonce != nonce),
        }
        self.show_sent_commands();
    }

    fn sent_command(&mut self, nonce: &str) -> Option<&mut SentCommand> {
        self.state
            .slash
            .sent
            .iter_mut()
            .find(|sent| sent.run.nonce == nonce)
    }

    /// Puts the open room's sent commands under its messages, and lets go
    /// of those whose answer is among them.
    pub(super) fn show_sent_commands(&mut self) {
        let Some(room) = self.state.selected.as_deref() else {
            return;
        };
        let messages = &self.state.messages;
        let answered = |id: &String| {
            messages
                .iter()
                .any(|message| message.interaction.as_ref() == Some(id))
        };
        self.state.slash.sent.retain(|sent| {
            sent.run.room != room || !sent.interaction.as_ref().is_some_and(answered)
        });
        let rows: Vec<Arc<ChatMessage>> = self
            .state
            .slash
            .sent
            .iter()
            .filter(|sent| sent.run.room == room)
            .map(|sent| {
                let item = format!("{SENT_COMMAND}{}", sent.run.nonce);
                Arc::new(ChatMessage {
                    id: item.clone(),
                    item,
                    application_id: Some(sent.run.application.clone()),
                    author: sent.app_name.clone(),
                    avatar: sent.app_icon.clone(),
                    bot: true,
                    body: sent
                        .failed
                        .clone()
                        .unwrap_or_else(|| "Sending command…".into()),
                    delivery: if sent.failed.is_some() {
                        chat::Delivery::Failed
                    } else {
                        chat::Delivery::Sent
                    },
                    ..Default::default()
                })
            })
            .collect();
        let shown = messages
            .iter()
            .any(|message| message.item.starts_with(SENT_COMMAND));
        if !shown && rows.is_empty() {
            return;
        }
        let old = self.state.messages.clone();
        let new = with_rows(&old, &rows);
        // With blocked messages hidden, the full list takes the rows too,
        // and stays the one HideBlocked brings back.
        if let Some((all, hidden)) = self.with_blocked.as_mut()
            && Arc::ptr_eq(hidden, &old)
        {
            *all = with_rows(all, &rows);
            *hidden = new.clone();
        }
        self.state.messages = new;
    }
}

/// `messages` with the command rows `rows` in place of those it had.
fn with_rows(messages: &[Arc<ChatMessage>], rows: &[Arc<ChatMessage>]) -> TimelineArc {
    Arc::new(
        messages
            .iter()
            .filter(|message| !message.item.starts_with(SENT_COMMAND))
            .chain(rows)
            .cloned()
            .collect(),
    )
}

/// Wraps a command's options in its subcommand and group, as
/// `/interactions` takes them: `[{type: 1, name: "set", options: [...]}]`.
fn nest(path: &[String], options: Vec<Value>) -> Value {
    let mut options = Value::Array(options);
    for (depth, name) in path.iter().enumerate().rev() {
        let kind = if depth + 1 == path.len() {
            SUB_COMMAND
        } else {
            SUB_COMMAND_GROUP
        };
        options = json!([{"type": kind, "name": name, "options": options}]);
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_nest_in_their_group() {
        assert_eq!(
            nest(
                &["a".into(), "b".into()],
                vec![json!({"type": 3, "name": "x", "value": "y"})]
            ),
            json!([{"type": 2, "name": "a", "options": [
                {"type": 1, "name": "b", "options": [{"type": 3, "name": "x", "value": "y"}]}
            ]}])
        );
        assert_eq!(nest(&[], vec![]), json!([]));
    }
}
