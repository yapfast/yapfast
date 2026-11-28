//! The dialogs a clicked Discord link opens beside the invite's: a server's
//! scheduled event (`EventDialog`, with Interested), a gift (`GiftDialog`,
//! with Redeem Gift or Open in Browser) and an application's authorization
//! (`OAuthDialog`: what it asks for, the server it's added to, Authorize).
//! What each says is the core's (`AppState::event_prompt`, `gift_prompt`,
//! `oauth_prompt`); the dialogs keep only whether the server list is open
//! and the address last copied ([`LinkDialogs`]). Escape or a click outside
//! closes each.

use iced::{
    Element, Length, Task,
    keyboard::key,
    widget::{container, row, text},
};
use yapfast_core::application::{Action, AppState};

use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::buttons,
    fonts::BOLD,
    form,
    style::rounded,
};
use yapfast_core::i18n::tr;

/// Each dialog's width.
const WIDTH: f32 = 440.0;
/// The boxes that hold a server's name or an address.
const BOX: f32 = 44.0;

/// What's chosen in the open dialog.
#[derive(Default)]
pub struct LinkDialogs {
    /// The authorization's list of servers is open.
    pub(crate) guild_menu: bool,
    /// The address last copied from it.
    copied: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Event {
    CloseScheduled,
    ToggleInterest,
    CloseGift,
    /// Gift's main button: redeem it, or open it in the browser.
    Redeem,
    CloseOAuth,
    Authorize,
    GuildMenu,
    /// The server to add the application to, by its place in the list.
    Guild(usize),
    Copy(String),
}

/// What a dialog's foot says and offers.
struct Foot<'a> {
    status: &'a str,
    error: String,
    /// The main button's words; empty for none.
    confirm: &'a str,
    busy: bool,
}

fn message(event: Event) -> Message {
    Message::LinkDialog(event)
}

/// Whether one of these dialogs is open, which takes the keys.
pub fn is_open(state: &AppState) -> bool {
    state.scheduled_event.is_some() || state.gift.is_some() || state.oauth.is_some()
}

/// The pictures the dialogs show: the application's.
pub fn pictures(state: &AppState) -> Vec<String> {
    state
        .oauth
        .as_ref()
        .and_then(|dialog| dialog.preview.as_ref())
        .and_then(|preview| preview.icon.clone())
        .into_iter()
        .collect()
}

impl LinkDialogs {
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let state = cx.state();
        let action = match event {
            Event::CloseScheduled => Action::CloseEvent,
            Event::ToggleInterest => match state.event_prompt(false) {
                Some(prompt) if !prompt.busy && !prompt.confirm.is_empty() => {
                    Action::ToggleEventInterest
                }
                _ => return Task::none(),
            },
            Event::CloseGift => Action::CloseGift,
            Event::Redeem => match state.gift_prompt() {
                Some(prompt) if prompt.busy || prompt.confirm.is_empty() => {
                    return Task::none();
                }
                Some(prompt) if prompt.browser => Action::OpenGiftInBrowser,
                Some(_) => Action::RedeemGift,
                None => return Task::none(),
            },
            Event::CloseOAuth => Action::CloseOAuth,
            Event::Authorize => match state.oauth_prompt() {
                Some(prompt) if !prompt.busy && !prompt.confirm.is_empty() => {
                    Action::AuthorizeOAuth
                }
                _ => return Task::none(),
            },
            Event::GuildMenu => {
                self.guild_menu = !self.guild_menu;
                return Task::none();
            }
            Event::Guild(index) => {
                self.guild_menu = false;
                let Some((id, _)) = state
                    .oauth_prompt()
                    .and_then(|prompt| prompt.guilds.into_iter().nth(index))
                else {
                    return Task::none();
                };
                Action::ChooseOAuthGuild(id)
            }
            Event::Copy(address) => {
                self.copied = Some(address.clone());
                return iced::clipboard::write(address);
            }
        };
        cx.dispatch(action);
        if !is_open(cx.state()) {
            *self = LinkDialogs::default();
            cx.request(Request::Focus(Focus::Default));
        }
        Task::none()
    }

    /// Escape closes the open one.
    pub fn key(&mut self, named: Option<key::Named>, cx: &mut Cx<'_>) -> Task<Message> {
        if named != Some(key::Named::Escape) {
            return Task::none();
        }
        let state = cx.state();
        let close = if state.oauth.is_some() {
            Event::CloseOAuth
        } else if state.gift.is_some() {
            Event::CloseGift
        } else {
            Event::CloseScheduled
        };
        self.update(close, cx)
    }
}

/// The open one of these dialogs, if any.
pub fn view<'a>(dialogs: &'a LinkDialogs, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: dialogs, cx }.view()
}

