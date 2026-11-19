//! The card's texts: the banner colour's box, the name and the line under
//! it (username, pronouns, server tag), the badges, and the bio
//! (`ui/server.slint:1946`-`:2090`, `:2178`-`:2236`). A text turns into a
//! field when it or its pencil is clicked.

use iced::widget::{Space, button, column, container, hover, row, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, rgb, typography};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::people::editor::{Editor, Field, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::composer::composer;
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::name::{self, styled_name};
use anomi_widgets::tag::server_tag;
use yapfast_lang::tr;

use super::{EDGE, hover_edit, inset};
use crate::profile::card::look::{Inks, Tone};

/// The ids of the fields, which the app focuses as one opens.
pub const NAME: &str = "profile-editor-name";
pub const PRONOUNS: &str = "profile-editor-pronouns";
pub const BIO: &str = "profile-editor-bio";
pub const COLOUR: &str = "profile-editor-colour";

/// The gap between the texts under the name, and how far the pronouns'
/// outline reaches past them (`x: -5px`).
const GAP: f32 = 6.0;
const PRONOUNS_OUT: f32 = 5.0;
/// The bio's box.
const BIO_BOX: f32 = 110.0;

/// The few colours to pick from without typing.
const SWATCHES: [(&str, u32); 9] = [
    ("#5865f2", 0x5865f2),
    ("#3ba55d", 0x3ba55d),
    ("#faa61a", 0xfaa61a),
    ("#ed4245", 0xed4245),
    ("#eb459e", 0xeb459e),
    ("#1abc9c", 0x1abc9c),
    ("#9b59b6", 0x9b59b6),
    ("#607d8b", 0x607d8b),
    ("#2c2f33", 0x2c2f33),
];

/// Words in one of the card's inks.
pub fn words<'a>(
    words: impl text::IntoFragment<'a>,
    size: f32,
    ink: Tone,
    font: iced::Font,
) -> iced::widget::Text<'a, Theme> {
    text(words)
        .size(size)
        .font(font)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        })
}

/// The banner colour, opened by clicking the banner: a sample, a field for
/// its hex code and a few to pick from. A wrong draft is kept and said.
pub fn colour_box<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let sample = editor.card.accent.filter(|_| !editor.colour_wrong);
    let wrong = editor.colour_wrong;
    let input = container(
        field(tr!("ProfileEditorDialog" => "Default"), &editor.colour)
            .id(COLOUR)
            .on_input(|text| Intent::Write(Field::Colour, text))
            .on_submit(Intent::ColourBox(false))
            .text_size(14.0)
            .pad(8.0)
            .height(32.0)
            .error(wrong.then(|| tr!("Not a colour")))
            .motion(motion)
            .view(),
    )
    .width(116.0);
    let mut line = row![
        text(tr!("Banner color"))
            .size(14)
            .font(typography::SEMIBOLD),
        container(Space::new().width(28.0).height(28.0)).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(sample.unwrap_or(palette.raised))),
                border: Border::default()
                    .rounded(6.0)
                    .width(1.0)
                    .color(palette.line),
                ..container::Style::default()
            }
        }),
        input,
    ]
    .spacing(10.0)
    .align_y(Alignment::Center);
    line = line.push(Space::new().width(Fill).boxed());
    if !editor.colour.is_empty() {
        line = line.push(
            action_button(tr!("ProfileEditorDialog" => "Default"), Kind::Ghost, motion)
                .small()
                .on_press(Intent::Write(Field::Colour, String::new()))
                .boxed(),
        );
    }
    let mut swatches = row![].spacing(6.0);
    for (hex, value) in SWATCHES {
        let chosen = editor.colour == hex;
        swatches = swatches.push(
            button(Space::new().width(26.0).height(26.0))
                .padding(0)
                .on_press(Intent::Write(Field::Colour, hex.into()))
                .style(move |theme: &Theme, status| button::Style {
                    background: Some(Background::Color(rgb(value))),
                    border: Border {
                        radius: 6.0.into(),
                        width: match (chosen, status) {
                            (true, _) => 2.0,
                            (_, button::Status::Hovered) => 1.0,
                            _ => 0.0,
                        },
                        color: theme.palette().text,
                    },
                    ..catalog::button::bare(theme, status)
                }),
        );
    }
    container(column![line, swatches].spacing(10.0))
        .padding(10.0)
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sunken)),
            border: Border::default().rounded(8.0),
            ..container::Style::default()
        })
        .boxed()
}

/// The name and under it the username, the pronouns and the server tag,
/// then the badges and what is too long.
pub fn identity<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let card = &editor.card;
    let mut block = column![].spacing(2.0);
    let ink = inks.ink;
    block = block.push(if editor.editing == Some(Field::Name) {
        inset(
            field(&editor.name_placeholder, &editor.name)
                .id(NAME)
                .on_input(|text| Intent::Write(Field::Name, text))
                .on_submit(Intent::Edit(None))
                .text_size(22.0)
                .pad(8.0)
                .height(40.0)
                .motion(motion)
                .view(),
        )
    } else {
        let shown = if editor.loading {
            tr!("Loading...")
        } else {
            card.name.as_str()
        };
        let name = styled_name(shown, card.name_style)
            .size(if editor.main { 24.0 } else { 26.0 })
            .weight(iced::font::Weight::Bold);
        let name = match ink {
            Tone::Fixed(color) => name.ink(name::Ink::Fixed(color)),
            Tone::Token(token) => name.ink(name::Ink::Token(token)),
        };
        hover_edit(
            name,
            editor.ready() && editor.may.name,
            EDGE,
            true,
            Intent::Edit(Some(Field::Name)),
        )
    });
    block = block.push(inset(handle_line(editor, inks, motion)));
    if editor.main && editor.editing != Some(Field::Pronouns) && !card.badges.badges.is_empty() {
        block = block.push(
            container(crate::profile::badges::view(&card.badges, motion).map(
                |intent| match intent {
                    anomi_vm::people::badges::Intent::OpenLink(url) => Intent::OpenLink(url),
                    _ => Intent::Edit(None),
                },
            ))
            .padding(Padding {
                top: 10.0,
                left: EDGE,
                right: EDGE,
                ..Padding::ZERO
            })
            .boxed(),
        );
    }
    if !editor.too_long.is_empty() {
        block = block.push(inset(text(editor.too_long.as_str()).size(13).style(
            |theme: &Theme| text::Style {
                color: Some(theme.palette().danger),
                ..text::Style::default()
            },
        )));
    }
    block.boxed()
}

