//! The conversation column: what takes the timeline's place (the friends
//! screens, browsing, the forum, a call) and what stands beside it
//! (search, the thread, the member list, the profile panel).

use iced::{
    Element, Length,
    widget::{column, row},
};

use crate::{
    app::{App, Message},
    controls::bar,
};

/// The navigation's width beside the conversation: the rail and the
/// sidebar, as wide as its edge was dragged.
pub fn navigation(layout: &crate::layout::Layout) -> f32 {
    crate::rail::WIDTH + crate::sidebar::width(layout)
}

/// The conversation column: the header, the timeline and the composer,
/// with a server's member list beside them; or a voice channel's call.
pub fn conversation(app: &App) -> Element<'_, Message> {
    if crate::call::stage::voice_panel(app) {
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
    let chat = match crate::forum::view(app.ctx()) {
        Some(forum) => chat.push(forum),
        None => chat
            .push(crate::timeline::view::timeline(app.ctx()))
            .push(crate::composer::view(app.ctx())),
    };
    let mut body = row![chat];
    if let Some(search) = crate::search::view(&app.searching, app.ctx()) {
        body = body.push(search);
    } else if let Some(aside) = crate::thread::aside_view(app.ctx()) {
        body = body.push(aside);
    } else if crate::header::members_shown(app) {
        body = body.push(crate::members::view(&app.roster, app.ctx()));
    } else if crate::profile::panel_shown(app)
        && let Some(panel) = crate::profile::profile_panel_view(app.ctx())
    {
        body = body.push(panel);
    }
    column![
        crate::header::view(app.ctx()),
        line,
        body.height(Length::Fill)
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The conversation column's left edge: the navigation's right.
pub fn chat_left(app: &App) -> f32 {
    navigation(&app.layout)
}

/// The conversation column's right edge: the member list's left.
pub fn chat_right(app: &App) -> f32 {
    app.window.width
        - if crate::search::search_shown(app) {
            crate::search::WIDTH
        } else if crate::thread::aside_shown(app) {
            crate::thread::aside_width(app)
        } else if crate::header::members_shown(app) {
            crate::members::width(&app.layout)
        } else if crate::profile::panel_shown(app) {
            crate::profile::PANEL_WIDTH
        } else {
            0.0
        }
}
