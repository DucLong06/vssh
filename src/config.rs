//! `~/.config/vssh/config.toml`: theme and keybindings. Every field is optional.

use std::path::PathBuf;
use std::{fs, io};

use anyhow::{Context, Result};
use ratatui::style::Color;
use serde::{Deserialize, Deserializer};

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

pub fn ssh_config_path() -> PathBuf {
    home().join(".ssh/config")
}

/// Directory of the SQLite database and the vault.
pub fn data_dir() -> PathBuf {
    home().join(".local/share/vssh")
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: Theme,
    pub keys: Keys,
    /// TCP-probe every host at startup. Off by default: it touches every
    /// machine in the ssh config on each launch.
    pub ping_on_start: bool,
}

fn color<'de, D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
    String::deserialize(d)?
        .parse()
        .map_err(|_| serde::de::Error::custom("expected a colour name or #rrggbb"))
}

/// Colours are names (`cyan`, `darkgray`, `reset`, ...) or `#rrggbb`.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    #[serde(deserialize_with = "color")]
    pub accent: Color,
    #[serde(deserialize_with = "color")]
    pub border: Color,
    #[serde(deserialize_with = "color")]
    pub text: Color,
    #[serde(deserialize_with = "color")]
    pub dim: Color,
    #[serde(deserialize_with = "color")]
    pub selection: Color,
    #[serde(deserialize_with = "color")]
    pub ok: Color,
    #[serde(deserialize_with = "color")]
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            border: Color::DarkGray,
            text: Color::Reset,
            dim: Color::DarkGray,
            selection: Color::Rgb(38, 79, 120),
            ok: Color::Green,
            error: Color::Red,
        }
    }
}

/// Keys of the host list, one character each. `leader` is pressed with Ctrl.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Keys {
    pub leader: char,
    pub quit: char,
    pub search: char,
    pub sftp: char,
    pub forwards: char,
    pub snippets: char,
    pub add: char,
    pub edit: char,
    pub delete: char,
    pub duplicate: char,
    pub pin: char,
    pub mark: char,
    pub ping: char,
    pub password: char,
    pub vault: char,
    pub help: char,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            leader: 'a',
            quit: 'q',
            search: '/',
            sftp: 's',
            forwards: 'f',
            snippets: 'n',
            add: 'a',
            edit: 'e',
            delete: 'd',
            duplicate: 'y',
            pin: 'p',
            mark: ' ',
            ping: 'r',
            password: 'P',
            vault: 'V',
            help: '?',
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = home().join(".config/vssh/config.toml");
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("{}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_config_keeps_defaults() {
        let c: Config = toml::from_str(
            "ping_on_start = true\n[theme]\naccent = \"#ff8800\"\n[keys]\nleader = \"b\"\n",
        )
        .unwrap();
        assert!(c.ping_on_start);
        assert_eq!(c.theme.accent, Color::Rgb(255, 136, 0));
        assert_eq!(c.theme.ok, Color::Green);
        assert_eq!((c.keys.leader, c.keys.quit), ('b', 'q'));
    }

    #[test]
    fn rejects_typos_and_bad_colours() {
        assert!(toml::from_str::<Config>("[theme]\nacent = \"red\"\n").is_err());
        assert!(toml::from_str::<Config>("[theme]\naccent = \"notacolour\"\n").is_err());
        assert!(toml::from_str::<Config>("[keys]\nquit = \"qq\"\n").is_err());
    }
}
