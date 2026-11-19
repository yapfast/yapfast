//! A profile's card, drawn in each of its places: the banner with the
//! avatar over its edge and the status in a bubble beside it, the name in
//! its style, the username, pronouns and server tag; the full profile's
//! actions; the badges, activities, the mutual line, the bio, the dates,
//! the roles, connections and note; and the card's own message box and
//! buttons over the banner.

use std::sync::Arc;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size,
    advanced::graphics::text::font_system,
    widget::{Space, button, column, container, mouse_area, row, stack, text, text_input, tooltip},
};
use yapfast_core::chat::{self, Presence};

use super::{
    Event, MESSAGE_FIELD, Mode, NOTE_FIELD, Shown, StyledName, WIDTH, frame_style,
    look::{Inks, Offer, Tab},
};
use crate::{
    app::Message,
    controls::{self, Kind, action, lit_style, placed},
    fonts::{BOLD, MEDIUM, REGULAR, SEMIBOLD},
    looks::{self, Chip},
    markdown, paint,
    pictures::{Icon, Mark},
    style::{corners, faded, hex, rounded},
    theme::rgb,
    widgets::{Above, Fold, Pic},
};

/// The selection row the bio's pieces say they're in: none of the
/// timeline's.
const BIO_ROW: usize = usize::MAX - 2;

/// A badge's tooltip waits for the pointer to rest on it, as Discord's;
/// a levelled one's card is this wide, its picture this high, its edge
/// Discord's.
const BADGE_TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
const BADGE_TIP_WIDTH: f32 = 200.0;
const BADGE_TIP_PICTURE: f32 = 64.0;
const BADGE_TIP_EDGE: Color = Color {
    r: 0x97 as f32 / 255.0,
    g: 0x97 as f32 / 255.0,
    b: 0x9f as f32 / 255.0,
    a: 0x1f as f32 / 255.0,
};

/// A mode's banner, avatar and padding.
struct Metrics {
    banner: f32,
    avatar: f32,
    pad: f32,
    name: f32,
}

impl Mode {
    fn metrics(self) -> Metrics {
        match self {
            Mode::Card | Mode::Docked => Metrics {
                banner: 105.0,
                avatar: 80.0,
                pad: 16.0,
                name: 20.0,
            },
            Mode::Side => Metrics {
                banner: 140.0,
                avatar: 96.0,
                pad: 20.0,
                name: 26.0,
            },
        }
    }
}

/// A section's title on a card, as Discord writes it: small, bold, in the
/// card's ink.
fn title<'a>(words: &'a str, ink: Color) -> Element<'a, Message> {
    text(words).size(12).font(BOLD).color(ink).into()
}

/// A thought's bubble over a profile's top: two dots, each `(x, y,
/// side)`, rising from the avatar to `content` at `at`, in the bubble's
/// inks. A stack is the size of its first child, which lays out the rest:
/// the whole top, so the second dot and the bubble have room.
pub(crate) fn thought<'a>(
    inks: &Inks,
    dots: [(f32, f32, f32); 2],
    at: Point,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let (bubble, edge) = (inks.bubble, inks.bubble_edge);
    let dot = move |(x, y, side): (f32, f32, f32)| {
        placed(
            container(Space::new())
                .width(side)
                .height(side)
                .style(move |_| crate::style::outlined(bubble, edge, side / 2.0)),
            x,
            y,
        )
    };
    stack![
        Space::new().width(Length::Fill).height(Length::Fill),
        dot(dots[0]),
        dot(dots[1]),
        placed(content, at.x, at.y),
    ]
    .into()
}

