//! macOS asks the person for each permission when a feature first needs it.
//! On the very first start of the app bundle Yapfast asks for all of them in
//! turn instead, so the prompts come together rather than in the middle of a
//! call. A marker file in the data folder records that the round ran.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_core_graphics::{CGPreflightListenEventAccess, CGRequestListenEventAccess};
use objc2_foundation::NSError;
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNNotificationSettings, UNUserNotificationCenter,
};
use yapfast_core::macos_permissions::{self as system, Media, Status};

/// The marker's file name, in the data folder `instance.lock` is in.
const MARKER: &str = "permissions-asked";

/// How long a prompt may stay unanswered before the round moves on.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the round waits for the Screen Recording prompt, which the system
/// shows without telling when it is answered, before the next one.
const SCREEN_PROMPT_WAIT: Duration = Duration::from_secs(30);

/// How often the round looks for that answer.
const SCREEN_PROMPT_POLL: Duration = Duration::from_millis(500);

/// What the notification prompt is for, as `notify` asks for it.
const NOTIFICATION_OPTIONS: UNAuthorizationOptions = UNAuthorizationOptions::Alert;

/// One permission the round may ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Notifications,
    Microphone,
    Camera,
    ScreenRecording,
    InputMonitoring,
}

/// The steps in the order they are asked.
const ORDER: [Step; 5] = [
    Step::Notifications,
    Step::Microphone,
    Step::Camera,
    Step::ScreenRecording,
    Step::InputMonitoring,
];

/// The steps still due: those whose status is not decided, in asking order.
fn due(status_of: impl Fn(Step) -> Status) -> Vec<Step> {
    ORDER
        .into_iter()
        .filter(|step| status_of(*step) == Status::NotDetermined)
        .collect()
}

/// Whether the round ran before in the data folder `root`.
fn already_asked(root: &Path) -> bool {
    marker(root).exists()
}

fn marker(root: &Path) -> PathBuf {
    root.join(MARKER)
}

/// Asks for every permission not decided yet, once for the data folder
/// `root`, from a thread of its own so the window never waits. Outside an app
/// bundle, where the prompts can't name Yapfast, it does nothing.
pub fn ask_once(root: &Path) {
    if !crate::notify::in_bundle() {
        tracing::debug!("not running from an app bundle; no permission round");
        return;
    }
    if already_asked(root) {
        tracing::debug!("the permission round ran before");
        return;
    }
    let root = root.to_owned();
    let started = std::thread::Builder::new()
        .name("yapfast-permissions".into())
        .spawn(move || {
            // A panic in a system callback must not take the app with it.
            if std::panic::catch_unwind(|| run_round()).is_err() {
                tracing::error!("the permission round panicked");
                return;
            }
            if let Err(error) = std::fs::create_dir_all(&root)
                .and_then(|()| std::fs::write(marker(&root), b"asked\n"))
            {
                tracing::warn!(%error, "could not record the permission round");
            }
        });
    if let Err(error) = started {
        tracing::warn!(%error, "could not start the permission round");
    }
}

fn run_round() {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let steps = due(|step| status(step, &center));
    tracing::info!(?steps, "asking for the permissions not decided yet");
    for step in steps {
        let answer = ask(step, &center);
        tracing::info!(?step, ?answer, "permission asked");
    }
}

fn status(step: Step, center: &UNUserNotificationCenter) -> Status {
    let status = match step {
        Step::Notifications => notification_status(center),
        Step::Microphone => system::media_status(Media::Microphone),
        Step::Camera => system::media_status(Media::Camera),
        Step::ScreenRecording => system::screen_recording_status(),
        Step::InputMonitoring => input_monitoring_status(),
    };
    tracing::info!(?step, ?status, "permission status");
    status
}

fn ask(step: Step, center: &UNUserNotificationCenter) -> Status {
    match step {
        Step::Notifications => request_notifications(center),
        Step::Microphone => system::request_media(Media::Microphone, PROMPT_TIMEOUT),
        Step::Camera => system::request_media(Media::Camera, PROMPT_TIMEOUT),
        Step::ScreenRecording => request_screen_recording(),
        Step::InputMonitoring => request_input_monitoring(),
    }
}

