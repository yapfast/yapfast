//! Appearance: the themes as small windows drawn in their own colours (a
//! click shows one at once and keeps it), New theme and each custom theme's
//! pencil opening the editor, whose changes show at once (Save keeps them
//! among the person's own themes, Cancel puts the theme from before back),
//! and the profile switches. See docs/themes.md.

use iced::{
    Alignment, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, stack, text},
};
use yapfast_core::{
    application::Action,
    fonts::{
        BuiltinEmojiFont, BuiltinTextFont, CustomFontEntry, CustomFontKind, EmojiFontChoice,
        TextFontChoice,
    },
    theme::{self as palette, Custom},
};

use super::{Drop, Event as SettingsEvent, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action, lights, placed},
    ctx::{Cx, Pref as Shown, Request},
    fonts::{BOLD, MEDIUM},
    form::{self, switch},
    pictures::Icon,
    themes::color,
};

/// A theme's tile: at least this wide, this tall, this far apart.
const TILE: f32 = 150.0;
const TILE_HEIGHT: f32 = 124.0;
const TILE_GAP: f32 = 12.0;
/// The editor's colours: two columns from this width, each row this tall.
const TWO_COLUMNS: f32 = 520.0;
const COLOUR_GAP: f32 = 24.0;
/// The name field.
const NAME: &str = "theme-name";

/// The theme editor while it's open: the theme as edited, each colour's
/// text and whether it's wrong, its name, and what it started from.
pub struct Editor {
    /// The custom theme edited, by id; none for a new one.
    pub editing: Option<String>,
    pub base: &'static str,
    pub theme: palette::Theme,
    pub name: String,
    /// Each colour's field, in `Theme::NAMES`' order: its name as
    /// `layout.json` has it, its text and whether it's a colour.
    pub fields: Vec<(String, String, bool)>,
    /// The theme a new one started as, among `palette::starts`.
    pub start: usize,
    /// Delete was pressed once; the next press deletes.
    pub confirming: bool,
}

impl Editor {
    fn new(
        editing: Option<String>,
        base: &'static str,
        theme: palette::Theme,
        name: String,
    ) -> Self {
        let mut editor = Self {
            editing,
            base,
            theme,
            name,
            fields: Vec::new(),
            start: 0,
            confirming: false,
        };
        editor.fill();
        editor
    }

    /// Every field's text from the theme's colours.
    fn fill(&mut self) {
        self.fields = palette::Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                Some((
                    palette::token(name),
                    palette::hex(self.theme.colour(index)?),
                    false,
                ))
            })
            .collect();
    }

    /// Takes a colour's text as typed: the theme takes it if it's a colour.
    /// Returns whether the theme changed.
    fn edit(&mut self, index: usize, text: String) -> bool {
        let parsed = palette::parse(&text);
        let Some(field) = self.fields.get_mut(index) else {
            return false;
        };
        (field.1, field.2) = (text, parsed.is_none());
        match (parsed, self.theme.colour_mut(index)) {
            (Some(colour), Some(slot)) if *slot != colour => {
                *slot = colour;
                true
            }
            _ => false,
        }
    }

    fn valid(&self) -> bool {
        self.fields.iter().all(|(_, _, wrong)| !wrong)
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Choose(String),
    New,
    Edit(String),
    Start(usize),
    Colour(usize, String),
    Dark(bool),
    Name(String),
    Save,
    Cancel,
    Delete,
    Profile(Cosmetic, bool),
    ChooseTextFont(TextFontChoice),
    ChooseEmojiFont(EmojiFontChoice),
    ImportFont(CustomFontKind),
    FontImported(Result<CustomFontEntry, String>),
    DeleteCustomFont(String),
}

/// Appearance's profile switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cosmetic {
    Effects,
    Decorations,
    Nameplates,
    Moving,
    NameStyles,
    ServerTags,
    Frames,
}

fn theme_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Theme(event))
}

