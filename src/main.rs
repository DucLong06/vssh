//! vssh: a terminal SSH connection manager.

use anyhow::Result;
use vssh::{config, ui};

const HELP: &str = "\
vssh - terminal SSH connection manager

Usage: vssh [--help | --version]

Hosts come from ~/.ssh/config. Press ? inside vssh for the keys.
Theme and keybindings: ~/.config/vssh/config.toml";

// The only output is --help / --version, before the TUI starts.
#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some("-V" | "--version") => {
            println!("vssh {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("-h" | "--help") => {
            println!("{HELP}");
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown argument {other}\n\n{HELP}"),
    }
    let cfg = config::Config::load()?;
    // Two workers are plenty for I/O-bound work and keep startup cheap.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(ui::run(cfg))
}
