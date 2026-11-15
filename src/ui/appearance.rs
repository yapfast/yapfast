//! Appearance: choosing a theme, and making and editing the person's own.

use super::*;
use yapfast_core::themes::Performance;
use yapfast_core::themes::media::SoundSlot;
use yapfast_core::themes::options;
use yapfast_ui::{ThemeOptionRow, ThemeOptions};

/// One option as Settings draws it.
fn option_row(option: &options::ThemeOption, value: &serde_json::Value) -> ThemeOptionRow {
    let mut row = ThemeOptionRow {
        id: option.id.as_str().into(),
        label: option.label.as_str().into(),
        description: option.description.as_str().into(),
        ..ThemeOptionRow::default()
    };
    match &option.kind {
        options::Kind::Toggle { .. } => row.on = value.as_bool().unwrap_or(false),
        options::Kind::Choice { choices, .. } => {
            row.kind = 1;
            row.choices = ModelRc::new(VecModel::from(
                choices
                    .iter()
                    .map(|choice| SharedString::from(choice.label.as_str()))
                    .collect::<Vec<_>>(),
            ));
            row.current = choices
                .iter()
                .position(|choice| Some(choice.value.as_str()) == value.as_str())
                .unwrap_or(0) as i32;
        }
        options::Kind::Slider { min, max, step, .. } => {
            row.kind = 2;
            row.steps = ((max - min) / step).round() as i32;
            row.current = ((value.as_f64().unwrap_or(*min) - min) / step).round() as i32;
            row.minimum = *min as f32;
            row.step = *step as f32;
            if option.id == options::SOUNDS {
                row.unit = "%".into();
                row.zero_off = true;
            }
        }
        options::Kind::Hue { .. } => {
            row.kind = 3;
            row.current = value.as_f64().unwrap_or(0.0).round() as i32;
        }
    }
    row
}

impl Adapter {
    /// Shows the window in a theme, built in or the person's own, and keeps
    /// it chosen. False for an id Yapfast doesn't know, which changes nothing.
    pub(super) fn choose_theme(&mut self, window: &YapfastWindow, id: &str) -> bool {
        // An installed theme is made ready on another thread, the window
        // keeping the theme it shows until then, and comes back here.
        if theme_look::is_package(id) && !theme_look::is_ready(id) {
            super::theme_timing::start("choose");
            let again = SharedString::from(id);
            theme_look::prepare(window, id, move |window| window.invoke_choose_theme(again));
            return true;
        }
        let shown = if theme_look::is_package(id) {
            match theme_look::show(window, id) {
                Ok(colours) => Some((id.to_owned(), colours)),
                Err(reason) => {
                    // The theme that was showing stays; the person hears why.
                    self.app.state.notice = Some(format!("That theme couldn't be shown: {reason}"));
                    self.refresh(window);
                    return false;
                }
            }
        } else {
            theme::lookup(id, &self.layout.custom_themes).map(|(id, theme)| (id.to_owned(), theme))
        };
        let Some((id, chosen)) = shown else {
            return false;
        };
        if !theme_look::is_package(&id) {
            // An installed theme still being made ready would land over it.
            theme_look::cancel_prepare();
            theme_look::clear(window);
        }
        window.set_theme_id(id.as_str().into());
        self.set_theme(window, &chosen);
        if id.starts_with(theme_look::INSTALLED) {
            theme_media::cue(SoundSlot::ThemeApplied, FOCUSED.get());
            // Settings → Themes, if it's open, marks the theme now applied.
            theme_look::mark_applied(window);
        }
        if self.layout.theme.as_deref() != Some(id.as_str()) {
            self.layout.previous_theme = self.layout.theme.take();
        }
        self.layout.theme = Some(id);
        self.layout.write(&self.root);
        self.show_previous_theme(window);
        super::theme_timing::values_set();
        true
    }

    /// Settings → Themes' Back, to the theme shown before the last one
    /// chosen, and Appearance's line naming the installed theme shown.
    pub(super) fn show_previous_theme(&self, window: &YapfastWindow) {
        let name = self
            .layout
            .previous_theme
            .as_deref()
            .filter(|previous| self.layout.theme.as_deref() != Some(*previous))
            .and_then(|previous| theme_look::name_of(previous, &self.layout.custom_themes));
        window
            .global::<yapfast_ui::ThemesPage>()
            .set_previous_name(name.unwrap_or_default().into());
        let installed = theme_look::applied_id()
            .is_some_and(|id| id.starts_with(theme_look::INSTALLED) || id == "dev");
        window.set_theme_installed_name(
            theme_look::applied_name()
                .filter(|_| installed)
                .unwrap_or_default()
                .into(),
        );
    }

    /// Ctrl+Shift+T: the default theme until Yapfast starts again; the
    /// chosen theme stays chosen.
    pub(super) fn default_theme_for_session(&mut self, window: &YapfastWindow) {
        theme_look::default_for_session(window);
        let default = window
            .global::<ClientStyle>()
            .get_enabled()
            .then_some("bonfire");
        let (id, chosen) = theme::chosen(default, &[]);
        window.set_theme_id(id.into());
        self.set_theme(window, &chosen);
        self.show_previous_theme(window);
        self.app.state.notice = Some(
            "Showing the default theme until Yapfast starts again (Ctrl+Shift+T). Choose any theme in Settings → Appearance."
                .to_owned(),
        );
        self.refresh(window);
    }

