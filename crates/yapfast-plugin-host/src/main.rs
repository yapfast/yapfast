//! `yapfast-plugins`, the process plugins run in.
//!
//! It exists only while at least one plugin instance does: the parent starts it
//! on the first activation and it exits when the last instance is evicted, so
//! nothing it holds — wasmtime's compile threads, a guest's linear memory —
//! outlives the plugins that needed it.
//!
//! Three subcommands:
//!
//! * `run` speaks the framed protocol on stdin and stdout. It is what the parent
//!   starts, and it holds no network access and no Discord token.
//! * `fetch` reads a plugin out of its git repository, for seconds, and exits,
//!   taking `gix` with it. It prints one JSON object on stdout, which is how the
//!   parent learns what it found; see [`fetch`] for that protocol.
//! * `catalog` reads a marketplace registry the same way, and `verify-entry`
//!   checks one of its entries, which is what a registry's CI runs.

use yapfast_plugin_host::{fetch, host};

/// Prints what this build can do, and exits.
const HELP: &str = "\
yapfast-plugins — the process Yapfast runs plugins in

Usage:
  yapfast-plugins run [--cache DIR] [--dev DIR]
  yapfast-plugins fetch <URL>
  yapfast-plugins catalog <URL>
  yapfast-plugins verify-entry <FILE>

`run` speaks the framed protocol on stdin and stdout and exits when the
parent closes it or when no plugin instance is left.

`fetch` fetches a git URL, reads the plugin it holds, and prints what it found
as one JSON object; the files it read wait in a temporary directory for the
parent to copy into the store and delete.

`catalog` does the same for a marketplace registry: its index, every entry it
lists, and every icon.

`verify-entry` checks one registry entry against the repository it pins: the
schema, the license, the source link, the commit, the module's hash, the
manifest and the icon. A registry's CI runs it on every pull request.
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
        // `fetch` owns its status: it reports its own failure as part of what it
        // prints, so an error there is a JSON object and not a line on stderr.
        Some("fetch") => std::process::exit(fetch::run(arguments.collect())),
        Some("catalog") => std::process::exit(fetch::run_catalog(arguments.collect())),
        Some("verify-entry") => std::process::exit(fetch::run_verify(arguments.collect())),
        Some("help" | "--help" | "-h") => {
            print!("{HELP}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown subcommand `{other}`; try `yapfast-plugins --help`"),
    }
}
