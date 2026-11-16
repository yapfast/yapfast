//! Runs the program: the daemon iced keeps going without a window (for the
//! tray, I14), its fonts, theme and executor.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use yapfast_core::runtime::BackgroundRuntime;

use anomi_theme::{fonts, typography};

use crate::app::App;
use crate::cli::Start;
use crate::executor::Small;
use crate::layout::Layout;
use crate::{language, pictures};

pub fn run(runtime: BackgroundRuntime, root: PathBuf, start: Start) -> Result<()> {
    // Before any picture is decoded.
    pictures::install();
    let layout = Layout::read(&root);
    language::start(layout.language());
    // iced boots through a closure it may call again; the state goes once.
    let app = Cell::new(Some(App::new(Arc::new(runtime), root, layout)));
    let boot = move || app.take().expect("the program boots once").boot(&start);
    iced::daemon(boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .fonts(fonts::ALL)
        .font(typography::UI)
        .executor::<Small>()
        .run()
        .map_err(|error| anyhow!("the window failed: {error}"))
}
