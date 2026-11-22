//! Settings → Themes' marketplace, GitHub URL and install-review flow.
//!
//! This is deliberately a self-contained controller for the Settings host
//! to map into its own message. Network work stays in iced [`Task`]s on the
//! core background runtime; checked bytes stay in [`Review`] until Install.
//! Reviewed bytes remain protected until Install or the explicit Try flow.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use iced::{
    Alignment, Element, Length, Task,
    widget::{Space, button, column, container, image, row, text},
};
use yapfast_core::{
    runtime::BackgroundRuntime,
    themes::{
        check::plain::{self, Weight},
        install::Fetched,
        remote::{Catalog, Listing, Repository},
    },
};
use yapfast_iced_kit::theme_page as kit;

use crate::{
    controls::{self, Kind},
    form,
    layout::{self, Layout},
    look::Look,
    pictures::Icon,
    theme_remote::{self, Preview, Previews},
    themes::{SoundPrefs, Themes},
};

const SEARCH: &str = "theme-marketplace-search";
const URL: &str = "theme-repository-url";
const MOST_NOTES: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Installed,
    Marketplace,
    AddFromUrl,
    /// Create, before the creator opens.
    Create,
}

/// A checked theme waiting at the protected install review. Nothing in the
/// fetched package is persisted until [`Event::Install`].
pub struct Review {
    pub fetched: Fetched,
    pub previews: Previews,
    pub notification_sounds: bool,
    pub interface_sounds: bool,
    pub content: Option<String>,
    pub notify_on: bool,
    pub interface_on: bool,
    pub content_on: bool,
    stock: crate::theme::Theme,
}

impl Review {
    pub(crate) fn new(fetched: Fetched, previews: Previews) -> Self {
        let package = fetched.package();
        let mut notification_sounds = false;
        let mut interface_sounds = false;
        let mut content = None;
        for resolved in package
            .modes()
            .into_iter()
            .filter_map(|mode| package.resolve(mode).ok())
        {
            notification_sounds |= resolved.sounds.keys().any(|slot| !slot.interface());
            interface_sounds |= resolved.sounds.keys().any(|slot| slot.interface());
            if content.is_none() {
                content = resolved.content.phrase();
            }
        }
        Self {
            fetched,
            previews,
            notification_sounds,
            interface_sounds,
            content,
            // The Slint build's safe review defaults: notification replacements
            // on, interface cues and people's content off.
            notify_on: true,
            interface_on: false,
            content_on: false,
            stock: crate::themes::stock_theme(),
        }
    }

    #[cfg(test)]
    pub fn notes(&self) -> String {
        format_notes(&self.fetched.package().notes)
    }

    pub fn modes(&self) -> &'static str {
        match self.fetched.package().modes().as_slice() {
            [
                yapfast_core::themes::Mode::Dark,
                yapfast_core::themes::Mode::Light,
            ] => "Dark and light",
            [yapfast_core::themes::Mode::Light] => "Light",
            _ => "Dark",
        }
    }
}

/// The two remote tabs' state. `catalog == None` is either not opened yet
/// or loading; `catalog_loading` distinguishes them.
#[derive(Default)]
pub struct Marketplace {
    pub tab: Tab,
    pub query: String,
    pub url: String,
    pub catalog: Option<Catalog>,
    pub catalog_loading: bool,
    pub catalog_error: Option<String>,
    pub fetching: Option<String>,
    pub error: Option<String>,
    pub review: Option<Review>,
    pub previews: HashMap<String, Preview>,
    /// Set after a successful install for the Settings host to apply/redraw.
    pub installed: Option<String>,
    pub theme_changed: bool,
    pub theme_applied: bool,
    pub gallery: bool,
    pub cost_details: HashSet<String>,
    /// The installed theme whose "…" menu is open, by id.
    pub menu: Option<String>,
    /// The package whose Remove was chosen once in its "…" menu, which then
    /// asks to be chosen again: removing drops the theme's settings too.
    pub removing: Option<String>,
    /// Where the keys are on the page, while Tab put them there.
    pub focus: Option<Spot>,
}

/// A place on Settings → Themes the keys can be, by what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spot {
    Tab(Tab),
    Back,
    /// An installed theme's main button, by its layout id.
    Main(String),
    /// Its "…", by its layout id, and that menu's items.
    More(String),
    Try(String),
    /// Roll back and Remove, by package id.
    RollBack(String),
    Remove(String),
    Retry,
    Find,
    NewTheme,
}

