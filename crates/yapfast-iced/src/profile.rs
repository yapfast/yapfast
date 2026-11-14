//! The profile card a member's line or a message's author opens, as the
//! Slint client's `ProfileCard`: the banner, the avatar with its
//! decoration and presence, the name with its whole style, the username and
//! pronouns, the server tag, three lines of bio and the roles. The full
//! profile and its other tabs are M1's.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use iced::{
    Alignment, Border, Color, Element, Font, Padding, Point, Rectangle, Size, Task,
    advanced::{graphics::text::font_system, image::Handle},
    widget::{Space, column, container, mouse_area, opaque, rich_text, row, span, stack, text},
};
use yapfast_core::{
    application::Action,
    chat::{self, Presence},
};

use crate::{
    app::{App, Message},
    fonts::{BOLD, ITALIC, REGULAR, SEMIBOLD, shown_name, style_font},
    looks::{self, Chip},
    paint::{self, StyledPicture},
    pictures::{Mark, shared},
    style::{CARD_EDGE, CARD_GROUND, CARD_INK, RAISED, corners, faded, hex, rounded},
    timeline::runs,
    widgets::{Above, Pic, Place},
};

pub const WIDTH: f32 = 300.0;
const BANNER: f32 = 105.0;
const AVATAR: f32 = 80.0;
const PAD: f32 = 16.0;

/// The open card: whose, where, whether the pointer is on it (its
/// decoration plays then), and what it alone shows, let go of when it
/// closes.
pub struct Card {
    pub user: String,
    pub at: Point,
    pub pointed: bool,
    /// Its pictures at the sizes it draws them, by address.
    pictures: RefCell<HashMap<(String, u32, u32), Handle>>,
    /// The name drawn with its whole style, and what it was drawn of.
    name: RefCell<Option<StyledName>>,
}

struct StyledName {
    name: String,
    /// The style, by where the core keeps it, and the font system's
    /// version: a font loaded since for a script the name uses draws it
    /// again.
    key: (usize, iced::advanced::graphics::text::Version),
    image: Handle,
    picture: StyledPicture,
}

impl App {
    pub fn open_card(&mut self, user: String, at: Point) -> Task<Message> {
        self.client.dispatch(Action::ShowProfile {
            user: user.clone(),
            large: false,
        });
        self.card = Some(Card {
            user,
            at,
            pointed: false,
            pictures: Default::default(),
            name: Default::default(),
        });
        self.sync()
    }

    /// A picture for the card at its size, made once while it's open.
    fn card_picture(
        &self,
        card: &Card,
        url: &str,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        let key = (url.to_owned(), width, height);
        if let Some(handle) = card.pictures.borrow().get(&key) {
            return Some(handle.clone());
        }
        let picture = self.client.state().pictures.get(url)?;
        if picture.width == 0 || picture.height == 0 {
            return None;
        }
        let handle = if picture.width == width && picture.height == height {
            shared(picture)
        } else {
            let pixels = paint::fitted(
                &picture.rgba,
                (picture.width, picture.height),
                (width, height),
                cover,
            );
            Handle::from_rgba(width, height, pixels)
        };
        card.pictures.borrow_mut().insert(key, handle.clone());
        Some(handle)
    }

