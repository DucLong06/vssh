//! The TUI application: state, event handling and the main loop.

mod draw;
mod files;
mod panes;
mod stats;
mod term;

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{self, stdout, Write};
use std::mem;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event as Input, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::widgets::ListState;
use zeroize::Zeroizing;

use crate::theme::Config;
use files::SftpView;
use panes::{Layout, Shape};
use vssh_core::config::{data_dir, private_dir, private_file, ssh_config_path, Lang};
use vssh_core::conn_err::Report;
use vssh_core::ssh::{Login, SessionCmd, Target};
use vssh_core::stats::{Peek, Stats};
use vssh_core::store::db::{Db, Meta, Snippet};
use vssh_core::store::ssh_config::{Forward, SshConfig, FORWARD_KEYS};
use vssh_core::task::{self, Task};
use vssh_core::vault::Vault;

/// Everything that can wake the UI: the keyboard, and what the background
/// work the app asked `vssh_core::task` for has to report.
pub enum Event {
    Input(Input),
    Task(task::Event),
}

enum Kind {
    Term {
        parser: Box<vt100::Parser<term::Replies>>,
        tx: task::Sender<SessionCmd>,
    },
    Sftp(Box<SftpView>),
}

struct Tab {
    id: u64,
    /// Tab title: the host alias, or `user@host` for a quick connect.
    alias: String,
    target: Target,
    /// False for a quick connect: no stats, nothing stored.
    saved: bool,
    kind: Kind,
    task: Task,
    /// Set once the first byte or listing arrives.
    connected: bool,
    closed: bool,
    /// Session log, while logging is on.
    log: Option<File>,
}

/// What to do with a submitted form or a confirmed question.
enum After {
    /// Original alias when editing, `None` when adding.
    SaveHost(Option<String>),
    DeleteHost(String),
    /// Retry a connection with a typed password.
    Password {
        alias: String,
        target: Target,
        saved: bool,
        sftp: bool,
    },
    /// Connect to a typed `user@host[:port]` without saving it.
    QuickConnect,
    /// Open a session on every host that has the typed tag or group.
    ConnectTag,
    /// Unlock, then optionally go on to set this host's password.
    VaultUnlock(Option<String>),
    /// Same, for a new vault; the second field is the first passphrase entry.
    VaultCreate(Option<String>, Option<Zeroizing<String>>),
    VaultSet(String),
    AddForward(String),
    /// Run the typed command on these hosts.
    Batch(Vec<String>),
    SaveSnippet(i64),
    DeleteSnippet(i64),
    SftpMkdir,
    SftpRename(String),
    SftpDelete,
    SftpCopy,
    Quit,
}

struct Form {
    title: String,
    fields: Vec<(&'static str, Zeroizing<String>)>,
    idx: usize,
    /// The last field is a secret, shown as dots.
    masked: bool,
    /// A "save this host" checkbox below the fields, and whether it is ticked.
    /// It is one more row for `idx`.
    save: Option<bool>,
    then: After,
}

impl Form {
    /// Rows the cursor can be on: the fields, plus the checkbox if there is one.
    fn rows(&self) -> usize {
        self.fields.len() + usize::from(self.save.is_some())
    }
}

/// What one host of a batch answered: `None` while it is still running.
type Outcome = Option<Result<String, Report>>;

/// One command run on several hosts at once, and what came back so far.
struct Batch {
    id: u64,
    command: String,
    /// In the order the hosts were asked, not the order they answered.
    rows: Vec<(String, Outcome)>,
    scroll: u16,
    /// Only hosts whose name contains this are shown.
    filter: String,
    /// The filter is being typed.
    typing: bool,
    tasks: Vec<Task>,
}

/// Closing the results gives up on the hosts that have not answered.
impl Drop for Batch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// What a line of batch results is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Row {
    Running,
    Out,
    Failed,
}

/// The lines a batch shows: every line of every host's output, each with its
/// host, hosts in the order they were asked. A host still running, one that
/// said nothing and one that failed get one line each, so none goes missing.
fn batch_lines(rows: &[(String, Outcome)], filter: &str, lang: Lang) -> Vec<(String, String, Row)> {
    let mut lines = vec![];
    for (host, outcome) in rows.iter().filter(|(host, _)| host.contains(filter)) {
        let mut line = |text: &str, row| lines.push((host.clone(), text.to_string(), row));
        match outcome {
            None => line(lang.pick("running…", "đang chạy…"), Row::Running),
            Some(Err(fail)) => {
                let text = format!("✗ {} {}", fail.class.label(lang), fail.message);
                line(&text, Row::Failed);
            }
            Some(Ok(out)) if out.trim().is_empty() => {
                line(lang.pick("(no output)", "(không có kết quả)"), Row::Out)
            }
            Some(Ok(out)) => out.lines().for_each(|l| line(l, Row::Out)),
        }
    }
    lines
}

enum Popup {
    Form(Form),
    Batch(Batch),
    Confirm(String, After),
    Text {
        title: String,
        body: String,
        scroll: u16,
    },
    /// Quick peek at a host, by alias; what it shows is in `App::peeks`.
    Peek(String),
    /// Choose a snippet (name, command) to type into the session.
    Pick {
        items: Vec<(String, String)>,
        sel: usize,
    },
}

/// Where a leader sequence is: not started, after the leader, or in the
/// window menu (`leader w`).
#[derive(Clone, Copy, Default, PartialEq)]
enum Leader {
    #[default]
    Off,
    Root,
    Window,
}

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    List,
    Search,
    Forwards,
    Snippets,
    /// The tag drawer.
    Tags,
}

pub struct App {
    cfg: Config,
    tx: Sender<Event>,
    sshc: SshConfig,
    db: Db,
    meta: HashMap<String, Meta>,
    snippets: Vec<Snippet>,
    vault: Option<Vault>,
    /// Hosts after filtering and sorting.
    hosts: Vec<String>,
    sel: usize,
    list: ListState,
    filter: String,
    focus: Focus,
    /// Selection inside the forwards / snippets panel or the tag drawer.
    sub_sel: usize,
    /// What narrows the tag drawer, and whether it is being typed.
    tag_query: (String, bool),
    marked: BTreeSet<String>,
    reach: HashMap<String, Option<u128>>,
    /// Why the last connection to a host failed, until one succeeds.
    fails: HashMap<String, Report>,
    /// Latest server metrics, by tab title or host alias. `None`: it was
    /// asked and had no answer, so the host list does not ask it again.
    stats: HashMap<String, Option<Stats>>,
    /// Quick peeks by host alias: when the answer came and what it was.
    /// `None` while the host is being asked.
    peeks: HashMap<String, Option<Peeked>>,
    /// The host-list selection whose metrics were last asked for, and the
    /// task asking; replaced, and the task aborted, when the selection moves.
    probe: (Option<String>, Option<Task>),
    /// Running forwards by key, with whether they are listening yet.
    fwds: HashMap<String, (Task, bool)>,
    tabs: Vec<Tab>,
    /// 0 is the host list, `i` is `tabs[i - 1]`.
    active: usize,
    next_id: u64,
    leader: Leader,
    /// Typing in one session goes to every live session.
    broadcast: bool,
    /// How many sessions the session view shows side by side.
    layout: Layout,
    /// Index, among the terminal sessions, of the first one shown.
    window: usize,
    /// How the last frame was arranged; follows the terminal size.
    shape: Shape,
    /// Details of the selected host are up, in a window too small to keep them on screen.
    info: bool,
    /// Columns and rows available to a terminal tab.
    body: (u16, u16),
    /// Frames left of the slide that follows a tab switch.
    slide: u16,
    popup: Option<Popup>,
    /// A password waiting for the vault to be unlocked, with its alias.
    pending: Option<(String, Zeroizing<String>)>,
    msg: String,
    quit: bool,
}

/// The answer to a quick peek, and when it came.
type Peeked = (Instant, Result<Peek, Report>);

fn vault_path() -> PathBuf {
    data_dir().join("vault")
}

fn fwd_key(alias: &str, kind: char, value: &str) -> String {
    format!("{alias} {kind} {value}")
}

/// Strips control characters from text that came from a remote machine, so
/// it cannot smuggle escape sequences into our terminal.
fn clean(s: &str) -> String {
    s.replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

/// `#web` in the search box: true if a tag or the group starts with `web`
/// (case-insensitive). A bare `#` matches every tagged or grouped host.
fn has_tag(meta: &Meta, query: &str) -> bool {
    let query = query.to_lowercase();
    meta.tags
        .split([' ', ','])
        .chain([meta.grp.as_str()])
        .any(|t| !t.is_empty() && t.to_lowercase().starts_with(&query))
}

/// Every tag and group of these hosts with the number of hosts `#name`
/// shows: the same prefix match as the search box, so the count in the
/// drawer is the length of the list you get. Most used first, then by name;
/// names that differ only in case are one entry, in the first spelling seen.
fn tag_counts(hosts: &[String], meta: &HashMap<String, Meta>) -> Vec<(String, usize)> {
    let metas: Vec<&Meta> = hosts.iter().filter_map(|h| meta.get(h)).collect();
    let mut names: Vec<&str> = vec![];
    for m in &metas {
        for name in m.tags.split([' ', ',']).chain([m.grp.as_str()]) {
            if !name.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                names.push(name);
            }
        }
    }
    let count = |name: &str| metas.iter().filter(|m| has_tag(m, name)).count();
    let mut counts: Vec<(String, usize)> = names
        .into_iter()
        .map(|name| (name.to_string(), count(name)))
        .collect();
    counts.sort_by(|(a, na), (b, nb)| {
        nb.cmp(na)
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });
    counts
}

/// Opens the log file of a session: `<dir>/<alias>-<stamp>.log`, private to
/// the user. The alias is reduced to file-name characters, so a host called
/// `../x` cannot write outside the log directory.
fn start_log(dir: &Path, alias: &str, stamp: &str) -> io::Result<(File, PathBuf)> {
    private_dir(dir)?;
    let safe = |c: char| c.is_ascii_alphanumeric() || "._@-".contains(c);
    let name: String = alias
        .chars()
        .map(|c| if safe(c) { c } else { '_' })
        .collect();
    let path = dir.join(format!("{name}-{stamp}.log"));
    let file = private_file().append(true).create(true).open(&path)?;
    Ok((file, path))
}

/// Moves a waiting password into the vault once there is an unlocked one,
/// and returns the alias it was stored under. Until then it keeps waiting.
fn store_waiting(
    vault: Option<&mut Vault>,
    pending: &mut Option<(String, Zeroizing<String>)>,
) -> Result<Option<String>> {
    let Some(vault) = vault else {
        return Ok(None);
    };
    let Some((alias, mut password)) = pending.take() else {
        return Ok(None);
    };
    vault.set(&alias, &mut password)?;
    Ok(Some(alias))
}

/// Exact, case-insensitive match on a tag or the group: what "connect
/// everything tagged web" means, unlike the prefix match of the search box.
fn tag_is(meta: &Meta, tag: &str) -> bool {
    meta.tags
        .split([' ', ','])
        .chain([meta.grp.as_str()])
        .any(|t| !t.is_empty() && t.eq_ignore_ascii_case(tag))
}

/// Something typed or pasted into a terminal session.
enum TermInput<'a> {
    Key(KeyEvent),
    Paste(&'a str),
}