/// A profile's card in `mode`; `pointed` plays its decoration.
/// `width` is the profile's across: the card's, the docked panel's
/// inside its inset, or the full profile's side column's, which grows
/// with the window.
pub(super) fn profile_body<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    mode: Mode,
    width: f32,
    pointed: bool,
) -> Element<'a, Message> {
    let look = &shown.look;
    let profile = look.profile();
    let inks = crate::profile::inks(ctx, look, mode);
    let metrics = mode.metrics();
    let theme = ctx.look.theme;
    let mut body = column![].spacing(12).padding([0.0, metrics.pad]);
    // Discord's panel beside a DM names a private profile in a bar above
    // the banner, and says why the rest is bare.
    let private = mode == Mode::Docked && profile.private && !look.own;
    // A plain name selects, to copy, as in Discord; a styled one is a
    // picture, which opens the full profile as the avatar does.
    let name = match styled_name(ctx, shown, metrics.name) {
        Some(name) if !mode.large() => mouse_area(name)
            .on_press(Message::Profile(Event::Enlarge(None, mode == Mode::Docked)))
            .interaction(iced::mouse::Interaction::Pointer)
            .into(),
        Some(name) => name,
        None => selectable(ctx, profile.name.as_str(), metrics.name, BOLD, inks.ink),
    };
    // Discord's card has the badges beside the username, up to six of
    // them (any number in the DM's panel); more go under the name, 2 px
    // below it, and the panel's own keep to their row below.
    let beside = !mode.large()
        && !profile.handle.is_empty()
        && !profile.badges.is_empty()
        && (mode == Mode::Docked || profile.badges.len() <= 6);
    let under = mode == Mode::Card && !ctx.state.preview && !beside && !profile.badges.is_empty();
    let mut names = column![name].spacing(2);
    if let Some(line) = handle_line(ctx, shown, &inks, mode, beside) {
        names = names.push(line);
    }
    if under {
        names = names.push(badge_rows(ctx, shown));
    }
    body = body.push(names);
    if mode.large()
        && let Some(actions) = profile_actions(ctx, shown, &inks)
    {
        body = body.push(actions);
    }
    if !profile.badges.is_empty() && !beside && !under {
        body = body.push(badge_rows(ctx, shown));
    }
    if !mode.large() && !look.activities.is_empty() {
        let mut activities = column![].spacing(12);
        for activity in &look.activities {
            let menu = (mode == Mode::Card && !look.bot && !ctx.state.preview).then_some(look.own);
            activities = activities.push(crate::profile::activity::activity_card_view(
                ctx, activity, &inks, menu,
            ));
        }
        body = body.push(activities);
    }
    if !mode.large() && !look.own && (!profile.friends.is_empty() || !profile.servers.is_empty()) {
        body = body.push(mutual_line(ctx, shown, &inks, mode));
    }
    if private {
        body = body.push(private_note(profile, &inks));
    }
    if let Some(bio) = &look.bio {
        body = body.push(bio_view(ctx, bio, &inks, mode));
    }
    if mode.full() && (profile.created.is_some() || profile.joined.is_some()) {
        // The DM's panel writes the account's date as plain text in the
        // card's ink, 7 px under its title.
        let panel = mode == Mode::Docked;
        body = body.push(
            column![
                title("Member Since", inks.ink),
                member_since(ctx, shown, &inks, panel)
            ]
            .spacing(if panel { 7 } else { 6 }),
        );
    }
    if mode.large() && !look.friends_since.is_empty() {
        body = body.push(
            column![
                title("Friends Since", inks.ink),
                text(look.friends_since.as_str()).size(14).color(inks.ink2),
            ]
            .spacing(6),
        );
    }
    if !profile.roles.is_empty() {
        let mut roles = column![].spacing(6);
        if mode.full() {
            roles = roles.push(title("Roles", inks.ink));
        }
        body = body.push(roles.push(role_chips(ctx, shown, &inks)));
    }
    if mode.large() && !profile.connections.is_empty() {
        body = body.push(connections(ctx, shown, &inks));
    }
    if mode.large() && look.note_editable {
        body = body.push(note_editor(ctx, &inks));
    }
    if !look.note.is_empty() {
        body = body.push(text(look.note.as_str()).size(13).color(inks.ink3));
    }
    if mode == Mode::Card && look.own {
        let pencil = ctx.look.icon(Icon::Pencil, 18, rgb(theme.on_accent));
        body = body.push(
            action(
                theme,
                "Edit Profile",
                Kind::Primary,
                Some(Message::Profile(Event::EditProfile)),
            )
            .icon(pencil)
            .wide(36.0),
        );
    }
    if mode != Mode::Side && !look.message_hint.is_empty() {
        body = body.push(message_box(ctx, shown, &inks));
    }
    let mut content = column![].padding(Padding {
        bottom: metrics.pad,
        ..Padding::ZERO
    });
    if private {
        content = content.push(private_bar(ctx, &inks));
    }
    let content = content
        .push(profile_top(ctx, shown, mode, width, pointed, &inks))
        .push(body);
    match mode {
        Mode::Card => container(content)
            .width(WIDTH)
            .style(frame_style(inks, theme.edge, 12.0, true))
            .into(),
        Mode::Side | Mode::Docked => content.into(),
    }
}

