//! The screen sharing portal, as far as a capture needs it: a session, what to
//! share, and the PipeWire node the desktop exported.
//!
//! Wayland has no way for an ordinary process to read a screen, and that is on
//! purpose: what a program can see is what it has been given. So the screen
//! arrives the way a file dialog does, over `org.freedesktop.portal.Desktop`, and
//! the portal does the two things only the desktop can. It asks the person which
//! screen or window to share — an ordinary compositor dialog, which a capture
//! therefore waits on — and it makes a PipeWire node carrying that screen's
//! frames, which is the fd in [`Session::pipewire`].
//!
//! The dance is four calls, each answered by a signal rather than a return, which
//! is what makes it awkward to read:
//!
//! 1. `CreateSession(a{sv})` → `o`, the *request*. The portal answers on
//!    `/org/freedesktop/portal/desktop/request/<sender>/<handle_token>` with
//!    `org.freedesktop.portal.Request.Response(u response, a{sv} results)`, and
//!    `results["session_handle"]` is the session object.
//! 2. `SelectSources(o session, a{sv})` → `o`: what to share — monitors,
//!    windows or either — how the pointer is drawn, and the token of a share the
//!    desktop already approved.
//! 3. `Start(s parent_window, o session, a{sv})` → `o`, answered on the same
//!    request path: `results["streams"]` is an `a(ua{sv})` whose first field is
//!    the PipeWire node, and `results["restore_token"]` is what the desktop will
//!    remember this share by.
//! 4. `OpenPipeWireRemote(o session, a{sv})` → `h`: a PipeWire socket connected as
//!    the portal's own client. A screencast's node is exported with restricted
//!    access, so it is not visible on the default socket; this is the fd that
//!    can see it, and it is why a capture needs no `pipewire` socket of its own.
//!
//! A `Response` of 0 is success, 1 the person saying no, and 2 anything else;
//! the last two are errors here, and cancel a session that has already started
//! like any other way out, since [`Session`] closes what it opened when it goes.
//!
//! Two things are not here, both deliberately. There is no `EnableCursor` call:
//! the pointer's mode is set in `SelectSources`, on portal 3 and later, and
//! nothing else is needed to have it in the frames. And there is no watch on the
//! session's own `Closed` signal: a capture waits on its stream for a frame, not on
//! the bus, and a session the desktop ends takes the PipeWire node with it, which is
//! what the stream reports as an end. An application that shares a screen without
//! reading frames would have to watch it.
use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator},
    message::Type as MessageType,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

/// The portal's name, object and interface on the session bus.
const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
/// A session's own interface, which is where `Close` is.
const SESSION: &str = "org.freedesktop.portal.Session";
/// The bus's own name, path and interface, which is where a share asks whether
/// the portal is on it at all: see [`answers`].
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const BUS_INTERFACE: &str = "org.freedesktop.DBus";

/// What a capture asks the desktop to share.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// One of the screens. The desktop picks, or asks which; a capture of a
    /// desktop that has several screens shares one of them, not all.
    #[default]
    Monitors,
    /// One window, which keeps working when it is moved, resized or covered, and
    /// shows what the window draws rather than what is on top of it.
    Windows,
    /// Either, as the person prefers.
    Both,
}

impl Source {
    /// The `types` bitmask `SelectSources` takes, as the portal numbers them.
    pub const fn types(self) -> u32 {
        match self {
            Self::Monitors => 1,
            Self::Windows => 2,
            Self::Both => 3,
        }
    }

    /// What this is called in a log line.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Monitors => "screens",
            Self::Windows => "windows",
            Self::Both => "a screen or a window",
        }
    }
}

/// How the pointer is drawn into the shared picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    /// Not at all. The right one for a capture whose viewer has its own pointer.
    Hidden,
    /// Painted into the frame by the desktop, so it is in what a viewer sees and
    /// costs the stream nothing.
    #[default]
    Embedded,
    /// As a separate stream of positions the compositor makes, which the viewer
    /// draws itself. It is the only mode that keeps a viewer's own pointer
    /// responsive, and it needs the frames to be composited with it; nothing in
    /// this crate reads it, so a share that asks for it gets its frames without a
    /// pointer unless the caller reads the metadata stream itself.
    Metadata,
}

