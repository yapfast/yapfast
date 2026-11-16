//! Type: the faces and sizes text takes. DM Sans is the interface's face,
//! as `ClientStyle.native-font` has it; a font the person loads replaces it.

use iced::Font;
use iced::font::Weight;

/// DM Sans, the interface's face, at its regular weight.
pub const UI: Font = Font::new("DM Sans");

/// The interface's face at a weight.
pub const fn weight(weight: Weight) -> Font {
    Font { weight, ..UI }
}

pub const MEDIUM: Font = weight(Weight::Medium);
pub const SEMIBOLD: Font = weight(Weight::Semibold);
pub const BOLD: Font = weight(Weight::Bold);

/// Text sizes, in logical pixels.
pub mod size {
    /// A message, and a DM's name in the list.
    pub const BODY: f32 = 15.0;
    /// A row's second line, a field's text, a button.
    pub const SMALL: f32 = 14.0;
    /// A caption, a time, a section heading.
    pub const CAPTION: f32 = 12.0;
    /// A heading over a list, as "Direct messages".
    pub const HEADING: f32 = 13.0;
    /// A header's title.
    pub const TITLE: f32 = 16.0;
    /// A dialog's title.
    pub const DIALOG_TITLE: f32 = 20.0;
    /// A server's initials on the rail.
    pub const RAIL_INITIALS: f32 = 16.0;
    /// A mention count's badge.
    pub const BADGE: f32 = 12.0;
}