/// The bar above a private profile's banner, in the profile's colour
/// mostly dark: a lock and "Private Profile", in white.
fn private_bar<'a>(ctx: crate::ctx::Ctx<'a>, inks: &Inks) -> Element<'a, Message> {
    let ground = crate::theme::mix(inks.primary, Color::from_rgb8(0x1a, 0x1a, 0x1e), 0.15);
    container(
        row![
            ctx.look.glyph(Icon::Lock, 14, Color::WHITE),
            text("Private Profile")
                .size(14)
                .font(BOLD)
                .color(Color::WHITE)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(32)
    .style(move |_| container::Style {
        background: Some(ground.into()),
        ..Default::default()
    })
    .into()
}

/// A private profile's note, in a tile: why it shows so little.
fn private_note<'a>(profile: &chat::Profile, inks: &Inks) -> Element<'a, Message> {
    let who = if profile.handle.is_empty() {
        profile.name.as_str()
    } else {
        profile.handle.as_str()
    };
    let words = format!(
        "{who}'s profile is private, so some information is hidden. Add them as a friend to see more."
    );
    container(text(words).size(14).color(inks.ink))
        .padding(12)
        .width(Length::Fill)
        .style(|_| rounded(Color::from_rgba8(255, 255, 255, 20.0 / 255.0), 8))
        .into()
}

/// The banner, with the avatar over its lower edge on a ring of the
/// card's ground, the status's bubble beside it, and on the card the
/// friend and More buttons over its top right corner.
fn profile_top<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    mode: Mode,
    width: f32,
    pointed: bool,
    inks: &Inks,
) -> Element<'a, Message> {
    let look = &shown.look;
    let profile = look.profile();
    let metrics = mode.metrics();
    let (banner_height, avatar, pad) = (metrics.banner, metrics.avatar, metrics.pad);
    // The banner: its picture, playing while shown, else the accent
    // colour (the theme's first colour on a themed card), else the
    // raised grey.
    let banner_size = Size::new(width, banner_height);
    let top_corners = [12.0, 12.0, 0.0, 0.0];
    let moving = profile
        .banner
        .as_deref()
        .and_then(chat::moving_banner)
        .and_then(|url| ctx.moving(&url, banner_size, Rectangle::with_size(banner_size), true));
    let still = profile.banner.as_deref().and_then(|url| {
        crate::profile::profile_picture(ctx, shown, url, width as u32, banner_height as u32, true)
    });
    let banner_ground = crate::profile::banner_ground(ctx, shown)
        .map(hex)
        .or(inks.themed.then_some(inks.primary))
        .unwrap_or(ctx.look.theme.raised);
    let mut banner = stack![
        container(Space::new())
            .width(Length::Fill)
            .height(banner_height)
            .style(move |_| rounded(banner_ground, corners(top_corners)))
    ];
    if let Some(moving) = moving {
        banner = banner.push(moving.radius_each(top_corners).still(still));
    } else if let Some(still) = still {
        banner = banner.push(Pic::new(still, banner_size).radius_each(top_corners));
    }
    let face = mouse_area(profile_avatar(
        ctx,
        shown,
        avatar,
        pointed,
        inks.ground,
        mode,
    ))
    .interaction(if mode.large() {
        iced::mouse::Interaction::default()
    } else {
        iced::mouse::Interaction::Pointer
    });
    // The picture opens the full profile.
    let face = if mode.large() {
        face
    } else {
        face.on_press(Message::Profile(Event::Enlarge(None, mode == Mode::Docked)))
    };
    let mut top = stack![
        Space::new()
            .width(Length::Fill)
            .height(banner_height + avatar / 2.0 + 8.0),
        banner,
        container(Above::new(face, 10.0)).padding(Padding {
            top: banner_height - avatar / 2.0 - 6.0,
            left: pad - 6.0,
            ..Padding::ZERO
        }),
    ];
    if !look.thought.is_empty() {
        top = top.push(thought_bubble(ctx, shown, mode, width, inks));
    }
    if mode != Mode::Side && (look.offer != Offer::None || !profile.id.is_empty()) {
        top = top.push(
            container(banner_buttons(ctx, shown))
                .width(Length::Fill)
                .align_x(Alignment::End)
                .padding(10),
        );
    }
    top.into()
}

/// The avatar on its ring, with its decoration and presence.
pub(super) fn profile_avatar<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    size: f32,
    pointed: bool,
    ground: Color,
    mode: Mode,
) -> Element<'a, Message> {
    let profile = shown.look.profile();
    let ring = size + 12.0;
    let ring_size = Size::new(ring, ring);
    let mut avatar = stack![
        container(Space::new())
            .width(ring)
            .height(ring)
            .style(move |_| rounded(ground, ring / 2.0))
    ];
    let face: Element<'a, Message> = match profile.avatar.as_deref().and_then(|url| {
        crate::profile::profile_picture(ctx, shown, url, size as u32, size as u32, true)
    }) {
        Some(picture) => Pic::new(picture, ring_size)
            .at(Rectangle::new(Point::new(6.0, 6.0), Size::new(size, size)))
            .radius(size / 2.0)
            .into(),
        None => container(ctx.avatar(None, &profile.name, size, size / 2.0, 1.0))
            .padding(6)
            .into(),
    };
    avatar = avatar.push(face);
    if let Some(decoration) = ctx
        .state
        .cosmetics
        .decoration(profile.decoration.as_deref())
    {
        let side = (size * 1.2).round();
        let inset = 6.0 - (side - size) / 2.0;
        let at = Rectangle::new(Point::new(inset, inset), Size::new(side, side));
        let moving = pointed
            .then(|| ctx.moving(&looks::animated_url(decoration), ring_size, at, false))
            .flatten();
        if let Some(moving) = moving {
            avatar = avatar.push(moving);
        } else if let Some(still) =
            crate::profile::profile_picture(ctx, shown, decoration, side as u32, side as u32, false)
        {
            avatar = avatar.push(Pic::new(still, ring_size).at(at));
        }
    }
    if let Some(presence) = shown
        .look
        .presence
        .filter(|presence| *presence != Presence::Offline)
        && let Some(dot) = ctx.mark(Mark::Dot(presence, rgb(ground)))
    {
        // The phone stands 5 px taller than the dot.
        let tall = if presence == Presence::Mobile {
            5.0
        } else {
            0.0
        };
        let inset = if mode.large() { 14.0 } else { 10.0 } + 16.0;
        let at = Rectangle::new(
            Point::new(ring - inset, ring - inset - tall),
            Size::new(16.0, 16.0 + tall),
        );
        avatar = avatar.push(Pic::new(dot, ring_size).at(at));
    }
    avatar.width(ring).height(ring).into()
}

