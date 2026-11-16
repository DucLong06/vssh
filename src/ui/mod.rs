//! The TUI application: state, event handling and the main loop.

mod draw;
mod files;
mod term;

use std::collections::{BTreeSet, HashMap};
use std::io::stdout;
use std::mem;
use std::num::NonZeroU16;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
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
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::config::{data_dir, ssh_config_path, Config};
use crate::sftp;
use crate::ssh::{self, forward, AuthFailed, SessionCmd, Target};
use crate::store::db::{Db, Meta, Snippet};
use crate::store::ssh_config::{Forward, SshConfig, FORWARD_KEYS};
use crate::vault::Vault;
use files::SftpView;

/// Everything that can wake the UI. Background tasks only ever talk to the
/// app through these.
pub enum Event {
    Input(Input),
    /// Output of a shell session.
    Data(u64, Vec<u8>),
    /// A tab's connection ended, with the reason.
    Closed(u64, String),
    /// A tab could not authenticate; ask for a password and retry.
    AuthFailed(u64),
    Sftp(u64, sftp::Reply),
    /// Host alias and TCP connect time in ms, `None` if unreachable.
    Ping(String, Option<u128>),
    /// Forward key; `Ok(true)` = now listening, anything else = it ended.
    Forward(String, Result<bool, String>),
    /// Host alias and command output.
    Snippet(String, String),
}

impl Event {
    fn ended(id: u64, res: Result<()>) -> Self {
        match res {
            Ok(()) => Self::Closed(id, "connection closed".into()),
            Err(e) if e.is::<AuthFailed>() => Self::AuthFailed(id),
            Err(e) => Self::Closed(id, format!("{e:#}")),
        }
    }
}

enum Kind {
    Term {
        parser: Box<vt100::Parser<term::Replies>>,
        tx: UnboundedSender<SessionCmd>,
    },
    Sftp(Box<SftpView>),
}

struct Tab {
    id: u64,
    alias: String,
    kind: Kind,
    task: JoinHandle<()>,
    /// Set once the first byte or listing arrives.
    connected: bool,
    closed: bool,
}

/// What to do with a submitted form or a confirmed question.
enum After {
    /// Original alias when editing, `None` when adding.
    SaveHost(Option<String>),
    DeleteHost(String),
    /// Retry a connection with a typed password.
    Password {
        alias: String,
        sftp: bool,
    },
    /// Unlock, then optionally go on to set this host's password.
    VaultUnlock(Option<String>),
    /// Same, for a new vault; the second field is the first passphrase entry.
    VaultCreate(Option<String>, Option<Zeroizing<String>>),
    VaultSet(String),
    AddForward(String),
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
    /// Single secret field, shown as dots.
    masked: bool,
    then: After,
}

