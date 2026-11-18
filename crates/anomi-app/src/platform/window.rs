//! The window: opening it, again from the tray; its own title bar (F18's
//! `chrome`) and the intents of its buttons, edges and drags as
//! `iced::window` tasks; its title. Closing it to the tray is
//! [`super::forget`]'s. The Slint side: `src/ui/shell.rs` (`open`,
//! `open_with`, `close`, `show_title`) and `src/ui/title_bar.rs`.

use iced::{Task, window};
use yapfast_core::application::{AppState, Effect, Navigation};
use yapfast_lang::tr;

use anomi_vm::chrome::{self, Edge};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::layout::Layout;
use crate::message::Message;

/// Where the bar's help button goes (`ui/yapfast.slint:5623`).
const HELP: &str = "https://support.discord.com";

/// Desktops whose windows have title bars, as `XDG_CURRENT_DESKTOP` names
/// them (`src/ui/title_bar.rs`): there the desktop draws one unless Anomi
/// draws its own, and a tiling compositor draws none, where Anomi's would
/// be in the way.
const FRAMED: [&str; 14] = [
    "KDE",
    "GNOME",
    "Unity",
    "Cinnamon",
    "X-Cinnamon",
    "XFCE",
    "MATE",
    "LXQt",
    "LXDE",
    "Budgie",
    "Pantheon",
    "Deepin",
    "COSMIC",
    "Enlightenment",
];

/// Whether Anomi draws the window's frame: as the layout chose, else on
/// Windows and on a Linux desktop known to give windows title bars. Each
/// window is made with the frame or without it, never given it once made.
pub fn own_bar(layout: &Layout) -> bool {
    let offered = cfg!(any(target_os = "linux", windows));
    offered && !layout.system_title_bar.unwrap_or_else(system_by_default)
}

fn system_by_default() -> bool {
    if cfg!(windows) {
        return false;
    }
    !std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| framed(&desktops))
}

/// Whether any desktop of a colon-separated list, such as "ubuntu:GNOME",
/// gives its windows title bars.
pub(super) fn framed(desktops: &str) -> bool {
    desktops.split(':').any(|desktop| {
        FRAMED
            .iter()
            .any(|framed| desktop.eq_ignore_ascii_case(framed))
    })
}

/// Shows the window, making it again if it was closed, and brings it
/// forward with the desktop's activation token, where one came.
pub fn open(app: &mut App, token: Option<String>) -> Task<Message> {
    if let Some(id) = app.window {
        let mut tasks = vec![window::minimize(id, false), window::gain_focus(id)];
        tasks.push(match token {
            Some(token) => activate(id, token),
            None => attention(id),
        });
        return Task::batch(tasks);
    }
    anomi_desktop::instance::give_next_window(token);
    let settings = super::placement::settings(app);
    let (id, opened) = window::open(settings);
    app.window = Some(id);
    super::forget::shown_again(app);
    tracing::info!("window opened");
    opened.map(Message::Opened)
}

/// The window opened: it takes the focus with the token it was made for,
/// comes to the size it was left at on this display, and the timeline
/// back to the messages it was closed on.
pub fn opened(app: &mut App, id: window::Id) -> Task<Message> {
    app.window = Some(id);
    let token = anomi_desktop::instance::take_next_token();
    let placed = window::scale_factor(id).then(move |base| {
        window::monitor_size(id).map(move |monitor| Message::Placed(base, monitor))
    });
    let reading = super::forget::reading_again(app);
    Task::batch([
        placed,
        token.map_or_else(Task::none, |token| activate(id, token)),
        reading,
    ])
}

/// Brings a shown window forward on Wayland, where focusing it asks
/// nothing without the token.
fn activate(id: window::Id, token: String) -> Task<Message> {
    window::run(id, move |window| {
        anomi_desktop::instance::activate(window, &token);
    })
    .discard()
}