/// What they're up to, a thought beside the picture: two lines at most,
/// all of it while pointed at, as Discord's.
fn thought_bubble<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    mode: Mode,
    width: f32,
    inks: &Inks,
) -> Element<'a, Message> {
    let metrics = mode.metrics();
    let (banner, avatar, pad) = (metrics.banner, metrics.avatar, metrics.pad);
    let (bubble, edge) = (inks.bubble, inks.bubble_edge);
    let large = mode.large();
    let (dots, at, size, lines, radius, inset) = if large {
        let y = banner - avatar / 2.0;
        (
            [
                (pad + avatar + 14.0, y + 30.0, 10.0),
                (pad + avatar + 26.0, y + 37.0, 18.0),
            ],
            Point::new(pad + avatar + 18.0, y + 45.0),
            14.0,
            42.0,
            16.0,
            (13.0, 8.0),
        )
    } else {
        (
            [
                (pad + avatar + 2.0, banner - 16.0, 7.0),
                (pad + avatar + 11.0, banner - 9.0, 11.0),
            ],
            Point::new(pad + avatar + 12.0, banner + 4.0),
            13.0,
            38.0,
            14.0,
            (12.0, 7.0),
        )
    };
    let room = width - at.x - 12.0;
    let room = if large { room.min(180.0) } else { room };
    let status = &shown.look.thought;
    let mut words = row![].spacing(4);
    if let Some(emoji) = &status.emoji {
        let side = (size + 3.0_f32).round();
        if let Some(picture) = ctx.picture(emoji, side as u32, side as u32, false) {
            words = words.push(Pic::square(picture, side));
        }
    }
    words = words.push(text(status.words.as_str()).size(size).color(inks.ink));
    let whole = ctx.profiles.bubble;
    let mut bubble_box = container(words)
        .padding([inset.1, inset.0])
        .max_width(room)
        .clip(true)
        .style(move |_| crate::style::outlined(bubble, edge, radius));
    if !whole {
        bubble_box = bubble_box.max_height(lines + 2.0 * inset.1);
    }
    let bubble_box = mouse_area(bubble_box)
        .on_enter(Message::Profile(Event::Bubble(true)))
        .on_exit(Message::Profile(Event::Bubble(false)));
    thought(inks, dots, at, bubble_box.into())
}

/// The friend request and the person's menu, over the banner's top
/// right corner, as in Discord.
fn banner_buttons<'a>(ctx: crate::ctx::Ctx<'a>, shown: &'a Shown) -> Element<'a, Message> {
    let look = &shown.look;
    let id = look.profile().id.clone();
    let round = |icon: Icon, label: &'static str, message: Option<Message>| {
        let enabled = message.is_some();
        let rest = Color {
            a: 0.5,
            ..Color::BLACK
        };
        let lit = Color {
            a: 0.7,
            ..Color::BLACK
        };
        let glyph = ctx
            .look
            .glyph(icon, 18, Color::WHITE)
            .opacity(if enabled { 1.0 } else { 0.5 });
        let face = button(glyph.boxed(32))
            .padding(0)
            .width(32)
            .height(32)
            .on_press_maybe(message)
            .style(move |_, status| iced::widget::button::Style {
                background: Some(
                    if enabled && controls::lights(status) {
                        lit
                    } else {
                        rest
                    }
                    .into(),
                ),
                border: crate::style::round(16),
                ..Default::default()
            });
        Element::from(
            tooltip(
                face,
                controls::tip(ctx.look.theme, label),
                tooltip::Position::Top,
            )
            .gap(6),
        )
    };
    let mut buttons = row![].spacing(8);
    if let Some((icon, label, message)) = look.offer.button(&id) {
        buttons = buttons.push(round(icon, label, message));
    }
    if !id.is_empty() {
        buttons = buttons.push(round(
            Icon::Ellipsis,
            "More actions",
            Some(Message::Profile(Event::More(id))),
        ));
    }
    buttons.into()
}

/// The name, with its whole style drawn once while the profile shows,
/// or in plain bold.
pub(crate) fn profile_name<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &Shown,
    size: f32,
    ink: Color,
) -> Element<'a, Message> {
    styled_name(ctx, shown, size).unwrap_or_else(|| {
        text(shown.look.profile().name.clone())
            .size(size)
            .font(BOLD)
            .color(ink)
            .into()
    })
}

