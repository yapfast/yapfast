//! The editor's profile card, as the full profile's large card shows it,
//! each part of it editable in place (`ProfileEditorDialog`'s body,
//! `ui/server.slint:1743`-`:2300`): the banner with the avatar over its edge
//! and the status thought, the name, username, pronouns and tag, the
//! buttons, the bio, the dates, the connections and the note. A theme draws
//! the card as the full profile's: its two colours at full strength in a 2 px
//! border, washed inside.

use iced::widget::{Column, column, container, space};
use iced::{Background, Border, Fill, Padding, gradient};

use anomi_theme::prelude::*;
use anomi_vm::people::editor::Editor;
use anomi_vm::people::editor::Intent;
use anomi_vm::people::profile::Place;
use anomi_widgets::list::scroll_view;

use super::{EDGE, fields, head, inset, sections};
use crate::profile::card::look::Inks;

/// The theme's border round the washed card.
const BORDER: f32 = 2.0;

/// The card's measures: the main profile's sits 8 px in from the dialog's
/// edges with its corners 8 px less round, a server's is the dialog itself.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub pad: f32,
    pub banner: f32,
    pub radius: f32,
    pub main: bool,
}

impl Layout {
    pub fn of(editor: &Editor) -> Self {
        Self {
            pad: if editor.main { 20.0 } else { 24.0 },
            banner: if editor.main { 193.0 } else { 210.0 },
            radius: if editor.main { 8.0 } else { 0.0 },
            main: editor.main,
        }
    }

    /// The banner and the avatar's lower half and the room the thought's
    /// bubble keeps under its edge.
    pub fn head(&self) -> f32 {
        self.banner
            + (head::AVATAR / 2.0 + 8.0).max(if self.main { 17.0 + 36.0 + 4.0 } else { 0.0 })
    }
}

/// The card: ground, head and the sections, scrolling when taller than its
/// room.
pub fn view<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let layout = Layout::of(editor);
    let inks = Inks::new(&editor.card, Place::Preview);
    let mut body = Column::<Element<'a, Intent>>::new().spacing(16.0);
    // The body stands EDGE out from the card's margin so that an outlined
    // text keeps its place; the parts without one are inset back.
    if editor.colour_open {
        body = body.push(inset(fields::colour_box(editor, motion)));
    }
    body = body.push(fields::identity(editor, &inks, motion));
    if editor.main {
        body = body.push(inset(sections::actions(&inks)));
    }
    if editor.offer {
        body = body.push(inset(sections::offer(editor, motion)));
    }
    body = body.push(sections::bio(editor, &inks, motion));
    if let Some(since) = sections::since(editor, &inks, motion) {
        body = body.push(inset(since));
    }
    if editor.main {
        body = body.push(inset(sections::connections(editor, &inks)));
        body = body.push(sections::note(editor, &inks));
    }
    if let Some(error) = sections::error(editor) {
        body = body.push(inset(error));
    }
    let page = column![
        head::view(editor, layout, &inks, motion),
        container(body).padding(Padding {
            left: layout.pad - EDGE,
            right: layout.pad - EDGE,
            bottom: layout.pad,
            ..Padding::ZERO
        })
    ];
    let inset = if inks.theme.is_some() { BORDER } else { 0.0 };
    let scrolled = container(scroll_view(page, motion).height(Fill)).padding(inset);
    let mut layers = iced::widget::stack![scrolled].width(Fill).height(Fill);
    if let Some(theme) = inks.theme.filter(|_| editor.main) {
        layers = layers.push_under(wash(theme, layout));
    }
    layers.push_under(ground(&inks, layout)).boxed()
}

/// The ground: the sidebar's in a 1 px edge for the main profile, nothing
/// for a server's; a theme's two colours at full strength.
fn ground<'a>(inks: &Inks, layout: Layout) -> Element<'a, Intent> {
    let radius = layout.radius;
    let theme = inks.theme.filter(|_| layout.main);
    container(space())
        .width(Fill)
        .height(Fill)
        .style(move |theme_: &Theme| {
            let palette = theme_.palette();
            match theme {
                Some(theme) => container::Style {
                    background: Some(Background::Gradient(
                        gradient::Linear::new(iced::Degrees(180.0))
                            .add_stop(0.0, theme.primary)
                            .add_stop(1.0, theme.secondary)
                            .into(),
                    )),
                    border: Border::default().rounded(radius),
                    ..container::Style::default()
                },
                None if layout.main => container::Style {
                    background: Some(Background::Color(palette.sidebar)),
                    border: Border::default()
                        .rounded(radius)
                        .width(1.0)
                        .color(palette.edge),
                    ..container::Style::default()
                },
                None => container::Style::default(),
            }
        })
        .boxed()
}

/// A theme's wash inside its border: flat down to the banner's end, then
/// fading to its second colour.
fn wash<'a>(theme: anomi_vm::people::profile::Theme, layout: Layout) -> Element<'a, Intent> {
    let top = layout.radius - BORDER;
    let flat = container(space())
        .width(Fill)
        .height(layout.banner)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(theme.start)),
            border: Border::default().rounded(iced::border::top(top)),
            ..container::Style::default()
        });
    let fading = container(space())
        .width(Fill)
        .height(Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Gradient(
                gradient::Linear::new(iced::Degrees(180.0))
                    .add_stop(0.0, theme.start)
                    .add_stop(1.0, theme.end)
                    .into(),
            )),
            border: Border::default().rounded(iced::border::bottom(top)),
            ..container::Style::default()
        });
    container(column![flat, fading])
        .padding(BORDER)
        .width(Fill)
        .height(Fill)
        .boxed()
}