impl Cursor {
    /// The `cursor_mode` value `SelectSources` takes, as the portal numbers them.
    pub const fn code(self) -> u32 {
        match self {
            Self::Hidden => 1,
            Self::Embedded => 2,
            Self::Metadata => 4,
        }
    }
}

/// Everything a capture says to the desktop before it is given a screen.
#[derive(Clone, Debug, Default)]
pub struct Ask {
    /// What kind of thing to share.
    pub source: Source,
    /// Whether the shared picture has the pointer in it.
    pub cursor: Cursor,
    /// A `restore_token` from an earlier session, which the desktop may take as
    /// the share already approved: no dialog, the same screen. It is what
    /// [`Session::restore_token`] gives back to store.
    pub restore_token: Option<String>,
}

impl Ask {
    /// Asking for `source`, with the pointer drawn in and no remembered share.
    pub fn new(source: Source) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }

    /// The same ask, drawing the pointer as `cursor` says.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = cursor;
        self
    }

    /// The same ask, with `token` as the share the desktop should reuse.
    pub fn restore_token(mut self, token: Option<String>) -> Self {
        self.restore_token = token;
        self
    }
}

/// One thing the desktop is sharing, as `Start` described it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    /// The PipeWire node the frames come on, which is what a stream connects to.
    pub node_id: u32,
    /// Where the shared thing is on the desktop, in the compositor's coordinates.
    pub position: (i32, i32),
    /// How large it is, in pixels, which is what a frame should be. A capture
    /// takes its size from the negotiated format instead: the desktop may share a
    /// scaled copy of a screen, and then the frames are not this size.
    pub size: (i32, i32),
    /// Which kind of thing was shared, where the desktop says: 1 for a monitor,
    /// 2 for a window. Absent from a portal older than 4.
    pub source_type: Option<u32>,
    /// The identifier a window or a screen keeps across sessions, for a share the
    /// desktop remembers. Absent when it is not remembered, and from a portal
    /// older than 4.
    pub id: Option<String>,
}

/// Whether a screen sharing portal answers this session's bus, which is the
/// question a share asks before it offers the desktop's dialog at all.
///
/// It costs one call on the bus and answers nothing about the person: a desktop
/// that has a portal can still refuse the share, and that refusal comes from
/// [`Session::open`]. What this keeps a share from is asking a desktop that has
/// no portal — XFCE on X11, say — and waiting for the timeout that comes back
/// instead of the dialog.
pub fn answers() -> bool {
    let Ok(connection) = Connection::session() else {
        return false;
    };
    let asked = connection.call_method(
        Some(BUS),
        BUS_PATH,
        Some(BUS_INTERFACE),
        "NameHasOwner",
        &(PORTAL,),
    );
    asked
        .ok()
        .and_then(|reply| reply.body().deserialize::<(bool,)>().ok())
        .is_some_and(|(owned,)| owned)
}

/// A share the desktop has started: the session object that holds it, what it is
/// sharing, and the token that asks for it again.
#[derive(Debug)]
pub struct Session {
    connection: Connection,
    path: OwnedObjectPath,
    streams: Vec<Stream>,
    restore_token: Option<String>,
}

impl Session {
    /// Asks the desktop for a screen, over the session bus, and returns once it
    /// has answered — which includes waiting for the person to pick one, so this
    /// can block for as long as a dialog is open, and it is cancelled by that
    /// dialog being cancelled.
    pub fn open(ask: &Ask) -> Result<Self> {
        let connection =
            Connection::session().context("no session bus to ask the desktop for a screen over")?;
        let mut signals = responses(&connection)?;
        let request = call(
            &connection,
            "CreateSession",
            &HashMap::from([("handle_token", Value::from(token(STEP_SESSION)))]),
        )?;
        let results = respond(&mut signals, &request)?;
        let path = session_handle(&results)?;
        // From here on there is a session to close, so every way out of this
        // function closes it: `session` is dropped on all of them.
        let mut session = Self {
            connection,
            path,
            streams: Vec::new(),
            restore_token: None,
        };
        let request = session.call(
            "SelectSources",
            &(ObjectPath::from(session.path.clone()), select_options(ask)),
        )?;
        respond(&mut signals, &request)?;
        let request = session.call(
            "Start",
            &(
                String::new(),
                ObjectPath::from(session.path.clone()),
                HashMap::<&str, Value>::new(),
            ),
        )?;
        let results = respond(&mut signals, &request)?;
        session.streams = streams(&results)?;
        session.restore_token = token_from(&results);
        Ok(session)
    }

