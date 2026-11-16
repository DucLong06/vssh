//! Lossless `~/.ssh/config` model. Every line is kept verbatim and options are
//! parsed on demand, so comments, unknown options and `Match` blocks survive edits.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::{fmt, fs, io};

#[derive(Clone, Debug, PartialEq)]
struct Block {
    /// The `Host ...` / `Match ...` line; empty for the global section at the top.
    header: String,
    lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SshConfig {
    blocks: Vec<Block>,
}

/// Splits `Key value`, `Key=value` and `Key "value"`; `None` for blanks and comments.
fn kv(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    let rest = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(rest);
    Some((&line[..end], rest))
}

fn has_key(line: &str, key: &str) -> bool {
    kv(line).is_some_and(|(k, _)| k.eq_ignore_ascii_case(key))
}

/// OpenSSH host pattern match: `*` is any run of characters, `?` exactly one.
fn glob(pat: &[u8], s: &[u8]) -> bool {
    match pat.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => (0..=s.len()).any(|i| glob(rest, &s[i..])),
        Some((&c, rest)) => s
            .split_first()
            .is_some_and(|(&d, s)| (c == b'?' || c == d) && glob(rest, s)),
    }
}

impl Block {
    /// Host patterns; empty for the global section and `Match` blocks.
    fn patterns(&self) -> Vec<&str> {
        match kv(&self.header) {
            Some((k, v)) if k.eq_ignore_ascii_case("host") => v.split_whitespace().collect(),
            _ => vec![],
        }
    }

    fn applies_to(&self, alias: &str) -> bool {
        if self.header.is_empty() {
            return true;
        }
        let hit = |p: &str| glob(p.as_bytes(), alias.as_bytes());
        let pats = self.patterns();
        pats.iter().any(|p| !p.starts_with('!') && hit(p))
            && !pats.iter().any(|p| p.strip_prefix('!').is_some_and(hit))
    }

    fn values<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> {
        self.lines
            .iter()
            .filter_map(move |l| kv(l).filter(|(k, _)| k.eq_ignore_ascii_case(key)))
            .map(|(_, v)| v)
    }

    /// Appends an option after the last non-blank line, keeping trailing spacing.
    fn push(&mut self, key: &str, value: &str) {
        let at = self.lines.iter().rposition(|l| !l.trim().is_empty());
        self.lines
            .insert(at.map_or(0, |i| i + 1), format!("    {key} {value}"));
    }

    fn end_with_blank(&mut self) {
        let empty = self.header.is_empty() && self.lines.is_empty();
        if !empty && self.lines.last().is_none_or(|l| !l.trim().is_empty()) {
            self.lines.push(String::new());
        }
    }
}

impl fmt::Display for SshConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.blocks {
            if !b.header.is_empty() {
                writeln!(f, "{}", b.header)?;
            }
            for l in &b.lines {
                writeln!(f, "{l}")?;
            }
        }
        Ok(())
    }
}

