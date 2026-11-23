//! Appearance's choices for the running app and its tray icon.

use std::{path::Path, sync::Arc};

use iced::{
    Alignment, Border, Color, Element, Length, Task,
    widget::{self, button, column, container, image, row, stack, text, tooltip},
};
use yapfast_core::theme::BUILT_IN;

use super::{Event as SettingsEvent, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action, lights, tip},
    ctx::{Cx, Pref, Request},
    desktop::{self, IconChoice, IconTarget},
    elide::elide,
    form,
    layout::Prefs,
    pictures::Icon,
};

const MIN_TILE_WIDTH: f32 = 88.0;
const MAX_COLUMNS: usize = 5;
const TILE_HEIGHT: f32 = 86.0;
const TILE_GAP: f32 = 8.0;
const PREVIEW: u32 = 46;
const YAP: &str = "yap";

/// Thumbnails are made as Appearance opens and reused until it closes.
pub(super) struct Icons {
    application: Grid,
    tray: Option<Grid>,
    picker: Option<widget::Id>,
    error: Option<(IconTarget, String)>,
}

struct Grid {
    presets: Vec<Tile>,
    custom: Option<image::Handle>,
}

struct Tile {
    name: &'static str,
    icon: IconChoice,
    preview: Option<image::Handle>,
}

impl Icons {
    pub(super) fn new(root: &Path, prefs: &Prefs) -> Option<Self> {
        if !desktop::desktop().supports_desktop_icons() {
            return None;
        }
        Some(Self {
            application: Grid::new(root, IconTarget::Application),
            tray: (!prefs.tray_uses_application_icon()).then(|| Grid::new(root, IconTarget::Tray)),
            picker: None,
            error: None,
        })
    }

    fn grid_mut(&mut self, target: IconTarget) -> Option<&mut Grid> {
        match target {
            IconTarget::Application => Some(&mut self.application),
            IconTarget::Tray => self.tray.as_mut(),
        }
    }
}

impl Grid {
    fn new(root: &Path, target: IconTarget) -> Self {
        let presets = BUILT_IN
            .iter()
            .map(|preset| {
                let icon = if preset.id == YAP {
                    IconChoice::Default
                } else {
                    IconChoice::Theme(preset.id.into())
                };
                Tile {
                    preview: thumbnail(root, target, &icon),
                    name: preset.name,
                    icon,
                }
            })
            .collect();
        Self {
            presets,
            custom: thumbnail(root, target, &IconChoice::Custom),
        }
    }
}

fn thumbnail(root: &Path, target: IconTarget, icon: &IconChoice) -> Option<image::Handle> {
    let (mut pixels, width, height) = desktop::desktop().icon_preview(root, target, icon)?;
    crate::paint::to_native(&mut pixels);
    Some(image::Handle::from_rgba(width, height, pixels))
}

#[derive(Clone, Debug)]
pub enum Event {
    Choose(IconTarget, IconChoice),
    FollowApplication(bool),
    Pick(IconTarget),
    Picked(IconTarget, widget::Id, Option<Result<Arc<[u8]>, String>>),
}

fn message(event: Event) -> Message {
    Message::Settings(SettingsEvent::DesktopIcon(event))
}