    /// What the desktop is sharing.
    pub fn streams(&self) -> &[Stream] {
        &self.streams
    }

    /// The token a later [`Ask`] can hand back so the desktop shares the same
    /// thing without asking again. Saving it is what makes the second run of a
    /// stream start without a dialog; the desktop may answer with none, and an
    /// older portal never does, so a caller keeps whatever it already had.
    pub fn restore_token(&self) -> Option<&str> {
        self.restore_token.as_deref()
    }

    /// A PipeWire socket connected as the portal's client, on which the exported
    /// node is visible. The capture that uses it takes it and closes it.
    pub fn pipewire(&self) -> Result<std::os::fd::OwnedFd> {
        use zbus::zvariant::OwnedFd;
        let reply = self
            .connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some(SCREENCAST),
                "OpenPipeWireRemote",
                &(
                    ObjectPath::from(self.path.clone()),
                    HashMap::<&str, Value>::new(),
                ),
            )
            .context("the desktop wouldn't open its PipeWire connection")?;
        let fd: OwnedFd = reply
            .body()
            .deserialize()
            .context("the desktop sent no PipeWire connection")?;
        Ok(fd.into())
    }

    /// The session as one line for the log: what is shared, and the node its
    /// frames come on.
    pub fn describe(&self) -> String {
        let shared = self
            .streams
            .iter()
            .map(|stream| {
                let kind = match stream.source_type {
                    Some(1) => "screen",
                    Some(2) => "window",
                    _ => "thing",
                };
                format!(
                    "a {}x{} {} at {},{} on node {}",
                    stream.size.0,
                    stream.size.1,
                    kind,
                    stream.position.0,
                    stream.position.1,
                    stream.node_id
                )
            })
            .collect::<Vec<_>>()
            .join(" and ");
        if shared.is_empty() {
            "a session sharing nothing".to_owned()
        } else {
            shared
        }
    }

    fn call<B>(&self, method: &str, body: &B) -> Result<OwnedObjectPath>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        call(&self.connection, method, body)
    }
}

impl Drop for Session {
    /// Ends the share, so the desktop stops capturing and takes its dialog down.
    /// The session would end with the connection anyway; this is what makes it end
    /// before the process does.
    fn drop(&mut self) {
        let closed = self.connection.call_method(
            Some(PORTAL),
            self.path.as_str(),
            Some(SESSION),
            "Close",
            &(),
        );
        if let Err(error) = closed {
            tracing::debug!(
                error = format!("{error:#}"),
                "the desktop didn't take the screen share's close"
            );
        }
    }
}

/// What a request's `handle_token` is on the way to becoming: the portal puts it
/// in the request object's path, so it has to be a legal object path element.
const STEP_SESSION: &str = "session";

fn token(step: &str) -> String {
    format!("yapfast_{}_{step}", std::process::id())
}

/// Listens for the portal's answers before anything is asked, so that no
/// `Response` arrives before there is somewhere for it to go.
fn responses(connection: &Connection) -> Result<MessageIterator> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(PORTAL)?
        .build();
    MessageIterator::for_match_rule(rule, connection, Some(16))
        .context("the session bus wouldn't carry the portal's answers")
}

