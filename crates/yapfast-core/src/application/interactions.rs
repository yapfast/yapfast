//! Bots' buttons, select menus and the forms they open, used as this
//! account; see docs/discord/messages.md, "Bots' buttons, menus and forms".
//! Each control is busy on its own while its interaction waits for Discord:
//! a slow bot holds only the control that was used.

use std::collections::VecDeque;

use serde_json::{Value, json};

use super::*;
use crate::chat::Interaction;

fn interaction_nonce() -> String {
    #[cfg(feature = "discord")]
    {
        yapfast_discord::message::nonce()
    }
    #[cfg(not(feature = "discord"))]
    {
        // Only local model tests can submit Discord forms in this build.
        uuid::Uuid::new_v4().simple().to_string()
    }
}

/// How many answered interactions are remembered, for a form that comes
/// after Discord said the app answered.
const ANSWERED: usize = 16;

/// The interactions sent and not yet answered, by nonce, and those answered
/// lately.
#[derive(Default)]
pub struct Interactions {
    pending: HashMap<String, Pending>,
    /// Answered lately, oldest first, with the server each was in: a bot's
    /// form may come after Discord said it answered.
    answered: VecDeque<(String, Option<String>)>,
}

/// What an interaction was sent from.
#[derive(Clone, Debug)]
struct Pending {
    /// The message and control used; none for a form submitted.
    control: Option<(String, String)>,
    /// The server it was in, which a form it opens may leave out.
    guild: Option<String>,
}

impl Interactions {
    /// Whether the control `custom_id` under the message `item` waits for
    /// Discord.
    pub fn busy(&self, item: &str, custom_id: &str) -> bool {
        self.pending.values().any(|pending| {
            pending
                .control
                .as_ref()
                .is_some_and(|(at, id)| at == item && id == custom_id)
        })
    }

    /// The controls waiting for Discord, for the window: each as
    /// `\u{1f}item\u{1e}custom_id\u{1f}`, which a control finds itself in.
    pub fn busy_controls(&self) -> String {
        let mut controls: Vec<_> = self
            .pending
            .values()
            .filter_map(|pending| pending.control.as_ref())
            .map(|(item, id)| format!("\u{1f}{item}\u{1e}{id}\u{1f}"))
            .collect();
        controls.sort();
        controls.concat()
    }

    pub fn is_pending(&self, nonce: &str) -> bool {
        self.pending.contains_key(nonce)
    }

    /// A nonce for a new interaction: a snowflake for now, as Discord's
    /// client sends, one past any still in use from the same millisecond.
    fn nonce(&self) -> String {
        let mut nonce = interaction_nonce();
        let used = |nonce: &str| {
            self.pending.contains_key(nonce) || self.answered.iter().any(|(sent, _)| sent == nonce)
        };
        while used(&nonce) {
            nonce = match nonce.parse::<u64>() {
                Ok(number) => (number + 1).to_string(),
                Err(_) => interaction_nonce(),
            };
        }
        nonce
    }

