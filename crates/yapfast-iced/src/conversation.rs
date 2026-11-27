//! The conversation column: what takes the timeline's place (the friends
//! screens, browsing, the forum, a call) and what stands beside it
//! (search, the thread, the member list, the profile panel).

use iced::{
    Alignment, Element, Length,
    widget::{column, container, row},
};

use crate::{
    app::{App, Message},
    controls::bar,
};
use yapfast_core::themes::layout::{ComposerSide, MembersSide, RailSide};

pub fn themed_navigation(
    layout: &crate::layout::Layout,
    theme: &crate::theme::Theme,
    package: Option<&yapfast_core::themes::layout::Layout>,
    chrome: Option<&yapfast_core::themes::chrome::Chrome>,
) -> f32 {
    let rail = if matches!(
        package.and_then(|layout| layout.rail),
        Some(RailSide::Hidden | RailSide::Top | RailSide::Tree)
    ) {
        0.0
    } else {
        crate::rail::resolved_width(package, chrome)
    };
    rail + crate::sidebar::width(layout, theme)
}

/// Whether a theme puts the server rail on the window's right edge. The
/// sidebar stays on the left, as in the Slint build layout.
pub fn navigation_right(package: Option<&yapfast_core::themes::layout::Layout>) -> bool {
    package.and_then(|layout| layout.rail) == Some(RailSide::Right)
}

/// The width of a rail on the right edge, else 0.
pub fn right_rail(
    package: Option<&yapfast_core::themes::layout::Layout>,
    chrome: Option<&yapfast_core::themes::chrome::Chrome>,
) -> f32 {
    if navigation_right(package) {
        crate::rail::resolved_width(package, chrome)
    } else {
        0.0
    }
}

/// The left of the sidebar: right of a rail on the left.
pub fn sidebar_left(
    package: Option<&yapfast_core::themes::layout::Layout>,
    chrome: Option<&yapfast_core::themes::chrome::Chrome>,
) -> f32 {
    themed_navigation_rail(package, chrome) - right_rail(package, chrome)
}

fn themed_navigation_rail(
    package: Option<&yapfast_core::themes::layout::Layout>,
    chrome: Option<&yapfast_core::themes::chrome::Chrome>,
) -> f32 {
    if matches!(
        package.and_then(|layout| layout.rail),
        Some(RailSide::Hidden | RailSide::Top | RailSide::Tree)
    ) {
        0.0
    } else {
        crate::rail::resolved_width(package, chrome)
    }
}

pub fn navigation_ctx(ctx: crate::ctx::Ctx<'_>) -> f32 {
    themed_navigation(
        ctx.layout,
        ctx.look.theme,
        ctx.themes.package_layout(&ctx.look.theme.id),
        ctx.themes.chrome(&ctx.look.theme.id),
    )
}

/// The navigation's share of the conversation's left edge. A right-docked
/// rail/sidebar still consumes width, but does not move window coordinates
/// measured from the chat's left.
pub fn navigation_left_ctx(ctx: crate::ctx::Ctx<'_>) -> f32 {
    let package = ctx.themes.package_layout(&ctx.look.theme.id);
    let chrome = ctx.themes.chrome(&ctx.look.theme.id);
    let gap = ctx.themes.glass_gap(&ctx.look.theme.id);
    themed_navigation(ctx.layout, ctx.look.theme, package, chrome) - right_rail(package, chrome)
        + gap * 2.0
}

/// Centres the message lane and composer at an installed theme's maximum
/// chat width. The surrounding chat surface still fills the window, so its
/// ground and art do not acquire an artificial narrow edge.
fn bounded_chat<'a>(
    content: impl Into<Element<'a, Message>>,
    package: Option<&yapfast_core::themes::layout::Layout>,
) -> Element<'a, Message> {
    let content = content.into();
    let Some(width) = package.and_then(|layout| layout.chat_max_width) else {
        return content;
    };
    container(container(content).width(Length::Fill).max_width(width))
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

/// The composer at a theme's widest chat: its box as wide as the cards
/// above it, as Slint's (the composer's side gutter lies outside the
/// lane, not inside it).
fn bounded_composer<'a>(
    content: Element<'a, Message>,
    app: &App,
    package: Option<&yapfast_core::themes::layout::Layout>,
) -> Element<'a, Message> {
    let Some(width) = package.and_then(|layout| layout.chat_max_width) else {
        return content;
    };
    let gutter = app
        .theme
        .spacing()
        .chat_padding
        .unwrap_or(crate::composer::GUTTER);
    container(
        container(content)
            .width(Length::Fill)
            .max_width(width + 2.0 * gutter),
    )
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}

