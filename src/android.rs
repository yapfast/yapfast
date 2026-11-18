//! Android platform integration, NativeActivity lifecycle, JNI bridges, and
//! runtime permissions / file pickers / foreground call service.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicI64, Ordering},
    },
};

use android_activity::{AndroidApp, MainEvent, PollEvent};
use jni::{
    EnvUnowned, JavaVM, jni_sig, jni_str,
    objects::{Global, JObject, JObjectArray, JString, JValue},
    sys::jlong,
};
use yapfast_core::runtime::{BackgroundRuntime, oneshot};

type PickWaiters = Mutex<HashMap<i64, oneshot::Sender<Option<Vec<PathBuf>>>>>;
type SaveWaiters = Mutex<HashMap<i64, oneshot::Sender<Option<PathBuf>>>>;
type PermWaiters = Mutex<HashMap<i64, oneshot::Sender<bool>>>;

static JVM: OnceLock<JavaVM> = OnceLock::new();
static ACTIVITY: OnceLock<Global<JObject<'static>>> = OnceLock::new();

static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);
static PICK_WAITERS: OnceLock<PickWaiters> = OnceLock::new();
static SAVE_WAITERS: OnceLock<SaveWaiters> = OnceLock::new();
static PERM_WAITERS: OnceLock<PermWaiters> = OnceLock::new();
static PENDING_SAVES: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

static OPEN_ROOM_HANDLER: Mutex<Option<fn(String)>> = Mutex::new(None);
static DISCONNECT_CALL_HANDLER: Mutex<Option<fn()>> = Mutex::new(None);
static DEEP_LINK_HANDLER: Mutex<Option<fn(String)>> = Mutex::new(None);
static FOCUS_HANDLER: Mutex<Option<fn(bool)>> = Mutex::new(None);
static PENDING_ROOM: Mutex<Option<String>> = Mutex::new(None);
static PENDING_LINK: Mutex<Option<String>> = Mutex::new(None);

fn pick_waiters() -> &'static PickWaiters {
    PICK_WAITERS.get_or_init(Mutex::default)
}

fn save_waiters() -> &'static SaveWaiters {
    SAVE_WAITERS.get_or_init(Mutex::default)
}

fn perm_waiters() -> &'static PermWaiters {
    PERM_WAITERS.get_or_init(Mutex::default)
}

fn pending_saves() -> &'static Mutex<Vec<PathBuf>> {
    PENDING_SAVES.get_or_init(Mutex::default)
}

/// Entrypoint invoked by `crates/yapfast-android` `android_main`.
pub fn main(app: AndroidApp) {
    yapfast_core::logging::init();
    if let Err(error) = yapfast_core::init_tls() {
        tracing::error!(%error, "could not initialize TLS on Android");
        return;
    }

    let root = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/data/io.github.yapfast/files"));
    if let Err(error) = std::fs::create_dir_all(&root) {
        tracing::error!(%error, "could not create Android data directory");
        return;
    }
    yapfast_core::set_android_data_directory(root.clone());

    #[cfg(feature = "discord")]
    yapfast_core::keep_discord_files(&root);

    init_jni(&app);

    if let Err(error) = slint::android::init_with_event_listener(app.clone(), |event| {
        if let PollEvent::Main(main_event) = event {
            match main_event {
                MainEvent::GainedFocus => notify_focus(true),
                MainEvent::LostFocus => notify_focus(false),
                _ => {}
            }
        }
    }) {
        tracing::error!(%error, "could not register Slint Android platform");
        return;
    }

    let runtime = match BackgroundRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start async runtime on Android");
            return;
        }
    };

    if let Err(error) = crate::ui::run_android(runtime, root) {
        tracing::error!("{error:#}");
    }
}

fn init_jni(app: &AndroidApp) {
    let jvm_ptr = app.vm_as_ptr().cast::<jni::sys::JavaVM>();
    let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
    if !yapfast_core::init_webrtc_android(jvm_ptr.cast(), activity_ptr.cast()) {
        tracing::warn!("could not initialize WebRTC Android context");
    }
    // SAFETY: `jvm_ptr` and `activity_ptr` come from `AndroidActivity` and stay
    // alive for the process/activity lifetime.
    let vm = unsafe { JavaVM::from_raw(jvm_ptr) };
    let init_res: jni::errors::Result<()> = vm.attach_current_thread(|env| {
        let local_activity = unsafe { JObject::from_raw(env, activity_ptr) };
        let global_activity = env.new_global_ref(&local_activity)?;
        let _ = ACTIVITY.set(global_activity);
        rustls_platform_verifier::android::init_with_env(env, local_activity)?;
        Ok(())
    });
    if let Err(error) = init_res {
        tracing::warn!(%error, "JNI runtime setup reported an error");
    }
    let _ = JVM.set(vm);
}