/// The username, then the pronouns or their field, then the tag.
fn handle_line<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let card = &editor.card;
    let mut line = row![words(
        format!("{} \u{b7}", card.handle),
        14.0,
        inks.ink2,
        typography::MEDIUM
    )]
    .spacing(GAP - PRONOUNS_OUT)
    .align_y(Alignment::Center);
    if editor.editing == Some(Field::Pronouns) {
        let hint = if editor.pronouns_fallback.is_empty() {
            tr!("Add your pronouns")
        } else {
            editor.pronouns_fallback.as_str()
        };
        return line
            .push(
                container(
                    field(hint, &editor.pronouns)
                        .id(PRONOUNS)
                        .on_input(|text| Intent::Write(Field::Pronouns, text))
                        .on_submit(Intent::Edit(None))
                        .text_size(14.0)
                        .pad(8.0)
                        .height(30.0)
                        .motion(motion)
                        .view(),
                )
                .width(220.0)
                .padding(Padding {
                    left: PRONOUNS_OUT,
                    ..Padding::ZERO
                })
                .boxed(),
            )
            .boxed();
    }
    let (shown, ink) = if !editor.pronouns.is_empty() {
        (editor.pronouns.as_str(), inks.ink2)
    } else if !editor.pronouns_fallback.is_empty() {
        (editor.pronouns_fallback.as_str(), inks.ink2)
    } else {
        (tr!("Add pronouns"), inks.ink3)
    };
    line = line.push(hover_edit(
        words(shown, 14.0, ink, typography::MEDIUM),
        editor.ready(),
        PRONOUNS_OUT,
        false,
        Intent::Edit(Some(Field::Pronouns)),
    ));
    if editor.main {
        line = line.push(tag_chip(editor, inks, motion));
    }
    line.boxed()
}

/// The server tag worn, and where another is chosen: its words and a
/// chevron on a wash of the card's ink.
fn tag_chip<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let (ink, ink2) = (inks.ink, inks.ink2);
    let face = move |lit: bool| -> Element<'a, Intent> {
        let inside: Element<'a, Intent> = match &editor.card.tag {
            Some(tag) => server_tag(tag, motion)
                .fill(Color::TRANSPARENT)
                .view()
                .boxed(),
            None => words(
                tr!("Server Tag"),
                12.0,
                ink2,
                iced::Font {
                    style: iced::font::Style::Italic,
                    ..typography::UI
                },
            )
            .boxed(),
        };
        let chevron =
            icon(Icon::ChevronDown, 14.0).style(move |theme: &Theme, _| iced::widget::svg::Style {
                color: Some(ink2.at(theme.palette())),
            });
        container(
            row![inside, chevron]
                .spacing(5.0)
                .align_y(Alignment::Center),
        )
        .height(24.0)
        .padding(Padding {
            left: 7.0,
            right: 10.0,
            ..Padding::ZERO
        })
        .align_y(Alignment::Center)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(
                ink.at(theme.palette())
                    .scale_alpha(if lit { 0.12 } else { 0.07 }),
            )),
            border: Border::default().rounded(7.0),
            ..container::Style::default()
        })
        .boxed()
    };
    opener(
        button(hover(face(false), face(true)))
            .padding(0)
            .style(catalog::button::bare),
    )
    .on_press(|at| Intent::TagMenu(Spot::below(at)))
    .boxed()
}

/// The bio's box, 110 px tall, with what it says under it.
pub fn bio_field<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let left = editor.bio_left;
    let note = words(
        tr!("You can use markdown and links if you'd like."),
        12.0,
        inks.ink3,
        typography::UI,
    );
    let count: Element<'a, Intent> = if left < 0 {
        text(tr!("Too long by {}", -left))
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().danger),
                ..text::Style::default()
            })
            .boxed()
    } else {
        words(tr!("{} left", left), 12.0, inks.ink3, typography::UI).boxed()
    };
    // The bio writes over lines, as Slint's box does: the composer's field,
    // which Enter doesn't leave, in a sunken box with the accent's rim.
    let well = container(
        composer(&editor.bio, motion)
            .id(BIO)
            .placeholder(editor.bio_placeholder.as_str())
            .muted_placeholder()
            .size(14.0)
            .pad(8.0)
            .min_height(BIO_BOX - 2.0)
            .max_height(BIO_BOX - 2.0)
            .on_edit(|text, _caret| Intent::Write(Field::Bio, text)),
    )
    .height(BIO_BOX)
    .padding(Padding::from([1.0, 10.0]))
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(Background::Color(palette.sunken)),
            border: Border::default()
                .rounded(6.0)
                .width(1.0)
                .color(palette.accent.scale_alpha(0.8)),
            ..container::Style::default()
        }
    });
    column![well, row![container(note).width(Fill), count]]
        .spacing(4.0)
        .boxed()
}
