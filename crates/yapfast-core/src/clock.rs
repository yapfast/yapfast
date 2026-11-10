//! Message times and days in the local time zone and the chosen date format.

use std::sync::atomic::{AtomicU8, Ordering};

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};

use crate::i18n::{Language, language, tr_ctx_in, tr_in};

/// How a date's day, month and year are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFormat {
    /// `20/10/2026`, `20 October 2026`.
    DayMonthYear,
    /// `10/20/2026`, `October 20, 2026`.
    MonthDayYear,
    /// `2026-10-20`.
    YearMonthDay,
}

impl DateFormat {
    pub const ALL: [DateFormat; 3] = [
        DateFormat::DayMonthYear,
        DateFormat::MonthDayYear,
        DateFormat::YearMonthDay,
    ];

    /// The pattern a date in digits follows, such as `{day}/{month}/{year}`,
    /// in the language: German writes `{day}.{month}.{year}`.
    fn short(self, language: Language) -> &'static str {
        tr_ctx_in(
            language,
            "short date",
            match self {
                Self::DayMonthYear => "{day}/{month}/{year}",
                Self::MonthDayYear => "{month}/{day}/{year}",
                Self::YearMonthDay => "{year}-{month}-{day}",
            },
        )
    }

    /// The pattern a date in words follows, such as `{day} {month} {year}`.
    fn words(self, language: Language) -> &'static str {
        tr_ctx_in(
            language,
            "long date",
            match self {
                Self::DayMonthYear => "{day} {month} {year}",
                Self::MonthDayYear => "{month} {day}, {year}",
                Self::YearMonthDay => "{year}-{month}-{day}",
            },
        )
    }

    /// Discord's `<t:…>` timestamps in digits, in English.
    fn numeric(self) -> &'static str {
        match self {
            Self::DayMonthYear => "%d/%m/%Y",
            Self::MonthDayYear => "%m/%d/%Y",
            Self::YearMonthDay => "%Y-%m-%d",
        }
    }

    /// Discord's `<t:…>` timestamps in words, in English.
    fn long(self) -> &'static str {
        match self {
            Self::DayMonthYear => "%-d %B %Y",
            Self::MonthDayYear => "%B %-d, %Y",
            Self::YearMonthDay => "%Y-%m-%d",
        }
    }
}

const AUTO_FORMAT: u8 = 0;
static FORMAT: AtomicU8 = AtomicU8::new(AUTO_FORMAT);

/// Sets the date format for what follows; none follows the clock: 12-hour is
/// month first.
pub fn set_date_format(format: Option<DateFormat>) {
    let format = format.map_or(AUTO_FORMAT, |format| {
        DateFormat::ALL
            .iter()
            .position(|&f| f == format)
            .unwrap_or(0) as u8
            + 1
    });
    FORMAT.store(format, Ordering::Relaxed);
}

fn format_for(twelve_hour: bool) -> DateFormat {
    match FORMAT.load(Ordering::Relaxed) {
        // A language of its own writes dates the way its people do.
        AUTO_FORMAT if !language().is_english() => language().locale().dates,
        AUTO_FORMAT if twelve_hour => DateFormat::MonthDayYear,
        AUTO_FORMAT => DateFormat::DayMonthYear,
        chosen => DateFormat::ALL[usize::from(chosen - 1).min(2)],
    }
}

/// A phrase of the catalog with its `{time}`, `{weekday}` and `{day}` filled in,
/// in `language`.
fn phrase(language: Language, text: &'static str, time: &str, weekday: &str, day: &str) -> String {
    tr_in(language, text)
        .replace("{time}", time)
        .replace("{weekday}", weekday)
        .replace("{day}", day)
}

/// A weekday's name in `language`.
fn weekday_name(weekday: chrono::Weekday, language: Language) -> &'static str {
    use chrono::Weekday::*;
    tr_ctx_in(
        language,
        "weekday",
        match weekday {
            Mon => "Monday",
            Tue => "Tuesday",
            Wed => "Wednesday",
            Thu => "Thursday",
            Fri => "Friday",
            Sat => "Saturday",
            Sun => "Sunday",
        },
    )
}

/// A month's name in `language`.
fn month_name(month: u32, language: Language) -> &'static str {
    tr_ctx_in(
        language,
        "month",
        match month.clamp(1, 12) {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            _ => "December",
        },
    )
}