fn with_activity<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&mut jni::Env<'_>, &JObject<'_>) -> jni::errors::Result<T>,
{
    let vm = JVM.get()?;
    let activity = ACTIVITY.get()?;
    let result: jni::errors::Result<T> = vm.attach_current_thread(|env| {
        let out = f(env, activity);
        if env.exception_check() {
            env.exception_describe();
            env.exception_clear();
        }
        out
    });
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(%error, "Android Activity JNI call failed");
            None
        }
    }
}

pub(crate) fn set_open_room_handler(handler: fn(String)) {
    *OPEN_ROOM_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(handler);
    if let Some(room) = PENDING_ROOM
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        let _ = slint::invoke_from_event_loop(move || handler(room));
    }
}

pub(crate) fn set_ui_handlers(disconnect_call: fn(), deep_link: fn(String), focus: fn(bool)) {
    *DISCONNECT_CALL_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(disconnect_call);
    *DEEP_LINK_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(deep_link);
    *FOCUS_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(focus);
    if let Some(link) = PENDING_LINK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        let _ = slint::invoke_from_event_loop(move || deep_link(link));
    }
}

pub(crate) fn take_pending_link() -> Option<String> {
    PENDING_LINK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

fn notify_focus(focused: bool) {
    let handler = *FOCUS_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handler) = handler {
        let _ = slint::invoke_from_event_loop(move || handler(focused));
    }
}

/// Opens a web URL in the Android browser.
pub(crate) fn open_url(url: &str) {
    with_activity(|env, activity| {
        let j_url = env.new_string(url)?;
        env.call_method(
            activity,
            jni_str!("openUrl"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&j_url)],
        )?;
        Ok(())
    });
}

/// Posts a message or incoming-call notification through `YapfastActivity`.
pub(crate) fn show_notification(room: &str, title: &str, body: &str, is_call: bool) {
    with_activity(|env, activity| {
        let j_room = env.new_string(room)?;
        let j_title = env.new_string(title)?;
        let j_body = env.new_string(body)?;
        env.call_method(
            activity,
            jni_str!("showMessageNotification"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)V"),
            &[
                JValue::Object(&j_room),
                JValue::Object(&j_title),
                JValue::Object(&j_body),
                JValue::Bool(is_call),
            ],
        )?;
        Ok(())
    });
}

/// Starts or stops the Android microphone foreground service during calls.
pub(crate) fn set_call_active(active: bool, title: &str) {
    with_activity(|env, activity| {
        let j_title = env.new_string(title)?;
        env.call_method(
            activity,
            jni_str!("setCallActive"),
            jni_sig!("(ZLjava/lang/String;)V"),
            &[JValue::Bool(active), JValue::Object(&j_title)],
        )?;
        Ok(())
    });
}

/// Checks whether an Android runtime permission is currently granted.
pub(crate) fn has_permission(permission: &str) -> bool {
    with_activity(|env, activity| {
        let j_perm = env.new_string(permission)?;
        env.call_method(
            activity,
            jni_str!("hasRuntimePermission"),
            jni_sig!("(Ljava/lang/String;)Z"),
            &[JValue::Object(&j_perm)],
        )?
        .z()
    })
    .unwrap_or(false)
}

/// Requests an Android runtime permission asynchronously (e.g. `RECORD_AUDIO`, `POST_NOTIFICATIONS`).
pub(crate) async fn request_permission(permission: &str) -> bool {
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    perm_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(request_id, tx);
    let called = with_activity(|env, activity| {
        let j_perm = env.new_string(permission)?;
        env.call_method(
            activity,
            jni_str!("requestRuntimePermission"),
            jni_sig!("(JLjava/lang/String;)V"),
            &[JValue::Long(request_id), JValue::Object(&j_perm)],
        )?;
        Ok(())
    });
    if called.is_none() {
        perm_waiters()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&request_id);
        return false;
    }
    rx.await.unwrap_or(false)
}

/// Launches the Android system document/media picker.
pub(crate) async fn pick_files(allow_multiple: bool, mime_type: &str) -> Option<Vec<PathBuf>> {
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    pick_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(request_id, tx);
    let called = with_activity(|env, activity| {
        let j_mime = env.new_string(mime_type)?;
        env.call_method(
            activity,
            jni_str!("pickFiles"),
            jni_sig!("(JZLjava/lang/String;)V"),
            &[
                JValue::Long(request_id),
                JValue::Bool(allow_multiple),
                JValue::Object(&j_mime),
            ],
        )?;
        Ok(())
    });
    if called.is_none() {
        pick_waiters()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&request_id);
        return None;
    }
    rx.await.ok().flatten()
}