/// The name in its style, if it has one.
fn styled_name<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &Shown,
    size: f32,
) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    let profile = shown.look.profile();
    let style = state.cosmetics.style(profile.style.as_deref());
    let dark = ctx.look.theme.dark;
    let style = looks::name_look(style, &profile.name, false, dark)?;
    let key = (
        profile
            .style
            .as_ref()
            .map_or(0, |style| Arc::as_ptr(style) as *const () as usize),
        size as u32,
        font_system().read().expect("font system").version(),
        dark,
    );
    let mut cached = shown.name.borrow_mut();
    if cached
        .as_ref()
        .is_none_or(|made| made.name != profile.name || made.key != key)
    {
        *cached = Some(StyledName {
            name: profile.name.clone(),
            key,
            picture: paint::NamePicture::new(&style, &profile.name, size, 700),
        });
    }
    Some(cached.as_ref().expect("made above").picture.view())
}

/// Text on a profile that selects, to copy, as its plain name and its
/// username do in Discord. A press on it clears the timeline's
/// selection, so one thing is selected at a time.
fn selectable<'a>(
    ctx: crate::ctx::Ctx<'a>,
    words: impl Into<std::borrow::Cow<'a, str>>,
    size: f32,
    font: iced::Font,
    ink: Color,
) -> Element<'a, Message> {
    crate::widgets::Selectable::new(words, |copied| Message::Profile(Event::Copy(copied)))
        .size(size)
        .font(font)
        .color(ink)
        .highlight(ctx.look.theme.selection)
        .on_press(
            ctx.lanes
                .timeline
                .selection
                .is_some()
                .then_some(Message::SelectClear),
        )
        .into()
}

/// The username and pronouns, the badges beside them when they fit, and the
/// server tag's chip.
fn handle_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
    mode: Mode,
    badges: bool,
) -> Option<Element<'a, Message>> {
    let profile = shown.look.profile();
    let tag = ctx.state.cosmetics.tag(profile.tag.as_ref());
    if profile.handle.is_empty() && tag.is_none() {
        return None;
    }
    let mut line = row![].spacing(6).align_y(Alignment::Center);
    if !profile.handle.is_empty() {
        let words: std::borrow::Cow<'a, str> = if profile.pronouns.is_empty() {
            profile.handle.as_str().into()
        } else {
            format!("{} \u{2022} {}", profile.handle, profile.pronouns).into()
        };
        line = line.push(selectable(ctx, words, 14.0, REGULAR, inks.ink));
    }
    if badges {
        line = line.push(badge_rows(ctx, shown));
    }
    if let Some(tag) = tag {
        let fill = if inks.light {
            Color {
                a: 0.24,
                ..Color::WHITE
            }
        } else {
            crate::style::MEMBER_HOVER
        };
        let chip = ctx.tag_chip(
            &tag.text,
            tag.badge.as_deref(),
            Chip {
                size: 12.0,
                font: REGULAR,
                height: 24.0,
                pad: 8.0,
                radius: 8.0,
                fill,
                ink: if inks.themed {
                    inks.ink
                } else {
                    ctx.look.theme.text
                },
            },
            1.0,
        );
        line = line.push(crate::tag_card::opens(chip, tag));
    }
    // Discord's panel keeps the row 21 px high with or without badges.
    if mode == Mode::Docked {
        line = line.height(21);
    }
    Some(line.into())
}

