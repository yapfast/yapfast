//! Dialogs over the dimmed window, drawn in the kit's frame
//! ([`crate::dialog`]): Delete's confirmation, the question before
//! following a masked link or a plugin's link, and the question a plugin's
//! request for a site nobody allowed waits on.

use iced::{
    Element, Length, Task,
    keyboard::key,
    widget::{Space, column, container, row, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    ctx::Ctx,
    dialog::{CONFIRM_WIDTH, dialog},
    fonts::{BOLD, MEDIUM},
};

/// The dialogs' answers.
#[derive(Clone, Debug)]
pub enum Event {
    /// The delete confirmation's answers.
    ConfirmDelete,
    CancelDelete,
    /// The question before a masked link: visit it, or go back.
    Leave(bool),
    /// The answer to a plugin's request to reach a site: allowed or not,
    /// and whether it is kept (`grants.json`) or for this run only.
    #[cfg(feature = "plugins")]
    Consent {
        allow: bool,
        remember: bool,
    },
    /// The answer to a powerful capability's first use: allowed or not.
    #[cfg(feature = "plugins")]
    Powerful(bool),
    /// The folder the person picked for `fs.folder`, or none when they closed
    /// the picker, which leaves the question up.
    #[cfg(feature = "plugins")]
    PowerfulFolder(Option<std::path::PathBuf>),
}

/// What each answer does.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::ConfirmDelete => {
            app.client.dispatch(Action::ConfirmDelete);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::CancelDelete => {
            app.client.dispatch(Action::CancelDelete);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::Leave(visit) => leave(app, visit),
        #[cfg(feature = "plugins")]
        Event::Consent { allow, remember } => answer_consent(app, allow, remember),
        #[cfg(feature = "plugins")]
        Event::Powerful(allow) => answer_powerful(app, allow),
        #[cfg(feature = "plugins")]
        Event::PowerfulFolder(folder) => match folder {
            Some(folder) => dispatch_powerful(app, true, Some(folder)),
            None => Task::none(),
        },
    }
}

/// Answers a powerful capability's first use. Allowing `fs.folder` opens the
/// system's folder picker first: the plugin is given the folder the person
/// picks, and nothing until they pick one.
#[cfg(feature = "plugins")]
pub fn answer_powerful(app: &mut App, allow: bool) -> Task<Message> {
    let Some(ask) = app.client.state().plugin_powerful.clone() else {
        return Task::none();
    };
    if allow && ask.capability == yapfast_plugin_api::capability::Capability::FsFolder {
        let title = format!("Choose the folder {} may read", ask.name);
        return Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .set_title(title)
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_owned())
            },
            |folder| Message::Dialog(Event::PowerfulFolder(folder)),
        );
    }
    dispatch_powerful(app, allow, None)
}

#[cfg(feature = "plugins")]
fn dispatch_powerful(
    app: &mut App,
    allow: bool,
    folder: Option<std::path::PathBuf>,
) -> Task<Message> {
    let Some(ask) = app.client.state().plugin_powerful.clone() else {
        return Task::none();
    };
    app.client.dispatch(Action::Plugin(
        yapfast_core::application::plugins::Action::AnswerPowerful {
            plugin: ask.plugin,
            capability: ask.capability,
            allow,
            folder,
        },
    ));
    app.sync()
}

/// The red question a powerful capability's first use waits on: which plugin,
/// what it would be able to do in plain words, and Don't allow or Allow (for a
/// folder, Choose folder). Escape refuses.
#[cfg(feature = "plugins")]
pub fn powerful_view<'a>(cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let ask = cx.state.plugin_powerful.as_ref()?;
    let theme = cx.look.theme;
    let danger = theme.danger;
    let answer = |allow| Some(Message::Dialog(Event::Powerful(allow)));
    let allow_label = if ask.capability == yapfast_plugin_api::capability::Capability::FsFolder {
        "Choose folder"
    } else {
        "Allow"
    };
    let body = column![
        text("Allow a powerful permission?")
            .size(18)
            .font(BOLD)
            .color(danger),
        text(format!(
            "{} ({}) wants to use {} for the first time.",
            ask.name, ask.plugin, ask.capability
        ))
        .size(15)
        .color(theme.secondary),
        container(
            text(ask.sentence.clone())
                .size(15)
                .font(BOLD)
                .color(theme.text)
        )
        .padding([10, 12])
        .width(Length::Fill)
        .style(move |_| {
            iced::widget::container::Style::default()
                .background(iced::Color { a: 0.14, ..danger })
                .border(iced::Border {
                    color: danger,
                    width: 1.0,
                    radius: 6.0.into(),
                })
        }),
        text(
            "Only allow this for a plugin whose code you trust. You can take it back with \
             one click on the plugin's page in Settings, Plugins.",
        )
        .size(13)
        .color(theme.muted),
        row![
            Space::new().width(Length::Fill),
            action(theme, "Don't allow", Kind::Ghost, answer(false)),
            action(theme, allow_label, Kind::Danger, answer(true)),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20);
    Some(dialog(theme, body.into(), CONFIRM_WIDTH, answer(false)?))
}

