//! The editor's card: its preview's top with the banner and avatar, the
//! status's thought, the names, the bio, and this account's own actions
//! and Nitro's offer under it.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size,
    widget::{Id, Space, button, column, container, row, stack, text},
};
use yapfast_core::{
    application::{BIO_LIMIT, NAME_LIMIT, PRONOUNS_LIMIT, ProfileEditor, ProfileText},
    chat::{self, ProfilePicture},
};

use crate::profile::Mode;
use crate::{
    app::Message,
    controls::{Kind, action, lights, placed},
    field::{self, Field},
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    form,
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::{corners, faded, hex, rounded, scrollbar},
    widgets::{Hover, Pic},
};

use super::{
    AVATAR, BIO, CONNECTIONS, Editor, Event, NAME, PRONOUNS, Pop, SHOP, Text, heading, hover_edit,
    item, link_text, message,
};

/// The card's middle: the banner and avatar, then each text, then the
/// bar asking to save.
pub(super) fn editor_middle<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    inks: &crate::profile::look::Inks,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let main = core.main();
    let ready = !core.loading() && !core.saving;
    let pad = if main { 32.0 } else { 24.0 };
    let mut body = column![]
        .spacing(if main { 28 } else { 16 })
        .padding(Padding {
            left: pad,
            right: if main { 8.0 } else { pad },
            bottom: pad,
            top: 0.0,
        });
    if editor.colour_open {
        body = body.push(crate::profile::editor::colour::banner_colour(ctx, core));
    }
    body = body.push(editor_names(ctx, editor, core, inks, ready));
    if main {
        body = body.push(own_actions(ctx, editor));
    }
    if !main && !core.nitro() {
        body = body.push(nitro_offer(ctx, core));
    }
    body = body.push(editor_bio(ctx, editor, core, inks, ready));
    let profile = editor.shown.look.profile();
    if profile.created.is_some() || profile.joined.is_some() {
        body = body.push(
            column![
                heading(main, "Member Since", inks),
                crate::profile::body::member_since(ctx, &editor.shown, inks, false)
            ]
            .spacing(6),
        );
    }
    if main {
        body = body.push(
            column![
                heading(main, "Connections", inks),
                link_text(
                    "+  Add Connection",
                    inks.ink2,
                    Message::Link(CONNECTIONS.into(), Point::ORIGIN)
                ),
            ]
            .spacing(6),
        );
        body = body.push(
            column![
                heading(main, "Note (only visible to you)", inks),
                hover_edit(
                    text("Click to add a note")
                        .size(15)
                        .color(inks.ink3)
                        .font(iced::Font {
                            style: iced::font::Style::Italic,
                            ..iced::Font::default()
                        }),
                    true,
                    message(Event::Note),
                )
            ]
            .spacing(6),
        );
    }
    if let Some(error) = &core.error {
        body = body.push(form::error(theme, error.as_str()));
    }
    let scroll = scroll_view(column![editor_top(ctx, editor, core, inks, pad), body])
        .height(Length::Fill)
        .style(scrollbar(theme));
    // A click on the card that nothing on it takes ends the editing and
    // closes the banner colour's box.
    let scroll: Element<'a, Message> = if editor.editing.is_some() || editor.colour_open {
        iced::widget::mouse_area(scroll)
            .on_press(message(Event::Edit(None)))
            .into()
    } else {
        scroll.into()
    };
    let mut middle = column![scroll];
    if core.change().is_some() || core.saving {
        let may_save =
            ready && core.reading.is_none() && core.too_long().is_none() && core.accent().is_ok();
        middle = middle.push(
            form::Unsaved {
                pad,
                radius: 0.0,
                saving: core.saving,
                may_reset: ready,
                may_save,
                shake: form::Shake::default(),
                undo: message(Event::Undo),
                save: message(Event::Save),
            }
            .view(theme),
        );
    }
    middle.into()
}

