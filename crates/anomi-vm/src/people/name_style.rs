//! The name style window: what it shows and what it asks for. Task P16
//! (docs/iced/tasks/people.md), after `NameStyleWindow` (`ui/server.slint:1158`).
//!
//! The window holds the style being made: a face, an effect and a colour key
//! (hex joined by commas, as Slint's `paint.key`), with the person's card
//! for the preview. [`NameStyle::update`] moves it as its intents ask; Apply
//! and Close leave it for the app to act on.

use crate::kind::NameStyle as Style;
use crate::people::editor_preview::{Change, Colour, format_hex};
use crate::people::profile::Card;

pub mod presets;

/// The style the window opened on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Start {
    pub font: u8,
    pub effect: u8,
    pub key: String,
}

/// The colour handles of a own gradient (two) or prism (five).
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    pub gradient: [String; 2],
    pub prism: [String; 5],
}

impl Default for Custom {
    fn default() -> Self {
        let own = |hex: &str| hex.to_owned();
        Self {
            gradient: ["2dafa7", "fec491"].map(own),
            prism: ["fe9444", "37ee80", "fee048", "ff4ed3", "5966f2"].map(own),
        }
    }
}

/// The name style window: what its screen draws. The window holds it as
/// `vm.people.name_style`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NameStyle {
    /// Whether the window is over the editor.
    pub open: bool,
    /// Whether it still draws: open, or closed and on its way out.
    pub shown: bool,
    pub start: Start,
    pub font: u8,
    pub effect: u8,
    /// The colours chosen, as hex joined by commas.
    pub key: String,
    /// The person's card, its `name_style` always the style chosen.
    pub card: Card,
    /// The message and member row show as in Discord's light mode.
    pub light: bool,
    /// The one-colour field is open.
    pub hex_open: bool,
    /// Which handle of the bar is open, from 1; 0 for none.
    pub handle: u8,
    /// The colour picker over the row, open with the one-colour square or
    /// a handle: its code is kept as typed when it isn't a colour.
    pub pick: Colour,
    pub custom: Custom,
    seed: u32,
}

/// What a click or a key on the window asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Font(u8),
    Effect(u8),
    /// A preset's key.
    Preset(String),
    /// Toggle the one-colour field.
    Hex,
    /// Open or close the bar's handle (from 1).
    Handle(u8),
    /// The colour picker's pad, hue bar or code.
    Pick(Change),
    /// A click away from the colour picker, which closes it.
    Shut,
    Light(bool),
    /// A random face, effect and colour.
    Surprise,
    /// Make the style the account's.
    Apply {
        font: u8,
        effect: u8,
        colours: String,
    },
    Close,
    /// The window has played its way out.
    Gone,
    /// A click inside the sample card, which opens nothing.
    Ignored,
}

impl NameStyle {
    /// The window open on a style as it stands (`init`): `effect` below 1 is
    /// solid; an empty `key` starts on the effect's first preset.
    pub fn opened(card: Card, font: u8, effect: u8, key: &str) -> Self {
        let effect = effect.max(1);
        let mut window = Self {
            open: true,
            shown: true,
            start: Start {
                font,
                effect,
                key: key.to_owned(),
            },
            font,
            effect,
            key: presets::of(effect)[0].to_owned(),
            card,
            ..Self::default()
        };
        if !key.is_empty() {
            window.key = key.to_owned();
            let parts: Vec<&str> = key.split(',').collect();
            let fill = |into: &mut [String]| {
                for (slot, part) in into.iter_mut().zip(&parts) {
                    *slot = (*part).to_owned();
                }
            };
            match effect {
                2 => fill(&mut window.custom.gradient),
                7 => fill(&mut window.custom.prism),
                _ => {}
            }
        }
        window.refresh();
        window
    }

    /// Whether Apply has anything to apply.
    pub fn changed(&self) -> bool {
        self.font != self.start.font
            || self.effect != self.start.effect
            || self.key != self.start.key
    }

    /// The bar of one's own colours: a gradient's or a prism's.
    pub fn bar(&self) -> bool {
        matches!(self.effect, 2 | 7)
    }

    /// The bar's handles' hex.
    pub fn handles(&self) -> &[String] {
        if self.effect == 2 {
            &self.custom.gradient
        } else {
            &self.custom.prism
        }
    }

    /// The style as chosen.
    pub fn style(&self) -> Option<Style> {
        Style::new(self.font, self.effect, &presets::colours(&self.key))
    }

    fn refresh(&mut self) {
        self.card.name_style = self.style();
    }

    fn shut(&mut self) {
        self.hex_open = false;
        self.handle = 0;
    }

    /// Whether the colour picker is open.
    pub fn picking(&self) -> bool {
        self.hex_open || self.handle > 0
    }