    /// Shows what a theme on trial or `--theme-dev` came back to: a theme
    /// by its id, without choosing it.
    pub(super) fn show_theme(&mut self, window: &YapfastWindow, id: Option<&str>) {
        let default = window
            .global::<ClientStyle>()
            .get_enabled()
            .then_some("bonfire");
        if let Some(id) = id.filter(|id| theme_look::is_package(id))
            && let Ok(colours) = theme_look::show(window, id)
        {
            window.set_theme_id(id.into());
            self.set_theme(window, &colours);
            self.show_previous_theme(window);
            return;
        }
        theme_look::clear(window);
        let (id, chosen) = theme::chosen(
            id.filter(|id| {
                !theme_look::is_package(id)
                    || theme::lookup(id, &self.layout.custom_themes).is_some()
            })
            .or(default),
            &self.layout.custom_themes,
        );
        window.set_theme_id(id.into());
        self.set_theme(window, &chosen);
        self.show_previous_theme(window);
    }

    /// The system went light or dark while an installed theme shows: it
    /// shows again in the variant that now fits, if it ships both.
    pub(super) fn system_mode_changed(&mut self, window: &YapfastWindow) {
        let Some(id) = theme_look::applied_id()
            .filter(|id| id.starts_with(theme_look::INSTALLED) || id == "dev")
        else {
            return;
        };
        if let Ok(colours) = theme_look::show(window, &id) {
            self.set_theme(window, &colours);
        }
    }

    /// Refreshes the Themes tab and Appearance's cards after the installed
    /// themes changed.
    fn themes_changed(&mut self, window: &YapfastWindow) {
        theme::show_cards(window, &self.layout.custom_themes);
        theme_look::show_page(window, &theme_look::marketplace(window));
    }

    /// Shows the window in another theme's colours, building again what
    /// carries the old mention colour in its markup: message rows, the
    /// topic, pins and search results.
    pub(super) fn set_theme(&mut self, window: &YapfastWindow, theme: &theme::Theme) {
        let icons = self.show_theme_media(window);
        self.show_theme_options(window);
        let content = self.show_theme_content(window);
        super::theme_chrome::workspaces(&self.app.state);
        super::theme_chrome::now_playing(window, &self.app.state);
        super::theme_timing::step("media, options and content shown");
        if theme.apply(window) || icons || content {
            super::theme_timing::step("colours set");
            self.restyle_mentions();
            self.refresh(window);
            super::theme_timing::step("refreshed");
        }
    }

    /// Settings' list of the shown installed theme's options, as the person
    /// has them.
    pub(super) fn show_theme_options(&self, window: &YapfastWindow) {
        let rows: Vec<ThemeOptionRow> = theme_look::applied_options()
            .iter()
            .map(|(option, value)| option_row(option, value))
            .collect();
        let global = window.global::<ThemeOptions>();
        global.set_theme(theme_look::applied_name().unwrap_or_default().into());
        global.set_rows(ModelRc::new(VecModel::from(rows)));
        theme_media::set_scale(theme_look::sounds_scale());
    }

    /// Keeps a new value for one of the shown theme's options and shows the
    /// theme with it: made ready off the UI thread, as choosing a theme is,
    /// the theme as it was staying on the window until then. `pick` gives
    /// the value from the option's kind, or nothing for one that doesn't fit.
    pub(super) fn change_theme_option(
        &mut self,
        window: &YapfastWindow,
        option: &str,
        pick: impl FnOnce(&options::Kind) -> Option<serde_json::Value>,
    ) {
        super::theme_timing::start("option");
        let Some(id) = theme_look::applied_id() else {
            return;
        };
        let Some((option, _)) = theme_look::applied_options()
            .into_iter()
            .find(|(known, _)| known.id == option)
        else {
            return;
        };
        let Some(value) = pick(&option.kind) else {
            return;
        };
        let mut chosen = self.layout.theme_options.remove(&id).unwrap_or_default();
        chosen.insert(option.id.clone(), value);
        let value = option.value(&chosen);
        // Only what differs from the default is kept; "Ransom names" also
        // remembers on when the old Plain names switch was on.
        let legacy = option.id == options::RANSOM && self.layout.plain_names;
        if value == option.default_value() && !legacy {
            chosen.remove(&option.id);
        } else {
            chosen.insert(option.id.clone(), value);
        }
        if !chosen.is_empty() {
            self.layout.theme_options.insert(id.clone(), chosen.clone());
        }
        super::theme_timing::step("option kept");
        self.layout.write(&self.root);
        super::theme_timing::step("layout written");
        theme_look::set_choices(&id, chosen);
        self.show_theme_options(window);
        super::theme_timing::step("options shown");
        // The theme's sounds' volume is the app's to apply: nothing to make.
        if option.id == options::SOUNDS {
            return;
        }
        let again = SharedString::from(id.as_str());
        theme_look::prepare(window, &id, move |window| {
            window.global::<ThemeOptions>().invoke_reshow(again);
        });
    }