/// The banner with the avatar hanging over its lower edge, the status's
/// thought beside it, and on a server's what it edits and its close.
pub(super) fn editor_top<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    inks: &crate::profile::look::Inks,
    pad: f32,
) -> Element<'a, Message> {
    let main = core.main();
    let ready = !core.loading() && !core.saving;
    let shown = &editor.shown;
    let profile = shown.look.profile();
    let banner_height = if main { 193.0 } else { 210.0 };
    let reading = core.reading.as_ref().map(|(kind, _)| *kind);
    let banner_editable = ready
        && (core.may_picture(ProfilePicture::Banner) || core.may_write(ProfileText::Colour))
        && reading.is_none();
    let banner_locked = (!main || core.nitro()) && !core.may_picture(ProfilePicture::Banner);
    // The banner: the picture (moving ones play), else its colour, else
    // with no theme the avatar's, as the card's, else the card's own.
    let ground = crate::profile::banner_ground(ctx, shown)
        .map(hex)
        .unwrap_or(if main {
            Color::TRANSPARENT
        } else {
            ctx.look.theme.raised
        });
    let top_corners = [12.0, 12.0, 0.0, 0.0];
    let mut banner = stack![
        container(Space::new())
            .width(Length::Fill)
            .height(banner_height)
            .style(move |_| rounded(ground, corners(if main { [0.0; 4] } else { top_corners })))
    ];
    // As wide as the middle, cover-fitted.
    let width = crate::profile::editor::editor_middle_width(ctx, editor, main).round();
    let banner_size = Size::new(width, banner_height);
    let still = profile.banner.as_deref().and_then(|url| {
        crate::profile::profile_picture(ctx, shown, url, width as u32, banner_height as u32, true)
    });
    let moving = profile
        .banner
        .as_deref()
        .and_then(chat::moving_banner)
        .and_then(|url| ctx.moving(&url, banner_size, Rectangle::with_size(banner_size), true));
    if let Some(moving) = moving {
        banner = banner.push(moving.still(still));
    } else if let Some(still) = still {
        banner = banner.push(Pic::new(still, banner_size));
    }
    if reading == Some(ProfilePicture::Banner) {
        banner = banner.push(
            container(text("Reading...").size(14).font(BOLD).color(Color::WHITE))
                .center(Length::Fill)
                .height(banner_height),
        );
    }
    let banner: Element<'a, Message> = if banner_editable || banner_locked {
        let over = stack![
            container(Space::new())
                .width(Length::Fill)
                .height(banner_height)
                .style(|_| rounded(
                    Color {
                        a: 0.35,
                        ..Color::BLACK
                    },
                    0
                )),
            container(crate::profile::editor::menus::edit_badge(
                ctx,
                banner_locked
            ))
            .width(Length::Fill)
            .align_x(Alignment::End)
            .padding(Padding {
                top: 18.0,
                right: 60.0,
                ..Padding::ZERO
            }),
        ];
        let face = button(Hover::new(banner, over))
            .padding(0)
            .on_press_maybe(
                banner_editable.then(|| message(Event::Picture(ProfilePicture::Banner))),
            )
            .style(|_, _| button::Style::default());
        crate::profile::editor::menus::picture_menu(
            ctx,
            editor,
            core,
            ProfilePicture::Banner,
            face.into(),
        )
    } else {
        banner.into()
    };
    let avatar = editor_avatar(ctx, editor, core, inks, ready);
    let mut top = stack![
        Space::new()
            .width(Length::Fill)
            .height(banner_height + AVATAR / 2.0 + 8.0),
        banner,
        placed(avatar, pad - 6.0, banner_height - AVATAR / 2.0 - 6.0),
    ];
    if main {
        top = top.push(status_thought(
            ctx,
            inks,
            pad - 6.0 + AVATAR + 12.0,
            banner_height - AVATAR / 2.0 - 6.0,
        ));
    } else {
        let detail = format!("Your profile in {}", core.server_name);
        top = top.push(placed(
            container(text(detail).size(13).font(SEMIBOLD).color(Color::WHITE))
                .padding([0, 10])
                .height(28)
                .align_y(Alignment::Center)
                .style(|_| {
                    rounded(
                        Color {
                            a: 0.7,
                            ..Color::BLACK
                        },
                        14,
                    )
                }),
            16.0,
            16.0,
        ));
        let close = button(ctx.look.glyph(Icon::X, 18, Color::WHITE).boxed(36))
            .padding(0)
            .width(36)
            .height(36)
            .on_press(message(Event::Close))
            .style(|_, status| button::Style {
                background: Some(
                    Color {
                        a: if lights(status) { 0.85 } else { 0.7 },
                        ..Color::BLACK
                    }
                    .into(),
                ),
                border: crate::style::round(18),
                ..Default::default()
            });
        top = top.push(
            container(close)
                .width(Length::Fill)
                .align_x(Alignment::End)
                .padding(16),
        );
    }
    top.into()
}