impl Settings {
    pub(super) fn theme_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Choose(id) => choose(cx, &id),
            Event::New => self.new_theme(cx),
            Event::Edit(id) => {
                let Some(custom) = cx.themes.custom.iter().find(|custom| custom.id == id) else {
                    return Task::none();
                };
                let editor = Editor::new(
                    Some(custom.id.clone()),
                    custom.base,
                    custom.theme,
                    custom.name.clone(),
                );
                self.open_editor(editor, cx);
            }
            Event::Start(index) => {
                let starts: Vec<_> = palette::starts(&cx.themes.custom)
                    .map(|start| (start.base, start.theme, copy_name(start.name)))
                    .collect();
                let Some(editor) = self.editor.as_mut() else {
                    return Task::none();
                };
                let Some((base, theme, name)) = starts.get(index).cloned() else {
                    return Task::none();
                };
                // It takes the new start's name too, unless the person
                // named it.
                if starts
                    .get(editor.start)
                    .is_some_and(|before| before.2 == editor.name)
                {
                    editor.name = name;
                }
                editor.base = base;
                editor.theme = theme;
                editor.start = index;
                editor.fill();
                self.open = None;
                self.show_edited(cx);
            }
            Event::Colour(index, text) => {
                let changed = self
                    .editor
                    .as_mut()
                    .is_some_and(|editor| editor.edit(index, text));
                if changed {
                    self.show_edited(cx);
                }
            }
            Event::Dark(dark) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.theme.dark = dark;
                }
                self.show_edited(cx);
            }
            Event::Name(name) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.name = name;
                }
            }
            Event::Save => self.save_theme(cx),
            Event::Cancel => self.cancel_theme(cx),
            Event::Delete => self.delete_theme(cx),
            Event::Profile(cosmetic, on) => self::cosmetic(cx, cosmetic, on),
            Event::ChooseTextFont(choice) => {
                self.open = None;
                cx.layout.fonts.text = choice;
                cx.keep("fonts", &cx.layout.fonts);
                apply_layout_fonts(cx);
                cx.touch();
            }
            Event::ChooseEmojiFont(choice) => {
                self.open = None;
                cx.layout.fonts.emoji = choice;
                cx.keep("fonts", &cx.layout.fonts);
                apply_layout_fonts(cx);
                cx.touch();
            }
            Event::ImportFont(kind) => return self.import_font(kind, cx),
            Event::FontImported(result) => match result {
                Ok(entry) => {
                    let kind = entry.kind;
                    let id = entry.id.clone();
                    cx.layout.fonts.custom.push(entry);
                    match kind {
                        CustomFontKind::Text => {
                            cx.layout.fonts.text = TextFontChoice::Custom { id };
                        }
                        CustomFontKind::Emoji => {
                            cx.layout.fonts.emoji = EmojiFontChoice::Custom { id };
                        }
                    }
                    cx.keep("fonts", &cx.layout.fonts);
                    apply_layout_fonts(cx);
                    cx.touch();
                }
                Err(err) => {
                    tracing::warn!(%err, "failed to import font");
                }
            },
            Event::DeleteCustomFont(id) => {
                self.open = None;
                let entry = cx
                    .layout
                    .fonts
                    .custom
                    .iter()
                    .position(|c| c.id == id)
                    .map(|pos| cx.layout.fonts.custom.remove(pos));
                if let Some(entry) = entry
                    && let Err(err) = crate::font_manager::delete_font(cx.client.root(), &entry)
                {
                    tracing::warn!(%err, id = %entry.id, "failed to delete custom font file");
                }
                if let TextFontChoice::Custom { id: ref active_id } = cx.layout.fonts.text
                    && active_id == &id
                {
                    cx.layout.fonts.text = TextFontChoice::Builtin {
                        builtin: BuiltinTextFont::DmSans,
                    };
                }
                if let EmojiFontChoice::Custom { id: ref active_id } = cx.layout.fonts.emoji
                    && active_id == &id
                {
                    cx.layout.fonts.emoji = EmojiFontChoice::Builtin {
                        builtin: BuiltinEmojiFont::Twemoji,
                    };
                }
                cx.keep("fonts", &cx.layout.fonts);
                apply_layout_fonts(cx);
                cx.touch();
            }
        }
        Task::none()
    }

    /// Opens the editor on a copy of the theme shown, which can start from
    /// any other.
    fn new_theme(&mut self, cx: &mut Cx<'_>) {
        let current = &cx.theme.id;
        let starts: Vec<_> = palette::starts(&cx.themes.custom)
            .map(|start| {
                (
                    start.id.to_owned(),
                    start.base,
                    start.theme,
                    copy_name(start.name),
                )
            })
            .collect();
        let start = starts
            .iter()
            .position(|(id, ..)| id == current)
            .unwrap_or(0);
        let (_, base, theme, name) = starts[start].clone();
        let mut editor = Editor::new(None, base, theme, name);
        editor.start = start;
        self.open_editor(editor, cx);
    }

    fn open_editor(&mut self, editor: Editor, cx: &mut Cx<'_>) {
        self.editor = Some(editor);
        self.show_edited(cx);
    }

    /// The window in the theme as edited, without keeping it.
    fn show_edited(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let id = editor.editing.as_deref().unwrap_or("editing");
        self.preview = Some(crate::themes::theme(id, &editor.theme));
        cx.request(Request::Changed(Shown::Theme));
    }

    /// Closes the editor: the window shows the theme chosen again.
    fn close_editor(&mut self, cx: &mut Cx<'_>) -> Option<Editor> {
        let editor = self.editor.take()?;
        if self.preview.take().is_some() {
            cx.request(Request::Changed(Shown::Theme));
        }
        Some(editor)
    }

    /// Keeps the theme as edited among the person's own, and chooses it.
    fn save_theme(&mut self, cx: &mut Cx<'_>) {
        if !self.editor.as_ref().is_some_and(Editor::valid) {
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        let name = match editor.name.trim() {
            "" => "My theme".to_owned(),
            name => name.to_owned(),
        };
        let custom = &mut cx.themes.custom;
        let saved = editor
            .editing
            .as_ref()
            .and_then(|id| custom.iter_mut().find(|custom| custom.id == *id));
        let id = match saved {
            Some(saved) => {
                saved.name = name;
                saved.theme = editor.theme;
                saved.id.clone()
            }
            None => {
                let id = palette::new_id(custom);
                custom.push(Custom {
                    id: id.clone(),
                    name,
                    base: editor.base,
                    theme: editor.theme,
                });
                id
            }
        };
        cx.keep("custom_themes", &cx.themes.custom);
        choose(cx, &id);
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, cx: &mut Cx<'_>) {
        self.close_editor(cx);
    }

    /// Deletes the theme edited; if it was the chosen one, Yap shows.
    fn delete_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if !editor.confirming {
            editor.confirming = true;
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        if let Some(id) = editor.editing {
            cx.themes.custom.retain(|custom| custom.id != id);
            cx.keep("custom_themes", &cx.themes.custom);
        }
        let (id, _) = palette::chosen(cx.themes.chosen.as_deref(), &cx.themes.custom);
        let id = id.to_owned();
        choose(cx, &id);
    }

    fn import_font(&self, kind: CustomFontKind, cx: &Cx<'_>) -> Task<Message> {
        let root = cx.client.root().to_path_buf();
        Task::perform(
            async move {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title(match kind {
                        CustomFontKind::Text => "Import Font",
                        CustomFontKind::Emoji => "Import Emoji Font",
                    })
                    .add_filter("Font Files", &["ttf", "otf"])
                    .pick_file()
                    .await?;
                let path = handle.path().to_owned();
                Some(crate::font_manager::import_font(&root, &path, kind))
            },
            move |result| match result {
                Some(res) => theme_message(Event::FontImported(res)),
                None => Message::Nothing,
            },
        )
    }
}

