//! Samples of the window's own frame, with its title bar, caption buttons and
//! resize edges for tests and shots, after the fake Discord's data: task
//! F18's.

use crate::chrome::Chrome;

/// Anomi's own bar on the DM home, the window in front, back not possible.
pub fn framed() -> Chrome {
    Chrome {
        framed: true,
        focused: true,
        workspace: true,
        place: yapfast_lang::tr!("Direct Messages").into(),
        can_go_forward: true,
        ..Chrome::default()
    }
}

/// The same after going back twice: back and forward both work, the inbox
/// open.
pub fn history() -> Chrome {
    Chrome {
        can_go_back: true,
        inbox_open: true,
        ..framed()
    }
}

/// A maximized window behind another: its middle button restores, the title
/// is dim.
pub fn maximized_behind() -> Chrome {
    Chrome {
        maximized: true,
        focused: false,
        ..framed()
    }
}

/// Before sign-in: no workspace, so no inbox or help.
pub fn signed_out() -> Chrome {
    Chrome {
        workspace: false,
        can_go_back: false,
        can_go_forward: false,
        place: "Anomi".into(),
        ..framed()
    }
}

/// The desktop draws the frame: nothing of ours.
pub fn system() -> Chrome {
    Chrome::default()
}
