//! Samples of a call's connection details for tests and shots, after the
//! fake Discord's data: task V02's.

use iced::{Rectangle, Size};

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::connection::{Connection, Stream, Tab};

/// Five minutes of a good connection: pings between 24 and 46 ms.
pub fn connection() -> Connection {
    let pings: Vec<u16> = (0..60u16)
        .map(|at| 24 + (at * 7) % 9 + at / 6 * 2 % 14)
        .collect();
    let last = pings.last().copied().unwrap_or(0);
    let average = pings.iter().map(|&ping| u32::from(ping)).sum::<u32>() / pings.len() as u32;
    Connection {
        ping: tr!("{} ms", last),
        average: tr!("{} ms", average),
        loss: "0.5%".into(),
        pings,
        ..Connection::default()
    }
}

/// The first moments of a call: no pings yet.
pub fn measuring() -> Connection {
    Connection::default()
}

/// Others are in the call: the privacy code shows, and its tab is picked.
pub fn private() -> Connection {
    Connection {
        tab: Tab::Privacy,
        code: ["12345", "67890", "24680", "13579", "11223", "44556"]
            .map(String::from)
            .to_vec(),
        end_to_end: true,
        ..connection()
    }
}

/// This account's screen is live: the stream tab is picked.
pub fn streaming() -> Connection {
    Connection {
        tab: Tab::Stream,
        stream: Some(Stream {
            encoder: tr!("NVENC on the GPU").into(),
            codec: "h264".into(),
            picture: tr!("{}×{} at {} fps", 1920, 1080, 60),
            sent: tr!("{} Mbit/s", "3.8"),
            target: tr!("{} Mbit/s", "4.0"),
            keyframes: "2".into(),
            resent: "14".into(),
        }),
        ..connection()
    }
}

/// The window with the popout open over the dock's signal icon, as the
/// golden `voice/connection_popout.png` has it.
pub fn popout() -> Vm {
    let mut vm = shell::open_at(
        super::dock::window(super::dock::joined()),
        Layer::Connection,
        Spot::below(Rectangle::new(
            iced::Point::new(8.0, 710.0),
            Size::new(64.0, 38.0),
        ))
        .gap(4.0),
    );
    vm.voice.connection = Connection {
        pings: vec![0, 0, 1, 1, 0],
        ping: tr!("{} ms", 0),
        average: tr!("{} ms", 0),
        ..Connection::default()
    };
    vm
}
