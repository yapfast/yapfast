//! The corner picture in a window of its own, over the others, so a share is
//! watched beside Yapfast while the call is anywhere: the stream at the size
//! it comes in, in a window the app's own, its title and icon Yapfast's.

use iced::{
    Alignment, Background, Color, Element, Length, Padding, Size, Task, gradient,
    widget::{column, container, pin, row, stack, text},
    window,
};

use crate::{
    app::{App, Message},
    call::Event,
    pictures::Icon,
    tile::NOTE,
};

/// How wide the window opens, at the share's 16:9.
pub(crate) const WIDE: f32 = 960.0;

/// The smallest it may be dragged to, at the same shape.
const NARROW: f32 = 320.0;

/// How far the picture stands in from the window's edge with the members
/// under it.
pub(crate) const MARGIN: f32 = 16.0;

/// How far the window's buttons stand in from its edge, and between them.
pub(crate) const CORNER: f32 = 12.0;
/// The window's buttons' side and their glyph's: a size down from the call's
/// controls, so they read over a picture without covering it.
pub(crate) const BUTTON: f32 = 44.0;
pub(crate) const BUTTON_GLYPH: u32 = 19;
/// The picture's own full screen button, in the picture's corner: a couple of
/// pixels under the window's own, so it sits lightly on the picture.
pub(crate) const ON_PICTURE: f32 = 42.0;
const ON_PICTURE_GLYPH: u32 = 18;

/// A picture in a window of its own: the window, the key of what it shows,
/// and whether it stays over the others and over the whole screen. It is
/// Yapfast's window, its title and icon the app's (`App::title`).
pub struct Popout {
    pub id: window::Id,
    pub key: String,
    pub on_top: bool,
    pub full_screen: bool,
}

/// Opens `key`'s picture in a window of its own, over the others, or brings
/// that window to the front while it shows `key` already.
pub(super) fn open(app: &mut App, key: String) -> Task<Message> {
    if let Some(popout) = app.call.popout.as_mut() {
        if popout.key == key {
            let id = popout.id;
            return Task::batch([window::minimize(id, false), window::gain_focus(id)]);
        }
        // Another picture takes the window's place, the window as it is:
        // kept where it stands, over the others and as large as it was.
        popout.key = key;
        let id = popout.id;
        return window::gain_focus(id);
    }
    let (id, opened) = window::open(settings(app.window_icon()));
    app.call.popout = Some(Popout {
        id,
        key,
        on_top: true,
        full_screen: false,
    });
    // Made, it's marked with the app's icon, as Yapfast's own window is.
    opened.map(|id| Message::Shell(crate::shell::Event::Opened(id)))
}

/// The picture's window gone: no picture, and no menu that opened in it.
pub(crate) fn closed(app: &mut App) {
    let Some(popout) = app.call.popout.take() else {
        return;
    };
    app.call.zoom = crate::call::Zoom::default();
    if app
        .win
        .menu
        .as_ref()
        .is_some_and(|menu| menu.window.is_some_and(|(id, _)| id == popout.id))
    {
        app.win.menu = None;
    }
    // The call is Yapfast's own again, and its frames with it.
    crate::call::stage::place_videos(app);
}

/// The window kept over every other, or let under them.
pub(super) fn on_top(app: &mut App) -> Task<Message> {
    let Some(popout) = app.call.popout.as_mut() else {
        return Task::none();
    };
    popout.on_top = !popout.on_top;
    let level = if popout.on_top {
        window::Level::AlwaysOnTop
    } else {
        window::Level::Normal
    };
    window::set_level(popout.id, level)
}

/// The window over the whole screen, or a window at the share's shape again.
pub(super) fn full_screen(app: &mut App) -> Task<Message> {
    let Some(popout) = app.call.popout.as_mut() else {
        return Task::none();
    };
    popout.full_screen = !popout.full_screen;
    let mode = if popout.full_screen {
        window::Mode::Fullscreen
    } else {
        window::Mode::Windowed
    };
    window::set_mode(popout.id, mode)
}

/// The window as Yapfast makes it: the share's shape, over every other
/// window, closed by Yapfast so the picture's going is its own, and marked
/// with the app's own `icon`.
fn settings(icon: Option<window::Icon>) -> window::Settings {
    window::Settings {
        size: Size::new(WIDE, WIDE * 9.0 / 16.0),
        min_size: Some(Size::new(NARROW, NARROW * 9.0 / 16.0)),
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        icon,
        ..window::Settings::default()
    }
}