/// Calls one of the portal's methods and returns the request it made: the object
/// path its answer will come on.
fn call<B>(connection: &Connection, method: &str, body: &B) -> Result<OwnedObjectPath>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let reply = connection
        .call_method(Some(PORTAL), PATH, Some(SCREENCAST), method, body)
        .with_context(|| format!("the desktop couldn't be asked for a screen ({method})"))?;
    reply
        .body()
        .deserialize()
        .with_context(|| format!("the desktop named no request for {method}"))
}

/// Waits for the answer to `request` and returns its results: 0 is yes, 1 is the
/// person saying no, and 2 is the portal failing.
fn respond(
    signals: &mut MessageIterator,
    request: &ObjectPath<'_>,
) -> Result<HashMap<String, OwnedValue>> {
    while let Some(message) = signals.next() {
        let message = message.context("the session bus stopped carrying the portal's answers")?;
        let header = message.header();
        if header.message_type() != MessageType::Signal {
            continue;
        }
        if header.member().map(|member| member.as_str()) != Some("Response") {
            continue;
        }
        if header.path().map(|path| path.as_str()) != Some(request.as_str()) {
            continue;
        }
        let (code, results): (u32, HashMap<String, OwnedValue>) = message
            .body()
            .deserialize()
            .context("the desktop's answer was not a response")?;
        return match code {
            0 => Ok(results),
            1 => Err(anyhow!("the screen share was cancelled")),
            code => Err(anyhow!(
                "the desktop refused the screen share (response {code})"
            )),
        };
    }
    Err(anyhow!(
        "the session bus closed before the desktop answered"
    ))
}

/// The session object `CreateSession`'s answer names. The portal gives it as a
/// string in the map, and a portal that names it as an object path instead is
/// taken too: both spellings are in the wild.
fn session_handle(results: &HashMap<String, OwnedValue>) -> Result<OwnedObjectPath> {
    match results.get("session_handle").map(|value| &**value) {
        Some(Value::Str(handle)) => Ok(OwnedObjectPath::try_from(handle.as_str())?),
        Some(Value::ObjectPath(handle)) => Ok(handle.clone().into()),
        _ => bail!("the desktop started no session"),
    }
}

/// What `Start`'s answer says is being shared.
fn streams(results: &HashMap<String, OwnedValue>) -> Result<Vec<Stream>> {
    let Some(value) = results.get("streams") else {
        return Ok(Vec::new());
    };
    let value: &Value<'_> = value;
    let shared: Vec<(u32, HashMap<String, OwnedValue>)> = value
        .clone()
        .downcast()
        .context("the desktop named no screen to share")?;
    Ok(shared
        .into_iter()
        .map(|(node_id, options)| Stream {
            node_id,
            position: pair(&options, "position"),
            size: pair(&options, "size"),
            source_type: option(&options, "source_type"),
            id: option(&options, "id"),
        })
        .collect())
}

/// A `(i32, i32)` from a stream's options, which a portal that leaves it out means
/// as zero.
fn pair(options: &HashMap<String, OwnedValue>, key: &str) -> (i32, i32) {
    option(options, key).unwrap_or((0, 0))
}