/// Answers the plugin the consent question is about, which takes the
/// question away.
#[cfg(feature = "plugins")]
pub fn answer_consent(app: &mut App, allow: bool, remember: bool) -> Task<Message> {
    let Some(ask) = app.client.state().plugin_consent.clone() else {
        return Task::none();
    };
    app.client.dispatch(Action::Plugin(
        yapfast_core::application::plugins::Action::AnswerConsent {
            plugin: ask.plugin,
            host: ask.host,
            allow,
            remember,
        },
    ));
    app.sync()
}

/// The question a plugin's request waits on when it asks for a site it didn't
/// declare and nobody has allowed: who asks (its name and id), the site, and
/// Deny, Allow once and Always allow. It is drawn in Yapfast's own look,
/// wherever the person is, and Escape denies for this time.
#[cfg(feature = "plugins")]
pub fn consent_view<'a>(cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let ask = cx.state.plugin_consent.as_ref()?;
    let theme = cx.look.theme;
    let sunken = theme.sunken;
    let answer = |allow, remember| Some(Message::Dialog(Event::Consent { allow, remember }));
    let body = column![
        text("Allow a plugin to reach a site?")
            .size(18)
            .font(BOLD)
            .color(theme.text),
        text(format!(
            "{} ({}) wants to connect to a site it didn't list when you installed it:",
            ask.name, ask.plugin
        ))
        .size(15)
        .color(theme.secondary),
        container(
            text(ask.host.clone())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::Glyph),
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_| crate::style::rounded(sunken, 6)),
        text(
            "Anything the plugin can read could be sent there. You can take this \
             back on the plugin's page in Settings, Plugins.",
        )
        .size(13)
        .color(theme.muted),
        row![
            Space::new().width(Length::Fill),
            action(theme, "Deny", Kind::Ghost, answer(false, false)),
            action(theme, "Allow once", Kind::Plain, answer(true, false)),
            action(theme, "Always allow", Kind::Primary, answer(true, true)),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20);
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        answer(false, false)?,
    ))
}

/// The confirmation's keys: Escape cancels, Enter does what the focused
/// button does, Tab and the side arrows go round the two buttons.
pub fn confirm_key(app: &mut App, named: Option<key::Named>) -> Task<Message> {
    match named {
        Some(key::Named::Escape) => app.update(Message::Dialog(Event::CancelDelete)),
        Some(key::Named::Enter) => {
            if app.confirm_focus {
                app.update(Message::Dialog(Event::ConfirmDelete))
            } else {
                app.update(Message::Dialog(Event::CancelDelete))
            }
        }
        Some(key::Named::Tab | key::Named::ArrowLeft | key::Named::ArrowRight) => {
            app.confirm_focus = !app.confirm_focus;
            Task::none()
        }
        _ => Task::none(),
    }
}

