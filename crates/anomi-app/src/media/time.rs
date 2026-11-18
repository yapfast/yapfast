//! How far a video or a voice message has played, as the players say it
//! (`video_time` and `voice_time`, `src/ui/refresh.rs:5639`, `:5655`), and
//! how often a playing video's bar moves.

/// A playing video's bar and time move at most this often, as a voice
/// message's position comes (`REPORT_EVERY` in the core): its frames
/// themselves redraw only their rectangle, while a step of the bar builds
/// the view again.
pub const STEP_MS: u64 = 250;

/// The step of the bar `time_ms` into a video falls on.
pub fn step(time_ms: u64) -> u64 {
    time_ms / STEP_MS
}

/// How far into a video, and how long it is when known, as "0:12 / 1:30".
pub fn video_time(at_ms: u64, duration_ms: Option<u64>) -> String {
    match duration_ms {
        Some(duration) => format!("{} / {}", clock(at_ms.min(duration)), clock(duration)),
        None => clock(at_ms),
    }
}

/// A voice message's time, in whole seconds rounded up as Discord's are, so
/// the last moments read 0:01 and a 2.4-second one 0:03.
pub fn voice_time(ms: u64) -> String {
    clock(ms.div_ceil(1000) * 1000)
}

fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds / 3600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_video_says_how_far_in_and_how_long() {
        assert_eq!(video_time(12_400, Some(90_000)), "0:12 / 1:30");
        assert_eq!(
            video_time(95_000, Some(90_000)),
            "1:30 / 1:30",
            "never past its end"
        );
        assert_eq!(video_time(3_725_000, None), "1:02:05");
    }

    #[test]
    fn a_voice_message_rounds_up_to_whole_seconds() {
        assert_eq!(voice_time(2_400), "0:03");
        assert_eq!(voice_time(1), "0:01");
        assert_eq!(voice_time(0), "0:00");
        assert_eq!(voice_time(3_600_000), "1:00:00");
    }

    #[test]
    fn the_bar_steps_four_times_a_second() {
        assert_eq!(step(0), step(249));
        assert_ne!(step(249), step(250));
        assert_eq!(step(1_000), 4);
    }
}