/// What's chosen in the dialogs and what they read, as their parts draw
/// them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a LinkDialogs,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        if state.oauth.is_some() {
            return self.oauth_view();
        }
        if state.gift.is_some() {
            return self.gift_view();
        }
        self.event_view()
    }

    /// The status, the error and the buttons every one of them ends with:
    /// Close, or Cancel beside the main button while there is one.
    fn link_dialog_foot(
        self,
        body: &mut Vec<Element<'a, Message>>,
        foot: Foot<'a>,
        close: Event,
        main: Event,
    ) {
        let theme = self.cx.look.theme;
        let Foot {
            status,
            error,
            confirm,
            busy,
        } = foot;
        if !status.is_empty() {
            body.push(form::note(theme, status));
        }
        if !error.is_empty() {
            body.push(form::note_in(error, theme.danger));
        }
        let mut row_ = vec![
            action(
                theme,
                if confirm.is_empty() {
                    tr("Close")
                } else {
                    tr("Cancel")
                },
                Kind::Ghost,
                Some(message(close)),
            )
            .into(),
        ];
        if !confirm.is_empty() {
            row_.push(
                action(
                    theme,
                    confirm,
                    Kind::Primary,
                    (!busy).then(|| message(main)),
                )
                .into(),
            );
        }
        body.push(buttons(row_));
    }

    /// A server's scheduled event: its name, when and where, what it says,
    /// and Interested or Remove interest.
    fn event_view(self) -> Option<Element<'a, Message>> {
        let prompt = self
            .cx
            .state
            .event_prompt(self.cx.layout.prefs.twelve_hour_clock)?;
        let theme = self.cx.look.theme;
        let mut body = vec![
            self.cx
                .look
                .dialog_head(prompt.title.to_owned(), message(Event::CloseScheduled)),
        ];
        if prompt.found {
            body.push(
                self.cx
                    .look
                    .found_card(None, prompt.name, [prompt.when, prompt.detail]),
            );
        }
        if !prompt.description.is_empty() {
            body.push(
                text(prompt.description)
                    .size(14)
                    .color(theme.text)
                    .width(Length::Fill)
                    .into(),
            );
        }
        self.link_dialog_foot(
            &mut body,
            Foot {
                status: prompt.status,
                error: prompt.error,
                confirm: prompt.confirm,
                busy: prompt.busy,
            },
            Event::CloseScheduled,
            Event::ToggleInterest,
        );
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDTH,
            message(Event::CloseScheduled),
        ))
    }

    /// A gift: what it grants and from whom, and Redeem Gift, or Open in
    /// Browser for one only Discord redeems.
    fn gift_view(self) -> Option<Element<'a, Message>> {
        let prompt = self.cx.state.gift_prompt()?;
        let mut body = vec![
            self.cx
                .look
                .dialog_head(prompt.title.to_owned(), message(Event::CloseGift)),
        ];
        if prompt.found {
            body.push(self.cx.look.found_card(None, prompt.name, [prompt.detail]));
        }
        self.link_dialog_foot(
            &mut body,
            Foot {
                status: prompt.status,
                error: prompt.error,
                confirm: prompt.confirm,
                busy: prompt.busy,
            },
            Event::CloseGift,
            Event::Redeem,
        );
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDTH,
            message(Event::CloseGift),
        ))
    }

    /// An application's authorization: the application, what of it can
    /// take over the account or a server in red, what it asks for, the
    /// server it's added to, the address to copy, and Authorize.
    fn oauth_view(self) -> Option<Element<'a, Message>> {
        let prompt = self.cx.state.oauth_prompt()?;
        let theme = self.cx.look.theme;
        let mut body = vec![
            self.cx
                .look
                .dialog_head(prompt.title.clone(), message(Event::CloseOAuth)),
        ];
        if prompt.found {
            let avatar = self
                .cx
                .avatar(prompt.icon.as_deref(), &prompt.name, 48.0, 24.0, 1.0);
            body.push(
                self.cx
                    .look
                    .found_card(Some(avatar), prompt.name.clone(), [prompt.detail]),
            );
        }
        if !prompt.warning.is_empty() {
            body.push(
                text(prompt.warning)
                    .size(14)
                    .font(BOLD)
                    .color(theme.danger)
                    .width(Length::Fill)
                    .into(),
            );
        }
        if !prompt.wants.is_empty() {
            body.push(
                text(prompt.wants)
                    .size(14)
                    .color(theme.text)
                    .width(Length::Fill)
                    .into(),
            );
        }
        if prompt.guild_picker || !prompt.guild_name.is_empty() {
            body.push(form::heading(theme, tr("ADD TO SERVER")));
        }
        let sunken = theme.sunken;
        let boxed = |content: Element<'static, Message>, right: f32| {
            container(content)
                .padding(iced::Padding {
                    left: 12.0,
                    right,
                    ..iced::Padding::ZERO
                })
                .height(BOX)
                .width(Length::Fill)
                .align_y(iced::Alignment::Center)
                .style(move |_| rounded(sunken, 8))
        };
        if prompt.guild_picker {
            let names = prompt.guilds.iter().map(|(_, name)| name.clone()).collect();
            let mut menu = form::dropdown(
                names,
                Some(prompt.guild_chosen),
                self.s.guild_menu,
                message(Event::GuildMenu),
                |index| message(Event::Guild(index)),
            );
            if prompt.busy {
                menu = menu.enabled(false);
            }
            body.push(menu.view(self.cx.look));
        } else if !prompt.guild_name.is_empty() {
            body.push(
                boxed(
                    text(prompt.guild_name)
                        .size(15)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None)
                        .into(),
                    12.0,
                )
                .clip(true)
                .into(),
            );
        }
        if !prompt.status.is_empty() {
            body.push(form::note(theme, prompt.status));
        }
        if !prompt.address.is_empty() {
            let copied = self.s.copied.as_deref() == Some(prompt.address.as_str());
            let copy = action(
                theme,
                if copied { tr("Copied") } else { tr("Copy") },
                if copied { Kind::Plain } else { Kind::Primary },
                Some(message(Event::Copy(prompt.address.clone()))),
            );
            body.push(
                boxed(
                    row![
                        container(
                            text(prompt.address)
                                .size(15)
                                .color(theme.text)
                                .wrapping(text::Wrapping::None)
                        )
                        .width(Length::Fill)
                        .clip(true),
                        copy
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center)
                    .into(),
                    4.0,
                )
                .into(),
            );
        }
        self.link_dialog_foot(
            &mut body,
            Foot {
                status: "",
                error: prompt.error,
                confirm: prompt.confirm,
                busy: prompt.busy,
            },
            Event::CloseOAuth,
            Event::Authorize,
        );
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDTH,
            message(Event::CloseOAuth),
        ))
    }
}