/// Chooses the theme `id` and keeps the choice, if there's such a theme:
/// the window shows it.
fn choose(cx: &mut Cx<'_>, id: &str) {
    let Some(theme) = cx.themes.find(id) else {
        return;
    };
    cx.themes.choose(cx.client.root(), &theme.id);
    cx.request(Request::Changed(Shown::Theme));
}

/// Updates the font system with the fonts chosen in layout.
fn apply_layout_fonts(cx: &Cx<'_>) {
    crate::font_manager::apply_font_prefs(cx.client.root(), &cx.layout.fonts);
}

/// A profile switch: kept, and the core told what may now show.
fn cosmetic(cx: &mut Cx<'_>, cosmetic: Cosmetic, on: bool) {
    let prefs = &mut cx.layout.prefs;
    let (slot, key) = match cosmetic {
        Cosmetic::Effects => (&mut prefs.profile_effects, "profile_effects"),
        Cosmetic::Decorations => (&mut prefs.decorations, "decorations"),
        Cosmetic::Nameplates => (&mut prefs.nameplates, "nameplates"),
        Cosmetic::Moving => (&mut prefs.moving_cosmetics, "moving_cosmetics"),
        Cosmetic::NameStyles => (&mut prefs.name_styles, "name_styles"),
        Cosmetic::ServerTags => (&mut prefs.server_tags, "server_tags"),
        Cosmetic::Frames => (&mut prefs.profile_frames, "profile_frames"),
    };
    *slot = Some(on);
    cx.keep(key, on);
    if cosmetic == Cosmetic::Nameplates {
        cx.dispatch(Action::HideNameplates(!on));
    } else {
        let cosmetics = cx.layout.prefs.cosmetics();
        cx.dispatch(Action::SetCosmetics(cosmetics));
    }
    cx.request(Request::Changed(Shown::Roster));
}

