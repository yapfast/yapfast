//! Delete Server and Transfer Ownership: Discord's question, the server's
//! name typed without two-factor authentication; who gets the server, the
//! acknowledgement, then the code Discord emailed. What each asks and when
//! it may go is the core's ([`Ownership`]).

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, rich_text, row, span, text},
};
use yapfast_core::application::{AppState, DeleteAsk, Ownership, OwnershipInput, TransferAsk};

use super::View;
use super::{expressions::answers, message, moderation::face};
use crate::{
    app::Message,
    controls::{Kind, ink_style},
    dialog,
    fonts::BOLD,
    form,
    pictures::Icon,
    style::{faded, rounded},
};
use yapfast_core::i18n::tr;

/// The dialogs' width, the two owners' avatars, and the arrow between.
const WIDTH: f32 = 440.0;
const AVATAR: f32 = 80.0;
const ARROW: u32 = 24;
/// The fields that take the keys: the server's name, and the emailed code.
pub(super) const SERVER_NAME: &str = "delete-server-name";
pub(super) const CODE: &str = "transfer-code";

fn answer(input: OwnershipInput) -> Message {
    message(super::Event::Ownership(input))
}

/// Both owners' avatars, while Transfer Ownership shows them.
pub fn pictures(state: &AppState, ownership: &Ownership) -> Vec<String> {
    match ownership {
        Ownership::Transfer(ask) if ask.code.is_none() => state
            .account_avatar
            .iter()
            .chain([&ask.to.avatar])
            .filter(|url| !url.is_empty())
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

/// The field that takes the keys while a dialog shows it.
pub(super) fn dialog_field(ownership: &Ownership) -> Option<&'static str> {
    match ownership {
        Ownership::Delete(ask) if ask.by_name => Some(SERVER_NAME),
        Ownership::Transfer(ask) if ask.code.is_some() => Some(CODE),
        _ => None,
    }
}

/// Discord's words with `names` in bold, between `parts`.
fn sentence<'a>(
    parts: [&'a str; 3],
    names: [&'a str; 2],
    size: u32,
    ink: iced::Color,
) -> Element<'a, Message> {
    let [before, between, after] = parts;
    let mut spans: Vec<iced::advanced::text::Span<'a, (), iced::Font>> =
        vec![span(before).color(ink)];
    if !names[0].is_empty() {
        spans.push(span(names[0]).font(BOLD).color(ink));
    }
    spans.push(span(between).color(ink));
    if !names[1].is_empty() {
        spans.push(span(names[1]).font(BOLD).color(ink));
    }
    spans.push(span(after).color(ink));
    rich_text(spans).size(size).into()
}

impl<'a> View<'a> {
    /// Delete Server's or Transfer Ownership's dialog, over the settings.
    pub(super) fn ownership_dialog(
        self,
        server: &'a str,
        ownership: &'a Ownership,
    ) -> Element<'a, Message> {
        let (title, mut body, sending) = match ownership {
            Ownership::Delete(ask) => (
                text(format!("Delete '{}'", ask.name)),
                self.delete_body(ask),
                ask.sending,
            ),
            Ownership::Transfer(ask) => (
                text(tr("Transfer Ownership")),
                self.transfer_body(server, ask),
                ask.sending,
            ),
        };
        let theme = self.cx.look.theme;
        body.insert(0, title.size(20).font(BOLD).color(theme.text).into());
        let outside = if sending {
            Message::Nothing
        } else {
            answer(OwnershipInput::Cancel)
        };
        dialog::panel(
            theme,
            column(body).spacing(14).padding(16).into(),
            WIDTH,
            None,
            outside,
        )
    }

    /// Delete Server: Discord's question, the server's name to type
    /// without two-factor authentication, and Delete Server.
    fn delete_body(self, ask: &'a DeleteAsk) -> Vec<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let mut body = vec![sentence(
            [
                "Are you sure you want to delete ",
                "",
                "? This action cannot be undone.",
            ],
            [&ask.name, ""],
            15,
            theme.secondary,
        )];
        if ask.by_name {
            let mut field = form::entry(theme, SERVER_NAME, "", &ask.typed);
            if !ask.sending {
                field = field
                    .on_input(|typed| answer(OwnershipInput::Type(typed)))
                    .on_submit(answer(OwnershipInput::Confirm));
            }
            let mut name = column![form::heading(theme, tr("ENTER SERVER NAME")), field].spacing(6);
            if ask.wrong {
                name = name.push(
                    text(tr("You didn't enter the server name correctly"))
                        .size(12)
                        .color(theme.danger),
                );
            }
            body.push(name.into());
        }
        if let Some(error) = &ask.error {
            body.push(form::error(theme, error.as_str()).into());
        }
        body.push(answers(
            theme,
            (!ask.sending).then(|| answer(OwnershipInput::Cancel)),
            if ask.sending {
                tr("Deleting...")
            } else {
                tr("Delete Server")
            },
            Kind::Danger,
            (!ask.sending).then(|| answer(OwnershipInput::Confirm)),
        ));
        body
    }

    /// Transfer Ownership: who gets the server, from whom, the warning a
    /// Verified or Partnered server gets, and the acknowledgement; or, once
    /// Discord emailed it, the code.
    fn transfer_body(self, server: &str, ask: &'a TransferAsk) -> Vec<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        // Discord's tag: the username.
        let tag = ask.to.username.as_deref().unwrap_or(&ask.to.name);
        let mut body = Vec::new();
        if let Some(code) = &ask.code {
            body.push(
                text(tr("Check your email: we sent you a verification code. Enter it here to verify you’re really you."))
                    .size(15)
                    .color(theme.secondary)
                    .into(),
            );
            let mut field = form::entry(theme, CODE, "", code);
            if !ask.sending {
                field = field
                    .on_input(|typed| answer(OwnershipInput::Type(typed)))
                    .on_submit(answer(OwnershipInput::Confirm));
            }
            let mut part = column![form::heading(theme, tr("VERIFICATION CODE")), field].spacing(6);
            if let Some(error) = &ask.error {
                part = part.push(text(error.as_str()).size(12).color(theme.danger));
            }
            let resend: Element<'a, Message> = if ask.resent {
                text(tr("Verification code sent."))
                    .size(13)
                    .color(theme.live)
                    .into()
            } else {
                button(text(tr("Didn’t receive a code or it expired? Resend it.")).size(13))
                    .padding(0)
                    .on_press_maybe(
                        (!ask.sending && !ask.resending).then(|| answer(OwnershipInput::Resend)),
                    )
                    .style(ink_style(theme.link, theme.accent_hover))
                    .into()
            };
            body.push(part.push(resend).into());
        } else {
            let name = state
                .servers
                .iter()
                .find(|known| known.id == server)
                .map(|known| known.name.as_str())
                .unwrap_or_default();
            body.push(sentence(
                [
                    "This will transfer ownership of ",
                    " to ",
                    ". This cannot be undone!",
                ],
                [name, tag],
                15,
                theme.secondary,
            ));
            let me = state.account_name.as_deref().unwrap_or_default();
            let mine = state.account_avatar.as_deref().unwrap_or_default();
            body.push(
                container(
                    row![
                        face(self.cx, mine, me, AVATAR),
                        self.cx.look.glyph(Icon::ChevronRight, ARROW, theme.muted),
                        face(self.cx, &ask.to.avatar, &ask.to.name, AVATAR),
                    ]
                    .spacing(24)
                    .align_y(Alignment::Center),
                )
                .center_x(Length::Fill)
                .into(),
            );
            if ask.verified {
                let ground = faded(theme.danger, 0.12);
                body.push(
                    container(
                        text(tr("Partnered or Verified servers must reach out to the Community team for ownership transfer requests. However, our Partner and Verified Server Programs are currently paused and not able to handle any requests."))
                            .size(13)
                            .color(theme.text),
                    )
                    .padding(10)
                    .width(Length::Fill)
                    .style(move |_| rounded(ground, 6))
                    .into(),
                );
            }
            // A Verified server's acknowledgement shows at half strength,
            // and takes no click.
            let ink = if ask.verified {
                faded(theme.secondary, 0.5)
            } else {
                theme.secondary
            };
            let acknowledgement = row![
                container(self.cx.look.checkbox(ask.agreed, false)).padding(iced::Padding {
                    top: 2.0,
                    ..iced::Padding::ZERO
                }),
                container(sentence(
                    [
                        "I acknowledge that by transferring ownership of this server to ",
                        "",
                        ", it officially belongs to them.",
                    ],
                    [tag, ""],
                    14,
                    ink,
                ))
                .width(Length::Fill),
            ]
            .spacing(10);
            body.push(
                button(acknowledgement)
                    .padding(0)
                    .on_press_maybe(
                        (!ask.verified && !ask.sending).then(|| answer(OwnershipInput::Agree)),
                    )
                    .style(|_, _| button::Style::default())
                    .into(),
            );
            if let Some(error) = &ask.error {
                body.push(form::error(theme, error.as_str()).into());
            }
        }
        body.push(answers(
            theme,
            (!ask.sending).then(|| answer(OwnershipInput::Cancel)),
            if ask.sending {
                tr("Transferring...")
            } else {
                tr("Transfer Ownership")
            },
            Kind::Danger,
            ask.ready().then(|| answer(OwnershipInput::Confirm)),
        ));
        body
    }
}