    /// The shown theme's sounds as the person chose them, and Settings →
    /// Themes' Sounds; the icons Rust hands the interface again if the
    /// theme's changed them. True if they did.
    pub(super) fn show_theme_media(&mut self, window: &YapfastWindow) -> bool {
        let id = theme_look::applied_id().unwrap_or_default();
        let prefs = self
            .layout
            .theme_sounds
            .get(&id)
            .copied()
            .unwrap_or_default();
        theme_media::set_prefs(window, prefs);
        self.sound_context();
        let (sounds, interface) = theme_media::has_sounds();
        let page = window.global::<yapfast_ui::ThemesPage>();
        page.set_has_sounds(sounds);
        page.set_has_notify_sounds(theme_media::has_notification_sounds());
        page.set_has_ui_sounds(interface);
        page.set_sounds_theme(theme_look::applied_name().unwrap_or_default().into());
        page.set_sounds_on(prefs.on);
        page.set_ui_sounds_on(prefs.interface);
        page.set_sounds_volume(prefs.volume.into());
        let cursors_on = !self.layout.theme_cursors_off.contains(&id);
        theme_cursor::set_on(cursors_on);
        page.set_has_cursors(theme_cursor::has_cursors());
        page.set_cursors_on(cursors_on);
        let generation = theme_media::generation();
        if generation == self.icons_generation {
            return false;
        }
        match load_icons() {
            Ok(icons) => {
                self.icons = icons;
                self.icons_generation = generation;
                // Built again with them: the channel list, threads and posts.
                self.sidebar = None;
                self.threads_shown = None;
                self.forum_shown = None;
                true
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the theme's icons");
                false
            }
        }
    }

    /// The shown theme's `content`, if the person lets it style people's
    /// profiles and media, and Settings → Themes' switch for it. True if
    /// what rows are built with changed; they're built again at the next
    /// refresh, and pictures converted again as they next show.
    pub(super) fn show_theme_content(&mut self, window: &YapfastWindow) -> bool {
        let id = theme_look::applied_id().unwrap_or_default();
        let applied = theme_look::applied_content();
        let on = self.layout.theme_content.contains(&id);
        theme_look::set_content_shown(self.layout.theme_content.iter().cloned());
        let page = window.global::<ThemesPage>();
        page.set_content(
            applied
                .and_then(|(content, _)| content.phrase())
                .unwrap_or_default()
                .into(),
        );
        page.set_content_on(on);
        // On Light a `dither` theme's pictures take its palette as a gradient.
        theme_content::set_light(self.layout.theme_performance == Performance::Light);
        let change = theme_content::show(window, applied.filter(|_| on));
        if change.media {
            pictures::media_changed();
            self.pictures.clear();
            crate::custom_emoji::forget_images();
            crate::twemoji::forget();
            // Emoji in text follow, once their font is recoloured: text is
            // shaped again with it.
            let weak = window.as_weak();
            crate::emoji_text::follow_media(theme_content::media(), move || {
                if let Some(window) = weak.upgrade() {
                    // Places that name the emoji font, such as reactions.
                    window.set_emoji_font(crate::emoji_text::shown_family().into());
                    let inner = i_slint_core::window::WindowInner::from_pub(window.window());
                    if let Some(cache) = inner.window_adapter().renderer().text_layout_cache() {
                        cache.clear_all();
                    }
                    window.window().request_redraw();
                }
            });
            bump_picture_slots(window, u32::MAX);
        }
        if change.rows || change.media {
            self.restyle_mentions();
            self.aside_restyle = true;
            self.roster_shown = None;
            self.sidebar = None;
            self.account_profile_shown = None;
        }
        change.rows || change.media
    }

    /// Converts every picture again, as it next shows: once a theme's
    /// dithered copies were made off the window's thread, or Performance
    /// turned the pattern on or off.
    pub(super) fn pictures_redithered(&mut self, window: &YapfastWindow) {
        pictures::previews_changed();
        self.pictures.clear();
        bump_picture_slots(window, u32::MAX);
        self.refresh(window);
    }

    /// Keeps what the review's sound switches say for the reviewed theme, and
    /// for its trial.
    fn keep_review_sounds(&mut self, window: &YapfastWindow, trial: bool) {
        let Some(id) = theme_look::review_id() else {
            return;
        };
        let page = window.global::<ThemesPage>();
        let prefs = theme_media::Prefs {
            on: page.get_review_notify_on(),
            interface: page.get_review_ui_on(),
            ..theme_media::Prefs::default()
        };
        let mut ids = vec![format!("{}{id}", theme_look::INSTALLED)];
        if trial {
            ids.push("trial".to_owned());
        }
        let content = page.get_review_content_on();
        for id in ids {
            if content {
                self.layout.theme_content.insert(id.clone());
            } else {
                self.layout.theme_content.remove(&id);
            }
            if prefs == theme_media::Prefs::default() {
                self.layout.theme_sounds.remove(&id);
            } else {
                self.layout.theme_sounds.insert(id, prefs);
            }
        }
        self.layout.write(&self.root);
    }