fn themed_composer(app: &App) -> Element<'_, Message> {
    // The typing strip lies on the conversation, over the composer's
    // own ground and art.
    let (strip, rest) = crate::composer::parts(app.ctx());
    let composer: Element<'_, Message> = column![
        strip,
        crate::theme_art::surface(
            rest,
            app.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Composer,
        )
    ]
    .into();
    match status_line(app) {
        Some((line, true)) => column![line, composer].into(),
        Some((line, false)) => column![composer, line].into(),
        None => composer,
    }
}

/// A theme's status line across the conversation's foot, as a terminal's
/// (`[{time}] [{name}] [{server}/{channel}]`), and whether it goes over the
/// composer.
fn status_line(app: &App) -> Option<(Element<'_, Message>, bool)> {
    let line = app.themes.chrome(&app.theme.id)?.status_line.as_ref()?;
    let ctx = app.ctx();
    let time = yapfast_core::clock::time_on(
        yapfast_core::clock::now_ms() / 1000,
        app.layout.prefs.twelve_hour_clock,
    );
    let unread = app
        .sidebar
        .rows
        .iter()
        .filter_map(|row| match row {
            crate::sidebar::SideRow::Room(room) if room.unread && !room.selected => {
                Some(room.name.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(",");
    let words = yapfast_core::themes::chrome::fill(
        &line.text,
        &[
            ("time", time.as_str()),
            ("name", ctx.account.name.as_str()),
            ("channel", ctx.header.name.as_str()),
            ("server", app.sidebar.header.name.as_str()),
            ("typing", ""),
            ("unread_channels", unread.as_str()),
        ],
    );
    let shown = container(
        crate::elide::elide(words)
            .font(crate::theme_chrome::mono(ctx))
            .size(13.0)
            .color(app.theme.secondary),
    )
    .height(22)
    .align_y(Alignment::Center);
    Some((shown.into(), line.above))
}

/// The conversation column: the header, the timeline and the composer,
/// with a server's member list beside them; or a voice channel's call.
pub fn conversation(app: &App) -> Element<'_, Message> {
    if crate::call::stage::voice_panel(app) {
        // The stage alone takes the `call` surface (see `call_view`).
        return crate::call::stage::call_view(app);
    }
    let outline = app.theme.outline_chat;
    let line = bar(outline, Length::Fill, 1);
    if let Some(screen) = crate::friends::dm_screen(app) {
        let page = match screen {
            yapfast_core::application::DmScreen::Friends => {
                crate::friends::friends_screen(app.ctx())
            }
            yapfast_core::application::DmScreen::Requests => {
                crate::friends::requests::requests_screen(app.ctx())
            }
        };
        return column![
            crate::friends::friends_header(app.ctx(), screen),
            line,
            page
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }
    if let Some(screen) = crate::browse_channels::view(app.ctx()) {
        return screen;
    }
    let mut chat = column(crate::forum::post_bar_view(app.ctx()))
        .extend(crate::banners::view(app.ctx()))
        .width(Length::Fill)
        .height(Length::Fill);
    // A DM's call shows above its messages.
    if let Some(call) = crate::call::dm::dm_call_view(app) {
        chat = chat.push(call);
    }
    let package_layout = app.themes.package_layout(&app.theme.id);
    let chat = match crate::forum::view(app.ctx()) {
        Some(forum) => chat.push(forum),
        None if package_layout.and_then(|layout| layout.composer) == Some(ComposerSide::Top) => {
            chat.push(bounded_composer(themed_composer(app), app, package_layout))
                .push(bounded_chat(
                    crate::timeline::view::timeline(app.ctx()),
                    package_layout,
                ))
        }
        None => chat
            .push(bounded_chat(
                crate::timeline::view::timeline(app.ctx()),
                package_layout,
            ))
            .push(bounded_composer(themed_composer(app), app, package_layout)),
    };
    // The chat's ground, art and glass lie under the whole conversation
    // (see `app::view`), as in the Slint build.
    let chat = crate::theme_transition::chat(app, chat.into());
    let header = app.ctx().header;
    let box_name = if header.direct {
        header.name.clone()
    } else {
        format!("#{}", header.name)
    };
    let chat = crate::theme_chrome::boxed(
        chat,
        app.ctx(),
        yapfast_core::themes::chrome::Pane::Chat,
        &box_name,
        None,
    );
    let members_side = package_layout
        .and_then(|layout| layout.members)
        .unwrap_or(MembersSide::Right);
    let mut members = (crate::header::members_shown(app) && members_side != MembersSide::Hidden)
        .then(|| {
            // A theme's box frame around the list keeps the list's width.
            let members = crate::theme_art::surface(
                container(crate::theme_chrome::boxed(
                    crate::members::view(&app.roster, app.ctx()),
                    app.ctx(),
                    yapfast_core::themes::chrome::Pane::Members,
                    "members",
                    Some(app.ctx().header.members_n as usize),
                ))
                .width(crate::members::docked_width(app))
                .height(Length::Fill),
                app.ctx(),
                yapfast_core::themes::package::art::ArtSurface::Members,
            );
            crate::theme_art::outlined(
                members,
                app.ctx(),
                yapfast_core::themes::layout::BorderPlace::Panes,
                "members",
                0.0,
                1.0,
                app.theme.outline,
            )
        });
    let members_left = members_side == MembersSide::Left;
    let mut body = row![];
    if members_left && let Some(members) = members.take() {
        body = body.push(members);
    }
    body = body.push(chat);
    if let Some(search) = crate::search::view(&app.searching, app.ctx()) {
        let search = crate::theme_art::surface(
            search,
            app.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Aside,
        );
        body = body.push(crate::theme_art::outlined(
            search,
            app.ctx(),
            yapfast_core::themes::layout::BorderPlace::Panes,
            "panel",
            0.0,
            1.0,
            app.theme.outline,
        ));
    } else if let Some(aside) = crate::thread::aside_view(app.ctx()) {
        let aside = crate::theme_art::surface(
            aside,
            app.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Aside,
        );
        body = body.push(crate::theme_art::outlined(
            aside,
            app.ctx(),
            yapfast_core::themes::layout::BorderPlace::Panes,
            "panel",
            0.0,
            1.0,
            app.theme.outline,
        ));
    } else if !members_left && let Some(members) = members.take() {
        body = body.push(members);
    } else if crate::profile::panel_shown(app)
        && let Some(panel) = crate::profile::profile_panel_view(app.ctx())
    {
        let panel = crate::theme_art::surface(
            panel,
            app.ctx(),
            yapfast_core::themes::package::art::ArtSurface::ProfilePane,
        );
        body = body.push(crate::theme_art::outlined(
            panel,
            app.ctx(),
            yapfast_core::themes::layout::BorderPlace::Panes,
            "panel",
            0.0,
            1.0,
            app.theme.outline,
        ));
    }
    let header = iced::widget::container(crate::theme_art::surface(
        crate::header::view(app.ctx()),
        app.ctx(),
        yapfast_core::themes::package::art::ArtSurface::Header,
    ))
    .height(crate::header::HEIGHT);
    column![header, line, body.height(Length::Fill)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The conversation column's left edge: the navigation's right.
pub fn chat_left(app: &App) -> f32 {
    let package = app.themes.package_layout(&app.theme.id);
    let chrome = app.themes.chrome(&app.theme.id);
    let gap = app.themes.glass_gap(&app.theme.id);
    let navigation = themed_navigation(&app.layout, &app.theme, package, chrome)
        - right_rail(package, chrome)
        + gap * 2.0;
    navigation
        + if crate::header::members_shown(app)
            && app
                .themes
                .package_layout(&app.theme.id)
                .and_then(|layout| layout.members)
                == Some(MembersSide::Left)
        {
            crate::members::docked_width(app)
        } else {
            0.0
        }
}

/// The conversation column's right edge: the member list's left.
pub fn chat_right(app: &App) -> f32 {
    let package = app.themes.package_layout(&app.theme.id);
    let navigation =
        right_rail(package, app.themes.chrome(&app.theme.id)) + app.themes.glass_gap(&app.theme.id);
    app.window.width
        - navigation
        - if crate::search::search_shown(app) {
            crate::search::WIDTH
        } else if crate::thread::aside_shown(app) {
            crate::thread::aside_width(app)
        } else if crate::header::members_shown(app)
            && app
                .themes
                .package_layout(&app.theme.id)
                .and_then(|layout| layout.members)
                .unwrap_or(MembersSide::Right)
                != MembersSide::Left
            && app
                .themes
                .package_layout(&app.theme.id)
                .and_then(|layout| layout.members)
                .unwrap_or(MembersSide::Right)
                != MembersSide::Hidden
        {
            crate::members::width(&app.layout, &app.theme)
        } else if crate::profile::panel_shown(app) {
            crate::profile::PANEL_WIDTH
        } else {
            0.0
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_right_rail_docks_on_the_right() {
        let mut layout = yapfast_core::themes::layout::Layout::default();
        assert!(!navigation_right(Some(&layout)));
        layout.rail = Some(RailSide::Hidden);
        assert!(!navigation_right(Some(&layout)));
        layout.rail = Some(RailSide::Right);
        assert!(navigation_right(Some(&layout)));
    }
}