impl Settings {
    pub(super) fn desktop_icon_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let Some(icons) = self.icons.as_mut() else {
            return Task::none();
        };
        let (target, choice) = match event {
            Event::FollowApplication(on) => {
                if icons.picker.is_some() {
                    return Task::none();
                }
                cx.layout.prefs.tray_uses_application_icon = Some(on);
                cx.keep("tray_uses_application_icon", on);
                icons.tray = (!on).then(|| Grid::new(cx.client.root(), IconTarget::Tray));
                icons.error = None;
                cx.request(Request::Changed(Pref::DesktopIcon(IconTarget::Tray)));
                return Task::none();
            }
            Event::Choose(target, choice) => {
                if icons.picker.is_some() {
                    return Task::none();
                }
                icons.error = None;
                (target, choice)
            }
            Event::Pick(target) => {
                if icons.picker.is_some() {
                    return Task::none();
                }
                icons.error = None;
                let picker = widget::Id::unique();
                icons.picker = Some(picker.clone());
                return Task::perform(
                    async {
                        let file = rfd::AsyncFileDialog::new()
                            .set_title("Choose an icon")
                            .add_filter("Images", &["png", "ico", "jpg", "jpeg", "webp"])
                            .pick_file()
                            .await?;
                        Some(
                            tokio::task::spawn_blocking(move || {
                                desktop::desktop().prepare_icon(file.path()).map(Arc::from)
                            })
                            .await
                            .unwrap_or_else(|_| Err("Could not prepare the icon.".into())),
                        )
                    },
                    move |result| message(Event::Picked(target, picker.clone(), result)),
                );
            }
            Event::Picked(target, picker, result) => {
                if icons.picker.as_ref() != Some(&picker) {
                    return Task::none();
                }
                icons.picker = None;
                let Some(result) = result else {
                    return Task::none();
                };
                if let Err(error) = result
                    .and_then(|png| desktop::desktop().save_icon(cx.client.root(), target, &png))
                {
                    icons.error = Some((target, error));
                    return Task::none();
                }
                if let Some(grid) = icons.grid_mut(target) {
                    grid.custom = thumbnail(cx.client.root(), target, &IconChoice::Custom);
                }
                (target, IconChoice::Custom)
            }
        };
        // Keep the mode inferred from an older layout before changing a
        // choice, so selecting a different icon can't flip the switch.
        if cx.layout.prefs.tray_uses_application_icon.is_none() {
            let follows = cx.layout.prefs.tray_uses_application_icon();
            cx.layout.prefs.tray_uses_application_icon = Some(follows);
            cx.keep("tray_uses_application_icon", follows);
        }
        let key = match target {
            IconTarget::Application => {
                cx.layout.prefs.application_icon = choice.clone();
                "application_icon"
            }
            IconTarget::Tray => {
                cx.layout.prefs.tray_icon = Some(choice.clone());
                "tray_icon"
            }
        };
        cx.keep(key, choice);
        cx.request(Request::Changed(Pref::DesktopIcon(target)));
        Task::none()
    }
}

