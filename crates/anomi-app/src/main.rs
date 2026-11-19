//! `anomi`: the iced interface. It takes the Slint app's command line, so
//! `scripts/ui-session start --bin` runs it the same way; see `cli`.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    anomi_app::cli::main()
}