/// Launches Android's `ACTION_CREATE_DOCUMENT` picker and returns a writable temporary staging path.
pub(crate) async fn prepare_save_file(title: &str, file_name: &str) -> Option<PathBuf> {
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    save_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(request_id, tx);
    let called = with_activity(|env, activity| {
        let j_title = env.new_string(title)?;
        let j_name = env.new_string(file_name)?;
        env.call_method(
            activity,
            jni_str!("prepareSaveFile"),
            jni_sig!("(JLjava/lang/String;Ljava/lang/String;)V"),
            &[
                JValue::Long(request_id),
                JValue::Object(&j_title),
                JValue::Object(&j_name),
            ],
        )?;
        Ok(())
    });
    if called.is_none() {
        save_waiters()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&request_id);
        return None;
    }
    let path = rx.await.ok().flatten()?;
    pending_saves()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(path.clone());
    Some(path)
}

/// Flushes saved temporary staging files into their chosen SAF document URIs.
pub(crate) fn finish_save(name: &str, ok: bool) {
    let paths: Vec<PathBuf> = {
        let mut guard = pending_saves()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut matched = Vec::new();
        let mut remaining = Vec::new();
        for path in guard.drain(..) {
            let matches_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == name);
            if matches_name || !ok {
                matched.push(path);
            } else {
                remaining.push(path);
            }
        }
        if matched.is_empty() && ok && !remaining.is_empty() {
            matched.push(remaining.remove(0));
        }
        *guard = remaining;
        matched
    };
    for path in paths {
        let Some(path_str) = path.to_str() else {
            continue;
        };
        with_activity(|env, activity| {
            let j_path = env.new_string(path_str)?;
            env.call_method(
                activity,
                jni_str!("finishSaveFile"),
                jni_sig!("(Ljava/lang/String;Z)V"),
                &[JValue::Object(&j_path), JValue::Bool(ok)],
            )?;
            Ok(())
        });
    }
}

pub fn jni_on_files_picked<'local>(
    mut env: EnvUnowned<'local>,
    request_id: jlong,
    paths: JObjectArray<'local, JString<'local>>,
) {
    let parsed: Option<Vec<PathBuf>> = match env
        .with_env(|env| -> jni::errors::Result<Option<Vec<PathBuf>>> {
            if paths.is_null() {
                return Ok(None);
            }
            let len = paths.len(env)?;
            let mut out = Vec::with_capacity(len);
            for index in 0..len {
                let item = paths.get_element(env, index)?;
                out.push(PathBuf::from(item.to_string()));
            }
            Ok(Some(out))
        })
        .into_outcome()
    {
        jni::Outcome::Ok(parsed) => parsed,
        _ => None,
    };
    if let Some(sender) = pick_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&request_id)
    {
        let _ = sender.send(parsed);
    }
}

pub fn jni_on_save_target<'local>(
    mut env: EnvUnowned<'local>,
    request_id: jlong,
    path: JString<'local>,
) {
    let parsed: Option<PathBuf> = match env
        .with_env(|_env| -> jni::errors::Result<Option<PathBuf>> {
            if path.is_null() {
                Ok(None)
            } else {
                Ok(Some(PathBuf::from(path.to_string())))
            }
        })
        .into_outcome()
    {
        jni::Outcome::Ok(parsed) => parsed,
        _ => None,
    };
    if let Some(sender) = save_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&request_id)
    {
        let _ = sender.send(parsed);
    }
}

pub fn jni_on_open_room<'local>(mut env: EnvUnowned<'local>, room: JString<'local>) {
    let parsed: Option<String> = match env
        .with_env(|_env| -> jni::errors::Result<Option<String>> {
            if room.is_null() {
                Ok(None)
            } else {
                Ok(Some(room.to_string()))
            }
        })
        .into_outcome()
    {
        jni::Outcome::Ok(parsed) => parsed,
        _ => None,
    };
    let Some(room) = parsed else { return };
    let handler = *OPEN_ROOM_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match handler {
        Some(handler) => {
            let _ = slint::invoke_from_event_loop(move || handler(room));
        }
        None => {
            *PENDING_ROOM
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(room);
        }
    }
}

pub fn jni_on_deep_link<'local>(mut env: EnvUnowned<'local>, link: JString<'local>) {
    let parsed: Option<String> = match env
        .with_env(|_env| -> jni::errors::Result<Option<String>> {
            if link.is_null() {
                Ok(None)
            } else {
                Ok(Some(link.to_string()))
            }
        })
        .into_outcome()
    {
        jni::Outcome::Ok(parsed) => parsed,
        _ => None,
    };
    let Some(link) = parsed else { return };
    let handler = *DEEP_LINK_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match handler {
        Some(handler) => {
            let _ = slint::invoke_from_event_loop(move || handler(link));
        }
        None => {
            *PENDING_LINK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(link);
        }
    }
}

pub fn jni_on_permission_result(request_id: jlong, granted: bool) {
    if let Some(sender) = perm_waiters()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&request_id)
    {
        let _ = sender.send(granted);
    }
}

pub fn jni_on_disconnect_call() {
    let handler = *DISCONNECT_CALL_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handler) = handler {
        let _ = slint::invoke_from_event_loop(handler);
    }
}
