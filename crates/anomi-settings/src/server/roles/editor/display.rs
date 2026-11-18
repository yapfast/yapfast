//! The Display tab: the role's name, colour and icon, and the two switches
//! (`ui/roles.slint:451`-`:520`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::settings::role_editor::{Intent, RoleEditor};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::{GAP, colors, label};
use crate::kit::{heading, switch};

const PART: f32 = 8.0;
const ICON_GAP: f32 = 16.0;
/// The icon's well, its corners, the picture and the empty glyph in it, the
/// emoji's size.
const WELL: f32 = 64.0;
const WELL_RADIUS: f32 = 8.0;
const PICTURE: f32 = 48.0;
const EMPTY: f32 = 24.0;
const EMOJI: f32 = 36.0;
const EMOJI_WIDTH: f32 = 110.0;
const CONTROL: f32 = 36.0;
const PAD: f32 = 10.0;

/// A part of the page: its heading, then what it holds.
fn part<'a>(
    name: &'a str,
    parts: impl IntoIterator<Item = Element<'a, Intent>>,
) -> Element<'a, Intent> {
    column![heading(name)]
        .extend(parts)
        .spacing(PART)
        .width(Length::Fill)
        .boxed()
}

fn name<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let mut name = field("", editor.name.as_str()).motion(motion);
    if editor.editable() {
        name = name.on_input(Intent::Name);
    }
    part(tr!("ROLE NAME"), [name.view().boxed()])
}

/// The icon's well: the picture once it's here, the emoji, or an image's
/// outline.
fn well(editor: &RoleEditor) -> Element<'_, Intent> {
    let inside: Element<'_, Intent> = match &editor.icon {
        Expected::Ready(handle) => picture(handle.clone())
            .side(PICTURE)
            .fit(Fit::Contain)
            .boxed(),
        _ if !editor.emoji.is_empty() => text(editor.emoji.as_str()).size(EMOJI).boxed(),
        _ => icon(Icon::Image, EMPTY).boxed(),
    };
    container(inside)
        .center(WELL)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sunken.into()),
                border: Border::default()
                    .rounded(WELL_RADIUS)
                    .width(1)
                    .color(palette.line),
                ..container::Style::default()
            }
        })
        .boxed()
}

fn emoji<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let live = editor.editable() && editor.icon_locked.is_empty();
    let mut input = field(tr!("Emoji"), editor.emoji_text.as_str())
        .text_size(typography::size::SMALL)
        .pad(PAD)
        .height(CONTROL)
        .motion(motion);
    if live {
        input = input
            .on_input(Intent::EmojiText)
            .on_submit(Intent::Emoji(editor.emoji_text.clone()));
    }
    row![
        container(input.view()).width(EMOJI_WIDTH),
        action_button(tr!("Use Emoji"), Kind::Ghost, motion)
            .enabled(live && !editor.emoji_text.is_empty())
            .on_press(Intent::Emoji(editor.emoji_text.clone()))
            .boxed(),
    ]
    .spacing(PART)
    .align_y(Alignment::Center)
    .boxed()
}

fn icon_part<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let live = editor.editable() && editor.icon_locked.is_empty();
    let mut actions = row![
        action_button(tr!("Choose Image"), Kind::Secondary, motion)
            .enabled(live && !editor.reading)
            .on_press(Intent::PickIcon)
    ]
    .spacing(PART);
    if editor.has_icon {
        actions = actions.push(
            action_button(tr!("Remove Icon"), Kind::Ghost, motion)
                .enabled(live)
                .on_press(Intent::RemoveIcon)
                .boxed(),
        );
    }
    let mut side = column![].spacing(PART);
    if !editor.icon_locked.is_empty() {
        side = side.push(label(editor.icon_locked.as_str()));
    }
    if editor.reading {
        side = side.push(label(tr!("Reading the picture...")));
    }
    let side = side.push(actions.boxed()).push(emoji(editor, motion));
    part(
        tr!("ROLE ICON"),
        [
            label(tr!(
                "Upload an image under 256 KB or use an emoji. We recommend at least 64x64 pixels. Members will see the icon for their highest role if they have multiple roles."
            )),
            row![well(editor), side].spacing(ICON_GAP).boxed(),
        ],
    )
}

pub fn view<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let enabled = editor.editable();
    column![
        name(editor, motion),
        part(
            tr!("ROLE COLOR"),
            [
                label(tr!(
                    "Members use the color of the highest role they have on the roles list."
                )),
                colors::view(editor, motion),
            ]
        ),
        icon_part(editor, motion),
        switch(
            tr!("Display role members separately from online members"),
            editor.hoist,
            motion
        )
        .enabled(enabled)
        .on_toggle(|_| Intent::Hoist)
        .view(),
        switch(
            tr!("Allow anyone to @mention this role"),
            editor.mentionable,
            motion
        )
        .description(tr!(
            "Note: Members with the \"Mention @everyone, @here, and All Roles\" permission will always be able to ping this role."
        ))
        .enabled(enabled)
        .on_toggle(|_| Intent::Mentionable)
        .view(),
    ]
    .spacing(GAP)
    .boxed()
}
