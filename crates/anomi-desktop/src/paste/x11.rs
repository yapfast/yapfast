//! The CLIPBOARD selection, asked of its owner as the ICCCM has it: its
//! targets, then the one Yapfast takes, each into a property on a window
//! of Yapfast's own. Data too big for one request comes in parts (INCR).
//! Text comes before pictures, as office suites offer a picture of the
//! cells or text they copy.

use std::error::Error;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, Property, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use super::{Clipboard, PICTURES, files};

/// How long the clipboard's owner may take to answer, or to send the
/// next part.
const PATIENCE: Duration = Duration::from_secs(5);

const TEXT: [&str; 6] = [
    "UTF8_STRING",
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "STRING",
    "TEXT",
    "text/plain",
];

pub fn read() -> Result<Clipboard, Box<dyn Error>> {
    let (connection, screen) = x11rb::connect(None)?;
    let names = [
        "CLIPBOARD",
        "YAPFAST_PASTE",
        "INCR",
        "TARGETS",
        "text/uri-list",
    ]
    .into_iter()
    .chain(TEXT)
    .chain(PICTURES.map(|(kind, _)| kind));
    let cookies = names
        .map(|name| connection.intern_atom(false, name.as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    let atoms = cookies
        .into_iter()
        .map(|cookie| Ok(cookie.reply()?.atom))
        .collect::<Result<Vec<Atom>, Box<dyn Error>>>()?;
    let [clipboard, property, incr, targets, uri_list] = atoms[..5] else {
        unreachable!()
    };
    let (text, pictures) = atoms[5..].split_at(TEXT.len());

    let window = connection.generate_id()?;
    let root = connection.setup().roots[screen].root;
    connection.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    let reader = Reader {
        connection,
        window,
        clipboard,
        property,
        incr,
    };

    let Some(offered) = reader.get(targets)? else {
        return Ok(Clipboard::Other);
    };
    let offered: Vec<Atom> = offered
        .chunks_exact(4)
        .map(|atom| u32::from_ne_bytes(atom.try_into().unwrap()))
        .collect();
    if offered.contains(&uri_list)
        && let Some(list) = reader.get(uri_list)?
    {
        let found = files(&String::from_utf8_lossy(&list));
        if !found.is_empty() {
            return Ok(Clipboard::Files(found));
        }
    }
    if text.iter().any(|kind| offered.contains(kind)) {
        return Ok(Clipboard::Other);
    }
    for (kind, (_, extension)) in pictures.iter().zip(PICTURES) {
        if offered.contains(kind)
            && let Some(bytes) = reader.get(*kind)?
        {
            return Ok(Clipboard::Picture(bytes, extension));
        }
    }
    Ok(Clipboard::Other)
}

struct Reader {
    connection: RustConnection,
    window: Window,
    clipboard: Atom,
    property: Atom,
    incr: Atom,
}

impl Reader {
    /// The clipboard as `target`, or `None` if its owner can't give it.
    fn get(&self, target: Atom) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.connection.convert_selection(
            self.window,
            self.clipboard,
            target,
            self.property,
            x11rb::CURRENT_TIME,
        )?;
        self.connection.flush()?;
        let Event::SelectionNotify(notify) = self.wait(|event| {
            matches!(event, Event::SelectionNotify(notify)
                if notify.requestor == self.window && notify.target == target)
        })?
        else {
            unreachable!()
        };
        if notify.property == x11rb::NONE {
            return Ok(None);
        }
        // Taking the property deletes it, which asks for INCR's first part.
        let reply = self.take()?;
        if reply.type_ != self.incr {
            return Ok(Some(reply.value));
        }
        let mut bytes = Vec::new();
        loop {
            self.wait(|event| {
                matches!(event, Event::PropertyNotify(change)
                    if change.window == self.window
                        && change.atom == self.property
                        && change.state == Property::NEW_VALUE)
            })?;
            let part = self.take()?.value;
            if part.is_empty() {
                return Ok(Some(bytes));
            }
            bytes.extend_from_slice(&part);
        }
    }

    fn take(&self) -> Result<x11rb::protocol::xproto::GetPropertyReply, Box<dyn Error>> {
        let reply = self
            .connection
            .get_property(
                true,
                self.window,
                self.property,
                AtomEnum::ANY,
                0,
                u32::MAX / 4,
            )?
            .reply()?;
        Ok(reply)
    }

    fn wait(&self, wanted: impl Fn(&Event) -> bool) -> Result<Event, Box<dyn Error>> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            while let Some(event) = self.connection.poll_for_event()? {
                if wanted(&event) {
                    return Ok(event);
                }
            }
            if Instant::now() > deadline {
                return Err("the clipboard's owner stopped answering".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
