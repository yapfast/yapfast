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
    pub saving: bool,
    pub error: Option<String>,
    shape: Value,
}

impl InteractionModal {
    fn parse(value: Value) -> Option<Self> {
        let fields = value["components"].as_array()?;
        let mut parsed = Vec::new();
        let mut supported = true;
        for row in fields {
            let (outer_label, inner) = match row["type"].as_u64() {
                Some(1) => (None, row["components"].as_array()?.first()?),
                Some(18) => (row["label"].as_str(), &row["component"]),
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
            saving: false,
            error: None,
            shape: value["components"].clone(),
        })
    }

    fn valid(&self) -> bool {
        self.supported
            && self.fields.iter().all(|field| {
                if matches!(field.kind, 3 | 5..=8) {
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
        rows.as_array_mut().unwrap().retain(|row| row["type"] != 10);
        for row in rows.as_array_mut().into_iter().flatten() {
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
        if self.state.pending_interaction.is_some() {
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
        if self.state.pending_interaction.is_some() {
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
        if self.state.pending_interaction.is_some() {
            return vec![];
        }
        let Some(menu) = self.state.select_menu.as_ref() else {
            return vec![];
        };
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
        let application_id = message.application_id.as_deref();
        if application_id.is_none() && !message.bot {
            self.state.notice = Some("Discord didn't identify the bot for this control.".into());
            return vec![];
        }
        let guild = guild.to_owned();
        let nonce = interaction_nonce();
        let mut data = json!({"component_type": kind, "custom_id": custom_id});
        if let Some(values) = values {
            data["values"] = json!(values);
        }
        let mut body = json!({
            "type": 3, "application_id": application_id, "channel_id": channel,
            "message_id": message_id, "message_flags": message.flags,
            "data": data, "nonce": nonce,
        });
        if guild != discord::DM_GUILD {
            body["guild_id"] = json!(guild);
        }
        self.state.pending_interaction = Some(nonce);
        self.state.recent_interaction = None;
        self.state.pending_interaction_guild = (guild != discord::DM_GUILD).then_some(guild);
        vec![Effect::Backend(Command::Discord(DiscordCommand::Interact(
            body,
        )))]
    }

    pub(super) fn open_interaction_modal(&mut self, raw: Value) {
        if raw["nonce"].as_str() != self.state.pending_interaction.as_deref()
            && raw["nonce"].as_str() != self.state.recent_interaction.as_deref()
        {
            return;
        }
        self.state.pending_interaction = None;
        self.state.recent_interaction = None;
        self.state.interaction_modal = InteractionModal::parse(raw).map(|mut modal| {
            if modal.guild_id.is_none() {
                modal.guild_id = self.state.pending_interaction_guild.take();
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
            modal
        });
    }

    pub(super) fn set_modal_field(&mut self, index: usize, value: String) -> Vec<Effect> {
        if let Some(field) = self
            .state
            .interaction_modal
            .as_mut()
            .and_then(|modal| modal.fields.get_mut(index))
            && self.state.pending_interaction.is_none()
        {
            field.value = value;
        }
        vec![]
    }

    pub(super) fn submit_modal(&mut self) -> Vec<Effect> {
        if self.state.pending_interaction.is_some() {
            return vec![];
        }
        let Some(modal) = self.state.interaction_modal.as_ref() else {
            return vec![];
        };
        if !modal.valid() {
            self.state.notice = Some("Complete the required fields before submitting.".into());
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
        let guild = modal.guild_id.clone();
        if let Some(modal) = &mut self.state.interaction_modal {
            modal.saving = true;
            modal.error = None;
        }
        self.state.pending_interaction = Some(nonce);
        self.state.recent_interaction = None;
        self.state.pending_interaction_guild = guild;
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
        assert!(app.state.interaction_modal.as_ref().unwrap().saving);
        assert!(app.dispatch(Action::SubmitModal).is_empty());
        app.dispatch(Action::CloseModal);
        assert!(app.state.interaction_modal.is_some());
        let nonce = app.state.pending_interaction.clone().unwrap();
        app.apply(Event::InteractionFailed {
            nonce,
            reason: "Offline".into(),
        });
        let modal = app.state.interaction_modal.as_ref().unwrap();
        assert!(!modal.saving);
        assert_eq!(modal.fields[0].value, "Keep this answer");
        assert_eq!(modal.error.as_deref(), Some("Offline"));
        assert!(app.state.pending_interaction.is_none());
        app.dispatch(Action::SubmitModal);
        let nonce = app.state.pending_interaction.clone().unwrap();
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
        let nonce = app.state.pending_interaction.clone().unwrap();
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
    fn informational_text_blocks_incomplete_modal_submission() {
        let mut app = Application::default();
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
        assert!(!modal.supported);
        assert!(!modal.valid());
        app.state.interaction_modal = Some(modal);
        assert!(app.dispatch(Action::SubmitModal).is_empty());
        assert!(app.state.pending_interaction.is_none());
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
            Some("Discord didn't identify the bot for this control.")
        );
        assert!(app.state.pending_interaction.is_none());
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
        let [Effect::Backend(Command::Discord(DiscordCommand::Interact(body)))] =
            effects.as_slice()
        else {
            panic!("the backend should resolve the application");
        };
        assert!(body["application_id"].is_null());
        assert_eq!(body["message_id"], "1");
        assert!(app.state.pending_interaction.is_some());
        assert!(app.state.notice.is_none());
    }

    #[test]
    fn expired_interactions_release_controls_without_clearing_a_newer_request() {
        let mut app = Application::default();
        app.state.pending_interaction = Some("first".into());
        app.apply(Event::InteractionSettled("first".into()));
        assert!(app.state.pending_interaction.is_none());
        app.state.pending_interaction = Some("second".into());
        app.apply(Event::InteractionSettled("first".into()));
        assert_eq!(app.state.pending_interaction.as_deref(), Some("second"));
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
