//! The form a server asks new members to fill in before they can talk (its
//! member screening): rules to agree to, questions to answer with a line or
//! a paragraph, and questions with choices. Discord sends it as
//! `{version, description, form_fields}` and takes it back with each field's
//! `response` filled in. See "Joining a server" in docs/discord/servers.md.

use serde_json::{Value, json};

/// What a field asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Rules to agree to; the response is whether they were.
    Terms,
    /// A line of text.
    Text,
    /// A longer text.
    Paragraph,
    /// One of the field's values; the response is its index.
    Choice,
}

impl Kind {
    fn of(name: &str) -> Option<Self> {
        match name {
            "TERMS" => Some(Self::Terms),
            "TEXT_INPUT" => Some(Self::Text),
            "PARAGRAPH" => Some(Self::Paragraph),
            "MULTIPLE_CHOICE" => Some(Self::Choice),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Terms => "TERMS",
            Self::Text => "TEXT_INPUT",
            Self::Paragraph => "PARAGRAPH",
            Self::Choice => "MULTIPLE_CHOICE",
        }
    }
}

/// One question of the form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub kind: Kind,
    pub label: String,
    pub description: Option<String>,
    pub required: bool,
    /// The rules of Terms, or the choices of Choice.
    pub values: Vec<String>,
}

impl Field {
    /// The field of one of the form's `form_fields`; one of a kind Yapfast
    /// doesn't know is left out.
    fn of(field: &Value) -> Option<Self> {
        Some(Self {
            kind: Kind::of(field["field_type"].as_str()?)?,
            label: field["label"].as_str().unwrap_or_default().to_owned(),
            description: field["description"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_owned),
            required: field["required"] == true,
            values: field["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect(),
        })
    }
}

/// What was answered to a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    Agreed(bool),
    Text(String),
    Choice(Option<usize>),
}

/// A server's form, as Discord sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// Which edition of the form this is; Discord wants it back, so it can
    /// tell a form the server has changed since.
    pub version: String,
    /// What the server says of its form, above the fields.
    pub description: String,
    pub fields: Vec<Field>,
}

impl Form {
    /// The form of `GET /guilds/{id}/member-verification`; none when it
    /// has no version or nothing Yapfast can show.
    pub fn of(answer: &Value) -> Option<Self> {
        let fields: Vec<Field> = answer["form_fields"]
            .as_array()?
            .iter()
            .filter_map(Field::of)
            .collect();
        (!fields.is_empty()).then(|| Self {
            version: answer["version"].as_str().unwrap_or_default().to_owned(),
            description: answer["description"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            fields,
        })
    }
}

/// The body of the request that hands the form in: every field with its
/// response.
pub fn body(version: &str, answered: &[(Field, Response)]) -> Value {
    let fields: Vec<Value> = answered
        .iter()
        .map(|(field, response)| {
            let response = match response {
                Response::Agreed(agreed) => json!(agreed),
                Response::Text(text) => json!(text.trim()),
                Response::Choice(choice) => json!(choice),
            };
            json!({
                "field_type": field.kind.name(),
                "label": field.label,
                "description": field.description,
                "required": field.required,
                "values": field.values,
                "response": response,
            })
        })
        .collect();
    json!({"version": version, "form_fields": fields})
}

/// What came of handing the form in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    /// The server let the account in.
    Approved,
    /// Its moderators decide.
    Waiting,
    Rejected,
}

impl Review {
    /// From the `application_status` of the answer to the request.
    pub fn of(answer: &Value) -> Self {
        match answer["application_status"].as_str() {
            Some("APPROVED") => Self::Approved,
            Some("REJECTED") => Self::Rejected,
            _ => Self::Waiting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Value {
        json!({
            "version": "2024-05-01T10:00:00.000000+00:00",
            "description": "Welcome to the server.",
            "form_fields": [
                {"field_type": "TERMS", "label": "Read and agree to the server rules",
                 "values": ["Be kind.", "No spam."], "required": true, "response": null},
                {"field_type": "TEXT_INPUT", "label": "Why do you want to join?",
                 "values": null, "required": false, "response": null},
                {"field_type": "MULTIPLE_CHOICE", "label": "Pick one",
                 "values": ["Red", "Blue"], "required": true, "response": null},
                {"field_type": "SOMETHING_NEW", "label": "Unknown", "required": true},
            ],
        })
    }

    #[test]
    fn reads_the_form_and_leaves_out_fields_of_a_kind_it_does_not_know() {
        let form = Form::of(&form()).unwrap();
        assert_eq!(form.description, "Welcome to the server.");
        assert_eq!(
            form.fields
                .iter()
                .map(|field| field.kind)
                .collect::<Vec<_>>(),
            [Kind::Terms, Kind::Text, Kind::Choice]
        );
        assert_eq!(form.fields[0].values, ["Be kind.", "No spam."]);
        assert!(form.fields[0].required && !form.fields[1].required);
        assert!(Form::of(&json!({"version": "1", "form_fields": []})).is_none());
        assert!(Form::of(&json!({})).is_none());
    }

    #[test]
    fn hands_the_form_back_with_every_response() {
        let form = Form::of(&form()).unwrap();
        let answered: Vec<(Field, Response)> = form
            .fields
            .iter()
            .cloned()
            .zip([
                Response::Agreed(true),
                Response::Text("  to chat  ".into()),
                Response::Choice(Some(1)),
            ])
            .collect();
        let body = body(&form.version, &answered);
        assert_eq!(body["version"], form.version);
        let fields = body["form_fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0]["field_type"], "TERMS");
        assert_eq!(fields[0]["response"], true);
        assert_eq!(fields[1]["response"], "to chat");
        assert_eq!(fields[2]["response"], 1);
        assert_eq!(fields[2]["values"], json!(["Red", "Blue"]));
    }

    #[test]
    fn reads_what_became_of_the_request() {
        let status = |status: &str| Review::of(&json!({"application_status": status}));
        assert_eq!(status("APPROVED"), Review::Approved);
        assert_eq!(status("REJECTED"), Review::Rejected);
        assert_eq!(status("SUBMITTED"), Review::Waiting);
        assert_eq!(Review::of(&json!({})), Review::Waiting);
    }
}
