//! A message's picture shown large over the window, as the Slint client's
//! `ImageViewer`: the core's full picture once it comes (its preview until
//! then, at the size it opened at, so the full one only sharpens it), fitted
//! beside the bars, its name and place among the message's pictures above,
//! Save and Close, round buttons either side and the arrow keys stepping
//! through the message's pictures, and a note while the full one loads or
//! why it can't. A right click offers what the message's menu does for it.
//! Escape, Close or a click outside closes it.
//!
//! Saving a file, from here, a file's card or a menu, asks where with the
//! system's dialog, suggesting only the file's own name.

use std::path::Path;

use iced::{
    Alignment, Color, Element, Length, Padding, Size, Task,
    widget::{Space, button, container, row, text},
};
use yapfast_core::{application::Action, chat::Attachment, links};

use crate::{
    app::{App, Message},
    media::{Fit, Shot},
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
    style::rounded,
    viewer::{GROUND, Viewer},
    widgets::RightPress,
};

/// Room kept beside and above and below a picture, with the bars.
const SIDES: f32 = 160.0;
const ENDS: f32 = 128.0;
/// The round buttons that step through pictures, and their gap from the
/// window's edge.
const STEP: f32 = 48.0;
const STEP_GAP: f32 = 20.0;
/// A step button's ground, and under the pointer.
const STEP_GROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0x1a as f32 / 255.0);
const STEP_LIT: Color = Color::from_rgba(1.0, 1.0, 1.0, 0x33 as f32 / 255.0);
/// The note's pill.
const NOTE_GROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0xb3 as f32 / 255.0);
const NOTE_INK: Color = Color::from_rgba(1.0, 1.0, 1.0, 0xd9 as f32 / 255.0);

#[derive(Clone, Debug)]
pub enum Event {
    /// A message's picture clicked: the message's item, the attachment.
    Open(String, usize),
    Close,
    Step(isize),
    /// Save a message's attachment, or copy its picture.
    Save(String, usize),
    Copy(String, usize),
    /// A right click on a message's picture or video, in the timeline or
    /// here: its menu at the pointer.
    Menu(String, usize, bool),
    /// Where Save's dialog said to put the attachment.
    Saved(String, usize, Option<std::path::PathBuf>),
}

