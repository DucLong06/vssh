//! vssh: a terminal SSH connection manager.

use anyhow::Result;
use vssh::{theme::Config, ui};
use vssh_core::config::Lang;

const HELP: &str = "\
vssh - terminal SSH connection manager

Usage: vssh [--lang en|vi] [--help | --version]

Hosts come from ~/.ssh/config. Press ? inside vssh for the keys.
Theme, keybindings and language: ~/.config/vssh/config.toml

  --lang en|vi   language of the key hints and help (also: VSSH_LANG, or
                 lang = \"vi\" in config.toml)";

/// The help, with the config file where this system keeps it.
fn help() -> String {
    let windows = || HELP.replace("~/.config/vssh/config.toml", r"%APPDATA%\vssh\config.toml");
    cfg!(windows).then(windows).unwrap_or_else(|| HELP.into())
}

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
                println!("{}", help());
                return Ok(());
            }
            "--lang" => lang = Some(args.next().unwrap_or_default()),
            other => match other.strip_prefix("--lang=") {
                Some(code) => lang = Some(code.to_string()),
                None => anyhow::bail!("unknown argument {other}\n\n{}", help()),
            },
        }
    }
    let mut cfg = Config::load()?;
    if let Some(code) = lang {
        cfg.lang = Lang::parse(&code)?;
    }
    // The UI runs on this thread; its connections on the core's runtime.
    vssh_core::task::run(|| ui::run(cfg))?
}
