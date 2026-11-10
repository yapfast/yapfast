//! `yapfast-plugins`, the process plugins run in.
//!
//! It exists only while at least one plugin instance does: the parent starts it
//! on the first activation and it exits when the last instance is evicted, so
//! nothing it holds — wasmtime's compile threads, a guest's linear memory —
//! outlives the plugins that needed it.
//!
//! Two subcommands:
//!
//! * `run` speaks the framed protocol on stdin and stdout. It is what the parent
//!   starts, and it holds no network access and no Discord token.
//! * `fetch` installs a plugin from a git URL, for seconds, and exits, taking
//!   `gix` with it.

use yapfast_plugin_host::{fetch, host};

/// Prints what this build can do, and exits.
const HELP: &str = "\
yapfast-plugins — the process Yapfast runs plugins in

Usage:
  yapfast-plugins run [--cache DIR] [--dev DIR]
  yapfast-plugins fetch <URL> --store DIR [--review-json]

`run` speaks the framed protocol on stdin and stdout and exits when the
parent closes it or when no plugin instance is left.

`fetch` resolves a git URL, checks the plugin it holds, and prints what it
found as JSON; the parent shows the review and commits the install.
";

fn main() {
    if let Err(error) = run() {
        eprintln!("yapfast-plugins: {error:#}");
        std::process::exit(1);
    }
}

/// Reads the subcommand and hands over to it.
fn run() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        None | Some("run") => host::run(host::Options::parse(arguments)?),
        Some("fetch") => fetch::run(arguments.collect()),
        Some("help" | "--help" | "-h") => {
            print!("{HELP}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown subcommand `{other}`; try `yapfast-plugins --help`"),
    }
}