    pub fn card_view(&self) -> Option<Element<'_, Message>> {
        let card = self.card.as_ref()?;
        let state = self.client.state();
        let view = state.profile.as_ref().filter(|view| !view.large)?;
        let profile = &view.profile;
        let cosmetics = state.cosmetics;
        let ink = hex(CARD_INK);
        // The banner: its picture, playing while the card is open, else the
        // accent colour, else the raised grey.
        let banner_size = Size::new(WIDTH, BANNER);
        let top_corners = [12.0, 12.0, 0.0, 0.0];
        let moving = profile
            .banner
            .as_deref()
            .and_then(chat::moving_banner)
            .and_then(|url| {
                self.moving(&url, banner_size, Rectangle::with_size(banner_size), true)
            });
        let still = profile
            .banner
            .as_deref()
            .and_then(|url| self.card_picture(card, url, WIDTH as u32, BANNER as u32, true));
        let banner_ground = profile.accent.map_or(hex(RAISED), hex);
        let mut banner = stack![
            container(Space::new())
                .width(WIDTH)
                .height(BANNER)
                .style(move |_| rounded(banner_ground, corners(top_corners)))
        ];
        if let Some(moving) = moving {
            banner = banner.push(moving.radius_each(top_corners).still(still));
        } else if let Some(still) = still {
            banner = banner.push(Pic::new(still, banner_size).radius_each(top_corners));
        }
        // The avatar on a ring of the card's ground, over the banner's edge.
        let ring = AVATAR + 12.0;
        let ring_size = Size::new(ring, ring);
        let mut avatar = stack![
            container(Space::new())
                .width(ring)
                .height(ring)
                .style(move |_| rounded(hex(CARD_GROUND), ring / 2.0))
        ];
        let face: Element<'_, Message> = match profile
            .avatar
            .as_deref()
            .and_then(|url| self.card_picture(card, url, AVATAR as u32, AVATAR as u32, true))
        {
            Some(picture) => Pic::new(picture, ring_size)
                .at(Rectangle::new(
                    Point::new(6.0, 6.0),
                    Size::new(AVATAR, AVATAR),
                ))
                .radius(AVATAR / 2.0)
                .into(),
            None => container(self.avatar(None, &profile.name, AVATAR, AVATAR / 2.0, 1.0))
                .padding(6)
                .into(),
        };
        avatar = avatar.push(face);
        if let Some(decoration) = cosmetics.decoration(profile.decoration.as_deref()) {
            let side = (AVATAR * 1.2).round();
            let inset = 6.0 - (side - AVATAR) / 2.0;
            let at = Rectangle::new(Point::new(inset, inset), Size::new(side, side));
            let moving = card
                .pointed
                .then(|| self.moving(&looks::animated_url(decoration), ring_size, at, false))
                .flatten();
            if let Some(moving) = moving {
                avatar = avatar.push(moving);
            } else if let Some(still) =
                self.card_picture(card, decoration, side as u32, side as u32, false)
            {
                avatar = avatar.push(Pic::new(still, ring_size).at(at));
            }
        }
        if let Some(presence) = profile
            .presence
            .filter(|presence| *presence != Presence::Offline)
            && let Some(dot) = self.mark(Mark::Dot(presence, CARD_GROUND))
        {
            // The phone stands 5 px taller than the dot.
            let tall = if presence == Presence::Mobile {
                5.0
            } else {
                0.0
            };
            let at = Rectangle::new(
                Point::new(ring - 26.0, ring - 26.0 - tall),
                Size::new(16.0, 16.0 + tall),
            );
            avatar = avatar.push(Pic::new(dot, ring_size).at(at));
        }
        let top = stack![
            Space::new()
                .width(WIDTH)
                .height(BANNER + AVATAR / 2.0 + 8.0),
            banner,
            container(Above::new(avatar.width(ring).height(ring), 10.0)).padding(Padding {
                top: BANNER - AVATAR / 2.0 - 6.0,
                right: 0.0,
                bottom: 0.0,
                left: PAD - 6.0,
            }),
        ];
        let mut handle_line = row![].spacing(6).align_y(Alignment::Center);
        if !profile.handle.is_empty() {
            let pronouns = if profile.pronouns.is_empty() {
                String::new()
            } else {
                format!(" \u{2022} {}", profile.pronouns)
            };
            handle_line = handle_line.push(
                text(format!("{}{pronouns}", profile.handle))
                    .size(14)
                    .color(ink),
            );
        }
        if let Some(tag) = cosmetics.tag(profile.tag.as_ref()) {
            handle_line = handle_line.push(self.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                Chip {
                    size: 12.0,
                    font: REGULAR,
                    height: 24.0,
                    pad: 8.0,
                    radius: 8.0,
                    fill: Color::from_rgba8(0x97, 0x97, 0x9f, 0x1f as f32 / 255.0),
                    ink,
                },
                1.0,
            ));
        }
        // The empty room of the activities the card leaves out keeps its
        // spacing, as the Slint client's empty layout does.
        let mut body = column![
            column![self.card_name(card), handle_line].spacing(2),
            Space::new().height(0)
        ]
        .spacing(12)
        .padding([0.0, PAD]);
        if !profile.bio.trim().is_empty() {
            let spans: Vec<_> = runs(&profile.bio)
                .into_iter()
                .map(|run| {
                    let font = if run.code {
                        Font::MONOSPACE
                    } else if run.bold {
                        BOLD
                    } else if run.italic {
                        ITALIC
                    } else {
                        REGULAR
                    };
                    span(run.text).font(font).color(ink)
                })
                .collect();
            // Three lines and no more, as the card shows.
            let line = 13.0 * 1.3;
            body = body.push(
                container(
                    rich_text::<(), _, _, _>(spans)
                        .size(13)
                        .line_height(text::LineHeight::Absolute(line.into())),
                )
                .max_height(line * 3.0)
                .clip(true),
            );
        }
        if !profile.roles.is_empty() {
            let mut chips = row![].spacing(4);
            for role in &profile.roles {
                let dot = role.color.map_or(hex(0x5c606a), hex);
                chips = chips.push(
                    container(
                        row![
                            container(Space::new())
                                .width(12)
                                .height(12)
                                .style(move |_| rounded(dot, 6)),
                            text(role.name.clone()).size(12).font(SEMIBOLD).color(ink)
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding(Padding {
                        top: 0.0,
                        right: 8.0,
                        bottom: 0.0,
                        left: 6.0,
                    })
                    .height(24)
                    .align_y(Alignment::Center)
                    .style(move |_| container::Style {
                        border: Border {
                            color: faded(ink, 0.2),
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }),
                );
            }
            body = body.push(chips.wrap().vertical_spacing(4));
        }
        let content = container(column![top, body].padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: PAD,
            left: 0.0,
        }))
        .width(WIDTH)
        .style(|_| container::Style {
            background: Some(hex(CARD_GROUND).into()),
            border: Border {
                color: hex(CARD_EDGE),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });
        let pointer = mouse_area(content)
            .on_enter(Message::PointCard(true))
            .on_exit(Message::PointCard(false));
        Some(Place::new(opaque(pointer), card.at, 8.0).into())
    }

    /// The name, with its whole style drawn once while the card is open,
    /// or in plain bold.
    fn card_name(&self, card: &Card) -> Element<'_, Message> {
        let state = self.client.state();
        let Some(view) = &state.profile else {
            return Space::new().into();
        };
        let profile = &view.profile;
        let style = state.cosmetics.style(profile.style.as_deref());
        let Some(style) = looks::name_look(style, &profile.name, false) else {
            return text(profile.name.clone())
                .size(20)
                .font(BOLD)
                .color(hex(CARD_INK))
                .into();
        };
        let key = (
            profile
                .style
                .as_ref()
                .map_or(0, |style| Arc::as_ptr(style) as *const () as usize),
            font_system().read().expect("font system").version(),
        );
        let mut cached = card.name.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|made| made.name != profile.name || made.key != key)
        {
            // Its font first, so the font system has it.
            let _ = style_font(Some(&style), 700);
            let mut system = font_system().write().expect("font system");
            let mut picture = paint::styled_name(
                system.raw(),
                &shown_name(&profile.name, Some(&style)),
                &style,
                20.0,
                700,
            );
            let mut pixels = std::mem::take(&mut picture.canvas.pixels);
            paint::finish(&mut pixels);
            let image = Handle::from_rgba(picture.canvas.width, picture.canvas.height, pixels);
            *cached = Some(StyledName {
                name: profile.name.clone(),
                key,
                image,
                picture,
            });
        }
        let made = cached.as_ref().expect("made above");
        let margin = made.picture.margin as f32;
        Pic::new(
            made.image.clone(),
            Size::new(made.picture.width, made.picture.height),
        )
        .at(Rectangle::new(
            Point::new(-margin, -margin),
            Size::new(
                made.picture.canvas.width as f32,
                made.picture.canvas.height as f32,
            ),
        ))
        .into()
    }
}
