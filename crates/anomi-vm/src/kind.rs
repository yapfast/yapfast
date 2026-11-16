//! Types several areas share: a person's presence, an avatar, a count badge.

use iced::Color;
use iced::widget::image;

/// A person's presence, as Discord shows it by their avatar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    /// Unknown: no dot.
    #[default]
    None,
    Online,
    Mobile,
    Idle,
    DoNotDisturb,
    Offline,
}

/// A round picture of a person, a group or a server: its picture once it
/// arrived, and until then its initial on its tint.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub initial: String,
    pub tint: Color,
    /// The decoded picture. Its pixels are the core's, shared, not copied.
    pub picture: Option<image::Handle>,
}

impl Default for Avatar {
    fn default() -> Self {
        Self {
            initial: "?".into(),
            tint: Color::from_rgb8(0x64, 0x6b, 0x7a),
            picture: None,
        }
    }
}

/// A count as a badge shows it: nothing for none, and at most "99+".
pub fn count_badge(count: u64) -> String {
    match count {
        0 => String::new(),
        count if count > 99 => "99+".into(),
        count => count.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::count_badge;

    #[test]
    fn a_badge_stops_at_99() {
        assert_eq!(count_badge(0), "");
        assert_eq!(count_badge(7), "7");
        assert_eq!(count_badge(100), "99+");
    }
}