impl App {
    pub fn image_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Open(item, index) => {
                self.client.dispatch(Action::ViewImage { item, index });
                Task::batch([
                    self.sync(),
                    iced::widget::operation::focus(crate::app::NOBODY),
                ])
            }
            Event::Close => {
                self.client.dispatch(Action::CloseImage);
                self.viewer_size = None;
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Step(step) => {
                self.client.dispatch(Action::StepImage(step));
                self.sync()
            }
            Event::Save(item, index) => self.save_attachment(item, index),
            Event::Saved(item, index, path) => {
                if let Some(path) = path {
                    self.client
                        .dispatch(Action::SaveAttachment { item, index, path });
                }
                self.sync()
            }
            Event::Copy(item, index) => {
                self.client.dispatch(Action::CopyAttachment { item, index });
                self.sync()
            }
            Event::Menu(item, index, video) => self.media_menu(item, index, video),
        }
    }

    /// Asks where to save a message's attachment, suggesting its own name.
    pub fn save_attachment(&mut self, item: String, index: usize) -> Task<Message> {
        let state = self.client.state();
        if state.preview {
            return Task::none();
        }
        let name = state
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .map(|attachment| suggested_name(&attachment.name))
            .unwrap_or_else(|| "attachment".into());
        Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .set_title("Save attachment")
                    .set_file_name(name)
                    .save_file()
                    .await
                    .map(|file| file.path().to_owned())
            },
            move |path| Message::Image(Event::Saved(item.clone(), index, path)),
        )
    }

    /// A message's menu for one of its pictures or videos, at the pointer:
    /// in the timeline, its rows and then these; in the viewer, these.
    fn media_menu(&mut self, item: String, index: usize, video: bool) -> Task<Message> {
        let at = self.pressed.get();
        let preview = self.client.state().preview;
        let link = self
            .client
            .state()
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .and_then(|attachment| attachment.url.clone())
            .unwrap_or_default();
        let rows = media_rows(index, video, preview, !link.is_empty());
        if self.client.state().viewer.is_some() {
            if rows.is_empty() {
                return Task::none();
            }
            self.menu = Some(Menu {
                item: Some(item),
                ..Menu::new(at, rows)
            });
            return iced::widget::operation::focus(crate::app::NOBODY);
        }
        // In the timeline, or the thread beside, the message's own rows
        // come first.
        let Some(row) = self.row_of(&item) else {
            return Task::none();
        };
        let task = self.open_menu(row, at);
        if let Some(menu) = &mut self.menu {
            let mut extra = rows.into_iter();
            if let Some(first) = extra.next() {
                // Before Delete and Copy Message ID, as Slint's.
                let at = menu
                    .rows
                    .iter()
                    .position(|row| row.separated)
                    .unwrap_or(menu.rows.len());
                let mut inserted = vec![MenuRow {
                    separated: true,
                    ..first
                }];
                inserted.extend(extra);
                menu.rows.splice(at..at, inserted);
            }
        }
        task
    }

    /// What a media row of a menu does to the message `item`'s attachment
    /// `index`.
    pub fn media_act(&mut self, act: MenuAct, item: String, index: usize) -> Task<Message> {
        let link = self
            .client
            .state()
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .and_then(|attachment| attachment.url.clone())
            .unwrap_or_default();
        let focus = self.refocus();
        match act {
            MenuAct::CopyImage => Task::batch([focus, self.image_event(Event::Copy(item, index))]),
            MenuAct::SaveMedia => Task::batch([focus, self.save_attachment(item, index)]),
            MenuAct::CopyMediaLink => {
                Task::batch([focus, iced::clipboard::write(link.to_string())])
            }
            MenuAct::OpenMediaLink => {
                crate::browser::open(&link);
                focus
            }
            _ => focus,
        }
    }

    /// The picture being viewed, over everything.
    pub fn image_viewer(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let viewer = state.viewer.as_ref()?;
        let message = state.message(&viewer.item)?;
        let attachment = message.attachments.get(viewer.index)?;
        let loaded = viewer.pictures.get(&viewer.index);
        let full = match loaded {
            Some(Some(Ok(picture))) => Some(picture),
            _ => None,
        };
        let note = match loaded {
            Some(Some(Err(error))) => *error,
            Some(None) => "Loading the full image...",
            _ => "",
        };
        let natural = self
            .viewer_size
            .as_ref()
            .filter(|(item, index, _)| *item == viewer.item && *index == viewer.index)
            .map(|(_, _, size)| *size)
            .unwrap_or_else(|| {
                natural_size(full.map(|full| (full.width, full.height)), attachment)
            });
        let room = Size::new(
            (self.window.width - SIDES).max(1.0),
            (self.window.height - ENDS).max(1.0),
        );
        let scale = 1f32
            .min(room.width / natural.width)
            .min(room.height / natural.height);
        let size = Size::new(
            (natural.width * scale).round().max(1.0),
            (natural.height * scale).round().max(1.0),
        );
        let shown = full.or(attachment.preview.as_ref());
        let picture = self.shot(Shot {
            fit: Fit::Contain,
            ground: false,
            ..Shot::new(shown, size)
        });
        let (item, index) = (viewer.item.clone(), viewer.index);
        let picture = RightPress::new(picture, move |_| {
            Message::Image(Event::Menu(item.clone(), index, false))
        });
        let images: Vec<usize> = message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| attachment.image)
            .map(|(index, _)| index)
            .collect();
        let place = images.iter().position(|index| *index == viewer.index);
        let mut buttons: Vec<Element<'_, Message>> = Vec::new();
        if !state.preview {
            buttons.push(
                self.look()
                    .face_button(
                        self.glyph(Icon::Download, 20, Color::WHITE),
                        32.0,
                        "Save",
                        Message::Image(Event::Save(viewer.item.clone(), viewer.index)),
                    )
                    .below()
                    .into(),
            );
        }
        buttons.push(
            self.look()
                .face_button(
                    self.glyph(Icon::X, 20, Color::WHITE),
                    32.0,
                    "Close",
                    Message::Image(Event::Close),
                )
                .below()
                .into(),
        );
        let mut shown = Viewer::new(GROUND, picture, size, Message::Image(Event::Close))
            .title(attachment.name.clone(), buttons);
        if images.len() > 1
            && let Some(place) = place
        {
            shown = shown
                .subtitle(format!("{} / {}", place + 1, images.len()))
                .over(self.step_buttons());
        }
        if !note.is_empty() {
            shown = shown.over(
                container(
                    container(text(note).size(13).color(NOTE_INK))
                        .padding([0, 12])
                        .height(32)
                        .align_y(Alignment::Center)
                        .style(|_| rounded(NOTE_GROUND, 6)),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding::ZERO.bottom(20))
                .align_x(Alignment::Center)
                .align_y(Alignment::End),
            );
        }
        Some(shown.view(self.look()))
    }

    /// The round buttons either side that step to the previous and next
    /// picture.
    fn step_buttons(&self) -> Element<'_, Message> {
        let step = |icon: Icon, by: isize| {
            button(self.glyph(icon, 28, Color::WHITE).boxed(STEP))
                .padding(0)
                .width(STEP)
                .height(STEP)
                .on_press(Message::Image(Event::Step(by)))
                .style(move |_, status| {
                    let ground = match status {
                        button::Status::Hovered | button::Status::Pressed => STEP_LIT,
                        _ => STEP_GROUND,
                    };
                    button::Style {
                        background: Some(ground.into()),
                        border: iced::Border::default().rounded(STEP / 2.0),
                        ..button::Style::default()
                    }
                })
        };
        container(
            row![
                step(Icon::ChevronLeft, -1),
                Space::new().width(Length::Fill),
                step(Icon::ChevronRight, 1)
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0.0, STEP_GAP])
        .align_y(Alignment::Center)
        .into()
    }

    /// Keeps the size the viewer opened at, once its picture is known, so
    /// the full picture coming doesn't move it. A viewer whose message the
    /// core let go of (a busy channel lets the oldest go) closes, its full
    /// picture with it, and gives the keys back.
    pub fn viewer_sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        let Some(viewer) = state.viewer.as_ref() else {
            self.viewer_size = None;
            return Task::none();
        };
        if state.message(&viewer.item).is_none() {
            self.client.dispatch(Action::CloseImage);
            self.viewer_size = None;
            return self.refocus();
        }
        if self
            .viewer_size
            .as_ref()
            .is_some_and(|(item, index, _)| *item == viewer.item && *index == viewer.index)
        {
            return Task::none();
        }
        let Some(attachment) = state
            .message(&viewer.item)
            .and_then(|message| message.attachments.get(viewer.index))
        else {
            return Task::none();
        };
        let full = match viewer.pictures.get(&viewer.index) {
            Some(Some(Ok(picture))) => Some((picture.width, picture.height)),
            _ => None,
        };
        if full.is_none() && attachment.dimensions.is_none() && attachment.preview.is_none() {
            return Task::none();
        }
        self.viewer_size = Some((
            viewer.item.clone(),
            viewer.index,
            natural_size(full, attachment),
        ));
        Task::none()
    }

    /// The viewer's keys: Escape closes it, the arrows step.
    pub fn viewer_key(&mut self, named: Option<iced::keyboard::key::Named>) -> Task<Message> {
        use iced::keyboard::key::Named;
        match named {
            Some(Named::Escape) => self.image_event(Event::Close),
            Some(Named::ArrowLeft) => self.image_event(Event::Step(-1)),
            Some(Named::ArrowRight) => self.image_event(Event::Step(1)),
            _ => Task::none(),
        }
    }
}

