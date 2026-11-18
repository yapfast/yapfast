//! XInput 2's raw key events on X11, which any client may hear.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, bail};
use x11rb::{
    connection::Connection as _,
    protocol::{
        Event,
        xinput::{self, ConnectionExt as _},
        xproto::{self, ConnectionExt as _},
    },
    rust_connection::RustConnection,
};

use super::super::{Matcher, Reach, Report, Sink, Status};

/// XInput 2's raw key events from the root window, which come whatever
/// window has the focus and leave the key to it. A thread waits on them.
pub struct X11 {
    connection: Arc<RustConnection>,
    window: xproto::Window,
    stop: Arc<AtomicBool>,
}

impl X11 {
    pub(super) fn start(mut matcher: Matcher, sink: Sink, report: Report) -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        let version = connection.xinput_xi_query_version(2, 2)?.reply()?;
        if version.major_version < 2 {
            bail!("the X server has no XInput 2");
        }
        // A window of its own to wake the thread with when stopping.
        let window = connection.generate_id()?;
        connection.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            xproto::WindowClass::INPUT_ONLY,
            0,
            &xproto::CreateWindowAux::new(),
        )?;
        connection.xinput_xi_select_events(
            root,
            &[xinput::EventMask {
                deviceid: xinput::Device::ALL_MASTER.into(),
                mask: vec![
                    xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE,
                ],
            }],
        )?;
        connection.flush()?;
        let connection = Arc::new(connection);
        let stop = Arc::new(AtomicBool::new(false));
        let (waiting, stopped) = (connection.clone(), stop.clone());
        std::thread::Builder::new()
            .name("yapfast-keybinds".into())
            .spawn(move || {
                let out = &mut |action, down| sink.act(action, down);
                while !stopped.load(Ordering::Relaxed) {
                    // X11 keycodes are evdev's plus 8.
                    match waiting.wait_for_event() {
                        Ok(Event::XinputRawKeyPress(event)) => {
                            matcher.key(event.detail.saturating_sub(8), true, out);
                        }
                        Ok(Event::XinputRawKeyRelease(event)) => {
                            matcher.key(event.detail.saturating_sub(8), false, out);
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(target: crate::LOG, %error, "lost the X server's keys");
                            report(Status {
                                reach: Reach::Focused("Yapfast lost the X server's keys"),
                                given: HashMap::new(),
                            });
                            break;
                        }
                    }
                }
                matcher.release_all(out);
            })?;
        Ok(Self {
            connection,
            window,
            stop,
        })
    }
}

impl Drop for X11 {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let wake =
            xproto::ClientMessageEvent::new(32, self.window, xproto::AtomEnum::NONE, [0u32; 5]);
        let _ = self
            .connection
            .send_event(false, self.window, xproto::EventMask::NO_EVENT, wake);
        let _ = self.connection.flush();
    }
}
