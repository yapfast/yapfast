//! A participant's menu: `VolumeMenu` (`ui/voice.slint:1594`), the card over
//! the context menu's rows with the person's name and their volumes, the
//! voice's and, while they share their screen, the stream's. Task V11
//! (docs/iced/tasks/voice.md). The rows under it are the window's menu
//! (`vm.menu`); the shell draws both, this is the card alone.

use iced::border::Radius;
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{column, container, text};
use iced::{Border, Color, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::voice::participant_menu::{
    Intent, MAXIMUM, NAME_HEIGHT, PADDING, ParticipantMenu, SPACING,
};
use anomi_widgets::slider::{self, Volume};
use yapfast_lang::tr;

/// The card's corners, as a context menu's (`border-radius: 8px`).
const RADIUS: f32 = metrics::RADIUS_CARD;
/// Its outline and shadow (`ui/voice.slint:1608-1610`: `border-width: 1px`,
/// `drop-shadow-blur: 24px; drop-shadow-color: #00000080;
/// drop-shadow-offset-y: 8px`).
const OUTLINE: f32 = 1.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DROP: f32 = 8.0;

pub fn view(menu: &ParticipantMenu, _motion: Motion) -> impl View<Intent> + '_ {
    let name = text(menu.name.as_str())
        .size(typography::size::BODY)
        .font(typography::BOLD)
        .width(Length::Fill)
        .height(NAME_HEIGHT)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .style(catalog::text::primary);
    let mut content = column![name, volume(menu, false)].spacing(SPACING);
    if menu.live {
        content = content.push(volume(menu, true));
    }
    let joined = menu.joined;
    container(content.padding(PADDING))
        .width(menu.width)
        .height(menu.height())
        .style(move |theme: &Theme| card(theme, joined))
}

/// One slider: the person's voice, or their stream.
fn volume(menu: &ParticipantMenu, stream: bool) -> Element<'_, Intent> {
    let (label, value) = if stream {
        (tr!("Stream volume"), menu.stream_volume)
    } else {
        (tr!("User volume"), menu.voice_volume)
    };
    let say = move |person: &str, volume, last| Intent::Volume {
        person: person.to_owned(),
        stream,
        volume,
        last,
    };
    slider::volume(
        Volume::up_to(label, value, MAXIMUM),
        move |volume| say(&menu.person, volume, false),
        move |volume| say(&menu.person, volume, true),
    )
}

/// A context menu's ground, outline and shadow; square at the bottom while
/// the rows join it.
fn card(theme: &Theme, joined: bool) -> container::Style {
    let palette = theme.palette();
    let corner = if joined { 0.0 } else { RADIUS };
    container::Style {
        background: Some(palette.menu().into()),
        border: Border {
            color: palette.menu_line(),
            width: OUTLINE,
            radius: Radius::new(RADIUS).bottom_left(corner).bottom_right(corner),
        },
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window with a participant's menu
/// open over the stage, as `voice/participant_menu.png`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::voice::participant_menu as fixture;
    vec![
        Shot {
            area: "voice",
            name: "participant_menu",
            size: GOLDEN,
            theme: "bonfire",
            vm: fixture::carol,
        },
        Shot {
            area: "voice",
            name: "participant_menu_stream",
            size: GOLDEN,
            theme: "bonfire",
            vm: fixture::sharing,
        },
    ]
}