/// One of a stream's or a response's options, if the desktop sent it and it is
/// the type asked for.
fn option<T>(options: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    for<'a> T: TryFrom<&'a Value<'a>>,
    for<'a> <T as TryFrom<&'a Value<'a>>>::Error: Into<zbus::zvariant::Error>,
{
    options.get(key)?.downcast_ref().ok()
}

/// The token the desktop will remember this share by, if it gave one.
fn token_from(results: &HashMap<String, OwnedValue>) -> Option<String> {
    option(results, "restore_token")
}

/// The options `SelectSources` takes: what to share, how the pointer is drawn,
/// and the share to reuse.
fn select_options(ask: &Ask) -> HashMap<&'static str, Value<'static>> {
    let mut options = HashMap::from([
        ("types", Value::from(ask.source.types())),
        ("multiple", Value::from(false)),
        ("cursor_mode", Value::from(ask.cursor.code())),
        // A token is worth having: with one, the next start of a stream needs no
        // dialog. 2 is "until the person forgets it", which is the portal's own
        // word for a remembered share; a desktop that keeps no permissions
        // ignores this and answers with no token.
        ("persist_mode", Value::from(2u32)),
        // The person's answer in the dialog, presented to them as this.
        ("handle_token", Value::from(token("sources"))),
    ]);
    if let Some(restore) = &ask.restore_token {
        options.insert("restore_token", Value::from(restore.clone()));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value of a `a{sv}`, as the bus would have handed it over.
    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("a value that can outlive its type")
    }
    #[test]
    fn a_source_becomes_the_portal_s_types_bitmask() {
        assert_eq!(Source::Monitors.types(), 1);
        assert_eq!(Source::Windows.types(), 2);
        assert_eq!(Source::Both.types(), 1 | 2);
    }

    #[test]
    fn a_cursor_mode_becomes_the_portal_s_number() {
        assert_eq!(Cursor::Hidden.code(), 1);
        assert_eq!(Cursor::Embedded.code(), 2);
        assert_eq!(Cursor::Metadata.code(), 4);
    }

    #[test]
    fn selecting_sources_asks_for_windows_with_the_pointer_drawn_in() {
        let options = select_options(&Ask::new(Source::Windows));
        assert_eq!(u32::try_from(&options["types"]).unwrap(), 2);
        assert_eq!(u32::try_from(&options["cursor_mode"]).unwrap(), 2);
        assert_eq!(bool::try_from(&options["multiple"]).unwrap(), false);
        assert_eq!(u32::try_from(&options["persist_mode"]).unwrap(), 2);
        assert!(!options.contains_key("restore_token"));
        assert!(
            options["handle_token"]
                .downcast_ref::<&str>()
                .unwrap()
                .starts_with("yapfast_")
        );
    }

    #[test]
    fn a_remembered_share_is_offered_back_to_the_desktop() {
        let ask = Ask::new(Source::Monitors)
            .cursor(Cursor::Hidden)
            .restore_token(Some("a token".to_owned()));
        let options = select_options(&ask);
        assert_eq!(u32::try_from(&options["cursor_mode"]).unwrap(), 1);
        assert_eq!(
            <&str>::try_from(&options["restore_token"]).unwrap(),
            "a token"
        );
    }

    #[test]
    fn a_handle_token_is_legal_in_an_object_path() {
        let token = token(STEP_SESSION);
        assert!(
            token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{token} would not survive being an object path's last element"
        );
        assert!(token.ends_with("session"));
    }

    #[test]
    fn a_session_comes_back_as_its_object_path() {
        let results = HashMap::from([(
            "session_handle".to_owned(),
            owned(Value::Str(
                "/org/freedesktop/portal/desktop/session/1_39/yapfast_4_session".into(),
            )),
        )]);
        assert_eq!(
            session_handle(&results).unwrap().as_str(),
            "/org/freedesktop/portal/desktop/session/1_39/yapfast_4_session"
        );
        assert!(session_handle(&HashMap::new()).is_err());
    }

    #[test]
    fn a_started_share_names_its_node_and_its_size() {
        let stream = HashMap::from([
            ("size".to_owned(), owned(Value::from((2560i32, 1440i32)))),
            ("position".to_owned(), owned(Value::from((0i32, 0i32)))),
            ("source_type".to_owned(), owned(Value::from(1u32))),
        ]);
        let results = HashMap::from([(
            "streams".to_owned(),
            owned(Value::from(vec![(42u32, stream)])),
        )]);
        assert_eq!(
            streams(&results).unwrap(),
            vec![Stream {
                node_id: 42,
                position: (0, 0),
                size: (2560, 1440),
                source_type: Some(1),
                id: None,
            }]
        );
    }

    #[test]
    fn a_share_with_no_streams_is_no_screens() {
        assert!(streams(&HashMap::new()).unwrap().is_empty());
    }

    #[test]
    fn a_remembered_token_comes_back() {
        let results =
            HashMap::from([("restore_token".to_owned(), owned(Value::Str("kept".into())))]);
        assert_eq!(token_from(&results).as_deref(), Some("kept"));
        assert_eq!(token_from(&HashMap::new()), None);
    }
}
