//! The editor's menus: a picture's (change, remove), the server tag's
//! picker and its list, and the pencil that opens them.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{button, column, container, opaque, row, text},
};
use yapfast_core::{application::ProfileEditor, chat::ProfilePicture};

use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Light, lights},
    form,
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::{faded, rounded, scrollbar},
};

use super::{Editor, Event, Pop, TAG_SEARCH, message, server_id};

/// A menu row.
pub(super) fn item<'a>(
    label: std::borrow::Cow<'a, str>,
    icon: Option<Icon>,
    danger: bool,
) -> Item<'a> {
    Item {
        label,
        icon,
        danger,
        slot: false,
        disabled: false,
        subtitle: None,
        check: Check::None,
    }
}

impl App {
    /// A picture's menu, Change and Remove (Reset in a server), under it.
    pub(super) fn picture_menu<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        kind: ProfilePicture,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let pop = match kind {
            ProfilePicture::Avatar => Pop::Avatar,
            ProfilePicture::Banner => Pop::Banner,
        };
        let word = if core.main() { "Remove" } else { "Reset" };
        let (change, reset) = match kind {
            ProfilePicture::Avatar => ("Change Avatar", format!("{word} Avatar")),
            ProfilePicture::Banner => ("Change Banner", format!("{word} Banner")),
        };
        let list = (editor.pop == Some(pop)).then(|| {
            self.menu_list(vec![
                (
                    item(change.into(), Some(Icon::Image), false),
                    message(Event::Pick(kind)),
                ),
                (
                    item(reset.into(), Some(Icon::X), true),
                    message(Event::Reset(kind)),
                ),
            ])
        });
        let popover = Popover::new(base, list, Side::Below)
            .width(Width::Own(180.0))
            .gap(4.0)
            .outside(message(Event::Pop(None)));
        if kind == ProfilePicture::Banner {
            popover.right().into()
        } else {
            popover.into()
        }
    }

    /// A menu's rows in its panel.
    pub(super) fn menu_list<'a>(&'a self, rows: Vec<(Item<'a>, Message)>) -> Element<'a, Message> {
        let mut list = column![].spacing(2);
        for (entry, pressed) in rows {
            list = list.push(self.menu_item(entry, Light::Pointer, pressed));
        }
        opaque(
            container(list)
                .padding(6)
                .width(Length::Fill)
                .style(controls::menu_panel(&self.theme)),
        )
    }

    /// The pencil, or the lock where Nitro is wanted, on a dark disc.
    pub(super) fn edit_badge(&self, locked: bool) -> Element<'_, Message> {
        let icon = if locked { Icon::Lock } else { Icon::Pencil };
        container(self.glyph(icon, 16, Color::WHITE))
            .center(32)
            .style(|_| {
                rounded(
                    Color {
                        a: 0.7,
                        ..Color::BLACK
                    },
                    16,
                )
            })
            .into()
    }

    /// The server tag worn, which opens the servers whose tags can be.
    pub(super) fn tag_picker<'a>(
        &'a self,
        editor: &'a Editor,
        inks: &crate::profile::look::Inks,
    ) -> Element<'a, Message> {
        let state = self.client.state();
        let profile = editor.shown.look.profile();
        let tag = state.cosmetics.tag(profile.tag.as_ref());
        let ink2 = inks.ink2;
        let mut face = row![].spacing(5).align_y(Alignment::Center);
        face = match tag {
            Some(tag) => face.push(self.ctx().tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: crate::fonts::REGULAR,
                    height: 20.0,
                    pad: 0.0,
                    radius: 0.0,
                    fill: Color::TRANSPARENT,
                    ink: inks.ink,
                },
                1.0,
            )),
            None => face.push(text("Server Tag").size(12).color(ink2).font(iced::Font {
                style: iced::font::Style::Italic,
                ..iced::Font::default()
            })),
        };
        face = face.push(self.glyph(Icon::ChevronDown, 14, ink2));
        let ink = inks.ink;
        let chip = button(face)
            .padding(Padding {
                left: 7.0,
                right: 10.0,
                top: 0.0,
                bottom: 0.0,
            })
            .height(24)
            .on_press(message(Event::Pop(Some(Pop::Tag))))
            .style(move |_, status| button::Style {
                background: Some(faded(ink, if lights(status) { 0.12 } else { 0.07 }).into()),
                border: Border::default().rounded(7),
                ..Default::default()
            });
        let list = (editor.pop == Some(Pop::Tag)).then(|| self.tag_list(editor));
        Popover::new(chip, list, Side::Below)
            .width(Width::Own(270.0))
            .gap(4.0)
            .outside(message(Event::Pop(None)))
            .into()
    }

    /// The servers whose tags can be worn, found by name or tag, under No
    /// Server Tag.
    pub(super) fn tag_list<'a>(&'a self, editor: &'a Editor) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let chosen = state
            .profile_editor
            .as_ref()
            .and_then(|core| core.chosen_tag.clone())
            .flatten();
        let query = editor.tag_query.trim().to_lowercase();
        let mut rows = column![self.menu_item(
            item("No Server Tag".into(), None, false),
            Light::Pointer,
            message(Event::Tag(String::new())),
        )]
        .spacing(2);
        for server in &state.servers {
            let Some(tag) = &server.tag else {
                continue;
            };
            if !query.is_empty()
                && !server.name.to_lowercase().contains(&query)
                && !tag.text.to_lowercase().contains(&query)
            {
                continue;
            }
            let id = server_id(&server.id);
            let mut entry = item(format!("{}  {}", server.name, tag.text).into(), None, false);
            if chosen.as_deref() == Some(id) {
                entry.check = Check::Current;
            }
            rows = rows.push(self.menu_item(
                entry,
                Light::Pointer,
                message(Event::Tag(id.to_owned())),
            ));
        }
        opaque(
            container(
                column![
                    form::field(theme, TAG_SEARCH, "Search", &editor.tag_query)
                        .size(13)
                        .padding(7)
                        .on_input(|query| message(Event::TagQuery(query))),
                    scroll_view(rows)
                        .height(Length::Shrink)
                        .style(scrollbar(theme)),
                ]
                .spacing(6),
            )
            .padding(8)
            .max_height(290)
            .width(Length::Fill)
            .style(controls::menu_panel(theme)),
        )
    }
}