/// The window's view: the picture over the whole window, the call's controls
/// over its foot while the pointer is on it; or, with the members shown, the
/// picture standing in on the call's ground with the members' block under it,
/// sliding in and out as the chevron is pressed; a note once the share has
/// ended and its frames are gone.
pub fn view(app: &App) -> Element<'_, Message> {
    // The room it has, which a menu it opens is kept inside.
    iced::widget::responsive(move |size| {
        app.call.popout_room.set(Some(size));
        drawn(app)
    })
    .into()
}

/// What the window draws, in the room it has.
fn drawn(app: &App) -> Element<'_, Message> {
    let revealed = crate::call::popout_revealed(app);
    let ground = app.theme.stage;
    // The ground comes up as the block does: black while the picture is the
    // whole window, the call's own once it stands in.
    let floor = mix(Color::BLACK, ground, revealed);
    let corners = (revealed > 0.0).then(|| (crate::tile::RADIUS as u32, crate::theme::rgb(floor)));
    // Its room here is where its frames are made, and with the members shown
    // it carries the picture's own button at its corner; over the whole
    // window both buttons sit in the window's corner, the pin first.
    let shown = revealed > 0.0;
    let picture: Element<'_, Message> = iced::widget::responsive(move |size| {
        app.call.popout_seen.set(Some(size));
        demo(app, size, floor, corners, shown)
    })
    .into();
    let body: Element<'_, Message> = if revealed <= 0.0 {
        let foot = crate::call::stage::members_block(app, false);
        over(picture, with_corner(app, foot))
    } else {
        let block = crate::call::stage::members_block(app, true);
        let tallest = crate::call::stage::members_block_height(app, true);
        column![
            container(picture).padding(MARGIN * revealed),
            // The block rises from the window's foot, cut to what has come up:
            // its own height kept, so the foot stays put and the top slides.
            container(
                stack![
                    pin(container(block)
                        .padding(Padding {
                            left: MARGIN,
                            right: MARGIN,
                            ..Padding::ZERO
                        })
                        .height(Length::Fixed(tallest)))
                    .y(tallest * revealed - tallest)
                ]
                .width(Length::Fill)
                .height(Length::Fixed(tallest * revealed))
            )
            .width(Length::Fill)
            .height(Length::Fixed(tallest * revealed))
            .clip(true),
        ]
        .into()
    };
    // With the members shown the pin stands at the window's corner, the
    // picture's own button over the picture; over the whole window both are
    // under the pointer, as the controls are, and drawn there (`with_corner`).
    let mut layers = stack![
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(floor))
    ];
    if revealed > 0.0 {
        layers = layers.push(
            container(row![keep_on_top(app)].spacing(CORNER))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .padding(CORNER),
        );
    }
    // A tile's menu opens here, over the picture it was pressed on, and
    // nowhere in Yapfast's own window.
    if let Some(menu) = app.win.menu.as_ref()
        && menu
            .window
            .is_some_and(|(id, _)| Some(id) == app.call.popout.as_ref().map(|popout| popout.id))
        && let Some(menu) = crate::menus::view(menu, app.ctx())
    {
        layers = layers
            .push(crate::controls::outside(
                Message::Menu(crate::menus::Event::Close),
                false,
            ))
            .push(menu);
    }
    layers.width(Length::Fill).height(Length::Fill).into()
}

/// The button that puts the window over the whole screen, or a window again,
/// with the icon of what it does next, at `side` px across.
fn over_screen<'a>(app: &'a App, side: f32, glyph: u32) -> Element<'a, Message> {
    let (icon, label) = if app
        .call
        .popout
        .as_ref()
        .is_some_and(|popout| popout.full_screen)
    {
        (Icon::Minimize, "Exit full screen")
    } else {
        (Icon::Maximize, "Full screen")
    };
    window_button(app, icon, label, Event::PopoutFull, side, glyph)
}

/// The pin that keeps the window over every other, over it, or lets them
/// cover it.
fn keep_on_top<'a>(app: &'a App) -> Element<'a, Message> {
    let (icon, label) = if app.call.popout.as_ref().is_some_and(|popout| popout.on_top) {
        (Icon::Pin, "Keep over other windows")
    } else {
        (Icon::PinOff, "Let other windows over")
    };
    window_button(app, icon, label, Event::PopoutOnTop, BUTTON, BUTTON_GLYPH)
}

