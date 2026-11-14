//! Signing in, as the Slint client's `SignInScreen` (`ui/sign-in.slint`)
//! and `DiscordLinkPanel` (`ui/discord.slint`): a centred card on the
//! rail's ground with the local preview beside it. A build without Matrix
//! links Discord: by the QR code Discord's phone app scans, or a pasted
//! token, with Discord's captcha in the browser when it asks for one. A
//! build with Matrix signs in with a homeserver, Matrix ID and password,
//! and offers the saved session again; once in, the verify banner stays
//! over the conversation until the device is verified. Everything goes to
//! the core as the Slint client's callbacks send it.

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{Space, column, container, operation, row, scrollable, stack, text},
};
use yapfast_core::application::{Action, DiscordLink};

use crate::{
    app::{App, Message},
    controls::{Kind, action, bar, field},
    fonts::BOLD,
    pictures::Icon,
    style::{card_shadow, fill, rounded, scrollbar},
    theme::Theme,
    widgets::Pic,
};

/// The fields, for focus and the tests.
pub const HOMESERVER: &str = "sign-in-homeserver";
pub const USERNAME: &str = "sign-in-username";
pub const PASSWORD: &str = "sign-in-password";
pub const TOKEN: &str = "sign-in-token";
pub const RECOVERY: &str = "sign-in-recovery";

/// What the sign-in screen holds as it's filled in.
#[derive(Debug)]
pub struct SignIn {
    pub homeserver: String,
    pub username: String,
    pub password: String,
    /// The pasted Discord token, and whether its field is open: kept
    /// outside the linking's phases, so a token Discord refused stays to
    /// fix.
    pub token: String,
    pub token_open: bool,
    /// A Matrix recovery key, in the verify banner.
    pub recovery: String,
    /// A saved session can be tried again: worked out while the screen
    /// shows, as finding out reads the disk or the credential store.
    pub restore_available: bool,
    /// The QR code shown, the address it carries, and its side in pixels.
    pub code: Option<(String, iced::advanced::image::Handle, f32)>,
    /// The window opened on a saved account that's being restored: until
    /// it's in, or it fails, the window shows only the core's notice
    /// ("Connecting to Discord..."), not the card, whose text at its own
    /// sizes would stay in the renderer's glyph cache for nothing.
    pub starting: bool,
}

impl Default for SignIn {
    fn default() -> Self {
        Self {
            homeserver: "https://matrix.org".into(),
            username: String::new(),
            password: String::new(),
            token: String::new(),
            token_open: false,
            recovery: String::new(),
            restore_available: false,
            code: None,
            starting: false,
        }
    }
}

/// What's done on the sign-in screen and the verify banner.
#[derive(Clone, Debug)]
pub enum Event {
    Homeserver(String),
    Username(String),
    Password(String),
    Login,
    Restore,
    Preview,
    /// Discord: show the code (or a new one), open the token's field,
    /// write the token, link with it, open the captcha again.
    ShowCode,
    OpenToken,
    Token(String),
    LinkToken,
    OpenCaptcha,
    /// Matrix's verify banner.
    Recovery(String),
    Recover,
    RefreshTrust,
}

/// Whether this build signs in with Matrix; one without links Discord.
pub const MATRIX: bool = cfg!(feature = "matrix");

impl App {
    /// Whether the window shows the workspace: signed in, or previewing.
    pub fn workspace(&self) -> bool {
        let state = self.client.state();
        state.preview || state.account.is_some()
    }

