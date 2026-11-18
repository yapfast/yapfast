//! Runs the program: the daemon iced keeps going without a window, while
//! the app sits in the tray; its fonts, theme and executor.

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
    // Before the first frame: the fonts with Twemoji ahead of any system
    // emoji font and the name faces at every weight, none copied.
    fonts::install();
    let layout = Layout::read(&root);
    // Another Anomi runs on the folder and took this start over.
    let Some(instance) = crate::platform::instance::claim(&root, &layout, &start) else {
        return Ok(());
    };
    language::start(layout.language());
    let mut app = App::new(Arc::new(runtime), root, layout);
    app.desktop.instance = instance;
    // iced boots through a closure it may call again; the state goes once.
    let app = Cell::new(Some(app));
    let boot = move || app.take().expect("the program boots once").boot(&start);
    iced::daemon(boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        // Moved as the person's font changes, so every pixel is drawn again.
        .style(|app: &App, theme| app.shown.settings.kept.repaint.style(theme))
        .scale_factor(App::scale_factor)
        .subscription(App::subscription)
        .font(typography::UI)
        .executor::<Small>()
        .run()
        .map_err(|error| anyhow!("the window failed: {error}"))
}