/// The rows a picture's or video's menu adds, as the Slint client's: Copy
/// Image (a picture with an address), Save, Copy Link and Open Link (with
/// one), each carrying the attachment's place.
pub fn media_rows(index: usize, video: bool, preview: bool, link: bool) -> Vec<MenuRow> {
    let key = index.to_string();
    let mut rows = Vec::new();
    let row = |act, label, icon| MenuRow::new(act, label, icon).key(key.clone());
    if link && !video && !preview {
        rows.push(row(MenuAct::CopyImage, "Copy Image", Icon::Image));
    }
    if !preview {
        rows.push(row(
            MenuAct::SaveMedia,
            if video { "Save Video" } else { "Save Image" },
            Icon::Download,
        ));
    }
    if link {
        rows.push(row(MenuAct::CopyMediaLink, "Copy Link", Icon::Chain));
        rows.push(row(MenuAct::OpenMediaLink, "Open Link", Icon::Link));
    }
    rows
}

/// A picture's full size where it's known, else the size its full picture
/// is fetched at, else its preview's.
fn natural_size(full: Option<(u32, u32)>, attachment: &Attachment) -> Size {
    let (width, height) = full
        .or_else(|| {
            attachment
                .dimensions
                .map(|(width, height)| fit(width, height))
        })
        .or_else(|| {
            attachment
                .preview
                .as_ref()
                .map(|preview| (preview.width, preview.height))
        })
        .unwrap_or((1, 1));
    Size::new(width.max(1) as f32, height.max(1) as f32)
}

/// The size the core fetches a picture at for viewing, as the Slint
/// client's: shrunk to fit the core's viewing size.
fn fit(width: u32, height: u32) -> (u32, u32) {
    use yapfast_core::chat::{VIEW_HEIGHT, VIEW_WIDTH};
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

/// The name a saved file is offered under: the file's own name, without a
/// folder, control or hidden characters, or a leading dot, so a sender
/// can't choose the folder, hide the file or reverse how its extension
/// reads.
pub fn suggested_name(name: &str) -> String {
    let name = Path::new(name)
        .file_name()
        .map(|name| {
            name.to_string_lossy()
                .chars()
                .filter(|&c| !c.is_control() && !links::hidden(c))
                .collect::<String>()
                .trim_start_matches(['.', ' '])
                .to_owned()
        })
        .unwrap_or_default();
    if name.is_empty() {
        "attachment".into()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_files_name_keeps_only_its_own_safe_name() {
        assert_eq!(suggested_name("../../etc/passwd"), "passwd");
        assert_eq!(suggested_name(".hidden"), "hidden");
        assert_eq!(suggested_name("cat\u{202e}gnp.exe"), "catgnp.exe");
        assert_eq!(suggested_name(""), "attachment");
    }

    #[test]
    fn a_picture_menu_offers_what_its_address_allows() {
        let labels = |rows: Vec<MenuRow>| -> Vec<String> {
            rows.into_iter().map(|row| row.label.into_owned()).collect()
        };
        assert_eq!(
            labels(media_rows(0, false, false, true)),
            ["Copy Image", "Save Image", "Copy Link", "Open Link"]
        );
        assert_eq!(
            labels(media_rows(0, true, false, true)),
            ["Save Video", "Copy Link", "Open Link"]
        );
        assert_eq!(labels(media_rows(0, false, false, false)), ["Save Image"]);
        assert_eq!(
            labels(media_rows(0, false, true, true)),
            ["Copy Link", "Open Link"]
        );
    }
}
