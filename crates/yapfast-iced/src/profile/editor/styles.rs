//! What the editor customizes beside the card: the avatar, decoration,
//! effect, nameplate and banner tiles, the theme, and the name's styles.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Size,
    widget::{Space, button, column, container, row, stack, text},
};
use yapfast_core::{
    application::{ProfileEditor, ProfileText},
    chat::ProfilePicture,
};

use crate::profile::Mode;
use crate::{
    app::Message,
    controls::{self, lights},
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::{hex, rounded, scrollbar},
    widgets::{Pic, PlateRow},
};

use super::{Editor, Event, Pop, SHOP, SIDE, TILE, message};

/// What the main profile wears, down the card's left, in the order
/// Discord's editor has it; what only Discord's shop sets opens it.
pub(super) fn customize<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let shown = &editor.shown;
    let profile = shown.look.profile();
    let ready = !core.loading() && !core.saving && core.reading.is_none();
    let avatar_enabled = ready && core.may_picture(ProfilePicture::Avatar);
    let banner_enabled = ready && core.may_picture(ProfilePicture::Banner);
    let look_enabled = core.may_write(ProfileText::ThemeTop);
    let shop = || Some(Message::Link(SHOP.into(), Point::ORIGIN));
    let plus = || Element::from(ctx.look.glyph(Icon::Plus, 26, Color::WHITE));
    let group = |title: &'a str, body: Element<'a, Message>| {
        column![text(title).size(13).font(SEMIBOLD).color(theme.text), body].spacing(8)
    };
    // The account's nameplate fills its tile, still and as strong as
    // under the pointer, with the avatar and name over it, as the
    // account card composes it; without one, the empty row and add.
    let nameplate_face: Element<'a, Message> =
        match crate::members::account_plate(ctx, 42.0, true, false) {
            Some((plate, palette)) => PlateRow::new(
                row![
                    ctx.avatar(profile.avatar.as_deref(), &profile.name, 35.0, 17.5, 1.0),
                    text(profile.name.as_str())
                        .size(15)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None)
                        .width(65),
                    Space::new().width(Length::Fill),
                ]
                .spacing(7)
                .padding([0, 6])
                .height(42)
                .align_y(Alignment::Center),
                Some(plate),
                palette,
                theme.raised,
                0.0,
                1.0,
            )
            .account()
            .into(),
            None => row![
                ctx.avatar(profile.avatar.as_deref(), &profile.name, 26.0, 13.0, 1.0),
                container(Space::new())
                    .width(Length::Fill)
                    .height(12)
                    .style({
                        let hover = theme.hover;
                        move |_| rounded(hover, 6)
                    }),
                ctx.look.glyph(Icon::Plus, 22, Color::WHITE),
            ]
            .spacing(8)
            .padding([0, 8])
            .align_y(Alignment::Center)
            .into(),
        };
    let nameplate = cell(ctx, nameplate_face, 42.0, None, shop());
    let avatar = cell(
        ctx,
        ctx.avatar(profile.avatar.as_deref(), &profile.name, 72.0, 36.0, 1.0),
        100.0,
        None,
        avatar_enabled.then(|| message(Event::Pick(ProfilePicture::Avatar))),
    );
    let decoration = cell(
        ctx,
        if profile.decoration.is_some() {
            crate::profile::body::profile_avatar(
                ctx,
                shown,
                64.0,
                false,
                Color::TRANSPARENT,
                Mode::Card,
            )
        } else {
            plus()
        },
        100.0,
        None,
        shop(),
    );
    let style = cell(
        ctx,
        crate::profile::body::profile_name(ctx, shown, 24.0, theme.text),
        42.0,
        None,
        look_enabled.then(|| message(Event::Style(crate::profile::name_style::Event::Open))),
    );
    // The accent, or with no banner or theme the avatar's colour, as
    // the card's banner and Slint's tile have it.
    let accent = crate::profile::banner_ground(ctx, shown).map(hex);
    let theme_fill = profile.theme.map(|[top, bottom]| (hex(top), hex(bottom)));
    let mut theme_cell = stack![cell(ctx, Space::new().into(), 100.0, theme_fill, None)];
    if look_enabled {
        // Without a theme, empty handles, as Slint's.
        let [top, bottom] = profile
            .theme
            .map_or([Color::TRANSPARENT; 2], |colours| colours.map(hex));
        theme_cell = theme_cell.push(
            container(
                column![
                    crate::profile::editor::colour::theme_handle(ctx, editor, core, Pop::Top, top),
                    Space::new().height(Length::Fill),
                    crate::profile::editor::colour::theme_handle(
                        ctx,
                        editor,
                        core,
                        Pop::Bottom,
                        bottom
                    ),
                ]
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(100)
            .align_x(Alignment::Center)
            .padding(4),
        );
    }
    let banner_still = profile
        .banner
        .as_deref()
        .and_then(|url| crate::profile::profile_picture(ctx, shown, url, 100, 100, true));
    let banner_face: Element<'a, Message> = match banner_still {
        Some(still) => Pic::new(still, Size::new(100.0, 100.0)).radius(6.0).into(),
        None if accent.is_none() => text("No banner").size(13).color(theme.muted).into(),
        None => Space::new().into(),
    };
    let banner = cell(
        ctx,
        banner_face,
        100.0,
        accent.map(|accent| (accent, accent)),
        banner_enabled.then(|| message(Event::Pick(ProfilePicture::Banner))),
    );
    let effect = cell(
        ctx,
        match crate::profile::cosmetic::effect_tile(ctx, shown, Size::new(TILE, 100.0)) {
            Some(tile) => tile,
            None if profile.effect.is_some() => text("Effect on").size(13).color(theme.text).into(),
            None => plus(),
        },
        100.0,
        None,
        shop(),
    );
    let frame = cell(ctx, plus(), 100.0, None, shop());
    let line = theme.line;
    let head = row![
        text("Main Profile")
            .size(16)
            .font(SEMIBOLD)
            .color(theme.text)
            .width(Length::Fill),
        ctx.look
            .icon_button(
                Icon::ChevronsRight,
                "Hide Styles",
                message(Event::Side(false))
            )
            .size(30.0, 16)
            .below(),
    ]
    .padding(Padding {
        left: 16.0,
        right: 8.0,
        ..Padding::ZERO
    })
    .height(50)
    .align_y(Alignment::Center);
    let list = column![
        group("Nameplate", nameplate),
        group(
            "Avatar & Decoration",
            row![avatar, decoration].spacing(8).into()
        ),
        group("Display Name Style", style),
        group("Theme & Banner", row![theme_cell, banner].spacing(8).into()),
        group(
            "Profile Effect & Frame",
            row![effect, frame].spacing(8).into()
        ),
    ]
    .spacing(16)
    .padding(16);
    let panel = theme.panel;
    container(column![
        head,
        container(Space::new())
            .width(Length::Fill)
            .height(1)
            .style(move |_| rounded(line, 0)),
        scroll_view(list)
            .height(Length::Fill)
            .style(scrollbar(theme)),
    ])
    .width(SIDE)
    .height(Length::Fill)
    .style(move |_| crate::style::fill(panel))
    .into()
}

