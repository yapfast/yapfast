//! Bot message controls and the modals they open.

use serde_json::{Value, json};

use super::*;

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

/// A bot's control used, or its form sent, that Discord hasn't answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingInteraction {
    pub nonce: String,
    /// The message whose control it was, or empty for a form.
    pub item: String,
    pub room: String,
}

impl AppState {
    /// Whether a control of the message `item` waits for its bot, which
    /// holds back its other controls, as in Discord's client.
    pub fn interaction_pending(&self, item: &str) -> bool {
        self.pending_interactions
            .iter()
            .any(|pending| pending.item == item)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SelectChoice {
    pub label: String,
    pub value: String,
    pub description: String,
    pub selected: bool,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModalField {
    pub label: String,
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
    pub supported: bool,
    pub error: Option<String>,
    /// The nonce it was sent with, until Discord answers.
    pub(super) nonce: Option<String>,
    shape: Value,
}

/// A text display's kind, which a form shows and sends back empty.
const TEXT_DISPLAY: u64 = 10;

impl InteractionModal {
    /// Whether it was sent and waits for Discord.
    pub fn saving(&self) -> bool {
        self.nonce.is_some()
    }

    fn parse(value: Value) -> Option<Self> {
        let fields = value["components"].as_array()?;
        let mut parsed = Vec::new();
        let mut supported = true;
        for row in fields {
            let (outer_label, inner) = match row["type"].as_u64() {
                Some(1) => (None, row["components"].as_array()?.first()?),
                Some(18) => (row["label"].as_str(), &row["component"]),
                Some(TEXT_DISPLAY) => {
                    parsed.push(ModalField {
                        label: row["content"].as_str().unwrap_or_default().to_owned(),
                        custom_id: String::new(),
                        value: String::new(),
                        placeholder: String::new(),
                        required: false,
                        min_length: 0,
                        max_length: 0,
                        paragraph: false,
                        kind: TEXT_DISPLAY,
                        options: Vec::new(),
                        values: Vec::new(),
                        min_values: 0,
                        max_values: 0,
                        channel_types: Vec::new(),
                    });
                    continue;
                }
                _ => {
                    supported = false;
                    continue;
                }
            };
            let kind = inner["type"].as_u64().unwrap_or_default();
            if !matches!(kind, 3..=8) {
                supported = false;
                continue;
            }
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
            parsed.push(ModalField {
                label: outer_label
                    .or_else(|| inner["label"].as_str())
                    .unwrap_or("Answer")
                    .to_owned(),
                custom_id: inner["custom_id"].as_str()?.to_owned(),
                value: inner["value"].as_str().unwrap_or_default().to_owned(),
                placeholder: inner["placeholder"].as_str().unwrap_or_default().to_owned(),
                required: inner["required"].as_bool().unwrap_or(true),
                min_length: inner["min_length"].as_u64().unwrap_or_default() as usize,
                max_length: inner["max_length"].as_u64().unwrap_or(4000) as usize,
                paragraph: inner["style"].as_u64() == Some(2),
                kind,
                options,
                values,
                min_values: inner["min_values"].as_u64().unwrap_or(1) as usize,
                max_values: inner["max_values"].as_u64().unwrap_or(1) as usize,
                channel_types: inner["channel_types"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .collect(),
            });
        }
        Some(Self {
            title: value["title"].as_str()?.to_owned(),
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
            error: None,
            nonce: None,
            shape: value["components"].clone(),
        })
    }

    fn valid(&self) -> bool {
        self.supported
            && self.fields.iter().all(|field| {
                if field.kind == TEXT_DISPLAY {
                    true
                } else if matches!(field.kind, 3 | 5..=8) {
                    (!field.required && field.values.is_empty())
                        || (field.values.len() >= field.min_values
                            && field.values.len() <= field.max_values)
                } else {
                    let length = field.value.chars().count();
                    (!field.required && field.value.is_empty())
                        || (!field.value.is_empty()
                            && length >= field.min_length
                            && length <= field.max_length)
                }
            })
    }

    fn submitted(&self) -> Value {
        let mut rows = self.shape.clone();
        for row in rows.as_array_mut().into_iter().flatten() {
            // Discord's client sends a text display back as its kind alone.
            if row["type"] == TEXT_DISPLAY {
                *row = json!({"type": TEXT_DISPLAY});
                continue;
            }
            let label = row["type"] == 18;
            let inner = if label {
                &mut row["component"]
            } else {
                &mut row["components"][0]
            };
            let id = inner["custom_id"].as_str().unwrap_or_default();
            if let Some(field) = self.fields.iter().find(|field| field.custom_id == id) {
                *inner = if matches!(field.kind, 3 | 5..=8) {
                    json!({"type": field.kind, "custom_id": id, "values": field.values})
                } else {
                    json!({"type": 4, "custom_id": id, "value": field.value})
                };
            }
            let submitted = inner.clone();
            // The submit payload contains the input shape and values, not
            // labels, placeholders or validation metadata.
            if label {
                *row = json!({"type": 18, "component": submitted});
            } else {
                *row = json!({"type": 1, "components": [submitted]});
            }
        }
        rows
    }
}

impl Application {
    pub(super) fn use_button(&mut self, item: String, custom_id: String) -> Vec<Effect> {
        if self.state.interaction_pending(&item) {
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
        if self.state.interaction_pending(&item) {
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
                });
            }
        }
        for choice in &mut choices {
            choice.selected |= defaults.contains(&choice.value);
        }
        let mut seen = std::collections::HashSet::new();
        choices.retain(|choice| seen.insert(choice.value.clone()));
        self.state.select_menu = Some(SelectMenu {
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
        });
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
                }));
            }
            choices.extend(self.state.messages.iter().filter_map(|message| {
                let id = message.sender.strip_prefix("discord:")?;
                (!message.bot).then(|| SelectChoice {
                    label: message.author.clone(),
                    value: id.to_owned(),
                    description: String::new(),
                    selected: false,
                })
            }));
            choices.extend(self.state.found_members.iter().map(|member| SelectChoice {
                label: member.name.clone(),
                value: member.id.clone(),
                description: member.username.clone().unwrap_or_default(),
                selected: false,
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
                            .is_some_and(|kind| types.contains(&u64::from(kind)))
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
                });
            }
        }
    }

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
                return vec![];
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

    pub(super) fn submit_select(&mut self) -> Vec<Effect> {
        let Some(menu) = self.state.select_menu.as_ref() else {
            return vec![];
        };
        if menu.modal_field.is_none() && self.state.interaction_pending(&menu.item) {
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
            .filter(|modal| !modal.saving())
        else {
            return vec![];
        };
        let Some(field) = modal.fields.get(index).cloned() else {
            return vec![];
        };
        if !matches!(field.kind, 3 | 5..=8) {
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
        });
        vec![]
    }

    fn component(
        &mut self,
        item: String,
        kind: u64,
        custom_id: String,
        values: Option<Vec<String>>,
    ) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        let Some((guild, channel)) = discord::parse_room(&room) else {
            return vec![];
        };
        let (Some(message), Some(message_id)) =
            (self.state.message(&item), item.strip_prefix("d:"))
        else {
            return vec![];
        };
        // As in Discord's client: the message's app, or else its author,
        // a bot's user having its app's id.
        let Some(application) = message
            .application_id
            .as_deref()
            .or_else(|| discord::sender_user(&message.sender))
        else {
            return vec![];
        };
        let nonce = interaction_nonce();
        let mut data = json!({"component_type": kind, "custom_id": custom_id});
        if let Some(values) = values {
            // Discord's client names a select's kind beside its values.
            data["type"] = json!(kind);
            data["values"] = json!(values);
        }
        let mut body = json!({
            "type": 3, "nonce": nonce, "channel_id": channel,
            "message_flags": message.flags, "message_id": message_id,
            "application_id": application, "data": data,
        });
        if guild != discord::DM_GUILD {
            body["guild_id"] = json!(guild);
        }
        if self
            .state
            .interaction_failed
            .as_ref()
            .is_some_and(|(failed, _)| *failed == item)
        {
            self.state.interaction_failed = None;
        }
        self.state
            .pending_interactions
            .push(PendingInteraction { nonce, item, room });
        vec![Effect::Backend(Command::Discord(DiscordCommand::Interact(
            body,
        )))]
    }

    /// Takes back the interaction sent with `nonce`, once Discord answers it.
    fn take_pending(&mut self, nonce: &str) -> Option<PendingInteraction> {
        let at = self
            .state
            .pending_interactions
            .iter()
            .position(|pending| pending.nonce == nonce)?;
        Some(self.state.pending_interactions.remove(at))
    }

    pub(super) fn interaction_succeeded(&mut self, nonce: &str) {
        self.take_pending(nonce);
        if self
            .state
            .interaction_modal
            .as_ref()
            .is_some_and(|modal| modal.nonce.as_deref() == Some(nonce))
        {
            self.state.interaction_modal = None;
        }
    }

    /// A form that failed stays open with what was typed and why; a
    /// control's message says why under its components.
    pub(super) fn interaction_failed(&mut self, nonce: &str, reason: String) {
        let Some(pending) = self.take_pending(nonce) else {
            return;
        };
        if let Some(modal) = self
            .state
            .interaction_modal
            .as_mut()
            .filter(|modal| modal.nonce.as_deref() == Some(nonce))
        {
            modal.nonce = None;
            modal.error = Some(reason);
        } else if !pending.item.is_empty() {
            self.state.interaction_failed = Some((pending.item, reason));
        }
    }

    /// Opens a bot's form. Discord sends it only to the session whose
    /// interaction it answers, so it opens even after its control gave up.
    pub(super) fn open_interaction_modal(&mut self, raw: Value) {
        let pending = raw["nonce"]
            .as_str()
            .and_then(|nonce| self.take_pending(nonce));
        let Some(mut modal) = InteractionModal::parse(raw) else {
            return;
        };
        if modal.guild_id.is_none() {
            // The event leaves the server out: the control's room has it,
            // or else the room of the form's channel.
            let room = pending
                .as_ref()
                .map(|pending| pending.room.as_str())
                .or_else(|| {
                    self.state
                        .rooms
                        .iter()
                        .map(|room| room.id.as_str())
                        .find(|room| {
                            discord::parse_room(room)
                                .is_some_and(|(_, channel)| channel == modal.channel_id)
                        })
                });
            modal.guild_id = room
                .and_then(discord::parse_room)
                .map(|(guild, _)| guild)
                .filter(|guild| *guild != discord::DM_GUILD)
                .map(str::to_owned);
        }
        let room = discord::room_id(
            modal.guild_id.as_deref().unwrap_or(discord::DM_GUILD),
            &modal.channel_id,
        );
        for field in &mut modal.fields {
            if matches!(field.kind, 5..=8) && !field.values.is_empty() {
                let choices = self.entity_choices(field.kind, &room, &field.channel_types);
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
        }
        self.state.interaction_modal = Some(modal);
    }

    pub(super) fn set_modal_field(&mut self, index: usize, value: String) -> Vec<Effect> {
        if let Some(modal) = self
            .state
            .interaction_modal
            .as_mut()
            .filter(|modal| !modal.saving())
        {
            // What's typed next answers the error.
            modal.error = None;
            if let Some(field) = modal.fields.get_mut(index) {
                field.value = value;
            }
        }
        vec![]
    }

    pub(super) fn submit_modal(&mut self) -> Vec<Effect> {
        let Some(modal) = self
            .state
            .interaction_modal
            .as_mut()
            .filter(|modal| !modal.saving())
        else {
            return vec![];
        };
        if !modal.valid() {
            modal.error = Some("Complete the required fields before submitting.".into());
            return vec![];
        }
        let nonce = interaction_nonce();
        let mut body = json!({
            "type": 5, "application_id": modal.application_id,
            "channel_id": modal.channel_id, "nonce": nonce,
            "data": {"id": modal.id, "custom_id": modal.custom_id, "components": modal.submitted()},
        });
        if let Some(guild) = &modal.guild_id {
            body["guild_id"] = json!(guild);
        }
        let room = discord::room_id(
            modal.guild_id.as_deref().unwrap_or(discord::DM_GUILD),
            &modal.channel_id,
        );
        modal.nonce = Some(nonce.clone());
        modal.error = None;
        self.state.pending_interactions.push(PendingInteraction {
            nonce,
            item: String::new(),
            room,
        });
        vec![Effect::Backend(Command::Discord(DiscordCommand::Interact(
            body,
        )))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(app.state.select_menu.as_ref().unwrap().choices[0].selected);
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
        assert!(app.state.interaction_modal.as_ref().unwrap().saving());
        assert!(app.dispatch(Action::SubmitModal).is_empty());
        app.dispatch(Action::CloseModal);
        assert!(app.state.interaction_modal.is_some());
        let nonce = app.state.pending_interactions[0].nonce.clone();
        app.apply(Event::InteractionFailed {
            nonce,
            reason: "Offline".into(),
        });
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(!modal.saving());
        assert_eq!(modal.fields[0].value, "Keep this answer");
        assert_eq!(modal.error.as_deref(), Some("Offline"));
        assert!(app.state.pending_interactions.is_empty());
        assert!(app.state.interaction_failed.is_none(), "the form says why");
        app.dispatch(Action::SubmitModal);
        let nonce = app.state.pending_interactions[0].nonce.clone();
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
        let nonce = app.state.pending_interactions[0].nonce.clone();
        app.apply(Event::InteractionSettled(nonce));
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(!modal.saving());
        assert_eq!(
            modal.error.as_deref(),
            Some("The application did not respond")
        );
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
    fn a_forms_text_shows_and_goes_back_as_its_kind() {
        let modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit",
            "components": [
                {"type": 10, "id": 1, "content": "**This can't be undone.**"},
                {"type": 1, "components": [{
                    "type": 4, "custom_id": "answer", "value": "Ready"
                }]}
            ]
        }))
        .unwrap();
        assert!(modal.supported);
        assert!(modal.valid());
        assert_eq!(modal.fields[0].kind, TEXT_DISPLAY);
        assert_eq!(modal.fields[0].label, "**This can't be undone.**");
        assert_eq!(
            modal.submitted(),
            json!([
                {"type": 10},
                {"type": 1, "components": [{"type": 4, "custom_id": "answer", "value": "Ready"}]}
            ])
        );
    }

    #[test]
    fn a_form_with_a_field_yapfast_lacks_isnt_sent() {
        let mut app = Application::default();
        let modal = InteractionModal::parse(json!({
            "id": "42", "channel_id": "7", "custom_id": "edit",
            "application": {"id": "3"}, "title": "Edit",
            "components": [{"type": 18, "label": "Proof", "component": {"type": 19, "custom_id": "file"}}]
        }))
        .unwrap();
        assert!(!modal.supported);
        app.state.interaction_modal = Some(modal);
        assert!(app.dispatch(Action::SubmitModal).is_empty());
        assert!(app.state.pending_interactions.is_empty());
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
        assert_eq!(modal.submitted()[0]["component"]["values"], json!(["4"]));
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

    /// An app's message in #7 of server 10 with a button, by the bot
    /// user 3, naming its app when `application` is given.
    fn bot_message(item: &str, application: Option<&str>) -> Arc<ChatMessage> {
        Arc::new(ChatMessage {
            item: item.into(),
            sender: "discord:3".into(),
            bot: true,
            application_id: application.map(str::to_owned),
            flags: 1 << 15,
            cards: vec![chat::Card {
                rows: vec![chat::CardRow::Buttons(vec![chat::CardButton {
                    label: "Approve".into(),
                    style: chat::ButtonStyle::Success,
                    url: None,
                    custom_id: Some("approve".into()),
                    sku_id: None,
                    disabled: false,
                }])],
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    fn with_messages(messages: Vec<Arc<ChatMessage>>) -> Application {
        let mut app = Application::default();
        app.state.selected = Some("discord:10:7".into());
        app.state.messages = Arc::new(messages);
        app
    }

    fn click(app: &mut Application, item: &str) -> Option<Value> {
        let effects = app.dispatch(Action::UseButton {
            item: item.into(),
            custom_id: "approve".into(),
        });
        match effects.as_slice() {
            [Effect::Backend(Command::Discord(DiscordCommand::Interact(body)))] => {
                Some(body.clone())
            }
            _ => None,
        }
    }

    #[test]
    fn a_control_names_its_app_or_else_its_bot_author() {
        // Discord's client sends the message's application_id, or else its
        // author's id, which a bot's user shares with its app.
        let mut app = with_messages(vec![
            bot_message("d:1", None),
            bot_message("d:2", Some("8")),
        ]);
        let body = click(&mut app, "d:1").unwrap();
        assert_eq!(body["type"], 3);
        assert_eq!(body["application_id"], "3");
        assert_eq!(body["guild_id"], "10");
        assert_eq!(body["channel_id"], "7");
        assert_eq!(body["message_id"], "1");
        assert_eq!(body["message_flags"], 1 << 15);
        assert_eq!(
            body["data"],
            json!({"component_type": 2, "custom_id": "approve"})
        );
        assert_eq!(click(&mut app, "d:2").unwrap()["application_id"], "8");
    }

    #[test]
    fn a_waiting_control_holds_back_only_its_own_message() {
        let mut app = with_messages(vec![bot_message("d:1", None), bot_message("d:2", None)]);
        let first = click(&mut app, "d:1").unwrap();
        assert!(click(&mut app, "d:1").is_none(), "one at a time a message");
        assert!(click(&mut app, "d:2").is_some());
        assert!(app.state.interaction_pending("d:1"));
        app.apply(Event::InteractionSucceeded(
            first["nonce"].as_str().unwrap().into(),
        ));
        assert!(!app.state.interaction_pending("d:1"));
        assert!(app.state.interaction_pending("d:2"));
        assert!(click(&mut app, "d:1").is_some());
    }

    #[test]
    fn a_failed_control_says_why_under_its_message() {
        let mut app = with_messages(vec![bot_message("d:1", None)]);
        let nonce = click(&mut app, "d:1").unwrap()["nonce"]
            .as_str()
            .unwrap()
            .to_owned();
        app.apply(Event::InteractionFailed {
            nonce: nonce.clone(),
            reason: "This interaction failed".into(),
        });
        assert_eq!(
            app.state.interaction_failed,
            Some(("d:1".into(), "This interaction failed".into()))
        );
        assert!(app.state.notice.is_none());
        // What comes after for the same nonce changes nothing.
        app.apply(Event::InteractionSettled(nonce));
        assert_eq!(
            app.state
                .interaction_failed
                .as_ref()
                .map(|(_, why)| why.as_str()),
            Some("This interaction failed")
        );
        // Trying again takes the line away.
        let nonce = click(&mut app, "d:1").unwrap()["nonce"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(app.state.interaction_failed.is_none());
        // A bot that never answers: Discord's client says so.
        app.apply(Event::InteractionSettled(nonce));
        assert_eq!(
            app.state.interaction_failed,
            Some(("d:1".into(), "The application did not respond".into()))
        );
    }

    #[test]
    fn a_form_takes_the_server_of_the_control_that_asked_for_it() {
        let mut app = with_messages(vec![bot_message("d:1", None)]);
        let nonce = click(&mut app, "d:1").unwrap()["nonce"].clone();
        // Discord's INTERACTION_MODAL_CREATE names no server.
        app.apply(Event::InteractionModal(json!({
            "id": "42", "channel_id": "7", "custom_id": "rollback", "nonce": nonce,
            "application": {"id": "3"}, "title": "Roll back",
            "components": [{"type": 18, "label": "Why", "component": {"type": 4, "custom_id": "why", "style": 2}}]
        })));
        assert!(!app.state.interaction_pending("d:1"));
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert_eq!(modal.guild_id.as_deref(), Some("10"));
        assert_eq!(modal.title, "Roll back");
        app.dispatch(Action::SetModalField {
            index: 0,
            value: "Broke the login".into(),
        });
        let sent = app.dispatch(Action::SubmitModal);
        let [Effect::Backend(Command::Discord(DiscordCommand::Interact(body)))] = sent.as_slice()
        else {
            panic!("the form is sent");
        };
        assert_eq!(body["type"], 5);
        assert_eq!(body["guild_id"], "10");
        assert_eq!(body["application_id"], "3");
        assert_eq!(
            body["data"],
            json!({"id": "42", "custom_id": "rollback", "components": [
                {"type": 18, "component": {"type": 4, "custom_id": "why", "value": "Broke the login"}}
            ]})
        );
        assert!(app.state.interaction_modal.as_ref().unwrap().saving());
        app.apply(Event::InteractionSucceeded(
            body["nonce"].as_str().unwrap().into(),
        ));
        assert!(app.state.interaction_modal.is_none());
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
            modal.submitted(),
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
            modal.submitted(),
            json!([
                {"type": 18, "component": {"type": 6, "custom_id": "role", "values": ["11"]}}
            ])
        );
        modal.fields[0].values.clear();
        assert!(!modal.valid());
    }
}