impl SshConfig {
    pub fn parse(text: &str) -> Self {
        let mut blocks = vec![Block {
            header: String::new(),
            lines: vec![],
        }];
        for line in text.lines() {
            match blocks.last_mut() {
                Some(b) if !has_key(line, "host") && !has_key(line, "match") => {
                    b.lines.push(line.to_string())
                }
                _ => blocks.push(Block {
                    header: line.to_string(),
                    lines: vec![],
                }),
            }
        }
        Self { blocks }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::parse("")),
            Err(e) => Err(e),
        }
    }

    /// Atomic write (temp file + rename, mode 0600). The file as it was before
    /// vssh first touched it is kept as `config.vssh.bak`.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        // Resolve symlinks so a dotfiles-managed config is written through, not replaced.
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let bak = path.with_extension("vssh.bak");
        if path.exists() && !bak.exists() {
            fs::copy(&path, &bak)?;
        }
        let tmp = path.with_extension("vssh.tmp");
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(self.to_string().as_bytes())?;
        f.sync_all()?;
        fs::rename(tmp, path)
    }

    /// Every concrete (wildcard-free) host alias, in file order.
    pub fn hosts(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for p in self.blocks.iter().flat_map(Block::patterns) {
            if !p.contains(['*', '?', '!']) && !out.iter().any(|o| o == p) {
                out.push(p.to_string());
            }
        }
        out
    }

    /// Effective value the way OpenSSH resolves it: first match wins across
    /// the global section and every `Host` block whose patterns match.
    pub fn get<'a>(&'a self, alias: &str, key: &'a str) -> Option<&'a str> {
        self.blocks
            .iter()
            .filter(|b| b.applies_to(alias))
            .find_map(|b| b.values(key).next())
    }

    /// All effective values of a multi-valued option such as `IdentityFile`.
    pub fn get_all<'a>(&'a self, alias: &str, key: &'a str) -> Vec<&'a str> {
        self.blocks
            .iter()
            .filter(|b| b.applies_to(alias))
            .flat_map(|b| b.values(key))
            .collect()
    }

    fn owner(&self, alias: &str) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.patterns().contains(&alias))
    }

    /// Value set in the host's own block, ignoring inherited defaults.
    pub fn own<'a>(&'a self, alias: &str, key: &'a str) -> &'a str {
        self.own_all(alias, key).first().copied().unwrap_or("")
    }

    pub fn own_all<'a>(&'a self, alias: &str, key: &'a str) -> Vec<&'a str> {
        self.owner(alias)
            .map(|i| self.blocks[i].values(key).collect())
            .unwrap_or_default()
    }

    /// Sets a single-valued option in the host's own block; an empty value removes it.
    pub fn set(&mut self, alias: &str, key: &str, value: &str) {
        let Some(i) = self.owner(alias) else { return };
        let b = &mut self.blocks[i];
        match (b.lines.iter().position(|l| has_key(l, key)), value) {
            (Some(at), "") => drop(b.lines.remove(at)),
            (Some(at), _) => b.lines[at] = format!("    {key} {value}"),
            (None, "") => {}
            (None, _) => b.push(key, value),
        }
    }

    /// Adds one line of a multi-valued option (forwards).
    pub fn add(&mut self, alias: &str, key: &str, value: &str) {
        if let Some(i) = self.owner(alias) {
            self.blocks[i].push(key, value);
        }
    }

    /// Removes the line of a multi-valued option that has exactly this value.
    pub fn remove(&mut self, alias: &str, key: &str, value: &str) {
        if let Some(i) = self.owner(alias) {
            self.blocks[i]
                .lines
                .retain(|l| !kv(l).is_some_and(|(k, v)| k.eq_ignore_ascii_case(key) && v == value));
        }
    }

    /// Adds an empty block for the alias, ahead of the first wildcard block
    /// that matches it (typically a trailing `Host *`). OpenSSH keeps the
    /// first value it sees, so the host's own options must come before defaults.
    pub fn add_host(&mut self, alias: &str) {
        let wildcard = self.blocks.iter().skip(1).position(|b| b.applies_to(alias));
        let at = wildcard.map_or(self.blocks.len(), |i| i + 1);
        self.blocks[at - 1].end_with_blank();
        let lines = match wildcard {
            Some(_) => vec![String::new()],
            None => vec![],
        };
        let header = format!("Host {alias}");
        self.blocks.insert(at, Block { header, lines });
    }

    /// Drops the alias; the block goes with it unless other aliases share it.
    pub fn remove_host(&mut self, alias: &str) {
        let Some(i) = self.owner(alias) else { return };
        let rest: Vec<&str> = self.blocks[i]
            .patterns()
            .into_iter()
            .filter(|p| *p != alias)
            .collect();
        if rest.is_empty() {
            self.blocks.remove(i);
        } else {
            self.blocks[i].header = format!("Host {}", rest.join(" "));
        }
    }

    pub fn rename_host(&mut self, old: &str, new: &str) {
        let Some(i) = self.owner(old) else { return };
        let pats: Vec<&str> = self.blocks[i]
            .patterns()
            .into_iter()
            .map(|p| if p == old { new } else { p })
            .collect();
        self.blocks[i].header = format!("Host {}", pats.join(" "));
    }

    pub fn duplicate_host(&mut self, alias: &str, new: &str) {
        let Some(i) = self.owner(alias) else { return };
        self.blocks[i].end_with_blank();
        let mut copy = self.blocks[i].clone();
        copy.header = format!("Host {new}");
        self.blocks.insert(i + 1, copy);
    }
}

