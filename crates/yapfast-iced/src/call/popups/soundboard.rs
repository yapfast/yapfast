//! The soundboard (`Soundboard`): the sounds of each server this
//! account may play, which also lists the entrance sounds to choose from.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, row, text},
};
use yapfast_core::voice::{self};

use crate::call::Event;
use crate::{
    app::Message,
    controls::{self},
    fonts::BOLD,
    pictures::Icon,
    widgets::{Hover, Pic},
};
use yapfast_core::i18n::{self, tr};

pub(crate) const SOUNDBOARD_WIDTH: f32 = 372.0;

pub(super) const SOUNDBOARD_TALLEST: f32 = 440.0;

/// A soundboard row, and a sound's button in it.
pub(super) const SOUND_ROW: f32 = 42.0;

pub(super) const SOUND: f32 = 36.0;

/// A board of sounds: `title`, a note (while there is one, no sound can be
/// chosen), the groups' sounds three to a row under their names, `chosen`'s
/// message for a click on one, and with `extras` hearing it first and its
/// star under the pointer. The call's soundboard, and the entrance sound's
/// choice in Settings.
pub(crate) fn sound_board<'a>(
    ctx: crate::ctx::Ctx<'a>,
    title: &'a str,
    groups: &'a [voice::SoundGroup],
    note: Option<&'a str>,
    extras: bool,
    chosen: impl Fn(&voice::SoundChoice) -> Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut top = column![text(title).size(16).font(BOLD).color(theme.text)]
        .spacing(4)
        .padding(Padding {
            top: 14.0,
            left: 14.0,
            right: 14.0,
            bottom: 4.0,
        });
    if let Some(note) = note {
        top = top.push(text(note).size(13).color(theme.muted));
    }
    if groups.is_empty() {
        top = top.push(text(tr("Loading sounds...")).size(13).color(theme.muted));
    }
    let cell = (SOUNDBOARD_WIDTH - 20.0 - 12.0) / 3.0;
    let mut rows = column![].padding([0, 10]);
    let mut count = 0;
    for group in groups {
        rows = rows.push(
            container(
                text(group.name.to_uppercase())
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            )
            .height(SOUND_ROW)
            .padding(Padding {
                bottom: 8.0,
                ..Padding::ZERO
            })
            .align_y(Alignment::End),
        );
        count += 1;
        for chunk in group.sounds.chunks(3) {
            let mut line = row![].spacing(6);
            for sound in chunk {
                let press = note.is_none().then(|| chosen(sound));
                line = line.push(sound_button(ctx, sound, cell, press, extras));
            }
            rows = rows.push(container(line).height(SOUND_ROW).align_y(Alignment::Center));
            count += 1;
        }
    }
    let listed = (count.max(1) as f32) * SOUND_ROW + 10.0;
    let room = (SOUNDBOARD_TALLEST - 80.0).max(SOUND_ROW);
    column![
        top,
        scroll_view(rows).height(if listed > room {
            Length::Fixed(room)
        } else {
            Length::Shrink
        })
    ]
    .width(SOUNDBOARD_WIDTH)
    .padding(Padding {
        bottom: 10.0,
        ..Padding::ZERO
    })
    .into()
}

/// A sound to play: its emoji and name; with the pointer on it, a button to
/// hear it first and its star.
fn sound_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    sound: &'a voice::SoundChoice,
    width: f32,
    press: Option<Message>,
    extras: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let enabled = press.is_some();
    let picture = match &sound.picture {
        Some(url) => ctx.picture(url, 18, 18, false),
        None => emojis::get(&sound.emoji).and_then(|emoji| ctx.emoji_picture(emoji.as_str(), 18)),
    };
    let mut face = row![].spacing(8).align_y(Alignment::Center);
    if let Some(picture) = picture {
        face = face.push(Pic::square(picture, 18.0));
    } else if sound.picture.is_some() {
        face = face.push(Space::new().width(18).height(18));
    }
    face = face.push(
        text(sound.name.as_str())
            .size(14)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
    );
    let (rest, lit) = (theme.button_default, theme.button_default_hover);
    let opacity = if enabled { 1.0 } else { 0.5 };
    let base = button(
        container(face)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| container::Style {
                text_color: None,
                ..Default::default()
            }),
    )
    .padding([0, 10])
    .width(width)
    .height(SOUND)
    .on_press_maybe(press)
    .style(move |_, status| button::Style {
        background: Some(
            crate::style::faded(
                if enabled && controls::lights(status) {
                    lit
                } else {
                    rest
                },
                opacity,
            )
            .into(),
        ),
        border: crate::style::round(8),
        ..Default::default()
    });
    let small = |icon, label: String, tint, ground: Color, message| {
        let face = button(ctx.look.glyph(icon, 15, tint).boxed(26))
            .padding(0)
            .width(26)
            .height(26)
            .on_press(message)
            .style(move |_, _| button::Style {
                background: Some(ground.into()),
                border: crate::style::round(6),
                ..Default::default()
            });
        iced::widget::tooltip(
            face,
            controls::tip(theme, label),
            iced::widget::tooltip::Position::Top,
        )
        .gap(6)
    };
    let star = |ground| {
        let (icon, tint, label) = if sound.favorite {
            (
                Icon::StarFilled,
                crate::style::hex(crate::style::CROWN),
                tr("Remove from Favorites"),
            )
        } else {
            (Icon::Star, theme.secondary, tr("Add to Favorites"))
        };
        small(
            icon,
            label.to_owned(),
            tint,
            ground,
            Message::Call(Event::Favorite(sound.id.clone(), !sound.favorite)),
        )
    };
    let corner = |content: Element<'a, Message>| {
        container(content)
            .padding([0, 4])
            .width(width)
            .height(SOUND)
            .align_x(Alignment::End)
            .align_y(Alignment::Center)
    };
    if !extras {
        return base.into();
    }
    // A favourite's star stays.
    let base: Element<'a, Message> = if sound.favorite {
        iced::widget::stack![base, corner(star(rest).into())].into()
    } else {
        base.into()
    };
    let over = corner(
        row![
            small(
                Icon::Volume,
                i18n::fill(tr("Preview {}"), &[&sound.name]),
                theme.text,
                lit,
                Message::Call(Event::Preview(sound.id.clone())),
            ),
            star(lit),
        ]
        .spacing(4)
        .into(),
    );
    mouse_area(Hover::new(base, over)).into()
}

/// Discord's soundboard in the call: the favourites, the call's
/// server's sounds, others', Discord's own, three to a row. A click
/// plays one for everyone; the pointer on one offers hearing it first
/// and starring it. While deafened or server muted, why none plays.
pub(super) fn soundboard<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let voice = &ctx.state.voice;
    let server_muted = voice
        .participants
        .iter()
        .any(|person| person.local && person.server_muted);
    let note = if voice.deafened {
        Some(tr("Soundboard disabled when deafened"))
    } else if server_muted {
        Some(tr("Soundboard disabled when server muted"))
    } else {
        None
    };
    let groups = voice.soundboard.as_deref().unwrap_or(&[]);
    sound_board(ctx, tr("Soundboard"), groups, note, true, |sound| {
        Message::Call(Event::Play(sound.id.clone()))
    })
}
