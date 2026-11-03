//! Message times and days in the local time zone.

use chrono::{DateTime, Local, TimeZone};

/// Now, in Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
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
    day_in(&Local, seconds)
}

fn time_in<Tz: TimeZone>(zone: &Tz, seconds: u64, twelve_hour: bool, with_seconds: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let format = match (twelve_hour, with_seconds) {
        (true, false) => "%-I:%M %p",
        (true, true) => "%-I:%M:%S %p",
        (false, false) => "%H:%M",
        (false, true) => "%H:%M:%S",
    };
    at(zone, seconds).format(format).to_string()
}

/// When a Unix time is from `now`, as Discord's client words an audit log
/// entry's: `Today at 21:41`, `Yesterday at …`, `Last Monday at …` within
/// the week before, `Tomorrow at …` or the weekday within the week after,
/// and its [`day`] beyond.
pub fn calendar(seconds: u64, now: u64, twelve_hour: bool) -> String {
    calendar_in(&Local, seconds, now, twelve_hour)
}

fn calendar_in<Tz: TimeZone>(zone: &Tz, seconds: u64, now: u64, twelve_hour: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    near_in(zone, seconds, now, twelve_hour).unwrap_or_else(|| day_in(zone, seconds))
}

/// [`calendar`]'s words within a week of `now`; none beyond.
fn near_in<Tz: TimeZone>(zone: &Tz, seconds: u64, now: u64, twelve_hour: bool) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let then = at(zone, seconds);
    let days = (then.date_naive() - at(zone, now).date_naive()).num_days();
    let time = time_in(zone, seconds, twelve_hour, false);
    Some(match days {
        0 => format!("Today at {time}"),
        -1 => format!("Yesterday at {time}"),
        1 => format!("Tomorrow at {time}"),
        -6..=-2 => format!("Last {} at {time}", then.format("%A")),
        2..=6 => format!("{} at {time}", then.format("%A")),
        _ => return None,
    })
}

/// When a scheduled event runs, from `now`: its start as [`calendar`]
/// words it, but with the time on any day, such as `14 November 2026 at
/// 21:00`, and its end after a dash, as a time on the same day or a
/// day and time on another.
pub fn event(starts: u64, ends: Option<u64>, now: u64, twelve_hour: bool) -> String {
    event_in(&Local, starts, ends, now, twelve_hour)
}

fn event_in<Tz: TimeZone>(
    zone: &Tz,
    starts: u64,
    ends: Option<u64>,
    now: u64,
    twelve_hour: bool,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let when = |seconds: u64| {
        near_in(zone, seconds, now, twelve_hour).unwrap_or_else(|| {
            let time = time_in(zone, seconds, twelve_hour, false);
            format!("{} at {time}", day_in(zone, seconds))
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
/// English write it.
pub fn sent(seconds: u64, now: u64, twelve_hour: bool, with_seconds: bool) -> String {
    sent_in(&Local, seconds, now, twelve_hour, with_seconds)
}

fn sent_in<Tz: TimeZone>(
    zone: &Tz,
    seconds: u64,
    now: u64,
    twelve_hour: bool,
    with_seconds: bool,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let then = at(zone, seconds);
    let days = (then.date_naive() - at(zone, now).date_naive()).num_days();
    let time = time_in(zone, seconds, twelve_hour, with_seconds);
    match days {
        0 => time,
        -1 => format!("Yesterday at {time}"),
        1 => format!("Tomorrow at {time}"),
        _ if twelve_hour => format!("{} {time}", then.format("%m/%d/%Y")),
        _ => format!("{} {time}", then.format("%d/%m/%Y")),
    }
}

/// The local calendar date of a Unix time, which [`day`] names; none when unknown.
pub fn date(seconds: u64) -> Option<chrono::NaiveDate> {
    (seconds != 0).then(|| at(&Local, seconds).date_naive())
}

fn day_in<Tz: TimeZone>(zone: &Tz, seconds: u64) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at(zone, seconds).format("%-d %B %Y").to_string()
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
        assert_eq!(day_in(&utc, seconds), "6 October 2026");
        assert_eq!(time_in(&plus_two, seconds, false, false), "01:30");
        assert_eq!(time_in(&plus_two, seconds, true, false), "1:30 AM");
        assert_eq!(day_in(&plus_two, seconds), "7 October 2026");
        assert_eq!(day(0), "");
    }

    #[test]
    fn calendar_times_count_days_as_discords_client_does() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let day = 86_400;
        assert_eq!(calendar_in(&utc, now - 60, now, false), "Today at 23:29");
        assert_eq!(
            calendar_in(&utc, now - day, now, true),
            "Yesterday at 11:30 PM"
        );
        assert_eq!(
            calendar_in(&utc, now - 6 * day, now, false),
            "Last Wednesday at 23:30"
        );
        assert_eq!(
            calendar_in(&utc, now - 7 * day, now, false),
            "29 September 2026"
        );
        assert_eq!(
            calendar_in(&utc, now + day, now, false),
            "Tomorrow at 23:30"
        );
        assert_eq!(
            calendar_in(&utc, now + 4 * day, now, false),
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
            event_in(&utc, now + day, None, now, false),
            "Tomorrow at 23:30"
        );
        assert_eq!(
            event_in(&utc, now + 10 * day, None, now, true),
            "16 October 2026 at 11:30 PM",
            "a week away or more still says when"
        );
        let starts = now + 10 * day - 2 * hour;
        assert_eq!(
            event_in(&utc, starts, Some(starts + hour), now, false),
            "16 October 2026 at 21:30 – 22:30"
        );
        assert_eq!(
            event_in(&utc, now + day, Some(now + day + hour), now, false),
            "Tomorrow at 23:30 – Thursday at 00:30",
            "an end on another day says which"
        );
        assert_eq!(
            event_in(&utc, now + day, Some(now + day), now, false),
            "Tomorrow at 23:30"
        );
    }

    #[test]
    fn sent_times_show_the_date_on_other_days_as_discords_client_does() {
        // Tuesday 2026-10-06 23:30 UTC.
        let now = 1_791_329_400;
        let utc = FixedOffset::east_opt(0).unwrap();
        let day = 86_400;
        assert_eq!(sent_in(&utc, now - 60, now, false, false), "23:29");
        assert_eq!(sent_in(&utc, now - 55, now, true, true), "11:29:05 PM");
        assert_eq!(
            sent_in(&utc, now - day, now, true, false),
            "Yesterday at 11:30 PM"
        );
        assert_eq!(
            sent_in(&utc, now - 2 * day, now, false, false),
            "04/10/2026 23:30"
        );
        assert_eq!(
            sent_in(&utc, now - 2 * day, now, true, false),
            "10/04/2026 11:30 PM"
        );
        assert_eq!(
            sent_in(&utc, now + day, now, false, false),
            "Tomorrow at 23:30"
        );
        // Days turn at local midnight: two hours east, two hours ago was yesterday.
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(sent_in(&utc, now - 7200, now, false, false), "21:30");
        assert_eq!(
            sent_in(&plus_two, now - 7200, now, false, false),
            "Yesterday at 23:30"
        );
    }
}
