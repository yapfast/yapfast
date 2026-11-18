//! Samples of the name style window for tests and shots, after the fake
//! Discord's data: task P16's. Alice's own card as the editor opens it,
//! over the editor.

use crate::Vm;
use crate::fixtures::people::{editor, profile};
use crate::kind::Presence;
use crate::people::name_style::{Intent, NameStyle};

fn alice() -> crate::people::profile::Card {
    let mut card = profile::own();
    card.presence = Presence::Online;
    card
}

/// Opened on the usual face, solid, nothing chosen yet: Apply waits.
pub fn solid() -> NameStyle {
    NameStyle::opened(alice(), 11, 1, "f0f0f2")
}

/// Opened on a Nitro gradient in the Tempo face.
pub fn gradient() -> NameStyle {
    NameStyle::opened(alice(), 12, 2, "ca63fa,417ae4")
}

/// Opened on a prism, with the light preview.
pub fn prism() -> NameStyle {
    let mut window = NameStyle::opened(alice(), 8, 7, "fe9444,37ee80,fee048,ff4ed3,5966f2");
    window.light = true;
    window
}

/// The gradient with its first handle's colour picker open.
pub fn picking() -> NameStyle {
    let mut window = gradient();
    window.update(&Intent::Handle(1));
    window
}

/// Alice's editor with `window` over it.
pub fn over(window: NameStyle) -> Vm {
    let mut vm = editor::open(editor::nitro());
    vm.people.name_style = window;
    vm
}

/// The gradient, over the editor.
pub fn shot() -> Vm {
    over(gradient())
}

/// The picker open on the gradient's first handle, over the editor.
pub fn shot_picking() -> Vm {
    over(picking())
}

/// The prism in the light preview, over the editor.
pub fn shot_prism() -> Vm {
    over(prism())
}
