//! The window driven through `iced_test`'s headless simulator, on the
//! core's preview: each test builds the view, clicks and types into it as a
//! person would, and hands the messages that came out to `App::update`, as
//! iced's runtime does. The tasks `update` returns (focus, the clipboard,
//! timers) don't run here, and the simulator keeps widget state only while
//! it lives, so a field is clicked in the same simulator it's typed into.

use std::{
    path::PathBuf,
    sync::{Arc, Once},
};

use iced::{Point, Settings, mouse, widget::Id};
use iced_test::{Simulator, selector::Candidate};
use yapfast_core::{
    application::Action,
    i18n::{Language, pin_language},
    runtime::BackgroundRuntime,
};

use crate::{
    app::{App, Message},
    client::Client,
    fonts,
};

/// The preview at the window's own size, its data in a folder of its own.
fn preview() -> (App, tempfile::TempDir) {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    (start(&folder, true), folder)
}

/// The window on `folder`'s data: the preview, or else the sign-in screen
/// of a Yapfast that has no account yet.
fn start(folder: &tempfile::TempDir, preview: bool) -> App {
    install_fonts();
    // The texts read in English whatever the machine's language is, and a
    // test's language change stays on its thread. The window follows the
    // system's language as it opens, so this is set again after.
    pin_language(Some(Language::ENGLISH));
    let runtime = Arc::new(BackgroundRuntime::new().expect("runtime"));
    let mut client = Client::new(runtime, folder.path().into());
    if preview {
        client.dispatch(Action::Preview);
    }
    let mut app = App::new(client, (1120, 760)).0;
    pin_language(Some(Language::ENGLISH));
    // As the window opens, in front.
    app.shell.window = Some(iced::window::Id::unique());
    app.shell.focused = true;
    app
}

/// The preview under an installed theme whose dark variant is `variant`
/// over Yap, with `files` in its package.
pub(super) fn themed(
    variant: serde_json::Value,
    files: &[(&str, &[u8])],
) -> (App, tempfile::TempDir) {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let source = folder.path().join("variant-theme");
    std::fs::create_dir_all(&source).expect("theme source");
    let mut variant = variant;
    variant["base"] = "yap".into();
    let manifest = serde_json::json!({
        "theme_api": 1,
        "id": "dev.example.variant",
        "name": "Variant",
        "version": "1.0.0",
        "variants": { "dark": variant }
    });
    std::fs::write(
        source.join(yapfast_core::themes::MANIFEST),
        serde_json::to_vec(&manifest).expect("manifest JSON"),
    )
    .expect("theme manifest");
    for (name, bytes) in files {
        std::fs::write(source.join(name), bytes).expect("theme file");
    }
    let fetched = yapfast_core::themes::install::Fetched::local(&source).expect("local package");
    yapfast_core::themes::install::Themes::open(folder.path())
        .install(&fetched)
        .expect("installed package");
    std::fs::write(
        folder.path().join("layout.json"),
        br#"{"theme":"installed:dev.example.variant"}"#,
    )
    .expect("chosen theme");
    let mut app = start(&folder, true);
    let _ = app.sync();
    (app, folder)
}

/// The window's fonts, once per test run.
pub(crate) fn install_fonts() {
    static FONTS: Once = Once::new();
    FONTS.call_once(|| fonts::install(twemoji()));
}

/// The Twemoji font for the tests that draw or offer emoji: the one the
/// window carries.
pub(crate) fn twemoji() -> &'static [u8] {
    crate::TWEMOJI
}

/// The workspace's ignored `.local`, where tests keep their fixtures.
fn local() -> PathBuf {
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local");
    std::fs::create_dir_all(&local).expect(".local");
    local
}

