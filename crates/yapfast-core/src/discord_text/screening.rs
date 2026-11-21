//! A server's form for new members: reading it after joining, and handing
//! it in with the answers. See "Joining a server" in docs/discord/servers.md.

use yapfast_discord::screening::{self as wire, Field, Kind, Response};

use super::{account_tools::ask, commands::NOT_DISCORD, *};
use crate::chat::{ScreeningField, ScreeningForm, ScreeningKind, ScreeningReview};

/// Carries out a command about a server's form, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    match command {
        DiscordCommand::LoadScreening { server, code } => {
            let Some(guild) = discord::parse_server(&server).map(str::to_owned) else {
                let result = Err(NOT_DISCORD.into());
                let _ = events.send(Event::ScreeningLoaded { server, result });
                return None;
            };
            ask(
                discord,
                events,
                "load the server's form",
                move |discord| async move { Ok(from_wire(discord.member_form(&guild, &code).await?)) },
                move |result| Event::ScreeningLoaded { server, result },
            );
        }
        DiscordCommand::SubmitScreening { server, form } => {
            let Some(guild) = discord::parse_server(&server).map(str::to_owned) else {
                let result = Err(NOT_DISCORD.into());
                let _ = events.send(Event::ScreeningSubmitted { server, result });
                return None;
            };
            let answered: Vec<(Field, Response)> = form.fields.iter().map(to_wire).collect();
            let version = form.version;
            ask(
                discord,
                events,
                "hand in the form",
                move |discord| async move {
                    discord
                        .submit_member_form(&guild, &version, &answered)
                        .await
                },
                move |result| Event::ScreeningSubmitted {
                    server,
                    result: result.map(|review| match review {
                        wire::Review::Approved => ScreeningReview::Approved,
                        wire::Review::Waiting => ScreeningReview::Waiting,
                        wire::Review::Rejected => ScreeningReview::Rejected,
                    }),
                },
            );
        }
        other => return Some(Command::Discord(other)),
    }
    None
}

fn from_wire(form: wire::Form) -> ScreeningForm {
    ScreeningForm {
        version: form.version,
        description: form.description,
        fields: form
            .fields
            .into_iter()
            .map(|field| ScreeningField {
                kind: match field.kind {
                    Kind::Terms => ScreeningKind::Terms,
                    Kind::Text => ScreeningKind::Text,
                    Kind::Paragraph => ScreeningKind::Paragraph,
                    Kind::Choice => ScreeningKind::Choice,
                },
                label: field.label,
                description: field.description,
                required: field.required,
                values: field.values,
                agreed: false,
                text: String::new(),
                choice: None,
            })
            .collect(),
    }
}

fn to_wire(field: &ScreeningField) -> (Field, Response) {
    let kind = match field.kind {
        ScreeningKind::Terms => Kind::Terms,
        ScreeningKind::Text => Kind::Text,
        ScreeningKind::Paragraph => Kind::Paragraph,
        ScreeningKind::Choice => Kind::Choice,
    };
    let response = match field.kind {
        ScreeningKind::Terms => Response::Agreed(field.agreed),
        ScreeningKind::Choice => Response::Choice(field.choice),
        ScreeningKind::Text | ScreeningKind::Paragraph => Response::Text(field.text.clone()),
    };
    (
        Field {
            kind,
            label: field.label.clone(),
            description: field.description.clone(),
            required: field.required,
            values: field.values.clone(),
        },
        response,
    )
}
