//! The call off screen and over it: the stream watched, this account's
//! own or someone's camera kept in a corner of the window while the call
//! isn't on screen (a click returns to the call, a drag moves it to the
//! nearest corner, and a drag at the corner of it away from the window's,
//! where the pointer says it resizes, makes it larger or smaller, its
//! shape kept); and the call's large tile over the whole window, full
//! screen.

use iced::{
    Alignment, Background, Color, Element, Length, Padding, Point, Rectangle, Size, gradient,
    widget::{Space, column, container, mouse_area, pin, stack, text},
};
use yapfast_core::voice::SharePhase;

use super::{
    Event,
    stage::{CORNER, OWN_SCREEN, Pip, share_status},
};
use crate::{
    app::{App, Message},
    controls,
    fonts::BOLD,
    header,
    pictures::Icon,
    tile::{self, GLASS, NOTE},
    viewer::Viewer,
    widgets::Hover,
};

/// From the window's edges, and the corner's own corners.
const MARGIN: f32 = 16.0;
const RADIUS: f32 = 8.0;

/// The widths its grip drags it between, as Discord's picture in picture
/// for a video (200 to 960 px, and 20 px past that while it's dragged), the
/// step it settles on when let go (40 px), the shape it keeps (a 16:9
/// picture, as [`CORNER`] is, 320 px wide as Discord's), and the square of
/// it its grip takes.
const LEAST: f32 = 200.0;
const MOST: f32 = 960.0;
const STEP: f32 = 40.0;
const SHAPE: f32 = CORNER.height / CORNER.width;
const GRIP: f32 = 22.0;

/// The picture's size: as its grip was dragged to, else its own, on the
/// window's own pixels. While it's dragged it follows the pointer within
/// the window's room; otherwise it's no wider than the room settles on, as
/// Discord's narrows when the window does.
pub fn corner_size(app: &App) -> Size {
    let wanted = app.layout.corner_width.unwrap_or(CORNER.width);
    let room = room(app);
    let width = if app.call.resize.is_some() {
        wanted.clamp(LEAST, (MOST + 20.0).min(room).max(LEAST))
    } else {
        wanted.min(settle(room))
    };
    let width = on_pixels(app, width);
    Size::new(width, on_pixels(app, width * SHAPE))
}

/// A width as Discord settles one when its grip is let go: to the nearest
/// 40 px, within 200 and 960.
fn settle(width: f32) -> f32 {
    ((width / STEP).round() * STEP).clamp(LEAST, MOST)
}

/// A length in the app's units as the window's pixels have it: the picture
/// keeps its edges on them, so its picture and the shade over it end on the
/// same row and column. A fraction of one left over showed, at the foot of
/// the shade, as a hairline of the picture lit a shade lighter than itself.
fn on_pixels(app: &App, length: f32) -> f32 {
    let pixels = app.device_scale();
    (length * pixels).round() / pixels
}

/// The widest the window has room for: right of the conversation, below
/// the header.
fn room(app: &App) -> f32 {
    let left = crate::conversation::chat_left(app) + MARGIN;
    let top = header::HEIGHT + MARGIN;
    (app.window.width - MARGIN - left).min((app.window.height - MARGIN - top) / SHAPE)
}

/// The corners the picture may sit in: its left, right, top and bottom
/// as the window has room.
fn corners(app: &App, size: Size) -> (f32, f32, f32, f32) {
    let left = crate::conversation::chat_left(app) + MARGIN;
    let right = app.window.width - size.width - MARGIN;
    let top = header::HEIGHT + MARGIN;
    let bottom = app.window.height - size.height - MARGIN;
    (left, right, top, bottom)
}

/// The picture let go after a drag: it goes to the nearest corner.
pub(super) fn drop_corner(app: &mut App) {
    let size = corner_size(app);
    let (left, right, top, bottom) = corners(app, size);
    let at = corner_at(app, size);
    app.call.left = at.x + size.width / 2.0 < (left + right + size.width) / 2.0;
    app.call.bottom = at.y + size.height / 2.0 >= (top + bottom + size.height) / 2.0;
    app.call.drag = None;
}

