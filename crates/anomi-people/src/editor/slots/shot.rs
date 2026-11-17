//! The editor's shot: Alice as `profile/editor.png` shows her, with her
//! nameplate, which the fixtures can't reach (it comes from the widgets).

use anomi_vm::Vm;
use anomi_vm::fixtures::people::editor as sample;
use anomi_vm::kind::Nameplate;
use anomi_widgets::nameplate::sample as plates;

/// `profile/editor.png`.
pub fn editor() -> Vm {
    let mut editor = sample::main();
    editor.nameplate = Nameplate {
        picture: Some(plates::strip("alice")),
        palette: plates::palette("bubble_gum"),
    };
    sample::open(editor)
}