    /// Tells the theme's sounds what the window plays on, while it has any.
    pub(super) fn sound_context(&self) {
        if theme_media::has_sounds().0 {
            theme_media::set_context(theme_media::Context {
                quiet: self.app.state.do_not_disturb(),
                speaker: self.app.state.voice.speaker.clone(),
                gain: sounds::gain(self.layout.sound_volume.unwrap_or(DEFAULT_SOUND_VOLUME)),
            });
        }
    }

    /// Marks for building again, at the next refresh, what carries the
    /// mention colour in its markup.
    pub(super) fn restyle_mentions(&mut self) {
        self.restyle = true;
        self.topic_shown = None;
        self.models.pins.clear();
        self.models.search_results.clear();
    }

    /// Opens the editor on a copy of the theme shown, which the person can
    /// swap for any other.
    pub(super) fn new_theme(&mut self, window: &YapfastWindow) {
        let current = window.get_theme_id();
        let starts: Vec<_> = theme::starts(&self.layout.custom_themes).collect();
        let index = starts
            .iter()
            .position(|start| start.id == current.as_str())
            .unwrap_or(0);
        let names: Vec<SharedString> = starts.iter().map(|start| start.name.into()).collect();
        let (base, start) = (starts[index].base, starts[index].theme);
        let start_id = starts[index].id.to_owned();
        window.set_theme_name(copy_name(starts[index].name).into());
        window.set_theme_starts(ModelRc::new(VecModel::from(names)));
        window.set_theme_start(index as i32);
        self.open_theme_editor(window, theme::Editor::new(None, base, start));
        // Simple first, from the theme shown, or all of it if the creator
        // made it.
        let made = self
            .layout
            .custom_themes
            .iter()
            .find(|custom| custom.id == start_id)
            .and_then(|custom| custom.creator.clone());
        match made {
            Some(spec) => {
                let from = theme_look::creator_folder(&self.root, &start_id);
                self.open_creator(window, spec, Some(from), false);
            }
            None => self.open_creator(window, theme_creator::spec_from(&start), None, true),
        }
        let page = window.global::<yapfast_ui::Creator>();
        page.set_start(index as i32);
        page.set_step(0);
    }

    /// Opens the editor on one of the person's own themes.
    pub(super) fn edit_theme(&mut self, window: &YapfastWindow, id: &str) {
        let Some(custom) = self
            .layout
            .custom_themes
            .iter()
            .find(|custom| custom.id == id)
        else {
            return;
        };
        window.set_theme_name(custom.name.as_str().into());
        let editor = theme::Editor::new(Some(custom.id.clone()), custom.base, custom.theme);
        let made = custom.creator.clone();
        let from = theme_look::creator_folder(&self.root, id);
        self.open_theme_editor(window, editor);
        let page = window.global::<yapfast_ui::Creator>();
        match made {
            // Made with the creator: its steps, as it was left, at Colours.
            Some(spec) => {
                self.open_creator(window, spec, Some(from), false);
                page.set_step(1);
            }
            // Made of colours alone: Advanced, and nothing to export.
            None => {
                page.set_simple(false);
                page.set_can_export(false);
            }
        }
    }

