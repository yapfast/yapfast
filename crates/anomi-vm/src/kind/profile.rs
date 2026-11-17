//! What a profile carries of its effect and its frame, as its view-model
//! holds them (P07's card, P09's full profile, P14's editor) and F16's
//! widgets draw them: `anomi_widgets::effect::effect` takes an [`Effect`],
//! `anomi_widgets::frame::frame` the [`FrameLayer`]s. Ported from Slint's
//! `ProfileData` effect fields and `ProfileFrameRow`; the projection fills
//! them as `effect_layers` and `frame_rows` do (`src/ui/profile.rs`).

use iced::widget::image;

/// A profile effect as a profile carries it. Slint's `effect-layers`,
/// `effect-still`, `effect-end` and `effect-loops` of `ProfileData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    /// What shows while it doesn't play, once it came: Discord's static
    /// frame. None draws nothing at rest.
    pub still: Option<image::Handle>,
    /// Its layers, the lowest first.
    pub layers: Vec<EffectLayer>,
}

impl Effect {
    /// When its last layer ends, in milliseconds into the play: an effect
    /// that doesn't loop is over then.
    pub fn end(&self) -> u64 {
        self.layers
            .iter()
            .map(|layer| u64::from(layer.start) + u64::from(layer.duration))
            .max()
            .unwrap_or(0)
    }

    /// Whether any layer loops, so that it plays for as long as it's asked.
    pub fn loops(&self) -> bool {
        self.layers.iter().any(|layer| layer.looped)
    }
}

/// One picture of an effect: where it lies on the card and when it shows.
/// Slint's `EffectLayer`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectLayer {
    /// Its picture once it came, as its first frame; the frames behind it
    /// are the picture store's (`anomi_widgets::picture::register`). None
    /// draws nothing, as Slint's picture of no size did.
    pub picture: Option<image::Handle>,
    /// Where its top left lies on the card, in logical pixels. It's as wide
    /// as the card, and as tall as that makes it.
    pub x: f32,
    pub y: f32,
    /// When it shows, in milliseconds into the play, and for how long.
    pub start: u32,
    pub duration: u32,
    /// It shows again `loop_delay` milliseconds after each round; with no
    /// delay it stays.
    pub looped: bool,
    pub loop_delay: u32,
}

/// One layer of a profile frame as a profile carries it, the back ones
/// first. Slint's `ProfileFrameRow`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameLayer {
    /// Its still once it came; none draws nothing.
    pub picture: Option<image::Handle>,
    pub kind: FrameKind,
    pub anchor: FrameAnchor,
    /// It goes over the card's contents rather than behind them.
    pub front: bool,
}

/// What a frame's layer is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameKind {
    /// Across the card and past its sides, once, beyond the end it's
    /// anchored to.
    #[default]
    Staple,
    /// The card's height, its picture at its anchor.
    Rail,
    /// Repeated down the card.
    Border,
}

impl FrameKind {
    /// The shop's name for it, `staple`, `rail` or `border`; none for one
    /// the client doesn't draw.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "staple" => Some(Self::Staple),
            "rail" => Some(Self::Rail),
            "border" => Some(Self::Border),
            _ => None,
        }
    }
}

/// Which end of the card a frame's layer keeps to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameAnchor {
    #[default]
    Top,
    Bottom,
    Center,
}

impl FrameAnchor {
    /// The shop's name for it, `top`, `bottom` or `center`; anything else
    /// keeps to the top, as Slint's last branch did.
    pub fn from_name(name: &str) -> Self {
        match name {
            "bottom" => Self::Bottom,
            "center" => Self::Center,
            _ => Self::Top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Effect, EffectLayer, FrameAnchor, FrameKind};

    #[test]
    fn an_effect_ends_with_its_last_layer_and_loops_with_any() {
        assert_eq!(Effect::default().end(), 0, "none, over at once");
        let effect = Effect {
            still: None,
            layers: vec![
                EffectLayer {
                    start: 1500,
                    duration: 3000,
                    looped: true,
                    ..EffectLayer::default()
                },
                EffectLayer {
                    duration: 2000,
                    ..EffectLayer::default()
                },
            ],
        };
        assert_eq!(effect.end(), 4500);
        assert!(effect.loops());
        assert!(!Effect::default().loops());
    }

    #[test]
    fn the_shops_names_map_as_the_card_reads_them() {
        assert_eq!(FrameKind::from_name("staple"), Some(FrameKind::Staple));
        assert_eq!(FrameKind::from_name("rail"), Some(FrameKind::Rail));
        assert_eq!(FrameKind::from_name("border"), Some(FrameKind::Border));
        assert_eq!(FrameKind::from_name("sticker"), None, "drawn by no branch");
        assert_eq!(FrameAnchor::from_name("bottom"), FrameAnchor::Bottom);
        assert_eq!(FrameAnchor::from_name("center"), FrameAnchor::Center);
        assert_eq!(FrameAnchor::from_name("top"), FrameAnchor::Top);
        assert_eq!(
            FrameAnchor::from_name(""),
            FrameAnchor::Top,
            "Slint's last branch"
        );
    }
}