enum Popup {
    Form(Form),
    Confirm(String, After),
    Text {
        title: String,
        body: String,
        scroll: u16,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    List,
    Search,
    Forwards,
    Snippets,
}

pub struct App {
    cfg: Config,
    tx: UnboundedSender<Event>,
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
    /// Selection inside the forwards / snippets panel.
    sub_sel: usize,
    marked: BTreeSet<String>,
    reach: HashMap<String, Option<u128>>,
    /// Running forwards by key, with whether they are listening yet.
    fwds: HashMap<String, (JoinHandle<()>, bool)>,
    tabs: Vec<Tab>,
    /// 0 is the host list, `i` is `tabs[i - 1]`.
    active: usize,
    next_id: u64,
    leader: bool,
    /// Columns and rows available to a terminal tab.
    body: (u16, u16),
    popup: Option<Popup>,
    msg: String,
    quit: bool,
}

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

impl App {
    fn new(cfg: Config, tx: UnboundedSender<Event>) -> Result<Self> {
        let sshc = SshConfig::load(&ssh_config_path()).context("cannot read ~/.ssh/config")?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(data_dir())?;
        let db = Db::open(&data_dir().join("vssh.db"))?;
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
            marked: BTreeSet::new(),
            reach: HashMap::new(),
            fwds: HashMap::new(),
            tabs: vec![],
            active: 0,
            next_id: 1,
            leader: false,
            body: (80, 24),
            popup: None,
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

    /// Opens a shell or SFTP tab; the connection runs in its own task.
    fn open(&mut self, alias: &str, sftp: bool, password: Option<Zeroizing<String>>) {
        let target = match Target::from_config(&self.sshc, alias) {
            Ok(t) => t,
            Err(e) => return self.msg = e.to_string(),
        };
        let password = password.or_else(|| self.vault_password(alias));
        let (id, tx, size) = (self.next_id, self.tx.clone(), self.body);
        self.next_id += 1;
        let (kind, task) = if sftp {
            let (cmd_tx, cmd_rx) = unbounded_channel();
            let task = tokio::spawn(async move {
                let res = async {
                    let pw = password.as_ref().map(|p| p.as_str());
                    let h = ssh::connect(&target, pw, None).await?;
                    let reply = |r| drop(tx.send(Event::Sftp(id, r)));
                    sftp::worker(h, cmd_rx, reply).await
                };
                let _ = tx.send(Event::ended(id, res.await));
            });
            (Kind::Sftp(Box::new(SftpView::new(cmd_tx))), task)
        } else {
            let (cmd_tx, cmd_rx) = unbounded_channel();
            let task = tokio::spawn(async move {
                let res = async {
                    let pw = password.as_ref().map(|p| p.as_str());
                    let h = ssh::connect(&target, pw, None).await?;
                    let data = |d: &[u8]| drop(tx.send(Event::Data(id, d.to_vec())));
                    ssh::shell(h, size, cmd_rx, data).await
                };
                let _ = tx.send(Event::ended(id, res.await));
            });
            let rows = NonZeroU16::new(size.1).unwrap_or(NonZeroU16::MIN);
            let cols = NonZeroU16::new(size.0).unwrap_or(NonZeroU16::MIN);
            let replies = term::Replies::default();
            let parser = vt100::Parser::new_with_callbacks(rows, cols, 5000, replies);
            let mut parser = Box::new(parser);
            parser.process(format!("Connecting to {alias}…\r\n").as_bytes());
            (Kind::Term { parser, tx: cmd_tx }, task)
        };
        self.tabs.push(Tab {
            id,
            alias: alias.to_string(),
            kind,
            task,
            connected: false,
            closed: false,
        });
        self.active = self.tabs.len();
    }

    /// Removes a tab. The view stays on the tab it was on; if that is the one
    /// going away it returns to the host list, never into another live shell.
    fn close_tab(&mut self, i: usize) -> Tab {
        let tab = self.tabs.remove(i);
        tab.task.abort();
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
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let start = Instant::now();
                let attempt = TcpStream::connect((t.host, t.port));
                let res = tokio::time::timeout(Duration::from_secs(3), attempt).await;
                let ms = matches!(res, Ok(Ok(_))).then(|| start.elapsed().as_millis());
                let _ = tx.send(Event::Ping(alias, ms));
            });
        }
        self.msg = "checking reachability…".into();
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
        let (tx, k) = (self.tx.clone(), key.clone());
        let task = tokio::spawn(async move {
            let pw = password.as_ref().map(|p| p.as_str());
            let ready = || drop(tx.send(Event::Forward(k.clone(), Ok(true))));
            let res = forward::run(&target, pw, fwd, ready).await;
            let res = res.map(|()| false).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Forward(k.clone(), res));
        });
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
        self.popup = Some(Popup::Text {
            title: format!("{} on {} host(s)", s.name, hosts.len()),
            body: String::new(),
            scroll: 0,
        });
        for host in hosts {
            let target = Target::from_config(&self.sshc, &host);
            let password = self.vault_password(&host);
            let (tx, command) = (self.tx.clone(), s.command.clone());
            tokio::spawn(async move {
                let res = async {
                    let pw = password.as_ref().map(|p| p.as_str());
                    let h = ssh::connect(&target?, pw, None).await?;
                    ssh::exec(&h, &command).await
                };
                let out = res.await.unwrap_or_else(|e| format!("error: {e:#}\n"));
                let _ = tx.send(Event::Snippet(host, out));
            });
        }
    }

    // ── events ──────────────────────────────────────────────────────────

    fn on_event(&mut self, ev: Event) {
        let tab_index = |tabs: &[Tab], id| tabs.iter().position(|t| t.id == id);
        match ev {
            Event::Input(Input::Key(k)) if k.kind == KeyEventKind::Press => {
                self.msg.clear();
                self.on_key(k);
            }
            Event::Input(Input::Paste(text)) => self.on_paste(&text),
            // Resize and focus changes only need the redraw that follows.
            Event::Input(_) => {}
            Event::Data(id, data) => {
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
            }
            Event::Sftp(id, reply) => {
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
            Event::Closed(id, why) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                self.tabs[i].closed = true;
                match &mut self.tabs[i].kind {
                    Kind::Term { parser, .. } => {
                        let note = format!("\r\n\x1b[0;33m[{}] press any key\x1b[0m", clean(&why));
                        parser.process(note.as_bytes());
                    }
                    Kind::Sftp(_) => self.msg = why,
                }
            }
            Event::AuthFailed(id) => {
                let Some(i) = tab_index(&self.tabs, id) else {
                    return;
                };
                let tab = self.close_tab(i);
                let then = After::Password {
                    sftp: matches!(tab.kind, Kind::Sftp(_)),
                    alias: tab.alias.clone(),
                };
                let title = format!("Keys were refused. Password for {}", tab.alias);
                self.prompt(&title, "Password", true, then);
            }
            Event::Ping(alias, ms) => {
                self.reach.insert(alias, ms);
            }
            Event::Forward(key, Ok(true)) => {
                if let Some(state) = self.fwds.get_mut(&key) {
                    state.1 = true;
                }
            }
            Event::Forward(key, res) => {
                self.fwds.remove(&key);
                let why = res.err().unwrap_or_else(|| "stopped".into());
                self.msg = format!("forward {key}: {why}");
            }
            Event::Snippet(host, out) => {
                let block = format!("── {host} ──\n{}\n", clean(&out));
                match &mut self.popup {
                    Some(Popup::Text { body, .. }) => body.push_str(&block),
                    _ => {
                        self.popup = Some(Popup::Text {
                            title: "Snippet output".into(),
                            body: block,
                            scroll: 0,
                        })
                    }
                }
            }
        }
    }

    /// Records a successful connection the first time a tab hears back.
    fn mark_connected(&mut self, i: usize) {
        if !mem::replace(&mut self.tabs[i].connected, true) {
            let res = self.db.touch(&self.tabs[i].alias);
            let res = res.and_then(|()| self.db.metas().map(|m| self.meta = m));
            self.check(res.map_err(Into::into));
        }
    }

    fn on_paste(&mut self, text: &str) {
        match &mut self.popup {
            Some(Popup::Form(f)) => {
                let idx = f.idx;
                return f.fields[idx].1.push_str(&text.replace(['\r', '\n'], " "));
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
        } else if let Kind::Term { parser, tx } = &self.tabs[self.active - 1].kind {
            let data = match parser.screen().bracketed_paste() {
                true => format!("\x1b[200~{text}\x1b[201~"),
                false => text.replace('\n', "\r"),
            };
            let _ = tx.send(SessionCmd::Data(data.into_bytes()));
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        if let Some(popup) = self.popup.take() {
            return self.popup_key(popup, k);
        }
        let is_leader =
            k.modifiers == KeyModifiers::CONTROL && k.code == KeyCode::Char(self.cfg.keys.leader);
        if mem::take(&mut self.leader) {
            return self.leader_key(k, is_leader);
        }
        if is_leader {
            return self.leader = true;
        }
        if self.active == 0 {
            return self.hosts_key(k);
        }
        let i = self.active - 1;
        let closed = self.tabs[i].closed;
        match &mut self.tabs[i].kind {
            Kind::Term { .. } if closed => drop(self.close_tab(i)),
            Kind::Term { parser, tx } => {
                parser.screen_mut().set_scrollback(0);
                let bytes = term::encode_key(k, parser.screen().application_cursor());
                let _ = tx.send(SessionCmd::Data(bytes));
            }
            Kind::Sftp(_) => self.sftp_key(k),
        }
    }

    fn cycle(&mut self, forward: bool) {
        let n = self.tabs.len() + 1;
        self.active = (self.active + if forward { 1 } else { n - 1 }) % n;
    }

    /// Tab control, reachable from every tab. Pressing the leader twice
    /// sends it through to the remote shell.
    fn leader_key(&mut self, k: KeyEvent, again: bool) {
        let term = match self.active.checked_sub(1).map(|i| &mut self.tabs[i].kind) {
            Some(Kind::Term { parser, tx }) => Some((parser, tx)),
            _ => None,
        };
        let scroll = |parser: &mut vt100::Parser<term::Replies>, by: isize| {
            let at = parser.screen().scrollback().saturating_add_signed(by);
            parser.screen_mut().set_scrollback(at);
        };
        match (k.code, term) {
            (_, Some((_, tx))) if again => {
                let _ = tx.send(SessionCmd::Data(term::encode_key(k, false)));
            }
            (KeyCode::PageUp | KeyCode::Char('k'), Some((parser, _))) => scroll(parser, 10),
            (KeyCode::PageDown | KeyCode::Char('j'), Some((parser, _))) => scroll(parser, -10),
            (KeyCode::Tab | KeyCode::Char('n'), _) => self.cycle(true),
            (KeyCode::BackTab | KeyCode::Char('p'), _) => self.cycle(false),
            (KeyCode::Char('h'), _) => self.active = 0,
            (KeyCode::Char('x'), _) if self.active > 0 => drop(self.close_tab(self.active - 1)),
            (KeyCode::Char(c @ '1'..='9'), _) => {
                let i = c as usize - '1' as usize;
                if i <= self.tabs.len() {
                    self.active = i;
                }
            }
            _ => {}
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let last = self.hosts.len().saturating_sub(1);
        self.sel = self.sel.saturating_add_signed(delta).min(last);
    }

    fn hosts_key(&mut self, k: KeyEvent) {
        match self.focus {
            Focus::Search => return self.search_key(k),
            Focus::Forwards | Focus::Snippets => return self.panel_key(k),
            Focus::List => {}
        }
        let keys = &self.cfg.keys;
        let ctrl = k.modifiers == KeyModifiers::CONTROL;
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::PageDown => self.move_sel(10),
            KeyCode::PageUp => self.move_sel(-10),
            KeyCode::Home | KeyCode::Char('g') => self.sel = 0,
            KeyCode::End | KeyCode::Char('G') => self.move_sel(isize::MAX),
            KeyCode::Tab => self.cycle(true),
            KeyCode::BackTab => self.cycle(false),
            KeyCode::Esc => {
                self.filter.clear();
                self.refilter();
            }
            KeyCode::Char(c) if c == keys.quit || (c == 'c' && ctrl) => {
                let (tabs, fwds) = (self.tabs.len(), self.fwds.len());
                if tabs + fwds == 0 {
                    return self.quit = true;
                }
                let text = format!("Quit? This closes {tabs} tab(s) and {fwds} forward(s).");
                self.popup = Some(Popup::Confirm(text, After::Quit));
            }
            KeyCode::Char(c) if c == keys.search => self.focus = Focus::Search,
            KeyCode::Char(c) if c == keys.add => self.host_form(None),
            KeyCode::Char(c) if c == keys.help => self.help(),
            KeyCode::Char(c) if c == keys.ping => self.ping_all(),
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
                self.open(alias, false, None);
            }
            return Ok(());
        };
        if c == keys.sftp {
            self.open(alias, true, None);
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
            (KeyCode::Char('q'), _) => return drop(self.close_tab(self.active - 1)),
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
        let body = format!(
            "HOST LIST\n\
             \x20 j/k ↑/↓      move            g/G     first / last\n\
             \x20 Enter        open shell      {}       SFTP browser\n\
             \x20 {}            search (fuzzy)  Esc     clear search\n\
             \x20 {}            add host        {}       edit host\n\
             \x20 {}            delete host     {}       duplicate host\n\
             \x20 {}            pin / unpin     Space   mark for snippets\n\
             \x20 {}            port forwards   {}       snippets\n\
             \x20 {}            check reachability\n\
             \x20 {}            store password  {}       unlock / lock vault\n\
             \x20 Tab          next tab        {}       quit\n\n\
             FORWARDS / SNIPPETS PANEL\n\
             \x20 a add   e edit   d delete   Enter start/stop or run   Esc back\n\
             \x20 forward syntax:  L 8080 localhost:80 | R 9000 localhost:3000 | D 1080\n\n\
             ANY TAB (leader = Ctrl-{leader})\n\
             \x20 leader Tab/n  next tab      leader p    previous tab\n\
             \x20 leader 1-9    go to tab     leader h    host list\n\
             \x20 leader x      close tab     leader k/j  scroll back / forward\n\
             \x20 leader leader send Ctrl-{leader} to the remote\n\n\
             SFTP\n\
             \x20 Tab switch pane   Enter/l open dir   h/Backspace parent\n\
             \x20 c copy to other pane   m mkdir   r rename   d delete\n\
             \x20 R refresh   q close\n",
            k.sftp,
            k.search,
            k.add,
            k.edit,
            k.delete,
            k.duplicate,
            k.pin,
            k.forwards,
            k.snippets,
            k.ping,
            k.password,
            k.vault,
            k.quit,
            leader = k.leader,
        );
        self.popup = Some(Popup::Text {
            title: "Help".into(),
            body,
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
            (Popup::Text { .. }, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
            | (Popup::Confirm(..), KeyCode::Esc | KeyCode::Char('n'))
            | (Popup::Form(_), KeyCode::Esc) => {}
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
                    (Popup::Form(f), KeyCode::Tab | KeyCode::Down) => {
                        f.idx = (f.idx + 1) % f.fields.len()
                    }
                    (Popup::Form(f), KeyCode::BackTab | KeyCode::Up) => {
                        f.idx = (f.idx + f.fields.len() - 1) % f.fields.len()
                    }
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
        if form.masked {
            mem::swap(&mut *secret, &mut *form.fields[0].1);
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
            After::Password { alias, sftp } => self.open(alias, *sftp, Some(secret)),
            After::VaultUnlock(next) => {
                self.vault = Some(Vault::open(&vault_path(), &secret)?);
                self.msg = "vault unlocked".into();
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

pub async fn run(cfg: Config) -> Result<()> {
    let (tx, mut rx) = unbounded_channel();
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
    let res = async {
        while !app.quit {
            terminal.draw(|f| draw::draw(&mut app, f))?;
            let Some(ev) = rx.recv().await else { break };
            app.on_event(ev);
            // Coalesce bursts (fast output, transfer progress) into one redraw.
            while let Ok(ev) = rx.try_recv() {
                app.on_event(ev);
            }
        }
        anyhow::Ok(())
    }
    .await;
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_escape_sequences() {
        assert_eq!(clean("a\x1b[31mb\x07\r\n\tc\u{9b}d"), "a[31mb\n    cd");
    }
}