    pub fn sign_in_event(&mut self, event: Event) -> Task<Message> {
        let busy = self.client.state().busy;
        match event {
            Event::Homeserver(text) => self.sign_in.homeserver = text,
            Event::Username(text) => self.sign_in.username = text,
            Event::Password(text) => self.sign_in.password = text,
            Event::Login => {
                let form = &self.sign_in;
                self.client.dispatch(Action::Login {
                    homeserver: form.homeserver.clone(),
                    username: form.username.clone(),
                    password: form.password.clone(),
                });
                // As the Slint client: the password goes once it's on its way.
                if !busy && self.client.state().busy {
                    self.sign_in.password.clear();
                }
                return self.sync();
            }
            Event::Restore => {
                self.client.dispatch(Action::Restore);
                return self.sync();
            }
            Event::Preview => {
                self.client.dispatch(Action::Preview);
                return self.sync();
            }
            Event::ShowCode => {
                self.client.dispatch(Action::LinkDiscord);
                return self.sync();
            }
            Event::OpenToken => {
                self.sign_in.token_open = true;
                return operation::focus(TOKEN);
            }
            Event::Token(text) => self.sign_in.token = text,
            Event::LinkToken => {
                if !self.sign_in.token.is_empty() {
                    self.client
                        .dispatch(Action::LinkDiscordToken(self.sign_in.token.clone()));
                    return self.sync();
                }
            }
            Event::OpenCaptcha => {
                self.client.dispatch(Action::OpenDiscordCaptcha);
                return self.sync();
            }
            Event::Recovery(text) => self.sign_in.recovery = text,
            Event::Recover => {
                self.client
                    .dispatch(Action::Recover(self.sign_in.recovery.clone()));
                if !busy && self.client.state().busy {
                    self.sign_in.recovery.clear();
                }
                return self.sync();
            }
            Event::RefreshTrust => {
                self.client.dispatch(Action::RefreshTrust);
                return self.sync();
            }
        }
        Task::none()
    }

    /// Takes in what changed for the sign-in screen: the QR code to show,
    /// and whether a saved session can be tried again.
    pub fn sync_sign_in(&mut self) {
        let state = self.client.state();
        if !state.busy {
            self.sign_in.starting = false;
        }
        if state.preview || state.account.is_some() {
            self.sign_in.code = None;
            self.sign_in.starting = false;
            return;
        }
        match &state.discord_link {
            Some(DiscordLink::Code(address)) => {
                if self
                    .sign_in
                    .code
                    .as_ref()
                    .is_none_or(|(shown, ..)| shown != address)
                {
                    self.sign_in.code =
                        qr_code(address).map(|(code, side)| (address.clone(), code, side));
                }
            }
            _ => self.sign_in.code = None,
        }
        if MATRIX {
            self.sign_in.restore_available = yapfast_core::chat::can_restore(self.client.root());
        }
    }