/// A window's own button, as the call's controls under it and a size down:
/// round, on the theme's raised ground, its icon in the theme's own ink.
fn window_button<'a>(
    app: &'a App,
    icon: Icon,
    label: &'a str,
    event: Event,
    side: f32,
    glyph: u32,
) -> Element<'a, Message> {
    let theme = &app.theme;
    crate::controls::round_button_of(
        app.look().glyph(icon, glyph, theme.text),
        label,
        theme.raised,
        side,
        theme,
        Some(Message::Call(event)),
    )
}

/// The members' block over the picture's foot with the window's buttons at
/// its corner, both under the pointer while the picture is the whole window.
fn with_corner<'a>(app: &'a App, block: Element<'a, Message>) -> Element<'a, Message> {
    let mut layers = stack![
        container(block)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End),
        container(row![keep_on_top(app), over_screen(app, BUTTON, BUTTON_GLYPH)].spacing(CORNER))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(CORNER)
    ];
    // The zoom bar says how close the picture has come, at the other corner
    // of the window, while it has come closer than its room.
    if let Some(bar) = crate::call::stage::zoom_bar(app) {
        layers = layers.push(
            container(bar)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Start)
                .align_y(Alignment::End)
                .padding(CORNER),
        );
    }
    layers.width(Length::Fill).height(Length::Fill).into()
}

/// The picture over the whole window, the members' chevron and the call's
/// controls over its foot while the pointer is on it, over a shade as the
/// corner card's own.
fn over<'a>(picture: Element<'a, Message>, foot: Element<'a, Message>) -> Element<'a, Message> {
    let shade = Background::Gradient(
        gradient::Linear::new(std::f32::consts::PI)
            .add_stop(0.58, Color::TRANSPARENT)
            .add_stop(1.0, Color::from_rgba(0.0, 0.0, 0.0, 0.6))
            .into(),
    );
    // Its foot flush with the window's, the controls' own inset from it.
    let foot: Element<'a, Message> = container(foot)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End)
        .into();
    crate::widgets::Hover::new(picture, foot)
        .shade(shade)
        .into()
}

/// What the window shows: the picture fitted in `ground`, its corners cut
/// round on the colour `corners` names, or the note once the share is gone;
/// with the members shown, the picture's own full screen button in the room
/// `size` leaves it, at its bottom right corner. It's the wheel's to bring
/// closer, about the pointer.
fn demo<'a>(
    app: &'a App,
    size: Size,
    ground: Color,
    corners: Option<(u32, u32)>,
    button: bool,
) -> Element<'a, Message> {
    use crate::call::stage::{fitted_in, frame_zoom, wheeled};

    let Some(popout) = app.call.popout.as_ref() else {
        return ended();
    };
    // Its size here, which the wheel turns it about.
    let fitted = fitted_in(app, &popout.key, size);
    app.call.zoom_seen.set(Some((size, fitted)));
    // The note once the share has ended, and the window's button still. Cut
    // at its room: brought closer the frame is drawn larger than that, and
    // no further than it.
    let picture: Element<'_, Message> = match frame_zoom(app, &popout.key, corners, ground) {
        Some(picture) => container(wheeled(picture))
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into(),
        None => ended(),
    };
    if !button {
        return picture;
    }
    // Where the picture itself stands in its room, the bars about it left
    // out: its corner is where its button goes, `CORNER` in from it.
    let left = ((size.width - fitted.width) / 2.0).max(0.0);
    let top = ((size.height - fitted.height) / 2.0).max(0.0);
    let mut layers = stack![
        picture,
        pin(over_screen(app, ON_PICTURE, ON_PICTURE_GLYPH))
            .x(left + fitted.width - CORNER - ON_PICTURE)
            .y(top + fitted.height - CORNER - ON_PICTURE)
    ];
    // How far the wheel has brought it closer, at the picture's own bottom
    // left corner, while it has come closer.
    if let Some(bar) = crate::call::stage::zoom_bar(app) {
        layers = layers.push(
            pin(bar)
                .x(left + CORNER)
                .y(top + fitted.height - CORNER - crate::call::stage::ZOOM_BAR),
        );
    }
    layers.width(Length::Fill).height(Length::Fill).into()
}

/// `to`'s share of `from`: the ground a picture stands on as its window's
/// members come up.
fn mix(from: Color, to: Color, t: f32) -> Color {
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

/// What the window says once the share has ended and its frames are gone.
fn ended<'a>() -> Element<'a, Message> {
    container(text("The share ended").size(14).color(NOTE))
        .center(Length::Fill)
        .into()
}