/// Builds the view, lets `act` work on it, and applies what came out.
fn interact(app: &mut App, act: impl FnOnce(&mut Simulator<'_, Message>)) {
    let messages: Vec<Message> = {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        act(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
}

/// Whether the window shows `text`: some text reading it is inside the
/// window and the lists it's in, and no dimmed layer drawn after it (a
/// dialog's, a settings card's) lies over it.
fn shows(app: &App, text: &str) -> bool {
    let window = iced::Rectangle::with_size(app.shell.frame);
    let mut ui = Simulator::with_size(Settings::default(), window.size(), app.view());
    let dimmed = Id::new(crate::controls::DIMMED);
    // Each text in view that reads it, and whether a dim came after it.
    let mut found: Vec<(iced::Rectangle, bool)> = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| {
        let bounds = candidate
            .visible_bounds()
            .and_then(|bounds| bounds.intersection(&window));
        let reads = match &candidate {
            Candidate::Text { content, .. } => *content == text,
            Candidate::TextInput { state, .. } => state.text() == text,
            _ => false,
        };
        match (bounds, &candidate) {
            (Some(bounds), _) if reads => found.push((bounds, false)),
            (Some(bounds), Candidate::Container { id: Some(id), .. }) if **id == dimmed => {
                for (shown, under) in &mut found {
                    *under |= bounds.contains(shown.center());
                }
            }
            _ => {}
        }
        None::<()>
    });
    found.iter().any(|(_, under)| !under)
}

/// Whether the window holds `text` anywhere, shown or not: in a list
/// scrolled away, under a dialog, cut off.
fn present(app: &App, text: &str) -> bool {
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    ui.find(text).is_ok()
}

/// The middle of the last text in view that reads `text`.
fn last(ui: &mut Simulator<'_, Message>, text: &str) -> Point {
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == text
        {
            found = Some(bounds.center());
        }
        None::<()>
    });
    found.unwrap_or_else(|| panic!("{text:?} isn't in view"))
}

/// The middle of a text in view that starts with `prefix`.
fn starting(ui: &mut Simulator<'_, Message>, prefix: &str) -> Option<Point> {
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content.starts_with(prefix)
        {
            found = Some(bounds.center());
        }
        None::<()>
    });
    found
}

/// The window as drawn now: its width and height in pixels (twice its
/// size, as the simulator draws it) and their RGBA.
fn drawn(ui: &mut Simulator<'_, Message>) -> (u32, u32, Vec<u8>) {
    let snapshot = ui.snapshot(&iced::Theme::Dark).expect("drawn");
    let folder = tempfile::tempdir_in(local()).expect("a folder");
    snapshot
        .matches_image(folder.path().join("drawn.png"))
        .expect("written");
    // Named after the renderer too: the folder's one file.
    let written = std::fs::read_dir(folder.path())
        .expect("the folder")
        .flatten()
        .next()
        .expect("the picture");
    let png = std::fs::read(written.path()).expect("read back");
    let picture = yapfast_core::chat::decode_picture(&png, u32::MAX, u32::MAX).expect("a PNG");
    (picture.width, picture.height, picture.rgba.to_vec())
}

/// Whether hovering `at` draws something in `area` that wasn't there: a
/// tooltip, which no operation finds (iced's tooltip overlay reports none).
fn hover_draws(ui: &mut Simulator<'_, Message>, at: Point, area: iced::Rectangle) -> bool {
    let away = Point::new(1.0, 1.0);
    ui.point_at(away);
    ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
        position: away,
    })]);
    let (width, _, before) = drawn(ui);
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
        position: at,
    })]);
    let (_, _, after) = drawn(ui);
    let scale = 2.0;
    let rows = (area.y * scale) as usize..((area.y + area.height) * scale) as usize;
    let columns = (area.x * scale) as usize..((area.x + area.width) * scale) as usize;
    rows.flat_map(|y| columns.clone().map(move |x| (y * width as usize + x) * 4))
        .any(|at| before.get(at..at + 4) != after.get(at..at + 4))
}

/// A right click on the message reading `text`.
fn right_click(ui: &mut Simulator<'_, Message>, text: &str) {
    let body = ui.find(text).expect("the message");
    ui.point_at(body.visible_bounds().expect("in view").center());
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
}

/// What a task the window's update returned writes to the clipboard.
fn clipboard_writes(task: iced::Task<Message>) -> Vec<String> {
    use iced::futures::StreamExt;
    let Some(mut actions) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut written = Vec::new();
    while let std::task::Poll::Ready(Some(action)) = actions.poll_next_unpin(&mut cx) {
        if let iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Write {
            contents,
            ..
        }) = action
        {
            written.push(contents);
        }
    }
    written
}

/// The core's state, changed as the backend would, then taken in.
fn change(app: &mut App, change: impl FnOnce(&mut yapfast_core::application::AppState)) {
    change(&mut app.client.app.state);
    app.client.app.state.list_changes += 1;
    app.client.app.state.people_changes += 1;
    let _ = app.sync();
}