// ---- Notifications ----

fn notification_status_from(status: UNAuthorizationStatus) -> Status {
    match status {
        UNAuthorizationStatus::NotDetermined => Status::NotDetermined,
        UNAuthorizationStatus::Denied => Status::Denied,
        _ => Status::Granted,
    }
}

fn notification_status(center: &UNUserNotificationCenter) -> Status {
    let (answer, answered) = std::sync::mpsc::sync_channel::<Status>(1);
    let handler = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        // SAFETY: the framework passes valid settings for the call's length.
        let settings = unsafe { settings.as_ref() };
        let _ = answer.try_send(notification_status_from(settings.authorizationStatus()));
    });
    center.getNotificationSettingsWithCompletionHandler(&handler);
    answered.recv_timeout(PROMPT_TIMEOUT).unwrap_or_else(|_| {
        tracing::warn!("the system did not report the notification settings");
        Status::Denied
    })
}

fn request_notifications(center: &UNUserNotificationCenter) -> Status {
    let (answer, answered) = std::sync::mpsc::sync_channel::<Status>(1);
    let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        // SAFETY: the framework passes a valid error or null.
        if let Some(error) = unsafe { error.as_ref() } {
            tracing::warn!(%error, "could not ask to show notifications");
        }
        let _ = answer.try_send(if granted.as_bool() {
            Status::Granted
        } else {
            Status::Denied
        });
    });
    center.requestAuthorizationWithOptions_completionHandler(NOTIFICATION_OPTIONS, &handler);
    answered
        .recv_timeout(PROMPT_TIMEOUT)
        .unwrap_or(Status::NotDetermined)
}

// ---- Screen Recording and Input Monitoring ----

/// Shows the prompt, then waits for the permission to turn up, up to
/// [`SCREEN_PROMPT_WAIT`], so the next prompt doesn't cover this one.
fn request_screen_recording() -> Status {
    if system::request_screen_recording() {
        return Status::Granted;
    }
    let started = Instant::now();
    while started.elapsed() < SCREEN_PROMPT_WAIT {
        std::thread::sleep(SCREEN_PROMPT_POLL);
        if system::screen_recording_status() == Status::Granted {
            return Status::Granted;
        }
    }
    system::screen_recording_status()
}

fn input_monitoring_status() -> Status {
    system::status_from_screen_access(CGPreflightListenEventAccess())
}

fn request_input_monitoring() -> Status {
    system::status_from_screen_access(CGRequestListenEventAccess())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_undecided_is_asked_in_order() {
        assert_eq!(due(|_| Status::NotDetermined), ORDER.to_vec());
    }

    #[test]
    fn decided_permissions_are_skipped() {
        let steps = due(|step| match step {
            Step::Microphone => Status::Granted,
            Step::Camera => Status::Denied,
            _ => Status::NotDetermined,
        });
        assert_eq!(
            steps,
            [
                Step::Notifications,
                Step::ScreenRecording,
                Step::InputMonitoring
            ]
        );
    }

    #[test]
    fn nothing_is_due_when_everything_is_decided() {
        assert!(due(|_| Status::Granted).is_empty());
    }

    #[test]
    fn notification_authorizations_map_to_statuses() {
        assert_eq!(
            notification_status_from(UNAuthorizationStatus::NotDetermined),
            Status::NotDetermined
        );
        assert_eq!(
            notification_status_from(UNAuthorizationStatus::Denied),
            Status::Denied
        );
        assert_eq!(
            notification_status_from(UNAuthorizationStatus::Authorized),
            Status::Granted
        );
        assert_eq!(
            notification_status_from(UNAuthorizationStatus::Provisional),
            Status::Granted
        );
    }

    #[test]
    fn the_marker_sits_in_the_data_folder() {
        let folder = tempfile::tempdir().unwrap();
        assert!(!already_asked(folder.path()));
        std::fs::write(marker(folder.path()), b"asked\n").unwrap();
        assert!(already_asked(folder.path()));
    }
}