/// A date in the pattern `dates` gives in the current language: in digits
/// (`20/10/2026`) or with the month named (`20 October 2026`).
fn date_text<Tz: TimeZone>(then: &DateTime<Tz>, dates: DateFormat, in_words: bool) -> String {
    let language = language();
    // Year first stays in digits, as ISO writes it, even where the others use words.
    if in_words && dates != DateFormat::YearMonthDay {
        dates
            .words(language)
            .replace("{day}", &then.day().to_string())
            .replace("{month}", month_name(then.month(), language))
            .replace("{year}", &then.year().to_string())
    } else {
        dates
            .short(language)
            .replace("{day}", &format!("{:02}", then.day()))
            .replace("{month}", &format!("{:02}", then.month()))
            .replace("{year}", &then.year().to_string())
    }
}

/// Now, in Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// `text` with Discord's `<t:…>` timestamps written as times; as it is
/// without Discord.
pub fn render_timestamps(text: &str) -> String {
    #[cfg(feature = "discord")]
    return yapfast_discord::message::render_timestamps(text);
    #[cfg(not(feature = "discord"))]
    text.to_owned()
}

/// Writes the `<t:…>` timestamps in Discord messages on a 12-hour clock from
/// now on, or a 24-hour one, as the app's setting says, and their dates in
/// the format [`set_date_format`] set.
pub fn set_twelve_hour_clock(on: bool) {
    #[cfg(feature = "discord")]
    {
        let dates = format_for(on);
        yapfast_discord::message::set_timestamp_format(yapfast_discord::message::TimestampFormat {
            twelve_hour: on,
            short_date: dates.numeric(),
            long_date: dates.long(),
        });
    }
    #[cfg(not(feature = "discord"))]
    let _ = on;
}

/// The clock time of a Unix time, such as `14:05`.
pub fn time(seconds: u64) -> String {
    time_on(seconds, false)
}

/// The clock time of a Unix time on a 24-hour clock, such as `21:41`, or a 12-hour one,
/// such as `9:41 PM`.
pub fn time_on(seconds: u64, twelve_hour: bool) -> String {
    time_in(&Local, seconds, twelve_hour, false)
}

/// The same with its seconds, such as `21:41:05` or `9:41:05 PM`, when
/// `with_seconds`.
pub fn time_with(seconds: u64, twelve_hour: bool, with_seconds: bool) -> String {
    time_in(&Local, seconds, twelve_hour, with_seconds)
}

/// The calendar date of a Unix time, such as `3 October 2026`; empty when unknown.
pub fn day(seconds: u64) -> String {
    if seconds == 0 {
        return String::new();
    }
    day_in(&Local, seconds, format_for(false))
}

fn time_in<Tz: TimeZone>(zone: &Tz, seconds: u64, twelve_hour: bool, with_seconds: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let then = at(zone, seconds);
    let format = match (twelve_hour, with_seconds) {
        (true, false) => "%-I:%M",
        (true, true) => "%-I:%M:%S",
        (false, false) => "%H:%M",
        (false, true) => "%H:%M:%S",
    };
    let time = then.format(format).to_string();
    if !twelve_hour {
        return time;
    }
    // AM and PM, worded and placed as the language does.
    let language = language();
    let period = tr_ctx_in(
        language,
        "period",
        if then.hour12().0 { "PM" } else { "AM" },
    );
    tr_ctx_in(language, "12-hour time", "{time} {period}")
        .replace("{time}", &time)
        .replace("{period}", period)
}

/// When a Unix time is from `now`, as Discord's client words an audit log
/// entry's: `Today at 21:41`, `Yesterday at …`, `Last Monday at …` within
/// the week before, `Tomorrow at …` or the weekday within the week after,
/// and its [`day`] beyond.
pub fn calendar(seconds: u64, now: u64, twelve_hour: bool) -> String {
    calendar_in(&Local, seconds, now, twelve_hour, format_for(twelve_hour))
}

fn calendar_in<Tz: TimeZone>(
    zone: &Tz,
    seconds: u64,
    now: u64,
    twelve_hour: bool,
    dates: DateFormat,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    near_in(zone, seconds, now, twelve_hour).unwrap_or_else(|| day_in(zone, seconds, dates))
}

/// [`calendar`]'s words within a week of `now`; none beyond.
fn near_in<Tz: TimeZone>(zone: &Tz, seconds: u64, now: u64, twelve_hour: bool) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let then = at(zone, seconds);
    let days = (then.date_naive() - at(zone, now).date_naive()).num_days();
    let time = time_in(zone, seconds, twelve_hour, false);
    let language = language();
    let weekday = weekday_name(then.weekday(), language);
    let say = |text: &'static str| phrase(language, text, &time, weekday, "");
    Some(match days {
        0 => say("Today at {time}"),
        -1 => say("Yesterday at {time}"),
        1 => say("Tomorrow at {time}"),
        -6..=-2 => say("Last {weekday} at {time}"),
        2..=6 => say("{weekday} at {time}"),
        _ => return None,
    })
}