/// The open room holding one message, Sam's, with an 800 by 600 picture,
/// which shows smaller: a copy the window makes. That copy made.
fn a_picture_shown(app: &mut App) -> Arc<yapfast_core::chat::Picture> {
    use yapfast_core::chat::{Attachment, ChatMessage, Picture, Pixels, Placement};
    let picture = Arc::new(Picture {
        width: 800,
        height: 600,
        rgba: Pixels::new(800, 600, vec![128; 800 * 600 * 4]),
        ..Default::default()
    });
    let shown = picture.clone();
    change(app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "i0".into(),
            author: "Sam".into(),
            attachments: vec![Attachment {
                name: "a.png".into(),
                size: None,
                image: true,
                dimensions: Some((800, 600)),
                preview: Some(shown),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            ..Default::default()
        })]);
    });
    drop(app.view());
    app.win.pictures.fitter.borrow_mut().make_asked(&picture);
    drop(app.view());
    assert!(
        !app.win.pictures.fitter.borrow().is_empty(),
        "the copy kept"
    );
    picture
}

/// Marks a room unread with `mentions` of this account.
fn unread(app: &mut App, id: &str, mentions: u64) {
    change(app, |state| {
        let room = state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .expect("the room");
        room.unread = mentions.max(1);
        room.mentions = mentions;
    });
}

/// The open room's messages made `count` long, each its own, the newest
/// last; returns their items.
fn long_history(app: &mut App, count: usize) -> Vec<String> {
    let mut items = Vec::new();
    change(app, |state| {
        let template = (*state.messages[0]).clone();
        let messages: Vec<_> = (0..count)
            .map(|index| {
                let item = format!("long-{index}");
                items.push(item.clone());
                Arc::new(yapfast_core::chat::ChatMessage {
                    id: item.clone(),
                    item,
                    body: format!("Message number {index}"),
                    timestamp: template.timestamp + index as u64 * 600,
                    reply: None,
                    own: false,
                    reactions: Vec::new(),
                    attachments: Vec::new(),
                    ..template.clone()
                })
            })
            .collect();
        state.messages = Arc::new(messages);
    });
    items
}

/// Scrolls the timeline `pixels` up from where the window shows it, as
/// a touchpad does over it.
fn scroll_up(app: &mut App, pixels: f32) {
    let middle = Point::new(
        crate::rail::WIDTH + crate::sidebar::WIDTH + 200.0,
        app.window.height / 2.0,
    );
    interact(app, |ui| {
        ui.point_at(middle);
        ui.simulate(crate::widgets::swipe(pixels));
    });
}

/// The message at the view's top, and how far into it the view starts.
fn at_top(app: &App) -> (String, f32) {
    crate::timeline::scroll::reading(app).expect("reading earlier messages")
}

/// Messages like those of [`long_history`], numbered `from..to`.
fn numbered(app: &App, from: usize, to: usize) -> Vec<Arc<yapfast_core::chat::ChatMessage>> {
    let template = app.client.state().messages.last().expect("a message");
    (from..to)
        .map(|index| {
            let item = format!("long-{index}");
            Arc::new(yapfast_core::chat::ChatMessage {
                id: item.clone(),
                item,
                body: format!("Message number {index}"),
                timestamp: template.timestamp + 600,
                ..(**template).clone()
            })
        })
        .collect()
}

/// An animated custom emoji's picture, a GIF as Discord serves one,
/// decoded as the core decodes it.
fn animated_emoji() -> Arc<yapfast_core::chat::Picture> {
    let gif = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testing/fixtures/gifs/hello.gif"
    ))
    .expect("the fixture");
    let picture = yapfast_core::chat::decode_picture(&gif, 64, 64).expect("decoded");
    assert!(picture.animation.is_some(), "it moves");
    Arc::new(picture)
}

/// The window drawn once more, `now`: the moving pictures in view take
/// the frame due.
fn redraw(app: &App) {
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    ui.simulate([iced::Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ))]);
}

mod accounts;
mod arrange;
mod calls;
mod cards;
mod channel_settings;
mod dialogs;
mod editor;
mod header;
mod link_dialogs;
mod media;
mod menus;
mod navigation;
mod people;
mod pickers;
mod polls;
mod rows;
mod server;
mod server_dialogs;
mod settings;
mod shell;
mod shortcuts;
mod signin;
mod threads;
mod timeline;
mod tree;
mod widgets;
mod writing;

/// The time now, in Unix milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}
