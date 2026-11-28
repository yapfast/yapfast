//! Appearance's choices for the running app and its tray icon.

use std::{path::Path, sync::Arc};

use iced::{
    Alignment, Border, Color, Element, Length, Rectangle, Size, Task,
    advanced::{Layout, Widget, layout, mouse, renderer, widget::Tree},
    widget::{self, button, column, container, image, row, stack, tooltip},
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
    widgets::Hover,
};

const MIN_TILE_WIDTH: f32 = 88.0;
const MAX_COLUMNS: usize = 5;
const TILE_HEIGHT: f32 = 86.0;
const TILE_GAP: f32 = 8.0;
const PREVIEW: u32 = 48;
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
    crate::app::settings(SettingsEvent::DesktopIcon(event))
}

impl Settings {
    pub(super) fn desktop_icon_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let Some(icons) = self.icons.as_mut() else {
            return Task::none();
        };
        if !matches!(&event, Event::Picked(..)) {
            if icons.picker.is_some() {
                return Task::none();
            }
            icons.error = None;
        }
        let (target, choice) = match event {
            Event::FollowApplication(on) => {
                cx.layout.prefs.tray_uses_application_icon = Some(on);
                cx.keep("tray_uses_application_icon", on);
                icons.tray = (!on).then(|| Grid::new(cx.client.root(), IconTarget::Tray));
                cx.request(Request::Changed(Pref::DesktopIcon(IconTarget::Tray)));
                return Task::none();
            }
            Event::Choose(target, choice) => (target, choice),
            Event::Pick(target) => {
                let picker = widget::Id::unique();
                icons.picker = Some(picker.clone());
                let title = crate::i18n::tr!(cx.i18n, "Choose an icon").to_owned();
                let filter = crate::i18n::tr!(cx.i18n, "Images").to_owned();
                return Task::perform(
                    async move {
                        let file = rfd::AsyncFileDialog::new()
                            .set_title(title)
                            .add_filter(filter, &["png", "ico", "jpg", "jpeg", "webp"])
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
                    move |result| message(Event::Picked(target, picker, result)),
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
        let theme = self.cx.look.theme;
        let follows = prefs.tray_uses_application_icon();
        let enabled = icons.picker.is_none();
        let mut parts = vec![
            form::rule(theme),
            self.icon_picker(
                "APPLICATION ICON",
                IconTarget::Application,
                &prefs.application_icon,
                &icons.application,
                icons,
            ),
            form::switch(
                crate::i18n::tr!(self.cx.i18n, "Use the application icon in the tray"),
                follows,
                enabled.then(|| message(Event::FollowApplication(!follows))),
            )
            .description(crate::i18n::tr!(self.cx.i18n, "Show the application icon in the notification area. Turn this off to choose a separate tray icon."))
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
        parts.push(form::rule(theme));
        column(parts).spacing(16).width(Length::Fill).into()
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
            IconChoice::Custom if grid.custom.is_none() => &IconChoice::Default,
            _ => choice,
        };
        let enabled = icons.picker.is_none();
        let tiles = widget::responsive(move |size| {
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
            .push(self.custom_icon(
                target,
                grid.custom.as_ref(),
                *chosen == IconChoice::Custom,
                enabled,
                width,
            ))
            .spacing(TILE_GAP)
            .into()
        })
        .height(Length::Shrink);
        let mut parts = vec![form::heading(look.theme, label), tiles.into()];
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
        width: f32,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let saved = preview.is_some();
        let label = if saved {
            "Custom image"
        } else {
            "Upload image…"
        };
        let face = self
            .icon_face(label, preview, current, width)
            .on_press_maybe(enabled.then(|| {
                message(if saved {
                    Event::Choose(target, IconChoice::Custom)
                } else {
                    Event::Pick(target)
                })
            }))
            .style(icon_tile_style(theme, current, true));
        let outline = Element::new(DashedOutline {
            color: if current { theme.accent } else { theme.faint },
            thickness: if current { 3.0 } else { 1.0 },
        });
        let tile = stack![face, outline].width(width).height(TILE_HEIGHT);
        if !saved || !enabled {
            return tile.into();
        }
        let replace = action(
            theme,
            crate::i18n::tr!(self.cx.i18n, "Replace"),
            Kind::Plain,
            Some(message(Event::Pick(target))),
        )
        .small()
        .wide(24.0);
        let replace = container(replace)
            .align_y(Alignment::End)
            .width(width)
            .height(TILE_HEIGHT)
            .padding(6);
        Hover::new(tile, replace).into()
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
        let theme = self.cx.look.theme;
        let face = self
            .icon_face(tile.name, tile.preview.as_ref(), current, width)
            .on_press_maybe(enabled.then(|| message(Event::Choose(target, tile.icon.clone()))))
            .style(icon_tile_style(theme, current, false));
        tooltip(face, tip(theme, tile.name), tooltip::Position::Top).into()
    }

    fn icon_face(
        self,
        label: &'static str,
        preview: Option<&image::Handle>,
        current: bool,
        width: f32,
    ) -> button::Button<'a, Message> {
        let look = self.cx.look;
        let theme = look.theme;
        let preview: Element<'a, Message> = match preview {
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
        button(
            column![
                face,
                elide(label).size(12).centred().color(if current {
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
    }
}

fn icon_tile_style(
    theme: &crate::theme::Theme,
    current: bool,
    custom: bool,
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
                width: if custom {
                    0.0
                } else if current {
                    3.0
                } else {
                    1.0
                },
                radius: 10.0.into(),
            },
            ..Default::default()
        }
    }
}

/// The custom tile's dashes follow its layout without a separate widget per dash.
struct DashedOutline {
    color: Color,
    thickness: f32,
}

impl Widget<Message, iced::Theme, iced::Renderer> for DashedOutline {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _: &mut Tree,
        _: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }

    fn draw(
        &self,
        _: &Tree,
        renderer: &mut iced::Renderer,
        _: &iced::Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let area = layout.bounds();
        let thickness = self.thickness;
        let mut paint = |bounds| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..Default::default()
                },
                self.color,
            );
        };
        let dashes = |length: f32| {
            let available = (length - 14.0).max(0.0);
            let count = ((available + 3.0) / 8.0) as u32;
            let start = (length - (count as f32 * 8.0 - 3.0)) / 2.0;
            (0..count).map(move |index| start + index as f32 * 8.0)
        };
        for x in dashes(area.width) {
            for y in [area.y, area.y + area.height - thickness] {
                paint(Rectangle {
                    x: area.x + x,
                    y,
                    width: 5.0,
                    height: thickness,
                });
            }
        }
        for y in dashes(area.height) {
            for x in [area.x, area.x + area.width - thickness] {
                paint(Rectangle {
                    x,
                    y: area.y + y,
                    width: thickness,
                    height: 5.0,
                });
            }
        }
    }
}
