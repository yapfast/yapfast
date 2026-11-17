//! Samples of a server's audit log for tests and shots, after the fake
//! Discord's data as the golden shot shows it: task X07's.

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, settings::server, shell};
use crate::settings::audit::{Audit, Kind, Line, Row};
use crate::settings::server::Section;
use crate::shell::Layer;
use crate::text::{Builder, Style, Text};

/// A sentence: the parts at odd places are bold.
fn sentence(parts: &[&str]) -> Text {
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let mut builder = Builder::new();
    for (index, part) in parts.iter().enumerate() {
        builder.text(
            part,
            if index % 2 == 1 {
                bold
            } else {
                Style::default()
            },
        );
    }
    builder.finish()
}

/// A numbered line, "01" and on.
fn line(number: usize, parts: &[&str]) -> Line {
    Line {
        number: format!("{number:02}"),
        text: sentence(parts),
    }
}

/// An item under a line.
fn item(words: &str) -> Line {
    Line {
        number: String::new(),
        text: sentence(&[words]),
    }
}

/// The six entries of the golden shot, all closed: changed (with lines),
/// one with nothing to open, removed and made.
pub fn audit() -> Audit {
    let row = |who: &str, title: &[&str], time: &str, kind, lines| Row {
        actor: pictured(
            avatar(&who[..1], if who == "Alice" { 0x5b8def } else { 0xe0807a }),
            who,
        ),
        title: sentence(title),
        time: time.into(),
        kind,
        lines,
    };
    Audit {
        rows: vec![
            row(
                "Alice",
                &["", "alice", " updated the role ", "Leads", ""],
                "Today at 00:49",
                Kind::Update,
                vec![
                    line(1, &["Changed the colour from #2ecc71 to #1abc9c"]),
                    line(2, &["Allowed ", "2", " permissions"]),
                    item("Manage Messages"),
                    item("Kick Members"),
                ],
            ),
            row(
                "Bob",
                &["", "bob", " updated ", "carol", ""],
                "Yesterday at 23:39\u{2014}Yesterday at 23:49",
                Kind::Update,
                vec![line(1, &["Changed the nickname to ", "caz", ""])],
            ),
            row(
                "Bob",
                &["", "bob", " banned ", "trent_42", ""],
                "Yesterday at 00:09",
                Kind::Delete,
                vec![],
            ),
            row(
                "Bob",
                &["", "bob", " banned ", "mallory", ""],
                "Last Sunday at 23:09",
                Kind::Delete,
                vec![line(1, &["With the reason ", "spam", ""])],
            ),
            row(
                "Bob",
                &[
                    "",
                    "bob",
                    " created permission overrides for ",
                    "#general",
                    "",
                ],
                "Last Sunday at 01:09",
                Kind::Create,
                vec![line(1, &["Denied ", "Send Messages", " to @everyone"])],
            ),
            row(
                "Bob",
                &["", "bob", " updated ", "Server", ""],
                "Last Saturday at 01:09",
                Kind::Update,
                vec![line(1, &["Changed the name from Old to ", "Server", ""])],
            ),
        ],
        users: ["All Users", "alice", "bob"].map(Into::into).into(),
        actions: [
            "All Actions",
            "Update Role",
            "Update Member",
            "Ban Member",
            "Create Permission Overrides",
            "Update Server",
        ]
        .map(Into::into)
        .into(),
        ..Audit::default()
    }
}

/// The log with its first row open and older entries to load.
pub fn opened() -> Audit {
    Audit {
        open: Some(0),
        older: true,
        ..audit()
    }
}

/// The window with the server's settings open on the Audit Log, which takes
/// the frame's height and scrolls on its own.
pub fn page() -> Vm {
    with(audit())
}

/// The window with its first row open and the button for older entries.
pub fn open() -> Vm {
    with(opened())
}

fn with(audit: Audit) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = crate::settings::server::Server {
        section: Section::AuditLog,
        fill: true,
        ..server::server()
    };
    vm.settings.audit = audit;
    vm
}

/// The first page still on its way.
pub fn loading() -> Vm {
    with(Audit {
        rows: Vec::new(),
        fetching: true,
        ..audit()
    })
}

/// No entries yet.
pub fn empty() -> Vm {
    with(Audit {
        rows: Vec::new(),
        ..audit()
    })
}

/// Discord didn't send the newest.
pub fn failed() -> Vm {
    with(Audit {
        rows: Vec::new(),
        failed: true,
        ..audit()
    })
}
