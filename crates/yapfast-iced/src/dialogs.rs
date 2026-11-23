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
    /// A second passed while the site question shows: past
    /// [`CONSENT_WAIT`] unanswered, it is denied for this time.
    #[cfg(feature = "plugins")]
    ConsentTick,
    /// The answer to a powerful capability's first use: allowed or not.
    #[cfg(feature = "plugins")]
    Powerful(bool),
    /// The folder the person picked for `fs.folder`, or none when they closed
    /// the picker, which leaves the question up.
    #[cfg(feature = "plugins")]
    PowerfulFolder(Option<std::path::PathBuf>),
    /// One of the apps plugins offered the leaving link in, by its place.
    #[cfg(feature = "plugins")]
    OpenInApp(usize),
    /// A plugin's sign-in dialog: the pasted text changed, Done, or Cancel.
    #[cfg(feature = "plugins")]
    SignInText(String),
    #[cfg(feature = "plugins")]
    SignInDone,
    #[cfg(feature = "plugins")]
    SignInClose,
}

/// The sign-in dialog's field.
#[cfg(feature = "plugins")]
pub const SIGN_IN_FIELD: &str = "plugin-sign-in-paste";

/// How long the site question waits for an answer before it denies: a
/// plugin's request mustn't wait on a person who walked away, and a yes is
/// never given for them.
#[cfg(feature = "plugins")]
pub const CONSENT_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

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
        Event::OpenInApp(index) => open_in_app(app, index),
        #[cfg(feature = "plugins")]
        Event::Consent { allow, remember } => answer_consent(app, allow, remember),
        #[cfg(feature = "plugins")]
        Event::ConsentTick => {
            let due = app
                .consent_since
                .as_ref()
                .is_some_and(|(_, since)| since.elapsed() >= CONSENT_WAIT);
            if due {
                answer_consent(app, false, false)
            } else {
                Task::none()
            }
        }
        #[cfg(feature = "plugins")]
        Event::Powerful(allow) => answer_powerful(app, allow),
        #[cfg(feature = "plugins")]
        Event::SignInText(text) => {
            app.sign_in_paste = text;
            Task::none()
        }
        #[cfg(feature = "plugins")]
        Event::SignInDone => {
            let Some(ask) = app.client.state().plugin_sign_in.clone() else {
                return Task::none();
            };
            let text = std::mem::take(&mut app.sign_in_paste);
            app.client.dispatch(Action::Plugin(
                yapfast_core::application::plugins::Action::SignInPasted {
                    plugin: ask.plugin,
                    text,
                },
            ));
            app.sync()
        }
        #[cfg(feature = "plugins")]
        Event::SignInClose => close_sign_in(app),
        #[cfg(feature = "plugins")]
        Event::PowerfulFolder(folder) => match folder {
            Some(folder) => dispatch_powerful(app, true, Some(folder)),
            None => Task::none(),
        },
    }
}

