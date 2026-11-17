//! The server voice channel someone is in (`VoiceSpot`,
//! `ui/controls.slint:947`): a green speaker, then the channel and its
//! server; or, compact, the speaker alone with those in its tooltip, for a
//! line their status fills. Clicking it goes to the channel.

use iced::widget::{button, row, text};
use iced::{Length, alignment};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::kind::Spot;

use super::fade::Fade;
use crate::icon::{Icon, icon};
use crate::tip::{Side, tip};

/// The speaker is a pixel larger than the words (`root.font-size + 1px`).
const EXTRA: f32 = 1.0;
const GAP: f32 = 4.0;

/// The spot of `spot`, sending `on_press` when clicked. `compact` shows the
/// speaker alone, naming the place in a tooltip.
pub struct VoiceSpot<'a, Message> {
    place: &'a str,
    size: f32,
    compact: bool,
    on_press: Message,
    motion: Motion,
}

pub fn voice_spot<'a, Message>(
    spot: &'a Spot,
    on_press: Message,
    motion: Motion,
) -> VoiceSpot<'a, Message> {
    VoiceSpot {
        place: &spot.place,
        size: 13.0,
        compact: false,
        on_press,
        motion,
    }
}

impl<Message> VoiceSpot<'_, Message> {
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl<'a, Message: Clone + 'a> VoiceSpot<'a, Message> {
    pub fn view(self) -> Element<'a, Message> {
        let Self {
            place,
            size,
            compact,
            on_press,
            motion,
        } = self;
        let speaker =
            icon(Icon::Volume, size + EXTRA).style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().live),
            });
        if compact {
            let press = button(speaker)
                .padding(0)
                .style(catalog::button::bare)
                .on_press(on_press);
            // The tooltip borrows the place for the frame; keep it alive in
            // the tooltip's own text.
            return tip(press, place, Side::Above, motion).boxed();
        }
        let words = text(place)
            .size(size)
            .ellipsis(iced::widget::text::Ellipsis::End)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Shrink);
        let line = row![speaker, words]
            .spacing(GAP)
            .align_y(alignment::Vertical::Center);
        Fade::new(line.boxed(), on_press, motion).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced_test::Simulator;

    type Sim<'a> = Simulator<'a, &'static str, Theme, anomi_theme::Renderer>;

    #[test]
    fn the_spot_names_its_place_and_goes_there_when_clicked() {
        let spot = Spot::new("Lounge", "Home");
        let view = voice_spot(&spot, "go", Motion::default()).view();
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 40.0), view);
        ui.click("Lounge \u{2014} Home").expect("the place");
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["go"]);
    }

    #[test]
    fn a_compact_spot_shows_no_words() {
        let spot = Spot::new("Lounge", "Home");
        let view = voice_spot(&spot, "go", Motion::default())
            .compact(true)
            .view();
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 40.0), view);
        assert!(ui.find("Lounge \u{2014} Home").is_err());
    }

    #[test]
    fn clicking_a_compact_spot_sends_its_message() {
        let spot = Spot::new("Lounge", "Home");
        let view = voice_spot(&spot, "go", Motion::default())
            .compact(true)
            .view();
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 40.0), view);
        // The speaker is the whole view, at its corner.
        ui.point_at(iced::Point::new(6.0, 6.0));
        ui.simulate(iced_test::simulator::click());
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["go"]);
    }
}