/// Without a token, a Wayland window asks for one of its own, which brings
/// it forward where the compositor lets it (`open` in `src/ui/shell.rs`):
/// winit's focus and un-minimizing do nothing there.
fn attention(id: window::Id) -> Task<Message> {
    window::run(id, |window| anomi_desktop::instance::wayland(window)).then(move |wayland| {
        if wayland {
            window::request_user_attention(id, Some(window::UserAttention::Informational))
        } else {
            Task::none()
        }
    })
}

/// The close button, the bar's or the desktop's: to the tray, or without
/// one to close into (or with the setting off), quits.
pub fn close(app: &mut App) -> Task<Message> {
    let to_tray = app.layout.close_to_tray.unwrap_or(true) && app.desktop.tray_shows();
    if !to_tray {
        tracing::info!("window closed without a tray to close to; quitting");
        return app.quit();
    }
    super::forget::window(app)
}

/// The title bar's intents.
pub fn handle(app: &mut App, intent: chrome::Intent) -> Task<Message> {
    use chrome::Intent;
    match intent {
        Intent::Close => return close(app),
        Intent::Back => return crate::keys::back(app),
        Intent::Forward => return crate::keys::forward(app),
        Intent::OpenHelp => {
            return crate::effects::run(app, vec![Effect::OpenPage(HELP.to_owned())]);
        }
        _ => {}
    }
    let Some(id) = app.window else {
        return Task::none();
    };
    match intent {
        Intent::Move => window::drag(id),
        Intent::Resize(edge) => window::drag_resize(id, direction(edge)),
        Intent::Minimize => window::minimize(id, true),
        Intent::ToggleMaximize => window::toggle_maximize(id),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

fn direction(edge: Edge) -> window::Direction {
    use window::Direction;
    match edge {
        Edge::East => Direction::East,
        Edge::North => Direction::North,
        Edge::NorthEast => Direction::NorthEast,
        Edge::NorthWest => Direction::NorthWest,
        Edge::South => Direction::South,
        Edge::SouthEast => Direction::SouthEast,
        Edge::SouthWest => Direction::SouthWest,
        Edge::West => Direction::West,
    }
}

/// The bar from the core's state and the layout (`WindowBar`'s bindings,
/// `ui/yapfast.slint:5610`): where the person is, and whether the arrows go
/// anywhere. The focus and the maximized window follow the window's own
/// events; the inbox's button its popout (I04).
pub fn show_chrome(state: &AppState, layout: &Layout, vm: &mut anomi_vm::Vm) {
    let settings = vm.shell.is_open(Layer::Settings);
    let chrome = &mut vm.chrome;
    chrome.framed = own_bar(layout);
    chrome.workspace = state.preview || state.account.is_some();
    let place = match &state.navigation {
        _ if !chrome.workspace => yapfast_lang::PRODUCT,
        Navigation::DirectMessages => tr!("Direct Messages"),
        Navigation::Server(id) => state
            .servers
            .iter()
            .find(|server| &server.id == id)
            .map_or(tr!("Server"), |server| server.name.as_str()),
    };
    if chrome.place != place {
        place.clone_into(&mut chrome.place);
    }
    let arrows = chrome.workspace && !settings;
    chrome.can_go_back = arrows && state.can_go_back();
    chrome.can_go_forward = arrows && state.can_go_forward();
}

/// The window's title, counting the mentions waiting, as a browser tab's
/// does; every message in a DM counts as one (`show_title`).
pub fn title(title: &mut String, mentions: u64) {
    let name = yapfast_lang::PRODUCT;
    let now = match mentions {
        0 => name.to_owned(),
        1..=99 => format!("({mentions}) {name}"),
        _ => format!("(99+) {name}"),
    };
    if *title != now {
        *title = now;
    }
}

/// The window was resized: whether it's maximized now, for the bar's
/// middle button and the size kept.
pub fn resized(app: &App) -> Task<Message> {
    app.window.map_or_else(Task::none, |id| {
        window::is_maximized(id).map(Message::Maximized)
    })
}

/// Whether the window is maximized, as asked after a resize.
pub fn maximized(app: &mut App, maximized: bool) {
    app.vm.chrome.maximized = maximized;
    super::placement::keep(app, maximized);
}