/// Where the picture is: in its corner, moved as far as it's dragged.
fn corner_at(app: &App, size: Size) -> Point {
    let (left, right, top, bottom) = corners(app, size);
    let x = if app.call.left { left } else { right };
    let y = if app.call.bottom { bottom } else { top };
    let by = app.call.drag.unwrap_or_default();
    Point::new(on_pixels(app, x + by.x), on_pixels(app, y + by.y))
}

/// The picture's inward corner, the one away from the window's, which a
/// drag resizes it from.
pub fn grip(app: &App) -> Rectangle {
    let size = corner_size(app);
    let at = corner_at(app, size);
    let x = if app.call.left {
        at.x + size.width - GRIP
    } else {
        at.x
    };
    let y = if app.call.bottom {
        at.y
    } else {
        at.y + size.height - GRIP
    };
    Rectangle::new(Point::new(x, y), Size::new(GRIP, GRIP))
}

/// Whether a press at `at` takes the picture's grip, to resize it rather
/// than move it.
pub(super) fn on_grip(app: &App, at: Point) -> bool {
    grip(app).contains(at)
}

/// The grip dragged to `at`: the picture keeps the window's corner and its
/// shape, and its width follows the pointer across, as Discord's does
/// (its grip resizes horizontally only): away from the window's edge it
/// grows, towards it it shrinks; the pointer going up or down does
/// nothing.
pub(super) fn resize_to(app: &mut App, at: Point) {
    let Some(resize) = app.call.resize else {
        return;
    };
    let across = if app.call.left {
        at.x - resize.from.x
    } else {
        resize.from.x - at.x
    };
    app.layout.corner_width = Some((resize.width + across).clamp(LEAST, MOST + 20.0));
}

/// The grip let go: the picture settles on the nearest 40 px within 200
/// and 960 and the window's room, as Discord's does; that width is kept
/// for the next window, and the frames the picture shows are made at it.
pub(super) fn let_go(app: &mut App) {
    let dragged = corner_size(app).width;
    app.call.resize = None;
    if app.layout.corner_width.is_some() {
        app.layout.corner_width = Some(settle(dragged).min(settle(room(app))));
    }
    crate::layout::store(app.client.root(), "corner_width", app.layout.corner_width);
    crate::call::stage::place_videos(app);
}