    /// Takes `nonce` as a form submitted in `guild` that waits for Discord,
    /// as a test's start.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn pretend_sent(&mut self, nonce: &str, guild: Option<&str>) {
        self.start(
            nonce.to_owned(),
            Pending {
                control: None,
                guild: guild.map(str::to_owned),
            },
        );
    }

    /// Takes `nonce` as an app's command sent in `guild`, whose answer may
    /// be a form.
    pub(super) fn command_sent(&mut self, nonce: String, guild: Option<String>) {
        self.start(
            nonce,
            Pending {
                control: None,
                guild,
            },
        );
    }

    fn start(&mut self, nonce: String, pending: Pending) {
        self.pending.insert(nonce, pending);
    }

    /// Ends the interaction `nonce`, if this account sent it and it was
    /// pending; it is remembered as answered when `answered`.
    fn finish(&mut self, nonce: &str, answered: bool) -> Option<Pending> {
        let pending = self.pending.remove(nonce)?;
        if answered {
            if self.answered.len() == ANSWERED {
                self.answered.pop_front();
            }
            self.answered
                .push_back((nonce.to_owned(), pending.guild.clone()));
        }
        Some(pending)
    }

    /// The server of the interaction `nonce` a form answers, ending it,
    /// or none when this account didn't send it lately.
    fn answered_by_form(&mut self, nonce: &str) -> Option<Option<String>> {
        if let Some(pending) = self.pending.remove(nonce) {
            return Some(pending.guild);
        }
        let index = self.answered.iter().position(|(sent, _)| sent == nonce)?;
        self.answered.remove(index).map(|(_, guild)| guild)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SelectChoice {
    pub label: String,
    pub value: String,
    pub description: String,
    pub selected: bool,
    /// Shown before its label: the emoji itself, or a custom one by
    /// `:name:`; empty for none.
    pub emoji: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SelectMenu {
    pub item: String,
    pub room: String,
    pub custom_id: String,
    pub kind: u64,
    pub label: String,
    pub min: usize,
    pub max: usize,
    pub choices: Vec<SelectChoice>,
    pub query: String,
    pub modal_field: Option<usize>,
    /// The values picked when it opened: a menu of several options sends
    /// when it closes only if they changed.
    pub initial: Vec<String>,
}

impl SelectMenu {
    fn picked(&self) -> Vec<String> {
        self.choices
            .iter()
            .filter(|choice| choice.selected)
            .map(|choice| choice.value.clone())
            .collect()
    }
}

/// A field of a bot's form: a text input ([`TEXT_INPUT`]), a select menu
/// (3, 5-8), text to read ([`TEXT_DISPLAY`], Markdown in `value`), files
/// ([`FILE_UPLOAD`]), options to pick one of ([`RADIO_GROUP`]) or some of
/// ([`CHECKBOX_GROUP`]), a [`CHECKBOX`], or a part Yapfast can't fill
/// ([`UNSUPPORTED`]), which shows as such and keeps the form from being sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModalField {
    pub label: String,
    /// Its label's description, said under the label.
    pub description: String,
    pub custom_id: String,
    pub value: String,
    pub placeholder: String,
    pub required: bool,
    pub min_length: usize,
    pub max_length: usize,
    pub paragraph: bool,
    pub kind: u64,
    pub options: Vec<SelectChoice>,
    pub values: Vec<String>,
    pub min_values: usize,
    pub max_values: usize,
    pub channel_types: Vec<u64>,
    /// A checkbox's state.
    pub checked: bool,
    /// A file upload's files, in the order chosen.
    pub files: Vec<PathBuf>,
}

pub const TEXT_INPUT: u64 = 4;
pub const TEXT_DISPLAY: u64 = 10;
pub const FILE_UPLOAD: u64 = 19;
pub const RADIO_GROUP: u64 = 21;
pub const CHECKBOX_GROUP: u64 = 22;
pub const CHECKBOX: u64 = 23;
pub const UNSUPPORTED: u64 = 0;

/// Whether a field of this kind is a select menu.
fn select_kind(kind: u64) -> bool {
    matches!(kind, 3 | 5..=8)
}

/// Whether a field of this kind is filled by picking some of its `values`.
fn picked_kind(kind: u64) -> bool {
    select_kind(kind) || matches!(kind, RADIO_GROUP | CHECKBOX_GROUP)
}

/// Whether a form field of this kind is one Yapfast fills.
fn fillable(kind: u64) -> bool {
    picked_kind(kind) || matches!(kind, TEXT_INPUT | FILE_UPLOAD | CHECKBOX)
}

#[derive(Clone, Debug)]
pub struct InteractionModal {
    pub title: String,
    pub id: String,
    pub application_id: String,
    pub guild_id: Option<String>,
    pub channel_id: String,
    pub custom_id: String,
    pub fields: Vec<ModalField>,
    /// Every field is one Yapfast can fill.
    pub supported: bool,
    /// It was submitted, and waits for Discord.
    pub saving: bool,
    pub error: Option<String>,
    /// The nonce it was last submitted with.
    sent: Option<String>,
    shape: Value,
}

impl InteractionModal {
    /// The form INTERACTION_MODAL_CREATE gave, or none without what
    /// submitting it takes. A part Yapfast doesn't know shows as one and
    /// keeps it from being sent, rather than hiding the whole form.
    fn parse(value: Value) -> Option<Self> {
        let mut parsed = Vec::new();
        let mut supported = true;
        let unsupported = |label: Option<&str>| ModalField {
            label: label.unwrap_or_default().to_owned(),
            kind: UNSUPPORTED,
            required: false,
            ..ModalField::default()
        };
        for row in value["components"].as_array().into_iter().flatten() {
            let (outer_label, inner) = match row["type"].as_u64() {
                Some(1) => (None, &row["components"][0]),
                Some(LABEL) => (row["label"].as_str(), &row["component"]),
                Some(TEXT_DISPLAY) => {
                    parsed.push(ModalField {
                        kind: TEXT_DISPLAY,
                        value: row["content"].as_str().unwrap_or_default().to_owned(),
                        required: false,
                        ..ModalField::default()
                    });
                    continue;
                }
                _ => {
                    supported = false;
                    parsed.push(unsupported(row["label"].as_str()));
                    continue;
                }
            };
            let kind = inner["type"].as_u64().unwrap_or_default();
            let Some(custom_id) = inner["custom_id"].as_str().filter(|_| fillable(kind)) else {
                supported = false;
                parsed.push(unsupported(outer_label.or_else(|| inner["label"].as_str())));
                continue;
            };
            let options: Vec<SelectChoice> = inner["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|option| {
                    Some(SelectChoice {
                        label: option["label"].as_str()?.to_owned(),
                        value: option["value"].as_str()?.to_owned(),
                        description: option["description"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        selected: option["default"].as_bool().unwrap_or(false),
                        emoji: emoji(&option["emoji"]),
                    })
                })
                .collect();
            let values = options
                .iter()
                .filter(|choice| choice.selected)
                .map(|choice| choice.value.clone())
                .chain(
                    inner["default_values"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|value| value["id"].as_str().map(str::to_owned)),
                )
                .collect();
            // As docs.discord.food gives each kind's defaults: a file upload
            // and a checkbox may be left alone, a checkbox group takes up to
            // all of its options, a file upload up to ten files.
            let (required, min_values, max_values) = match kind {
                FILE_UPLOAD => (false, 0, 10),
                CHECKBOX => (false, 0, 1),
                RADIO_GROUP => (true, 1, 1),
                CHECKBOX_GROUP => (true, 1, options.len().max(1) as u64),
                _ => (true, 1, 1),
            };
            let required = inner["required"].as_bool().unwrap_or(required);
            let mut min_values = inner["min_values"].as_u64().unwrap_or(min_values) as usize;
            if kind == FILE_UPLOAD && required {
                min_values = min_values.max(1);
            }
            parsed.push(ModalField {
                label: outer_label
                    .or_else(|| inner["label"].as_str())
                    .unwrap_or("Answer")
                    .to_owned(),
                description: row["description"].as_str().unwrap_or_default().to_owned(),
                custom_id: custom_id.to_owned(),
                value: inner["value"].as_str().unwrap_or_default().to_owned(),
                placeholder: inner["placeholder"].as_str().unwrap_or_default().to_owned(),
                required,
                min_length: inner["min_length"].as_u64().unwrap_or_default() as usize,
                max_length: inner["max_length"].as_u64().unwrap_or(4000) as usize,
                paragraph: inner["style"].as_u64() == Some(2),
                kind,
                options,
                values,
                min_values,
                max_values: inner["max_values"].as_u64().unwrap_or(max_values) as usize,
                checked: kind == CHECKBOX && inner["default"].as_bool().unwrap_or(false),
                files: Vec::new(),
                channel_types: inner["channel_types"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .collect(),
            });
        }
        Some(Self {
            title: value["title"].as_str().unwrap_or_default().to_owned(),
            id: value["id"].as_str()?.to_owned(),
            application_id: value["application"]["id"]
                .as_str()
                .or_else(|| value["application_id"].as_str())?
                .to_owned(),
            guild_id: value["guild_id"].as_str().map(str::to_owned),
            channel_id: value["channel_id"].as_str()?.to_owned(),
            custom_id: value["custom_id"].as_str()?.to_owned(),
            fields: parsed,
            supported,
            saving: false,
            error: None,
            sent: None,
            shape: value["components"].clone(),
        })
    }

    fn valid(&self) -> bool {
        let count = |found: usize, field: &ModalField| {
            (!field.required && found == 0)
                || (found >= field.min_values.max(1) && found <= field.max_values)
        };
        self.supported
            && self.fields.iter().all(|field| {
                if matches!(field.kind, TEXT_DISPLAY | CHECKBOX) {
                    true
                } else if picked_kind(field.kind) {
                    count(field.values.len(), field)
                } else if field.kind == FILE_UPLOAD {
                    count(field.files.len(), field)
                } else {
                    let length = field.value.chars().count();
                    (!field.required && field.value.is_empty())
                        || (!field.value.is_empty()
                            && length >= field.min_length
                            && length <= field.max_length)
                }
            })
    }

    /// What was entered, as Discord takes it, and the files its file
    /// uploads carry, which their `values` name by place.
    fn submitted(&self) -> (Value, Vec<PathBuf>) {
        let mut rows = self.shape.clone();
        let mut files = Vec::new();
        if let Some(rows) = rows.as_array_mut() {
            rows.retain(|row| row["type"] != TEXT_DISPLAY);
        }
        for row in rows.as_array_mut().into_iter().flatten() {
            let label = row["type"] == LABEL;
            let inner = if label {
                &mut row["component"]
            } else {
                &mut row["components"][0]
            };
            let id = inner["custom_id"].as_str().unwrap_or_default();
            if let Some(field) = self.fields.iter().find(|field| field.custom_id == id) {
                *inner = match field.kind {
                    kind if select_kind(kind) || kind == CHECKBOX_GROUP => {
                        json!({"type": kind, "custom_id": id, "values": field.values})
                    }
                    // A radio group left alone, when it may be, says nothing.
                    RADIO_GROUP => match field.values.first() {
                        Some(value) => {
                            json!({"type": RADIO_GROUP, "custom_id": id, "value": value})
                        }
                        None => json!({"type": RADIO_GROUP, "custom_id": id}),
                    },
                    CHECKBOX => json!({"type": CHECKBOX, "custom_id": id, "value": field.checked}),
                    FILE_UPLOAD => {
                        let start = files.len();
                        files.extend(field.files.iter().cloned());
                        let ids: Vec<String> =
                            (start..files.len()).map(|at| at.to_string()).collect();
                        json!({"type": FILE_UPLOAD, "custom_id": id, "values": ids})
                    }
                    _ => json!({"type": TEXT_INPUT, "custom_id": id, "value": field.value}),
                };
            }
            let submitted = inner.clone();
            // The submit payload contains the input shape and values, not
            // labels, placeholders or validation metadata.
            if label {
                *row = json!({"type": LABEL, "component": submitted});
            } else {
                *row = json!({"type": 1, "components": [submitted]});
            }
        }
        (rows, files)
    }
}

/// The component that labels a form's field.
const LABEL: u64 = 18;

/// The most files a form carries, as an interaction's attachments.
const MOST_FILES: usize = 25;

/// A partial emoji object as an option shows it: the emoji itself, or a
/// custom one by `:name:`.
fn emoji(emoji: &Value) -> String {
    match (emoji["id"].as_str(), emoji["name"].as_str()) {
        (Some(_), Some(name)) => format!(":{name}:"),
        (None, Some(name)) => name.to_owned(),
        _ => String::new(),
    }
}

impl Application {
    pub(super) fn use_button(&mut self, item: String, custom_id: String) -> Vec<Effect> {
        if self.state.interactions.busy(&item, &custom_id) {
            return vec![];
        }
        let known = self.state.message(&item).is_some_and(|message| {
            message.cards.iter().any(|card| {
                card.rows.iter().any(|row| match row {
                    chat::CardRow::Buttons(buttons) => buttons.iter().any(|button| {
                        button.custom_id.as_deref() == Some(&custom_id) && !button.disabled
                    }),
                    chat::CardRow::Text {
                        beside: Some(chat::Beside::Button(button)),
                        ..
                    } => button.custom_id.as_deref() == Some(&custom_id) && !button.disabled,
                    _ => false,
                })
            })
        });
        if !known {
            return vec![];
        }
        self.component(item, 2, custom_id, None)
    }

    pub(super) fn open_select(&mut self, item: String, custom_id: String) -> Vec<Effect> {
        if self.state.interactions.busy(&item, &custom_id) {
            return vec![];
        }
        let Some(message) = self.state.message(&item) else {
            return vec![];
        };
        let Some((label, kind, options, defaults, channel_types, min, max)) = message
            .cards
            .iter()
            .flat_map(|card| &card.rows)
            .find_map(|row| match row {
                chat::CardRow::Select {
                    text,
                    disabled: false,
                    kind,
                    custom_id: id,
                    options,
                    defaults,
                    channel_types,
                    min_values,
                    max_values,
                } if id == &custom_id => Some((
                    text.clone(),
                    *kind,
                    options.clone(),
                    defaults.clone(),
                    channel_types.clone(),
                    *min_values as usize,
                    *max_values as usize,
                )),
                _ => None,
            })
        else {
            return vec![];
        };
        let room = self.state.room_of(&item).cloned().unwrap_or_default();
        let mut choices: Vec<SelectChoice> = options
            .into_iter()
            .map(|option| SelectChoice {
                label: option.label,
                value: option.value,
                description: option.description.unwrap_or_default(),
                selected: option.default,
                emoji: option.emoji.unwrap_or_default(),
            })
            .collect();
        choices.extend(self.entity_choices(kind, &room, &channel_types));
        for value in &defaults {
            if !choices.iter().any(|choice| &choice.value == value) {
                choices.push(SelectChoice {
                    label: value.clone(),
                    value: value.clone(),
                    description: String::new(),
                    selected: true,
                    ..Default::default()
                });
            }
        }
        for choice in &mut choices {
            choice.selected |= defaults.contains(&choice.value);
        }
        let mut seen = std::collections::HashSet::new();
        choices.retain(|choice| seen.insert(choice.value.clone()));
        let mut menu = SelectMenu {
            item,
            room,
            custom_id,
            kind,
            label,
            min,
            max,
            choices,
            query: String::new(),
            modal_field: None,
            initial: Vec::new(),
        };
        menu.initial = menu.picked();
        self.state.select_menu = Some(menu);
        vec![]
    }

    fn entity_choices(&self, kind: u64, room: &str, channel_types: &[u64]) -> Vec<SelectChoice> {
        let mut choices = Vec::new();
        if matches!(kind, 5 | 7) {
            if let Some(Ok(members)) = &self.state.members {
                choices.extend(members.iter().map(|member| SelectChoice {
                    label: member.name.clone(),
                    value: member.id.clone(),
                    description: member.username.clone().unwrap_or_default(),
                    selected: false,
                    ..Default::default()
                }));
            }
            choices.extend(self.state.messages.iter().filter_map(|message| {
                let id = message.sender.strip_prefix("discord:")?;
                (!message.bot).then(|| SelectChoice {
                    label: message.author.clone(),
                    value: id.to_owned(),
                    description: String::new(),
                    selected: false,
                    ..Default::default()
                })
            }));
            choices.extend(self.state.found_members.iter().map(|member| SelectChoice {
                label: member.name.clone(),
                value: member.id.clone(),
                description: member.username.clone().unwrap_or_default(),
                selected: false,
                ..Default::default()
            }));
        }
        if matches!(kind, 6 | 7) {
            choices.extend(
                self.server_roles(room)
                    .into_iter()
                    .map(|role| SelectChoice {
                        label: format!("@{}", role.name),
                        value: role.id,
                        description: "Role".into(),
                        selected: false,
                        ..Default::default()
                    }),
            );
        }
        if kind == 8
            && let Some((guild, _)) = discord::parse_room(room)
        {
            let prefix = format!("discord:{guild}:");
            let allowed = self.select_channels(room, channel_types);
            choices.extend(self.state.rooms.iter().filter_map(|room| {
                let id = room.id.strip_prefix(&prefix)?;
                (!room.locked && allowed.as_ref().is_none_or(|ids| ids.contains(id))).then(|| {
                    SelectChoice {
                        label: format!("#{}", room.name),
                        value: id.to_owned(),
                        description: String::new(),
                        selected: false,
                        ..Default::default()
                    }
                })
            }));
            if channel_types.is_empty() || channel_types.contains(&4) {
                choices.extend(
                    self.categories(&discord::server_id(guild))
                        .into_iter()
                        .filter_map(|category| {
                            Some(SelectChoice {
                                value: category.id.strip_prefix(&prefix)?.to_owned(),
                                label: category.name,
                                description: "Category".into(),
                                selected: false,
                                ..Default::default()
                            })
                        }),
                );
            }
        }
        choices
    }

    fn select_channels<'a>(&'a self, room: &'a str, types: &[u64]) -> Option<HashSet<&'a str>> {
        if types.is_empty() {
            return None;
        }
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.discord_place(room) {
            return Some(
                server
                    .text_channels
                    .iter()
                    .chain(&server.voice_channels)
                    .chain(&server.threads)
                    .filter(|channel| {
                        channel
                            .channel_type
                            .is_some_and(|kind| types.contains(&kind))
                    })
                    .map(|channel| channel.id.as_str())
                    .collect(),
            );
        }
        let _ = room;
        Some(HashSet::new())
    }

    pub(super) fn search_select(&mut self, query: String) -> Vec<Effect> {
        let room = self
            .state
            .select_menu
            .as_ref()
            .map(|menu| menu.room.clone());
        if let Some(menu) = &mut self.state.select_menu {
            menu.query = query.clone();
            if matches!(menu.kind, 5 | 7)
                && query.chars().count() >= 2
                && let Some(room) = room
            {
                return vec![Effect::Backend(Command::Discord(
                    DiscordCommand::SearchMembers { room, query },
                ))];
            }
        }
        vec![]
    }

    pub(super) fn select_member_matches(
        &mut self,
        room: &str,
        query: &str,
        members: &[ChatMember],
    ) {
        let menu_room = self
            .state
            .select_menu
            .as_ref()
            .map(|menu| menu.room.clone());
        let Some(menu) = &mut self.state.select_menu else {
            return;
        };
        if !matches!(menu.kind, 5 | 7) || menu.query != query || menu_room.as_deref() != Some(room)
        {
            return;
        }
        for member in members {
            if !menu.choices.iter().any(|choice| choice.value == member.id) {
                menu.choices.push(SelectChoice {
                    label: member.name.clone(),
                    value: member.id.clone(),
                    description: member.username.clone().unwrap_or_default(),
                    selected: false,
                    ..Default::default()
                });
            }
        }
    }

    /// Picks or unpicks the choice at `index` of the open menu. A menu of
    /// one option takes it at once, as Discord's does: under a message it
    /// is sent, in a form it is the field's.
    pub(super) fn toggle_select(&mut self, index: usize) -> Vec<Effect> {
        if let Some(menu) = &mut self.state.select_menu {
            if index >= menu.choices.len() {
                return vec![];
            }
            if menu.max == 1 {
                let clear = menu.min == 0 && menu.choices[index].selected;
                for (position, choice) in menu.choices.iter_mut().enumerate() {
                    choice.selected = !clear && position == index;
                }
                if menu.picked() == menu.initial {
                    self.state.select_menu = None;
                    return vec![];
                }
                return self.submit_select();
            }
            let chosen = menu.choices.iter().filter(|choice| choice.selected).count();
            if let Some(choice) = menu.choices.get_mut(index)
                && (choice.selected || chosen < menu.max)
            {
                choice.selected = !choice.selected;
            }
        }
        vec![]
    }

    /// Closes the open menu. One of several options takes what changed, as
    /// Discord's dropdown does when it closes; otherwise nothing is sent.
    pub(super) fn close_select(&mut self) -> Vec<Effect> {
        let Some(menu) = &self.state.select_menu else {
            return vec![];
        };
        let picked = menu.picked();
        if menu.max > 1
            && picked != menu.initial
            && picked.len() >= menu.min
            && picked.len() <= menu.max
        {
            return self.submit_select();
        }
        self.state.select_menu = None;
        vec![]
    }

    pub(super) fn submit_select(&mut self) -> Vec<Effect> {
        let Some(menu) = self.state.select_menu.as_ref() else {
            return vec![];
        };
        if menu.modal_field.is_none() && self.state.interactions.busy(&menu.item, &menu.custom_id) {
            return vec![];
        }
        let values: Vec<String> = menu
            .choices
            .iter()
            .filter(|choice| choice.selected)
            .map(|choice| choice.value.clone())
            .collect();
        if values.len() < menu.min || values.len() > menu.max {
            self.state.notice = Some("Choose the required number of options.".into());
            return vec![];
        }
        let menu = self.state.select_menu.take().unwrap();
        if let Some(index) = menu.modal_field {
            if let Some(field) = self
                .state
                .interaction_modal
                .as_mut()
                .and_then(|modal| modal.fields.get_mut(index))
            {
                let additions: Vec<_> = menu
                    .choices
                    .iter()
                    .filter(|choice| {
                        values.contains(&choice.value)
                            && !field
                                .options
                                .iter()
                                .any(|option| option.value == choice.value)
                    })
                    .cloned()
                    .collect();
                field.options.extend(additions);
                field.values = values;
            }
            return vec![];
        }
        self.component(menu.item, menu.kind, menu.custom_id, Some(values))
    }

    pub(super) fn open_modal_select(&mut self, index: usize) -> Vec<Effect> {
        let Some(modal) = self
            .state
            .interaction_modal
            .as_ref()
            .filter(|modal| !modal.saving)
        else {
            return vec![];
        };
        let Some(field) = modal.fields.get(index).cloned() else {
            return vec![];
        };
        if !select_kind(field.kind) {
            return vec![];
        }
        let room = discord::room_id(
            modal.guild_id.as_deref().unwrap_or(discord::DM_GUILD),
            &modal.channel_id,
        );
        let mut choices = field.options.clone();
        choices.extend(self.entity_choices(field.kind, &room, &field.channel_types));
        let mut seen = std::collections::HashSet::new();
        choices.retain(|choice| seen.insert(choice.value.clone()));
        for value in &field.values {
            if !choices.iter().any(|choice| &choice.value == value) {
                choices.push(SelectChoice {
                    label: value.clone(),
                    value: value.clone(),
                    description: String::new(),
                    selected: true,
                    ..Default::default()
                });
            }
        }
        for choice in &mut choices {
            choice.selected = field.values.contains(&choice.value);
        }
        self.state.select_menu = Some(SelectMenu {
            item: String::new(),
            room,
            custom_id: field.custom_id.clone(),
            kind: field.kind,
            label: field.label.clone(),
            min: if field.required { field.min_values } else { 0 },
            max: field.max_values,
            choices,
            query: String::new(),
            modal_field: Some(index),
            initial: field.values.clone(),
        });
        vec![]
    }

    /// Uses the control `custom_id` of kind `kind` under the message `item`.
    fn component(
        &mut self,
        item: String,
        kind: u64,
        custom_id: String,
        values: Option<Vec<String>>,
    ) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item) else {
            return vec![];
        };
        let Some((guild, channel)) = discord::parse_room(room) else {
            return vec![];
        };
        let Some(message) = self.state.message(&item) else {
            return vec![];
        };
        let Some(message_id) = item.strip_prefix("d:") else {
            return vec![];
        };
        // Only a bot's or an app's message has controls to use.
        let author = discord::sender_user(&message.sender).unwrap_or_default();
        if message.application_id.is_none() && !message.bot {
            self.state.notice = Some("Discord didn't say which app this control is for.".into());
            return vec![];
        }
        let guild = (guild != discord::DM_GUILD).then(|| guild.to_owned());
        let nonce = self.state.interactions.nonce();
        let interaction = Interaction::Component {
            nonce: nonce.clone(),
            guild: guild.clone(),
            channel: channel.to_owned(),
            message: message_id.to_owned(),
            message_flags: message.flags,
            application: message.application_id.clone(),
            author: author.to_owned(),
            component_type: kind,
            custom_id: custom_id.clone(),
            values,
        };
        self.state.interactions.start(
            nonce,
            Pending {
                control: Some((item, custom_id)),
                guild,
            },
        );
        vec![Effect::Backend(Command::Discord(DiscordCommand::Interact(
            Box::new(interaction),
        )))]
    }

    /// A bot's form, if it answers an interaction this account sent lately.
    /// One Yapfast can't read says so rather than leaving the click
    /// unanswered.
    pub(super) fn open_interaction_modal(&mut self, raw: Value) {
        let Some(guild) = raw["nonce"]
            .as_str()
            .and_then(|nonce| self.state.interactions.answered_by_form(nonce))
        else {
            return;
        };
        // A command answered with a form: its row goes.
        if let Some(nonce) = raw["nonce"].as_str() {
            self.command_answered(nonce, None, false);
        }
        let Some(mut modal) = InteractionModal::parse(raw) else {
            self.state.notice = Some("Yapfast couldn't show this app's form.".into());
            return;
        };
        if modal.guild_id.is_none() {
            modal.guild_id = guild;
        }
        let room = discord::room_id(
            modal.guild_id.as_deref().unwrap_or(discord::DM_GUILD),
            &modal.channel_id,
        );
        for index in 0..modal.fields.len() {
            let field = &modal.fields[index];
            if !matches!(field.kind, 5..=8) || field.values.is_empty() {
                continue;
            }
            let choices = self.entity_choices(field.kind, &room, &field.channel_types);
            let field = &mut modal.fields[index];
            for choice in choices {
                if field.values.contains(&choice.value)
                    && !field
                        .options
                        .iter()
                        .any(|option| option.value == choice.value)
                {
                    field.options.push(choice);
                }
            }
        }
        self.state.interaction_modal = Some(modal);
    }

    /// Discord's word on the interaction `nonce`: answered, or failed with
    /// `failure`, or neither in time when `settled`.
    pub(super) fn interaction_answered(
        &mut self,
        nonce: &str,
        failure: Option<String>,
        settled: bool,
    ) {
        self.command_answered(nonce, failure.clone(), settled);
        let Some(pending) = self
            .state
            .interactions
            .finish(nonce, failure.is_none() && !settled)
        else {
            return;
        };
        let modal = self
            .state
            .interaction_modal
            .as_mut()
            .filter(|modal| modal.saving && modal.sent.as_deref() == Some(nonce));
        match (modal, failure) {
            (Some(modal), Some(reason)) => {
                modal.saving = false;
                modal.error = Some(reason);
            }
            (Some(modal), None) if settled => {
                modal.saving = false;
                modal.error = Some(
                    "Discord didn't confirm this form was sent. Check the app's answer before sending it again."
                        .into(),
                );
            }
            (Some(_), None) => self.state.interaction_modal = None,
            (None, Some(reason)) if pending.control.is_some() => {
                self.state.notice = Some(reason);
            }
            (None, _) => {}
        }
    }

    /// The form's field at `index`, while it can be changed.
    fn modal_field(&mut self, index: usize) -> Option<&mut ModalField> {
        self.state
            .interaction_modal
            .as_mut()
            .filter(|modal| !modal.saving)
            .and_then(|modal| modal.fields.get_mut(index))
    }

    /// [`Action::SetModalField`].
    pub(super) fn set_modal_field(&mut self, index: usize, value: String) -> Vec<Effect> {
        let Some(field) = self.modal_field(index) else {
            return vec![];
        };
        let known = field.options.iter().any(|option| option.value == value);
        match field.kind {
            RADIO_GROUP if known => field.values = vec![value],
            CHECKBOX_GROUP if known => {
                if let Some(at) = field.values.iter().position(|picked| picked == &value) {
                    field.values.remove(at);
                } else if field.values.len() < field.max_values {
                    field.values.push(value);
                }
            }
            CHECKBOX => field.checked = !field.checked,
            TEXT_INPUT => field.value = value,
            _ => {}
        }
        vec![]
    }

    /// [`Action::AddModalFiles`]: up to as many as the upload takes, and
    /// the form's attachments may carry.
    pub(super) fn add_modal_files(&mut self, index: usize, files: Vec<PathBuf>) -> Vec<Effect> {
        let carried: usize = self
            .state
            .interaction_modal
            .as_ref()
            .map(|modal| modal.fields.iter().map(|field| field.files.len()).sum())
            .unwrap_or_default();
        let Some(field) = self
            .modal_field(index)
            .filter(|field| field.kind == FILE_UPLOAD)
        else {
            return vec![];
        };
        let room = field
            .max_values
            .saturating_sub(field.files.len())
            .min(MOST_FILES.saturating_sub(carried));
        if files.len() > room {
            let notice = format!(
                "{} takes up to {} files.",
                field.label,
                field.max_values.min(MOST_FILES)
            );
            field.files.extend(files.into_iter().take(room));
            self.state.notice = Some(notice);
        } else {
            field.files.extend(files);
        }
        vec![]
    }

    /// [`Action::RemoveModalFile`].
    pub(super) fn remove_modal_file(&mut self, index: usize, file: usize) -> Vec<Effect> {
        if let Some(field) = self.modal_field(index)
            && file < field.files.len()
        {
            field.files.remove(file);
        }
        vec![]
    }

    pub(super) fn submit_modal(&mut self) -> Vec<Effect> {
        let Some(modal) = self.state.interaction_modal.as_mut() else {
            return vec![];
        };
        if modal.saving {
            return vec![];
        }
        if !modal.valid() {
            self.state.notice = Some("Complete the required fields before submitting.".into());
            return vec![];
        }
        let nonce = self.state.interactions.nonce();
        let (components, files) = modal.submitted();
        let interaction = Interaction::Modal {
            nonce: nonce.clone(),
            guild: modal.guild_id.clone(),
            channel: modal.channel_id.clone(),
            application: modal.application_id.clone(),
            id: modal.id.clone(),
            custom_id: modal.custom_id.clone(),
            components,
            files,
        };
        modal.saving = true;
        modal.error = None;
        modal.sent = Some(nonce.clone());
        let guild = modal.guild_id.clone();
        self.state.interactions.start(
            nonce,
            Pending {
                control: None,
                guild,
            },
        );
        vec![Effect::Backend(Command::Discord(DiscordCommand::Interact(
            Box::new(interaction),
        )))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one interaction waiting for Discord.
    fn only_pending(app: &Application) -> String {
        let pending = &app.state.interactions.pending;
        assert_eq!(pending.len(), 1, "one interaction pending");
        pending.keys().next().unwrap().clone()
    }

    /// A bot's message in #7 of server 10 with buttons `ids`.
    fn with_buttons(app: &mut Application, item: &str, ids: &[&str]) {
        app.state.selected = Some("discord:10:7".into());
        let message = Arc::new(ChatMessage {
            item: item.into(),
            sender: "discord:3".into(),
            bot: true,
            cards: vec![chat::Card {
                rows: vec![chat::CardRow::Buttons(
                    ids.iter()
                        .map(|id| chat::CardButton {
                            label: id.to_string(),
                            style: chat::ButtonStyle::Primary,
                            url: None,
                            custom_id: Some(id.to_string()),
                            sku_id: None,
                            disabled: false,
                        })
                        .collect(),
                )],
                ..Default::default()
            }],
            ..Default::default()
        });
        let mut messages = app.state.messages.as_ref().clone();
        messages.push(message);
        app.state.messages = Arc::new(messages);
    }

    fn modal_with(component: Value) -> InteractionModal {
        InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "guild_id": "10", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit",
            "components": [{"type": 18, "label": "Answer", "component": component}]
        }))
        .unwrap()
    }

    #[test]
    fn optional_single_select_can_clear_a_default_and_submit_empty() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 3, "custom_id": "pick", "required": false,
            "options": [{"label": "A", "value": "a", "default": true}]
        })));
        app.dispatch(Action::OpenModalSelect(0));
        assert_eq!(app.state.select_menu.as_ref().unwrap().min, 0);
        app.dispatch(Action::ToggleSelect(0));
        app.dispatch(Action::SubmitSelect);
        assert!(app.state.select_menu.is_none());
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(modal.fields[0].values.is_empty());
        assert!(modal.valid());
    }

    #[test]
    fn required_single_select_keeps_its_selection() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 3, "custom_id": "pick",
            "options": [{"label": "A", "value": "a", "default": true}]
        })));
        app.dispatch(Action::OpenModalSelect(0));
        app.dispatch(Action::ToggleSelect(0));
        // Picking it again closes the menu, the pick unchanged.
        assert!(app.state.select_menu.is_none());
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert_eq!(modal.fields[0].values, ["a"]);
    }

    #[test]
    fn failed_modal_submission_keeps_values_and_can_be_retried() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 4, "custom_id": "reason", "value": "Keep this answer"
        })));
        let effects = app.dispatch(Action::SubmitModal);
        assert!(matches!(
            effects.first(),
            Some(Effect::Backend(Command::Discord(DiscordCommand::Interact(
                _
            ))))
        ));
        assert!(app.state.interaction_modal.as_ref().unwrap().saving);
        assert!(app.dispatch(Action::SubmitModal).is_empty());
        app.dispatch(Action::CloseModal);
        assert!(app.state.interaction_modal.is_some());
        let nonce = only_pending(&app);
        app.apply(Event::InteractionFailed {
            nonce,
            reason: "Offline".into(),
        });
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(!modal.saving);
        assert_eq!(modal.fields[0].value, "Keep this answer");
        assert_eq!(modal.error.as_deref(), Some("Offline"));
        assert!(app.state.interactions.pending.is_empty());
        app.dispatch(Action::SubmitModal);
        let nonce = only_pending(&app);
        app.apply(Event::InteractionSucceeded(nonce));
        assert!(app.state.interaction_modal.is_none());
    }

    #[test]
    fn unconfirmed_modal_submission_keeps_the_form() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 4, "custom_id": "reason", "value": "Keep this answer"
        })));
        app.dispatch(Action::SubmitModal);
        let nonce = only_pending(&app);
        app.apply(Event::InteractionSettled(nonce));
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(!modal.saving);
        assert!(modal.error.is_some());
        assert_eq!(modal.fields[0].value, "Keep this answer");
    }

    #[test]
    fn bot_forms_block_the_quick_switcher() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 3, "custom_id": "pick", "options": [{"label": "A", "value": "a"}]
        })));
        assert!(app.state.dialog_open());
        app.dispatch(Action::ToggleSwitcher);
        assert!(app.state.switcher.is_none());
        app.dispatch(Action::OpenModalSelect(0));
        app.state.interaction_modal = None;
        assert!(app.state.dialog_open());
        app.dispatch(Action::ToggleSwitcher);
        assert!(app.state.switcher.is_none());
        app.dispatch(Action::CloseSelect);
        assert!(!app.state.dialog_open());
    }

    #[test]
    fn text_in_a_form_shows_and_is_left_out_of_what_it_sends() {
        let raw = json!({
            "id": "42", "channel_id": "7", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit",
            "components": [
                {"type": 10, "content": "Warning: this action is permanent."},
                {"type": 1, "components": [{
                    "type": 4, "custom_id": "answer", "value": "Ready"
                }]}
            ]
        });
        let modal = InteractionModal::parse(raw).unwrap();
        assert!(modal.supported);
        assert_eq!(modal.fields[0].kind, TEXT_DISPLAY);
        assert_eq!(modal.fields[0].value, "Warning: this action is permanent.");
        assert!(modal.valid());
        assert_eq!(
            modal.submitted().0,
            json!([{"type": 1, "components": [{"type": 4, "custom_id": "answer", "value": "Ready"}]}])
        );
    }

    #[test]
    fn a_form_with_a_part_yapfast_cant_fill_still_shows_the_rest() {
        let modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit",
            "components": [
                {"type": 18, "label": "Name", "component": {"type": 4, "custom_id": "name"}},
                {"type": 18, "label": "Proof", "component": {"type": 99, "custom_id": "file"}},
                {"type": 1, "components": []},
                {"type": 99}
            ]
        }))
        .unwrap();
        assert!(!modal.supported);
        assert!(!modal.valid());
        let kinds: Vec<_> = modal.fields.iter().map(|field| field.kind).collect();
        assert_eq!(kinds, [TEXT_INPUT, UNSUPPORTED, UNSUPPORTED, UNSUPPORTED]);
        assert_eq!(modal.fields[1].label, "Proof");
    }

    #[test]
    fn a_form_yapfast_cant_read_says_so() {
        let mut app = Application::default();
        with_buttons(&mut app, "d:1", &["form"]);
        app.dispatch(Action::UseButton {
            item: "d:1".into(),
            custom_id: "form".into(),
        });
        let nonce = only_pending(&app);
        app.apply(Event::InteractionModal(
            json!({"nonce": nonce, "title": "No id"}),
        ));
        assert!(app.state.interaction_modal.is_none());
        assert_eq!(
            app.state.notice.as_deref(),
            Some("Yapfast couldn't show this app's form.")
        );
        // One this account didn't ask for doesn't open.
        app.state.notice = None;
        app.apply(Event::InteractionModal(json!({
            "nonce": "someone else's", "id": "1", "channel_id": "7", "custom_id": "x",
            "application": {"id": "3"}, "components": []
        })));
        assert!(app.state.interaction_modal.is_none());
        assert!(app.state.notice.is_none());
    }

    #[test]
    fn a_form_that_comes_after_its_success_still_opens() {
        let mut app = Application::default();
        with_buttons(&mut app, "d:1", &["form"]);
        app.dispatch(Action::UseButton {
            item: "d:1".into(),
            custom_id: "form".into(),
        });
        let nonce = only_pending(&app);
        app.apply(Event::InteractionSucceeded(nonce.clone()));
        assert!(app.state.interactions.pending.is_empty());
        app.apply(Event::InteractionModal(json!({
            "nonce": nonce, "id": "1", "channel_id": "7", "custom_id": "x",
            "application": {"id": "3"}, "title": "Notes", "components": []
        })));
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert_eq!(modal.guild_id.as_deref(), Some("10"));
    }

    #[test]
    fn two_bots_controls_wait_for_discord_each_on_their_own() {
        let mut app = Application::default();
        with_buttons(&mut app, "d:1", &["ok", "slow"]);
        with_buttons(&mut app, "d:2", &["ok"]);
        let click = |app: &mut Application, item: &str, id: &str| {
            app.dispatch(Action::UseButton {
                item: item.into(),
                custom_id: id.into(),
            })
        };
        assert_eq!(click(&mut app, "d:1", "slow").len(), 1);
        // The same control again waits; the others still work.
        assert!(click(&mut app, "d:1", "slow").is_empty());
        assert_eq!(click(&mut app, "d:1", "ok").len(), 1);
        assert_eq!(click(&mut app, "d:2", "ok").len(), 1);
        assert!(app.state.interactions.busy("d:1", "slow"));
        assert!(app.state.interactions.busy("d:2", "ok"));
        assert_eq!(
            app.state.interactions.busy_controls(),
            "\u{1f}d:1\u{1e}ok\u{1f}\u{1f}d:1\u{1e}slow\u{1f}\u{1f}d:2\u{1e}ok\u{1f}"
        );
        // A failure shows its reason and frees only its control.
        let nonce = app
            .state
            .interactions
            .pending
            .iter()
            .find(|(_, pending)| pending.control == Some(("d:2".into(), "ok".into())))
            .map(|(nonce, _)| nonce.clone())
            .unwrap();
        app.apply(Event::InteractionFailed {
            nonce,
            reason: "This interaction failed.".into(),
        });
        assert_eq!(
            app.state.notice.as_deref(),
            Some("This interaction failed.")
        );
        assert!(!app.state.interactions.busy("d:2", "ok"));
        assert!(app.state.interactions.busy("d:1", "slow"));
        assert_eq!(app.state.interactions.pending.len(), 2);
    }

    #[cfg(feature = "discord")]
    #[test]
    fn message_and_modal_channel_selects_only_offer_allowed_types() {
        use yapfast_discord::{
            ready::{Category, Channel, Server},
            service::Snapshot,
        };
        let mut app = Application {
            discord: Some(Arc::new(Snapshot {
                user_id: Some("me".into()),
                servers: vec![
                    Arc::new(Server {
                        id: "10".into(),
                        owner_id: Some("me".into()),
                        categories: Arc::new(HashMap::from([(
                            "4".into(),
                            Category {
                                name: "Projects".into(),
                                ..Default::default()
                            },
                        )])),
                        text_channels: [0, 5, 15, 16]
                            .into_iter()
                            .map(|kind| Channel {
                                id: kind.to_string(),
                                channel_type: Some(kind),
                                ..Default::default()
                            })
                            .collect(),
                        voice_channels: [2, 13]
                            .into_iter()
                            .map(|kind| Channel {
                                id: kind.to_string(),
                                channel_type: Some(kind),
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    })
                    .into(),
                ],
                ..Default::default()
            })),
            ..Default::default()
        };
        app.state.rooms = [0, 2, 5, 13, 15, 16]
            .into_iter()
            .map(|kind| {
                Arc::new(super::super::preview::preview_room(
                    &format!("discord:10:{kind}"),
                    &kind.to_string(),
                    false,
                    0,
                ))
            })
            .collect();
        app.state.selected = Some("discord:10:0".into());
        app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "d:1".into(),
            cards: vec![chat::Card {
                rows: vec![chat::CardRow::Select {
                    text: "Text channel".into(),
                    disabled: false,
                    kind: 8,
                    custom_id: "channel".into(),
                    options: vec![],
                    defaults: vec![],
                    channel_types: vec![0],
                    min_values: 1,
                    max_values: 1,
                }],
                ..Default::default()
            }],
            ..Default::default()
        })]);
        app.dispatch(Action::OpenSelect {
            item: "d:1".into(),
            custom_id: "channel".into(),
        });
        let menu = app.state.select_menu.as_ref().unwrap();
        assert_eq!(
            menu.choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            ["0"]
        );
        app.dispatch(Action::CloseSelect);
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 8, "custom_id": "channel", "channel_types": [2, 13]
        })));
        app.dispatch(Action::OpenModalSelect(0));
        let menu = app.state.select_menu.as_ref().unwrap();
        assert_eq!(
            menu.choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            ["2", "13"]
        );
        app.dispatch(Action::CloseSelect);
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 8, "custom_id": "category", "channel_types": [4]
        })));
        app.dispatch(Action::OpenModalSelect(0));
        let choices = &app.state.select_menu.as_ref().unwrap().choices;
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].value, "4");
        assert_eq!(choices[0].label, "Projects");
        app.dispatch(Action::ToggleSelect(0));
        app.dispatch(Action::SubmitSelect);
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(modal.valid());
        assert_eq!(modal.submitted().0[0]["component"]["values"], json!(["4"]));
        assert!(
            app.entity_choices(8, "discord:10:0", &[])
                .iter()
                .any(|choice| choice.value == "4")
        );
    }

    #[test]
    #[cfg(feature = "discord")]
    fn nonce_uses_discord_snowflake_format() {
        let nonce = interaction_nonce().parse::<u64>().unwrap();
        let current = yapfast_discord::message::nonce().parse::<u64>().unwrap();
        assert_eq!(nonce & ((1 << 22) - 1), 0);
        assert!(nonce <= current);
        assert!(current - nonce < (1 << 22) * 1000);
    }

    #[test]
    fn unidentified_non_bot_message_controls_are_rejected() {
        let mut app = Application::default();
        app.state.selected = Some("discord:10:7".into());
        app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "d:1".into(),
            sender: "discord:3".into(),
            ..Default::default()
        })]);
        assert!(
            app.component("d:1".into(), 2, "click".into(), None)
                .is_empty()
        );
        assert_eq!(
            app.state.notice.as_deref(),
            Some("Discord didn't say which app this control is for.")
        );
        assert!(app.state.interactions.pending.is_empty());
    }

    #[test]
    fn bot_controls_without_application_ids_are_resolved_by_the_backend() {
        let mut app = Application::default();
        app.state.selected = Some("discord:10:7".into());
        app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "d:1".into(),
            sender: "discord:3".into(),
            bot: true,
            ..Default::default()
        })]);
        let effects = app.component("d:1".into(), 2, "click".into(), None);
        let [Effect::Backend(Command::Discord(DiscordCommand::Interact(interaction)))] =
            effects.as_slice()
        else {
            panic!("a click goes to Discord");
        };
        let Interaction::Component {
            application,
            author,
            message,
            guild,
            ..
        } = interaction.as_ref()
        else {
            panic!("a component's interaction");
        };
        // The backend knows the bot's application, or takes the bot's id.
        assert_eq!(*application, None);
        assert_eq!(author, "3");
        assert_eq!(message, "1");
        assert_eq!(guild.as_deref(), Some("10"));
        only_pending(&app);
        assert!(app.state.notice.is_none());
    }

    #[test]
    fn unanswered_interactions_release_only_their_control() {
        let mut app = Application::default();
        with_buttons(&mut app, "d:1", &["first", "second"]);
        for id in ["first", "second"] {
            app.dispatch(Action::UseButton {
                item: "d:1".into(),
                custom_id: id.into(),
            });
        }
        let first = app
            .state
            .interactions
            .pending
            .iter()
            .find(|(_, pending)| pending.control.as_ref().unwrap().1 == "first")
            .map(|(nonce, _)| nonce.clone())
            .unwrap();
        app.apply(Event::InteractionSettled(first.clone()));
        assert!(!app.state.interactions.busy("d:1", "first"));
        assert!(app.state.interactions.busy("d:1", "second"));
        // Its word coming late changes nothing.
        app.apply(Event::InteractionFailed {
            nonce: first,
            reason: "late".into(),
        });
        assert!(app.state.notice.is_none());
        assert!(app.state.interactions.busy("d:1", "second"));
    }

    #[test]
    fn modal_submission_preserves_input_shape_and_values() {
        let mut modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit setting", "nonce": "click",
            "components": [
                {"type": 1, "components": [{"type": 4, "custom_id": "short", "label": "Name", "style": 1, "required": true}]},
                {"type": 18, "label": "Reason", "component": {"type": 4, "custom_id": "long", "style": 2, "required": false}}
            ]
        })).unwrap();
        assert!(modal.supported);
        assert!(!modal.valid());
        modal.fields[0].value = "Updated".into();
        modal.fields[1].value = "Because".into();
        assert!(modal.valid());
        assert_eq!(
            modal.submitted().0,
            json!([
                {"type": 1, "components": [{"type": 4, "custom_id": "short", "value": "Updated"}]},
                {"type": 18, "component": {"type": 4, "custom_id": "long", "value": "Because"}}
            ])
        );
    }

    #[test]
    fn modal_submission_includes_selected_values() {
        let mut modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "custom_id": "assign",
            "application": {"id": "3"}, "title": "Assign role",
            "components": [
                {"type": 18, "label": "Role", "component": {
                    "type": 6, "custom_id": "role", "min_values": 1, "max_values": 1,
                    "default_values": [{"id": "11", "type": "role"}]
                }}
            ]
        }))
        .unwrap();
        assert!(modal.valid());
        assert_eq!(
            modal.submitted().0,
            json!([
                {"type": 18, "component": {"type": 6, "custom_id": "role", "values": ["11"]}}
            ])
        );
        modal.fields[0].values.clear();
        assert!(!modal.valid());
    }

    /// A bot's message in #7 of server 10 with a menu `id` of options a, b
    /// and c, which picks `max` of them.
    fn with_menu(app: &mut Application, id: &str, max: u64) {
        app.state.selected = Some("discord:10:7".into());
        let option = |value: &str| chat::CardOption {
            label: value.to_uppercase(),
            value: value.into(),
            description: None,
            default: value == "a",
            emoji: (value == "b").then(|| ":party:".into()),
        };
        app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "d:1".into(),
            sender: "discord:3".into(),
            bot: true,
            cards: vec![chat::Card {
                rows: vec![chat::CardRow::Select {
                    text: "A".into(),
                    disabled: false,
                    kind: 3,
                    custom_id: id.into(),
                    options: ["a", "b", "c"].into_iter().map(option).collect(),
                    defaults: vec![],
                    channel_types: vec![],
                    min_values: 1,
                    max_values: max,
                }],
                ..Default::default()
            }],
            ..Default::default()
        })]);
    }

    /// The values the one interaction sent picks.
    fn sent_values(effects: &[Effect]) -> Vec<String> {
        let [Effect::Backend(Command::Discord(DiscordCommand::Interact(interaction)))] = effects
        else {
            panic!("one interaction is sent");
        };
        let Interaction::Component {
            values: Some(values),
            ..
        } = interaction.as_ref()
        else {
            panic!("a menu's pick");
        };
        values.clone()
    }

    #[test]
    fn a_single_menu_sends_its_pick_at_once() {
        let mut app = Application::default();
        with_menu(&mut app, "env", 1);
        app.dispatch(Action::OpenSelect {
            item: "d:1".into(),
            custom_id: "env".into(),
        });
        let menu = app.state.select_menu.as_ref().unwrap();
        assert_eq!(menu.choices[1].emoji, ":party:");
        assert_eq!(menu.initial, ["a"]);
        // The option already picked closes it, sending nothing.
        assert!(app.dispatch(Action::ToggleSelect(0)).is_empty());
        assert!(app.state.select_menu.is_none());
        app.dispatch(Action::OpenSelect {
            item: "d:1".into(),
            custom_id: "env".into(),
        });
        let effects = app.dispatch(Action::ToggleSelect(1));
        assert_eq!(sent_values(&effects), ["b"]);
        assert!(app.state.select_menu.is_none());
        assert!(app.state.interactions.busy("d:1", "env"));
    }

    #[test]
    fn a_menu_of_several_sends_when_it_closes_if_its_picks_changed() {
        let mut app = Application::default();
        with_menu(&mut app, "checks", 3);
        let open = |app: &mut Application| {
            app.dispatch(Action::OpenSelect {
                item: "d:1".into(),
                custom_id: "checks".into(),
            })
        };
        open(&mut app);
        // Unchanged: nothing goes.
        assert!(app.dispatch(Action::CloseSelect).is_empty());
        open(&mut app);
        // Fewer than it takes: nothing goes either.
        app.dispatch(Action::ToggleSelect(0));
        assert!(app.state.select_menu.is_some());
        assert!(app.dispatch(Action::CloseSelect).is_empty());
        assert!(app.state.select_menu.is_none());
        open(&mut app);
        app.dispatch(Action::ToggleSelect(2));
        let effects = app.dispatch(Action::CloseSelect);
        assert_eq!(sent_values(&effects), ["a", "c"]);
    }

    #[test]
    fn a_forms_single_menu_takes_its_pick_and_closes() {
        let mut app = Application::default();
        app.state.interaction_modal = Some(modal_with(json!({
            "type": 3, "custom_id": "pick",
            "options": [{"label": "A", "value": "a"}, {"label": "B", "value": "b", "emoji": {"name": "🅱️"}}]
        })));
        app.dispatch(Action::OpenModalSelect(0));
        assert_eq!(
            app.state.select_menu.as_ref().unwrap().choices[1].emoji,
            "🅱️"
        );
        assert!(app.dispatch(Action::ToggleSelect(1)).is_empty());
        assert!(app.state.select_menu.is_none());
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert_eq!(modal.fields[0].values, ["b"]);
    }

    #[test]
    fn radio_checkbox_and_file_fields_fill_and_submit_as_discord_takes_them() {
        let mut app = Application::default();
        let modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "guild_id": "10", "custom_id": "survey",
            "application": {"id": "3"}, "title": "Survey",
            "components": [
                {"type": 10, "content": "## Before you start\n**Read** this."},
                {"type": 18, "label": "Size", "description": "Pick one", "component": {
                    "type": 21, "custom_id": "size", "options": [
                        {"label": "Small", "value": "s"}, {"label": "Large", "value": "l", "default": true}]}},
                {"type": 18, "label": "Areas", "component": {
                    "type": 22, "custom_id": "areas", "max_values": 2, "options": [
                        {"label": "Chat", "value": "chat"}, {"label": "Calls", "value": "calls"},
                        {"label": "Bots", "value": "bots"}]}},
                {"type": 18, "label": "Agree", "component": {"type": 23, "custom_id": "agree"}},
                {"type": 18, "label": "Screenshots", "component": {
                    "type": 19, "custom_id": "shots", "max_values": 2, "required": true}},
                {"type": 18, "label": "Logs", "component": {"type": 19, "custom_id": "logs"}},
            ]
        }))
        .unwrap();
        assert!(modal.supported);
        let kinds: Vec<_> = modal.fields.iter().map(|field| field.kind).collect();
        assert_eq!(
            kinds,
            [
                TEXT_DISPLAY,
                RADIO_GROUP,
                CHECKBOX_GROUP,
                CHECKBOX,
                FILE_UPLOAD,
                FILE_UPLOAD
            ]
        );
        assert_eq!(modal.fields[1].description, "Pick one");
        assert_eq!(modal.fields[1].values, ["l"]);
        assert!(!modal.fields[5].required);
        // A required upload and a checkbox group without a pick hold it back.
        assert!(!modal.valid());
        app.state.interaction_modal = Some(modal);
        let set = |app: &mut Application, index: usize, value: &str| {
            app.dispatch(Action::SetModalField {
                index,
                value: value.into(),
            });
        };
        set(&mut app, 1, "s");
        set(&mut app, 1, "nothing offered");
        for value in ["chat", "calls", "bots", "chat"] {
            set(&mut app, 2, value);
        }
        set(&mut app, 3, "");
        app.dispatch(Action::AddModalFiles {
            index: 4,
            files: vec![
                "/tmp/a.png".into(),
                "/tmp/b.png".into(),
                "/tmp/c.png".into(),
            ],
        });
        assert_eq!(
            app.state.notice.as_deref(),
            Some("Screenshots takes up to 2 files.")
        );
        app.dispatch(Action::RemoveModalFile { index: 4, file: 0 });
        app.dispatch(Action::AddModalFiles {
            index: 5,
            files: vec!["/tmp/log.txt".into()],
        });
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(modal.valid());
        let (components, files) = modal.submitted();
        assert_eq!(
            components,
            json!([
                {"type": 18, "component": {"type": 21, "custom_id": "size", "value": "s"}},
                {"type": 18, "component": {"type": 22, "custom_id": "areas", "values": ["calls"]}},
                {"type": 18, "component": {"type": 23, "custom_id": "agree", "value": true}},
                {"type": 18, "component": {"type": 19, "custom_id": "shots", "values": ["0"]}},
                {"type": 18, "component": {"type": 19, "custom_id": "logs", "values": ["1"]}},
            ])
        );
        assert_eq!(
            files,
            [PathBuf::from("/tmp/b.png"), PathBuf::from("/tmp/log.txt")]
        );
        let effects = app.dispatch(Action::SubmitModal);
        let [Effect::Backend(Command::Discord(DiscordCommand::Interact(interaction)))] =
            effects.as_slice()
        else {
            panic!("the form is sent");
        };
        assert!(
            matches!(interaction.as_ref(), Interaction::Modal { files, .. } if files.len() == 2)
        );
    }
}
