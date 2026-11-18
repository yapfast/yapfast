//! The image viewer (`show_viewer`, `src/ui/refresh.rs:5069`; its
//! callbacks, `src/ui/callbacks.rs:1146`, `:1159`) and the menu of a
//! picture or video shown large (`ViewerMenu`).
//!
//! The core opens it (`Action::ViewImage`) and lets it go
//! (`Action::CloseImage`); the projection opens `Layer::Viewer` while it
//! has it and closes it after, and the picture goes once the layer played
//! its way out ([`gone`]). The zoom is the window's own.

use iced::Task;
use yapfast_core::application::{Action, AppState};
use yapfast_core::chat::{VIEW_HEIGHT, VIEW_WIDTH};
use yapfast_lang::tr;

use anomi_vm::chat::viewer::{Intent, Note, Viewer};
use anomi_vm::menu::{Menu, Spot};
use anomi_vm::shell::{Layer, Shell};

use crate::app::App;
use crate::message::Message;

/// The size the viewer opened at for an item's image: the full image only
/// sharpens it on arrival (`viewer_size`).
#[derive(Default)]
pub struct Shown {
    size: Option<(String, usize, (u32, u32))>,
}

/// Shows the core's viewer, and opens or closes its layer.
pub fn show(state: &AppState, shell: &mut Shell, viewer: &mut Viewer, shown: &mut Shown) {
    let viewing = state.viewer.as_ref().and_then(|viewer| {
        let message = state.message(&viewer.item)?;
        Some((viewer, message, message.attachments.get(viewer.index)?))
    });
    let Some((open, message, attachment)) = viewing else {
        // The picture stays while the layer plays its way out.
        shown.size = None;
        if shell.is_open(Layer::Viewer) {
            shell.close(Layer::Viewer);
        }
        return;
    };
    let loaded = open.pictures.get(&open.index);
    let full = match loaded {
        Some(Some(Ok(picture))) => Some(picture),
        _ => None,
    };
    let note = match loaded {
        Some(Some(Err(error))) => Note::Failed((*error).to_owned()),
        Some(None) => Note::Loading,
        _ => Note::None,
    };
    let size = match &shown.size {
        Some((item, index, size)) if *item == open.item && *index == open.index => *size,
        _ => {
            let size = match (full, attachment.dimensions, &attachment.preview) {
                (Some(full), _, _) => Some((full.width, full.height)),
                (None, Some((width, height)), _) => Some(fit(width, height)),
                (None, None, Some(preview)) => Some((preview.width, preview.height)),
                _ => None,
            };
            shown.size = size.map(|size| (open.item.clone(), open.index, size));
            size.unwrap_or((1, 1))
        }
    };
    let images: Vec<usize> = (message.attachments.iter().enumerate())
        .filter(|(_, attachment)| attachment.image)
        .map(|(index, _)| index)
        .collect();
    let place = (images.iter().position(|&index| index == open.index))
        .filter(|_| images.len() > 1)
        .map(|place| (place + 1, images.len()));
    // Another picture starts at the fit.
    let zoom = if viewer.item == open.item && viewer.index == open.index {
        viewer.zoom
    } else {
        1.0
    };
    let made = Viewer {
        item: open.item.clone(),
        index: open.index,
        name: attachment.name.clone(),
        picture: full
            .or(attachment.preview.as_ref())
            .and_then(crate::pictures::shown),
        size,
        place,
        note,
        preview: state.preview,
        link: attachment.url.as_deref().map(str::to_owned),
        zoom,
        save_tip: tr!("Save {}", attachment.name),
    };
    if *viewer != made {
        *viewer = made;
    }
    if !shell.is_open(Layer::Viewer) {
        shell.open(Layer::Viewer);
    }
}

/// A full image's size once shrunk to the viewer's largest (`fit`,
/// `src/ui/pictures.rs:209`).
fn fit(width: u32, height: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(VIEW_WIDTH) / f64::from(width.max(1)),
            f64::from(VIEW_HEIGHT) / f64::from(height.max(1)),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (side(width), side(height))
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Close => close(app),
        Intent::Step(step) => app.dispatch(Action::StepImage(step)),
        Intent::Zoom(zoom) => {
            let viewer = &mut app.vm.chat.viewer;
            viewer.zoom = zoom.apply(viewer.zoom);
            Task::none()
        }
        Intent::Save { item, index } => crate::intent::chat::save::choose_item(app, &item, index),
        Intent::CopyImage { item, index } => app.dispatch(Action::CopyAttachment { item, index }),
        Intent::CopyLink(link) => iced::clipboard::write(link).discard(),
        Intent::OpenLink(link) => crate::platform::links::open_page(app, &link),
        Intent::Menu(spot) => {
            let rows = anomi_chat::viewer::menu::rows(&app.vm.chat.viewer, false);
            crate::intent::menus::show(app, Menu::new(0, spot, rows));
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Escape, a press beside the picture, its Close: the core lets it go, and
/// the projection closes the layer.
pub fn close(app: &mut App) -> Task<Message> {
    if app.core.state.viewer.is_none() {
        app.vm.shell.close(Layer::Viewer);
        return Task::none();
    }
    app.dispatch(Action::CloseImage)
}

/// The viewer played its way out: its picture goes.
pub fn gone(app: &mut App) {
    if app.vm.shell.opened(Layer::Viewer).is_none() {
        app.vm.chat.viewer = Viewer::default();
    }
}

/// A right click on the video full screen: its menu, the viewer's rows for
/// a video (`video-menu`).
pub fn video_menu(app: &mut App, spot: Spot) -> Task<Message> {
    let state = &app.core.state;
    let Some((item, index)) = super::video::playing(&app.shown.media.video) else {
        return Task::none();
    };
    let link = (state.message(item))
        .and_then(|message| message.attachments.get(index))
        .and_then(|attachment| attachment.url.as_deref().map(str::to_owned));
    let about = Viewer {
        item: item.to_owned(),
        index,
        preview: state.preview,
        link,
        ..Viewer::default()
    };
    if !anomi_vm::chat::video::FullScreen::has_menu(&app.vm.chat.video) {
        return Task::none();
    }
    let rows = anomi_chat::viewer::menu::rows(&about, true);
    crate::intent::menus::show(app, Menu::new(0, spot, rows));
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_image_opens_at_the_viewers_largest() {
        let (width, height) = fit(VIEW_WIDTH * 2, VIEW_HEIGHT);
        assert_eq!(width, VIEW_WIDTH);
        assert!(height <= VIEW_HEIGHT);
        assert_eq!(fit(200, 100), (200, 100), "never enlarged");
    }
}