    /// Start over: the editor opens again on what it opened on, at the step
    /// it shows.
    pub(super) fn restart_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.as_ref() else {
            return;
        };
        let page = window.global::<yapfast_ui::Creator>();
        let (step, simple) = (page.get_step(), page.get_simple());
        match editor.editing.clone() {
            Some(id) => self.edit_theme(window, &id),
            None => self.new_theme(window),
        }
        page.set_step(step);
        if !simple {
            self.creator_advanced(window);
        }
    }

    fn open_theme_editor(&mut self, window: &YapfastWindow, editor: theme::Editor) {
        window.set_theme_editing_saved(editor.editing.is_some());
        window.set_theme_dark(editor.theme.dark);
        window.set_theme_colours(editor.rows());
        window.set_theme_can_save(true);
        // The editor is Settings → Themes → Create's.
        window.set_settings_section(super::theme_look::themes_section(window));
        window.global::<yapfast_ui::ThemesPage>().set_tab(3);
        window.set_theme_editor(true);
        self.set_theme(window, &editor.theme);
        self.theme_editor = Some(editor);
    }

    /// The new theme starts again from another, and takes its name too
    /// unless the person named it.
    pub(super) fn theme_start_chosen(&mut self, window: &YapfastWindow, index: i32) {
        let start = |custom: &[theme::Custom], index: i32| {
            let index = usize::try_from(index).ok()?;
            theme::starts(custom)
                .nth(index)
                .map(|start| (start.base, start.theme, copy_name(start.name)))
        };
        let custom = &self.layout.custom_themes;
        let Some((base, theme, name)) = start(custom, index) else {
            return;
        };
        let named = start(custom, window.get_theme_start())
            .is_none_or(|(_, _, before)| before != window.get_theme_name().as_str());
        self.creator_remember(window, None);
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        if !named {
            window.set_theme_name(name.into());
        }
        editor.restart(base, theme);
        if let Some(spec) = editor.creator.as_mut() {
            *spec = theme_creator::Spec {
                fonts: std::mem::take(&mut spec.fonts),
                picture: spec.picture.take(),
                ..theme_creator::spec_from(&theme)
            };
        }
        window.set_theme_start(index);
        window.set_theme_dark(theme.dark);
        window.set_theme_can_save(true);
        self.set_theme(window, &theme);
        self.creator_recoloured(window);
    }

    /// A colour's field changed: the window shows it at once if it's a
    /// colour.
    pub(super) fn theme_colour_edited(&mut self, window: &YapfastWindow, index: i32, text: &str) {
        // A colour typed letter by letter is one change to take back.
        self.creator_remember(window, u32::try_from(index).ok());
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let changed = usize::try_from(index).is_ok_and(|index| editor.edit(index, text));
        window.set_theme_can_save(editor.valid());
        if changed {
            let theme = editor.theme;
            self.set_theme(window, &theme);
            self.creator_recoloured(window);
        }
    }

    pub(super) fn theme_dark_toggled(&mut self, window: &YapfastWindow, dark: bool) {
        self.creator_remember(window, None);
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        editor.theme.dark = dark;
        let theme = editor.theme;
        // Dark or light anew: the creator derives every colour for it.
        let made = editor.creator.as_mut().map(|spec| {
            spec.background = if dark {
                theme_creator::Background::Dark
            } else {
                theme_creator::Background::Light
            };
        });
        if made.is_some() {
            self.creator_changed(window, true);
            return;
        }
        self.set_theme(window, &theme);
    }

    /// Keeps the theme as edited, among the person's own, and chooses it.
    pub(super) fn save_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.take_if(|editor| editor.valid()) else {
            return;
        };
        let name = match window.get_theme_name().trim() {
            "" => "My theme".to_owned(),
            name => name.to_owned(),
        };
        let id = editor
            .editing
            .clone()
            .unwrap_or_else(|| theme::new_id(&self.layout.custom_themes));
        // The creator's theme keeps its package in a folder of its own.
        let kept = match &editor.creator {
            Some(spec) => self.creator_save(&id, &name, spec, &editor.theme),
            None => true,
        };
        if !kept {
            self.theme_editor = Some(editor);
            self.refresh(window);
            return;
        }
        let custom = &mut self.layout.custom_themes;
        match custom.iter_mut().find(|custom| custom.id == id) {
            Some(saved) => {
                saved.name = name;
                saved.theme = editor.theme;
                saved.base = editor.base;
                if editor.creator.is_some() {
                    saved.folder = Some(theme_creator::creator_relative(&id));
                }
                saved.creator = editor.creator;
            }
            None => custom.push(theme::Custom {
                id: id.clone(),
                name,
                base: editor.base,
                theme: editor.theme,
                folder: editor
                    .creator
                    .is_some()
                    .then(|| theme_creator::creator_relative(&id)),
                creator: editor.creator,
            }),
        }
        theme_creator::register(&self.root, &self.layout.custom_themes);
        // The saved theme shows from its own folder, made ready again.
        theme_look::cancel_prepare();
        close_theme_editor(window);
        self.creator_closed(window);
        theme::show_cards(window, &self.layout.custom_themes);
        self.choose_theme(window, &id);
        // It's among the person's own in Installed now.
        window.global::<yapfast_ui::ThemesPage>().set_tab(1);
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, window: &YapfastWindow) {
        if self.theme_editor.take().is_some() {
            close_theme_editor(window);
            self.creator_closed(window);
            self.show_chosen(window);
        }
    }

    /// Deletes the theme being edited. If it was the chosen one, Yap shows.
    pub(super) fn delete_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.take() else {
            return;
        };
        close_theme_editor(window);
        self.creator_closed(window);
        if let Some(id) = editor.editing {
            self.layout.custom_themes.retain(|custom| custom.id != id);
            let _ = std::fs::remove_dir_all(theme_look::creator_folder(&self.root, &id));
            theme_creator::register(&self.root, &self.layout.custom_themes);
        }
        theme::show_cards(window, &self.layout.custom_themes);
        let (id, _) = theme::chosen(self.layout.theme.as_deref(), &self.layout.custom_themes);
        let id = id.to_owned();
        self.choose_theme(window, &id);
    }
}

/// What a new theme copied from another is called until the person names it.
fn copy_name(name: &str) -> String {
    format!("{name} copy")
}

/// Drops the editor's models from the window, so they're held only while
/// it's open.
fn close_theme_editor(window: &YapfastWindow) {
    window.set_theme_editor(false);
    window.set_theme_colours(ModelRc::default());
    window.set_theme_starts(ModelRc::default());
}