/// A tile of the styles column: raised, lit under the pointer while it
/// does something, filled with a colour or a gradient's two.
pub(super) fn cell<'a>(
    ctx: crate::ctx::Ctx<'a>,
    content: Element<'a, Message>,
    height: f32,
    fill: Option<(Color, Color)>,
    pressed: Option<Message>,
) -> Element<'a, Message> {
    let (raised, hover) = (ctx.look.theme.raised, ctx.look.theme.hover);
    let enabled = pressed.is_some();
    button(container(content).center(Length::Fill).clip(true))
        .padding(0)
        .width(Length::Fill)
        .height(height)
        .on_press_maybe(pressed)
        .style(move |_, status| button::Style {
            background: Some(match fill {
                Some((top, bottom)) => {
                    iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                        .add_stop(0.0, top)
                        .add_stop(1.0, bottom)
                        .into()
                }
                None => if enabled && lights(status) {
                    hover
                } else {
                    raised
                }
                .into(),
            }),
            border: crate::style::round(6),
            ..Default::default()
        })
        .into()
}

/// Show Styles: a tab on the card's left edge once the column has gone.
pub(super) fn show_styles<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let edge = ctx.look.theme.edge;
    let face = button(ctx.look.glyph(Icon::Brush, 18, Color::WHITE).boxed(36))
        .padding(Padding {
            left: 7.0,
            ..Padding::ZERO
        })
        .width(42)
        .height(36)
        .on_press(message(Event::Side(true)))
        .style(move |_, _| button::Style {
            background: Some(Color::BLACK.into()),
            border: crate::style::edge(edge, 10.0),
            ..Default::default()
        });
    iced::widget::tooltip(
        face,
        controls::tip(ctx.look.theme, "Show Styles"),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
}
