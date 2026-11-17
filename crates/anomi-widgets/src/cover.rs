//! What lies over content: the spoiler cover and the banner.
//!
//! - [`spoiler_cover`] and [`spoiler`]: `SpoilerCover` (`ui/controls.slint:1146`),
//!   the dim layer with a SPOILER pill that lies over a spoiler's blurred
//!   picture until it is clicked. tiny-skia has no blur to draw with, so a
//!   spoiler is blurred data under this cover: callers pass
//!   `picture(blurred(&handle))` ([`crate::picture::blurred`]) while covered
//!   and the handle itself once revealed.
//! - [`banner`]: `Banner` (`ui/controls.slint:1722`), a dismissible notice
//!   across the window, such as a renderer fallback.

use iced::font::Weight;
use iced::mouse::Interaction;
use iced::widget::text::{Style as TextStyle, Wrapping};
use iced::widget::{container, mouse_area, row, stack, text};
use iced::{Border, Length, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::{rgba, typography};

use crate::icon::{Icon, icon};

/// The dim layer over the picture: `#00000059`.
const DIM: u32 = 0x0000_0059;
/// The pill's ground: `#000000b3`.
const PILL: u32 = 0x0000_00b3;
const PILL_HEIGHT: f32 = 28.0;
const PILL_RADIUS: f32 = 14.0;
/// Room each side of the word: the Slint pill is its text plus 24 px.
const PILL_PAD: f32 = 12.0;
/// `font-size: 13px; font-weight: 800`.
const PILL_TEXT: f32 = 13.0;
/// `letter-spacing: 0.5px`, which iced lacks: it goes into the padding.
const PILL_TRACKING: f32 = 0.5;

/// Half of the tracking of `word`, to add to the padding of each side.
fn tracking(letter_spacing: f32, word: &str) -> f32 {
    letter_spacing * word.chars().count() as f32 / 2.0
}

/// The dim layer and its pill, filling what it lies over, its corners
/// `radius` so it ends at a rounded picture's edge (8 for gallery cells,
/// link thumbnails and search results, 4 for embed images). It draws; the
/// caller's click reveals (see [`spoiler`]).
pub fn spoiler_cover<'a, Message: 'a>(radius: f32) -> impl View<Message> + 'a {
    let word = tr!("SPOILER");
    let pad = tracking(PILL_TRACKING, word);
    container(
        container(
            text(word)
                .size(PILL_TEXT)
                .font(typography::weight(Weight::ExtraBold))
                .wrapping(Wrapping::None)
                .style(|_theme: &Theme| TextStyle {
                    color: Some(iced::Color::WHITE),
                    ..TextStyle::default()
                }),
        )
        .padding(Padding::from([0.0, PILL_PAD + pad]))
        .height(Length::Fixed(PILL_HEIGHT))
        .align_y(alignment::Vertical::Center)
        .style(|_theme: &Theme| container::Style {
            background: Some(rgba(PILL).into()),
            border: iced::Border::default().rounded(PILL_RADIUS),
            ..container::Style::default()
        }),
    )
    .center(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(rgba(DIM).into()),
        border: Border::default().rounded(radius),
        ..container::Style::default()
    })
}

/// `content` under a spoiler's cover while `covered`; a click on it asks to
/// reveal it with `reveal`. Uncovered, it is `content` alone and takes no
/// click of its own. Pass `picture(blurred(&handle))` as `content` while
/// covered and the handle itself once revealed; `radius` is the cell's, as
/// for [`spoiler_cover`].
pub fn spoiler<'a, Message: Clone + 'a>(
    content: impl View<Message> + 'a,
    covered: bool,
    radius: f32,
    reveal: Message,
) -> Element<'a, Message> {
    if !covered {
        return content.boxed();
    }
    // The click sits on the top layer: the stack updates it first and its
    // capture stops a press from reaching clickable content below.
    stack![
        content.boxed(),
        mouse_area(spoiler_cover(radius))
            .on_press(reveal)
            .interaction(Interaction::Pointer)
            .boxed()
    ]
    .boxed()
}