/// The full profile's actions under the name: the friend button, the
/// message button and the menu, as Discord sets them; one's own has
/// Edit Profile in their place.
fn profile_actions<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
) -> Option<Element<'a, Message>> {
    let look = &shown.look;
    let theme = ctx.look.theme;
    if look.own {
        let pencil = ctx.look.icon(Icon::Pencil, 16, rgb(theme.on_accent));
        return Some(
            action(
                theme,
                "Edit Profile",
                Kind::Primary,
                Some(Message::Profile(Event::EditProfile)),
            )
            .icon(pencil)
            .small()
            .into(),
        );
    }
    let id = look.profile().id.clone();
    if look.offer == Offer::None && !look.can_message && id.is_empty() {
        return None;
    }
    let ink = inks.ink;
    let (rest, lit) = (Color { a: 0.12, ..ink }, Color { a: 0.2, ..ink });
    let mut row = row![].spacing(8).align_y(Alignment::Center);
    if let Some((icon, label, message)) = look.offer.button(&id) {
        let accept = look.offer == Offer::Accept;
        let (accent, on_accent) = (theme.accent, theme.on_accent);
        let face_ink = if accept { on_accent } else { ink };
        let enabled = message.is_some();
        row = row.push(
            button(
                container(
                    row![
                        ctx.look.glyph(icon, 16, face_ink),
                        text(label).size(14).font(MEDIUM).color(face_ink),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([0, 12])
                .height(32)
                .align_y(Alignment::Center),
            )
            .padding(0)
            .on_press_maybe(message)
            .style(move |_, status| iced::widget::button::Style {
                background: Some(
                    if accept {
                        accent
                    } else if enabled && controls::lights(status) {
                        lit
                    } else if enabled {
                        rest
                    } else {
                        faded(rest, 0.5)
                    }
                    .into(),
                ),
                border: crate::style::round(4),
                ..Default::default()
            }),
        );
    }
    let round = |icon: Icon, message: Message| {
        button(ctx.look.glyph(icon, 18, ink).boxed(32))
            .padding(0)
            .width(32)
            .height(32)
            .on_press(message)
            .style(lit_style(Some(rest), lit, 16.0))
    };
    if look.can_message {
        row = row.push(round(
            Icon::MessageCircle,
            Message::Profile(Event::Message(id.clone())),
        ));
    }
    if !id.is_empty() {
        row = row.push(round(Icon::Ellipsis, Message::Profile(Event::More(id))));
    }
    Some(row.into())
}

/// Someone's badges as Discord's profile sets them: twelve to a row,
/// 20 px icons 2 px apart, each row a pill of its own; a badge's name
/// shows over it, and a click opens its page when it has one.
pub(crate) fn badge_rows<'a>(ctx: crate::ctx::Ctx<'a>, shown: &'a Shown) -> Element<'a, Message> {
    let profile = shown.look.profile();
    let mut rows = column![].spacing(4);
    for chunk in profile.badges.chunks(12) {
        let mut pill = row![].spacing(2);
        for badge in chunk {
            let picture: Element<'a, Message> =
                match crate::profile::profile_picture(ctx, shown, &badge.icon, 20, 20, false) {
                    Some(picture) => Pic::square(picture, 20.0).into(),
                    None => Space::new().width(20).height(20).into(),
                };
            let face: Element<'a, Message> = match &badge.link {
                Some(link) => button(picture)
                    .padding(0)
                    .on_press(Message::Link(link.clone(), Point::ORIGIN))
                    .style(|_, _| iced::widget::button::Style::default())
                    .into(),
                None => picture,
            };
            // A badge that levels up has Discord's card for a tooltip,
            // after the pointer rests on it, as Discord's waits.
            let tip = match &badge.level {
                Some(level) => badge_level_tip(ctx, shown, level),
                None => controls::tip(ctx.look.theme, badge.description.as_str()),
            };
            pill = pill.push(
                tooltip(face, tip, tooltip::Position::Top)
                    .gap(6)
                    .delay(BADGE_TIP_DELAY),
            );
        }
        let (ground, edge) = (
            crate::style::MEMBER_HOVER,
            crate::style::faded(crate::style::MEMBER_HOVER, 0.33),
        );
        rows = rows.push(
            container(pill)
                .padding([1, 3])
                .height(24)
                .align_y(Alignment::Center)
                .style(move |_| crate::style::outlined(ground, edge, 8.0)),
        );
    }
    rows.into()
}

/// The tooltip of a badge that levels up, set by Discord's own values:
/// 200 px wide on the tooltip's ground, the level's picture 64 px high,
/// then the kind, the level in capitals and the progress.
fn badge_level_tip<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    level: &'a chat::ProfileBadgeLevel,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut card = column![].spacing(16).padding(Padding {
        left: 12.0,
        right: 12.0,
        top: 16.0,
        bottom: if level.progress.is_empty() {
            24.0
        } else {
            16.0
        },
    });
    // As wide as the picture's shape makes it at its height.
    let picture = ctx
        .state
        .pictures
        .get(&level.picture)
        .filter(|picture| picture.height > 0)
        .and_then(|picture| {
            let width = (BADGE_TIP_PICTURE * picture.width as f32 / picture.height as f32)
                .round()
                .max(1.0);
            let height = BADGE_TIP_PICTURE as u32;
            crate::profile::profile_picture(ctx, shown, &level.picture, width as u32, height, false)
                .map(|handle| Pic::new(handle, Size::new(width, BADGE_TIP_PICTURE)))
        });
    card = card.push(match picture {
        Some(picture) => container(picture)
            .center_x(Length::Fill)
            .height(BADGE_TIP_PICTURE),
        None => container(Space::new()).height(BADGE_TIP_PICTURE),
    });
    let line = |words: String, size: u32, font, ink| {
        text(words)
            .size(size)
            .font(font)
            .color(ink)
            .width(Length::Fill)
            .align_x(Alignment::Center)
    };
    let mut words = column![].spacing(4);
    if let Some(kind) = &level.kind {
        words = words.push(line(kind.clone(), 14, MEDIUM, theme.secondary));
    }
    words = words.push(line(level.title.to_uppercase(), 16, BOLD, theme.text));
    if !level.progress.is_empty() {
        words = words.push(line(level.progress.clone(), 14, MEDIUM, theme.secondary));
    }
    let ground = theme.tooltip;
    container(card.push(words))
        .width(BADGE_TIP_WIDTH)
        .style(move |_| crate::style::panel(ground, BADGE_TIP_EDGE, 8.0))
        .into()
}