#[derive(Clone, Debug)]
pub enum Event {
    Open,
    Tab(Tab),
    Search(String),
    ToggleGallery(bool),
    ToggleCostDetails(String),
    /// An installed theme's "…" menu opened, or none.
    Menu(Option<String>),
    RetryCatalog,
    CatalogLoaded(Result<Catalog, String>),
    PreviewLoaded(String, Option<Preview>),
    Url(String),
    FindUrl,
    FetchListed(String),
    Fetched(Box<Result<(Fetched, Previews), String>>),
    ReviewNotify(bool),
    ReviewInterface(bool),
    ReviewContent(bool),
    CancelReview,
    TryReview,
    TryInstalled(String),
    KeepTrial,
    RevertTrial,
    RollBack(String),
    /// Remove chosen once: the menu stays open asking for it again.
    AskUninstall(String),
    Uninstall(String),
    Install,
    ClearInstalled,
}

impl Marketplace {
    /// The stock Bonfire look while the protected install review is open.
    /// Settings exposes this as its preview so the whole window, not merely
    /// the review card, gives up the untrusted theme's styling.
    pub fn protected_theme(&self) -> Option<&crate::theme::Theme> {
        self.review.as_ref().map(|review| &review.stock)
    }

    /// Handles one marketplace action. The caller maps this task's [`Event`]
    /// back into its Settings message.
    pub fn update(
        &mut self,
        event: Event,
        runtime: Arc<BackgroundRuntime>,
        root: &Path,
        layout: &mut Layout,
        themes: &mut Themes,
    ) -> Task<Event> {
        // A choice made anywhere closes the "…" menu.
        if !matches!(event, Event::Menu(_)) {
            self.menu = None;
            self.removing = None;
        }
        match event {
            Event::Menu(menu) => {
                self.menu = menu;
                self.removing = None;
            }
            Event::AskUninstall(id) => {
                self.menu = Some(format!("{}{id}", crate::themes::INSTALLED));
                self.removing = Some(id);
            }
            Event::Open => {
                if self.tab == Tab::Marketplace && self.catalog.is_none() && !self.catalog_loading {
                    return self.load_catalog(runtime);
                }
            }
            Event::Tab(tab) => {
                self.tab = tab;
                self.error = None;
                if tab == Tab::Marketplace && self.catalog.is_none() && !self.catalog_loading {
                    return self.load_catalog(runtime);
                }
            }
            Event::Search(query) => self.query = query,
            Event::ToggleGallery(gallery) => self.gallery = gallery,
            Event::ToggleCostDetails(key) => {
                if !self.cost_details.remove(&key) {
                    self.cost_details.insert(key);
                }
            }
            Event::RetryCatalog => return self.load_catalog(runtime),
            Event::CatalogLoaded(result) => {
                self.catalog_loading = false;
                match result {
                    Ok(catalog) => {
                        self.catalog_error = None;
                        let urls = catalog
                            .themes
                            .iter()
                            .filter(|listing| {
                                !listing.preview.is_empty()
                                    && !self.previews.contains_key(&listing.preview)
                                    && !themes.installed.iter().any(|installed| {
                                        installed.id
                                            == format!("{}{}", crate::themes::INSTALLED, listing.id)
                                    })
                            })
                            .map(|listing| listing.preview.clone());
                        #[allow(
                            clippy::needless_collect,
                            reason = "the catalog moves into self before they're fetched"
                        )]
                        let urls = urls.collect::<Vec<_>>();
                        self.catalog = Some(catalog);
                        return Task::batch(urls.into_iter().map(|url| {
                            let runtime = runtime.clone();
                            Task::perform(
                                async move {
                                    let preview = theme_remote::preview(runtime, url.clone()).await;
                                    (url, preview)
                                },
                                |(url, preview)| Event::PreviewLoaded(url, preview),
                            )
                        }));
                    }
                    Err(error) => {
                        self.catalog = None;
                        self.catalog_error = Some(error);
                    }
                }
            }
            Event::PreviewLoaded(url, preview) => {
                if let Some(preview) = preview {
                    self.previews.insert(url, preview);
                }
            }
            // What went wrong stays until the next Find theme: clearing it
            // as the address changed moved the field, which let go of the
            // keys after one letter.
            Event::Url(url) => self.url = url,
            Event::FindUrl => {
                if self.fetching.is_some() {
                    return Task::none();
                }
                let repository = match Repository::parse(&self.url) {
                    Ok(repository) => repository,
                    Err(error) => {
                        self.error = Some(error);
                        return Task::none();
                    }
                };
                self.fetching = Some(self.url.clone());
                self.error = None;
                return Task::perform(theme_remote::fetch(runtime, repository, None), |result| {
                    Event::Fetched(Box::new(result))
                });
            }
            Event::FetchListed(id) => {
                if self.fetching.is_some() {
                    return Task::none();
                }
                let Some(listing) = self
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.themes.iter().find(|listing| listing.id == id))
                    .cloned()
                else {
                    return Task::none();
                };
                self.fetching = Some(id);
                self.error = None;
                return Task::perform(
                    theme_remote::fetch(runtime, listing.repository(), Some(listing)),
                    |result| Event::Fetched(Box::new(result)),
                );
            }
            Event::Fetched(result) => {
                // A fetch this Settings didn't start (one asked before
                // Settings closed and opened again) opens no review.
                if self.fetching.take().is_none() {
                    return Task::none();
                }
                match *result {
                    Ok((fetched, previews)) => {
                        self.error = None;
                        self.review = Some(Review::new(fetched, previews));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Event::ReviewNotify(on) => {
                if let Some(review) = &mut self.review {
                    review.notify_on = on;
                }
            }
            Event::ReviewInterface(on) => {
                if let Some(review) = &mut self.review {
                    review.interface_on = on;
                }
            }
            Event::ReviewContent(on) => {
                if let Some(review) = &mut self.review {
                    review.content_on = on;
                }
            }
            Event::CancelReview => self.review = None,
            Event::TryReview => {
                let Some(review) = self.review.take() else {
                    return Task::none();
                };
                let prefs = SoundPrefs {
                    on: review.notify_on,
                    interface: review.interface_on,
                    ..SoundPrefs::default()
                };
                if review.content_on {
                    layout.theme_content.insert("trial".into());
                } else {
                    layout.theme_content.remove("trial");
                }
                layout.theme_sounds.insert("trial".into(), prefs);
                match themes.try_reviewed(&review.fetched, layout) {
                    Ok(()) => {
                        themes.set_sound_prefs("trial", prefs);
                        self.theme_changed = true;
                    }
                    Err(error) => {
                        layout.theme_sounds.remove("trial");
                        layout.theme_content.remove("trial");
                        self.error = Some(error);
                        self.review = Some(review);
                    }
                }
            }
            Event::TryInstalled(id) => match themes.try_installed(&id) {
                Ok(()) => {
                    self.theme_changed = true;
                }
                Err(error) => self.error = Some(error),
            },
            Event::KeepTrial => match keep_trial(themes, layout, root) {
                Ok(Some(id)) => {
                    self.installed = Some(id);
                    self.tab = Tab::Installed;
                    self.theme_applied = true;
                }
                Ok(None) => {}
                Err(error) => self.error = Some(error),
            },
            Event::RevertTrial => {
                if revert_trial(themes, layout) {
                    self.theme_changed = true;
                }
            }
            Event::RollBack(id) => match themes.roll_back(root, layout, &id) {
                Ok(()) => self.theme_changed = true,
                Err(error) => self.error = Some(error),
            },
            Event::Uninstall(id) => match themes.uninstall(root, layout, &id) {
                Ok(changed) => self.theme_changed |= changed,
                Err(error) => self.error = Some(error),
            },
            Event::Install => {
                let Some(review) = self.review.take() else {
                    return Task::none();
                };
                let id = match themes.install_fetched(root, layout, &review.fetched) {
                    Ok(id) => id,
                    Err(error) => {
                        self.error = Some(error);
                        self.review = Some(review);
                        return Task::none();
                    }
                };

                let prefs = SoundPrefs {
                    on: review.notify_on,
                    interface: review.interface_on,
                    ..SoundPrefs::default()
                };
                if prefs == SoundPrefs::default() {
                    layout.theme_sounds.remove(&id);
                } else {
                    layout.theme_sounds.insert(id.clone(), prefs);
                }
                themes.set_sound_prefs(&id, prefs);
                if review.content_on {
                    layout.theme_content.insert(id.clone());
                } else {
                    layout.theme_content.remove(&id);
                }
                layout::store(root, "theme_sounds", &layout.theme_sounds);
                layout::store(root, "theme_content", &layout.theme_content);

                let before = themes.chosen.clone();
                themes.choose(root, &id);
                if before.as_deref() != Some(id.as_str()) {
                    layout.previous_theme = before;
                }
                layout.theme = Some(id.clone());
                self.installed = Some(id);
                self.tab = Tab::Installed;
                self.theme_applied = true;
            }
            Event::ClearInstalled => self.installed = None,
        }
        Task::none()
    }

    fn load_catalog(&mut self, runtime: Arc<BackgroundRuntime>) -> Task<Event> {
        self.catalog_loading = true;
        self.catalog_error = None;
        Task::perform(theme_remote::catalog(runtime), Event::CatalogLoaded)
    }

    /// Marketplace entries that are not installed and match the current
    /// search by name, author or description.
    pub fn listed<'a>(&'a self, themes: &'a Themes) -> impl Iterator<Item = &'a Listing> + 'a {
        let query = self.query.trim().to_lowercase();
        self.catalog
            .iter()
            .flat_map(|catalog| &catalog.themes)
            .filter(move |listing| {
                !themes.installed.iter().any(|installed| {
                    installed.id == format!("{}{}", crate::themes::INSTALLED, listing.id)
                }) && (query.is_empty()
                    || listing.name.to_lowercase().contains(&query)
                    || listing.author.to_lowercase().contains(&query)
                    || listing.description.to_lowercase().contains(&query))
            })
    }

    /// Draws Marketplace or Add from URL. The Settings host owns the tabs,
    /// the error under them and the review, which it draws over the page
    /// ([`review_dialog`]).
    pub fn view<'a>(&'a self, look: Look<'a>, themes: &'a Themes) -> Element<'a, Event> {
        match self.tab {
            Tab::Marketplace => self.marketplace_view(look, themes),
            Tab::AddFromUrl => self.url_view(look),
            Tab::Installed | Tab::Create => Space::new().height(0).into(),
        }
    }

    fn marketplace_view<'a>(&'a self, look: Look<'a>, themes: &'a Themes) -> Element<'a, Event> {
        let theme = look.theme;
        let search = form::field(theme, SEARCH, "Search the marketplace", &self.query)
            .on_input(Event::Search)
            .padding(iced::Padding {
                top: 9.0,
                right: 36.0,
                bottom: 9.0,
                left: 12.0,
            });
        let search: Element<'a, Event> = iced::widget::stack![
            search,
            container(look.glyph(Icon::Search, 18, theme.muted))
                .padding(iced::Padding {
                    right: 10.0,
                    ..iced::Padding::ZERO
                })
                .align_right(Length::Fill)
                .center_y(Length::Fill),
        ]
        .into();
        let body: Element<'a, Event> = if self.catalog_loading {
            let pair = || -> Element<'a, Event> {
                row![
                    container(kit::skeleton(theme, 248.0)).width(Length::FillPortion(1)),
                    container(kit::skeleton(theme, 248.0)).width(Length::FillPortion(1)),
                ]
                .spacing(12)
                .into()
            };
            column![pair(), pair()].spacing(12).into()
        } else if let Some(error) = &self.catalog_error {
            kit::notice(
                look,
                Icon::TriangleAlert,
                theme.loud,
                "The marketplace can't be shown right now",
                Some(error.clone()),
                vec![
                    self.ring(
                        theme,
                        Spot::Retry,
                        controls::action(
                            theme,
                            "Try again",
                            Kind::Primary,
                            Some(Event::RetryCatalog),
                        )
                        .icon(look.icon(
                            Icon::Refresh,
                            18,
                            crate::theme::rgb(theme.on_accent),
                        )),
                    ),
                    controls::action(
                        theme,
                        "Add from URL",
                        Kind::Plain,
                        Some(Event::Tab(Tab::AddFromUrl)),
                    )
                    .into(),
                ],
            )
        } else {
            let listings: Vec<_> = self.listed(themes).collect();
            if listings.is_empty() && self.query.trim().is_empty() {
                kit::notice(
                    look,
                    Icon::Check,
                    theme.live,
                    "You have them all",
                    Some("Every theme the marketplace lists is installed.".into()),
                    vec![
                        controls::action(
                            theme,
                            "See your themes",
                            Kind::Plain,
                            Some(Event::Tab(Tab::Installed)),
                        )
                        .into(),
                    ],
                )
            } else if listings.is_empty() {
                kit::notice(
                    look,
                    Icon::Search,
                    theme.muted,
                    format!("No themes match \u{201c}{}\u{201d}", self.query.trim()),
                    Some("Try another word, or a theme's author.".into()),
                    vec![
                        controls::action(
                            theme,
                            "Clear the search",
                            Kind::Plain,
                            Some(Event::Search(String::new())),
                        )
                        .into(),
                    ],
                )
            } else {
                iced::widget::responsive(move |size| {
                    let card = ((size.width - 12.0) / 2.0).max(0.0);
                    let picture = ((card - 2.0) * 9.0 / 16.0).round().max(0.0);
                    let mut grid = column![].spacing(12);
                    for pair in listings.chunks(2) {
                        let mut line = row![].spacing(12);
                        for listing in pair {
                            line = line.push(
                                container(self.listing_view(look, listing, picture))
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
            }
        };
        // Nothing to search while the catalog can't be read.
        if self.catalog_error.is_some() && !self.catalog_loading {
            return body;
        }
        column![search, body].spacing(16).width(Length::Fill).into()
    }

    /// A listed theme: its preview, name and who made it, three lines of
    /// what it is, and Install.
    fn listing_view<'a>(
        &'a self,
        look: Look<'a>,
        listing: &'a Listing,
        picture: f32,
    ) -> Element<'a, Event> {
        let theme = look.theme;
        let preview: Element<'a, Event> = match self.previews.get(&listing.preview) {
            Some(preview) => image(preview.handle.clone())
                .height(picture)
                .width(Length::Fill)
                .border_radius(crate::style::corners([7.0, 7.0, 0.0, 0.0]))
                .content_fit(iced::ContentFit::Cover)
                .into(),
            None => {
                let ground = theme.sunken;
                container(Space::new())
                    .width(Length::Fill)
                    .height(picture)
                    .style(move |_| {
                        crate::style::rounded(ground, crate::style::corners([7.0, 7.0, 0.0, 0.0]))
                    })
                    .into()
            }
        };
        let finding = self.fetching.as_deref() == Some(listing.id.as_str());
        let mut body = column![
            column![
                row![
                    container(
                        text(listing.name.clone())
                            .size(16)
                            .font(crate::fonts::SEMIBOLD)
                            .color(theme.text)
                            .wrapping(text::Wrapping::None)
                    )
                    .width(Length::Fill)
                    .clip(true),
                    kit::tag(look, listing.modes(), theme.secondary, false, None),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(format!("by {}", listing.author))
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(2)
        ]
        .spacing(8);
        if !listing.description.is_empty() {
            body = body.push(
                container(
                    text(listing.description.clone())
                        .size(13)
                        .color(theme.secondary)
                        .width(Length::Fill),
                )
                .height(51)
                .clip(true),
            );
        }
        body = body.push(
            controls::action(
                theme,
                if finding { "Fetching…" } else { "Install" },
                Kind::Primary,
                (self.fetching.is_none()).then(|| Event::FetchListed(listing.id.clone())),
            )
            .small(),
        );
        let (ground, line) = (theme.sidebar, theme.line);
        container(column![preview, container(body).padding(12)])
            .padding(1)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: iced::Border {
                    color: line,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Add from URL: the field and Find theme on one line, a word on them,
    /// then what happens next.
    /// `content`, ringed while the keys are on `spot`.
    fn ring<'a>(
        &self,
        theme: &crate::theme::Theme,
        spot: Spot,
        content: impl Into<Element<'a, Event>>,
    ) -> Element<'a, Event> {
        if self.focus.as_ref() == Some(&spot) {
            kit::focus_ring(theme, content)
        } else {
            content.into()
        }
    }

    fn url_view<'a>(&'a self, look: Look<'a>) -> Element<'a, Event> {
        let theme = look.theme;
        let finding = self.fetching.is_some();
        let mut field = form::field(theme, URL, "https://github.com/ada/vaporwave", &self.url)
            .padding([9, 12])
            .width(Length::Fill);
        if !finding {
            field = field.on_input(Event::Url).on_submit(Event::FindUrl);
        }
        column![
            form::group([
                form::heading(theme, "GITHUB REPOSITORY"),
                row![
                    field,
                    self.ring(
                        theme,
                        Spot::Find,
                        controls::action(
                            theme,
                            if finding { "Finding…" } else { "Find theme" },
                            Kind::Primary,
                            (!finding && !self.url.trim().is_empty()).then_some(Event::FindUrl),
                        ),
                    )
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                // What went wrong, under the field it's about.
                match &self.error {
                    Some(error) => row![
                        look.glyph(Icon::TriangleAlert, 16, theme.danger_text),
                        text(error.clone())
                            .size(14)
                            .color(theme.danger_text)
                            .width(Length::Fill),
                    ]
                    .spacing(8)
                    .into(),
                    None => form::note(
                        theme,
                        "The address of a theme's public GitHub repository. A branch, tag or folder in the address works too.",
                    ),
                },
            ]),
            form::rule(theme),
            column![
                form::heading(theme, "WHAT HAPPENS NEXT"),
                kit::step(
                    look,
                    Icon::Download,
                    "Yapfast fetches the theme",
                    "Straight from its repository, at the commit it's at now.",
                ),
                kit::step(
                    look,
                    Icon::Shield,
                    "Every file is checked",
                    "A theme is data, never code: colours, fonts, pictures and sounds, each within limits.",
                ),
                kit::step(
                    look,
                    Icon::Eye,
                    "You review it first",
                    "You see what it changes and what it costs to run, and can try it for 30 seconds, before anything is installed.",
                ),
            ]
            .spacing(16),
        ]
        .spacing(24)
        .width(Length::Fill)
        .into()
    }

    /// What a theme from a URL or the marketplace holds, before anything is
    /// kept, as a dialog over Settings in the stock look: its name, tags,
    /// a card of facts, the switches, and Cancel, Try 30 s and Install in a
    /// footer that stays in reach while the rest scrolls.
    pub fn review_dialog<'a>(&'a self, look: Look<'a>) -> Option<Element<'a, Event>> {
        let review = self.review.as_ref()?;
        let look = Look {
            theme: &review.stock,
            ..look
        };
        let theme = look.theme;
        let package = review.fetched.package();
        let mut head = column![
            row![
                text(package.name.clone())
                    .size(20)
                    .font(crate::fonts::BOLD)
                    .color(theme.text)
                    .width(Length::Fill),
                kit::tag(
                    look,
                    format!("v{}", package.version),
                    theme.secondary,
                    false,
                    None
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ]
        .spacing(4);
        if !package.authors.is_empty() {
            head = head.push(
                text(format!("by {}", package.authors.join(", ")))
                    .size(14)
                    .color(theme.muted),
            );
        }
        let mut parts: Vec<Element<'a, Event>> = vec![head.into()];
        if !package.description.is_empty() {
            parts.push(
                text(package.description.clone())
                    .size(14)
                    .color(theme.secondary)
                    .into(),
            );
        }
        if !review.previews.is_empty() {
            let mut previews = row![].spacing(8);
            for preview in &review.previews {
                previews = previews.push(
                    image(preview.handle.clone())
                        .width(288)
                        .height(180)
                        .border_radius(8.0)
                        .content_fit(iced::ContentFit::Cover),
                );
            }
            parts.push(
                crate::widgets::scroll_view(previews)
                    .across()
                    .style(crate::style::scrollbar(theme))
                    .width(Length::Fill)
                    .into(),
            );
        }
        let listed = review.fetched.listed;
        parts.push(
            row![
                kit::tag(
                    look,
                    if listed {
                        "In the marketplace"
                    } else {
                        "Unreviewed source"
                    },
                    if listed { theme.live } else { theme.loud },
                    false,
                    Some(if listed {
                        Icon::Check
                    } else {
                        Icon::TriangleAlert
                    }),
                ),
                kit::tag(
                    look,
                    "No code · no permissions",
                    theme.live,
                    false,
                    Some(Icon::Shield)
                ),
            ]
            .spacing(6)
            .into(),
        );
        let fact = |name: &'a str, value: Element<'a, Event>| -> Element<'a, Event> {
            row![
                container(text(name).size(13).color(theme.muted)).width(84),
                container(value).width(Length::Fill),
            ]
            .spacing(12)
            .into()
        };
        let value = |words: String| -> Element<'a, Event> {
            text(words).size(13).color(theme.secondary).into()
        };
        let mut facts = column![
            fact(
                "From",
                value(format!(
                    "{} at {}",
                    review.fetched.source,
                    review.fetched.rev.chars().take(7).collect::<String>()
                ))
            ),
            fact("Works in", value(review.modes().to_owned())),
        ]
        .spacing(10);
        if let Some(based_on) = &package.based_on {
            facts = facts.push(fact("Remixes", value(based_on.clone())));
        }
        facts = facts.push(fact(
            "To run",
            cost_line(
                look,
                &review.fetched.report.cost.text(),
                &package.notes,
                self.cost_details.contains("review"),
                Event::ToggleCostDetails("review".into()),
            ),
        ));
        let sidebar = theme.sidebar;
        parts.push(
            container(facts)
                .padding(12)
                .width(Length::Fill)
                .style(move |_| crate::style::rounded(sidebar, 8))
                .into(),
        );
        if review.notification_sounds || review.interface_sounds {
            let mut sounds = vec![form::heading(theme, "SOUNDS")];
            if review.notification_sounds {
                sounds.push(
                    form::switch(
                        "Notification sounds",
                        review.notify_on,
                        Some(Event::ReviewNotify(!review.notify_on)),
                    )
                    .description(
                        "Its sounds for messages, mentions, calls and the rest, in place of Yapfast's.",
                    )
                    .view(look),
                );
            }
            if review.interface_sounds {
                sounds.push(
                    form::switch(
                        "Interface sounds — clicks, hovers, menus, send",
                        review.interface_on,
                        Some(Event::ReviewInterface(!review.interface_on)),
                    )
                    .description(
                        "Only while Yapfast is in front. You can change both later in Settings → Themes.",
                    )
                    .view(look),
                );
            }
            parts.push(form::group(sounds));
        }
        if let Some(content) = &review.content {
            parts.push(form::group([
                form::heading(theme, "PEOPLE'S CONTENT"),
                form::switch(
                    "Let the theme style people's profiles and media",
                    review.content_on,
                    Some(Event::ReviewContent(!review.content_on)),
                )
                .description(format!(
                    "It can draw {content}. Off, everyone's own looks show. You can change it later in Settings → Themes."
                ))
                .view(look),
            ]));
        }
        if let Some(error) = &self.error {
            parts.push(form::note_in(error.clone(), theme.danger_text));
        }
        let (line, footer_ground) = (theme.line, theme.sidebar);
        let footer = container(column![
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| crate::style::fill(line)),
            container(
                row![
                    controls::action(theme, "Cancel", Kind::Ghost, Some(Event::CancelReview)),
                    Space::new().width(Length::Fill),
                    controls::action(theme, "Try 30 s", Kind::Plain, Some(Event::TryReview)),
                    controls::action(theme, "Install", Kind::Primary, Some(Event::Install)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([16, 24]),
        ])
        .width(Length::Fill)
        .style(move |_| {
            crate::style::rounded(footer_ground, crate::style::corners([0.0, 0.0, 12.0, 12.0]))
        });
        let tall = (look.window.height - 64.0).clamp(240.0, 760.0);
        let scroller =
            crate::widgets::scroll_view(column(parts).spacing(16).padding(iced::Padding {
                top: 24.0,
                right: 24.0,
                bottom: 16.0,
                left: 24.0,
            }))
            .style(crate::style::scrollbar(theme))
            .width(Length::Fill)
            .height(Length::Fill);
        Some(
            column![scroller, footer]
                .width(Length::Fill)
                .height(tall)
                .into(),
        )
    }
}

/// Keeps the theme on trial, from the banner or Settings: a reviewed one is
/// installed with the sound and content choices made in its review, then
/// it's chosen. Its id, or `None` with no trial.
pub(crate) fn keep_trial(
    themes: &mut Themes,
    layout: &mut Layout,
    root: &Path,
) -> Result<Option<String>, String> {
    let reviewed = themes.trial_reviewed();
    let Some(id) = themes.keep_trial(root, layout)? else {
        return Ok(None);
    };
    let prefs = layout.theme_sounds.remove("trial");
    let content = layout.theme_content.remove("trial");
    if reviewed && let Some(prefs) = prefs {
        if prefs == SoundPrefs::default() {
            layout.theme_sounds.remove(&id);
        } else {
            layout.theme_sounds.insert(id.clone(), prefs);
        }
        themes.set_sound_prefs(&id, prefs);
        if content {
            layout.theme_content.insert(id.clone());
        } else {
            layout.theme_content.remove(&id);
        }
    }
    layout::store(root, "theme_sounds", &layout.theme_sounds);
    layout::store(root, "theme_content", &layout.theme_content);
    let before = themes.chosen.clone();
    themes.choose(root, &id);
    layout.previous_theme = before;
    layout.theme = Some(id.clone());
    Ok(Some(id))
}

/// Ends a trial, showing again what showed before it. Whether there was one.
pub(crate) fn revert_trial(themes: &mut Themes, layout: &mut Layout) -> bool {
    if themes.revert_trial().is_none() {
        return false;
    }
    layout.theme_sounds.remove("trial");
    layout.theme_content.remove("trial");
    true
}

/// What a theme costs to run, in plain words with a dot (`Light on
/// resources`, `Moderate: animated, ~23 MB memory`), and Details, which
/// opens the whole of it sorted under what it is, with what this Yapfast
/// leaves out of the theme.
pub(super) fn cost_line<'a, M: Clone + 'a>(
    look: Look<'_>,
    cost: &str,
    notes: &[String],
    expanded: bool,
    on_toggle: M,
) -> Element<'a, M> {
    let theme = look.theme;
    let parsed = plain::plain(cost);
    if parsed.is_none() && notes.is_empty() {
        return Space::new().height(0).into();
    }
    let (dot_colour, summary, rows) = match &parsed {
        Some(p) => (
            match p.weight {
                Weight::Light => theme.live,
                Weight::Moderate => theme.muted,
                Weight::Heavy => theme.loud,
            },
            p.summary.clone(),
            p.rows.as_slice(),
        ),
        None => (
            theme.muted,
            "Not everything in it is used".to_owned(),
            [].as_slice(),
        ),
    };
    let dot = container(Space::new())
        .width(8)
        .height(8)
        .style(move |_| crate::style::rounded(dot_colour, 4));
    let (muted, ink) = (theme.muted, theme.text);
    let details = button(
        row![
            text("Details").size(13),
            look.glyph(
                if expanded {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                },
                14,
                theme.muted
            )
        ]
        .spacing(2)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .height(20)
    .on_press(on_toggle)
    .style(controls::ink_style(muted, ink));
    let header = row![
        dot,
        // It wraps where a card is narrow, Details staying in reach.
        container(
            text(summary)
                .size(13)
                .color(theme.secondary)
                .wrapping(text::Wrapping::WordOrGlyph)
        )
        .width(Length::Fill),
        details,
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if !expanded {
        return header.into();
    }
    let mut detail = column![].spacing(8);
    let part = |label: String, words: String| {
        column![
            text(label)
                .size(12)
                .font(crate::fonts::SEMIBOLD)
                .color(theme.muted),
            text(words)
                .size(12)
                .color(theme.secondary)
                .width(Length::Fill),
        ]
        .spacing(2)
    };
    for (label, value) in rows {
        detail = detail.push(part(label.clone(), value.clone()));
    }
    if !notes.is_empty() {
        detail = detail.push(part("Left out by this Yapfast".into(), format_notes(notes)));
    }
    let sunken = theme.sunken;
    column![
        header,
        container(detail)
            .padding(10)
            .width(Length::Fill)
            .style(move |_| crate::style::rounded(sunken, 6)),
    ]
    .spacing(6)
    .into()
}

fn format_notes(notes: &[String]) -> String {
    let mut shown = notes
        .iter()
        .take(MOST_NOTES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    if notes.len() > MOST_NOTES {
        shown += "\nand ";
        shown += &(notes.len() - MOST_NOTES).to_string();
        shown += " more";
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_core::themes::check::Files;

    fn fetched() -> Fetched {
        let mut fetched = Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            "0123456789abcdef0123456789abcdef01234567".into(),
            Files {
                manifest: r##"{
                    "theme_api": 1,
                    "id": "com.example.plum",
                    "name": "Plum",
                    "version": "1.0.0",
                    "variants": { "dark": { "base": "yap", "colours": { "chat": "#2a1a3a" } } }
                }"##
                .into(),
                ..Files::default()
            },
        )
        .unwrap();
        fetched.report.package.as_mut().unwrap().notes =
            (1..=10).map(|number| number.to_string()).collect();
        fetched
    }

    #[test]
    fn a_fetch_from_before_settings_opened_again_opens_no_review() {
        let runtime = Arc::new(BackgroundRuntime::new().unwrap());
        let data = tempfile::tempdir().unwrap();
        let mut layout = Layout::default();
        let mut themes = Themes::read(data.path(), &layout);
        let mut market = Marketplace::default();
        let arrived = || Event::Fetched(Box::new(Ok((fetched(), Vec::new()))));
        let _ = market.update(
            arrived(),
            runtime.clone(),
            data.path(),
            &mut layout,
            &mut themes,
        );
        assert!(market.review.is_none(), "nobody here asked for it");
        market.fetching = Some("https://github.com/ada/plum".into());
        let _ = market.update(arrived(), runtime, data.path(), &mut layout, &mut themes);
        assert!(market.review.is_some(), "asked for, it opens");
    }

    #[test]
    fn review_uses_owner_defaults_and_bounds_notes() {
        let review = Review::new(fetched(), Vec::new());
        assert!(review.notify_on);
        assert!(!review.interface_on);
        assert!(!review.content_on);
        assert_eq!(review.modes(), "Dark");
        assert_eq!(review.notes(), "1\n2\n3\n4\n5\n6\n7\n8\nand 2 more");
    }

    #[test]
    fn marketplace_searches_names_authors_and_descriptions() {
        let root = tempfile::tempdir().unwrap();
        let themes = Themes::read(root.path(), &Layout::default());
        let listing = Listing {
            id: "com.example.plum".into(),
            name: "Plum".into(),
            author: "Ada".into(),
            description: "A quiet purple theme".into(),
            ..Listing::default()
        };
        let mut market = Marketplace {
            catalog: Some(Catalog {
                themes: vec![listing],
                skipped: Vec::new(),
            }),
            ..Marketplace::default()
        };
        for query in ["plum", "ADA", "purple"] {
            market.query = query.into();
            assert_eq!(market.listed(&themes).count(), 1);
        }
        market.query = "orange".into();
        assert_eq!(market.listed(&themes).count(), 0);
    }
}