/// Config keys of the three forward kinds, tagged with `ssh`'s flag letter.
pub const FORWARD_KEYS: [(char, &str); 3] = [
    ('L', "LocalForward"),
    ('R', "RemoteForward"),
    ('D', "DynamicForward"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Forward {
    /// `L`, `R` or `D`.
    pub kind: char,
    pub bind: String,
    pub port: u16,
    /// Destination; unused for dynamic forwards.
    pub host: String,
    pub hport: u16,
}

fn unbracket(s: &str) -> &str {
    s.trim_start_matches('[').trim_end_matches(']')
}

impl Forward {
    /// Parses an ssh config value: `[bind_address:]port [host:hostport]`.
    pub fn parse(kind: char, value: &str) -> Option<Self> {
        let mut parts = value.split_whitespace();
        let listen = parts.next()?;
        let (bind, port) = match listen.rsplit_once(':') {
            Some((b, p)) => (unbracket(b), p),
            None => ("127.0.0.1", listen),
        };
        let bind = if bind == "*" { "0.0.0.0" } else { bind };
        let (host, hport) = match kind {
            'L' | 'R' => {
                let (h, p) = parts.next()?.rsplit_once(':')?;
                (unbracket(h), p.parse().ok()?)
            }
            'D' => ("", 0),
            _ => return None,
        };
        Some(Self {
            kind,
            bind: bind.to_string(),
            port: port.parse().ok()?,
            host: host.to_string(),
            hport,
        })
    }

    pub fn key(&self) -> &'static str {
        FORWARD_KEYS
            .iter()
            .find(|(k, _)| *k == self.kind)
            .map_or("", |(_, key)| key)
    }

    /// The value as written to the ssh config.
    pub fn value(&self) -> String {
        let wrap = |h: &str| match h.contains(':') {
            true => format!("[{h}]"),
            false => h.to_string(),
        };
        let listen = match self.bind.as_str() {
            "127.0.0.1" => self.port.to_string(),
            b => format!("{}:{}", wrap(b), self.port),
        };
        match self.kind {
            'D' => listen,
            _ => format!("{listen} {}:{}", wrap(&self.host), self.hport),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# global defaults
ServerAliveInterval 60

Host web1 web-alias
    HostName 10.0.0.1
    User deploy
    # keep this comment
    Port 2222
    IdentityFile ~/.ssh/web
    LocalForward 8080 localhost:80
    SomeFutureOption yes

Host db=ignored
Host db
\tHostName=db.internal
\tUser \"postgres\"

Match host *.corp exec \"true\"
    User matched

Host *.example.com !bad.example.com
    User wild
    Port 2200

Host *
    User fallback
    IdentityFile ~/.ssh/id_default
";

    #[test]
    fn round_trip_is_lossless() {
        assert_eq!(SshConfig::parse(SAMPLE).to_string(), SAMPLE);
        assert_eq!(SshConfig::parse("").to_string(), "");
    }

    #[test]
    fn lists_only_concrete_hosts() {
        let c = SshConfig::parse(SAMPLE);
        assert_eq!(c.hosts(), ["web1", "web-alias", "db=ignored", "db"]);
    }

    #[test]
    fn parses_separators_quotes_and_case() {
        let c = SshConfig::parse(SAMPLE);
        assert_eq!(c.get("db", "hostname"), Some("db.internal"));
        assert_eq!(c.get("db", "USER"), Some("postgres"));
        assert_eq!(c.get("web1", "Port"), Some("2222"));
        assert_eq!(c.get("web-alias", "HostName"), Some("10.0.0.1"));
    }

    #[test]
    fn resolves_like_openssh() {
        let c = SshConfig::parse(SAMPLE);
        // First match wins: own block beats `Host *`, globals beat everything.
        assert_eq!(c.get("web1", "User"), Some("deploy"));
        assert_eq!(c.get("web1", "ServerAliveInterval"), Some("60"));
        assert_eq!(c.get("a.example.com", "User"), Some("wild"));
        assert_eq!(c.get("a.example.com", "Port"), Some("2200"));
        // Negated pattern excludes the block; Match blocks are never applied.
        assert_eq!(c.get("bad.example.com", "User"), Some("fallback"));
        assert_eq!(c.get("x.corp", "User"), Some("fallback"));
        assert_eq!(c.get("web1", "Nope"), None);
        assert_eq!(
            c.get_all("web1", "IdentityFile"),
            ["~/.ssh/web", "~/.ssh/id_default"]
        );
        // `own` ignores inherited values.
        assert_eq!(c.own("db", "IdentityFile"), "");
    }

    #[test]
    fn glob_patterns() {
        let m = |p: &str, s: &str| glob(p.as_bytes(), s.as_bytes());
        assert!(m("*", "anything"));
        assert!(m("web?", "web1"));
        assert!(!m("web?", "web12"));
        assert!(m("*.example.*", "a.example.com"));
        assert!(!m("*.example.com", "example.com"));
    }

    #[test]
    fn set_replaces_adds_and_removes() {
        let mut c = SshConfig::parse(SAMPLE);
        c.set("web1", "port", "22");
        c.set("web1", "User", "");
        c.set("web1", "ProxyJump", "bastion");
        c.set("missing", "User", "nobody");
        let text = c.to_string();
        assert!(text.contains("    port 22\n"));
        assert!(!text.contains("deploy"));
        // New options land inside the block, before its trailing blank line.
        assert!(text.contains("    SomeFutureOption yes\n    ProxyJump bastion\n\nHost db=ignored"));
        assert!(text.contains("# keep this comment"));
        assert_eq!(c.get("web1", "User"), Some("fallback"));
    }

    #[test]
    fn host_lifecycle() {
        let mut c = SshConfig::parse(SAMPLE);
        c.add_host("new");
        c.set("new", "HostName", "1.2.3.4");
        c.set("new", "User", "me");
        // Lands before the matching `Host *`, so its own values win over the defaults.
        assert!(c
            .to_string()
            .contains("    Port 2200\n\nHost new\n    HostName 1.2.3.4\n    User me\n\nHost *\n"));
        assert_eq!(c.get("new", "User"), Some("me"));
        assert_eq!(c.get("new", "IdentityFile"), Some("~/.ssh/id_default"));

        c.rename_host("new", "newer");
        c.duplicate_host("newer", "copy");
        assert_eq!(c.get("copy", "HostName"), Some("1.2.3.4"));
        assert_eq!(c.own("new", "HostName"), "");

        // Removing one alias of a shared block keeps the block for the other.
        c.remove_host("web1");
        assert_eq!(c.get("web-alias", "User"), Some("deploy"));
        c.remove_host("web-alias");
        assert!(!c.to_string().contains("10.0.0.1"));
        assert_eq!(c.hosts(), ["db=ignored", "db", "newer", "copy"]);
    }

    #[test]
    fn first_host_in_empty_file() {
        let mut c = SshConfig::parse("");
        c.add_host("a");
        c.set("a", "User", "me");
        assert_eq!(c.to_string(), "Host a\n    User me\n");
    }

    #[test]
    fn forwards_parse_and_format() {
        let f = Forward::parse('L', "8080 localhost:80").unwrap();
        assert_eq!((f.bind.as_str(), f.port), ("127.0.0.1", 8080));
        assert_eq!((f.host.as_str(), f.hport), ("localhost", 80));
        assert_eq!(
            (f.key(), f.value().as_str()),
            ("LocalForward", "8080 localhost:80")
        );

        let f = Forward::parse('R', "*:9000 [::1]:3000").unwrap();
        assert_eq!((f.bind.as_str(), f.host.as_str()), ("0.0.0.0", "::1"));
        assert_eq!(f.value(), "0.0.0.0:9000 [::1]:3000");

        let f = Forward::parse('D', "1080").unwrap();
        assert_eq!((f.key(), f.value().as_str()), ("DynamicForward", "1080"));

        for bad in ["", "abc", "8080", "8080 nohostport", "99999 a:1"] {
            assert_eq!(Forward::parse('L', bad), None, "{bad}");
        }
        assert_eq!(Forward::parse('X', "1 a:1"), None);
    }

    #[test]
    fn multi_valued_add_remove() {
        let mut c = SshConfig::parse(SAMPLE);
        c.add("web1", "LocalForward", "9090 localhost:90");
        assert_eq!(
            c.own_all("web1", "LocalForward"),
            ["8080 localhost:80", "9090 localhost:90"]
        );
        c.remove("web1", "localforward", "8080 localhost:80");
        assert_eq!(c.own_all("web1", "LocalForward"), ["9090 localhost:90"]);
    }

    #[test]
    fn save_backs_up_once_and_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vssh-cfg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config");
        fs::write(&path, "Host a\n").unwrap();

        let mut c = SshConfig::load(&path).unwrap();
        c.set("a", "User", "me");
        c.save(&path).unwrap();
        c.set("a", "User", "you");
        c.save(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "Host a\n    User you\n");
        assert_eq!(
            fs::read_to_string(dir.join("config.vssh.bak")).unwrap(),
            "Host a\n"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(SshConfig::load(&dir.join("absent"))
            .unwrap()
            .hosts()
            .is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