/// The avatar on its ring, the pencil (or a lock) over it while pointed
/// at, and its menu.
pub(super) fn editor_avatar<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    inks: &crate::profile::look::Inks,
    ready: bool,
) -> Element<'a, Message> {
    let reading = core.reading.as_ref().map(|(kind, _)| *kind) == Some(ProfilePicture::Avatar);
    let enabled = core.may_picture(ProfilePicture::Avatar);
    let editable = ready && enabled && core.reading.is_none();
    let face = crate::profile::body::profile_avatar(
        ctx,
        &editor.shown,
        AVATAR,
        true,
        inks.ground,
        Mode::Side,
    );
    let ring = AVATAR + 12.0;
    let disc = move |alpha: f32| {
        container(Space::new())
            .width(AVATAR)
            .height(AVATAR)
            .style(move |_| {
                rounded(
                    Color {
                        a: alpha,
                        ..Color::BLACK
                    },
                    AVATAR / 2.0,
                )
            })
    };
    let face: Element<'a, Message> = if reading {
        stack![
            face,
            container(stack![
                disc(0.6),
                container(text("Reading...").size(13).font(BOLD).color(Color::WHITE))
                    .center(AVATAR)
            ])
            .padding(6)
        ]
        .into()
    } else if ready {
        let over = container(stack![
            disc(0.45),
            container(crate::profile::editor::menus::edit_badge(ctx, !enabled)).center(AVATAR)
        ])
        .padding(6);
        Hover::new(face, over).into()
    } else {
        face
    };
    let face = button(face)
        .padding(0)
        .width(ring)
        .height(ring)
        .on_press_maybe(editable.then(|| message(Event::Picture(ProfilePicture::Avatar))))
        .style(|_, _| button::Style::default());
    crate::profile::editor::menus::picture_menu(
        ctx,
        editor,
        core,
        ProfilePicture::Avatar,
        face.into(),
    )
}

/// The custom status as a thought beside the avatar, or the offer to set
/// one; a click opens its dialog.
pub(super) fn status_thought<'a>(
    ctx: crate::ctx::Ctx<'a>,
    inks: &crate::profile::look::Inks,
    x: f32,
    y: f32,
) -> Element<'a, Message> {
    let (bubble, edge) = (inks.bubble, inks.bubble_edge);
    let thought = &ctx
        .profiles
        .editor
        .get()
        .map(|editor| &editor.shown.look.thought);
    let set = thought.is_some_and(|thought| !thought.is_empty());
    let mut words = row![].spacing(4).align_y(Alignment::Center);
    if let Some(thought) = thought.filter(|_| set) {
        if let Some(emoji) = &thought.emoji
            && let Some(picture) = ctx.picture(emoji, 17, 17, false)
        {
            words = words.push(Pic::square(picture, 17.0));
        }
        words = words.push(text(thought.words.as_str()).size(14).color(inks.ink));
    } else {
        words = words.push(text("Set a custom status").size(14).color(inks.ink2));
    }
    let face = button(words)
        .padding([9, 13])
        .on_press(message(Event::Status))
        .style(move |_, _| button::Style {
            background: Some(bubble.into()),
            border: crate::style::edge(edge, 16.0),
            ..Default::default()
        });
    crate::profile::body::thought(
        inks,
        [(x + 2.0, y + 38.0, 10.0), (x + 16.0, y + 45.0, 20.0)],
        Point::new(x + 6.0, y + 53.0),
        container(face).max_width(180.0).into(),
    )
}