impl<'a> Page<'a> {
    pub(super) fn appearance_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        if let Some(editor) = &settings.editor {
            return self.editor_view(editor, settings.open == Some(Drop::Start));
        }
        let tiles = self.theme_tiles();
        let prefs = &self.cx.layout.prefs;
        let cosmetics = prefs.cosmetics();
        let profile = |label, description, on: bool, which| {
            switch(label, on, Some(theme_message(Event::Profile(which, !on))))
                .description(description)
                .view(self.cx.look)
        };
        form::page([
            form::title(theme, "Appearance"),
            form::group([
                form::beside(
                    form::heading(theme, "THEME"),
                    action(theme, "New theme", Kind::Plain, Some(theme_message(Event::New)))
                        .small()
                        .icon(self.cx.look.icon(Icon::Plus, 16, crate::theme::rgb(theme.text))),
                ),
                tiles,
            ]),
            self.title_bar_switch(),
            column![
                form::heading(theme, "PROFILES"),
                profile(
                    "Play profile effects",
                    "The animations some people put over their profile cards.",
                    cosmetics.effects,
                    Cosmetic::Effects,
                ),
                profile(
                    "Show avatar decorations",
                    "The pictures around people's avatars.",
                    cosmetics.decorations,
                    Cosmetic::Decorations,
                ),
                profile(
                    "Show nameplates",
                    "The pictures Discord members equip behind their rows in the member list and on your account card.",
                    prefs.nameplates.unwrap_or(true),
                    Cosmetic::Nameplates,
                ),
                profile(
                    "Animate decorations and nameplates",
                    "They play when pointed at, as in Discord. Off, they hold still.",
                    cosmetics.moving,
                    Cosmetic::Moving,
                ),
                profile(
                    "Show name styles",
                    "The fonts and colours people give their display names.",
                    cosmetics.name_styles,
                    Cosmetic::NameStyles,
                ),
                profile(
                    "Show server tags",
                    "The tag and its badge beside people's names.",
                    cosmetics.server_tags,
                    Cosmetic::ServerTags,
                ),
                profile(
                    "Show profile frames",
                    "The frames some people put around their profile cards.",
                    cosmetics.frames,
                    Cosmetic::Frames,
                ),
            ]
            .spacing(12)
            .into(),
            self.fonts_section(),
        ])
    }

    fn fonts_section(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let fonts = &self.cx.layout.fonts;

        let custom_text_fonts: Vec<&CustomFontEntry> = fonts
            .custom
            .iter()
            .filter(|c| c.kind == CustomFontKind::Text)
            .collect();
        let custom_emoji_fonts: Vec<&CustomFontEntry> = fonts
            .custom
            .iter()
            .filter(|c| c.kind == CustomFontKind::Emoji)
            .collect();

        let mut text_options = vec![
            BuiltinTextFont::DmSans.label().to_string(),
            BuiltinTextFont::Inter.label().to_string(),
            BuiltinTextFont::SystemDefault.label().to_string(),
        ];
        for custom in &custom_text_fonts {
            text_options.push(custom.name.clone());
        }

        let text_current = match &fonts.text {
            TextFontChoice::Builtin { builtin } => match builtin {
                BuiltinTextFont::DmSans => Some(0),
                BuiltinTextFont::Inter => Some(1),
                BuiltinTextFont::SystemDefault => Some(2),
            },
            TextFontChoice::Custom { id } => custom_text_fonts
                .iter()
                .position(|c| c.id == *id)
                .map(|idx| 3 + idx),
        };

        let custom_text_ids: Vec<String> = custom_text_fonts.iter().map(|c| c.id.clone()).collect();
        let text_dropdown = form::dropdown(
            text_options,
            text_current,
            self.s.open == Some(Drop::TextFont),
            Message::Settings(SettingsEvent::Drop(Drop::TextFont)),
            move |index| match index {
                0 => theme_message(Event::ChooseTextFont(TextFontChoice::Builtin {
                    builtin: BuiltinTextFont::DmSans,
                })),
                1 => theme_message(Event::ChooseTextFont(TextFontChoice::Builtin {
                    builtin: BuiltinTextFont::Inter,
                })),
                2 => theme_message(Event::ChooseTextFont(TextFontChoice::Builtin {
                    builtin: BuiltinTextFont::SystemDefault,
                })),
                idx => {
                    if let Some(id) = custom_text_ids.get(idx - 3) {
                        theme_message(Event::ChooseTextFont(TextFontChoice::Custom {
                            id: id.clone(),
                        }))
                    } else {
                        Message::Nothing
                    }
                }
            },
        )
        .view(self.cx.look);

        let import_text_button = action(
            theme,
            "Import Font...",
            Kind::Plain,
            Some(theme_message(Event::ImportFont(CustomFontKind::Text))),
        )
        .icon(
            self.cx
                .look
                .icon(Icon::Plus, 16, crate::theme::rgb(theme.text)),
        );

        let mut custom_text_list = column![].spacing(6);
        for font in &custom_text_fonts {
            let is_active = match &fonts.text {
                TextFontChoice::Custom { id } => id == &font.id,
                _ => false,
            };
            custom_text_list = custom_text_list.push(
                row![
                    text(font.name.clone())
                        .size(14)
                        .font(if is_active { BOLD } else { MEDIUM })
                        .color(if is_active { theme.accent } else { theme.text })
                        .width(Length::Fill),
                    self.cx
                        .look
                        .icon_button(
                            Icon::Trash,
                            "Delete font",
                            theme_message(Event::DeleteCustomFont(font.id.clone())),
                        )
                        .size(24.0, 14),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding(iced::Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 2.0,
                    bottom: 2.0,
                }),
            );
        }

        let mut emoji_options = vec![
            BuiltinEmojiFont::Twemoji.label().to_string(),
            BuiltinEmojiFont::SystemDefault.label().to_string(),
        ];
        for custom in &custom_emoji_fonts {
            emoji_options.push(custom.name.clone());
        }

        let emoji_current = match &fonts.emoji {
            EmojiFontChoice::Builtin { builtin } => match builtin {
                BuiltinEmojiFont::Twemoji => Some(0),
                BuiltinEmojiFont::SystemDefault => Some(1),
            },
            EmojiFontChoice::Custom { id } => custom_emoji_fonts
                .iter()
                .position(|c| c.id == *id)
                .map(|idx| 2 + idx),
        };

        let custom_emoji_ids: Vec<String> =
            custom_emoji_fonts.iter().map(|c| c.id.clone()).collect();
        let emoji_dropdown = form::dropdown(
            emoji_options,
            emoji_current,
            self.s.open == Some(Drop::EmojiFont),
            Message::Settings(SettingsEvent::Drop(Drop::EmojiFont)),
            move |index| match index {
                0 => theme_message(Event::ChooseEmojiFont(EmojiFontChoice::Builtin {
                    builtin: BuiltinEmojiFont::Twemoji,
                })),
                1 => theme_message(Event::ChooseEmojiFont(EmojiFontChoice::Builtin {
                    builtin: BuiltinEmojiFont::SystemDefault,
                })),
                idx => {
                    if let Some(id) = custom_emoji_ids.get(idx - 2) {
                        theme_message(Event::ChooseEmojiFont(EmojiFontChoice::Custom {
                            id: id.clone(),
                        }))
                    } else {
                        Message::Nothing
                    }
                }
            },
        )
        .view(self.cx.look);

        let import_emoji_button = action(
            theme,
            "Import Emoji Font...",
            Kind::Plain,
            Some(theme_message(Event::ImportFont(CustomFontKind::Emoji))),
        )
        .icon(
            self.cx
                .look
                .icon(Icon::Plus, 16, crate::theme::rgb(theme.text)),
        );

        let mut custom_emoji_list = column![].spacing(6);
        for font in &custom_emoji_fonts {
            let is_active = match &fonts.emoji {
                EmojiFontChoice::Custom { id } => id == &font.id,
                _ => false,
            };
            custom_emoji_list = custom_emoji_list.push(
                row![
                    text(font.name.clone())
                        .size(14)
                        .font(if is_active { BOLD } else { MEDIUM })
                        .color(if is_active { theme.accent } else { theme.text })
                        .width(Length::Fill),
                    self.cx
                        .look
                        .icon_button(
                            Icon::Trash,
                            "Delete font",
                            theme_message(Event::DeleteCustomFont(font.id.clone())),
                        )
                        .size(24.0, 14),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding(iced::Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 2.0,
                    bottom: 2.0,
                }),
            );
        }

        let preview_card = container(
            column![
                form::note(theme, "PREVIEW"),
                text("The quick brown fox jumps over the lazy dog. 🦊🎉❤️ 1234567890")
                    .size(16)
                    .color(theme.text),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(move |_| crate::style::rounded(theme.field, 8.0));

        let mut font_controls = column![
            form::heading(theme, "FONTS"),
            form::note(theme, "TEXT FONT"),
            row![
                container(text_dropdown).width(Length::Fill),
                import_text_button,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        if !custom_text_fonts.is_empty() {
            font_controls = font_controls.push(custom_text_list);
        }

        font_controls = font_controls.push(Space::new().height(8));
        font_controls = font_controls.push(form::note(theme, "EMOJI FONT"));
        font_controls = font_controls.push(
            row![
                container(emoji_dropdown).width(Length::Fill),
                import_emoji_button,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if !custom_emoji_fonts.is_empty() {
            font_controls = font_controls.push(custom_emoji_list);
        }

        font_controls = font_controls.push(Space::new().height(12));
        font_controls = font_controls.push(preview_card);

        font_controls.into()
    }

    /// The themes as tiles, as many 150 px or wider across as fit.
    fn theme_tiles(self) -> Element<'a, Message> {
        iced::widget::responsive(move |size| {
            let columns = ((size.width + TILE_GAP) / (TILE + TILE_GAP))
                .floor()
                .max(1.0) as usize;
            let width = (size.width - (columns - 1) as f32 * TILE_GAP) / columns as f32;
            let cards: Vec<(String, String, bool, palette::Theme)> =
                palette::BUILT_IN
                    .iter()
                    .map(|built_in| {
                        (
                            built_in.id.to_owned(),
                            built_in.name.to_owned(),
                            false,
                            built_in.theme,
                        )
                    })
                    .chain(
                        self.cx.themes.custom.iter().map(|custom| {
                            (custom.id.clone(), custom.name.clone(), true, custom.theme)
                        }),
                    )
                    .collect();
            let mut rows = column![].spacing(TILE_GAP);
            for chunk in cards.chunks(columns) {
                let mut line = row![].spacing(TILE_GAP);
                for (id, name, custom, palette) in chunk {
                    line = line.push(self.theme_tile(id, name, *custom, palette, width));
                }
                rows = rows.push(line);
            }
            rows.into()
        })
        .height(Length::Shrink)
        .into()
    }

    /// A theme to choose: a small window in its colours, its name under it,
    /// an accent ring and a tick on the one shown, and a pencil on the
    /// person's own.
    fn theme_tile(
        self,
        id: &str,
        name: &str,
        custom: bool,
        palette: &palette::Theme,
        width: f32,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let current = self.cx.themes.chosen_id() == id;
        let preview = preview(palette, width - 16.0);
        let mut label = row![
            text(name.to_owned())
                .size(14)
                .font(if current { BOLD } else { MEDIUM })
                .color(if current { theme.text } else { theme.secondary })
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .height(24);
        if current {
            label = label.push(self.cx.look.glyph(Icon::Check, 16, theme.accent));
        }
        let (accent, faint, line, hover) = (theme.accent, theme.faint, theme.line, theme.hover);
        let face = button(
            column![preview, container(label).padding([0, 2])]
                .spacing(6)
                .padding(8),
        )
        .padding(0)
        .width(width)
        .height(TILE_HEIGHT)
        .on_press(theme_message(Event::Choose(id.to_owned())))
        .style(move |_, status| {
            let lit = lights(status);
            button::Style {
                background: lit.then(|| hover.into()),
                border: Border {
                    color: if current {
                        accent
                    } else if lit {
                        faint
                    } else {
                        line
                    },
                    width: if current { 2.0 } else { 1.0 },
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        });
        if !custom {
            return face.into();
        }
        let pencil = self
            .cx
            .look
            .icon_button(
                Icon::Pencil,
                "Edit",
                theme_message(Event::Edit(id.to_owned())),
            )
            .size(24.0, 14);
        stack![face, placed(pencil, width - 8.0 - 24.0, 8.0 + 78.0 + 6.0)].into()
    }

    /// The editor: the name with Save and Cancel, what a new theme starts
    /// from, Dark, each colour's field, and Delete for a saved one.
    fn editor_view(self, editor: &'a Editor, starts_open: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut parts = vec![
            form::title(theme, "Appearance"),
            form::group([
                form::heading(
                    theme,
                    if editor.editing.is_some() {
                        "EDIT THEME"
                    } else {
                        "NEW THEME"
                    },
                ),
                row![
                    form::field(theme, NAME, "Name", &editor.name)
                        .on_input(|name| theme_message(Event::Name(name)))
                        .on_submit(theme_message(Event::Save)),
                    action(
                        theme,
                        "Save",
                        Kind::Primary,
                        editor.valid().then(|| theme_message(Event::Save))
                    ),
                    action(
                        theme,
                        "Cancel",
                        Kind::Plain,
                        Some(theme_message(Event::Cancel))
                    )
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ]),
        ];
        if editor.editing.is_none() {
            let names: Vec<String> = palette::starts(&self.cx.themes.custom)
                .map(|start| start.name.to_owned())
                .collect();
            parts.push(form::group([
                form::heading(theme, "START FROM"),
                form::dropdown(
                    names,
                    Some(editor.start),
                    starts_open,
                    Message::Settings(SettingsEvent::Drop(Drop::Start)),
                    |index| theme_message(Event::Start(index)),
                )
                .view(self.cx.look),
            ]));
        }
        parts.push(
            switch(
                "Dark",
                editor.theme.dark,
                Some(theme_message(Event::Dark(!editor.theme.dark))),
            )
            .description("Drop-down lists and scroll bars draw dark to match.")
            .view(self.cx.look),
        );
        let colours = iced::widget::responsive(move |size| {
            let columns = if size.width >= TWO_COLUMNS { 2 } else { 1 };
            let per = editor.fields.len().div_ceil(columns);
            let mut line = row![].spacing(COLOUR_GAP);
            for column_index in 0..columns {
                let mut cells = column![];
                for index in column_index * per..((column_index + 1) * per).min(editor.fields.len())
                {
                    let (name, hex, wrong) = &editor.fields[index];
                    let shown = editor.theme.colour(index).map_or(Color::TRANSPARENT, color);
                    cells = cells.push(form::colour_row(
                        theme,
                        name,
                        shown,
                        hex,
                        *wrong,
                        move |text| theme_message(Event::Colour(index, text)),
                    ));
                }
                line = line.push(container(cells).width(Length::FillPortion(1)));
            }
            line.into()
        })
        .height(Length::Shrink);
        parts.push(form::group([
            form::heading(theme, "COLOURS"),
            form::note(
                theme,
                "Each as #rgb, #rrggbb or #rrggbbaa. The window shows a change as you type it.",
            ),
            colours.into(),
        ]));
        if editor.editing.is_some() {
            parts.push(form::buttons([action(
                theme,
                if editor.confirming {
                    "Delete it for good"
                } else {
                    "Delete theme"
                },
                Kind::Danger,
                Some(theme_message(Event::Delete)),
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Trash, 16, crate::theme::rgb(theme.on_danger)),
            )
            .into()]));
        }
        form::page(parts)
    }
}

/// What a theme copied from another is called until it's named.
fn copy_name(name: &str) -> String {
    format!("{name} copy")
}

/// A theme's small window, 78 px tall: the rail with three servers, the
/// sidebar's channels, two messages and the composer, in its colours.
fn preview<'a>(palette: &palette::Theme, width: f32) -> Element<'a, Message> {
    let c = |colour| color(colour);
    let (rail, sidebar, chat, raised, accent, ink, muted) = (
        c(palette.rail),
        c(palette.sidebar),
        c(palette.chat),
        c(palette.raised),
        c(palette.accent),
        c(palette.text),
        c(palette.muted),
    );
    let blob = |w: f32, h: f32, colour: Color, radius: f32| -> Element<'a, Message> {
        container(Space::new())
            .width(w)
            .height(h)
            .style(move |_| crate::style::rounded(colour, radius))
            .into()
    };
    let mut layers = stack![blob(width, 78.0, chat, 6.0)];
    layers = layers.push(blob(16.0, 78.0, rail, 0.0));
    for index in 0..3 {
        let (colour, radius) = if index == 0 {
            (accent, 3.0)
        } else {
            (raised, 5.0)
        };
        layers = layers.push(placed(
            blob(10.0, 10.0, colour, radius),
            3.0,
            6.0 + index as f32 * 13.0,
        ));
    }
    layers = layers.push(placed(blob(38.0, 78.0, sidebar, 0.0), 16.0, 0.0));
    for index in 0..4 {
        let w = if index == 1 {
            24.0
        } else {
            18.0 + (index % 2) as f32 * 4.0
        };
        let colour = if index == 1 { ink } else { muted };
        layers = layers.push(placed(
            blob(w, 4.0, colour, 2.0),
            22.0,
            8.0 + index as f32 * 11.0,
        ));
    }
    let room = width - 66.0;
    for index in 0..2 {
        let y = 8.0 + index as f32 * 22.0;
        let first = index == 0;
        layers = layers
            .push(placed(
                blob(10.0, 10.0, if first { accent } else { muted }, 5.0),
                60.0,
                y,
            ))
            .push(placed(
                blob(if first { 22.0 } else { 16.0 }, 4.0, ink, 2.0),
                74.0,
                y + 1.0,
            ))
            .push(placed(
                blob(
                    (room - 14.0).min(if first { 52.0 } else { 40.0 }),
                    3.0,
                    muted,
                    1.5,
                ),
                74.0,
                y + 8.0,
            ));
    }
    layers = layers
        .push(placed(blob(room, 12.0, raised, 4.0), 60.0, 78.0 - 18.0))
        .push(placed(
            blob(6.0, 6.0, accent, 3.0),
            60.0 + room - 10.0,
            78.0 - 18.0,
        ));
    container(layers.width(width).height(78))
        .clip(true)
        .style(|_| iced::widget::container::Style {
            border: crate::style::round(6),
            ..Default::default()
        })
        .into()
}

impl<'a> Page<'a> {
    /// Appearance's "Use the system title bar", where Yapfast can draw its
    /// own: the desktop's way until it's chosen.
    pub(super) fn title_bar_switch(self) -> Element<'a, Message> {
        if !crate::window_bar::offered() {
            return Space::new().into();
        }
        let system = !self.cx.own_title_bar;
        switch(
            "Use the system title bar",
            system,
            Some(Message::Settings(SettingsEvent::Pref(
                super::pages::Pref::SystemTitleBar(!system),
            ))),
        )
        .description("Your desktop draws the window's title bar and buttons, as on its other windows. Off, Yapfast draws its own.")
        .view(self.cx.look)
    }
}