/// Sends input to the focused session, or to every live session when
/// broadcasting. Each session encodes it for its own terminal modes, so
/// arrow keys stay right for a pane that happens to be inside vim.
/// Returns how many sessions received it.
fn send_input(tabs: &mut [Tab], focused: usize, broadcast: bool, input: &TermInput) -> usize {
    let mut sent = 0;
    for (i, tab) in tabs.iter_mut().enumerate() {
        let Kind::Term { parser, tx } = &mut tab.kind else {
            continue;
        };
        if tab.closed || !(broadcast || i == focused) {
            continue;
        }
        parser.screen_mut().set_scrollback(0);
        let screen = parser.screen();
        let bytes = match input {
            TermInput::Key(k) => term::encode_key(*k, screen.application_cursor()),
            TermInput::Paste(text) if screen.bracketed_paste() => {
                format!("\x1b[200~{text}\x1b[201~").into_bytes()
            }
            TermInput::Paste(text) => text.replace('\n', "\r").into_bytes(),
        };
        sent += usize::from(tx.send(SessionCmd::Data(bytes)).is_ok());
    }
    sent
}

impl App {
    /// Where background work reports: back into the event loop.
    fn sink(&self) -> impl task::Sink {
        let tx = self.tx.clone();
        move |ev| drop(tx.send(Event::Task(ev)))
    }

    fn new(cfg: Config, tx: Sender<Event>) -> Result<Self> {
        let sshc = SshConfig::load(&ssh_config_path()).context("cannot read ~/.ssh/config")?;
        private_dir(&data_dir())?;
        let db = Db::open(&data_dir().join("vssh.db"))?;
        Self::with(cfg, tx, sshc, db)
    }

    /// The app over an already loaded ssh config and database.
    fn with(cfg: Config, tx: Sender<Event>, sshc: SshConfig, db: Db) -> Result<Self> {
        db.import(&sshc.hosts())?;
        let mut app = Self {
            cfg,
            tx,
            sshc,
            db,
            meta: HashMap::new(),
            snippets: vec![],
            vault: None,
            hosts: vec![],
            sel: 0,
            list: ListState::default(),
            filter: String::new(),
            focus: Focus::List,
            sub_sel: 0,
            tag_query: (String::new(), false),
            marked: BTreeSet::new(),
            reach: HashMap::new(),
            fails: HashMap::new(),
            stats: HashMap::new(),
            peeks: HashMap::new(),
            probe: (None, None),
            fwds: HashMap::new(),
            tabs: vec![],
            active: 0,
            next_id: 1,
            leader: Leader::Off,
            broadcast: false,
            layout: Layout::Single,
            window: 0,
            shape: Shape::Wide,
            info: false,
            body: (80, 24),
            slide: 0,
            popup: None,
            pending: None,
            msg: String::new(),
            quit: false,
        };
        app.reload()?;
        if app.cfg.ping_on_start {
            app.ping_all();
        }
        Ok(app)
    }

    // ── data ────────────────────────────────────────────────────────────

    fn reload(&mut self) -> Result<()> {
        self.meta = self.db.metas()?;
        self.snippets = self.db.snippets()?;
        self.refilter();
        Ok(())
    }

    /// Writes `~/.ssh/config`; if that fails, drops the unsaved edit.
    fn save_config(&mut self) -> Result<()> {
        if let Err(e) = self.sshc.save(&ssh_config_path()) {
            self.sshc = SshConfig::load(&ssh_config_path())?;
            self.refilter();
            return Err(e).context("cannot write ~/.ssh/config");
        }
        self.reload()
    }

    fn refilter(&mut self) {
        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(i64, String)> = self
            .sshc
            .hosts()
            .into_iter()
            .filter_map(|a| {
                if self.filter.is_empty() {
                    return Some((0, a));
                }
                let m = self.meta.get(&a);
                if let Some(tag) = self.filter.strip_prefix('#') {
                    return m.is_some_and(|m| has_tag(m, tag)).then_some((0, a));
                }
                let hay = format!(
                    "{a} {} {} {} {}",
                    self.sshc.get(&a, "HostName").unwrap_or(""),
                    self.sshc.get(&a, "User").unwrap_or(""),
                    m.map_or("", |m| &m.grp),
                    m.map_or("", |m| &m.tags),
                );
                matcher.fuzzy_match(&hay, &self.filter).map(|s| (s, a))
            })
            .collect();
        // Best match first; otherwise pinned, then grouped, then by name.
        let rank = |a: &String| {
            let m = self.meta.get(a);
            let grp = m.map_or("", |m| m.grp.as_str());
            (
                !m.is_some_and(|m| m.pinned),
                grp.is_empty(),
                grp.to_string(),
            )
        };
        scored.sort_by(|(sa, a), (sb, b)| {
            sb.cmp(sa)
                .then_with(|| rank(a).cmp(&rank(b)))
                .then_with(|| a.cmp(b))
        });
        self.hosts = scored.into_iter().map(|(_, a)| a).collect();
        self.sel = self.sel.min(self.hosts.len().saturating_sub(1));
    }

    fn alias(&self) -> Option<String> {
        self.hosts.get(self.sel).cloned()
    }

    fn forwards(&self, alias: &str) -> Vec<(char, String)> {
        FORWARD_KEYS
            .iter()
            .flat_map(|(kind, key)| {
                let values = self.sshc.own_all(alias, key);
                values.into_iter().map(|v| (*kind, v.to_string()))
            })
            .collect()
    }

    /// What the tag drawer lists: every tag with its count, narrowed by the
    /// drawer's own search.
    fn drawer_tags(&self) -> Vec<(String, usize)> {
        let query = self.tag_query.0.to_lowercase();
        let mut tags = tag_counts(&self.sshc.hosts(), &self.meta);
        tags.retain(|(name, _)| name.to_lowercase().contains(&query));
        tags
    }