/// The name, then the username with the pronouns and the server tag,
/// the badges, and what's too long.
pub(super) fn editor_names<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    inks: &crate::profile::look::Inks,
    ready: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let main = core.main();
    let shown = &editor.shown;
    let profile = shown.look.profile();
    let mut names = column![].spacing(2);
    if editor.editing == Some(Text::Name) {
        let placeholder = profile.handle.as_str();
        names = names.push(
            form::field(theme, NAME, placeholder, &core.name)
                .size(22)
                .padding(8)
                .on_input(|text| message(Event::Write(ProfileText::Name, text)))
                .on_submit(message(Event::Edit(None))),
        );
    } else {
        let name: Element<'a, Message> = if core.loading() {
            text("Loading...")
                .size(if main { 24 } else { 26 })
                .font(BOLD)
                .color(inks.ink)
                .into()
        } else {
            crate::profile::body::profile_name(ctx, shown, if main { 24.0 } else { 26.0 }, inks.ink)
        };
        let pencil = ctx.look.glyph(Icon::Pen, 15, inks.ink3);
        let enabled = ready && core.may_write(ProfileText::Name);
        names = names.push(hover_edit(
            row![name, Hover::new(Space::new().width(15), pencil)]
                .spacing(12)
                .align_y(Alignment::Center),
            enabled,
            message(Event::Edit(Some(Text::Name))),
        ));
    }
    let mut line = row![
        text(format!("{} \u{b7}", profile.handle))
            .size(14)
            .font(MEDIUM)
            .color(inks.ink2)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let fallback = core
        .profile
        .as_ref()
        .map_or("", |profile| profile.fallback.pronouns.as_str());
    if editor.editing == Some(Text::Pronouns) {
        let placeholder = if fallback.is_empty() {
            "Add your pronouns"
        } else {
            fallback
        };
        line = line.push(
            form::field(theme, PRONOUNS, placeholder, &core.pronouns)
                .size(14)
                .padding(6)
                .width(220)
                .on_input(|text| message(Event::Write(ProfileText::Pronouns, text)))
                .on_submit(message(Event::Edit(None))),
        );
    } else {
        let (words, ink) = if !core.pronouns.is_empty() {
            (core.pronouns.as_str(), inks.ink2)
        } else if !fallback.is_empty() {
            (fallback, inks.ink2)
        } else {
            ("Add pronouns", inks.ink3)
        };
        line = line.push(hover_edit(
            text(words).size(14).font(MEDIUM).color(ink),
            ready,
            message(Event::Edit(Some(Text::Pronouns))),
        ));
        if main {
            line = line.push(crate::profile::editor::menus::tag_picker(ctx, editor, inks));
        }
    }
    names = names.push(line);
    if main && editor.editing != Some(Text::Pronouns) && !profile.badges.is_empty() {
        names = names.push(
            container(crate::profile::body::badge_rows(ctx, shown)).padding(Padding {
                top: 10.0,
                ..Padding::ZERO
            }),
        );
    }
    // The core counts the limits; the bio's says so under the bio.
    let too_long = match core.too_long() {
        Some(ProfileText::Name) if main => Some(format!(
            "A display name can be up to {NAME_LIMIT} characters."
        )),
        Some(ProfileText::Name) => {
            Some(format!("A nickname can be up to {NAME_LIMIT} characters."))
        }
        Some(ProfileText::Pronouns) => Some(format!(
            "Pronouns can be up to {PRONOUNS_LIMIT} characters."
        )),
        _ => None,
    };
    if let Some(words) = too_long {
        names = names.push(text(words).size(13).color(theme.danger));
    }
    names.into()
}

/// What others would press on one's own profile: Message can't be sent
/// to oneself; the shop; More with Copy User ID.
pub(super) fn own_actions<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let square = |icon: Icon, pressed: Message| {
        let (ground, lit) = (theme.raised, theme.hover);
        button(ctx.look.glyph(icon, 18, theme.text).boxed(34))
            .padding(0)
            .width(34)
            .height(34)
            .on_press(pressed)
            .style(crate::controls::lit_style(Some(ground), lit, 8.0))
    };
    let more = (editor.pop == Some(Pop::More)).then(|| {
        crate::profile::editor::menus::menu_list(
            ctx,
            vec![(
                item("Copy User ID".into(), Some(Icon::Copy), false),
                message(Event::CopyId),
            )],
        )
    });
    let more = Popover::new(
        square(Icon::Ellipsis, message(Event::Pop(Some(Pop::More)))),
        more,
        Side::Below,
    )
    .width(Width::Own(170.0))
    .gap(4.0)
    .outside(message(Event::Pop(None)));
    row![
        action(theme, "Message", Kind::Plain, None),
        square(Icon::Shop, Message::Link(SHOP.into(), Point::ORIGIN)),
        more,
    ]
    .spacing(8)
    .into()
}

