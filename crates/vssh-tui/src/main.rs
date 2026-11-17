//! vssh: a terminal SSH connection manager.

use anyhow::Result;
use vssh::{config, ui};

const HELP: &str = "\
vssh - terminal SSH connection manager

Usage: vssh [--lang en|vi] [--help | --version]

Hosts come from ~/.ssh/config. Press ? inside vssh for the keys.
Theme, keybindings and language: ~/.config/vssh/config.toml

  --lang en|vi   language of the key hints and help (also: VSSH_LANG, or
                 lang = \"vi\" in config.toml)";

// The only output is --help / --version, before the TUI starts.
#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    // Later sources win: config.toml, then VSSH_LANG, then --lang.
    let mut lang = std::env::var("VSSH_LANG").ok().filter(|l| !l.is_empty());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("vssh {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            "--lang" => lang = Some(args.next().unwrap_or_default()),
            other => match other.strip_prefix("--lang=") {
                Some(code) => lang = Some(code.to_string()),
                None => anyhow::bail!("unknown argument {other}\n\n{HELP}"),
            },
        }
    }
    let mut cfg = config::Config::load()?;
    if let Some(code) = lang {
        cfg.lang = config::Lang::parse(&code)?;
    }
    // Two workers are plenty for I/O-bound work and keep startup cheap.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(ui::run(cfg))
}
