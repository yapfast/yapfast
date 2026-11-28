//! Interface language and the clock's hour cycle.

use iced::Element;

use super::{Pref, pref};
use crate::{
    app::{Message, settings},
    form,
    i18n::{self, Key},
    settings::{Drop, Event, Page},
    time_format::TimeFormat,
};

const TIME_FORMATS: [(TimeFormat, Key); 3] = [
    (TimeFormat::Auto, Key::TimeAuto),
    (TimeFormat::Hour12, Key::TimeHour12),
    (TimeFormat::Hour24, Key::TimeHour24),
];

impl<'a> Page<'a> {
    pub(in crate::settings) fn language_time_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let tr = |key| self.cx.i18n.text(key);
        let languages = std::iter::once(tr(Key::SystemLanguage).into_owned())
            .chain(
                i18n::LANGUAGES
                    .iter()
                    .map(|language| language.name.to_owned()),
            )
            .collect();
        let formats = TIME_FORMATS
            .iter()
            .map(|&(_, key)| tr(key).into_owned())
            .collect();
        let current_format = TIME_FORMATS
            .iter()
            .position(|&(format, _)| format == prefs.time_format());
        form::page([
            form::title(theme, tr(Key::LanguageTime)),
            form::group([
                form::heading(theme, tr(Key::Language)),
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
                form::heading(theme, tr(Key::TimeFormat)),
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