/// Discord's offer, over what Nitro unlocks in a server.
pub(super) fn nitro_offer<'a>(
    ctx: crate::ctx::Ctx<'a>,
    core: &'a ProfileEditor,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let sunken = theme.sunken;
    container(
        row![
            ctx.look.glyph(Icon::Lock, 20, theme.accent),
            column![
                text("Make unique profiles for each server")
                    .size(14)
                    .font(BOLD)
                    .color(theme.text),
                form::note(
                    theme,
                    "Customize your profile for every server, from avatar decorations to bios. Only with Nitro."
                ),
            ]
            .spacing(2)
            .width(Length::Fill),
            action(
                theme,
                if core.upgrade() {
                    "Upgrade Nitro"
                } else {
                    "Unlock with Nitro"
                },
                Kind::Primary,
                Some(message(Event::Nitro)),
            )
            .small(),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(12)
    .style(move |_| rounded(sunken, 8))
    .into()
}

/// The bio, as it shows or in its field, with what's left of Discord's
/// limit.
pub(super) fn editor_bio<'a>(
    ctx: crate::ctx::Ctx<'a>,
    editor: &'a Editor,
    core: &'a ProfileEditor,
    inks: &crate::profile::look::Inks,
    ready: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let main = core.main();
    let enabled = core.may_write(ProfileText::Bio);
    let placeholder = if main {
        "Tell everyone a bit about yourself"
    } else {
        "Tell this server a bit about yourself"
    };
    let mut head = row![heading(main, if main { "Bio" } else { "ABOUT ME" }, inks)]
        .spacing(6)
        .align_y(Alignment::Center);
    if !enabled && !core.loading() {
        head = head.push(ctx.look.glyph(Icon::Lock, 13, inks.ink3));
    }
    let left = BIO_LIMIT as i64 - core.bio.chars().count() as i64;
    let count = if left < 0 {
        text(format!("Too long by {}", -left))
            .size(12)
            .color(theme.danger)
    } else {
        text(format!("{left} left")).size(12).color(inks.ink3)
    };
    let mut bio = column![head].spacing(6);
    if editor.editing == Some(Text::Bio) {
        let colors = field::Colors {
            ground: theme.sunken,
            text: theme.text,
            placeholder: theme.muted,
            selection: theme.selection,
        };
        let accent = theme.accent;
        let sunken = theme.sunken;
        let field = Field::new(
            &editor.bio,
            Id::new(BIO),
            placeholder.to_owned(),
            |event| message(Event::Bio(event)),
            colors,
        )
        .size(14.0)
        .lines(5.0)
        // A plain text box: Enter breaks the line, and a click elsewhere
        // or Escape ends the editing.
        .breaks()
        .padding(Padding::from([8.0, 10.0]));
        bio = bio.push(
            container(field)
                .height(110)
                .style(move |_| crate::style::outlined(sunken, faded(accent, 0.8), 6)),
        );
        bio = bio.push(
            row![
                text("You can use markdown and links if you'd like.")
                    .size(12)
                    .color(inks.ink3)
                    .width(Length::Fill),
                count
            ]
            .spacing(8),
        );
    } else {
        let shown: Element<'a, Message> = match &editor.shown.look.bio {
            Some(body) => crate::profile::body::bio_view(ctx, body, inks, Mode::Side),
            None => text(placeholder).size(14).color(inks.ink3).into(),
        };
        bio = bio.push(hover_edit(
            container(shown).width(if main {
                Length::Fixed(281.0)
            } else {
                Length::Fill
            }),
            ready && enabled,
            message(Event::Edit(Some(Text::Bio))),
        ));
        if left < 0 {
            bio = bio.push(count);
        }
    }
    bio.into()
}
