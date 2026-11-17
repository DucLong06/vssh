//! `~/.config/vssh/config.toml`: theme and keybindings. Every field is optional.
//!
//! What a colour is depends on who draws it, so the theme's type is the
//! frontend's: [`Config<T>`] reads `[theme]` as whatever `T` is. Left at the
//! default it is the raw table.

use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, io};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

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

/// Language of the key hints, the leader menu and the help screen.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Vi,
}

impl Lang {
    /// `en` or `vi`, as written in the config, `--lang` and `VSSH_LANG`.
    pub fn parse(code: &str) -> Result<Self> {
        match code {
            "en" => Ok(Self::En),
            "vi" => Ok(Self::Vi),
            other => anyhow::bail!("unknown language {other:?}: use en or vi"),
        }
    }

    /// Picks the text for this language.
    pub fn pick<'a>(self, en: &'a str, vi: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::Vi => vi,
        }
    }
}

/// Which way panels are arranged. `Auto` follows the terminal's shape.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Auto,
    Landscape,
    Portrait,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config<T = toml::Table> {
    pub theme: T,
    pub keys: Keys,
    pub lang: Lang,
    /// `"auto"` (default), `"landscape"` or `"portrait"`.
    pub layout: Orientation,
    /// Log every session from the start. Off by default: a log holds
    /// everything the terminal showed, secrets included.
    pub log_sessions: bool,
    /// Login scripts: host alias to a command typed into the shell once it
    /// is up. (`RemoteCommand` in the ssh config replaces the shell instead.)
    pub exec: HashMap<String, String>,
    /// TCP-probe every host at startup. Off by default: it touches every
    /// machine in the ssh config on each launch.
    pub ping_on_start: bool,
    /// Load, memory and disk of the focused host in the status line. On by
    /// default; off, no session opens a second channel to ask for them.
    pub status_stats: bool,
    /// Round the badge ends with the Powerline half circles of a Nerd Font
    /// instead of the plain Unicode ones. Off by default: without such a
    /// font they show as empty boxes.
    pub nerd_font: bool,
    /// Slide a tab in over three frames when switching to it. On by default;
    /// turn it off on a slow link, where three redraws are three round trips.
    pub transitions: bool,
    /// Copy through the OSC 52 terminal escape. On by default; turn it off
    /// for a terminal that prints the sequence instead of obeying it.
    pub osc52: bool,
}

impl<T: Default> Default for Config<T> {
    fn default() -> Self {
        Self {
            theme: T::default(),
            keys: Keys::default(),
            lang: Lang::default(),
            layout: Orientation::default(),
            log_sessions: false,
            exec: HashMap::new(),
            ping_on_start: false,
            status_stats: true,
            nerd_font: false,
            transitions: true,
            osc52: true,
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
    /// Quick connect: a one-off session that is not saved.
    pub connect: char,
    /// Open a session on every host with a tag.
    pub connect_tag: char,
    /// Show the selected host's details, in windows too small to keep them on screen.
    pub info: char,
    pub forwards: char,
    pub snippets: char,
    /// Open the tag drawer: every tag and group with its host count.
    pub tags: char,
    pub add: char,
    pub edit: char,
    pub delete: char,
    pub duplicate: char,
    /// Copy `ssh <alias>` to the clipboard.
    pub copy: char,
    pub pin: char,
    pub mark: char,
    pub ping: char,
    /// Run one command on every marked host.
    pub batch: char,
    /// Quick peek: the selected host's uptime, load, memory and disk, without a session.
    pub peek: char,
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
            connect: 'c',
            connect_tag: 'T',
            info: 'i',
            forwards: 'f',
            snippets: 'n',
            tags: 'w',
            add: 'a',
            edit: 'e',
            delete: 'd',
            // Was `y` until `y` became copy; a config that says `y` still wins.
            duplicate: 'Y',
            copy: 'y',
            pin: 'p',
            mark: ' ',
            ping: 'r',
            batch: 'x',
            peek: 'v',
            password: 'P',
            vault: 'V',
            help: '?',
        }
    }
}

impl<T: Default + DeserializeOwned> Config<T> {
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
    fn login_scripts_and_logging() {
        let c: Config = toml::from_str(
            "log_sessions = true\n[exec]\nweb-01 = \"cd /srv/app && git status\"\n\"db.prod\" = \"psql\"\n",
        )
        .unwrap();
        assert!(c.log_sessions);
        assert_eq!(c.exec["web-01"], "cd /srv/app && git status");
        assert_eq!(c.exec["db.prod"], "psql");
        let none: Config = toml::from_str("").unwrap();
        assert!(!none.log_sessions && none.exec.is_empty());
    }

    #[test]
    fn status_bar_switches_default_to_on() {
        let none: Config = toml::from_str("").unwrap();
        assert!(none.status_stats && none.transitions && none.osc52 && !none.nerd_font);
        let set = "status_stats = false\nnerd_font = true\ntransitions = false\nosc52 = false";
        let set: Config = toml::from_str(set).unwrap();
        assert!(!set.status_stats && !set.transitions && !set.osc52 && set.nerd_font);
    }

    #[test]
    fn layout_override() {
        let layout = |text: &str| toml::from_str::<Config>(text).map(|c| c.layout);
        assert_eq!(layout("").unwrap(), Orientation::Auto);
        assert_eq!(
            layout("layout = \"portrait\"").unwrap(),
            Orientation::Portrait
        );
        assert_eq!(
            layout("layout = \"landscape\"").unwrap(),
            Orientation::Landscape
        );
        assert!(layout("layout = \"diagonal\"").is_err());
    }

    #[test]
    fn language_setting() {
        assert_eq!(toml::from_str::<Config>("").unwrap().lang, Lang::En);
        assert_eq!(
            toml::from_str::<Config>("lang = \"vi\"").unwrap().lang,
            Lang::Vi
        );
        assert!(toml::from_str::<Config>("lang = \"fr\"").is_err());
        assert_eq!(Lang::parse("vi").unwrap(), Lang::Vi);
        assert!(Lang::parse("vi_VN").is_err() && Lang::parse("").is_err());
        assert_eq!(
            (
                Lang::En.pick("quit", "thoát"),
                Lang::Vi.pick("quit", "thoát")
            ),
            ("quit", "thoát")
        );
    }
}