impl<'a> Page<'a> {
    pub(super) fn desktop_icons(self) -> Element<'a, Message> {
        let Some(icons) = self.s.icons.as_ref() else {
            return widget::Space::new().into();
        };
        let prefs = &self.cx.layout.prefs;
        let follows = prefs.tray_uses_application_icon();
        let enabled = icons.picker.is_none();
        let mut parts = vec![
            self.icon_picker(
                "APPLICATION ICON",
                IconTarget::Application,
                &prefs.application_icon,
                &icons.application,
                icons,
            ),
            form::switch(
                "Use the application icon in the tray",
                follows,
                enabled.then(|| message(Event::FollowApplication(!follows))),
            )
            .view(self.cx.look),
        ];
        if let Some(grid) = &icons.tray {
            parts.push(self.icon_picker(
                "TRAY ICON",
                IconTarget::Tray,
                prefs.tray_icon.as_ref().unwrap_or(&IconChoice::Default),
                grid,
                icons,
            ));
        }
        form::group(parts)
    }

    fn icon_picker(
        self,
        label: &'static str,
        target: IconTarget,
        choice: &'a IconChoice,
        grid: &'a Grid,
        icons: &'a Icons,
    ) -> Element<'a, Message> {
        let look = self.cx.look;
        let chosen = match choice {
            IconChoice::Theme(id) if id == YAP || yapfast_core::theme::find(id).is_none() => {
                &IconChoice::Default
            }
            _ => choice,
        };
        let enabled = icons.picker.is_none();
        let presets = widget::responsive(move |size| {
            let fit = ((size.width + TILE_GAP) / (MIN_TILE_WIDTH + TILE_GAP)).floor() as usize;
            let columns = fit.clamp(1, MAX_COLUMNS);
            let width = (size.width - (columns - 1) as f32 * TILE_GAP) / columns as f32;
            column(grid.presets.chunks(columns).map(|tiles| {
                row(tiles
                    .iter()
                    .map(|tile| self.icon_tile(target, tile, tile.icon == *chosen, enabled, width)))
                .spacing(TILE_GAP)
                .into()
            }))
            .spacing(TILE_GAP)
            .into()
        })
        .height(Length::Shrink);
        let mut parts = vec![form::heading(look.theme, label), presets.into()];
        parts.push(self.custom_icon(
            target,
            grid.custom.as_ref(),
            *chosen == IconChoice::Custom,
            enabled,
        ));
        if let Some((_, error)) = icons.error.as_ref().filter(|(which, _)| *which == target) {
            parts.push(form::error(look.theme, error.as_str()).into());
        }
        form::group(parts)
    }

    fn custom_icon(
        self,
        target: IconTarget,
        preview: Option<&image::Handle>,
        current: bool,
        enabled: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let upload = action(
            theme,
            if preview.is_some() {
                "Replace image…"
            } else {
                "Upload custom image…"
            },
            Kind::Plain,
            enabled.then(|| message(Event::Pick(target))),
        )
        .small();
        let Some(preview) = preview else {
            return upload.into();
        };
        let image = image(preview.clone()).width(30).height(30);
        let mut custom = row![image].spacing(8).align_y(Alignment::Center);
        if current {
            custom = custom
                .push(text("Custom image").size(13).color(theme.text))
                .push(self.icon_check());
        } else {
            custom = custom.push(
                action(
                    theme,
                    "Use saved image",
                    Kind::Ghost,
                    enabled.then(|| message(Event::Choose(target, IconChoice::Custom))),
                )
                .small(),
            );
        }
        form::beside(custom, upload)
    }

    fn icon_check(self) -> Element<'a, Message> {
        let look = self.cx.look;
        let (accent, ink) = (look.theme.accent, look.theme.on_accent);
        container(look.glyph(Icon::Check, 13, ink))
            .center_x(20)
            .center_y(20)
            .style(move |_| crate::style::rounded(accent, 10))
            .into()
    }

    fn icon_tile(
        self,
        target: IconTarget,
        tile: &Tile,
        current: bool,
        enabled: bool,
        width: f32,
    ) -> Element<'a, Message> {
        let look = self.cx.look;
        let theme = look.theme;
        let preview: Element<'a, Message> = match &tile.preview {
            Some(handle) => image(handle.clone()).width(PREVIEW).height(PREVIEW).into(),
            None => look.glyph(Icon::Image, 22, theme.secondary).into(),
        };
        let preview = container(preview).center_x(Length::Fill).center_y(PREVIEW);
        let mut face = stack![preview];
        if current {
            face = face.push(
                container(self.icon_check())
                    .align_x(Alignment::End)
                    .width(Length::Fill),
            );
        }
        let face = button(
            column![
                face,
                elide(tile.name).size(12).centred().color(if current {
                    theme.text
                } else {
                    theme.secondary
                }),
            ]
            .spacing(3),
        )
        .padding(8)
        .width(width)
        .height(TILE_HEIGHT)
        .on_press_maybe(enabled.then(|| message(Event::Choose(target, tile.icon.clone()))))
        .style(icon_tile_style(theme, current));
        tooltip(face, tip(theme, tile.name), tooltip::Position::Top).into()
    }
}

fn icon_tile_style(
    theme: &crate::theme::Theme,
    current: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let (accent, faint, line, hover) = (theme.accent, theme.faint, theme.line, theme.hover);
    move |_, status| {
        let hovered = lights(status);
        button::Style {
            background: if current {
                Some(
                    Color {
                        a: if hovered { 0.22 } else { 0.14 },
                        ..accent
                    }
                    .into(),
                )
            } else {
                hovered.then(|| hover.into())
            },
            border: Border {
                color: if current {
                    accent
                } else if hovered {
                    faint
                } else {
                    line
                },
                width: if current { 3.0 } else { 1.0 },
                radius: 10.0.into(),
            },
            ..Default::default()
        }
    }
}