    /// Keys of the tag drawer. Picking a tag is the search `#tag`.
    fn tags_key(&mut self, k: KeyEvent) {
        let tags = self.drawer_tags();
        let (query, typing) = &mut self.tag_query;
        let pick = match k.code {
            // While the drawer's search is typed every key belongs to it.
            KeyCode::Esc if *typing => {
                query.clear();
                *typing = false;
                return self.sub_sel = 0;
            }
            KeyCode::Enter if *typing => return *typing = false,
            KeyCode::Backspace | KeyCode::Char(_) if *typing => {
                match k.code {
                    KeyCode::Char(c) => query.push(c),
                    _ => drop(query.pop()),
                }
                return self.sub_sel = 0;
            }
            KeyCode::Char('/') => return *typing = true,
            KeyCode::Down | KeyCode::Char('j') => {
                return self.sub_sel = (self.sub_sel + 1).min(tags.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                return self.sub_sel = self.sub_sel.saturating_sub(1)
            }
            KeyCode::Char(c @ '1'..='9') => tags.get(c as usize - '1' as usize),
            KeyCode::Enter => tags.get(self.sub_sel),
            KeyCode::Char('0') => None,
            // Esc clears the filter on its way out; the other ways out keep it.
            KeyCode::Esc => {
                self.focus = Focus::List;
                None
            }
            KeyCode::Left | KeyCode::Char('q' | 'h') => return self.focus = Focus::List,
            KeyCode::Char(c) if c == self.cfg.keys.tags => return self.focus = Focus::List,
            _ => return,
        };
        self.filter = pick.map(|(tag, _)| format!("#{tag}")).unwrap_or_default();
        self.sel = 0;
        self.refilter();
    }

    /// Global snippets plus the ones bound to this host.
    fn host_snippets(&self, alias: &str) -> Vec<&Snippet> {
        self.snippets
            .iter()
            .filter(|s| s.host.is_empty() || s.host == alias)
            .collect()
    }

    fn vault_password(&self, alias: &str) -> Option<Zeroizing<String>> {
        let stored = self.vault.as_ref()?.get(alias)?;
        Some(Zeroizing::new(stored.to_string()))
    }

    fn check(&mut self, res: Result<()>) {
        if let Err(e) = res {
            self.msg = format!("{e:#}");
        }
    }

    fn sftp_view(&mut self) -> Option<&mut SftpView> {
        match &mut self.tabs.get_mut(self.active.checked_sub(1)?)?.kind {
            Kind::Sftp(view) => Some(view),
            Kind::Term { .. } => None,
        }
    }

    // ── background work ─────────────────────────────────────────────────

    /// Opens a shell or SFTP tab for a host from the ssh config.
    fn open(&mut self, alias: &str, sftp: bool) {
        match Target::from_config(&self.sshc, alias) {
            Ok(target) => {
                let password = self.vault_password(alias);
                self.open_target(alias, target, true, sftp, password);
            }
            Err(e) => self.msg = e.to_string(),
        }
    }

    /// Opens one session per host and shows them together.
    fn open_many(&mut self, aliases: &[String]) {
        let first = self.tabs.len();
        for alias in aliases {
            self.open(alias, false);
        }
        if self.tabs.len() > first {
            self.active = first + 1;
            self.window = self.terms().len() - (self.tabs.len() - first);
        }
        // Side by side, unless the layout already has room.
        let fit = Layout::fit(self.tabs.len() - first);
        if fit.panes() > self.layout.panes() {
            self.layout = fit;
        }
    }

    /// Tab indices of the terminal sessions, in tab order.
    fn terms(&self) -> Vec<usize> {
        let is_term = |i: &usize| matches!(self.tabs[*i].kind, Kind::Term { .. });
        (0..self.tabs.len()).filter(is_term).collect()
    }

    /// Tab indices of the sessions shown together with the active one.
    fn shown(&mut self) -> Vec<usize> {
        let terms = self.terms();
        let active = self.active.checked_sub(1);
        let Some(at) = terms.iter().position(|i| Some(*i) == active) else {
            return vec![];
        };
        let size = self.layout.panes();
        self.window = panes::window(self.window, at, size, terms.len());
        terms[self.window..(self.window + size).min(terms.len())].to_vec()
    }

    /// Moves focus to the pane in a vim direction, if there is one.
    fn focus_pane(&mut self, direction: char) {
        let shown = self.shown();
        let slot = shown.iter().position(|i| i + 1 == self.active);
        let stacked = self.shape == Shape::Tall;
        let to = slot.and_then(|s| self.layout.neighbor(s, direction, shown.len(), stacked));
        if let Some(to) = to {
            self.active = shown[to] + 1;
        }
    }

    /// Closes the active tab. From a session the view stays with the sessions:
    /// the one that slides into its place, else the one before it.
    fn close_pane(&mut self) {
        let Some(i) = self.active.checked_sub(1) else {
            return;
        };
        let is_term = |t: &Tab| matches!(t.kind, Kind::Term { .. });
        if !is_term(&self.close_tab(i)) {
            return;
        }
        let after = (i..self.tabs.len()).find(|j| is_term(&self.tabs[*j]));
        let before = (0..i).rev().find(|j| is_term(&self.tabs[*j]));
        self.active = after.or(before).map_or(0, |j| j + 1);
    }

    fn scroll(&mut self, by: isize) {
        let kind = self.active.checked_sub(1).map(|i| &mut self.tabs[i].kind);
        if let Some(Kind::Term { parser, .. }) = kind {
            let at = parser.screen().scrollback().saturating_add_signed(by);
            parser.screen_mut().set_scrollback(at);
        }
    }

    /// Puts text on the clipboard of the terminal vssh runs in, and says so.
    fn copy(&mut self, text: &str) {
        let lang = self.cfg.lang;
        if text.is_empty() {
            return self.msg = lang.pick("nothing to copy", "không có gì để chép").into();
        }
        term::copy(text, self.cfg.osc52);
        self.msg = match text.lines().count() {
            1 => format!("{}: {text}", lang.pick("copied", "đã chép")),
            n => format!(
                "{} {n} {}",
                lang.pick("copied", "đã chép"),
                lang.pick("lines", "dòng")
            ),
        };
    }

    /// Copies from the active tab: the last command's output of a session,
    /// the path of the selected file in SFTP.
    fn copy_active(&mut self) {
        let Some(i) = self.active.checked_sub(1) else {
            return;
        };
        let text = match &self.tabs[i].kind {
            Kind::Term { parser, .. } => term::last_output(parser.screen()),
            Kind::Sftp(view) => view.selected().map_or(String::new(), |e| {
                format!("{}/{}", view.dirs[view.side].trim_end_matches('/'), e.name)
            }),
        };
        self.copy(&text);
    }

    /// Number of sessions that would receive typed input right now.
    fn live_sessions(&self) -> usize {
        let live = |t: &&Tab| matches!(t.kind, Kind::Term { .. }) && !t.closed;
        self.tabs.iter().filter(live).count()
    }

    fn toggle_broadcast(&mut self) {
        self.broadcast ^= true;
        self.msg = match self.broadcast {
            true => format!(
                "BROADCAST ON: typing goes to all {} live session(s)",
                self.live_sessions()
            ),
            false => "broadcast off".into(),
        };
    }

    /// Opens a tab; the connection runs in its own task.
    fn open_target(
        &mut self,
        alias: &str,
        target: Target,
        saved: bool,
        sftp: bool,
        password: Option<Zeroizing<String>>,
    ) {
        let (id, sink, size) = (self.next_id, self.sink(), self.body);
        let tab_target = target.clone();
        self.next_id += 1;
        // Login scripts belong to saved hosts; a quick connect runs none.
        let login = Login {
            command: target.remote_command.clone(),
            script: self.cfg.exec.get(alias).filter(|_| saved).cloned(),
        };
        let (kind, task) = if sftp {
            let (task, cmd_tx) = task::sftp(id, target, password, sink);
            (Kind::Sftp(Box::new(SftpView::new(cmd_tx))), task)
        } else {
            let stats = self.cfg.status_stats.then(|| alias.to_string());
            let (task, cmd_tx) = task::session(id, target, password, size, login, stats, sink);
            let rows = NonZeroU16::new(size.1).unwrap_or(NonZeroU16::MIN);
            let cols = NonZeroU16::new(size.0).unwrap_or(NonZeroU16::MIN);
            let replies = term::Replies::default();
            let parser = vt100::Parser::new_with_callbacks(rows, cols, 5000, replies);
            let mut parser = Box::new(parser);
            let connecting = self.cfg.lang.pick("Connecting to", "Đang kết nối tới");
            parser.process(format!("{connecting} {alias}…\r\n").as_bytes());
            (Kind::Term { parser, tx: cmd_tx }, task)
        };
        self.tabs.push(Tab {
            id,
            alias: alias.to_string(),
            target: tab_target,
            saved,
            kind,
            task,
            connected: false,
            closed: false,
            log: None,
        });
        self.active = self.tabs.len();
        if self.cfg.log_sessions && !sftp {
            self.toggle_log();
        }
    }

    /// Starts or stops logging the active session to a file.
    fn toggle_log(&mut self) {
        let Some(i) = self.active.checked_sub(1) else {
            return;
        };
        let Kind::Term { parser, .. } = &self.tabs[i].kind else {
            return;
        };
        // What is on screen now goes in first, then everything that follows.
        let screen = parser.screen().contents();
        if self.tabs[i].log.take().is_some() {
            return self.msg = "session log closed".into();
        }
        let alias = self.tabs[i].alias.clone();
        let res = (|| {
            let stamp = self.db.timestamp()?;
            let (mut file, path) = start_log(&data_dir().join("logs"), &alias, &stamp)?;
            writeln!(
                file,
                "# vssh session log: {alias}, started {stamp}\n{screen}"
            )?;
            anyhow::Ok((file, path))
        })();
        match res {
            Ok((file, path)) => {
                self.tabs[i].log = Some(file);
                self.msg = format!(
                    "LOGGING to {}: everything shown is written as is, secrets included",
                    path.display()
                );
            }
            Err(e) => self.msg = format!("cannot start the session log: {e:#}"),
        }
    }

    /// Offers the snippets that apply to the active session, to type one in.
    fn pick_snippet(&mut self) {
        let Some(i) = self.active.checked_sub(1) else {
            return;
        };
        if !matches!(self.tabs[i].kind, Kind::Term { .. }) {
            return;
        }
        let snippets = self.host_snippets(&self.tabs[i].alias);
        let items: Vec<_> = snippets
            .into_iter()
            .map(|s| (s.name.clone(), s.command.clone()))
            .collect();
        match items.is_empty() {
            true => self.msg = "no snippets yet: add some with n on the host list".into(),
            false => self.popup = Some(Popup::Pick { items, sel: 0 }),
        }
    }

    /// Removes a tab. The view stays on the tab it was on; if that is the one
    /// going away it returns to the host list, never into another live shell.
    fn close_tab(&mut self, i: usize) -> Tab {
        let tab = self.tabs.remove(i);
        tab.task.abort();
        self.stats.remove(&tab.alias);
        self.active = match self.active {
            a if a == i + 1 => 0,
            a if a > i + 1 => a - 1,
            a => a,
        };
        tab
    }

    fn ping_all(&mut self) {
        for alias in self.sshc.hosts() {
            let Ok(t) = Target::from_config(&self.sshc, &alias) else {
                continue;
            };
            task::ping(alias, t, self.sink());
        }
        self.msg = "checking reachability…".into();
    }

    /// On the host list, asks the selected host for its metrics once the
    /// selection has rested on it. A host with a live session is left to that
    /// session. Called once per frame; does nothing until the selection (or
    /// the tab) changes.
    fn probe_selected(&mut self) {
        let on_list = self.active == 0 && self.cfg.status_stats;
        let alias = self.alias().filter(|_| on_list);
        if alias == self.probe.0 {
            return;
        }
        if let Some(task) = self.probe.1.take() {
            task.abort();
        }
        self.probe.0 = alias.clone();
        let Some(alias) = alias else { return };
        let live = |t: &Tab| t.alias == alias && !t.closed && matches!(t.kind, Kind::Term { .. });
        // Not a host that is down, and not one that already refused: every
        // refused probe is a failed login in that server's log.
        let refused = self.stats.get(&alias) == Some(&None);
        if self.tabs.iter().any(live) || refused || self.reach.get(&alias) == Some(&None) {
            return;
        }
        let Ok(target) = Target::from_config(&self.sshc, &alias) else {
            return;
        };
        self.probe.1 = Some(task::probe(alias, target, self.sink()));
    }

    /// Opens the quick peek for a host. The answer is asked for over a
    /// connection of its own (same host key rules and login as a session)
    /// unless one from the last 30 seconds is there, or `refresh` says so.
    fn peek(&mut self, alias: &str, refresh: bool) {
        self.popup = Some(Popup::Peek(alias.to_string()));
        let keep = match self.peeks.get(alias) {
            Some(Some((at, _))) => !refresh && at.elapsed() < Duration::from_secs(30),
            // Already asking: one request at a time.
            Some(None) => true,
            None => false,
        };
        if keep {
            return;
        }
        self.peeks.insert(alias.to_string(), None);
        let target = Target::from_config(&self.sshc, alias);
        let password = self.vault_password(alias);
        task::peek(alias.to_string(), target, password, self.sink());
    }

    /// True while something on screen is waiting for an answer and shows a
    /// spinner: the main loop then redraws on a timer.
    fn waiting(&self) -> bool {
        match &self.popup {
            Some(Popup::Peek(alias)) => matches!(self.peeks.get(alias), Some(None)),
            Some(Popup::Batch(batch)) => batch.rows.iter().any(|row| row.1.is_none()),
            _ => false,
        }
    }

    fn toggle_forward(&mut self, alias: &str, kind: char, value: &str) -> Result<()> {
        let key = fwd_key(alias, kind, value);
        if let Some((task, _)) = self.fwds.remove(&key) {
            task.abort();
            return Ok(());
        }
        let fwd = Forward::parse(kind, value).context("cannot parse this forward")?;
        let target = Target::from_config(&self.sshc, alias)?;
        let password = self.vault_password(alias);
        let task = task::forward(key.clone(), target, password, fwd, self.sink());
        self.fwds.insert(key, (task, false));
        Ok(())
    }

    /// Runs a snippet on its own host, or on every marked host if it is global.
    fn run_snippet(&mut self, s: &Snippet, alias: &str) {
        let hosts: Vec<String> = if s.host.is_empty() && !self.marked.is_empty() {
            self.marked.iter().cloned().collect()
        } else {
            vec![alias.to_string()]
        };
        self.batch(hosts, &s.command);
    }

    /// Runs one command on every host, each over its own connection, all at
    /// once, and opens the results: they fill in as the hosts answer.
    // shortcut: no cap on how many connect at once; add a semaphore if
    // batches grow to hundreds of hosts.
    fn batch(&mut self, hosts: Vec<String>, command: &str) {
        let id = self.next_id;
        self.next_id += 1;
        let tasks = hosts.iter().map(|host| {
            let target = Target::from_config(&self.sshc, host);
            let password = self.vault_password(host);
            task::batch(
                id,
                host.clone(),
                target,
                password,
                command.to_string(),
                self.sink(),
            )
        });
        let tasks = tasks.collect();
        self.popup = Some(Popup::Batch(Batch {
            id,
            command: command.to_string(),
            rows: hosts.into_iter().map(|host| (host, None)).collect(),
            scroll: 0,
            filter: String::new(),
            typing: false,
            tasks,
        }));
    }

    // ── events ──────────────────────────────────────────────────────────

    fn on_event(&mut self, ev: Event) {
        match ev {
            Event::Input(Input::Key(k)) if k.kind == KeyEventKind::Press => {
                self.msg.clear();
                self.on_key(k);
            }
            Event::Input(Input::Paste(text)) => self.on_paste(&text),
            // Resize and focus changes only need the redraw that follows.
            Event::Input(_) => {}
            Event::Task(ev) => self.on_task(ev),
        }
    }

    fn on_task(&mut self, ev: task::Event) {
        let tab_index = |tabs: &[Tab], id| tabs.iter().position(|t| t.id == id);
        match ev {
            task::Event::Data(id, data) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                self.mark_connected(i);
                if let Kind::Term { parser, tx } = &mut self.tabs[i].kind {
                    parser.process(&data);
                    let reply = mem::take(&mut parser.callbacks_mut().0);
                    if !reply.is_empty() {
                        let _ = tx.send(SessionCmd::Data(reply));
                    }
                }
                // A log that cannot be written is closed, loudly, not half kept.
                let tab = &mut self.tabs[i];
                if let Some(Err(e)) = tab.log.as_mut().map(|log| log.write_all(&data)) {
                    tab.log = None;
                    self.msg = format!("session log of {} stopped: {e}", tab.alias);
                }
            }
            task::Event::Sftp(id, reply) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                self.mark_connected(i);
                if let Kind::Sftp(view) = &mut self.tabs[i].kind {
                    let msg = view.on_reply(reply);
                    if !msg.is_empty() {
                        self.msg = msg;
                    }
                }
            }
            task::Event::Closed(id, err) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                let lang = self.cfg.lang;
                let report = err.map(|e| e.report(lang));
                let why = match report {
                    // It never got through: there is no session to keep on
                    // screen, only the reason to show.
                    Some(report) if !self.tabs[i].connected => {
                        let tab = self.close_tab(i);
                        return self.failed(&tab.alias, report);
                    }
                    Some(report) => report.message,
                    None => lang
                        .pick("connection closed", "kết nối đã đóng")
                        .to_string(),
                };
                self.tabs[i].closed = true;
                self.stats.remove(&self.tabs[i].alias);
                match &mut self.tabs[i].kind {
                    Kind::Term { parser, .. } => {
                        let leader = self.cfg.keys.leader;
                        let why = clean(&why);
                        let note =
                            format!("\r\n\x1b[0;33m[{why}] Ctrl-{leader} x closes this tab\x1b[0m");
                        parser.process(note.as_bytes());
                    }
                    Kind::Sftp(_) => self.msg = why,
                }
            }
            task::Event::AuthFailed(id, err) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                let tab = self.close_tab(i);
                let report = err.report(self.cfg.lang);
                let then = After::Password {
                    sftp: matches!(tab.kind, Kind::Sftp(_)),
                    alias: tab.alias.clone(),
                    target: tab.target.clone(),
                    saved: tab.saved,
                };
                // The title says what was refused: a key, a password, or nothing to offer.
                self.prompt(&report.message, "Password", true, then);
                self.fails.insert(tab.alias, report);
            }
            task::Event::Ping(alias, ms) => {
                self.reach.insert(alias, ms);
            }
            task::Event::Stats(alias, stats) => {
                self.stats.insert(alias, stats);
            }
            task::Event::Peek(alias, res) => {
                if let Ok(peek) = &res {
                    // It answered: the status line can use the numbers too.
                    self.fails.remove(&alias);
                    if peek.stats != Stats::default() {
                        self.stats.insert(alias.clone(), Some(peek.stats.clone()));
                    }
                }
                let res = res.map_err(|e| e.report(self.cfg.lang));
                self.peeks.insert(alias, Some((Instant::now(), res)));
            }
            task::Event::Forward(key, Ok(true)) => {
                if let Some(state) = self.fwds.get_mut(&key) {
                    state.1 = true;
                }
            }
            task::Event::Forward(key, res) => {
                self.fwds.remove(&key);
                let why = match res {
                    Err(e) => e.report(self.cfg.lang).message,
                    Ok(_) => "stopped".into(),
                };
                self.msg = format!("forward {key}: {why}");
            }
            task::Event::Batch(id, host, out) => {
                // An answer to a batch that was closed has nowhere to go.
                let Some(Popup::Batch(batch)) = &mut self.popup else {
                    return;
                };
                let out = out.map(|o| clean(&o));
                let out = out.map_err(|e| e.report(self.cfg.lang));
                let row = batch.rows.iter_mut().find(|(h, _)| *h == host);
                if let Some(row) = row.filter(|_| batch.id == id) {
                    row.1 = Some(out);
                }
            }
        }
    }

    /// A connection that never got through: remembered for the host's
    /// details, and shown at once with the full chain. Several failures (a
    /// tag of hosts opened together) share one popup; a form or a question
    /// that is open is not replaced, the message line carries it instead.
    fn failed(&mut self, alias: &str, report: Report) {
        let lang = self.cfg.lang;
        let heading = lang.pick("! Connection failed", "! Kết nối thất bại");
        let mut block = format!(
            "{alias}  [{}]\n  {}\n",
            report.class.label(lang),
            clean(&report.message)
        );
        if report.chain != report.message {
            block.push_str(&format!("  {}\n", clean(&report.chain)));
        }
        match &mut self.popup {
            Some(Popup::Text { title, body, .. }) if title == heading => {
                body.push_str(&format!("\n{block}"));
            }
            Some(_) => self.msg = format!("{alias}: {}", report.message),
            None => {
                self.popup = Some(Popup::Text {
                    title: heading.into(),
                    body: block,
                    scroll: 0,
                })
            }
        }
        self.fails.insert(alias.to_string(), report);
    }

    /// Records a successful connection the first time a tab hears back.
    fn mark_connected(&mut self, i: usize) {
        if mem::replace(&mut self.tabs[i].connected, true) {
            return;
        }
        self.fails.remove(&self.tabs[i].alias);
        if self.tabs[i].saved {
            let res = self.db.touch(&self.tabs[i].alias);
            let res = res.and_then(|()| self.db.metas().map(|m| self.meta = m));
            self.check(res.map_err(Into::into));
        }
    }

    fn on_paste(&mut self, text: &str) {
        match &mut self.popup {
            Some(Popup::Form(f)) => {
                // Nothing to paste into when the cursor is on the save checkbox.
                if let Some((_, field)) = f.fields.get_mut(f.idx) {
                    field.push_str(&text.replace(['\r', '\n'], " "));
                }
                return;
            }
            Some(_) => return,
            None => {}
        }
        if self.active == 0 {
            if self.focus == Focus::Search {
                self.filter.push_str(text.trim());
                self.sel = 0;
                self.refilter();
            }
        } else {
            let input = TermInput::Paste(text);
            send_input(&mut self.tabs, self.active - 1, self.broadcast, &input);
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        if let Some(popup) = self.popup.take() {
            return self.popup_key(popup, k);
        }
        if k.modifiers == KeyModifiers::ALT {
            match k.code {
                KeyCode::Char('b') => return self.toggle_broadcast(),
                // Window navigation, only while there is more than one pane:
                // with a single pane these keys still reach the shell.
                KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l')) if self.shown().len() > 1 => {
                    return self.focus_pane(c);
                }
                _ => {}
            }
        }
        let is_leader =
            k.modifiers == KeyModifiers::CONTROL && k.code == KeyCode::Char(self.cfg.keys.leader);
        match mem::take(&mut self.leader) {
            Leader::Root => return self.leader_key(k, is_leader),
            Leader::Window => return self.window_key(k),
            Leader::Off => {}
        }
        if is_leader {
            return self.leader = Leader::Root;
        }
        if self.active == 0 {
            return self.hosts_key(k);
        }
        let i = self.active - 1;
        match self.tabs[i].kind {
            // A dead session swallows keys (`send_input` skips it). Closing on
            // "any key" would let the rest of a half-typed command run as
            // host-list commands.
            Kind::Term { .. } => {
                send_input(&mut self.tabs, i, self.broadcast, &TermInput::Key(k));
            }
            Kind::Sftp(_) => self.sftp_key(k),
        }
    }

    fn cycle(&mut self, forward: bool) {
        let n = self.tabs.len() + 1;
        self.active = (self.active + if forward { 1 } else { n - 1 }) % n;
    }

    /// Tab and pane control, reachable from every tab. Pressing the leader
    /// twice sends it through to the remote shell.
    fn leader_key(&mut self, k: KeyEvent, again: bool) {
        let ctrl = k.modifiers == KeyModifiers::CONTROL;
        match k.code {
            _ if again && self.active > 0 => {
                let input = TermInput::Key(k);
                send_input(&mut self.tabs, self.active - 1, self.broadcast, &input);
            }
            KeyCode::PageUp => self.scroll(10),
            KeyCode::PageDown => self.scroll(-10),
            KeyCode::Char('u') if ctrl => self.scroll(10),
            KeyCode::Char('d') if ctrl => self.scroll(-10),
            KeyCode::Left => self.focus_pane('h'),
            KeyCode::Down => self.focus_pane('j'),
            KeyCode::Up => self.focus_pane('k'),
            KeyCode::Right => self.focus_pane('l'),
            KeyCode::Char('L') => {
                self.layout = self.layout.next();
                self.msg = format!("layout: {}", self.layout.name(self.shape == Shape::Tall));
            }
            KeyCode::Char('b') => self.toggle_broadcast(),
            KeyCode::Char('w') => self.leader = Leader::Window,
            KeyCode::Char('l') => self.toggle_log(),
            KeyCode::Char('s') => self.pick_snippet(),
            KeyCode::Char('y') => self.copy_active(),
            KeyCode::Tab | KeyCode::Char('n') => self.cycle(true),
            KeyCode::BackTab | KeyCode::Char('p') => self.cycle(false),
            KeyCode::Char('q') => self.active = 0,
            KeyCode::Char('x' | 'd') => self.close_pane(),
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i <= self.tabs.len() {
                    self.active = i;
                }
            }
            _ => {}
        }
    }

    /// The window menu, `leader w`: focus, layout presets, close.
    fn window_key(&mut self, k: KeyEvent) {
        let layout = match k.code {
            KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l')) => return self.focus_pane(c),
            KeyCode::Left => return self.focus_pane('h'),
            KeyCode::Down => return self.focus_pane('j'),
            KeyCode::Up => return self.focus_pane('k'),
            KeyCode::Right => return self.focus_pane('l'),
            KeyCode::Char('d' | 'x') => return self.close_pane(),
            KeyCode::Char('s') => Layout::Single,
            KeyCode::Char('v') => Layout::Columns2,
            KeyCode::Char('g') => Layout::Grid,
            KeyCode::Char('t') => Layout::Columns3,
            KeyCode::Char('L') => self.layout.next(),
            _ => return,
        };
        self.layout = layout;
        self.msg = format!("layout: {}", layout.name(self.shape == Shape::Tall));
    }

    fn ask_quit(&mut self) {
        let (tabs, fwds) = (self.tabs.len(), self.fwds.len());
        if tabs + fwds == 0 {
            return self.quit = true;
        }
        let text = format!("Quit? This closes {tabs} tab(s) and {fwds} forward(s).");
        self.popup = Some(Popup::Confirm(text, After::Quit));
    }

    fn move_sel(&mut self, delta: isize) {
        let last = self.hosts.len().saturating_sub(1);
        self.sel = self.sel.saturating_add_signed(delta).min(last);
    }

    fn hosts_key(&mut self, k: KeyEvent) {
        match self.focus {
            Focus::Search => return self.search_key(k),
            Focus::Forwards | Focus::Snippets => return self.panel_key(k),
            Focus::Tags => return self.tags_key(k),
            Focus::List => {}
        }
        let keys = &self.cfg.keys;
        if k.modifiers == KeyModifiers::CONTROL {
            // Half a screen of two-line rows, like Ctrl-d / Ctrl-u in vim.
            let half = (self.body.1.saturating_sub(2) / 4).max(1) as isize;
            return match k.code {
                KeyCode::Char('d') => self.move_sel(half),
                KeyCode::Char('u') => self.move_sel(-half),
                KeyCode::Char('c') => self.ask_quit(),
                // Never let Ctrl-d fall through to `d` (delete).
                _ => {}
            };
        }
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::PageDown => self.move_sel(10),
            KeyCode::PageUp => self.move_sel(-10),
            KeyCode::Home | KeyCode::Char('g') => self.sel = 0,
            KeyCode::End | KeyCode::Char('G') => self.move_sel(isize::MAX),
            KeyCode::Tab => self.cycle(true),
            KeyCode::BackTab => self.cycle(false),
            KeyCode::Esc if self.info => self.info = false,
            KeyCode::Esc => {
                self.filter.clear();
                self.refilter();
            }
            KeyCode::Char(c) if c == keys.info => self.info ^= true,
            KeyCode::Char(c) if c == keys.quit || c == 'Q' => self.ask_quit(),
            KeyCode::Char(c) if c == keys.search => self.focus = Focus::Search,
            KeyCode::Char(c) if c == keys.add => self.host_form(None),
            KeyCode::Char(c) if c == keys.connect => {
                let lang = self.cfg.lang;
                let fields = vec![
                    ("user@host[:port]", String::new()),
                    (
                        lang.pick("Password (optional)", "Mật khẩu (nếu cần)"),
                        String::new(),
                    ),
                ];
                let title = lang.pick("Quick connect", "Kết nối nhanh");
                self.form(title, fields, true, After::QuickConnect);
                if let Some(Popup::Form(form)) = &mut self.popup {
                    form.save = Some(false);
                }
            }
            KeyCode::Enter if !self.marked.is_empty() => {
                let marked: Vec<String> = mem::take(&mut self.marked).into_iter().collect();
                self.open_many(&marked);
            }
            KeyCode::Char(c) if c == keys.connect_tag => {
                // Offer the tag being filtered on, else the selected host's first tag or group.
                let meta = self.alias().and_then(|a| self.meta.get(&a).cloned());
                let meta = meta.unwrap_or_default();
                let own = meta.tags.split([' ', ',']).chain([meta.grp.as_str()]);
                let own = own.into_iter().find(|t| !t.is_empty()).unwrap_or("");
                let tag = self.filter.strip_prefix('#').unwrap_or(own).to_string();
                let lang = self.cfg.lang;
                let fields = vec![(lang.pick("Tag or group", "Thẻ hoặc nhóm"), tag)];
                let title = lang.pick("Connect to every host with", "Kết nối mọi máy chủ có");
                self.form(title, fields, false, After::ConnectTag);
            }
            KeyCode::Char(c) if c == keys.help => self.help(),
            KeyCode::Char(c) if c == keys.tags => {
                (self.focus, self.sub_sel) = (Focus::Tags, 0);
                self.tag_query = (String::new(), false);
            }
            KeyCode::Char(c) if c == keys.ping => self.ping_all(),
            KeyCode::Char(c) if c == keys.peek => {
                if let Some(alias) = self.alias() {
                    self.peek(&alias, false);
                }
            }
            KeyCode::Char(c) if c == keys.vault => {
                if self.vault.take().is_some() {
                    self.msg = "vault locked".into();
                } else {
                    self.unlock_prompt(None);
                }
            }
            code => {
                if let Some(alias) = self.alias() {
                    let res = self.host_key(code, &alias);
                    self.check(res);
                }
            }
        }
    }

    /// Actions on the selected host.
    fn host_key(&mut self, code: KeyCode, alias: &str) -> Result<()> {
        let keys = &self.cfg.keys;
        let KeyCode::Char(c) = code else {
            if code == KeyCode::Enter {
                self.open(alias, false);
            }
            return Ok(());
        };
        if c == keys.sftp {
            self.open(alias, true);
        } else if c == keys.edit {
            self.host_form(Some(alias));
        } else if c == keys.delete {
            let text = format!("Delete host {alias} from ~/.ssh/config?");
            self.popup = Some(Popup::Confirm(text, After::DeleteHost(alias.into())));
        } else if c == keys.duplicate {
            let mut copy = format!("{alias}-copy");
            while self.sshc.hosts().contains(&copy) {
                copy.push_str("-copy");
            }
            self.sshc.duplicate_host(alias, &copy);
            self.save_config()?;
            let mut meta = self.meta.get(alias).cloned().unwrap_or_default();
            meta.pinned = false;
            self.db.set_meta(&copy, &meta)?;
            self.reload()?;
            self.msg = format!("created {copy}");
        } else if c == keys.copy {
            self.copy(&format!("ssh {alias}"));
        } else if c == keys.batch {
            // Every marked host, or the selected one when nothing is marked.
            let hosts: Vec<String> = match self.marked.is_empty() {
                true => vec![alias.to_string()],
                false => self.marked.iter().cloned().collect(),
            };
            let lang = self.cfg.lang;
            let title = match lang {
                Lang::En => format!("Run on {} host(s)", hosts.len()),
                Lang::Vi => format!("Chạy trên {} máy", hosts.len()),
            };
            let label = lang.pick("Command", "Lệnh");
            self.prompt(&title, label, false, After::Batch(hosts));
        } else if c == keys.pin {
            let mut meta = self.meta.get(alias).cloned().unwrap_or_default();
            meta.pinned ^= true;
            self.db.set_meta(alias, &meta)?;
            self.reload()?;
            // Keep the cursor on the host that just moved.
            self.sel = self.hosts.iter().position(|h| h == alias).unwrap_or(0);
        } else if c == keys.mark {
            if !self.marked.remove(alias) {
                self.marked.insert(alias.into());
            }
            self.move_sel(1);
        } else if c == keys.password {
            match self.vault {
                Some(_) => self.password_prompt(alias),
                None => self.unlock_prompt(Some(alias.into())),
            }
        } else if c == keys.forwards {
            (self.focus, self.sub_sel) = (Focus::Forwards, 0);
        } else if c == keys.snippets {
            (self.focus, self.sub_sel) = (Focus::Snippets, 0);
        }
        Ok(())
    }

    fn search_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.focus = Focus::List;
            }
            KeyCode::Enter => self.focus = Focus::List,
            KeyCode::Down => self.move_sel(1),
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Backspace | KeyCode::Char(_) => {
                match k.code {
                    KeyCode::Char(c) => self.filter.push(c),
                    _ => drop(self.filter.pop()),
                }
                // A new query puts the cursor on its best match.
                self.sel = 0;
            }
            _ => {}
        }
        self.refilter();
    }

    /// Keys of the forwards and snippets panels.
    fn panel_key(&mut self, k: KeyEvent) {
        let Some(alias) = self.alias() else {
            return self.focus = Focus::List;
        };
        let forwards = self.forwards(&alias);
        let snippets: Vec<Snippet> = self.host_snippets(&alias).into_iter().cloned().collect();
        let on_forwards = self.focus == Focus::Forwards;
        let len = if on_forwards {
            forwards.len()
        } else {
            snippets.len()
        };
        let forward = forwards.get(self.sub_sel).filter(|_| on_forwards);
        let snippet = snippets.get(self.sub_sel).filter(|_| !on_forwards);
        let res = match k.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('q' | 'h') => {
                self.focus = Focus::List;
                Ok(())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.sub_sel = (self.sub_sel + 1).min(len.saturating_sub(1));
                Ok(())
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.sub_sel = self.sub_sel.saturating_sub(1);
                Ok(())
            }
            KeyCode::Char('a') if on_forwards => {
                let hint = "L|R|D [bind:]port [host:port]";
                self.prompt("Add forward", hint, false, After::AddForward(alias));
                Ok(())
            }
            KeyCode::Char('a') => {
                self.snippet_form(None);
                Ok(())
            }
            KeyCode::Char('e') => {
                if let Some(s) = snippet {
                    self.snippet_form(Some(s));
                }
                Ok(())
            }
            KeyCode::Enter | KeyCode::Char(' ') => match (forward, snippet) {
                (Some((kind, value)), _) => self.toggle_forward(&alias, *kind, value),
                (_, Some(s)) => {
                    self.run_snippet(s, &alias);
                    Ok(())
                }
                _ => Ok(()),
            },
            KeyCode::Char('d') => match (forward, snippet) {
                (Some((kind, value)), _) => {
                    if let Some((task, _)) = self.fwds.remove(&fwd_key(&alias, *kind, value)) {
                        task.abort();
                    }
                    let key = FORWARD_KEYS.iter().find(|(k, _)| k == kind);
                    self.sshc.remove(&alias, key.map_or("", |k| k.1), value);
                    self.sub_sel = self.sub_sel.saturating_sub(1);
                    self.save_config()
                }
                (_, Some(s)) => {
                    let text = format!("Delete snippet {}?", s.name);
                    self.popup = Some(Popup::Confirm(text, After::DeleteSnippet(s.id)));
                    Ok(())
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        };
        self.check(res);
    }

    fn sftp_key(&mut self, k: KeyEvent) {
        let Some(view) = self.sftp_view() else { return };
        let selected = view.selected().cloned();
        let msg = match (k.code, selected) {
            (KeyCode::Down | KeyCode::Char('j'), _) => return view.move_sel(1),
            (KeyCode::Up | KeyCode::Char('k'), _) => return view.move_sel(-1),
            (KeyCode::PageDown, _) => return view.move_sel(10),
            (KeyCode::PageUp, _) => return view.move_sel(-10),
            (KeyCode::Home | KeyCode::Char('g'), _) => return view.move_sel(isize::MIN),
            (KeyCode::End | KeyCode::Char('G'), _) => return view.move_sel(isize::MAX),
            (KeyCode::Tab, _) => return view.side ^= 1,
            (KeyCode::Enter | KeyCode::Right | KeyCode::Char('l'), _) => view.enter(),
            (KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h'), _) => view.up(),
            (KeyCode::Char('R'), _) => view.refresh(),
            (KeyCode::Char('y'), _) => return self.copy_active(),
            (KeyCode::Char('q'), _) => return self.close_pane(),
            (KeyCode::Char('Q'), _) => return self.ask_quit(),
            (KeyCode::Char('m'), _) => {
                return self.prompt("New directory", "Name", false, After::SftpMkdir);
            }
            (KeyCode::Char('c'), Some(e)) if view.progress.is_some() => {
                format!("a transfer is still running; {} not copied", e.name)
            }
            // Never overwrite silently.
            (KeyCode::Char('c'), Some(e)) if !e.is_dir && view.other_has(&e.name) => {
                let text = format!("{} exists on the other side. Overwrite?", clean(&e.name));
                return self.popup = Some(Popup::Confirm(text, After::SftpCopy));
            }
            (KeyCode::Char('c'), Some(_)) => view.copy(),
            (KeyCode::Char('r'), Some(e)) => {
                let fields = vec![("New name", e.name.clone())];
                return self.form("Rename", fields, false, After::SftpRename(e.name));
            }
            (KeyCode::Char('d'), Some(e)) => {
                let text = format!("Delete {}?", clean(&e.name));
                return self.popup = Some(Popup::Confirm(text, After::SftpDelete));
            }
            _ => return,
        };
        self.msg = msg;
    }

    // ── popups ──────────────────────────────────────────────────────────

    fn form(
        &mut self,
        title: &str,
        fields: Vec<(&'static str, String)>,
        masked: bool,
        then: After,
    ) {
        let fields = fields.into_iter().map(|(l, v)| (l, Zeroizing::new(v)));
        self.popup = Some(Popup::Form(Form {
            title: title.to_string(),
            fields: fields.collect(),
            idx: 0,
            masked,
            save: None,
            then,
        }));
    }

    fn prompt(&mut self, title: &str, label: &'static str, masked: bool, then: After) {
        self.form(title, vec![(label, String::new())], masked, then);
    }

    fn host_form(&mut self, alias: Option<&str>) {
        let own = |key| alias.map_or("", |a| self.sshc.own(a, key)).to_string();
        let meta = alias.and_then(|a| self.meta.get(a)).cloned();
        let meta = meta.unwrap_or_default();
        let fields = vec![
            ("Alias", alias.unwrap_or_default().to_string()),
            ("HostName", own("HostName")),
            ("User", own("User")),
            ("Port", own("Port")),
            ("IdentityFile", own("IdentityFile")),
            ("Group", meta.grp),
            ("Tags", meta.tags),
            ("Notes", meta.notes),
        ];
        let title = if alias.is_some() {
            "Edit host"
        } else {
            "Add host"
        };
        self.form(title, fields, false, After::SaveHost(alias.map(Into::into)));
    }

    fn snippet_form(&mut self, s: Option<&Snippet>) {
        let field = |f: fn(&Snippet) -> &String| s.map(f).cloned().unwrap_or_default();
        let fields = vec![
            ("Name", field(|s| &s.name)),
            ("Command", field(|s| &s.command)),
            ("Host (empty = all)", field(|s| &s.host)),
        ];
        let then = After::SaveSnippet(s.map_or(0, |s| s.id));
        self.form("Snippet", fields, false, then);
    }

    /// Puts a password into the vault under an alias. A locked vault is
    /// unlocked (or created) first; the password waits in `pending` meanwhile
    /// and is dropped, zeroized, if that prompt is cancelled.
    fn store_password(&mut self, alias: String, password: Zeroizing<String>) {
        self.pending = Some((alias, password));
        match self.vault {
            Some(_) => {
                let res = self.store_pending();
                self.check(res);
            }
            None => self.unlock_prompt(None),
        }
    }

    fn store_pending(&mut self) -> Result<()> {
        if let Some(alias) = store_waiting(self.vault.as_mut(), &mut self.pending)? {
            self.msg = format!("saved as {alias}, password stored in the vault");
        }
        Ok(())
    }

    fn unlock_prompt(&mut self, next: Option<String>) {
        if vault_path().exists() {
            let then = After::VaultUnlock(next);
            self.prompt("Unlock vault", "Master passphrase", true, then);
        } else {
            let then = After::VaultCreate(next, None);
            self.prompt("Create vault", "New master passphrase", true, then);
        }
    }

    fn password_prompt(&mut self, alias: &str) {
        let title = format!("Vault password for {alias}");
        let then = After::VaultSet(alias.to_string());
        self.prompt(&title, "Password (empty removes)", true, then);
    }

    fn help(&mut self) {
        let k = &self.cfg.keys;
        let en = format!(
            "HOST LIST\n\
             \x20 j/k ↑/↓      move              g/G     first / last\n\
             \x20 Ctrl-d/u     half a screen     Tab     next tab\n\
             \x20 Enter        open shell (on every marked host, if any are marked)\n\
             \x20 Space        mark host         {tag}       connect to every host with a tag\n\
             \x20 {sftp}            SFTP browser      {connect}       quick connect (Ctrl-s in the form saves it)\n\
             \x20 {search}            search (fuzzy)    Esc     clear search\n\
             \x20              search #tag shows only hosts with that tag or group\n\
             \x20 {add}            add host          {edit}       edit host\n\
             \x20 {delete}            delete host       {duplicate}       duplicate host\n\
             \x20 {copy}            copy `ssh <host>` to the clipboard (OSC 52)\n\
             \x20 {pin}            pin / unpin       {ping}       check reachability\n\
             \x20 {peek}            quick peek: uptime, load, memory, disk, without a session\n\
             \x20 {batch}            run one command on every marked host (/ filters the results)\n\
             \x20 {forwards}            port forwards     {snippets}       snippets\n\
             \x20 {tags}            tag drawer: 1-9 filter by a tag, 0 or Esc clear, / search tags\n\
             \x20 {password}            store password    {vault}       unlock / lock vault\n\
             \x20 {quit}            quit              Q       quit (from SFTP too)\n\n\
             FORWARDS / SNIPPETS PANEL\n\
             \x20 a add   e edit   d delete   Enter start/stop or run   Esc back\n\
             \x20 forward syntax:  L 8080 localhost:80 | R 9000 localhost:3000 | D 1080\n\n\
             SESSIONS (leader = Ctrl-{leader}; a menu of these keys appears after it)\n\
             \x20 Alt-h/j/k/l      focus pane (also leader + arrow, leader w h/j/k/l)\n\
             \x20 leader L         cycle layout      leader w      window menu:\n\
             \x20                  s single, v 2 columns, g 2x2 grid, t 3 columns\n\
             \x20 leader b, Alt-b  broadcast: typing goes to every live session\n\
             \x20 leader l         log this session to a file (everything shown, as is)\n\
             \x20 leader s         type a snippet into the session\n\
             \x20 leader y         copy the last command's output to the clipboard\n\
             \x20 leader d / x     close pane        leader q      back to the host list\n\
             \x20 leader Tab/n     next tab          leader p      previous tab\n\
             \x20 leader 1-9       go to tab         leader PgUp/PgDn  scroll (also Ctrl-u/d)\n\
             \x20 leader leader    send Ctrl-{leader} to the remote\n\n\
             SFTP\n\
             \x20 Tab switch pane   Enter/l open dir   h/Backspace parent\n\
             \x20 c copy to other pane   m mkdir   r rename   d delete\n\
             \x20 y copy the path of the selected file to the clipboard\n\
             \x20 R refresh   q close   Q quit\n",
            tag = k.connect_tag,
            sftp = k.sftp,
            connect = k.connect,
            search = k.search,
            add = k.add,
            edit = k.edit,
            delete = k.delete,
            duplicate = k.duplicate,
            copy = k.copy,
            pin = k.pin,
            ping = k.ping,
            peek = k.peek,
            batch = k.batch,
            forwards = k.forwards,
            snippets = k.snippets,
            tags = k.tags,
            password = k.password,
            vault = k.vault,
            quit = k.quit,
            leader = k.leader,
        );
        let vi = format!(
            "DANH SÁCH MÁY CHỦ\n\
             \x20 j/k ↑/↓      di chuyển         g/G     về đầu / về cuối\n\
             \x20 Ctrl-d/u     nửa màn hình      Tab     tab kế tiếp\n\
             \x20 Enter        mở shell (trên mọi máy đã đánh dấu, nếu có)\n\
             \x20 Space        đánh dấu máy      {tag}       kết nối mọi máy có cùng thẻ\n\
             \x20 {sftp}            trình duyệt SFTP  {connect}       kết nối nhanh (Ctrl-s trong biểu mẫu để lưu)\n\
             \x20 {search}            tìm (mờ)          Esc     xoá tìm kiếm\n\
             \x20              tìm #tag chỉ hiện máy có thẻ hoặc nhóm đó\n\
             \x20 {add}            thêm máy          {edit}       sửa máy\n\
             \x20 {delete}            xoá máy           {duplicate}       nhân bản máy\n\
             \x20 {copy}            chép `ssh <máy>` vào clipboard (OSC 52)\n\
             \x20 {pin}            ghim / bỏ ghim    {ping}       kiểm tra kết nối\n\
             \x20 {peek}            xem nhanh: uptime, tải, bộ nhớ, đĩa, không cần mở phiên\n\
             \x20 {batch}            chạy một lệnh trên mọi máy đã đánh dấu (/ để lọc kết quả)\n\
             \x20 {forwards}            chuyển tiếp cổng  {snippets}       snippet\n\
             \x20 {tags}            ngăn thẻ: 1-9 lọc theo thẻ, 0 hoặc Esc bỏ lọc, / tìm thẻ\n\
             \x20 {password}            lưu mật khẩu      {vault}       mở / khoá kho mật khẩu\n\
             \x20 {quit}            thoát             Q       thoát (cả từ SFTP)\n\n\
             KHUNG CHUYỂN TIẾP CỔNG / SNIPPET\n\
             \x20 a thêm   e sửa   d xoá   Enter bật/tắt hoặc chạy   Esc quay lại\n\
             \x20 cú pháp:  L 8080 localhost:80 | R 9000 localhost:3000 | D 1080\n\n\
             PHIÊN LÀM VIỆC (leader = Ctrl-{leader}; nhấn xong sẽ hiện menu các phím này)\n\
             \x20 Alt-h/j/k/l      chuyển khung (hoặc leader + mũi tên, leader w h/j/k/l)\n\
             \x20 leader L         đổi bố cục        leader w      menu cửa sổ:\n\
             \x20                  s một khung, v 2 cột, g lưới 2x2, t 3 cột\n\
             \x20 leader b, Alt-b  phát: gõ một lần, gửi tới mọi phiên đang mở\n\
             \x20 leader l         ghi log phiên này ra file (mọi thứ hiện ra, nguyên văn)\n\
             \x20 leader s         gõ một snippet vào phiên\n\
             \x20 leader y         chép kết quả của lệnh cuối vào clipboard\n\
             \x20 leader d / x     đóng khung        leader q      về danh sách máy chủ\n\
             \x20 leader Tab/n     tab kế tiếp       leader p      tab trước\n\
             \x20 leader 1-9       tới tab số        leader PgUp/PgDn  cuộn (hoặc Ctrl-u/d)\n\
             \x20 leader leader    gửi Ctrl-{leader} tới máy từ xa\n\n\
             SFTP\n\
             \x20 Tab đổi khung   Enter/l vào thư mục   h/Backspace thư mục cha\n\
             \x20 c chép sang khung kia   m tạo thư mục   r đổi tên   d xoá\n\
             \x20 y chép đường dẫn của file đang chọn vào clipboard\n\
             \x20 R tải lại   q đóng   Q thoát\n",
            tag = k.connect_tag,
            sftp = k.sftp,
            connect = k.connect,
            search = k.search,
            add = k.add,
            edit = k.edit,
            delete = k.delete,
            duplicate = k.duplicate,
            copy = k.copy,
            pin = k.pin,
            ping = k.ping,
            peek = k.peek,
            batch = k.batch,
            forwards = k.forwards,
            snippets = k.snippets,
            tags = k.tags,
            password = k.password,
            vault = k.vault,
            quit = k.quit,
            leader = k.leader,
        );
        let lang = self.cfg.lang;
        self.popup = Some(Popup::Text {
            title: lang.pick("Help", "Trợ giúp").into(),
            body: lang.pick(&en, &vi).to_string(),
            scroll: 0,
        });
    }

    fn popup_key(&mut self, popup: Popup, k: KeyEvent) {
        match (popup, k.code) {
            (Popup::Confirm(_, then), KeyCode::Char('y')) => {
                let res = self.confirmed(then);
                self.check(res);
            }
            (Popup::Form(mut form), KeyCode::Enter) => {
                // On failure the form comes back for another try.
                if let Err(e) = self.submit(&mut form) {
                    self.msg = format!("{e:#}");
                    self.popup = Some(Popup::Form(form));
                }
            }
            (Popup::Pick { items, sel }, KeyCode::Enter) => {
                // Pasted, then Enter: the command arrives as one piece of text.
                if let (Some((_, command)), Some(i)) = (items.get(sel), self.active.checked_sub(1))
                {
                    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                    for input in [TermInput::Paste(command), TermInput::Key(enter)] {
                        send_input(&mut self.tabs, i, self.broadcast, &input);
                    }
                }
            }
            (Popup::Batch(mut batch), code) => {
                match code {
                    // While the filter is typed every key belongs to it.
                    KeyCode::Esc if batch.typing => {
                        batch.filter.clear();
                        batch.typing = false;
                    }
                    KeyCode::Enter if batch.typing => batch.typing = false,
                    KeyCode::Backspace if batch.typing => drop(batch.filter.pop()),
                    KeyCode::Char(c) if batch.typing => batch.filter.push(c),
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => return,
                    KeyCode::Char('/') => batch.typing = true,
                    KeyCode::Down | KeyCode::Char('j') => batch.scroll += 1,
                    KeyCode::Up | KeyCode::Char('k') => {
                        batch.scroll = batch.scroll.saturating_sub(1)
                    }
                    KeyCode::PageDown => batch.scroll += 10,
                    KeyCode::PageUp => batch.scroll = batch.scroll.saturating_sub(10),
                    _ => {}
                }
                if batch.typing {
                    batch.scroll = 0;
                }
                self.popup = Some(Popup::Batch(batch));
            }
            (Popup::Pick { .. } | Popup::Peek(_), KeyCode::Esc | KeyCode::Char('q')) => {}
            (Popup::Peek(alias), KeyCode::Char('r') | KeyCode::Enter) => self.peek(&alias, true),
            (Popup::Form(_), KeyCode::Esc) => {
                // Cancelling the vault prompt also gives up the password waiting for it.
                if self.pending.take().is_some() {
                    self.msg = "password not stored: the vault stayed locked".into();
                }
            }
            (Popup::Text { .. }, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
            | (Popup::Confirm(..), KeyCode::Esc | KeyCode::Char('n')) => {}
            (mut popup, code) => {
                match (&mut popup, code) {
                    (Popup::Text { scroll, .. }, KeyCode::Down | KeyCode::Char('j')) => {
                        *scroll = scroll.saturating_add(1)
                    }
                    (Popup::Text { scroll, .. }, KeyCode::Up | KeyCode::Char('k')) => {
                        *scroll = scroll.saturating_sub(1)
                    }
                    (Popup::Text { scroll, .. }, KeyCode::PageDown) => {
                        *scroll = scroll.saturating_add(10)
                    }
                    (Popup::Text { scroll, .. }, KeyCode::PageUp) => {
                        *scroll = scroll.saturating_sub(10)
                    }
                    (Popup::Pick { items, sel }, KeyCode::Down | KeyCode::Char('j')) => {
                        *sel = (*sel + 1).min(items.len().saturating_sub(1))
                    }
                    (Popup::Pick { sel, .. }, KeyCode::Up | KeyCode::Char('k')) => {
                        *sel = sel.saturating_sub(1)
                    }
                    (Popup::Form(f), KeyCode::Tab | KeyCode::Down) => {
                        f.idx = (f.idx + 1) % f.rows()
                    }
                    (Popup::Form(f), KeyCode::BackTab | KeyCode::Up) => {
                        f.idx = (f.idx + f.rows() - 1) % f.rows()
                    }
                    // The save checkbox: Ctrl-s from any field, or S / Space on its own
                    // row. A plain S cannot toggle it while a text field has the cursor.
                    (Popup::Form(f), KeyCode::Char(c))
                        if f.save.is_some()
                            && ((c == 's' && k.modifiers == KeyModifiers::CONTROL)
                                || (f.idx == f.fields.len() && "Ss ".contains(c))) =>
                    {
                        f.save = f.save.map(|on| !on)
                    }
                    (Popup::Form(f), _) if f.idx == f.fields.len() => {}
                    (Popup::Form(f), KeyCode::Backspace) => drop(f.fields[f.idx].1.pop()),
                    (Popup::Form(f), KeyCode::Char(c))
                        if !k
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        f.fields[f.idx].1.push(c)
                    }
                    _ => {}
                }
                self.popup = Some(popup);
            }
        }
    }

    fn confirmed(&mut self, then: After) -> Result<()> {
        match then {
            After::DeleteHost(alias) => {
                self.fwds.retain(|key, (task, _)| {
                    let own = key.starts_with(&format!("{alias} "));
                    if own {
                        task.abort();
                    }
                    !own
                });
                self.sshc.remove_host(&alias);
                self.save_config()?;
                self.db.delete(&alias)?;
                self.marked.remove(&alias);
                self.reload()?;
            }
            After::DeleteSnippet(id) => {
                self.db.delete_snippet(id)?;
                self.sub_sel = self.sub_sel.saturating_sub(1);
                self.reload()?;
            }
            After::SftpDelete => {
                self.msg = self.sftp_view().map(|v| v.delete()).unwrap_or_default()
            }
            After::SftpCopy => self.msg = self.sftp_view().map(|v| v.copy()).unwrap_or_default(),
            After::Quit => self.quit = true,
            _ => {}
        }
        Ok(())
    }

    fn submit(&mut self, form: &mut Form) -> Result<()> {
        // A secret leaves the form here and is zeroized when this returns.
        let mut secret = Zeroizing::new(String::new());
        if let Some((_, last)) = form.fields.last_mut().filter(|_| form.masked) {
            mem::swap(&mut *secret, &mut **last);
        }
        let text = |i: usize| form.fields[i].1.trim().to_string();
        match &form.then {
            After::SaveHost(orig) => {
                let alias = text(0);
                let bad = |c: char| c.is_whitespace() || "*?!#\"=".contains(c);
                ensure!(
                    !alias.is_empty() && !alias.contains(bad),
                    "alias must be one word without wildcards"
                );
                let renamed = orig.as_deref() != Some(alias.as_str());
                ensure!(
                    !renamed || !self.sshc.hosts().contains(&alias),
                    "host {alias} already exists"
                );
                ensure!(
                    text(3).is_empty() || text(3).parse::<u16>().is_ok(),
                    "port must be a number up to 65535"
                );
                match orig {
                    Some(old) if renamed => self.sshc.rename_host(old, &alias),
                    Some(_) => {}
                    None => self.sshc.add_host(&alias),
                }
                for (i, key) in ["HostName", "User", "Port", "IdentityFile"]
                    .iter()
                    .enumerate()
                {
                    self.sshc.set(&alias, key, &text(i + 1));
                }
                self.save_config()?;
                if let Some(old) = orig.as_ref().filter(|_| renamed) {
                    self.db.rename(old, &alias)?;
                }
                let mut meta = self.db.metas()?.remove(&alias).unwrap_or_default();
                (meta.grp, meta.tags, meta.notes) = (text(5), text(6), text(7));
                self.db.set_meta(&alias, &meta)?;
                self.reload()?;
                self.sel = self.hosts.iter().position(|h| *h == alias).unwrap_or(0);
            }
            After::Password {
                alias,
                target,
                saved,
                sftp,
            } => self.open_target(alias, target.clone(), *saved, *sftp, Some(secret)),
            After::QuickConnect => {
                let target = Target::parse(&text(0))?;
                let password = (!secret.is_empty()).then_some(secret);
                if form.save == Some(true) {
                    // The host goes into the ssh config, the password only into the vault.
                    let alias = target.save_to(&mut self.sshc)?;
                    self.save_config()?;
                    self.db.import(std::slice::from_ref(&alias))?;
                    self.reload()?;
                    self.sel = self.hosts.iter().position(|h| *h == alias).unwrap_or(0);
                    self.open_target(&alias, target, true, false, password.clone());
                    self.msg = format!("saved as {alias}");
                    if let Some(password) = password {
                        self.store_password(alias, password);
                    }
                } else {
                    let title = match target.port {
                        22 => format!("{}@{}", target.user, target.host),
                        port => format!("{}@{}:{port}", target.user, target.host),
                    };
                    // The password lives in this session's task only.
                    self.open_target(&title, target, false, false, password);
                }
            }
            After::ConnectTag => {
                let tag = text(0);
                let with_tag = |a: &String| self.meta.get(a).is_some_and(|m| tag_is(m, &tag));
                let hosts: Vec<String> = self.sshc.hosts().into_iter().filter(with_tag).collect();
                ensure!(!hosts.is_empty(), "no host has the tag or group {tag}");
                self.open_many(&hosts);
            }
            After::VaultUnlock(next) => {
                self.vault = Some(Vault::open(&vault_path(), &secret)?);
                self.msg = "vault unlocked".into();
                self.store_pending()?;
                if let Some(alias) = next {
                    self.password_prompt(alias);
                }
            }
            After::VaultCreate(next, None) => {
                ensure!(!secret.is_empty(), "passphrase must not be empty");
                let then = After::VaultCreate(next.clone(), Some(secret));
                self.prompt("Create vault", "Repeat passphrase", true, then);
            }
            After::VaultCreate(next, Some(first)) => {
                ensure!(**first == *secret, "passphrases do not match");
                self.vault = Some(Vault::create(&vault_path(), &secret)?);
                self.msg = "vault created".into();
                self.store_pending()?;
                if let Some(alias) = next {
                    self.password_prompt(alias);
                }
            }
            After::VaultSet(alias) => {
                let removing = secret.is_empty();
                let vault = self.vault.as_mut().context("vault is locked")?;
                vault.set(alias, &mut secret)?;
                self.msg = match removing {
                    true => format!("password for {alias} removed"),
                    false => format!("password for {alias} stored"),
                };
            }
            After::AddForward(alias) => {
                let spec = text(0);
                let mut chars = spec.chars();
                let kind = chars.next().unwrap_or(' ').to_ascii_uppercase();
                let fwd = Forward::parse(kind, chars.as_str()).context(
                    "expected e.g. L 8080 localhost:80, R 9000 localhost:3000 or D 1080",
                )?;
                self.sshc.add(alias, fwd.key(), &fwd.value());
                self.save_config()?;
            }
            After::Batch(hosts) => {
                ensure!(!text(0).is_empty(), "type a command to run");
                self.batch(hosts.clone(), &text(0));
            }
            After::SaveSnippet(id) => {
                ensure!(
                    !text(0).is_empty() && !text(1).is_empty(),
                    "name and command are required"
                );
                let host = text(2);
                ensure!(
                    host.is_empty() || self.sshc.hosts().contains(&host),
                    "no host named {host}"
                );
                self.db.save_snippet(&Snippet {
                    id: *id,
                    name: text(0),
                    command: text(1),
                    host,
                })?;
                self.reload()?;
            }
            After::SftpMkdir => {
                ensure!(!text(0).is_empty(), "name is required");
                self.msg = self
                    .sftp_view()
                    .map(|v| v.mkdir(&text(0)))
                    .unwrap_or_default();
            }
            After::SftpRename(old) => {
                ensure!(!text(0).is_empty(), "name is required");
                self.msg = self
                    .sftp_view()
                    .map(|v| v.rename(old, &text(0)))
                    .unwrap_or_default();
            }
            After::DeleteHost(_)
            | After::DeleteSnippet(_)
            | After::SftpDelete
            | After::SftpCopy
            | After::Quit => {}
        }
        Ok(())
    }
}

/// Runs the TUI until the user quits. Call it inside `vssh_core::task::run`:
/// the connections it opens are that runtime's work.
pub fn run(cfg: Config) -> Result<()> {
    let (tx, rx) = channel();
    // Everything that can fail on startup fails here, before the screen is taken over.
    let mut app = App::new(cfg, tx.clone())?;
    let mut terminal = ratatui::try_init()?;
    let _ = execute!(stdout(), EnableBracketedPaste);
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if tx.send(Event::Input(ev)).is_err() {
                break;
            }
        }
    });
    let res = (|| {
        while !app.quit {
            terminal.draw(|f| draw::draw(&mut app, f))?;
            app.probe_selected();
            let tab = app.active;
            // A transition under way, or a spinner on screen, draws its next
            // frame on time, event or not.
            let tick = match app.slide {
                0 if app.waiting() => Some(draw::SPIN),
                0 => None,
                _ => Some(draw::FRAME),
            };
            let ev = match tick {
                None => rx.recv().ok(),
                Some(tick) => match rx.recv_timeout(tick) {
                    Ok(ev) => Some(ev),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => None,
                },
            };
            let Some(ev) = ev else { break };
            app.on_event(ev);
            // Coalesce bursts (fast output, transfer progress) into one redraw.
            while let Ok(ev) = rx.try_recv() {
                app.on_event(ev);
            }
            if app.active != tab && app.cfg.transitions {
                app.slide = draw::SLIDE;
            }
        }
        anyhow::Ok(())
    })();
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_filter_matches_tags_and_group_by_prefix() {
        let meta = Meta {
            grp: "Prod".into(),
            tags: "web, eu-west db".into(),
            ..Meta::default()
        };
        for hit in ["web", "WEB", "eu", "eu-west", "db", "pro", ""] {
            assert!(has_tag(&meta, hit), "{hit}");
        }
        for miss in ["west", "eb", "prod2", "x"] {
            assert!(!has_tag(&meta, miss), "{miss}");
        }
        assert!(!has_tag(&Meta::default(), ""));
    }

    /// A live terminal tab whose "connection" is just the receiving end of its channel.
    fn term_tab(id: u64) -> (Tab, task::Receiver<SessionCmd>) {
        let (tx, rx) = task::channel();
        let size = NonZeroU16::new(10).unwrap();
        let parser = vt100::Parser::new_with_callbacks(size, size, 100, term::Replies::default());
        let tab = Tab {
            id,
            alias: format!("host{id}"),
            target: Target::parse("user@host").unwrap(),
            saved: true,
            kind: Kind::Term {
                parser: Box::new(parser),
                tx,
            },
            task: Task::default(),
            connected: true,
            closed: false,
            log: None,
        };
        (tab, rx)
    }

    fn received(rx: &mut task::Receiver<SessionCmd>) -> Vec<u8> {
        let mut all = vec![];
        while let Ok(SessionCmd::Data(bytes)) = rx.try_recv() {
            all.extend(bytes);
        }
        all
    }

    #[test]
    fn broadcast_sends_keys_to_every_live_session() {
        let (mut tabs, mut rxs): (Vec<_>, Vec<_>) = (1..=4).map(term_tab).unzip();
        let (sftp_tx, _sftp_rx) = task::channel();
        tabs.push(Tab {
            kind: Kind::Sftp(Box::new(SftpView::new(sftp_tx))),
            ..term_tab(9).0
        });
        tabs[3].closed = true;
        let key = |c| TermInput::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));

        // Broadcast off: only the focused session hears the key.
        assert_eq!(send_input(&mut tabs, 1, false, &key('a')), 1);
        let got: Vec<_> = rxs.iter_mut().map(received).collect();
        assert_eq!(
            got,
            [b"".to_vec(), b"a".to_vec(), b"".to_vec(), b"".to_vec()]
        );

        // Broadcast on: every live terminal does; the closed one and SFTP do not.
        assert_eq!(send_input(&mut tabs, 1, true, &key('l')), 3);
        assert_eq!(send_input(&mut tabs, 1, true, &key('s')), 3);
        let enter = TermInput::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        send_input(&mut tabs, 1, true, &enter);
        let got: Vec<_> = rxs.iter_mut().map(received).collect();
        assert_eq!(
            got,
            [
                b"ls\r".to_vec(),
                b"ls\r".to_vec(),
                b"ls\r".to_vec(),
                b"".to_vec()
            ]
        );

        // Focus on a dead pane still broadcasts to the others, and types nowhere without it.
        assert_eq!(send_input(&mut tabs, 3, true, &key('x')), 3);
        assert_eq!(send_input(&mut tabs, 3, false, &key('x')), 0);
    }

    #[test]
    fn broadcast_encodes_for_each_sessions_own_mode() {
        let (mut tabs, mut rxs): (Vec<_>, Vec<_>) = (1..=2).map(term_tab).unzip();
        // Session 2 is in a full-screen program: application cursor keys, bracketed paste.
        if let Kind::Term { parser, .. } = &mut tabs[1].kind {
            parser.process(b"\x1b[?1h\x1b[?2004h");
        }
        let up = TermInput::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        send_input(&mut tabs, 0, true, &up);
        send_input(&mut tabs, 0, true, &TermInput::Paste("a\nb"));
        assert_eq!(received(&mut rxs[0]), b"\x1b[Aa\rb");
        assert_eq!(received(&mut rxs[1]), b"\x1bOA\x1b[200~a\nb\x1b[201~");
    }

    #[test]
    fn session_logs_are_private_and_stay_in_their_directory() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vssh-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let logs = dir.join("logs");

        let (mut file, path) = start_log(&logs, "web-01", "20261116-101500").unwrap();
        assert_eq!(path, logs.join("web-01-20261116-101500.log"));
        file.write_all(b"hello").unwrap();
        #[cfg(unix)]
        {
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!((mode(&path), mode(&logs)), (0o600, 0o700));
        }

        // Names that try to leave the directory, and a quick-connect title.
        for (alias, name) in [
            ("../../etc/cron.d/x", ".._.._etc_cron.d_x-s.log"),
            ("/abs/path", "_abs_path-s.log"),
            ("a b\n$(c)", "a_b___c_-s.log"),
            ("root@10.0.0.5:2222", "root@10.0.0.5_2222-s.log"),
        ] {
            let (_, path) = start_log(&logs, alias, "s").unwrap();
            assert_eq!(path, logs.join(name), "{alias}");
        }
        // Logging the same session twice in one second appends, it does not truncate.
        let (mut again, _) = start_log(&logs, "web-01", "20261116-101500").unwrap();
        again.write_all(b" again").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello again");
        // Windows keeps a file that is open, and so its directory.
        drop((file, again));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_picked_snippet_is_typed_into_every_broadcast_session() {
        let (mut tabs, mut rxs): (Vec<_>, Vec<_>) = (1..=2).map(term_tab).unzip();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        for input in [TermInput::Paste("df -h /"), TermInput::Key(enter)] {
            send_input(&mut tabs, 0, true, &input);
        }
        assert_eq!(received(&mut rxs[0]), b"df -h /\r");
        assert_eq!(received(&mut rxs[1]), b"df -h /\r");
    }

    #[test]
    fn a_quick_connect_password_waits_for_the_vault_then_goes_only_there() {
        let dir = std::env::temp_dir().join(format!("vssh-quicksave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("vault");
        let waiting = || {
            Some((
                "web.example".to_string(),
                Zeroizing::new("s3cret-pw".to_string()),
            ))
        };

        // Vault locked: nothing is stored and the password keeps waiting.
        let mut pending = waiting();
        assert_eq!(store_waiting(None, &mut pending).unwrap(), None);
        assert!(pending.is_some() && !path.exists());

        // Vault unlocked: stored under the alias, and no longer held here.
        let mut vault = Vault::create(&path, "master").unwrap();
        let stored = store_waiting(Some(&mut vault), &mut pending).unwrap();
        assert_eq!(stored.as_deref(), Some("web.example"));
        assert!(pending.is_none());
        assert_eq!(store_waiting(Some(&mut vault), &mut pending).unwrap(), None);
        drop(vault);

        // It is what a later connection to that alias looks up, and it is not readable on disk.
        let vault = Vault::open(&path, "master").unwrap();
        assert_eq!(vault.get("web.example"), Some("s3cret-pw"));
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(9).any(|w| w == b"s3cret-pw"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_checkbox_is_one_more_form_row() {
        let form = |save| Form {
            title: String::new(),
            fields: vec![("a", Zeroizing::new(String::new())); 2],
            idx: 0,
            masked: true,
            save,
            then: After::QuickConnect,
        };
        assert_eq!((form(None).rows(), form(Some(false)).rows()), (2, 3));
    }

    #[test]
    fn tag_drawer_counts_what_the_filter_would_show() {
        let meta = |grp: &str, tags: &str| Meta {
            grp: grp.into(),
            tags: tags.into(),
            ..Meta::default()
        };
        let all: HashMap<String, Meta> = [
            ("web-01", meta("prod", "web eu")),
            ("web-02", meta("prod", "Web, us")),
            ("web-03", meta("staging", "webapp")),
            ("db-01", meta("prod", "db")),
            ("bare", meta("", "")),
            ("gone", meta("prod", "web")),
        ]
        .into_iter()
        .map(|(alias, meta)| (alias.to_string(), meta))
        .collect();
        // `gone` has metadata but is no longer a host: it does not count.
        let hosts: Vec<String> = ["web-01", "web-02", "web-03", "db-01", "bare", "nometa"]
            .map(String::from)
            .into();
        let counts = tag_counts(&hosts, &all);
        let shown: Vec<(&str, usize)> = counts.iter().map(|(t, n)| (t.as_str(), *n)).collect();
        // `web` also counts `webapp`: `#web` shows that host too. `Web` is the same tag.
        assert_eq!(
            shown,
            [
                ("prod", 3),
                ("web", 3),
                ("db", 1),
                ("eu", 1),
                ("staging", 1),
                ("us", 1),
                ("webapp", 1)
            ]
        );
        // Each count is exactly the number of hosts the search `#tag` lists.
        for (tag, count) in &counts {
            let listed = hosts
                .iter()
                .filter(|h| all.get(*h).is_some_and(|m| has_tag(m, tag)));
            assert_eq!(listed.count(), *count, "{tag}");
        }
        assert!(tag_counts(&[], &all).is_empty());
    }

    #[test]
    fn batch_results_keep_every_host_and_every_line() {
        let refused = Report {
            class: vssh_core::conn_err::Class::Refused,
            message: "Connection refused by 10.0.0.3:22 — is sshd running?".into(),
            chain: String::new(),
        };
        let rows = vec![
            (
                "web-01".to_string(),
                Some(Ok("up 3 days\nload 0.5\n".to_string())),
            ),
            ("web-02".to_string(), None),
            ("db-01".to_string(), Some(Err(refused))),
            ("db-02".to_string(), Some(Ok(" \n".to_string()))),
        ];
        let lines = batch_lines(&rows, "", Lang::En);
        let shown: Vec<(&str, &str, Row)> = lines
            .iter()
            .map(|(host, text, row)| (host.as_str(), text.as_str(), *row))
            .collect();
        assert_eq!(
            shown,
            [
                ("web-01", "up 3 days", Row::Out),
                ("web-01", "load 0.5", Row::Out),
                ("web-02", "running…", Row::Running),
                (
                    "db-01",
                    "✗ REFUSED Connection refused by 10.0.0.3:22 — is sshd running?",
                    Row::Failed
                ),
                ("db-02", "(no output)", Row::Out),
            ]
        );
        // Filtering by host keeps whole hosts, never part of one's output.
        let hosts = |filter| {
            let lines = batch_lines(&rows, filter, Lang::Vi);
            let mut hosts: Vec<String> = lines.into_iter().map(|l| l.0).collect();
            hosts.dedup();
            hosts
        };
        assert_eq!(hosts("db"), ["db-01", "db-02"]);
        assert_eq!(hosts("-01"), ["web-01", "db-01"]);
        assert!(hosts("nope").is_empty());
        assert_eq!(batch_lines(&rows, "web-02", Lang::Vi)[0].1, "đang chạy…");
    }

    #[test]
    fn connect_by_tag_is_exact() {
        let meta = Meta {
            grp: "Prod".into(),
            tags: "web, eu-west".into(),
            ..Meta::default()
        };
        assert!(tag_is(&meta, "web") && tag_is(&meta, "WEB") && tag_is(&meta, "prod"));
        assert!(!tag_is(&meta, "we") && !tag_is(&meta, "eu") && !tag_is(&meta, ""));
    }

    #[test]
    fn clean_strips_escape_sequences() {
        assert_eq!(clean("a\x1b[31mb\x07\r\n\tc\u{9b}d"), "a[31mb\n    cd");
    }
}