/// Delete's confirmation over the dimmed window: the question, the
/// message as it shows, Cancel and Delete (focused when `focus`, as it is
/// first). A click on the dimmed window cancels; nothing under it takes a
/// key.
pub fn delete_view(focus: bool, cx: Ctx<'_>) -> Option<Element<'_, Message>> {
    let state = cx.state;
    let item = state.confirm_delete.as_ref()?;
    let message = state.message(item);
    let theme = cx.look.theme;
    let line = theme.line;
    let mut body = column![
        text("Delete Message").size(20).font(BOLD).color(theme.text),
        text("Are you sure you want to delete this message?")
            .size(15)
            .color(theme.secondary),
    ]
    .spacing(12)
    .padding(16);
    if let Some(message) = message {
        body = body.push(
            container(
                column![
                    text(message.author.clone()).font(MEDIUM).color(theme.text),
                    text(message.body.clone())
                        .color(theme.text)
                        .wrapping(text::Wrapping::WordOrGlyph),
                ]
                .spacing(2),
            )
            .padding(12)
            .width(Length::Fill)
            .max_height(160)
            .clip(true)
            .style(move |_| container::Style {
                border: crate::style::edge(line, 6.0),
                ..Default::default()
            }),
        );
    }
    body = body.push(
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(Message::Dialog(Event::CancelDelete))
            )
            .ring(!focus),
            action(
                theme,
                "Delete",
                Kind::Danger,
                Some(Message::Dialog(Event::ConfirmDelete))
            )
            .ring(focus),
        ]
        .spacing(8),
    );
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        Message::Dialog(Event::CancelDelete),
    ))
}

/// The question before following a masked link (one whose words aren't
/// where it goes), or a link a plugin asked to open (`from`, the plugin's
/// name): where it goes, Go back and Visit site. Escape goes back, Enter
/// visits.
pub fn leaving_view<'a>(
    link: Option<&'a str>,
    from: Option<&'a str>,
    cx: Ctx<'a>,
) -> Option<Element<'a, Message>> {
    let link = link?;
    let theme = cx.look.theme;
    let sunken = theme.sunken;
    let lead = match from {
        Some(plugin) => format!("The plugin {plugin} wants to open a link that goes to"),
        None => "This link goes to".to_owned(),
    };
    let body = column![
        text("Leaving Yapfast")
            .size(18)
            .font(BOLD)
            .color(theme.text),
        text(lead).size(15).color(theme.secondary),
        text(yapfast_core::links::host(link))
            .size(17)
            .font(BOLD)
            .color(theme.text),
        container(
            text(link)
                .size(14)
                .color(theme.secondary)
                .wrapping(text::Wrapping::Glyph),
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_| crate::style::rounded(sunken, 6)),
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Go back",
                Kind::Ghost,
                Some(Message::Dialog(Event::Leave(false)))
            ),
            action(
                theme,
                "Visit site",
                Kind::Primary,
                Some(Message::Dialog(Event::Leave(true)))
            ),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20);
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        Message::Dialog(Event::Leave(false)),
    ))
}

/// The question's answer: the link opens, or not.
pub fn leave(app: &mut App, visit: bool) -> Task<Message> {
    app.leaving_from = None;
    if let Some(link) = app.leaving.take()
        && visit
    {
        crate::browser::open(&link);
    }
    Task::none()
}

/// An emoji to pick in a form: 40 px on a sunken ground, the emoji chosen
/// (`key`) or a smile with a plus, and with one, a small badge on its
/// corner that takes it off.
pub fn emoji_slot<'a>(
    ctx: crate::ctx::Ctx<'a>,
    key: Option<&'a str>,
    pick: Message,
    remove: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let face: Element<'a, Message> = match key {
        Some(key) => crate::suggest::emoji_face(ctx, key, 22),
        None => ctx
            .look
            .glyph(crate::pictures::Icon::SmilePlus, 20, theme.secondary)
            .into(),
    };
    let pick = iced::widget::button(container(face).center(40))
        .padding(0)
        .width(40)
        .height(40)
        .on_press(pick)
        .style(crate::controls::lit_style(
            Some(theme.sunken),
            theme.hover,
            6.0,
        ));
    if key.is_none() {
        return pick.into();
    }
    let (faint, danger, raised) = (theme.faint, theme.danger, theme.raised);
    let remove = iced::widget::button(
        container(ctx.look.glyph(crate::pictures::Icon::X, 10, theme.text)).center(14),
    )
    .padding(0)
    .width(18)
    .height(18)
    .on_press(remove)
    .style(move |_, status| iced::widget::button::Style {
        background: Some(
            if crate::controls::lights(status) {
                danger
            } else {
                faint
            }
            .into(),
        ),
        border: iced::Border {
            color: raised,
            width: 2.0,
            radius: 9.0.into(),
        },
        ..Default::default()
    });
    iced::widget::stack![pick, crate::controls::placed(remove, 28.0, 0.0)].into()
}
