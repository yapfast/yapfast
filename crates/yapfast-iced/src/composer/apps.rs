//! The composer Apps browser uses the room's command index without typing a
//! slash into the message. Choosing a command still goes through the core.

use std::collections::HashSet;

use iced::{
    Alignment, Border, Color, Element, Length, Point, Task,
    widget::{Id, button, column, container, opaque, operation, row, text, text_input},
};
use yapfast_core::{application::Action, discord, i18n::tr};

use crate::{
    app::{App, Message},
    controls::{lit_style, menu_panel},
    ctx::Ctx,
    menus::Menu,
    pictures::Icon,
    widgets::{Anchor, Pic, Place, scroll_view},
};

const FIELD: &str = "composer-app-search";
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 360.0;

pub struct Browser {
    room: String,
    search: String,
    app: Option<String>,
    requested_icons: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Search(String),
    Filter(Option<String>),
    Pick { command: String, path: Vec<String> },
}

fn message(event: Event) -> Message {
    Message::Write(super::Event::Apps(event))
}

pub fn open(app: &mut App, room: String) -> Task<Message> {
    let closed = if app.win.picker.is_open() {
        crate::picker::close_picker(app)
    } else {
        Task::none()
    };
    let at = Point::new(
        crate::conversation::chat_right(app) - WIDTH - super::GUTTER,
        super::composer_top(app) - 8.0,
    );
    app.win.menu = Some(Menu {
        composer_apps: Some(Browser {
            room,
            search: String::new(),
            app: None,
            requested_icons: false,
        }),
        ..Menu::new(at, vec![])
    });
    app.client.dispatch(Action::AskAppCommands);
    Task::batch([closed, app.sync()]).chain(operation::focus(Id::new(FIELD)))
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    let Some(browser) = app
        .win
        .menu
        .as_mut()
        .and_then(|menu| menu.composer_apps.as_mut())
    else {
        return Task::none();
    };
    match event {
        Event::Search(search) => browser.search = search,
        Event::Filter(filter) => {
            browser.app = filter;
            browser.search.clear();
            return operation::focus(Id::new(FIELD));
        }
        Event::Pick { command, path } => {
            let room = browser.room.clone();
            app.client.dispatch(Action::PickBrowsedCommand {
                room,
                command,
                path,
            });
            if app.client.state().command_draft().is_some() {
                app.win.menu = None;
                app.composer.writing_last = super::Writing::Draft;
                return Task::batch([app.sync(), super::refocus(app)]);
            }
        }
    }
    Task::none()
}

pub fn sync(app: &mut App) {
    let Some(browser) = app
        .win
        .menu
        .as_mut()
        .and_then(|menu| menu.composer_apps.as_mut())
    else {
        return;
    };
    let state = app.client.state();
    if state.selected.as_deref() != Some(browser.room.as_str()) || !state.can_write() {
        app.win.menu = None;
        return;
    }
    if state.app_commands.index(&browser.room).is_none() {
        browser.requested_icons = false;
        if !state.app_commands.loading(&browser.room) && state.app_commands.error.is_none() {
            app.client.dispatch(Action::AskAppCommands);
        }
        return;
    }
    if !browser.requested_icons {
        browser.requested_icons = true;
        let mut seen = HashSet::new();
        let urls = app
            .client
            .app
            .slash_commands(&browser.room, "")
            .into_iter()
            .filter_map(|command| command.app_icon)
            .filter(|url| seen.insert(url.clone()))
            .collect();
        app.client.want_pictures(urls);
    }
}

