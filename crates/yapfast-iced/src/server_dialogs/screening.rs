//! The form a server asks new members to fill in (its rules to agree to, a
//! question to answer, a choice to make), shown in Join a Server's dialog
//! once the server was joined. What was answered is the core's
//! (`state.join.form`), so nothing is kept here.

use yapfast_core::{
    application::{JoinDialog, ScreeningDialog},
    chat::{ScreeningField, ScreeningKind},
};

use super::*;

/// How wide the form's choices may grow: the dialog's width less its padding.
const CHOICES: f32 = NARROW - 40.0;

impl<'a> View<'a> {
    /// The whole dialog while a form is on: the server, the questions or how
    /// handing them in goes, and the buttons.
    pub(super) fn screening_view(
        self,
        join: &'a JoinDialog,
        dialog: &'a ScreeningDialog,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut body: Vec<Element<'a, Message>> = vec![self.cx.look.dialog_head(
            tr("A few questions first").to_owned(),
            message(Event::CloseJoin),
        )];
        if let Some(preview) = join.preview.as_ref() {
            body.push(self.found(preview));
        }
        let busy = dialog.loading || dialog.submitting;
        if dialog.waiting {
            body.push(form::note(
                theme,
                tr(
                    "Your answers are with the server's moderators. You can talk as soon as they let you in.",
                ),
            ));
        } else if dialog.loading {
            body.push(form::note(theme, tr("Loading the server's questions...")));
        } else if let Some(form) = &dialog.form {
            if !form.description.is_empty() {
                body.push(form::note(theme, form.description.as_str()));
            }
            for (index, field) in form.fields.iter().enumerate() {
                body.push(self.screening_field(index, field, dialog));
            }
            if dialog.submitting {
                body.push(form::note(theme, tr("Handing in your answers...")));
            }
        } else if dialog.error.is_none() {
            body.push(form::note(
                theme,
                tr("The server's questions did not load."),
            ));
        }
        if let Some(error) = &dialog.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        let mut row_: Vec<Element<'a, Message>> = vec![
            action(
                theme,
                if dialog.waiting {
                    tr("Close")
                } else {
                    tr("Not now")
                },
                Kind::Ghost,
                Some(message(Event::CloseJoin)),
            )
            .into(),
        ];
        if !dialog.waiting {
            let ready = dialog.form.as_ref().is_none_or(|form| form.complete());
            row_.push(
                action(
                    theme,
                    if dialog.form.is_some() {
                        tr("Submit")
                    } else {
                        tr("Try again")
                    },
                    Kind::Primary,
                    (!busy && ready).then(|| message(Event::Confirm)),
                )
                .into(),
            );
        }
        body.push(buttons(row_));
        self.cx.look.framed(body, NARROW, message(Event::CloseJoin))
    }

    /// One question: its label (starred when it must be answered), what it
    /// says of itself, and the way to answer it.
    fn screening_field(
        self,
        index: usize,
        field: &'a ScreeningField,
        dialog: &ScreeningDialog,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let open = !dialog.submitting;
        let label = if field.required {
            format!("{} *", field.label)
        } else {
            field.label.clone()
        };
        let mut part: Vec<Element<'a, Message>> = vec![
            text(label)
                .size(14)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill)
                .into(),
        ];
        if let Some(description) = field
            .description
            .as_deref()
            .filter(|words| !words.is_empty())
        {
            part.push(form::note(theme, description));
        }
        match field.kind {
            ScreeningKind::Terms => {
                for (rule, words) in field.values.iter().enumerate() {
                    part.push(
                        text(format!("{}. {words}", rule + 1))
                            .size(14)
                            .color(theme.text)
                            .width(Length::Fill)
                            .into(),
                    );
                }
                part.push(
                    form::switch(
                        tr("I have read and agree to the rules"),
                        field.agreed,
                        open.then(|| message(Event::ScreeningAgree(index, !field.agreed))),
                    )
                    .view(self.cx.look),
                );
            }
            ScreeningKind::Text | ScreeningKind::Paragraph => {
                let input = form::field(
                    theme,
                    format!("screening-{index}"),
                    if field.required { "" } else { tr("Optional") },
                    field.text.as_str(),
                )
                .padding([8.0, 10.0]);
                part.push(if open {
                    input
                        .on_input(move |words| message(Event::ScreeningText(index, words)))
                        .into()
                } else {
                    input.into()
                });
            }
            ScreeningKind::Choice => {
                let values: Vec<&str> = field.values.iter().map(String::as_str).collect();
                part.push(controls::choices_within(
                    theme,
                    &values,
                    field.choice.unwrap_or(usize::MAX),
                    CHOICES,
                    move |choice| message(Event::ScreeningChoose(index, choice)),
                ));
            }
        }
        column(part).spacing(6).width(Length::Fill).into()
    }
}
