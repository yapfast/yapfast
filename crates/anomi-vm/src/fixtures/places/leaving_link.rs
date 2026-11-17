//! Samples of the confirmation before leaving for a link for tests and shots,
//! after the fake Discord's data: task N13's.

use crate::Vm;
use crate::fixtures::shell;
use crate::places::leaving_link::LeavingLink;
use crate::shell::Layer;

/// A link whose text hid where it goes.
pub fn masked() -> LeavingLink {
    LeavingLink {
        link: "https://example.com/download/setup?utm_source=chat&ref=friend-invite-2026".into(),
        site: "example.com".into(),
    }
}

/// A DM's window with the confirmation over it.
pub fn asked() -> Vm {
    let mut vm = shell::open(shell::dm_home(), Layer::LeavingLink);
    vm.places.leaving_link = masked();
    vm
}
