//! Interface language and the clock's hour cycle.

use iced::Element;

use super::{Pref, pref};
use crate::{
    app::{Message, settings},
    form,
    i18n::{self, tr, tr_mark},
    settings::{Drop, Event, Page, Section},
    time_format::TimeFormat,
};

const TIME_FORMATS: [(TimeFormat, &str); 3] = [
    (TimeFormat::Auto, tr_mark!("Auto")),
    (TimeFormat::Hour12, tr_mark!("12-hour")),
    (TimeFormat::Hour24, tr_mark!("24-hour")),
];

impl<'a> Page<'a> {
    pub(in crate::settings) fn language_time_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let languages = std::iter::once(tr!(self.cx.i18n, "System language").to_owned())
            .chain(
                i18n::LANGUAGES
                    .iter()
                    .map(|language| language.name.to_owned()),
            )
            .collect();
        let formats = TIME_FORMATS
            .iter()
            .map(|&(_, source)| self.cx.i18n.text(source).to_owned())
            .collect();
        let current_format = TIME_FORMATS
            .iter()
            .position(|&(format, _)| format == prefs.time_format());
        form::page([
            form::title(theme, self.cx.i18n.text(Section::LanguageTime.title())),
            form::group([
                form::heading(theme, tr!(self.cx.i18n, "Interface language")),
                form::dropdown(
                    languages,
                    Some(i18n::choice(prefs.language.as_deref())),
                    self.s.open == Some(Drop::Language),
                    settings(Event::Drop(Drop::Language)),
                    |index| pref(Pref::Language(index)),
                )
                .width(280.0)
                .view(self.cx.look),
            ]),
            form::group([
                form::heading(theme, tr!(self.cx.i18n, "Time format")),
                form::dropdown(
                    formats,
                    current_format,
                    self.s.open == Some(Drop::TimeFormat),
                    settings(Event::Drop(Drop::TimeFormat)),
                    |index| pref(Pref::TimeFormat(TIME_FORMATS[index].0)),
                )
                .width(280.0)
                .view(self.cx.look),
            ]),
        ])
    }
}
