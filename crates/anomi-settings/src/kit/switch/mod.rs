//! A switch with its label beside it and what it does below. A click on
//! either, or Space or Enter with the keys, asks to flip it; the page owns
//! the value (`controlled`), so the switch only says it was flipped and
//! follows what it is given. The Slint side: `SettingsSwitch`
//! (`ui/settings.slint:27`) and its face `SettingsToggle`
//! (`ui/yapfast/settings-toggle.slint:8`): a 48 by 24 track whose 18 px knob
//! glides over `QUICK` while the track fades, ringed in the text colour
//! under the keys, faded to half while it can't change.
//!
//! A locked switch (Discord's setting the account may not change) ends its
//! label with a lock whose tooltip says why. A NEW tag follows the label of
//! a setting the person hasn't seen.

mod body;

use iced::widget::{row, svg, text};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_widgets::icon::icon;
use anomi_widgets::new_tag::new_tag;
use anomi_widgets::tip::{Side, tip};

use body::Body;

/// Between the label and its parts (`spacing: 6px`).
const PARTS: f32 = 6.0;
/// The lock's side (`width: 14px`).
const LOCK: f32 = 14.0;

/// A switch saying `label`; see [`switch`].
pub struct Switch<'a, M> {
    label: &'a str,
    description: &'a str,
    on: bool,
    enabled: bool,
    fresh: bool,
    locked: Option<&'a str>,
    on_toggle: Option<Box<dyn Fn(bool) -> M + 'a>>,
    motion: Motion,
}

/// A switch saying `label`, set to `on`. Chain [`Switch::description`],
/// [`Switch::locked`], [`Switch::fresh`], [`Switch::enabled`] and
/// [`Switch::on_toggle`], and end with [`Switch::view`].
pub fn switch<'a, M>(label: &'a str, on: bool, motion: Motion) -> Switch<'a, M> {
    Switch {
        label,
        description: "",
        on,
        enabled: true,
        fresh: false,
        locked: None,
        on_toggle: None,
        motion,
    }
}

impl<'a, M: 'a> Switch<'a, M> {
    /// What it does, in small grey words under it.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = description;
        self
    }

    /// Whether it can change: a switch that can't is faded to half.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Why it can't change, shown by a lock after its name while it can't.
    pub fn locked(mut self, why: &'a str) -> Self {
        self.locked = Some(why).filter(|why| !why.is_empty());
        self
    }

    /// A NEW tag follows its name until the page has been seen.
    pub fn fresh(mut self, fresh: bool) -> Self {
        self.fresh = fresh;
        self
    }

    /// What a flip asks for: the value it should have.
    pub fn on_toggle(mut self, on_toggle: impl Fn(bool) -> M + 'a) -> Self {
        self.on_toggle = Some(Box::new(on_toggle));
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let mut children = vec![label_row(
            self.label,
            self.enabled,
            self.fresh,
            self.locked,
            self.motion,
        )];
        if !self.description.is_empty() {
            children.push(
                typography::label(self.description)
                    .width(Length::Fill)
                    .boxed(),
            );
        }
        Body {
            children,
            on: self.on,
            enabled: self.enabled,
            on_toggle: self.on_toggle,
            motion: self.motion,
        }
        .boxed()
    }
}

/// The label with its tags.
fn label_row<'a, M: 'a>(
    label: &'a str,
    enabled: bool,
    fresh: bool,
    locked: Option<&'a str>,
    motion: Motion,
) -> Element<'a, M> {
    let name = text(label).size(typography::size::BODY).style(if enabled {
        catalog::text::primary
    } else {
        catalog::text::muted
    });
    let mut parts: Vec<Element<'a, M>> = vec![name.boxed()];
    if fresh {
        parts.push(new_tag().boxed());
    }
    if let (false, Some(why)) = (enabled, locked) {
        let lock = icon(Icon::Lock, LOCK).style(|theme: &Theme, _status| svg::Style {
            color: Some(theme.palette().muted),
        });
        parts.push(tip(lock, why, Side::Above, motion).boxed());
    }
    row(parts)
        .spacing(PARTS)
        .align_y(alignment::Vertical::Center)
        .boxed()
}
