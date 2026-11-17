//! A display name style as Discord sends it: `anomi_vm::kind::NameStyle`,
//! which a view-model holds, under the names this module draws it by, with
//! the face it's drawn in.

use anomi_theme::fonts::Name;

pub use anomi_vm::kind::{NameEffect as Effect, NameStyle as Style};

/// The face `style` is drawn in; none for the usual font, which is 11 and
/// the retired ids.
pub fn face(style: &Style) -> Option<Name> {
    Name::of(style.font_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_draws_in_its_face_in_a_server_too() {
        let style = Style::new(14, 3, &[0x00ff99]).expect("a colour");
        assert_eq!(face(&style.font_only()), face(&style));
        assert_eq!(face(&style).map(Name::family), Some("Mainframe"));
        // The usual font has no face.
        assert_eq!(Style::new(11, 1, &[1]).and_then(|s| face(&s)), None);
    }
}