/// What is shared, with a few of the faces: "5 Mutual Friends", a
/// bullet, "2 Mutual Servers", each opening the full profile on its tab.
fn mutual_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
    mode: Mode,
) -> Element<'a, Message> {
    let look = &shown.look;
    let profile = look.profile();
    let panel = mode == Mode::Docked;
    // Discord's counts are plain and muted, a step fainter in the DM's panel.
    let ink2 = if panel {
        faded(inks.ink3, 0.8)
    } else {
        inks.ink3
    };
    let mut line = row![].spacing(4).align_y(Alignment::Center);
    let link = |label: Element<'a, Message>, tab: Tab| {
        button(label)
            .padding(0)
            .on_press(Message::Profile(Event::Enlarge(Some(tab), panel)))
            .style(|_, _| iced::widget::button::Style::default())
    };
    if !profile.friends.is_empty() {
        let shown_faces = profile.friends.len().min(3);
        let ground = inks.ground;
        let mut faces = stack![
            Space::new()
                .width(shown_faces as f32 * 12.0 + 6.0)
                .height(18)
        ];
        for (index, friend) in profile.friends.iter().take(3).enumerate() {
            faces = faces.push(placed(
                container(ctx.avatar(friend.avatar.as_deref(), &friend.name, 16.0, 8.0, 1.0))
                    .padding(1)
                    .style(move |_| rounded(ground, 9)),
                index as f32 * 12.0,
                0.0,
            ));
        }
        line = line.push(link(
            row![
                faces,
                text(look.mutual_friends.as_str())
                    .size(12)
                    .font(REGULAR)
                    .color(ink2),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into(),
            Tab::Friends,
        ));
    }
    if !profile.friends.is_empty() && !profile.servers.is_empty() {
        line = line.push(text("\u{2022}").size(12).color(ink2));
    }
    if !profile.servers.is_empty() {
        let mut servers = row![].spacing(4).align_y(Alignment::Center);
        // The DM's panel shows the servers' icons first, when no friends
        // show theirs.
        if panel && profile.friends.is_empty() {
            let shown_icons = profile.servers.len().min(3);
            let mut icons = stack![
                Space::new()
                    .width(shown_icons as f32 * 12.0 + 6.0)
                    .height(16)
            ];
            for (index, server) in profile.servers.iter().take(3).enumerate() {
                icons = icons.push(placed(
                    ctx.avatar(server.icon.as_deref(), &server.name, 16.0, 4.0, 1.0),
                    index as f32 * 12.0,
                    0.0,
                ));
            }
            servers = servers.push(icons);
        }
        servers = servers.push(
            text(look.mutual_servers.as_str())
                .size(12)
                .font(REGULAR)
                .color(ink2),
        );
        line = line.push(link(servers.into(), Tab::Servers));
    }
    container(line).height(20).align_y(Alignment::Center).into()
}

/// The bio: three lines and "View full bio" on the card, whole
/// elsewhere.
pub(super) fn bio_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    bio: &'a markdown::Body,
    inks: &Inks,
    mode: Mode,
) -> Element<'a, Message> {
    let size: f32 = if mode.full() { 14.0 } else { 13.0 };
    let line_height = (size * 1.3).round();
    let shown = ctx.body_view(
        bio,
        BIO_ROW,
        0,
        markdown::Look {
            size,
            line_height,
            ink: inks.ink,
        },
    );
    if mode.full() {
        return shown;
    }
    // Cut to three lines as laid out, with the link under them only
    // when the whole bio is taller than the three lines.
    let three = line_height * 3.0;
    Fold::new(
        shown,
        button(text("View full bio").size(12).font(BOLD).color(inks.ink))
            .padding(0)
            .on_press(Message::Profile(Event::Enlarge(None, false)))
            .style(|_, _| iced::widget::button::Style::default()),
        true,
        (three + 2.0, three, 6.0),
    )
    .into()
}

/// Member Since's dates in one row, as Discord's: the Discord logo and
/// the account's date, then a bullet, the server's icon and the date of
/// joining it.
pub(super) fn member_since<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
    plain: bool,
) -> Element<'a, Message> {
    let profile = shown.look.profile();
    let ink = if plain { inks.ink } else { inks.ink2 };
    let mut line = row![].spacing(8).align_y(Alignment::Center);
    if let Some(created) = &profile.created {
        if !plain {
            line = line.push(ctx.look.glyph(Icon::Discord, 16, ink));
        }
        line = line.push(text(created.as_str()).size(14).color(ink));
    }
    if profile.created.is_some() && profile.joined.is_some() {
        line = line.push(text("\u{2022}").size(14).color(ink));
    }
    if let Some(joined) = &profile.joined {
        let icon: Element<'a, Message> = match profile
            .server_icon
            .as_deref()
            .and_then(|url| crate::profile::profile_picture(ctx, shown, url, 16, 16, true))
        {
            Some(picture) => Pic::square(picture, 16.0).radius(4.0).into(),
            None => {
                let raised = ctx.look.theme.raised;
                container(
                    text(crate::pictures::initials(&profile.server_name))
                        .size(8)
                        .font(BOLD)
                        .color(ink),
                )
                .center(16)
                .style(move |_| rounded(raised, 4))
                .into()
            }
        };
        line = line
            .push(icon)
            .push(text(joined.as_str()).size(14).color(ink));
    }
    line.into()
}