/// When a scheduled event runs, from `now`: its start as [`calendar`]
/// words it, but with the time on any day, such as `14 November 2026 at
/// 21:00`, and its end after a dash, as a time on the same day or a
/// day and time on another.
pub fn event(starts: u64, ends: Option<u64>, now: u64, twelve_hour: bool) -> String {
    event_in(
        &Local,
        starts,
        ends,
        now,
        twelve_hour,
        format_for(twelve_hour),
    )
}

fn event_in<Tz: TimeZone>(
    zone: &Tz,
    starts: u64,
    ends: Option<u64>,
    now: u64,
    twelve_hour: bool,
    dates: DateFormat,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let when = |seconds: u64| {
        near_in(zone, seconds, now, twelve_hour).unwrap_or_else(|| {
            let time = time_in(zone, seconds, twelve_hour, false);
            let day = day_in(zone, seconds, dates);
            phrase(language(), "{day} at {time}", &time, "", &day)
        })
    };
    let start = when(starts);
    match ends.filter(|&ends| ends > starts) {
        Some(ends) if at(zone, ends).date_naive() == at(zone, starts).date_naive() => {
            format!("{start} – {}", time_in(zone, ends, twelve_hour, false))
        }
        Some(ends) => format!("{start} – {}", when(ends)),
        None => start,
    }
}

/// When a message was sent, as Discord's client stamps it from `now`: its
/// time today, such as `21:41`, `Yesterday at …` or `Tomorrow at …`, and
/// the date before the time on other days, `20/10/2026 21:41` on a 24-hour
/// clock and `10/20/2026 9:41 PM` on a 12-hour one, as British and American
/// English write it, unless a date format is chosen.
pub fn sent(seconds: u64, now: u64, twelve_hour: bool, with_seconds: bool) -> String {
    sent_in(
        &Local,
        seconds,
        now,
        twelve_hour,
        with_seconds,
        format_for(twelve_hour),
    )
}

fn sent_in<Tz: TimeZone>(
    zone: &Tz,
    seconds: u64,
    now: u64,
    twelve_hour: bool,
    with_seconds: bool,
    dates: DateFormat,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let then = at(zone, seconds);
    let days = (then.date_naive() - at(zone, now).date_naive()).num_days();
    let time = time_in(zone, seconds, twelve_hour, with_seconds);
    match days {
        0 => time,
        -1 => phrase(language(), "Yesterday at {time}", &time, "", ""),
        1 => phrase(language(), "Tomorrow at {time}", &time, "", ""),
        _ => format!("{} {time}", date_text(&then, dates, false)),
    }
}

/// The local calendar date of a Unix time, which [`day`] names; none when unknown.
pub fn date(seconds: u64) -> Option<chrono::NaiveDate> {
    (seconds != 0).then(|| at(&Local, seconds).date_naive())
}

fn day_in<Tz: TimeZone>(zone: &Tz, seconds: u64, dates: DateFormat) -> String
where
    Tz::Offset: std::fmt::Display,
{
    date_text(&at(zone, seconds), dates, true)
}

