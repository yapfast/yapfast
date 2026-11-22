//! Plugin identifiers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A plugin's identifier: reverse-DNS, lowercase, stable forever.
///
/// `com.example.word-filter` is the shape. It is the key of the lock file, the
/// name of the plugin's storage directory, and the name shown beside anything
/// the plugin puts on screen, so it is never derived from the display name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PluginId(Box<str>);

/// Why a string isn't a plugin id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdError {
    /// No dot: a reverse-DNS id has at least two labels.
    NotReverseDns,
    /// A label is empty, too long, or has a character outside `a-z0-9-`.
    BadLabel { label: String },
    /// A label starts or ends with a hyphen.
    BadHyphen { label: String },
    /// Longer than [`PluginId::MAX_LEN`].
    TooLong,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdError::NotReverseDns => f.write_str(
                "a plugin id needs at least two dot-separated labels, as in com.example.plugin",
            ),
            IdError::BadLabel { label } => {
                write!(
                    f,
                    "`{label}` is not a label of a plugin id: use a-z, 0-9 and hyphen"
                )
            }
            IdError::BadHyphen { label } => {
                write!(f, "`{label}` starts or ends with a hyphen")
            }
            IdError::TooLong => write!(f, "a plugin id is at most {} bytes", PluginId::MAX_LEN),
        }
    }
}

impl std::error::Error for IdError {}

impl PluginId {
    /// The longest id accepted, in bytes.
    pub const MAX_LEN: usize = 128;
    /// The longest label accepted, in bytes.
    pub const MAX_LABEL: usize = 63;

    /// Checks and takes a reverse-DNS id.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.len() > Self::MAX_LEN {
            return Err(IdError::TooLong);
        }
        let mut labels = 0;
        for label in text.split('.') {
            labels += 1;
            if label.is_empty() || label.len() > Self::MAX_LABEL {
                return Err(IdError::BadLabel {
                    label: label.to_owned(),
                });
            }
            if label.starts_with('-') || label.ends_with('-') {
                return Err(IdError::BadHyphen {
                    label: label.to_owned(),
                });
            }
            if !label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(IdError::BadLabel {
                    label: label.to_owned(),
                });
            }
        }
        if labels < 2 {
            return Err(IdError::NotReverseDns);
        }
        Ok(PluginId(text.into()))
    }

    /// The id as it is written in a manifest, a lock file and a log line.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PluginId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<String> for PluginId {
    type Error = IdError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<PluginId> for String {
    fn from(id: PluginId) -> String {
        id.0.into()
    }
}

impl AsRef<str> for PluginId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reverse_dns() {
        for text in ["com.example.plugin", "org.forgejo.filter", "io.x-y.z1"] {
            assert_eq!(PluginId::parse(text).unwrap().as_str(), text);
        }
    }

    #[test]
    fn rejects_everything_else() {
        for text in [
            "",
            "plugin",
            "com..plugin",
            "com.Example",
            "com.-x",
            "com.x-",
            "com.x/y",
        ] {
            assert!(PluginId::parse(text).is_err(), "{text} should be refused");
        }
    }
}