/// The picture in its corner, while the call is off screen.
pub fn corner_view(app: &App) -> Option<Element<'_, Message>> {
    let size = corner_size(app);
    let pip = crate::call::stage::corner(app);
    let voice = &app.client.state().voice;
    let (title, status, key, live, stop, viewers) = match pip {
        Pip::None => return None,
        Pip::Watched(index) => {
            let person = voice.participants.get(index)?;
            let watching = voice
                .watching
                .iter()
                .find(|watching| watching.user == person.id)?;
            (
                format!("{}'s screen", watching.name),
                crate::call::stage::watch_status(watching),
                crate::call::stage::stream_key(&watching.user),
                false,
                Some((
                    Icon::X,
                    "Stop watching",
                    Event::StopWatching(watching.user.clone()),
                )),
                crate::call::stage::stream_viewers(app, &watching.viewers, Some(&watching.user)),
            )
        }
        Pip::Shared => {
            let sharing = voice.sharing.as_ref()?;
            (
                "Your screen".to_owned(),
                share_status(voice, &app.i18n).to_owned(),
                OWN_SCREEN.to_owned(),
                sharing.phase == SharePhase::Live,
                Some((Icon::ScreenShareOff, "Stop sharing", Event::StopSharing)),
                crate::call::stage::stream_viewers(app, &sharing.viewers, None),
            )
        }
        Pip::Camera(index) => {
            let person = voice.participants.get(index)?;
            (
                person.user.clone(),
                String::new(),
                person.id.clone(),
                false,
                None,
                None,
            )
        }
    };
    let theme = &app.theme;
    // Round on the chat behind it, as a tile on the stage is on its ground:
    // the picture is drawn square to its corners, and would spill past them
    // where the card's own are cut round.
    let corners = Some((RADIUS as u32, crate::theme::rgb(theme.chat)));
    let content = crate::call::stage::frame(app, &key, corners).unwrap_or_else(|| {
        container(text(status).size(14).color(NOTE).align_x(Alignment::Center))
            .padding([0, 16])
            .center(Length::Fill)
            .into()
    });
    let content = if pip == Pip::Shared && live && !app.shell.focused {
        crate::call::stage::paused_share_preview(content, &app.i18n)
    } else {
        content
    };
    let mut base = stack![content].width(size.width).height(size.height);
    if live {
        let activity_themed = app
            .themes
            .content(&theme.id, app.layout.theme_content.contains(&theme.id))
            .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
        base = base.push(
            pin(controls::live_pill_in(theme, activity_themed, None))
                .x(8)
                .y(8),
        );
    }
    if let Some(viewers) = viewers {
        base = base.push(
            container(viewers)
                .padding(8)
                .width(Length::Fill)
                .align_x(Alignment::End),
        );
    }
    // Its title over a shade along its foot, and its stop, while the
    // pointer is on it and it isn't held.
    let shade = Background::Gradient(
        gradient::Linear::new(std::f32::consts::PI)
            .add_stop(0.55, Color::TRANSPARENT)
            .add_stop(1.0, tile::SHADE)
            .into(),
    );
    let mut over = stack![
        pin(container(
            text(title)
                .size(14)
                .font(BOLD)
                .color(Color::WHITE)
                .wrapping(text::Wrapping::None),
        )
        .width(size.width - 72.0)
        .height(24)
        .clip(true)
        .align_y(Alignment::Center))
        .x(12)
        .y(size.height - 34.0)
    ];
    over = over.width(size.width).height(size.height);
    if let Some((icon, label, event)) = stop {
        over = over.push(
            container(tile::glass_button(
                app.look(),
                icon,
                label,
                Message::Call(event),
            ))
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End),
        );
    }
    let dragging = app.call.drag.is_some() || app.call.resize.is_some();
    let face: Element<'_, Message> = if dragging {
        base.into()
    } else {
        Hover::new(base, over).shade(shade).into()
    };
    let edge = theme.edge;
    let framed = container(face)
        .width(size.width)
        .height(size.height)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Color::BLACK.into()),
            border: crate::style::edge(edge, RADIUS),
            ..Default::default()
        });
    let held = mouse_area(framed)
        .on_press(Message::Call(Event::Grab))
        .interaction(if dragging {
            iced::mouse::Interaction::Grabbing
        } else {
            iced::mouse::Interaction::Pointer
        });
    // Over its grip the pointer says it resizes it, along the diagonal that
    // grip is on; the whole picture says so while it's resized, so it keeps
    // saying it as the size follows the pointer.
    let diagonal = if app.call.left == app.call.bottom {
        iced::mouse::Interaction::ResizingDiagonallyUp
    } else {
        iced::mouse::Interaction::ResizingDiagonallyDown
    };
    let at = corner_at(app, size);
    let grip = grip(app);
    let over = if app.call.resize.is_some() {
        Rectangle::new(Point::ORIGIN, size)
    } else {
        Rectangle::new(Point::new(grip.x - at.x, grip.y - at.y), grip.size())
    };
    // The grip takes its press first: over the stop button (its bottom
    // right, the grip's when the picture sits top left) a press resizes,
    // as the pointer says, and doesn't stop the share.
    let held = crate::widgets::Grip::new(held, over, diagonal, Message::Call(Event::Grab));
    Some(
        stack![pin(held).x(at.x).y(at.y)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

/// The call's large tile over the whole window, in full screen, until
/// Escape, its button or a double click.
pub fn stage_full_view(app: &App) -> Option<Element<'_, Message>> {
    if !crate::call::stage::full_shown(app) {
        return None;
    }
    Some(
        Viewer::new(
            Color::BLACK,
            crate::call::stage::stage_view(app),
            app.window,
            Message::Nothing,
        )
        .view(app.look()),
    )
}

/// A message's video full screen: its name, Save and Exit full screen
/// over it, the video fitted between (a click plays or pauses it, a right
/// click opens its menu), its controls under it.
pub fn clip_viewer(app: &App) -> Option<Element<'_, Message>> {
    if !app.playback.clip_full {
        return None;
    }
    let video = app.client.state().video.as_ref()?;
    let size = crate::viewer::fitted(app.window, video.size);
    let key = format!("clip:{}/{}", video.item, video.index);
    let mut screen = stack![
        container(Space::new())
            .width(size.width)
            .height(size.height)
            .style(|_| crate::style::fill(Color::BLACK))
    ];
    if let Some(frame) = crate::call::stage::frame(app, &key, None) {
        screen = screen.push(frame);
    }
    if let Some(error) = &video.error {
        screen = screen.push(
            container(
                container(text(error.as_str()).size(13).color(Color::WHITE))
                    .padding([0, 10])
                    .height(28)
                    .align_y(Alignment::Center)
                    .style(|_| crate::style::rounded(crate::playback::CLIP_SHADE, 6)),
            )
            .center(Length::Fill),
        );
    } else if video.loading {
        screen = screen.push(
            container(
                container(
                    text(crate::i18n::tr!(app.i18n, "Loading..."))
                        .size(13)
                        .color(Color::WHITE),
                )
                .padding([0, 10])
                .height(28)
                .align_y(Alignment::Center)
                .style(|_| crate::style::rounded(GLASS, 14)),
            )
            .center(Length::Fill),
        );
    }
    let screen = mouse_area(screen.width(size.width).height(size.height))
        .on_press(Message::Clip(crate::playback::Clip::Toggle))
        .interaction(iced::mouse::Interaction::Pointer);
    // A right click offers what the message's menu does for a video.
    let (item, index) = (video.item.clone(), video.index);
    let screen = crate::widgets::RightPress::new(screen, move |_| {
        Message::Image(crate::image_viewer::Event::Menu(item.clone(), index, true))
    });
    let name = app
        .client
        .state()
        .message(&video.item)
        .and_then(|message| message.attachments.get(video.index))
        .map(|attachment| attachment.name.clone())
        .unwrap_or_default();
    let mut buttons: Vec<Element<'_, Message>> = Vec::new();
    if !app.client.state().preview {
        buttons.push(
            app.look()
                .face_button(
                    app.look().glyph(Icon::Download, 20, Color::WHITE),
                    32.0,
                    format!("Save {name}"),
                    Message::Image(crate::image_viewer::Event::Save(
                        video.item.clone(),
                        video.index,
                    )),
                )
                .below()
                .into(),
        );
    }
    buttons.push(
        app.look()
            .face_button(
                app.look().glyph(Icon::X, 20, Color::WHITE),
                32.0,
                "Exit full screen",
                Message::Clip(crate::playback::Clip::Full(false)),
            )
            .below()
            .into(),
    );
    let width = size.width.max(360.0);
    Some(
        Viewer::new(
            crate::viewer::GROUND,
            screen,
            Size::new(size.width, size.height),
            Message::Clip(crate::playback::Clip::Full(false)),
        )
        .title(name, buttons)
        .foot(
            column![crate::playback::clip_controls(
                app.ctx(),
                video,
                width,
                true
            )]
            .padding(Padding::ZERO),
        )
        .view(app.look()),
    )
}
