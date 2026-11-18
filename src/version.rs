//! Release version numbers shared between the desktop updater and NEW tags.

use std::cmp::Ordering;

/// A version tag such as `0.4.0` or `0.5.0-nightly.20261107.12`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Identifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Number(a), Identifier::Number(b)) => a.cmp(b),
            // A number sorts before text, as SemVer says.
            (Identifier::Number(_), Identifier::Text(_)) => Ordering::Less,
            (Identifier::Text(_), Identifier::Number(_)) => Ordering::Greater,
            (Identifier::Text(a), Identifier::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    /// Reads `0.4.0`, `v0.4.0` and `0.5.0-nightly.20261107.12`; anything else
    /// is not a version of Yapfast's.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = core.split('.');
        let mut next = || numbers.next()?.parse::<u64>().ok();
        let core = [next()?, next()?, next()?];
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|part| match part {
                    "" => None,
                    part => Some(
                        part.parse()
                            .map_or_else(|_| Identifier::Text(part.to_owned()), Identifier::Number),
                    ),
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Self { core, pre })
    }

    pub fn is_nightly(&self) -> bool {
        matches!(self.pre.first(), Some(Identifier::Text(text)) if text == "nightly")
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            // A release sorts after its own prereleases.
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
