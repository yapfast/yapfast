//! The banner's colour and the profile theme's two colours: their
//! swatches, the handles that open a pad, and a typed code read.

use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Space, button, column, container, row, text, text_input},
};
use yapfast_core::{
    application::{ProfileEditor, ProfileText},
    chat::ProfilePicture,
};

use crate::{
    app::Message,
    controls::{Kind, action, lights},
    fonts::SEMIBOLD,
    form,
    popover::{Popover, Side, Width},
    style::{faded, hex, rounded},
    widgets::ColourPad,
};

use super::{COLOUR, Editor, Event, Pop, SWATCHES, THEME_COLOUR, message};

/// A colour's code as 0xRRGGBB, if it's one.
pub(super) fn parse_colour(code: &str) -> Option<u32> {
    yapfast_core::application::parse_colour(code).ok().flatten()
}

/// The banner colour, opened by a click on the banner: its swatch, its
/// code, Default, and a few to pick from.
pub(super) fn banner_colour<'a>(
    ctx: crate::ctx::Ctx<'a>,
    core: &'a ProfileEditor,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let colour = core.accent();
    let wrong = colour.is_err();
    let swatch_fill = colour.ok().flatten().map_or(theme.raised, hex);
    let line = theme.line;
    let swatch = container(Space::new())
        .width(28)
        .height(28)
        .style(move |_| crate::style::outlined(swatch_fill, line, 6.0));
    let (raised, danger) = (theme.raised, theme.danger);
    let field = form::field(
        theme,
        COLOUR,
        crate::i18n::tr!(ctx.i18n, "Default"),
        &core.colour,
    )
    .size(14)
    .padding(8)
    .width(116)
    .on_input(|text| message(Event::Write(ProfileText::Colour, text)))
    .on_submit(message(Event::Picture(ProfilePicture::Banner)))
    .style({
        let (ink, hint, accent, selection) =
            (theme.text, theme.muted, theme.accent, theme.selection);
        move |_, status| text_input::Style {
            background: if wrong { faded(danger, 0.16) } else { raised }.into(),
            border: Border {
                color: faded(accent, 0.8),
                width: if matches!(status, text_input::Status::Focused { .. }) {
                    1.0
                } else {
                    0.0
                },
                radius: 6.0.into(),
            },
            icon: ink,
            placeholder: hint,
            value: ink,
            selection,
        }
    });
    let mut head = row![
        text(crate::i18n::tr!(ctx.i18n, "Banner color"))
            .size(14)
            .font(SEMIBOLD)
            .color(theme.text),
        swatch,
        field
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if wrong {
        head = head.push(
            text(crate::i18n::tr!(ctx.i18n, "Not a colour"))
                .size(13)
                .color(theme.danger),
        );
    }
    head = head.push(Space::new().width(Length::Fill));
    if !core.colour.is_empty() {
        head = head.push(
            action(
                theme,
                crate::i18n::tr!(ctx.i18n, "Default"),
                Kind::Ghost,
                Some(message(Event::Write(ProfileText::Colour, String::new()))),
            )
            .small(),
        );
    }
    let mut swatches = row![].spacing(6);
    let text_ink = theme.text;
    for swatch in SWATCHES {
        let code = format!("#{swatch:06x}");
        let chosen = core.colour == code;
        let fill = hex(swatch);
        swatches = swatches.push(
            button(Space::new())
                .width(26)
                .height(26)
                .on_press(message(Event::Write(ProfileText::Colour, code)))
                .style(move |_, status| button::Style {
                    background: Some(fill.into()),
                    border: Border {
                        color: text_ink,
                        width: if chosen {
                            2.0
                        } else if lights(status) {
                            1.0
                        } else {
                            0.0
                        },
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }),
        );
    }
    let sunken = theme.sunken;
    container(column![head, swatches].spacing(10))
        .padding(10)
        .style(move |_| rounded(sunken, 8))
        .into()
}

/// A theme colour's handle on its tile, which opens its code.
pub(super) fn theme_handle<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    pop: Pop,
    fill: Color,
) -> Element<'a, Message> {
    let open = editor.pop == Some(pop);
    let handle = button(Space::new())
        .width(24)
        .height(24)
        .on_press(message(Event::Pop(Some(pop))))
        .style(move |_, _| button::Style {
            background: Some(fill.into()),
            border: Border {
                color: Color::WHITE,
                width: if open { 3.0 } else { 2.0 },
                radius: 7.0.into(),
            },
            ..Default::default()
        });
    let (field, label) = match pop {
        Pop::Top => (ProfileText::ThemeTop, core.theme_top.as_str()),
        _ => (ProfileText::ThemeBottom, core.theme_bottom.as_str()),
    };
    let theme = ctx.look.theme;
    let code = open.then(|| {
        form::colour_card(
            theme,
            ColourPad::new(editor.hsv, |hsv| message(Event::Hsv(hsv))),
            form::field(theme, THEME_COLOUR, "#rrggbb", label)
                .on_input(move |text| message(Event::Write(field, text)))
                .on_submit(message(Event::Pop(None))),
        )
    });
    Popover::new(handle, code, Side::Below)
        .width(Width::Own(form::COLOUR_CARD))
        .gap(6.0)
        .outside(message(Event::Pop(None)))
        .into()
}
