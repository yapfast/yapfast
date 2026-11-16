//! The log: the core's targets and this interface's, at info, or at debug
//! with `YAPFAST_LOG=debug`, as the Slint app logs (`yapfast_core::logging`,
//! whose filter knows only the `yapfast` targets).

use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;

pub fn init() {
    let level = if std::env::var("YAPFAST_LOG").is_ok_and(|level| level == "debug") {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let targets = Targets::new()
        .with_target("yapfast", level)
        .with_target("yapfast_core", level)
        .with_target("anomi_app", level)
        // iced's own warnings, such as a primitive tiny-skia can't draw.
        .with_target("iced", LevelFilter::WARN);
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(targets))
        .try_init();
}