/// Starts the site question's clock when a question shows, and stops it when
/// none does; a question about another site starts it again.
#[cfg(feature = "plugins")]
pub fn follow_consent(app: &mut App) {
    let asked = app
        .client
        .state()
        .plugin_consent
        .as_ref()
        .map(|ask| format!("{}\u{1f}{}", ask.plugin, ask.host));
    match asked {
        None => app.consent_since = None,
        Some(key)
            if app
                .consent_since
                .as_ref()
                .is_none_or(|(kept, _)| *kept != key) =>
        {
            app.consent_since = Some((key, std::time::Instant::now()));
        }
        Some(_) => {}
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
    // `profile.write` is asked about the same way, and isn't the powerful
    // tier: the title says which this is.
    let title = if ask.capability.is_powerful() {
        "Allow a powerful permission?"
    } else {
        "Allow this permission?"
    };
    let body = column![
        text(title).size(18).font(BOLD).color(danger),
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

/// Closes a plugin's sign-in dialog, which ends the sign-in it waits on.
#[cfg(feature = "plugins")]
pub fn close_sign_in(app: &mut App) -> Task<Message> {
    let Some(ask) = app.client.state().plugin_sign_in.clone() else {
        return Task::none();
    };
    app.sign_in_paste.clear();
    app.client.dispatch(Action::Plugin(
        yapfast_core::application::plugins::Action::SignInClosed(ask.plugin),
    ));
    app.sync()
}

/// The dialog a sign-in through a service's own server waits in: the plugin
/// and the service named, and a field for what the service's page gave back,
/// for when it can't hand it to Yapfast by a link itself. Escape and Cancel
/// end the sign-in.
#[cfg(feature = "plugins")]
pub fn sign_in_view<'a>(pasted: &'a str, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let ask = cx.state.plugin_sign_in.as_ref()?;
    let theme = cx.look.theme;
    let done = (!pasted.trim().is_empty()).then_some(Message::Dialog(Event::SignInDone));
    let body = column![
        text(format!("Sign in to {}", ask.provider))
            .size(18)
            .font(BOLD)
            .color(theme.text),
        text(format!(
            "{} ({}) opened {}'s sign-in in your browser. When it's done, it may come back \
             here by itself; if the page shows a token or some text instead, copy all of it \
             and paste it here.",
            ask.name, ask.plugin, ask.provider
        ))
        .size(15)
        .color(theme.secondary),
        crate::controls::field(
            theme,
            SIGN_IN_FIELD,
            "Paste what the page shows",
            pasted,
            true,
            true
        )
        .on_input(|text| Message::Dialog(Event::SignInText(text)))
        .on_submit_maybe(done.clone()),
        text("Only paste what this service's own page gave you.")
            .size(13)
            .color(theme.muted),
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(Message::Dialog(Event::SignInClose))
            ),
            action(theme, "Done", Kind::Primary, done),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20);
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        Message::Dialog(Event::SignInClose),
    ))
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
    let since = cx.consent_since;
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
        // Said, and counted down, so the denial never comes as a surprise.
        crate::widgets::live_text(std::time::Duration::from_secs(1), move || {
            let left = since.map_or(CONSENT_WAIT, |since| {
                CONSENT_WAIT.saturating_sub(since.elapsed())
            });
            format!(
                "If you don't answer, it's denied in {} s, for this time only.",
                left.as_secs()
            )
        })
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
    #[cfg(feature = "plugins")]
    let offers = offers_view(link, cx);
    #[cfg(not(feature = "plugins"))]
    let offers: Option<Element<'a, Message>> = None;
    let mut body = column![
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
    ]
    .spacing(12)
    .padding(20);
    if let Some(offers) = offers {
        body = body.push(offers);
    }
    let body = body.push(
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
    );
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
    #[cfg(feature = "plugins")]
    app.client.dispatch(Action::Plugin(
        yapfast_core::application::plugins::Action::LinkClosed,
    ));
    Task::none()
}

/// What plugins offered to open the leaving link in: a button each, its
/// plugin's name under it, so the person sees whose app it is and still
/// chooses. None until one has answered.
#[cfg(feature = "plugins")]
fn offers_view<'a>(link: &str, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let asked = cx
        .state
        .plugin_link
        .as_ref()
        .filter(|asked| asked.url == link)?;
    if asked.offers.is_empty() {
        return None;
    }
    let theme = cx.look.theme;
    let mut offers = column![].spacing(6);
    for (index, offer) in asked.offers.iter().enumerate() {
        offers = offers.push(
            row![
                action(
                    theme,
                    offer.label.clone(),
                    Kind::Plain,
                    Some(Message::Dialog(Event::OpenInApp(index))),
                ),
                text(format!("Offered by the {} plugin", offer.name))
                    .size(12)
                    .color(theme.secondary),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        );
    }
    Some(offers.into())
}

/// Opens the leaving link in the app a plugin offered, which the person
/// chose in the dialog.
#[cfg(feature = "plugins")]
fn open_in_app(app: &mut App, index: usize) -> Task<Message> {
    let uri = app
        .client
        .state()
        .plugin_link
        .as_ref()
        .and_then(|asked| asked.offers.get(index))
        .map(|offer| offer.uri.clone());
    if let Some(uri) = uri {
        crate::browser::open(&uri);
        app.leaving = None;
        app.leaving_from = None;
        app.client.dispatch(Action::Plugin(
            yapfast_core::application::plugins::Action::LinkClosed,
        ));
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
