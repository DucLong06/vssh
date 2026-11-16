//! vssh: a terminal SSH connection manager.

mod config;
mod proto;
mod sftp;
mod ssh;
mod store;
mod ui;
mod vault;

use anyhow::Result;

const HELP: &str = "\
vssh - terminal SSH connection manager

Usage: vssh [--help | --version]

Hosts come from ~/.ssh/config. Press ? inside vssh for the keys.
Theme and keybindings: ~/.config/vssh/config.toml";

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
