//! Samples of the Game activity settings page for tests and shots, after the
//! fake Discord's data: task S08's.

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::frame::Page;
use crate::settings::games::{Executable, Games};

/// The page as the golden screenshot has it: sharing on, a Discord account
/// linked, no custom game.
pub fn games() -> Games {
    Games {
        shown: true,
        linked: true,
        ..Games::default()
    }
}

/// The window with the settings open on Game activity.
pub fn page() -> Vm {
    with(games())
}

/// The window with Game activity as `page` has it.
pub fn with(page: Games) -> Vm {
    let mut vm = settings(Page::Games);
    vm.settings.games = page;
    vm
}

/// Two custom games, the second being edited.
pub fn custom() -> Games {
    Games {
        games: vec![
            Executable {
                executable: "/opt/games/blockhead/blockhead.x86_64".into(),
                text: "Blockhead".into(),
            },
            Executable {
                executable: "dwarves.exe".into(),
                text: "Deep Dwarves".into(),
            },
        ],
        editing: Some(1),
        executable_draft: "dwarves.exe".into(),
        text_draft: "Deep Dwarves".into(),
        ..games()
    }
}

/// With no account linked the switch is locked.
pub fn unlinked() -> Games {
    Games {
        linked: false,
        ..games()
    }
}

/// A refused save.
pub fn refused() -> Games {
    Games {
        executable_draft: "game".into(),
        text_draft: "x".into(),
        invalid: true,
        ..games()
    }
}
