//! How long ago, as Active Now says it: `ActiveTime` in
//! `ui/active_now.slint:73`. Both count from the clock the app hands the
//! column (`ActiveNow::now`), so they advance only while it ticks.

use yapfast_lang::tr;

/// "13m", "1h", "2d": a card's time, as Discord's Active Now times a game.
pub fn since(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 3600 {
        tr!("{}m", (seconds / 60).max(1))
    } else if seconds < 86_400 {
        tr!("{}h", seconds / 3600)
    } else {
        tr!("{}d", seconds / 86_400)
    }
}

/// "01:19:45 elapsed": a row's time.
pub fn elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let two = |n: i64| format!("{n:02}");
    tr!(
        "{}:{}:{} elapsed",
        two(seconds / 3600),
        two(seconds / 60 % 60),
        two(seconds % 60)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_counts_in_minutes_then_hours_then_days() {
        assert_eq!(since(-5), "1m");
        assert_eq!(since(59), "1m");
        assert_eq!(since(13 * 60 + 20), "13m");
        assert_eq!(since(3600), "1h");
        assert_eq!(since(86_400 * 2 + 5), "2d");
    }

    #[test]
    fn a_row_counts_in_hours_minutes_and_seconds() {
        assert_eq!(elapsed(96), "00:01:36 elapsed");
        assert_eq!(elapsed(4785), "01:19:45 elapsed");
        assert_eq!(elapsed(-1), "00:00:00 elapsed");
    }
}