    /// The sign-in screen: the wordmark, then the card, centred, on the
    /// rail's ground.
    pub fn sign_in_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let rail = theme.rail;
        if self.sign_in.starting {
            return container(
                text(state.notice.clone().unwrap_or_default())
                    .size(14)
                    .color(theme.muted),
            )
            .center(Length::Fill)
            .style(move |_| fill(rail))
            .into();
        }
        let form = if MATRIX {
            self.matrix_form()
        } else {
            self.discord_form()
        };
        let divider = bar(theme.line, 1, Length::Fill);
        let mark = self.glyph(Icon::Logo, 44, theme.accent).boxed(72).style({
            let raised = theme.raised;
            move |_| rounded(raised, 24)
        });
        let preview = column![
            mark,
            text("Just looking?")
                .size(20)
                .font(BOLD)
                .color(theme.text)
                .align_x(Alignment::Center),
            text("Try the layout with sample conversations. Nothing leaves this computer.")
                .size(15)
                .color(theme.secondary)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            action(
                theme,
                "Explore local preview",
                Kind::Plain,
                (!state.busy).then_some(Message::SignIn(Event::Preview)),
            )
            .wide(36.0),
        ]
        .spacing(12)
        .width(220)
        .align_x(Alignment::Center);
        let card = container(
            // Shrink, so the line between takes the height of what's beside it.
            row![container(form).width(Length::Fill), divider, preview]
                .spacing(32)
                .height(Length::Shrink)
                .align_y(Alignment::Center),
        )
        .padding(32)
        .max_width(760)
        .style({
            let chat = theme.chat;
            move |_| container::Style {
                shadow: card_shadow(),
                ..rounded(chat, 12)
            }
        });
        let page = scrollable(
            container(card)
                .padding(Padding {
                    top: 0.0,
                    right: 24.0,
                    bottom: 24.0,
                    left: 24.0,
                })
                .width(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fixed((self.window.height - 64.0).max(0.0))),
        )
        .style(scrollbar(theme))
        .height(Length::Fill);
        let wordmark = container(text("Yapfast").size(24).font(BOLD).color(theme.accent))
            .padding(Padding {
                top: 24.0,
                right: 0.0,
                bottom: 0.0,
                left: 32.0,
            })
            .height(64);
        container(stack![column![wordmark, page]])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| fill(rail))
            .into()
    }

    /// Linking Discord, in a build without Matrix.
    fn discord_form(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let mut form = column![
            title(theme, "Sign in to Yapfast"),
            text("Link your Discord account with your phone.")
                .color(theme.secondary)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            Space::new().height(12),
        ]
        .spacing(8)
        .width(Length::Fill);
        if !state.busy {
            form = form.push(self.link_panel());
        }
        if let Some(notice) = &state.notice
            && (state.busy || state.discord_link.is_none())
        {
            form = form.push(
                text(notice.clone())
                    .size(15)
                    .color(if state.busy {
                        theme.secondary
                    } else {
                        theme.danger
                    })
                    .width(Length::Fill),
            );
        }
        form.into()
    }

    /// Discord's linking, phase by phase, as the Slint client's
    /// `DiscordLinkPanel`: the warning and its buttons (the token's field
    /// under them once asked for), the code and what to do with it, the
    /// captcha, the token being checked.
    fn link_panel(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let form = &self.sign_in;
        let link = self.client.state().discord_link.as_ref();
        match link {
            None | Some(DiscordLink::Failed(_)) => {
                let warning = container(
                    row![
                        self.glyph(Icon::TriangleAlert, 18, theme.danger),
                        text(
                            "Discord doesn't allow apps like Yapfast on personal accounts. \
                             Linking puts your account at some risk of being suspended."
                        )
                        .size(14)
                        .color(theme.text)
                        .width(Length::Fill),
                    ]
                    .spacing(10),
                )
                .padding(12)
                .width(Length::Fill)
                .style({
                    let danger = theme.danger;
                    move |_| container::Style {
                        background: Some(Color { a: 0.1, ..danger }.into()),
                        border: Border {
                            color: Color { a: 0.35, ..danger },
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                });
                let mut panel = column![warning].spacing(12).width(Length::Fill);
                if let Some(DiscordLink::Failed(note)) = link {
                    panel = panel.push(
                        text(note.clone())
                            .size(14)
                            .color(theme.danger)
                            .width(Length::Fill),
                    );
                }
                let again = link.is_some() && !form.token_open;
                let mut buttons = row![action(
                    theme,
                    if again { "Try again" } else { "Show the code" },
                    if form.token_open {
                        Kind::Plain
                    } else {
                        Kind::Primary
                    },
                    Some(Message::SignIn(Event::ShowCode)),
                )]
                .spacing(8);
                if !form.token_open {
                    buttons = buttons.push(action(
                        theme,
                        "Use a token",
                        Kind::Plain,
                        Some(Message::SignIn(Event::OpenToken)),
                    ));
                }
                panel = panel.push(buttons);
                if form.token_open {
                    panel = panel.push(
                        column![
                            label(theme, "TOKEN"),
                            row![
                                field(theme, TOKEN, "Discord token", &form.token, true, true)
                                    .on_input(|text| Message::SignIn(Event::Token(text)))
                                    .on_submit(Message::SignIn(Event::LinkToken)),
                                action(
                                    theme,
                                    "Link",
                                    Kind::Primary,
                                    (!form.token.is_empty())
                                        .then_some(Message::SignIn(Event::LinkToken)),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            text(
                                "Anyone with your token can use your account. Paste it only \
                                 here, and never share it."
                            )
                            .size(13)
                            .color(theme.secondary)
                            .width(Length::Fill),
                        ]
                        .spacing(8),
                    );
                }
                panel.into()
            }
            Some(DiscordLink::Checking) => text("Checking the token...")
                .size(15)
                .color(theme.secondary)
                .into(),
            Some(DiscordLink::Captcha(_)) => column![
                text("Solve the captcha in your browser")
                    .size(17)
                    .font(BOLD)
                    .color(theme.text),
                text(
                    "Discord wants to check you're a person before it links your account. \
                     Yapfast opened the check in your browser; linking finishes once it's \
                     solved."
                )
                .size(14)
                .color(theme.secondary)
                .width(Length::Fill),
                container(action(
                    theme,
                    "Open it again",
                    Kind::Plain,
                    Some(Message::SignIn(Event::OpenCaptcha)),
                ))
                .padding(Padding {
                    top: 8.0,
                    ..Padding::ZERO
                }),
            ]
            .spacing(8)
            .into(),
            Some(DiscordLink::Starting | DiscordLink::Code(_) | DiscordLink::Scanned(_)) => {
                let scanned = match link {
                    Some(DiscordLink::Scanned(name)) => Some(name.as_str()),
                    _ => None,
                };
                let code = form.code.as_ref().filter(|_| scanned.is_none());
                let inside: Element<'_, Message> = match (code, scanned) {
                    (Some((_, code, side)), _) => Pic::square(code.clone(), *side).into(),
                    (None, Some(_)) => column![
                        self.glyph(Icon::Check, 40, theme.live),
                        text("Scanned").size(15).font(BOLD).color(theme.text),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center)
                    .into(),
                    (None, None) => text("Getting a code...").size(14).color(theme.muted).into(),
                };
                let ground = if code.is_some() {
                    Color::WHITE
                } else {
                    theme.sunken
                };
                let square = container(inside)
                    .center(184)
                    .style(move |_| rounded(ground, 12));
                let (heading, words) = match scanned {
                    Some(name) => (
                        "Confirm on your phone".to_owned(),
                        format!("Linking {name}. Tap Log in on your phone to finish."),
                    ),
                    None => (
                        "Scan it with the Discord app".to_owned(),
                        "On your phone, open Discord, go to Settings, tap Scan QR Code, and \
                         point the camera here. A new code replaces this one after a couple of \
                         minutes."
                            .to_owned(),
                    ),
                };
                row![
                    square,
                    column![
                        text(heading).size(17).font(BOLD).color(theme.text),
                        text(words)
                            .size(14)
                            .color(theme.secondary)
                            .width(Length::Fill),
                    ]
                    .spacing(8)
                    .width(Length::Fill)
                ]
                .spacing(20)
                .align_y(Alignment::Center)
                .into()
            }
        }
    }

    /// Matrix's sign-in, in a build with Matrix.
    fn matrix_form(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let form = &self.sign_in;
        let open = !state.busy;
        let input =
            |id, placeholder, value, secure| field(theme, id, placeholder, value, secure, open);
        let mut homeserver = input(HOMESERVER, "", &form.homeserver, false);
        let mut username = input(USERNAME, "@you:matrix.org", &form.username, false);
        let mut password = input(PASSWORD, "", &form.password, true);
        if open {
            homeserver = homeserver.on_input(|text| Message::SignIn(Event::Homeserver(text)));
            username = username.on_input(|text| Message::SignIn(Event::Username(text)));
            password = password
                .on_input(|text| Message::SignIn(Event::Password(text)))
                .on_submit(Message::SignIn(Event::Login));
        }
        let mut form = column![
            title(theme, "Sign in to Yapfast"),
            text("Use your existing Matrix account.")
                .color(theme.secondary)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            Space::new().height(12),
            label(theme, "HOMESERVER"),
            homeserver,
            Space::new().height(4),
            label(theme, "MATRIX ID"),
            username,
            Space::new().height(4),
            label(theme, "PASSWORD"),
            password,
            Space::new().height(12),
            action(
                theme,
                if state.busy {
                    "Connecting..."
                } else {
                    "Sign in"
                },
                Kind::Primary,
                open.then_some(Message::SignIn(Event::Login)),
            )
            .wide(44.0),
        ]
        .spacing(8)
        .width(Length::Fill);
        if let Some(notice) = &state.notice {
            form = form.push(
                text(notice.clone())
                    .size(15)
                    .color(theme.danger)
                    .width(Length::Fill),
            );
        }
        if self.sign_in.restore_available && open {
            form = form.push(
                action(
                    theme,
                    "Retry saved session",
                    Kind::Plain,
                    Some(Message::SignIn(Event::Restore)),
                )
                .wide(36.0),
            );
        }
        form.push(
            text("Sign in with your Matrix password. Single sign-on isn't available yet.")
                .size(13)
                .color(theme.muted)
                .width(Length::Fill),
        )
        .into()
    }

    /// Above the conversation until this Matrix device is verified, as the
    /// Slint client's `VerifyBanner`: a recovery key verifies it, or
    /// verifying it in another client and checking again.
    pub fn verify_banner(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        if state.preview || state.trusted || !MATRIX {
            return None;
        }
        let theme = &self.theme;
        let open = !state.busy;
        let mut key = field(
            theme,
            RECOVERY,
            "Recovery key",
            &self.sign_in.recovery,
            true,
            open,
        );
        if open {
            key = key
                .on_input(|text| Message::SignIn(Event::Recovery(text)))
                .on_submit(Message::SignIn(Event::Recover));
        }
        let lock = self.glyph(Icon::Lock, 18, theme.accent).boxed(36).style({
            let soft = theme.accent_soft;
            move |_| rounded(soft, 18)
        });
        let raised = theme.raised;
        Some(
            container(
                row![
                    lock,
                    column![
                        text("Verify this device to send messages")
                            .font(BOLD)
                            .color(theme.text),
                        text(
                            "Use your Matrix recovery key, or verify Yapfast in another client \
                             and check again."
                        )
                        .size(14)
                        .color(theme.secondary),
                        row![
                            key,
                            action(
                                theme,
                                "Recover",
                                Kind::Primary,
                                open.then_some(Message::SignIn(Event::Recover)),
                            ),
                            action(
                                theme,
                                "Check again",
                                Kind::Plain,
                                open.then_some(Message::SignIn(Event::RefreshTrust)),
                            ),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(8),
                ]
                .spacing(14),
            )
            .padding(16)
            .width(Length::Fill)
            .style(move |_| fill(raised))
            .into(),
        )
    }
}

/// The card's heading.
fn title<'a>(theme: &Theme, words: &'a str) -> Element<'a, Message> {
    text(words)
        .size(26)
        .font(BOLD)
        .color(theme.text)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// A field's label over it, as the Slint client spells them out.
fn label<'a>(theme: &Theme, words: &'a str) -> Element<'a, Message> {
    text(words)
        .size(12)
        .font(BOLD)
        .color(theme.secondary)
        .into()
}

/// The QR code Discord's phone app scans to link, black on white with the
/// blank margin scanners need, at whole pixels a module, up to 176 px.
/// Grey pixels read the same in either renderer's format.
pub fn qr_code(address: &str) -> Option<(iced::advanced::image::Handle, f32)> {
    /// The blank margin scanners need, in modules.
    const QUIET: usize = 4;
    let code = qrcode::QrCode::new(address.as_bytes()).ok()?;
    let width = code.width();
    let modules = code.to_colors();
    let side = width + 2 * QUIET;
    let scale = (176 / side).max(1);
    let pixels_side = side * scale;
    let mut pixels = vec![255u8; pixels_side * pixels_side * 4];
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = (index % pixels_side / scale, index / pixels_side / scale);
        let dark = (QUIET..QUIET + width).contains(&x)
            && (QUIET..QUIET + width).contains(&y)
            && modules[(y - QUIET) * width + (x - QUIET)] == qrcode::Color::Dark;
        if dark {
            pixel[..3].fill(0);
        }
    }
    let handle =
        iced::advanced::image::Handle::from_rgba(pixels_side as u32, pixels_side as u32, pixels);
    Some((handle, pixels_side as f32))
}