/// Their roles as chips: the role's colour in a dot, its name.
fn role_chips<'a>(ctx: crate::ctx::Ctx<'a>, shown: &'a Shown, inks: &Inks) -> Element<'a, Message> {
    let profile = shown.look.profile();
    let ink = inks.ink;
    let faint = ctx.look.theme.faint;
    let mut chips = row![].spacing(4);
    for role in &profile.roles {
        let dot = role.color.map_or(faint, hex);
        chips = chips.push(
            container(
                row![
                    container(Space::new())
                        .width(12)
                        .height(12)
                        .style(move |_| rounded(dot, 6)),
                    text(role.name.as_str()).size(12).font(SEMIBOLD).color(ink)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding(Padding {
                right: 8.0,
                left: 6.0,
                ..Padding::ZERO
            })
            .height(24)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                border: crate::style::edge(faded(ink, 0.2), 8.0),
                ..Default::default()
            }),
        );
    }
    chips.wrap().vertical_spacing(4).into()
}

/// Their accounts on other services: each one's name, verified, and
/// where it opens.
fn connections<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
) -> Element<'a, Message> {
    let profile = shown.look.profile();
    let mut list = column![title("Connections", inks.ink)].spacing(8);
    for connection in &profile.connections {
        let mut name = row![
            text(connection.name.as_str())
                .size(14)
                .font(MEDIUM)
                .color(inks.ink)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if connection.verified {
            name = name.push(ctx.look.glyph(Icon::Check, 14, inks.ink2));
        }
        if connection.url.is_some() {
            name = name.push(ctx.look.glyph(Icon::Link, 14, inks.ink2));
        }
        let mut words = column![name];
        if let Some(since) = &connection.since {
            words = words.push(
                text(format!("Member since {since}"))
                    .size(12)
                    .color(inks.ink3),
            );
        }
        let entry = row![ctx.look.glyph(Icon::Chain, 24, inks.ink2), words]
            .spacing(8)
            .align_y(Alignment::Center);
        list = list.push(match &connection.url {
            Some(url) => Element::from(
                button(entry)
                    .padding(0)
                    .on_press(Message::Link(url.clone(), Point::ORIGIN))
                    .style(|_, _| iced::widget::button::Style::default()),
            ),
            None => entry.into(),
        });
    }
    list.into()
}

/// The note this account keeps on them, saved on Enter or as the full
/// profile closes.
fn note_editor<'a>(ctx: crate::ctx::Ctx<'a>, inks: &Inks) -> Element<'a, Message> {
    let (ground, ink, hint, accent) = (inks.field, inks.ink, inks.ink3, ctx.look.theme.accent);
    let field = text_input("Click to add a note", &ctx.profiles.note)
        .id(iced::widget::Id::new(NOTE_FIELD))
        .size(14)
        .padding(8)
        .on_input(|text| Message::Profile(Event::Note(text)))
        .on_submit(Message::Profile(Event::SaveNote))
        .style(move |_, status| text_input::Style {
            background: ground.into(),
            border: Border {
                color: accent,
                width: if matches!(status, text_input::Status::Focused { .. }) {
                    1.0
                } else {
                    0.0
                },
                radius: 4.0.into(),
            },
            icon: ink,
            placeholder: hint,
            value: ink,
            selection: Color { a: 0.35, ..accent },
        });
    column![title("Note (only visible to you)", inks.ink), field]
        .spacing(6)
        .into()
}

/// The card's box that messages them in their DM, its smile at the end.
fn message_box<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    inks: &Inks,
) -> Element<'a, Message> {
    let (ground, ink, hint, accent) = (inks.message, inks.ink, inks.ink2, ctx.look.theme.accent);
    let field = text_input(&shown.look.message_hint, &ctx.profiles.field)
        .id(iced::widget::Id::new(MESSAGE_FIELD))
        .size(14)
        .padding(iced::Padding {
            top: 10.0,
            right: 36.0,
            bottom: 10.0,
            left: 12.0,
        })
        .on_input(|text| Message::Profile(Event::Field(text)))
        .on_submit(Message::Profile(Event::Send))
        .style(move |_, _| text_input::Style {
            background: ground.into(),
            border: crate::style::round(8),
            icon: ink,
            placeholder: hint,
            value: ink,
            selection: Color { a: 0.35, ..accent },
        });
    stack![
        field,
        container(ctx.look.glyph(Icon::Smile, 20, inks.ink2))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Center)
            .padding([0, 10]),
    ]
    .into()
}