fn at<Tz: TimeZone>(zone: &Tz, seconds: u64) -> DateTime<Tz> {
    let utc =
        DateTime::from_timestamp(i64::try_from(seconds).unwrap_or(i64::MAX), 0).unwrap_or_default();
    utc.with_timezone(zone)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    #[test]
    fn times_and_days_follow_the_time_zone() {
        // 2026-10-06 23:30 UTC.
        let seconds = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(time_in(&utc, seconds, false, false), "23:30");
        assert_eq!(time_in(&utc, seconds, true, false), "11:30 PM");
        assert_eq!(time_in(&utc, seconds + 5, false, true), "23:30:05");
        assert_eq!(time_in(&utc, seconds + 5, true, true), "11:30:05 PM");
        assert_eq!(
            day_in(&utc, seconds, DateFormat::DayMonthYear),
            "6 October 2026"
        );
        assert_eq!(time_in(&plus_two, seconds, false, false), "01:30");
        assert_eq!(time_in(&plus_two, seconds, true, false), "1:30 AM");
        assert_eq!(
            day_in(&plus_two, seconds, DateFormat::DayMonthYear),
            "7 October 2026"
        );
        assert_eq!(day(0), "");
    }

    #[test]
    fn calendar_times_count_days_as_discords_client_does() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let day = 86_400;
        assert_eq!(
            calendar_in(&utc, now - 60, now, false, DateFormat::DayMonthYear),
            "Today at 23:29"
        );
        assert_eq!(
            calendar_in(&utc, now - day, now, true, DateFormat::DayMonthYear),
            "Yesterday at 11:30 PM"
        );
        assert_eq!(
            calendar_in(&utc, now - 6 * day, now, false, DateFormat::DayMonthYear),
            "Last Wednesday at 23:30"
        );
        assert_eq!(
            calendar_in(&utc, now - 7 * day, now, false, DateFormat::DayMonthYear),
            "29 September 2026"
        );
        assert_eq!(
            calendar_in(&utc, now + day, now, false, DateFormat::DayMonthYear),
            "Tomorrow at 23:30"
        );
        assert_eq!(
            calendar_in(&utc, now + 4 * day, now, false, DateFormat::DayMonthYear),
            "Saturday at 23:30"
        );
    }

    #[test]
    fn event_times_give_the_time_on_any_day_and_the_end() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let (hour, day) = (3_600, 86_400);
        assert_eq!(
            event_in(&utc, now + day, None, now, false, DateFormat::DayMonthYear),
            "Tomorrow at 23:30"
        );
        assert_eq!(
            event_in(
                &utc,
                now + 10 * day,
                None,
                now,
                true,
                DateFormat::DayMonthYear
            ),
            "16 October 2026 at 11:30 PM",
            "a week away or more still says when"
        );
        let starts = now + 10 * day - 2 * hour;
        assert_eq!(
            event_in(
                &utc,
                starts,
                Some(starts + hour),
                now,
                false,
                DateFormat::DayMonthYear
            ),
            "16 October 2026 at 21:30 – 22:30"
        );
        assert_eq!(
            event_in(
                &utc,
                now + day,
                Some(now + day + hour),
                now,
                false,
                DateFormat::DayMonthYear
            ),
            "Tomorrow at 23:30 – Thursday at 00:30",
            "an end on another day says which"
        );
        assert_eq!(
            event_in(
                &utc,
                now + day,
                Some(now + day),
                now,
                false,
                DateFormat::DayMonthYear
            ),
            "Tomorrow at 23:30"
        );
    }

    #[test]
    fn sent_times_show_the_date_on_other_days_as_discords_client_does() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let day = 86_400;
        assert_eq!(
            sent_in(&utc, now - 60, now, false, false, DateFormat::DayMonthYear),
            "23:29"
        );
        assert_eq!(
            sent_in(&utc, now - 55, now, true, true, DateFormat::MonthDayYear),
            "11:29:05 PM"
        );
        assert_eq!(
            sent_in(&utc, now - day, now, true, false, DateFormat::MonthDayYear),
            "Yesterday at 11:30 PM"
        );
        assert_eq!(
            sent_in(
                &utc,
                now - 2 * day,
                now,
                false,
                false,
                DateFormat::DayMonthYear
            ),
            "04/10/2026 23:30"
        );
        assert_eq!(
            sent_in(
                &utc,
                now - 2 * day,
                now,
                true,
                false,
                DateFormat::MonthDayYear
            ),
            "10/04/2026 11:30 PM"
        );
        assert_eq!(
            sent_in(&utc, now + day, now, false, false, DateFormat::DayMonthYear),
            "Tomorrow at 23:30"
        );
        // Days turn at local midnight: two hours east, two hours ago was yesterday.
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            sent_in(
                &utc,
                now - 7200,
                now,
                false,
                false,
                DateFormat::DayMonthYear
            ),
            "21:30"
        );
        assert_eq!(
            sent_in(
                &plus_two,
                now - 7200,
                now,
                false,
                false,
                DateFormat::DayMonthYear
            ),
            "Yesterday at 23:30"
        );
    }

    #[test]
    fn dates_follow_the_chosen_format() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let then = now - 2 * 86_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let sent = |dates| sent_in(&utc, then, now, false, false, dates);
        assert_eq!(sent(DateFormat::DayMonthYear), "04/10/2026 23:30");
        assert_eq!(sent(DateFormat::MonthDayYear), "10/04/2026 23:30");
        assert_eq!(sent(DateFormat::YearMonthDay), "2026-10-04 23:30");
        assert_eq!(
            day_in(&utc, then, DateFormat::MonthDayYear),
            "October 4, 2026"
        );
        assert_eq!(day_in(&utc, then, DateFormat::YearMonthDay), "2026-10-04");
        assert_eq!(
            calendar_in(&utc, now - 9 * 86_400, now, false, DateFormat::YearMonthDay),
            "2026-09-27"
        );
        // Five and a half hours east, the half hour carries into the next day.
        let india = FixedOffset::east_opt(330 * 60).unwrap();
        assert_eq!(
            sent_in(&india, then, now, false, false, DateFormat::YearMonthDay),
            "2026-10-05 05:00"
        );
    }
}