/// Binds Appearance's installed-theme settings, the Themes tab and
/// `--theme-dev`'s reload.
pub(super) fn bind_themes(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    use yapfast_core::themes::check::plain;
    use yapfast_ui::{Look, ThemesPage};
    // Each callback borrows the adapter and the window, as the others do.
    macro_rules! on {
        ($target:expr, $callback:ident($($arg:ident),*) => |$adapter:ident, $window:ident| $body:expr) => {{
            let (state, weak) = (adapter.clone(), window.as_weak());
            $target.$callback(move |$($arg),*| {
                if let Some($window) = weak.upgrade()
                    && let Ok(mut $adapter) = state.try_borrow_mut()
                {
                    let $adapter = &mut *$adapter;
                    $body;
                }
            });
        }};
    }
    super::theme_timing::watch(window);
    super::theme_layout::init(window);
    super::theme_lists::init(window);
    super::theme_chrome::init(window);
    super::theme_cursor::init(window);
    super::theme_trim::init(window);
    {
        // Pictures dithered off the window's thread show once made; a
        // window busy then tries again a moment later.
        fn redraw(state: std::rc::Weak<RefCell<Adapter>>, weak: slint::Weak<YapfastWindow>) {
            let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
                return;
            };
            match adapter.try_borrow_mut() {
                Ok(mut adapter) => adapter.pictures_redithered(&window),
                Err(_) => {
                    slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
                        redraw(state, weak)
                    })
                }
            }
        }
        let (state, weak) = (Rc::downgrade(adapter), window.as_weak());
        theme_content::on_dithered(move || redraw(state.clone(), weak.clone()));
    }
    {
        let weak = window.as_weak();
        window
            .global::<Look>()
            .on_fx(move |slot, text, _tick| match weak.upgrade() {
                Some(window) => theme_look::effect_text(&window, slot, text),
                None => text,
            });
    }
    let option_page = window.global::<ThemeOptions>();
    on!(option_page, on_toggled(id, on) => |adapter, window| {
        adapter.change_theme_option(&window, &id, |_| Some(serde_json::Value::Bool(on)));
    });
    on!(option_page, on_chosen(id, index) => |adapter, window| {
        adapter.change_theme_option(&window, &id, |kind| match kind {
            options::Kind::Choice { choices, .. } => usize::try_from(index)
                .ok()
                .and_then(|index| choices.get(index))
                .map(|choice| serde_json::Value::String(choice.value.clone())),
            _ => None,
        });
    });
    on!(option_page, on_slid(id, step) => |adapter, window| {
        adapter.change_theme_option(&window, &id, |kind| match kind {
            options::Kind::Slider { min, step: size, .. } => {
                serde_json::Number::from_f64(min + f64::from(step) * size)
                    .map(serde_json::Value::Number)
            }
            options::Kind::Hue { .. } => Some(serde_json::json!(step)),
            _ => None,
        });
    });
    option_page.on_sliding({
        let weak = window.as_weak();
        // Only the window's colours follow the slider: what's slow to make
        // waits for its release. No adapter borrow, so it keeps up.
        move |id, hue| {
            if let Some(window) = weak.upgrade()
                && let Some(colours) = theme_look::hue_preview(&id, f64::from(hue))
            {
                let _ = colours.apply(&window);
            }
        }
    });
    on!(option_page, on_reshow(id) => |adapter, window| {
        super::theme_timing::step("reshow");
        match theme_look::show(&window, &id) {
            Ok(colours) => {
                super::theme_timing::step("shown");
                adapter.set_theme(&window, &colours);
            }
            Err(reason) => {
                // The theme as it was stays; the person hears why.
                adapter.app.state.notice = Some(format!("That option couldn't be shown: {reason}"));
                adapter.show_theme_options(&window);
            }
        }
        super::theme_timing::step("theme set");
        adapter.refresh(&window);
        super::theme_timing::values_set();
    });
    on!(window, on_theme_reload() => |adapter, window| {
        match theme_look::show(&window, "dev") {
            Ok(colours) => {
                adapter.set_theme(&window, &colours);
                adapter.app.state.notice = None;
            }
            Err(reason) => {
                // The last good version stays on the window.
                adapter.app.state.notice = Some(format!("theme-dev: {reason}"));
            }
        }
        adapter.refresh(&window);
    });
    // A theme's interface sounds and painted icons ask here; neither needs
    // the adapter, so they answer even while it's busy.
    window.global::<Cue>().on_play(|index| {
        if let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|index| SoundSlot::ALL.get(index))
        {
            theme_media::cue(*slot, FOCUSED.get());
        }
    });
    window
        .global::<IconPack>()
        .on_paint(|source, stock, _| theme_media::paint(&source, stock));
    let page = window.global::<ThemesPage>();
    {
        let weak = window.as_weak();
        page.on_settings_shown(move |open| {
            if let Some(window) = weak.upgrade() {
                theme_pace::covered(&window, open);
            }
        });
    }
    // A theme's cost in plain words: how heavy it is, and its line's parts.
    page.on_cost_summary(|cost| {
        plain::plain(&cost)
            .map(|plain| plain.summary.into())
            .unwrap_or_default()
    });
    page.on_cost_weight(|cost| match plain::plain(&cost).map(|plain| plain.weight) {
        None => -1,
        Some(plain::Weight::Light) => 0,
        Some(plain::Weight::Moderate) => 1,
        Some(plain::Weight::Heavy) => 2,
    });
    page.on_cost_rows(|cost| {
        let rows: Vec<yapfast_ui::CostRow> = plain::plain(&cost)
            .map(|plain| plain.rows)
            .unwrap_or_default()
            .into_iter()
            .map(|(label, text)| yapfast_ui::CostRow {
                label: label.into(),
                text: text.into(),
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    });
    on!(page, on_opened() => |adapter, window| {
        adapter.show_previous_theme(&window);
        let page = window.global::<ThemesPage>();
        page.set_performance(adapter.layout.theme_performance.index());
        page.set_reduce_motion(adapter.layout.reduce_motion);
        theme_look::show_page(&window, &theme_look::marketplace(&window));
        theme_look::load_market(&window, adapter.runtime.clone());
    });
    on!(page, on_market_retry() => |adapter, window| {
        theme_look::retry_market(&window, adapter.runtime.clone());
    });
    on!(page, on_closed() => |adapter, window| {
        let _ = adapter;
        theme_look::close_page(&window);
    });
    on!(page, on_tab_chosen(tab) => |adapter, window| {
        let _ = adapter;
        let page = window.global::<ThemesPage>();
        page.set_tab(tab);
        page.set_error(SharedString::new());
    });
    on!(page, on_search(query) => |adapter, window| {
        let _ = (adapter, query);
        theme_look::show_listed(&window, &theme_look::marketplace(&window));
    });
    on!(page, on_revert_theme() => |adapter, window| {
        if let Some(previous) = adapter.layout.previous_theme.clone() {
            adapter.choose_theme(&window, &previous);
            adapter.themes_changed(&window);
        }
    });
    on!(page, on_performance_chosen(index) => |adapter, window| {
        adapter.layout.theme_performance = Performance::from_index(index);
        adapter.layout.write(&adapter.root);
        window.global::<ThemesPage>().set_performance(index);
        theme_look::set_motion(&window, adapter.layout.theme_performance, adapter.layout.reduce_motion);
        if theme_content::set_light(adapter.layout.theme_performance == Performance::Light) {
            adapter.pictures_redithered(&window);
        }
    });
    on!(page, on_sounds_changed() => |adapter, window| {
        let Some(id) = theme_look::applied_id() else { return };
        let page = window.global::<ThemesPage>();
        let prefs = theme_media::Prefs {
            on: page.get_sounds_on(),
            interface: page.get_ui_sounds_on(),
            volume: page.get_sounds_volume().clamp(0, 100) as u8,
        };
        if prefs == theme_media::Prefs::default() {
            adapter.layout.theme_sounds.remove(&id);
        } else {
            adapter.layout.theme_sounds.insert(id, prefs);
        }
        adapter.layout.write(&adapter.root);
        theme_media::set_prefs(&window, prefs);
        // Hearing the new volume is how it's judged.
        theme_media::cue(SoundSlot::Press, FOCUSED.get());
    });
    on!(page, on_content_changed() => |adapter, window| {
        let Some(id) = theme_look::applied_id() else { return };
        if window.global::<ThemesPage>().get_content_on() {
            adapter.layout.theme_content.insert(id);
        } else {
            adapter.layout.theme_content.remove(&id);
        }
        adapter.layout.write(&adapter.root);
        if adapter.show_theme_content(&window) {
            adapter.refresh(&window);
        }
    });
    on!(page, on_cursors_toggled(on) => |adapter, _window| {
        let Some(id) = theme_look::applied_id() else { return };
        if on {
            adapter.layout.theme_cursors_off.remove(&id);
        } else {
            adapter.layout.theme_cursors_off.insert(id);
        }
        adapter.layout.write(&adapter.root);
        theme_cursor::set_on(on);
    });
    on!(page, on_transitions_toggled(on) => |adapter, window| {
        let _ = window;
        adapter.layout.theme_transitions = on;
        adapter.layout.write(&adapter.root);
        theme_art::set_transitions(on);
    });
    on!(page, on_reduce_motion_toggled(on) => |adapter, window| {
        adapter.layout.reduce_motion = on;
        adapter.layout.write(&adapter.root);
        theme_look::set_motion(&window, adapter.layout.theme_performance, on);
    });
    on!(page, on_apply(id) => |adapter, window| {
        let layout_id = format!("{}{id}", theme_look::INSTALLED);
        // Ready off the UI thread first, then applied here again.
        if !theme_look::is_ready(&layout_id) {
            super::theme_timing::start("apply");
            theme_look::prepare(&window, &layout_id, move |window| {
                window.global::<ThemesPage>().invoke_apply(id);
            });
            return;
        }
        super::theme_timing::step("apply ready");
        adapter.choose_theme(&window, &layout_id);
        adapter.themes_changed(&window);
        super::theme_timing::step("gallery shown");
    });
    on!(page, on_turn_off() => |adapter, window| {
        adapter.default_theme_for_session(&window);
        theme_look::mark_applied(&window);
    });
    on!(page, on_try(id) => |adapter, window| {
        let layout_id = format!("{}{id}", theme_look::INSTALLED);
        if !theme_look::is_ready(&layout_id) {
            theme_look::prepare(&window, &layout_id, move |window| {
                window.global::<ThemesPage>().invoke_try(id);
            });
            return;
        }
        let back = adapter.layout.theme.clone();
        match theme_look::try_installed(&window, &id, back) {
            Ok(colours) => {
                adapter.set_theme(&window, &colours);
                theme_media::cue(SoundSlot::ThemeApplied, FOCUSED.get());
            }
            Err(reason) => window.global::<ThemesPage>().set_error(reason.into()),
        }
    });
    on!(page, on_remove(id) => |adapter, window| {
        let layout_id = format!("{}{id}", theme_look::INSTALLED);
        if adapter.layout.theme.as_deref() == Some(layout_id.as_str()) {
            let back = adapter.layout.previous_theme.clone().filter(|previous| *previous != layout_id);
            adapter.layout.theme = back.clone();
            adapter.layout.previous_theme = None;
            adapter.layout.write(&adapter.root);
            adapter.show_theme(&window, back.as_deref());
        }
        if let Err(reason) = theme_look::remove(&id) {
            window.global::<ThemesPage>().set_error(reason.into());
        }
        adapter.themes_changed(&window);
    });
    on!(page, on_roll_back(id) => |adapter, window| {
        if let Err(reason) = theme_look::roll_back(&id) {
            window.global::<ThemesPage>().set_error(reason.into());
        }
        let layout_id = format!("{}{id}", theme_look::INSTALLED);
        if adapter.layout.theme.as_deref() == Some(layout_id.as_str()) {
            adapter.show_theme(&window, Some(&layout_id));
        }
        adapter.themes_changed(&window);
    });
    on!(page, on_install_listed(id) => |adapter, window| {
        theme_look::install_listed(&window, adapter.runtime.clone(), &id);
    });
    on!(page, on_find(url) => |adapter, window| {
        theme_look::find(&window, adapter.runtime.clone(), url.to_string());
    });
    on!(page, on_install_review() => |adapter, window| {
        adapter.keep_review_sounds(&window, false);
        match theme_look::install_review(&window) {
            Ok(id) => {
                adapter.choose_theme(&window, &id);
                window.global::<ThemesPage>().set_tab(1);
                adapter.themes_changed(&window);
            }
            Err(reason) => window.global::<ThemesPage>().set_error(reason.into()),
        }
    });
    on!(page, on_cancel_review() => |adapter, window| {
        let _ = adapter;
        theme_look::close_review(&window);
    });
    on!(page, on_try_review() => |adapter, window| {
        adapter.keep_review_sounds(&window, true);
        let back = adapter.layout.theme.clone();
        match theme_look::try_review(&window, back) {
            Ok(colours) => {
                adapter.set_theme(&window, &colours);
                theme_media::cue(SoundSlot::ThemeApplied, FOCUSED.get());
            }
            Err(reason) => window.global::<ThemesPage>().set_error(reason.into()),
        }
    });
    on!(page, on_keep() => |adapter, window| {
        match theme_look::end_trial(&window, true) {
            Some(Ok(id)) => {
                adapter.choose_theme(&window, &id);
                adapter.themes_changed(&window);
            }
            Some(Err(back)) => adapter.show_theme(&window, back.as_deref()),
            None => {}
        }
    });
    on!(page, on_revert() => |adapter, window| {
        if let Some(Err(back)) = theme_look::end_trial(&window, false) {
            adapter.show_theme(&window, back.as_deref());
        }
    });
}

/// Appearance's Export: the theme being edited as a package folder in a
/// folder the person picks, ready for a GitHub repository and Settings →
/// Themes.
pub(super) fn bind_export(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = adapter.clone();
    window.on_export_theme(move || {
        let Ok(adapter) = state.try_borrow() else { return };
        let Some(window) = adapter.window.upgrade() else { return };
        let Some(editor) = adapter.theme_editor.as_ref() else { return };
        let name = window.get_theme_name().to_string();
        let author = window.get_account_name().to_string();
        let base = editor.base;
        let shown = editor.theme;
        // A custom theme's own folder (its fonts, pictures and other
        // layers), relative to the data folder.
        let own = editor.editing.as_ref().and_then(|id| {
            adapter
                .layout
                .custom_themes
                .iter()
                .find(|custom| custom.id == *id)
                .and_then(|custom| custom.folder.clone())
        });
        let data_dir = adapter.root.clone();
        let folder_name = yapfast_core::themes::package::folder_name(&name);
        drop(adapter);
        let weak = Rc::downgrade(&state);
        state.borrow_mut().dialog(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Export the theme as a package")
                .pick_folder()
                .await;
            let (Some(picked), Some(adapter)) = (picked, weak.upgrade()) else {
                return;
            };
            let folder = picked.path().join(&folder_name);
            let written = super::theme_tools::write_custom(
                &folder,
                &name,
                &author,
                base,
                &shown,
                own.as_deref(),
                &data_dir,
            );
            // The checks an install runs, so what's shared installs.
            theme_look::register_bases();
            let refused = yapfast_core::themes::check::Files::read(&folder)
                .map(|files| {
                    yapfast_core::themes::check::check(&files, yapfast_core::themes::check::Strictness::Install)
                        .reason()
                        .map(str::to_owned)
                })
                .unwrap_or_else(Some);
            let mut adapter = adapter.borrow_mut();
            adapter.app.state.notice = Some(match (written, refused) {
                (Ok(_), None) => format!(
                    "Exported to {}. Add a preview, put it in a GitHub repository, and it installs with Add from URL.",
                    folder.display()
                ),
                (Ok(_), Some(reason)) => format!(
                    "Exported to {}, but it wouldn't install yet: {reason}",
                    folder.display()
                ),
                (Err(error), _) => format!("The theme couldn't be exported: {error}"),
            });
            if let Some(window) = adapter.window.upgrade() {
                adapter.refresh(&window);
            }
        });
    });
}
