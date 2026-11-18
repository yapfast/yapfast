//! The call's connection in detail (`src/ui/call_link.rs`): the ping, its
//! average and the packets lost, the pings of the last minutes, the voice
//! privacy code while others are in the call, and how a shared screen goes
//! out. The call measures them all along; they're copied out only as the
//! popout opens and once a second while it shows, or as the pointer comes
//! to the status.

use yapfast_core::application::AppState;
use yapfast_core::call_details::{
    average, encoder, groups, last_ping, loss, milliseconds, picture, rate,
};

use anomi_vm::voice::connection::{Connection, Stream};

/// Copies the call's numbers into the popout.
pub fn read(state: &AppState, connection: &mut Connection) {
    let voice = &state.voice;
    let stats = (voice.link.as_ref())
        .map(yapfast_core::voice::Link::read)
        .unwrap_or_default();
    let others = voice.participants.len() > 1;
    let code = stats.privacy_code.as_deref().filter(|_| others);
    let made = Connection {
        tab: connection.tab,
        ping: last_ping(&stats).unwrap_or_default(),
        average: average(&stats.pings).map(milliseconds).unwrap_or_default(),
        loss: loss(&stats).unwrap_or_default(),
        code: code.map(groups).unwrap_or_default(),
        stream: stats.stream.as_ref().map(|stream| Stream {
            encoder: encoder(&stream.engine),
            codec: stream.codec.clone(),
            picture: picture(stream),
            sent: rate(stream.sent_kbps),
            target: rate(stream.target_kbps),
            keyframes: stream.keyframe_requests.to_string(),
            resent: stream.resent.to_string(),
        }),
        end_to_end: voice.end_to_end,
        copied: connection.copied,
        pings: stats.pings,
    };
    if *connection != made {
        *connection = made;
    }
}
