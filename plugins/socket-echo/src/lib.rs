//! `/socket-echo <port>`: `net.socket`, the powerful tier's raw connection.
//!
//! It connects to the port on this computer (`socket-local` in the manifest,
//! which the red install box says), sends `hello`, logs what comes back and
//! closes. The first time, Yapfast asks the person in a red dialog, and the
//! connection goes ahead once they say yes.

use yapfast_pdk::json::json;
use yapfast_pdk::prelude::*;

#[derive(Default)]
struct SocketEcho {
    /// The `net.connect` whose answers are this connection's news.
    waiting: Option<u64>,
}

impl Plugin for SocketEcho {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        let port = command.integer("port").unwrap_or(0);
        let answer = call::send("net.connect", json!({ "host": "127.0.0.1", "port": port }))?;
        match Option::<Pending>::from(answer.clone()) {
            Some(pending) => {
                self.waiting = pending.call();
                Ok(Reply::notice(format!("Connecting to port {port}…")))
            }
            None => Ok(Reply::notice(format!("Not connecting: {answer:?}"))),
        }
    }

    fn answered(&mut self, ctx: &Context<'_>) {
        let Some(waiting) = self.waiting else { return };
        for result in ctx.results().iter().filter(|result| result.call == waiting) {
            let HostReturn::Ok(news) = &result.answer else {
                log::error(format!("the connection didn't open: {:?}", result.answer));
                self.waiting = None;
                return;
            };
            let socket = news["socket"].clone();
            match news["news"].as_str() {
                Some("connected") => {
                    let _ = call::send("net.send", json!({ "socket": socket, "text": "hello\n" }));
                }
                Some("data") => {
                    log::info(format!("it answered: {}", news["data_base64"]));
                    let _ = call::send("net.close", json!({ "socket": socket }));
                }
                Some("closed") => {
                    log::info(format!("closed: {}", news["reason"]));
                    self.waiting = None;
                }
                _ => {}
            }
        }
    }
}

yapfast_plugin!(SocketEcho);