/// The banner's icon: `width: 18px`.
const BANNER_ICON: f32 = 18.0;
/// `padding-left: 16px; padding-right: 8px; padding-top/bottom: 6px`.
const BANNER_PADDING: Padding = Padding {
    top: 6.0,
    right: 8.0,
    bottom: 6.0,
    left: 16.0,
};
const BANNER_GAP: f32 = 12.0;
/// Dismiss's id, which tests find it by.
const DISMISS: &str = "banner-dismiss";
/// The notice's size: `font-size: 15px`.
const BANNER_TEXT: f32 = 15.0;
/// A notice across the window, its text wrapping at words, with Dismiss at
/// its end asking `on_dismiss`.
pub fn banner<'a, Message: Clone + 'a>(
    notice: &'a str,
    on_dismiss: Message,
    motion: Motion,
) -> impl View<Message> + 'a {
    container(
        row![
            icon(Icon::TriangleAlert, BANNER_ICON).style(|theme: &Theme, _status| {
                iced::widget::svg::Style {
                    color: Some(theme.palette().accent),
                }
            }),
            text(notice)
                .size(BANNER_TEXT)
                .width(Length::Fill)
                .style(catalog::text::primary),
            crate::button::action_button(tr!("Dismiss"), crate::button::Kind::Ghost, motion)
                .small()
                .id(iced::advanced::widget::Id::new(DISMISS))
                .on_press(on_dismiss),
        ]
        .spacing(BANNER_GAP)
        .align_y(alignment::Vertical::Center),
    )
    .padding(BANNER_PADDING)
    .width(Length::Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().raised.into()),
        ..container::Style::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced::widget::button;
    use iced_test::Simulator;

    type Sim<'a> = Simulator<'a, &'static str, Theme, anomi_theme::Renderer>;

    fn sim(view: impl View<&'static str> + 'static) -> Sim<'static> {
        Simulator::with_size(iced::Settings::default(), Size::new(400.0, 200.0), view)
    }

    #[test]
    fn a_covered_spoiler_reveals_on_a_click_and_opens_nothing_else() {
        let picture = container(text("under")).width(200).height(100);
        let mut ui = sim(spoiler(picture, true, 8.0, "reveal"));
        assert!(ui.find("SPOILER").is_ok());
        ui.click("SPOILER").expect("the pill is there");
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["reveal"]);
    }

    #[test]
    fn a_covered_spoiler_reveals_over_clickable_content() {
        let content = button(text("under")).on_press("view");
        let mut ui = sim(spoiler(content, true, 8.0, "reveal"));
        ui.click("SPOILER").expect("the pill is there");
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["reveal"]);
    }

    #[test]
    fn a_blurred_picture_is_covered_and_reveals() {
        use crate::picture::{self, picture as pic, sample};
        let blurred = picture::blurred(&sample::sunset(800, 500)).expect("it blurs");
        let content = pic(blurred).width(550.0).height(344.0);
        let mut ui: Sim<'static> = Simulator::with_size(
            iced::Settings::default(),
            Size::new(600.0, 400.0),
            spoiler(content, true, 8.0, "reveal"),
        );
        ui.click("SPOILER").expect("the pill is there");
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["reveal"]);
    }

    #[test]
    fn the_pill_is_wider_by_its_tracking() {
        assert_eq!(tracking(PILL_TRACKING, "SPOILER"), 1.75);
    }

    #[test]
    fn a_click_anywhere_on_the_cover_reveals() {
        let picture = container(text("under")).width(200).height(100);
        let mut ui = sim(spoiler(picture, true, 8.0, "reveal"));
        ui.point_at(iced::Point::new(5.0, 5.0));
        ui.simulate(iced_test::simulator::click());
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["reveal"]);
    }

    #[test]
    fn a_revealed_spoiler_is_its_content_alone() {
        let picture = container(text("under")).width(200).height(100);
        let mut ui = sim(spoiler(picture, false, 8.0, "reveal"));
        assert!(ui.find("SPOILER").is_err());
        assert!(ui.find("under").is_ok());
        ui.point_at(iced::Point::new(5.0, 5.0));
        ui.simulate(iced_test::simulator::click());
        assert!(ui.into_messages().next().is_none());
    }

    #[test]
    fn the_banner_shows_its_notice_and_dismisses() {
        let mut ui = sim(banner("Software rendering", "dismissed", Motion::default()));
        assert!(ui.find("Software rendering").is_ok());
        ui.click(iced_test::selector::id(DISMISS))
            .expect("the button is there");
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["dismissed"]);
    }

    #[test]
    fn the_banner_wraps_a_long_notice_inside_its_width() {
        let long = "A long notice that does not fit on one line of a narrow window, \
                    so it wraps at words and the button stays where it is. "
            .repeat(3);
        let long: &'static str = Box::leak(long.into_boxed_str());
        let mut ui: Sim<'static> = Simulator::with_size(
            iced::Settings::default(),
            Size::new(300.0, 300.0),
            banner(long, "dismissed", Motion::default()),
        );
        let button = ui
            .find(iced_test::selector::id(DISMISS))
            .expect("the button is there")
            .bounds();
        assert!(button.x + button.width <= 300.0, "{button:?}");
    }
}
