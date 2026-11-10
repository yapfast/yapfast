//! Commands: what a person typed, and the line they get back.

use serde_json::Value;

use crate::wire;
use crate::{Error, Result};

/// A command a person typed, with the options the composer collected.
///
/// Options are declared in the plugin's manifest, which is what the composer
/// parses them against; here they are read by name, and a missing one is an
/// error a person can read rather than a panic.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// The command's name, without the leading `/`.
    pub name: String,
    /// The options, by the name the manifest gave them.
    pub options: serde_json::Map<String, Value>,
}

impl Command {
    /// The string an option holds, if it was given as one.
    pub fn string(&self, name: &str) -> Option<&str> {
        self.options.get(name)?.as_str()
    }

    /// The integer an option holds, if it was given as one.
    pub fn integer(&self, name: &str) -> Option<i64> {
        self.options.get(name)?.as_i64()
    }

    /// The number an option holds, if it was given as one.
    pub fn number(&self, name: &str) -> Option<f64> {
        self.options.get(name)?.as_f64()
    }

    /// The boolean an option holds, if it was given as one.
    pub fn boolean(&self, name: &str) -> Option<bool> {
        self.options.get(name)?.as_bool()
    }

    /// A required option's JSON, or an error naming it.
    pub fn required(&self, name: &str) -> Result<&Value> {
        self.options
            .get(name)
            .ok_or_else(|| Error::msg(format!("`{name}` is required")))
    }

    /// A required string option, or an error naming it.
    ///
    /// Empty counts as missing: a command that inserts text has nothing to do
    /// with an empty line, and saying so is friendlier than doing nothing.
    pub fn required_string(&self, name: &str) -> Result<&str> {
        match self.string(name) {
            Some(text) if !text.trim().is_empty() => Ok(text),
            _ => Err(Error::msg(format!("`{name}` is required"))),
        }
    }
}

impl From<wire::CommandInvocation> for Command {
    fn from(invocation: wire::CommandInvocation) -> Self {
        Command {
            name: invocation.name,
            options: invocation.options,
        }
    }
}

/// What a command answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// Nothing to show.
    None,
    /// One line for the person, in the room they typed the command in.
    Notice(String),
    /// The command's own JSON, for another plugin or the caller to read.
    Value(Value),
    /// It couldn't be done, with a reason they can read.
    Failed(String),
}

impl Reply {
    /// A line for the person.
    pub fn notice(text: impl Into<String>) -> Self {
        Reply::Notice(text.into())
    }

    /// A command's own JSON.
    pub fn value(value: impl Into<Value>) -> Self {
        Reply::Value(value.into())
    }

    /// A failure, with a reason a person can read.
    pub fn failed(message: impl Into<String>) -> Self {
        Reply::Failed(message.into())
    }
}

impl From<Reply> for wire::CommandOutcome {
    fn from(reply: Reply) -> Self {
        match reply {
            Reply::None => wire::CommandOutcome::None,
            Reply::Notice(text) => wire::CommandOutcome::Notice { text },
            Reply::Value(value) => wire::CommandOutcome::Value(value),
            Reply::Failed(message) => wire::CommandOutcome::Failed { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn command() -> Command {
        Command {
            name: "shout".to_owned(),
            options: serde_json::from_value(json!({"text": "hi", "times": 2, "loud": true}))
                .expect("an options map"),
        }
    }

    #[test]
    fn an_option_is_read_as_the_type_it_was_given_as() {
        let command = command();
        assert_eq!(command.string("text"), Some("hi"));
        assert_eq!(command.integer("times"), Some(2));
        assert_eq!(command.boolean("loud"), Some(true));
        assert_eq!(command.string("times"), None);
    }

    #[test]
    fn a_missing_or_blank_required_option_names_itself() {
        let command = command();
        assert_eq!(command.required_string("text").expect("given"), "hi");
        assert!(command.required_string("body").is_err_and(|error| error.to_string().contains("body")));
        let blank = Command {
            name: "shout".to_owned(),
            options: serde_json::from_value(json!({"text": "  "})).expect("an options map"),
        };
        assert!(blank.required_string("text").is_err());
    }

    #[test]
    fn a_reply_is_the_outcome_the_app_reads() {
        assert_eq!(
            serde_json::to_value(wire::CommandOutcome::from(Reply::notice("hi"))).expect("json"),
            json!({"notice": {"text": "hi"}})
        );
        assert_eq!(
            serde_json::to_value(wire::CommandOutcome::from(Reply::None)).expect("json"),
            json!("none")
        );
    }
}
