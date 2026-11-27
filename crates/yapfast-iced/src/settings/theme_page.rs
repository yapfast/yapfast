//! Settings → Themes: its title and tabs (Installed, Marketplace, Add from
//! URL, Create), the installed themes as cards or a list with what each
//! does first and the rest behind "…", the person's own themes, what the
//! applied theme lets the person set, and Performance and Motion. The
//! marketplace's tabs are drawn by `theme_marketplace`, the creator by
//! `theme_creator`. It reads as the other settings pages: a 24 px title,
//! 12 px headings, groups 16 px apart. See docs/themes.md.

use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Space, column, container, image, row, stack, text},
};
use yapfast_core::themes::{Mode, options};
use yapfast_iced_kit::theme_page as kit;

use super::{
    Page,
    appearance::{Event, marketplace_message, theme_message},
    theme_creator, theme_marketplace,
    theme_marketplace::{Spot, Tab},
};
use crate::{
    app::Message,
    controls::{self, Item, Kind, Light, action, slider},
    fonts::{REGULAR, SEMIBOLD},
    form::{self, switch},
    pictures::Icon,
    popover::{Popover, Side, Width},
    themes::{Installed, color},
};

/// The "…" menu's width.
const MENU: f32 = 208.0;

pub(super) fn modes_text(modes: &[Mode]) -> &'static str {
    match modes {
        [Mode::Dark, Mode::Light] => "Dark and light",
        [Mode::Light] => "Light",
        _ => "Dark",
    }
}

fn number(value: f64) -> serde_json::Value {
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}

