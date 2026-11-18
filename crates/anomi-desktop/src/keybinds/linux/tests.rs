use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use yapfast_core::runtime::BackgroundRuntime;
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use super::super::{
    Action, Key, Matcher, Reach, Report, Shortcut, Sink, TALK_SHORTCUT, Toggled, Watched,
};
use super::portal::{PATH, PORTAL, SHORTCUTS, portal};
use super::{Wanted, X11};

/// Waits up to two seconds for push to talk to be `held` or not.
fn wait_held(held: bool) {
    let until = Instant::now() + Duration::from_secs(2);
    while yapfast_core::push_to_talk::held() != held {
        assert!(Instant::now() < until, "the key wasn't held: {held}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn watched(id: &str, action: Action, keys: &[(u32, &str)]) -> Watched {
    Watched {
        id: id.into(),
        action,
        keys: Shortcut::new(
            keys.iter()
                .map(|&(scancode, trigger)| Key {
                    scancode,
                    name: trigger.to_uppercase(),
                    trigger: Some(trigger.into()),
                })
                .collect(),
        )
        .unwrap(),
    }
}

/// A sink that keeps the toggles it hears.
fn sink() -> (Sink, Arc<Mutex<Vec<Action>>>) {
    let toggles = Arc::new(Mutex::new(Vec::new()));
    let heard = toggles.clone();
    let toggled: Toggled = Arc::new(move |action| heard.lock().unwrap().push(action));
    (Sink { toggled }, toggles)
}

/// Types through XTEST, as a keyboard would, from a client of its own:
/// keys by their evdev codes, which X11 reports 8 higher.
struct Typist {
    connection: x11rb::rust_connection::RustConnection,
    root: u32,
}

impl Typist {
    fn new() -> Self {
        use x11rb::connection::Connection as _;
        let (connection, screen) = x11rb::connect(None).unwrap();
        let root = connection.setup().roots[screen].root;
        Self { connection, root }
    }

    fn key(&self, evdev: u8, down: bool) {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::{xproto, xtest::ConnectionExt as _};
        let kind = if down {
            xproto::KEY_PRESS_EVENT
        } else {
            xproto::KEY_RELEASE_EVENT
        };
        (self.connection)
            .xtest_fake_input(kind, evdev + 8, x11rb::CURRENT_TIME, self.root, 0, 0, 0)
            .unwrap();
        self.connection.flush().unwrap();
    }

    /// The keys pressed together, and let go the other way round.
    fn tap(&self, keys: &[u8]) {
        for &key in keys {
            self.key(key, true);
        }
        for &key in keys.iter().rev() {
            self.key(key, false);
        }
    }
}

/// Pressing the keys in another client runs the keybinds. Needs
/// `DISPLAY` on a private X server with XTEST, such as `Xvfb :57`, or
/// Xwayland on a private Weston.
#[test]
#[ignore = "needs DISPLAY on a private X server"]
fn x11_hears_the_keys_whatever_has_the_focus() {
    let (sink, toggles) = sink();
    let matcher = Matcher::new(&[
        watched("talk", Action::PushToTalk, &[(47, "v")]), // evdev's V
        watched("mute", Action::ToggleMute, &[(29, "Control_L"), (50, "m")]),
    ]);
    let x11 = X11::start(matcher, sink, Arc::new(|_| {})).unwrap();
    let (v, b, m, right_ctrl) = (47, 48, 50, 97);
    let typist = Typist::new();
    typist.key(v, true);
    wait_held(true);
    typist.key(v, false);
    wait_held(false);
    // Another key leaves it alone.
    typist.tap(&[b]);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!yapfast_core::push_to_talk::held());
    // The right Ctrl holds the combination as well as the left.
    typist.tap(&[right_ctrl, m]);
    typist.tap(&[m]);
    let until = Instant::now() + Duration::from_secs(2);
    while toggles.lock().unwrap().is_empty() {
        assert!(Instant::now() < until, "Ctrl+M didn't toggle");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(*toggles.lock().unwrap(), [Action::ToggleMute], "M alone");
    drop(x11);
}

/// A stand-in for xdg-desktop-portal's GlobalShortcuts: it answers
/// each request with Response on the request's path, binds each
/// shortcut to what was preferred, and then presses the first.
struct FakePortal {
    bound: Arc<Mutex<Vec<(String, String)>>>,
}

#[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
impl FakePortal {
    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> OwnedObjectPath {
        let request = request_path(&header, &options);
        let session = format!("{PATH}/session/test/1");
        let results = HashMap::from([("session_handle", Value::from(session))]);
        respond(connection, &request, results).await;
        request
    }

    async fn bind_shortcuts(
        &self,
        session: OwnedObjectPath,
        shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> OwnedObjectPath {
        let request = request_path(&header, &options);
        let mut bound = Vec::new();
        for (id, details) in &shortcuts {
            let Some(Value::Str(trigger)) = details.get("preferred_trigger").map(|v| &**v) else {
                panic!("no preferred trigger for {id}");
            };
            self.bound
                .lock()
                .unwrap()
                .push((id.clone(), trigger.to_string()));
            bound.push((
                id.clone(),
                HashMap::from([(
                    "trigger_description".to_owned(),
                    Value::from(trigger.to_uppercase()),
                )]),
            ));
        }
        respond(
            connection,
            &request,
            HashMap::from([("shortcuts", Value::from(bound))]),
        )
        .await;
        let connection = connection.clone();
        let (session, id) = (session.clone(), shortcuts[0].0.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            zbus::block_on(press(&connection, &session, &id, "Activated"));
        });
        request
    }
}

fn request_path(
    header: &zbus::message::Header<'_>,
    options: &HashMap<String, OwnedValue>,
) -> OwnedObjectPath {
    let sender = header
        .sender()
        .unwrap()
        .trim_start_matches(':')
        .replace('.', "_");
    let Some(Value::Str(token)) = options.get("handle_token").map(|v| &**v) else {
        panic!("no handle token");
    };
    OwnedObjectPath::try_from(format!("{PATH}/request/{sender}/{token}")).unwrap()
}

async fn respond(
    connection: &Connection,
    request: &OwnedObjectPath,
    results: HashMap<&str, Value<'_>>,
) {
    connection
        .emit_signal(
            None::<()>,
            request,
            "org.freedesktop.portal.Request",
            "Response",
            &(0u32, results),
        )
        .await
        .unwrap();
}

async fn press(connection: &Connection, session: &OwnedObjectPath, id: &str, signal: &str) {
    let options = HashMap::<&str, Value>::new();
    connection
        .emit_signal(
            None::<()>,
            PATH,
            SHORTCUTS,
            signal,
            &(session, id, 0u64, options),
        )
        .await
        .unwrap();
}

/// The portal's handshake, each keybind's shortcut, and push to talk
/// and a toggle going down and up. Needs a private session bus, as
/// `dbus-run-session` gives.
#[test]
#[ignore = "needs a private session bus"]
fn the_portal_binds_the_keybinds_and_runs_them() {
    let bound = Arc::new(Mutex::new(Vec::new()));
    let server = zbus::blocking::connection::Builder::session()
        .unwrap()
        .name(PORTAL)
        .unwrap()
        .serve_at(
            PATH,
            FakePortal {
                bound: bound.clone(),
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let keybinds = [
        watched(TALK_SHORTCUT, Action::PushToTalk, &[(47, "v")]),
        watched(
            "keybind-3",
            Action::ToggleDeafen,
            &[(29, "Control_L"), (42, "Shift_L"), (32, "d")],
        ),
    ];
    let told = Arc::new(Mutex::new(Vec::new()));
    let heard = told.clone();
    let report: Report = Arc::new(move |status| heard.lock().unwrap().push(status));
    let (sink, toggles) = sink();
    let wanted = keybinds
        .iter()
        .map(|watched| Wanted {
            id: watched.id.clone(),
            description: watched.action.label(),
            trigger: watched.keys.trigger(),
        })
        .collect();
    let mut matcher = Matcher::new(&keybinds);
    let runtime = BackgroundRuntime::new().unwrap();
    let _task = runtime.spawn(async move { portal(wanted, &mut matcher, &sink, &report).await });
    let until = Instant::now() + Duration::from_secs(5);
    while !yapfast_core::push_to_talk::held() {
        assert!(
            Instant::now() < until,
            "the shortcut wasn't held: {:?}",
            told.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        *bound.lock().unwrap(),
        [
            ("push-to-talk".to_owned(), "v".to_owned()),
            ("keybind-3".to_owned(), "CTRL+SHIFT+d".to_owned())
        ]
    );
    let status = told.lock().unwrap().last().cloned().unwrap();
    assert_eq!(status.reach, Reach::Desktop);
    assert!(status.talk_note().contains("shortcut V."));
    assert_eq!(
        status.given_note("keybind-3").as_deref(),
        Some("Your desktop gave it the shortcut CTRL+SHIFT+D.")
    );
    let session = OwnedObjectPath::try_from(format!("{PATH}/session/test/1")).unwrap();
    zbus::block_on(press(
        server.inner(),
        &session,
        TALK_SHORTCUT,
        "Deactivated",
    ));
    wait_held(false);
    zbus::block_on(press(server.inner(), &session, "keybind-3", "Activated"));
    zbus::block_on(press(server.inner(), &session, "keybind-3", "Deactivated"));
    let until = Instant::now() + Duration::from_secs(2);
    while toggles.lock().unwrap().is_empty() {
        assert!(Instant::now() < until, "the toggle wasn't heard");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*toggles.lock().unwrap(), [Action::ToggleDeafen]);
}