    fn handle_hex(&self) -> &str {
        let at = usize::from(self.handle.max(1)) - 1;
        self.handles().get(at).map_or("", String::as_str)
    }

    fn own_key(&mut self) {
        self.key = self.handles().join(",");
    }

    /// Moves the window as `intent` asks.
    pub fn update(&mut self, intent: &Intent) {
        match intent {
            Intent::Font(id) => {
                self.font = *id;
                self.shut();
            }
            Intent::Effect(id) => {
                self.effect = *id;
                self.key = presets::of(*id)[0].to_owned();
                self.shut();
            }
            Intent::Preset(key) => {
                self.key.clone_from(key);
                self.shut();
            }
            Intent::Hex => {
                let open = !self.hex_open;
                self.shut();
                self.hex_open = open;
                let first = presets::colours(&self.key).first().copied();
                self.pick = Colour::start(&format!("{:06x}", first.unwrap_or(0xf0_f0_f2)));
            }
            Intent::Handle(number) => {
                let same = self.handle == *number;
                self.shut();
                if !same {
                    self.handle = *number;
                    self.pick = Colour::start(self.handle_hex());
                }
                self.own_key();
            }
            Intent::Pick(change) => {
                if let Some(colour) = self.pick.apply(change.clone()) {
                    self.use_hex(format_hex(colour));
                }
            }
            Intent::Shut => self.shut(),
            Intent::Light(light) => {
                self.light = *light;
                self.shut();
            }
            Intent::Surprise => self.surprise(),
            Intent::Close | Intent::Apply { .. } => self.open = false,
            Intent::Gone => self.shown = self.open,
            Intent::Ignored => {}
        }
        self.refresh();
    }

    /// The picker's colour as the one colour, or as the open handle's.
    fn use_hex(&mut self, hex: String) {
        if self.handle > 0 {
            let at = usize::from(self.handle) - 1;
            let slots = if self.effect == 2 {
                &mut self.custom.gradient[..]
            } else {
                &mut self.custom.prism[..]
            };
            if let Some(slot) = slots.get_mut(at) {
                *slot = hex;
            }
            self.own_key();
        } else {
            self.key = hex;
        }
    }

    /// A random face, effect and preset, from a small generator of its own.
    fn surprise(&mut self) {
        let mut next = |bound: usize| {
            self.seed = self
                .seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223)
                .wrapping_add(7);
            (self.seed >> 8) as usize % bound
        };
        let font = presets::FACES[next(presets::FACES.len())].0;
        let effect = presets::EFFECTS[next(presets::EFFECTS.len())];
        let list = presets::of(effect);
        let key = list[next(list.len())];
        self.font = font;
        self.effect = effect;
        self.key = key.to_owned();
        self.shut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> NameStyle {
        NameStyle::opened(Card::default(), 11, 1, "")
    }

    #[test]
    fn a_new_window_starts_on_the_first_preset_and_changed() {
        let w = window();
        assert_eq!(w.key, "f0f0f2");
        assert!(w.changed());
        let kept = NameStyle::opened(Card::default(), 11, 2, "2dafa7,fec491");
        assert!(!kept.changed());
        assert_eq!(kept.custom.gradient, ["2dafa7", "fec491"]);
    }

    #[test]
    fn a_wrong_hex_is_kept_and_a_right_one_paints() {
        let mut w = window();
        w.update(&Intent::Hex);
        assert_eq!(w.pick.text, "f0f0f2");
        w.update(&Intent::Pick(Change::Text("12".into())));
        assert_eq!(w.pick.text, "12");
        assert_eq!(w.key, "f0f0f2");
        w.update(&Intent::Pick(Change::Text("#00ff99".into())));
        assert_eq!(w.key, "00ff99");
        assert_eq!(
            w.card.name_style.map(|s| s.colors().to_vec()),
            Some(vec![0x00ff99])
        );
    }

    #[test]
    fn a_handle_edits_its_own_colour_of_the_bar() {
        let mut w = window();
        w.update(&Intent::Effect(2));
        w.update(&Intent::Handle(2));
        assert_eq!(w.pick.text, "fec491");
        w.update(&Intent::Pick(Change::Text("000000".into())));
        assert_eq!(w.key, "2dafa7,000000");
        w.update(&Intent::Pick(Change::Hue(120.0)));
        assert_eq!(w.key, "2dafa7,000000", "black stays black on any hue");
        w.update(&Intent::Pick(Change::Pad { sat: 1.0, val: 1.0 }));
        assert_eq!(w.key, "2dafa7,00ff00");
        w.update(&Intent::Shut);
        assert!(!w.picking());
        w.update(&Intent::Handle(2));
        w.update(&Intent::Handle(2));
        assert_eq!(w.handle, 0);
    }
}