pub fn view<'a>(browser: &'a Browser, at: Point, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    if cx.state.selected.as_deref() != Some(browser.room.as_str()) {
        return None;
    }
    let theme = cx.look.theme;
    let query = browser.search.trim().to_lowercase();
    let browse_apps = query.is_empty() && browser.app.is_none();
    let commands = cx.app.slash_commands(&browser.room, "");
    let may_pick = cx.state.command_draft().is_none()
        && (cx.state.draft().is_empty() || cx.state.slash_query().is_some());

    let search = text_input(tr("Search apps and commands"), &browser.search)
        .id(Id::new(FIELD))
        .on_input(|value| message(Event::Search(value)))
        .padding(0)
        .size(14)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: theme.muted,
            placeholder: theme.muted,
            value: theme.text,
            selection: theme.selection,
        });
    let search = container(
        row![cx.look.glyph(Icon::Search, 16, theme.muted), search]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([9, 10])
    .style(move |_| container::Style {
        background: Some(theme.field.into()),
        border: Border::default()
            .color(theme.accent)
            .width(1.0)
            .rounded(8.0),
        ..container::Style::default()
    });

    let mut lines = column![].spacing(4);
    let mut seen = HashSet::new();
    let mut count = 0;
    for command in commands {
        let title = if browse_apps {
            if !seen.insert(command.app.clone()) {
                continue;
            }
            command.app_name.clone()
        } else {
            if browser.app.as_ref().is_some_and(|id| id != &command.app)
                || (!query.is_empty()
                    && !command
                        .name
                        .to_lowercase()
                        .contains(query.trim_start_matches('/'))
                    && !command.app_name.to_lowercase().contains(query.as_str())
                    && !command.description.to_lowercase().contains(query.as_str()))
            {
                continue;
            }
            format!("/{}", command.name)
        };
        count += 1;
        let label = text(title).size(14).color(theme.text);
        let content = if browse_apps {
            column![label]
        } else {
            column![
                label,
                text(format!("{} · {}", command.app_name, command.description))
                    .size(12)
                    .color(theme.muted)
            ]
            .spacing(2)
        };
        let icon: Element<'_, Message> = command
            .app_icon
            .as_deref()
            .and_then(|url| cx.picture(url, 32, 32, true))
            .map_or_else(
                || {
                    container(cx.look.glyph(Icon::Apps, 18, theme.muted))
                        .center(32)
                        .into()
                },
                |picture| Pic::square(picture, 32.0).radius(7.0).into(),
            );
        let mut item = button(row![icon, content].spacing(10).align_y(Alignment::Center))
            .padding([8, 10])
            .width(Length::Fill)
            .style(lit_style(None, theme.wash(0.09), 6.0));
        if browse_apps {
            item = item.on_press(message(Event::Filter(Some(command.app))));
        } else if may_pick {
            item = item.on_press(message(Event::Pick {
                command: command.command,
                path: command.path,
            }));
        }
        lines = lines.push(item);
    }
    if count == 0 {
        let status = if cx.state.app_commands.loading(&browser.room) {
            tr("Loading commands")
        } else if let Some(error) = &cx.state.app_commands.error {
            error.as_str()
        } else if !query.is_empty() {
            tr("No matching apps or commands")
        } else {
            tr("No apps with commands are available here")
        };
        lines = lines.push(text(status).size(13).color(theme.muted));
    }

    let mut panel = column![search].spacing(10).padding(12);
    if browser.app.is_some() {
        panel = panel.push(
            button(text(tr("All apps")).size(13))
                .padding([4, 8])
                .style(lit_style(None, theme.wash(0.09), 6.0))
                .on_press(message(Event::Filter(None))),
        );
    } else if browse_apps && count > 0 {
        let is_dm =
            discord::parse_room(&browser.room).is_some_and(|(guild, _)| guild == discord::DM_GUILD);
        panel = panel.push(
            text(if is_dm {
                tr("Apps in this conversation")
            } else {
                tr("Apps available in this server")
            })
            .size(13)
            .color(theme.muted),
        );
    }
    if !browse_apps && count > 0 && !may_pick {
        panel = panel.push(
            text(tr("Apps: finish or clear your message first"))
                .size(13)
                .color(theme.muted),
        );
    }
    let panel = container(
        panel.push(
            scroll_view(lines)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(crate::style::scrollbar(theme)),
        ),
    )
    .width(WIDTH)
    .height(HEIGHT.min(cx.look.window.height - 16.0).max(1.0))
    .style(menu_panel(theme));

    Some(
        Place::new(opaque(panel), at, 8.0)
            .anchor(Anchor::Bottom)
            .into(),
    )
}
