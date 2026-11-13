//! Appearance: choosing a theme, and making and editing the person's own.

use super::*;
use yapfast_core::themes::media::SoundSlot;

impl Adapter {
    /// Shows the window in a theme, built in or the person's own, and keeps
    /// it chosen. False for an id Yapfast doesn't know, which changes nothing.
    pub(super) fn choose_theme(&mut self, window: &YapfastWindow, id: &str) -> bool {
        let shown = if id.starts_with(theme_look::INSTALLED) {
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
        if !id.starts_with(theme_look::INSTALLED) {
            theme_look::clear(window);
        }
        window.set_theme_id(id.as_str().into());
        self.set_theme(window, &chosen);
        if id.starts_with(theme_look::INSTALLED) {
            theme_media::cue(SoundSlot::ThemeApplied, FOCUSED.get());
        }
        if self.layout.theme.as_deref() != Some(id.as_str()) {
            self.layout.previous_theme = self.layout.theme.take();
        }
        self.layout.theme = Some(id);
        self.layout.write(&self.root);
        self.show_previous_theme(window);
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
        window.set_theme_installed_name(theme_look::applied_name().unwrap_or_default().into());
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
    fn show_theme(&mut self, window: &YapfastWindow, id: Option<&str>) {
        let default = window
            .global::<ClientStyle>()
            .get_enabled()
            .then_some("bonfire");
        if let Some(id) = id.filter(|id| id.starts_with(theme_look::INSTALLED))
            && let Ok(colours) = theme_look::show(window, id)
        {
            window.set_theme_id(id.into());
            self.set_theme(window, &colours);
            self.show_previous_theme(window);
            return;
        }
        theme_look::clear(window);
        let (id, chosen) = theme::chosen(
            id.filter(|id| !id.starts_with(theme_look::INSTALLED))
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
        window.set_themes(theme::cards(&self.layout.custom_themes));
        theme_look::show_page(window, &theme_look::marketplace(window));
    }

    /// Shows the window in another theme's colours, building again what
    /// carries the old mention colour in its markup: message rows, the
    /// topic, pins and search results.
    pub(super) fn set_theme(&mut self, window: &YapfastWindow, theme: &theme::Theme) {
        // `app.theme_mode` is the plugins': the theme is the window's, and this
        // is the one place that knows which one is on.
        #[cfg(feature = "plugins")]
        self.app.plugins_theme(theme.dark);
        let icons = self.show_theme_media(window);
        let content = self.show_theme_content(window);
        if theme.apply(window) || icons || content {
            self.restyle_mentions();
            self.refresh(window);
        }
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
        let page = window.global::<ThemesPage>();
        page.set_content(
            applied
                .and_then(|(content, _)| content.phrase())
                .unwrap_or_default()
                .into(),
        );
        page.set_content_on(on);
        let change = theme_content::show(window, applied.filter(|_| on));
        if change.media {
            pictures::media_changed();
            self.pictures.clear();
            crate::custom_emoji::forget_images();
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
        window.set_theme_name(copy_name(starts[index].name).into());
        window.set_theme_starts(ModelRc::new(VecModel::from(names)));
        window.set_theme_start(index as i32);
        self.open_theme_editor(window, theme::Editor::new(None, base, start));
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
        self.open_theme_editor(window, editor);
    }

    fn open_theme_editor(&mut self, window: &YapfastWindow, editor: theme::Editor) {
        window.set_theme_editing_saved(editor.editing.is_some());
        window.set_theme_dark(editor.theme.dark);
        window.set_theme_colours(editor.rows());
        window.set_theme_can_save(true);
        window.set_theme_editor(true);
        self.set_theme(window, &editor.theme);
        self.theme_editor = Some(editor);
    }

    /// The new theme starts again from another, and takes its name too
    /// unless the person named it.
    pub(super) fn theme_start_chosen(&mut self, window: &YapfastWindow, index: i32) {
        let custom = &self.layout.custom_themes;
        let start = |index: i32| {
            let index = usize::try_from(index).ok()?;
            theme::starts(custom)
                .nth(index)
                .map(|start| (start.base, start.theme, copy_name(start.name)))
        };
        let (Some(editor), Some((base, theme, name))) = (self.theme_editor.as_mut(), start(index))
        else {
            return;
        };
        let named = start(window.get_theme_start())
            .is_none_or(|(_, _, before)| before != window.get_theme_name().as_str());
        if !named {
            window.set_theme_name(name.into());
        }
        editor.restart(base, theme);
        window.set_theme_start(index);
        window.set_theme_dark(theme.dark);
        window.set_theme_can_save(true);
        self.set_theme(window, &theme);
    }

    /// A colour's field changed: the window shows it at once if it's a
    /// colour.
    pub(super) fn theme_colour_edited(&mut self, window: &YapfastWindow, index: i32, text: &str) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let changed = usize::try_from(index).is_ok_and(|index| editor.edit(index, text));
        window.set_theme_can_save(editor.valid());
        if changed {
            let theme = editor.theme;
            self.set_theme(window, &theme);
        }
    }

    pub(super) fn theme_dark_toggled(&mut self, window: &YapfastWindow, dark: bool) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        editor.theme.dark = dark;
        let theme = editor.theme;
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
        let custom = &mut self.layout.custom_themes;
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
                let id = theme::new_id(custom);
                custom.push(theme::Custom {
                    id: id.clone(),
                    name,
                    base: editor.base,
                    theme: editor.theme,
                });
                id
            }
        };
        close_theme_editor(window);
        window.set_themes(theme::cards(&self.layout.custom_themes));
        self.choose_theme(window, &id);
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, window: &YapfastWindow) {
        if self.theme_editor.take().is_some() {
            close_theme_editor(window);
            let (_, before) =
                theme::chosen(self.layout.theme.as_deref(), &self.layout.custom_themes);
            self.set_theme(window, &before);
        }
    }

