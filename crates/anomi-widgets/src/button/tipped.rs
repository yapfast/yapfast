//! A face with its tooltip: the face under, and over it a probe the size of
//! the face that is the window's one tooltip ([`crate::tip`]) naming it. The
//! probe is a sibling of the face, not its parent, so a face whose label
//! comes and goes (the caret's, while its menu is open; a disabled one) swaps
//! only the probe: the tooltip's own state starts afresh when the label
//! returns, and the card waits for the pointer to move before it shows, as
//! Slint shows a tip on a change of `has-hover`, not on a change of text.

use iced::widget::{space, stack};
use iced::{Length, Widget as _};

use anomi_theme::{Element, View};

use super::face::Face;
use crate::tip::{Side, tip};

impl<'a, Message: Clone + 'a> Face<'a, Message> {
    /// The face with `label` beside it while the pointer is on it. A disabled
    /// face, and one given an empty label, show none.
    pub fn tipped(self, label: &'a str, side: Side) -> impl View<Message> + 'a {
        let probe = space().width(Length::Fill).height(Length::Fill);
        let probe: Element<'a, Message> = if self.enabled && !label.is_empty() {
            tip(probe, label, side, self.motion).boxed()
        } else {
            probe.boxed()
        };
        stack![self.boxed(), probe]
    }
}
