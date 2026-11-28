//! Appearance's Font: the person's own font files, which the whole window is
//! drawn in while some are loaded. The files are the kit's
//! ([`custom_font`]); this holds the page's list and buttons, and keeps the
//! files' paths in `layout.json` (`ui_font_files`) for the next start. See
//! docs/themes.md.

use iced::{
    Alignment, Element, Length, Task,
    widget::{column, row, text},
};

use super::{Event, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Cx, Pref as Shown, Request},
    custom_font::{self, Entry},
    form,
    pictures::Icon,
};

/// What Font's buttons do.
#[derive(Clone, Debug)]
pub enum FontEvent {
    /// Opens the system's file dialog.
    Choose,
    /// The files it gave, by their paths.
    Chosen(Vec<String>),
    /// Takes a file out, by its place in the list.
    Remove(usize),
    /// Goes back to DM Sans.
    Reset,
}

fn font_message(event: FontEvent) -> Message {
    crate::app::settings(Event::Font(event))
}

impl Settings {
    pub(super) fn font_event(&mut self, event: FontEvent, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            FontEvent::Choose => return choose_files(),
            FontEvent::Chosen(files) => self.add_fonts(&files, cx),
            FontEvent::Remove(index) => {
                let mut paths = custom_font::paths();
                if index < paths.len() {
                    paths.remove(index);
                    self.font_note = None;
                    use_files(&paths, cx);
                }
            }
            FontEvent::Reset => {
                self.font_note = None;
                use_files(&[], cx);
            }
        }
        Task::none()
    }

    /// Adds the files chosen to the ones loaded. A file that isn't a usable
    /// font is left out, and the note says why.
    fn add_fonts(&mut self, chosen: &[String], cx: &mut Cx<'_>) {
        let mut paths = custom_font::paths();
        paths.extend(chosen.iter().cloned());
        use_files(&paths, cx);
        let failed = custom_font::drop_failed(chosen);
        if failed.is_empty() {
            self.font_note = None;
            return;
        }
        keep_paths(cx);
        let named = |entry: &Entry| match &entry.outcome {
            Err(problem) => format!("{}: {problem}", entry.name()),
            Ok(_) => entry.name().to_owned(),
        };
        self.font_note = match failed.as_slice() {
            [one] => Some(named(one)),
            [first, rest @ ..] => Some(format!(
                "{} And {} more weren't loaded.",
                named(first),
                rest.len()
            )),
            [] => None,
        };
    }
}

/// The system's file dialog for font files: `FontEvent::Chosen` with the
/// files, or nothing if it's closed.
fn choose_files() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Choose font files")
                .add_filter("Fonts", &["ttf", "otf", "ttc", "woff"])
                .pick_files()
                .await
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|file| file.path().to_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
        },
        |files| match files {
            Some(files) if !files.is_empty() => font_message(FontEvent::Chosen(files)),
            _ => Message::Nothing,
        },
    )
}

/// Loads `paths` in place of the files loaded and keeps them chosen: the
/// window shapes its text again, and the timeline's rows are measured
/// again, as when their text size changes.
fn use_files(paths: &[String], cx: &mut Cx<'_>) {
    custom_font::set(paths);
    keep_paths(cx);
    cx.request(Request::Changed(Shown::Rows));
    cx.touch();
}

/// The files chosen, as the next start loads them.
fn keep_paths(cx: &mut Cx<'_>) {
    cx.layout.prefs.ui_font_files = custom_font::paths();
    cx.keep("ui_font_files", &cx.layout.prefs.ui_font_files);
}

impl<'a> Page<'a> {
    /// The Font group: what it does, a row for each file chosen with what
    /// it gave, a note when some didn't load, and the buttons.
    pub(super) fn font_section(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let entries = custom_font::entries();
        let note = self.s.font_note.clone().unwrap_or_else(custom_font::note);
        let mut parts: Vec<Element<'a, Message>> = vec![
            form::heading(theme, crate::i18n::tr!(self.cx.i18n, "FONT")),
            form::note(
                theme,
                "Draw all of Yapfast in your own font. Choose its TTF, OTF, TTC or WOFF files, every weight and italic you have; WOFF2 isn't supported. Code and emoji keep their fonts.",
            ),
        ];
        let any = !entries.is_empty();
        for (index, entry) in entries.iter().enumerate() {
            let failed = entry.outcome.is_err();
            parts.push(
                column![
                    row![
                        text(entry.name().to_owned())
                            .size(15)
                            .color(theme.text)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::None),
                        action(
                            theme,
                            "Remove",
                            Kind::Plain,
                            Some(font_message(FontEvent::Remove(index)))
                        )
                        .small()
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(entry.detail()).size(12).color(if failed {
                        theme.danger
                    } else {
                        theme.muted
                    })
                ]
                .spacing(2)
                .into(),
            );
        }
        if !note.is_empty() {
            parts.push(form::note(theme, note));
        }
        let mut buttons = vec![
            action(
                theme,
                "Choose font files",
                Kind::Plain,
                Some(font_message(FontEvent::Choose)),
            )
            .icon(
                self.cx
                    .look
                    .icon(Icon::Plus, 16, crate::theme::rgb(theme.text)),
            )
            .into(),
        ];
        if any {
            buttons.push(
                action(
                    theme,
                    "Reset to default",
                    Kind::Plain,
                    Some(font_message(FontEvent::Reset)),
                )
                .into(),
            );
        }
        parts.push(form::buttons(buttons));
        column(parts).spacing(8).into()
    }
}