    /// Deletes the theme being edited. If it was the chosen one, Yap shows.
    pub(super) fn delete_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.take() else {
            return;
        };
        close_theme_editor(window);
        if let Some(id) = editor.editing {
            self.layout.custom_themes.retain(|custom| custom.id != id);
        }
        window.set_themes(theme::cards(&self.layout.custom_themes));
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
    use yapfast_core::themes::Performance;
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
    super::theme_layout::init(window);
    super::theme_chrome::init(window);
    {
        let weak = window.as_weak();
        window
            .global::<Look>()
            .on_fx(move |slot, text, _tick| match weak.upgrade() {
                Some(window) => theme_look::effect_text(&window, slot, text),
                None => text,
            });
    }
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
    on!(page, on_opened() => |adapter, window| {
        adapter.show_previous_theme(&window);
        let page = window.global::<ThemesPage>();
        page.set_performance(adapter.layout.theme_performance.index());
        page.set_reduce_motion(adapter.layout.reduce_motion);
        theme_look::show_page(&window, &theme_look::marketplace(&window));
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
        theme_look::show_page(&window, &theme_look::marketplace(&window));
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
    on!(page, on_reduce_motion_toggled(on) => |adapter, window| {
        adapter.layout.reduce_motion = on;
        adapter.layout.write(&adapter.root);
        theme_look::set_motion(&window, adapter.layout.theme_performance, on);
    });
    on!(page, on_apply(id) => |adapter, window| {
        adapter.choose_theme(&window, &format!("{}{id}", theme_look::INSTALLED));
        adapter.themes_changed(&window);
    });
    on!(page, on_try(id) => |adapter, window| {
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
        let _ = (adapter, id);
        // The marketplace's own fetch is the plugin system's, and lands with
        // it; until then a listed theme installs from its repository.
        window.global::<ThemesPage>().set_error(
            "Installing from the marketplace comes with the marketplace itself; add the theme from its GitHub repository below.".into(),
        );
    });
    on!(page, on_find(url) => |adapter, window| {
        let _ = adapter;
        theme_look::find(&window, url.to_string());
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
        let colours: Vec<(String, u32)> = theme::Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                Some((token.replace('_', "-"), editor.theme.colour(index)?.as_argb_encoded()))
            })
            .collect();
        let named: Vec<(&str, u32)> = colours.iter().map(|(token, colour)| (token.as_str(), *colour)).collect();
        let text = yapfast_core::themes::package::export(&name, editor.base, editor.theme.dark, &named);
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
            let written = std::fs::create_dir_all(&folder)
                .and_then(|()| std::fs::write(folder.join(yapfast_core::themes::MANIFEST), &text))
                .and_then(|()| {
                    std::fs::write(
                        folder.join("LICENSE"),
                        "Choose your theme's licence, and list every font and picture it ships with theirs.\n",
                    )
                });
            let mut adapter = adapter.borrow_mut();
            adapter.app.state.notice = Some(match written {
                Ok(()) => format!(
                    "Exported to {}. Add a preview and a description, check it with `yapfast theme check`, and publish it on GitHub.",
                    folder.display()
                ),
                Err(error) => format!("The theme couldn't be exported: {error}"),
            });
            if let Some(window) = adapter.window.upgrade() {
                adapter.refresh(&window);
            }
        });
    });
}
