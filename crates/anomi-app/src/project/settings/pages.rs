//! The pages that show the layout alone: Messages, Notifications, Sounds,
//! Advanced and About, as the Slint app's `Layout::apply` sets their
//! properties (`src/ui.rs:1728`) and `src/ui/updates.rs` fills About. Each
//! is built again only after the layout changed.

use anomi_vm::settings::{about, advanced, messages, notifications, sounds};
use yapfast_core::new_tags;
use yapfast_core::sounds::{Sound, SoundSet};
use yapfast_lang::{PRODUCT, tr};

use super::Kept;
use crate::layout::{Animate, Layout};

/// The choices Settings offers for how long an empty private call waits, in
/// minutes, in its order (`WAITS`, `src/ui/alone.rs`), and the one until
/// another is chosen.
pub(crate) const WAITS: [u32; 5] = [0, 3, 10, 30, 60];
const WAIT: usize = 1;

/// The choice of [`WAITS`] the saved wait is, or the one nearest it.
pub(crate) fn alone_choice(layout: &Layout) -> usize {
    match layout.alone_timeout {
        None => WAIT,
        Some(0) => 0,
        Some(saved) => (1..WAITS.len())
            .min_by_key(|&choice| WAITS[choice].abs_diff(saved))
            .unwrap_or(WAIT),
    }
}

/// Sets `page` to `built` when they differ.
fn set<T: PartialEq>(page: &mut T, built: T) {
    if *page != built {
        *page = built;
    }
}

pub(super) fn messages(layout: &Layout, page: &mut messages::Messages) {
    use messages::{Animate as Shown, DateFormat, TextSize};
    let date_format = (layout.date_format)
        .and_then(|index| DateFormat::ALL.get(usize::try_from(index).ok()?).copied());
    let built = messages::Messages {
        compact: layout.compact_messages,
        text_size: match layout.text_size {
            Some(0) => TextSize::Small,
            Some(2) => TextSize::Large,
            _ => TextSize::Default,
        },
        twelve_hour: layout.twelve_hour_clock,
        seconds: layout.clock_seconds,
        date_format,
        animate: match layout.animate {
            Animate::Focused => Shown::Focused,
            Animate::Never => Shown::Never,
        },
        link_previews: layout.link_previews.unwrap_or(true),
        collapse_long: layout.collapse_long,
        boost_gems: layout.boost_gems.unwrap_or(true),
        share_typing: layout.share_typing.unwrap_or(true),
        reply_ping: layout.reply_ping.unwrap_or(true),
        strip_tracking: layout.strip_tracking.unwrap_or(true),
        matrix: cfg!(feature = "matrix"),
        discord: cfg!(feature = "discord"),
    };
    set(page, built);
}

pub(super) fn notifications(layout: &Layout, page: &mut notifications::Notifications) {
    let built = notifications::Notifications {
        notifications: layout.notifications.unwrap_or(true),
        text: layout.notification_text.unwrap_or(true),
        flash_taskbar: layout.flash_taskbar.unwrap_or(true),
        matrix: cfg!(feature = "matrix"),
        discord: cfg!(feature = "discord"),
    };
    set(page, built);
}

pub(super) fn sounds(layout: &Layout, page: &mut sounds::Sounds) {
    let set_at = SoundSet::ALL
        .iter()
        .position(|&set| set == layout.sound_set);
    let built = sounds::Sounds {
        set: set_at.unwrap_or(0),
        volume: layout.sound_volume.map_or(100, i32::from),
        adjust_each: layout.adjust_each_sound,
        each: Sound::ALL.map(|sound| layout.each_sound.get(&sound).map_or(100, |&v| v.into())),
    };
    set(page, built);
}

pub(super) fn advanced(layout: &Layout, kept: &Kept, page: &mut advanced::Advanced) {
    use anomi_desktop::{autostart, protocol};
    let links = cfg!(feature = "discord") && protocol::SUPPORTED;
    let built = advanced::Advanced {
        gpu_rendering: layout.gpu_rendering,
        gpu_encoding: layout.gpu_encoding.unwrap_or(true),
        close_to_tray: layout.close_to_tray.unwrap_or(true),
        multiple_windows: layout.multiple_windows,
        developer_mode: layout.developer_mode,
        start_on_sign_in: autostart::SUPPORTED && autostart::enabled(),
        discord_links: links && protocol::discord_enabled(layout.folder()),
        // This interface draws on the processor whatever the switch, which
        // the Slint app reads: nothing to restart for.
        gpu_running: layout.gpu_rendering,
        tray_available: kept.tray,
        autostart_available: autostart::SUPPORTED,
        discord_links_available: links,
    };
    set(page, built);
}

/// About: the build, the fork's name and the original credited; this fork
/// has no releases, so the update's switches are kept and nothing checks.
pub(super) fn about(layout: &Layout, page: &mut about::About) {
    let new = |key| new_tags::key_new(layout.first_version.as_deref(), &layout.seen_new, key);
    let built = about::About {
        build: crate::build_info::current(),
        copied: page.copied,
        product: PRODUCT.to_owned(),
        original: tr!("Original: {}", "Yapfast"),
        releases: false,
        manager: String::new(),
        status: about::Status::Idle,
        check: layout.check_for_updates,
        nightlies: layout.update_nightlies,
        fresh_check: new("check-updates"),
        fresh_nightlies: new("update-nightlies"),
        ..about::About::default()
    };
    set(page, built);
}