impl<'a> Page<'a> {
    /// The page: the creator or the colour editor while one is open, else
    /// the tab chosen.
    pub(super) fn themes_page(self) -> Element<'a, Message> {
        let settings = self.s;
        if let Some(creator) = &settings.creator {
            return self.creator_view(creator);
        }
        if let Some(editor) = &settings.editor {
            return self.editor_view(editor);
        }
        let theme = self.cx.look.theme;
        let tab = settings.marketplace.tab;
        let body = match tab {
            Tab::Installed => self.installed_tab(),
            Tab::Create => self.create_tab(),
            Tab::Marketplace | Tab::AddFromUrl => settings
                .marketplace
                .view(self.cx.look, self.cx.themes)
                .map(marketplace_message),
        };
        let mut parts = vec![
            form::title(theme, "Themes"),
            form::note(
                theme,
                "Themes restyle Yapfast: its colours, fonts, shapes, art and sounds. A theme is data, never code, and you see what one changes before it's installed.",
            ),
            self.theme_tabs(),
        ];
        // Always a slot, so the body keeps its place in the tree as an error
        // comes and goes: typing into Add from URL's field after an error
        // cleared it, and the field lost the keys after one letter.
        // Add from URL says what went wrong under its field instead.
        let error = settings
            .marketplace
            .error
            .as_deref()
            .filter(|_| tab != Tab::AddFromUrl);
        parts.push(
            column![
                error.map_or_else(
                    || Space::new().height(0).into(),
                    |error| self.error_banner(error)
                ),
                body
            ]
            .spacing(if error.is_some() { form::GAP } else { 0.0 })
            .width(Length::Fill)
            .into(),
        );
        form::page(parts)
    }

    /// Installed, Marketplace, Add from URL, Create: the role editor's
    /// underlined tabs.
    pub(super) fn theme_tabs(self) -> Element<'a, Message> {
        let current = if self.s.creator.is_some() || self.s.editor.is_some() {
            Tab::Create
        } else {
            self.s.marketplace.tab
        };
        let tabs = [
            ("Installed", Tab::Installed),
            ("Marketplace", Tab::Marketplace),
            ("Add from URL", Tab::AddFromUrl),
            ("Create", Tab::Create),
        ];
        let focused = tabs
            .iter()
            .position(|(_, tab)| self.s.marketplace.focus == Some(Spot::Tab(*tab)));
        kit::tabs_focused(
            self.cx.look.theme,
            tabs.map(|(label, tab)| (label, current == tab, theme_message(Event::Tab(tab)))),
            focused,
        )
    }

    /// `content`, ringed while the keys are on `spot`.
    fn ring(self, spot: Spot, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        if self.s.marketplace.focus.as_ref() == Some(&spot) {
            kit::focus_ring(self.cx.look.theme, content)
        } else {
            content.into()
        }
    }

    /// What went wrong, under the tabs where the eye is.
    fn error_banner(self, error: &str) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ground = crate::style::faded(theme.danger, 0.12);
        container(
            row![
                container(
                    self.cx
                        .look
                        .glyph(Icon::TriangleAlert, 18, theme.danger_text)
                )
                .align_top(Length::Shrink),
                text(error.to_owned())
                    .size(14)
                    .color(theme.danger_text)
                    .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .width(Length::Fill)
        .style(move |_| crate::style::rounded(ground, 8))
        .into()
    }

    /// Installed: the themes, the person's own, the applied one's settings,
    /// then Performance and Motion.
    fn installed_tab(self) -> Element<'a, Message> {
        let themes = self.cx.themes;
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        if themes.installed.is_empty() && themes.custom.is_empty() && themes.refused.is_empty() {
            let theme = self.cx.look.theme;
            parts.push(kit::notice(
                self.cx.look,
                Icon::Brush,
                theme.muted,
                "No themes yet",
                Some("Find one in the Marketplace, add one from its GitHub repository, or make your own in Create.".into()),
                vec![
                    action(
                        theme,
                        "Browse the Marketplace",
                        Kind::Primary,
                        Some(theme_message(Event::Tab(Tab::Marketplace))),
                    )
                    .into(),
                    action(
                        theme,
                        "Add from URL",
                        Kind::Plain,
                        Some(theme_message(Event::Tab(Tab::AddFromUrl))),
                    )
                    .into(),
                    action(
                        theme,
                        "Create",
                        Kind::Plain,
                        Some(theme_message(Event::Tab(Tab::Create))),
                    )
                    .into(),
                ],
            ));
        }
        if !themes.installed.is_empty() {
            parts.push(self.installed_themes());
        }
        if !themes.refused.is_empty() {
            parts.push(self.refused_themes());
        }
        if !themes.custom.is_empty() {
            parts.push(self.own_themes());
        }
        parts.push(self.applied_group());
        parts.push(self.theme_performance());
        column(parts).spacing(24).width(Length::Fill).into()
    }

    /// INSTALLED with Back and the cards or list toggle on its line, then
    /// the themes.
    fn installed_themes(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let cards = self.s.marketplace.gallery;
        let mut header = row![container(form::heading(theme, "INSTALLED")).width(Length::Fill)]
            .spacing(8)
            .height(28)
            .align_y(Alignment::Center);
        if let Some((_, name)) = self.cx.themes.previous() {
            header = header.push(
                self.ring(
                    Spot::Back,
                    action(
                        theme,
                        format!("Back to {name}"),
                        Kind::Ghost,
                        Some(theme_message(Event::Revert)),
                    )
                    .small()
                    .icon(self.cx.look.icon(
                        Icon::ArrowLeft,
                        16,
                        crate::theme::rgb(theme.secondary),
                    )),
                ),
            );
        }
        header = header.push(kit::view_toggle(
            theme,
            cards,
            marketplace_message(theme_marketplace::Event::ToggleGallery(false)),
            marketplace_message(theme_marketplace::Event::ToggleGallery(true)),
        ));
        let installed = &self.cx.themes.installed;
        let list: Element<'a, Message> = if cards {
            iced::widget::responsive(move |size| {
                let card = ((size.width - 12.0) / 2.0).max(0.0);
                let picture = ((card - 4.0) * 9.0 / 16.0).round().max(0.0);
                let mut grid = column![].spacing(12);
                for pair in installed.chunks(2) {
                    let mut line = row![].spacing(12);
                    for one in pair {
                        line = line.push(
                            container(self.gallery_card(one, picture))
                                .width(Length::FillPortion(1)),
                        );
                    }
                    if pair.len() == 1 {
                        line = line.push(Space::new().width(Length::FillPortion(1)));
                    }
                    grid = grid.push(line);
                }
                grid.into()
            })
            .height(Length::Shrink)
            .into()
        } else {
            column(installed.iter().map(|one| self.list_row(one)))
                .spacing(8)
                .into()
        };
        column![header, list].spacing(8).into()
    }

    /// Whether `installed` is the theme showing.
    fn applied(self, installed: &Installed) -> bool {
        self.cx.themes.chosen_id() == installed.id
    }

    /// Whether `installed` is showing on trial, not applied.
    fn on_trial(self, installed: &Installed) -> bool {
        self.cx.themes.trial().is_some() && self.applied(installed)
    }

    /// The applied theme's tag: "Applied", or "On trial" while it's tried.
    fn applied_tag(self, installed: &Installed) -> &'static str {
        if self.on_trial(installed) {
            "On trial"
        } else {
            "Applied"
        }
    }

    /// "by Yapfast · v3.5.0".
    fn byline(installed: &Installed) -> String {
        let authors = installed.authors.join(", ");
        match (authors.is_empty(), installed.version.is_empty()) {
            (false, false) => format!("by {authors} · v{}", installed.version),
            (false, true) => format!("by {authors}"),
            (true, false) => format!("v{}", installed.version),
            (true, true) => String::new(),
        }
    }

    /// One theme in the gallery: its preview (ringed and tagged while
    /// applied), its name and who made it, what it costs to run, and Apply
    /// or Turn off theme with the rest behind "…".
    fn gallery_card(self, installed: &'a Installed, picture: f32) -> Element<'a, Message> {
        let look = self.cx.look;
        let theme = look.theme;
        let applied = self.applied(installed);
        let inner = if applied { 7.0 - 1.0 } else { 7.0 };
        let shown: Element<'a, Message> = match &installed.preview {
            Some(handle) => image(handle.clone())
                .content_fit(iced::ContentFit::Cover)
                .width(Length::Fill)
                .height(picture)
                .border_radius(crate::style::corners([inner, inner, 0.0, 0.0]))
                .into(),
            None => colour_preview(&installed.palette_preview, picture),
        };
        let shown: Element<'a, Message> = if applied {
            stack![
                shown,
                container(kit::tag(
                    look,
                    self.applied_tag(installed),
                    theme.accent,
                    true,
                    Some(Icon::Check)
                ))
                .padding(8)
            ]
            .into()
        } else {
            shown
        };
        let mut name = row![
            container(
                text(installed.name.clone())
                    .size(theme.type_size("names", 16.0))
                    .font(theme.font("names", SEMIBOLD))
                    .color(theme.text)
                    .wrapping(text::Wrapping::None)
            )
            .width(Length::Fill)
            .clip(true)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !installed.modes.is_empty() {
            name = name.push(kit::tag(
                look,
                modes_text(&installed.modes),
                theme.secondary,
                false,
                None,
            ));
        }
        let mut body = column![
            column![
                name,
                text(Self::byline(installed))
                    .size(13)
                    .font(theme.font("small", REGULAR))
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(2)
        ]
        .spacing(8);
        // What it is, two lines at most, on every card alike so a row's
        // cards stay level. It was a tooltip over the picture, as wide as
        // its words, running off Settings.
        if !installed.description.is_empty() {
            body = body.push(
                container(
                    text(two_lines(
                        &installed.description,
                        picture * 16.0 / 9.0 - 24.0,
                    ))
                    .size(13)
                    .line_height(iced::widget::text::LineHeight::Absolute(18.0.into()))
                    .color(theme.secondary),
                )
                .height(36)
                .clip(true),
            );
        }
        if let Some(based_on) = &installed.based_on {
            body = body.push(
                text(format!("Remixes {based_on}"))
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        body = body.push(self.cost(installed));
        body = body.push(
            row![
                self.main_action(installed),
                Space::new().width(Length::Fill),
                self.more_menu(installed)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        card(
            theme,
            applied,
            column![shown, container(body).padding(12)].into(),
        )
    }

    /// An installed theme as a row: a small preview, its name and what it
    /// is, and what can be done with it.
    fn list_row(self, installed: &'a Installed) -> Element<'a, Message> {
        let look = self.cx.look;
        let theme = look.theme;
        let applied = self.applied(installed);
        let shown: Element<'a, Message> = match &installed.preview {
            Some(handle) => image(handle.clone())
                .content_fit(iced::ContentFit::Cover)
                .width(104)
                .height(60)
                .border_radius(6.0)
                .into(),
            None => container(colour_preview(&installed.palette_preview, 60.0))
                .width(104)
                .into(),
        };
        let mut name = row![
            text(installed.name.clone())
                .size(theme.type_size("names", 16.0))
                .font(theme.font("names", SEMIBOLD))
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if applied {
            name = name.push(kit::tag(
                look,
                self.applied_tag(installed),
                theme.accent,
                true,
                Some(Icon::Check),
            ));
        }
        let mut about = Self::byline(installed);
        if !installed.modes.is_empty() {
            if !about.is_empty() {
                about.push_str(" · ");
            }
            about.push_str(modes_text(&installed.modes));
        }
        let words = column![
            name,
            text(about)
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
            self.cost(installed),
        ]
        .spacing(4)
        .width(Length::Fill);
        let actions = container(
            row![self.main_action(installed), self.more_menu(installed)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 16.0,
            ..iced::Padding::ZERO
        });
        card(
            theme,
            applied,
            container(row![shown, words, actions].spacing(12))
                .padding(12)
                .into(),
        )
    }

    fn cost(self, installed: &'a Installed) -> Element<'a, Message> {
        theme_marketplace::cost_line(
            self.cx.look,
            &installed.cost,
            &installed.notes,
            self.s.marketplace.cost_details.contains(&installed.id),
            marketplace_message(theme_marketplace::Event::ToggleCostDetails(
                installed.id.clone(),
            )),
        )
    }

    /// What an installed theme does first: Apply, or Turn off theme on the
    /// one that's applied.
    fn main_action(self, installed: &Installed) -> Element<'a, Message> {
        let shown = self.main_button(installed);
        self.ring(Spot::Main(installed.id.clone()), shown)
    }

    fn main_button(self, installed: &Installed) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        // On trial, Keep is what's left to do, as on the banner.
        if self.on_trial(installed) {
            return action(
                theme,
                "Keep",
                Kind::Primary,
                Some(Message::ThemeTrial(true)),
            )
            .small()
            .into();
        }
        if self.applied(installed) {
            action(
                theme,
                "Turn off theme",
                Kind::Plain,
                Some(theme_message(Event::TurnOff)),
            )
            .small()
            .into()
        } else {
            action(
                theme,
                "Apply",
                Kind::Primary,
                Some(theme_message(Event::Choose(installed.id.clone()))),
            )
            .small()
            .into()
        }
    }

    /// What else can be done with an installed theme, behind "…": Try for
    /// 30 seconds, Roll back, Remove.
    fn more_menu(self, installed: &'a Installed) -> Element<'a, Message> {
        let look = self.cx.look;
        let theme = look.theme;
        let open = self.s.marketplace.menu.as_deref() == Some(installed.id.as_str());
        let face = look
            .icon_button(
                Icon::Ellipsis,
                // No tip over the menu it opened.
                if open {
                    String::new()
                } else {
                    format!("More for {}", installed.name)
                },
                marketplace_message(theme_marketplace::Event::Menu(
                    (!open).then(|| installed.id.clone()),
                )),
            )
            .size(28.0, 18);
        let face = self.ring(Spot::More(installed.id.clone()), face);
        let popup = open.then(|| {
            let package_id = installed
                .id
                .strip_prefix(crate::themes::INSTALLED)
                .unwrap_or(&installed.id)
                .to_owned();
            let item = |label: String, icon, danger| Item {
                label: label.into(),
                icon: Some(icon),
                danger,
                slot: true,
                disabled: false,
                subtitle: None,
                check: controls::Check::None,
            };
            let mut items = column![].spacing(2);
            if !self.applied(installed) {
                items = items.push(self.ring(
                    Spot::Try(installed.id.clone()),
                    look.menu_item(
                        item("Try for 30 seconds".into(), Icon::Clock, false),
                        Light::Pointer,
                        marketplace_message(theme_marketplace::Event::TryInstalled(
                            installed.id.clone(),
                        )),
                    ),
                ));
            }
            if let Some(previous) = &installed.previous {
                items = items.push(self.ring(
                    Spot::RollBack(package_id.clone()),
                    look.menu_item(
                        item(format!("Roll back to {previous}"), Icon::History, false),
                        Light::Pointer,
                        marketplace_message(theme_marketplace::Event::RollBack(package_id.clone())),
                    ),
                ));
            }
            // Remove asks once more in place, as it takes the theme's
            // settings with it.
            let asked = self.s.marketplace.removing.as_deref() == Some(package_id.as_str());
            let remove_spot = Spot::Remove(package_id.clone());
            items = items.push(self.ring(
                remove_spot,
                look.menu_item(
                    item(
                        if asked {
                            "Click again to remove".to_owned()
                        } else {
                            "Remove…".to_owned()
                        },
                        Icon::Trash,
                        true,
                    ),
                    Light::Pointer,
                    marketplace_message(if asked {
                        theme_marketplace::Event::Uninstall(package_id)
                    } else {
                        theme_marketplace::Event::AskUninstall(package_id)
                    }),
                ),
            ));
            container(items)
                .padding(6)
                .width(MENU)
                .style(controls::menu_panel(theme))
                .into()
        });
        Popover::new(face, popup, Side::Below)
            .width(Width::Own(MENU))
            .right()
            .gap(4.0)
            .outside(marketplace_message(theme_marketplace::Event::Menu(None)))
            .into()
    }

    /// Lock entries or packages the core refused, with why.
    fn refused_themes(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut rows = column![].spacing(10);
        for refused in &self.cx.themes.refused {
            rows = rows.push(
                column![
                    text(refused.id.clone())
                        .size(14)
                        .font(SEMIBOLD)
                        .color(theme.danger_text),
                    form::note_in(refused.reason.clone(), theme.danger_text),
                ]
                .spacing(3),
            );
        }
        form::group([
            form::heading(theme, "THEMES THAT COULDN'T BE OPENED"),
            form::note(
                theme,
                "They stay installed, but Yapfast won't use files it can't check.",
            ),
            rows.into(),
        ])
    }

    /// The person's own themes, made in Create: chosen and edited here as in
    /// Appearance.
    fn own_themes(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let own_applied = self
            .cx
            .themes
            .custom
            .iter()
            .any(|custom| self.cx.themes.chosen_id() == custom.id);
        let mut header = row![container(form::heading(theme, "YOUR THEMES")).width(Length::Fill)]
            .spacing(8)
            .height(28)
            .align_y(Alignment::Center);
        if own_applied {
            header = header.push(
                action(
                    theme,
                    "Turn off theme",
                    Kind::Plain,
                    Some(theme_message(Event::TurnOff)),
                )
                .small(),
            );
        }
        header = header.push(
            action(
                theme,
                "New theme",
                Kind::Ghost,
                Some(theme_message(Event::Creator(theme_creator::Event::Open))),
            )
            .small()
            .icon(self.cx.look.icon(
                Icon::CirclePlus,
                16,
                crate::theme::rgb(theme.secondary),
            )),
        );
        column![header, self.theme_tiles(false)].spacing(8).into()
    }

    /// Create, before the creator opens: what it does and New theme, then
    /// the person's own themes to edit.
    fn create_tab(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut parts = vec![kit::notice(
            self.cx.look,
            Icon::Palette,
            theme.accent_text(),
            "Make a theme of your own",
            Some("Pick a colour or a picture and Yapfast works out the rest, keeping text readable. Then shape it: message style, corners, fonts, glass. It's saved with your themes, and Export makes it a package you can share.".into()),
            vec![self.ring(
                Spot::NewTheme,
                action(
                    theme,
                    "New theme",
                    Kind::Primary,
                    Some(theme_message(Event::Creator(theme_creator::Event::Open))),
                )
                .icon(
                    self.cx
                        .look
                        .icon(Icon::CirclePlus, 18, crate::theme::rgb(theme.on_accent)),
                ),
            )],
        )];
        if !self.cx.themes.custom.is_empty() {
            parts.push(form::group([
                form::heading(theme, "YOUR THEMES"),
                form::note(theme, "Click one to show it, or its pencil to edit it."),
                self.theme_tiles(false),
            ]));
        }
        column(parts).spacing(24).width(Length::Fill).into()
    }

    /// Everything the applied theme lets the person set, under its name:
    /// its options, people's content and its sounds. Nothing when it has
    /// none of them.
    fn applied_group(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let id = self.cx.themes.chosen_id();
        let installed = self
            .cx
            .themes
            .installed
            .iter()
            .find(|installed| installed.id == id);
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        if let Some(installed) = installed
            && let Some(options) = self.theme_options(installed)
        {
            parts.push(options);
        }
        let name = installed.map_or("The applied theme", |installed| installed.name.as_str());
        if let Some(phrase) = self.cx.themes.content_phrase(id) {
            let on = self.cx.layout.theme_content.contains(id);
            parts.push(form::group([
                form::heading(theme, "PEOPLE'S CONTENT"),
                switch(
                    "Let the theme style people's profiles and media",
                    on,
                    Some(theme_message(Event::ThemeContent(id.to_owned(), !on))),
                )
                .description(format!(
                    "{name} can draw {phrase}. Off, everyone's pictures, colours and profiles show as they made them."
                ))
                .view(self.cx.look),
            ]));
        }
        if let Some((notifications, interface)) = self.cx.themes.has_sounds(id)
            && (notifications || interface)
        {
            let prefs = self.cx.themes.sound_prefs(id).unwrap_or_default();
            let mut sounds = vec![form::heading(theme, "SOUNDS")];
            if notifications {
                sounds.push(
                    switch(
                        "Notification sounds",
                        prefs.on,
                        Some(theme_message(Event::ThemeNotificationSounds(
                            id.to_owned(),
                            !prefs.on,
                        ))),
                    )
                    .description(format!(
                        "{name}'s sounds for messages, mentions, calls and the rest, in place of Yapfast's. Your sound volumes and Do Not Disturb still apply."
                    ))
                    .view(self.cx.look),
                );
            }
            if interface {
                sounds.push(
                    switch(
                        "Interface sounds — clicks, hovers, menus, send",
                        prefs.interface,
                        Some(theme_message(Event::ThemeInterfaceSounds(
                            id.to_owned(),
                            !prefs.interface,
                        ))),
                    )
                    .description("Only while Yapfast is in front, and never in Do Not Disturb.")
                    .view(self.cx.look),
                );
                let owned = id.to_owned();
                // The volume only while the sounds it sets are on.
                if prefs.interface {
                    sounds.push(
                        slider(
                            "Interface sound volume",
                            u32::from(prefs.volume),
                            100,
                            move |volume| {
                                theme_message(Event::ThemeInterfaceVolume(
                                    owned.clone(),
                                    volume as u8,
                                ))
                            },
                        )
                        .step(1)
                        .shown(format!("{}%", prefs.volume))
                        .mark(false)
                        .view(theme),
                    );
                }
            }
            parts.push(form::group(sounds));
        }
        if parts.is_empty() {
            return Space::new().height(0).into();
        }
        let mut group = vec![
            form::rule(theme),
            column![
                text(name.to_owned())
                    .size(16)
                    .font(SEMIBOLD)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None),
                form::note(
                    theme,
                    "Settings of the theme that's applied. They're kept if you switch away and come back.",
                ),
            ]
            .spacing(2)
            .into(),
        ];
        group.extend(parts);
        column(group).spacing(16).width(Length::Fill).into()
    }

    /// THEME OPTIONS: the applied package's switches, choices and sliders.
    pub(super) fn theme_options(self, installed: &'a Installed) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let mut rows = column![form::heading(theme, "THEME OPTIONS")].spacing(8);
        let mut any = false;
        for option in &installed.options {
            if !option.active(&installed.options, &installed.choices) {
                continue;
            }
            any = true;
            let value = option.value(&installed.choices);
            let id = installed.id.clone();
            let option_id = option.id.clone();
            let message = move |value| {
                theme_message(Event::ThemeOption(id.clone(), option_id.clone(), value))
            };
            let note = (!option.description.is_empty())
                .then(|| form::note(theme, option.description.clone()));
            let control: Element<'a, Message> = match &option.kind {
                options::Kind::Toggle { .. } => {
                    let on = value.as_bool().unwrap_or(false);
                    switch(
                        option.label.clone(),
                        on,
                        Some(message(serde_json::Value::Bool(!on))),
                    )
                    .description(option.description.clone())
                    .view(self.cx.look)
                }
                options::Kind::Choice { choices, .. } => {
                    let current = value
                        .as_str()
                        .and_then(|value| choices.iter().position(|choice| choice.value == value))
                        .unwrap_or(usize::MAX);
                    let labels: Vec<&'a str> =
                        choices.iter().map(|choice| choice.label.as_str()).collect();
                    let values: Vec<String> =
                        choices.iter().map(|choice| choice.value.clone()).collect();
                    let widest = (labels.len() as f32 * 120.0).max(360.0);
                    let mut parts = column![
                        text(option.label.clone())
                            .size(15)
                            .font(REGULAR)
                            .color(theme.text),
                        controls::choices_within(theme, &labels, current, widest, move |index| {
                            message(serde_json::Value::String(values[index].clone()))
                        }),
                    ]
                    .spacing(6);
                    if let Some(note) = note {
                        parts = parts.push(note);
                    }
                    parts.into()
                }
                options::Kind::Slider { min, max, step, .. } => {
                    option_slider(theme, option, &value, *min, *max, *step, message, note)
                }
                options::Kind::Hue { .. } => {
                    option_slider(theme, option, &value, 0.0, 360.0, 5.0, message, note)
                }
            };
            rows = rows.push(control);
        }
        any.then(|| rows.into())
    }

    /// How much of an installed theme runs, and its motion.
    fn theme_performance(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let performance = self.cx.layout.theme_performance.index() as usize;
        let said = match performance {
            0 => "Everything an installed theme has: every animation, at the rate it asks for.",
            1 => {
                "Heavy animations stay still, the rest run at up to 30 frames a second and pause while Yapfast isn't focused."
            }
            _ => "Colours, fonts, shapes and spacing only: no theme animation, effect or motion.",
        };
        column![
            form::rule(theme),
            form::group([
                form::heading(theme, "PERFORMANCE"),
                controls::choices(theme, &["Full", "Balanced", "Light"], performance, |index| {
                    theme_message(Event::Performance(index as i32))
                }),
                form::note(theme, said),
            ]),
            column![
                form::heading(theme, "MOTION"),
                switch(
                    "Reduce motion",
                    self.cx.layout.reduced_motion(),
                    Some(theme_message(Event::ReduceMotion(
                        !self.cx.layout.reduced_motion(),
                    ))),
                )
                .description(
                    "Themes' animations, text effects and hover motion stay still, whatever Performance is set to, and GIFs, emoji and stickers stop playing. Accessibility has it too.",
                )
                .view(self.cx.look),
                switch(
                    "Transitions between channels",
                    self.cx.layout.theme_transitions,
                    Some(theme_message(Event::ThemeTransitions(
                        !self.cx.layout.theme_transitions,
                    ))),
                )
                .description(
                    "A theme's fade or dissolve when you change channel or server, at Full. Off, a channel shows at once.",
                )
                .view(self.cx.look),
            ]
            .spacing(12),
        ]
        .spacing(16)
        .width(Length::Fill)
        .into()
    }
}

/// A theme on trial, over everything at the top of the window, Settings
/// open or not, so the theme can be looked at in place: its name, the
/// seconds left, Revert and Keep.
pub fn trial_banner(cx: crate::ctx::Ctx<'_>) -> Option<Element<'_, Message>> {
    let theme = cx.look.theme;
    let trial = cx.themes.trial()?;
    let (raised, edge) = (theme.raised, theme.edge);
    let pill = container(
        row![
            text(format!(
                "Trying {} · back in {} s",
                trial.name, trial.seconds
            ))
            .size(14)
            .font(SEMIBOLD)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
            action(
                theme,
                "Revert",
                Kind::Plain,
                Some(Message::ThemeTrial(false))
            )
            .small(),
            action(
                theme,
                "Keep",
                Kind::Primary,
                Some(Message::ThemeTrial(true))
            )
            .small(),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(iced::Padding {
        left: 14.0,
        right: 10.0,
        ..iced::Padding::ZERO
    })
    .height(48)
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        border: Border {
            color: edge,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: crate::style::raised_shadow(),
        ..crate::style::rounded(raised, 10)
    });
    Some(
        container(pill)
            .center_x(Length::Fill)
            .padding(iced::Padding {
                top: 12.0,
                ..iced::Padding::ZERO
            })
            .into(),
    )
}

/// `words` cut to about two lines `width` wide at 13 px, at a word, with
/// "…" where they go on: iced draws no ellipsis of its own, and a card's
/// description stopped mid-sentence.
fn two_lines(words: &str, width: f32) -> String {
    // About 7 px a character at 13 px, a little short so the "…" shows.
    let fits = ((width / 7.2).floor().max(8.0) as usize) * 2 - 4;
    if words.chars().count() <= fits {
        return words.to_owned();
    }
    let cut: String = words.chars().take(fits).collect();
    let cut = match cut.rfind(' ') {
        Some(space) if space > fits / 2 => &cut[..space],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ', '-']))
}

/// A theme's card or row: on the sidebar's ground, an accent ring while
/// applied.
fn card<'a>(
    theme: &crate::theme::Theme,
    applied: bool,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let (ground, accent, line) = (theme.sidebar, theme.accent, theme.line);
    container(body)
        .padding(if applied { 2 } else { 1 })
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: Border {
                color: if applied { accent } else { line },
                width: if applied { 2.0 } else { 1.0 },
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A theme's colours as a small window, `height` tall, while it has no
/// preview picture: its rail, channels and three messages.
fn colour_preview<'a>(palette: &yapfast_core::theme::Theme, height: f32) -> Element<'a, Message> {
    let c = |colour| color(colour);
    let (rail, sidebar, chat, accent, ink, muted) = (
        c(palette.rail),
        c(palette.sidebar),
        c(palette.chat),
        c(palette.accent),
        c(palette.text),
        c(palette.muted),
    );
    let blob = |w: Length, h: f32, colour: Color, radius: f32| -> Element<'a, Message> {
        container(Space::new())
            .width(w)
            .height(h)
            .style(move |_| crate::style::rounded(colour, radius))
            .into()
    };
    let channels = column((0..5).map(|index| {
        blob(
            Length::FillPortion(if index == 1 { 60 } else { 45 }),
            6.0,
            if index == 1 { ink } else { muted },
            3.0,
        )
    }))
    .spacing(10)
    .padding([14, 8]);
    let messages = column((0..3).map(|index| {
        row![
            blob(
                Length::Fixed(18.0),
                18.0,
                if index == 0 { accent } else { muted },
                9.0
            ),
            column![
                blob(Length::FillPortion(40), 6.0, ink, 3.0),
                blob(Length::FillPortion(70), 5.0, muted, 2.5)
            ]
            .spacing(5)
        ]
        .spacing(8)
        .into()
    }))
    .spacing(12)
    .padding([16, 12]);
    container(
        row![
            container(Space::new())
                .width(Length::FillPortion(8))
                .height(Length::Fill)
                .style(move |_| crate::style::fill(rail)),
            container(channels)
                .width(Length::FillPortion(24))
                .height(Length::Fill)
                .style(move |_| crate::style::fill(sidebar)),
            container(messages).width(Length::FillPortion(68)),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(height)
    .clip(true)
    .style(move |_| crate::style::fill(chat))
    .into()
}

/// A theme option's slider, what it is under it.
#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
fn option_slider<'a>(
    theme: &crate::theme::Theme,
    option: &options::ThemeOption,
    value: &serde_json::Value,
    min: f64,
    max: f64,
    step: f64,
    message: impl Fn(serde_json::Value) -> Message + 'a,
    note: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let steps = ((max - min) / step).round().max(1.0) as u32;
    let chosen = value.as_f64().unwrap_or(min).clamp(min, max);
    let at = ((chosen - min) / step).round().clamp(0.0, f64::from(steps)) as u32;
    let mut control = column![
        slider(option.label.clone(), at, steps, move |at| {
            message(number(min + f64::from(at) * step))
        })
        .step(1)
        .shown(option.describe(value))
        .mark(false)
        .view(theme)
    ]
    .spacing(6);
    if let Some(note) = note {
        control = control.push(note);
    }
    control.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_long_description_ends_in_an_ellipsis_at_a_word() {
        let words = "A pastel handheld console of our own design: a pink-to-lilac plastic shell with a star D-pad, candy buttons and a little speaker grille";
        let cut = super::two_lines(words, 300.0);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(cut.chars().count() < words.chars().count());
        assert!(!cut.contains(" …"));
        assert_eq!(super::two_lines("Short.", 300.0), "Short.");
    }
}
