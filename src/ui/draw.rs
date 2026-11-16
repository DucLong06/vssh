//! Rendering: header bar, body (host list + details, a session, or SFTP
//! panes), bottom bar with the keys of the focused view.

use std::num::NonZeroU16;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::Frame;

use super::files::{SftpView, LOCAL, REMOTE};
use super::panes;
use super::{clean, fwd_key, term, App, Focus, Kind, Leader, Popup, Tab};
use crate::config::Theme;
use crate::ssh::SessionCmd;
use crate::store::db::Meta;

/// Stable colour per group or tag name. 256-colour pastels, so they look the
/// same on every terminal theme and stay readable under black badge text.
fn chip(name: &str) -> Color {
    const PALETTE: [u8; 8] = [212, 86, 220, 141, 114, 209, 75, 180];
    let hash = name
        .bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b.into()));
    Color::Indexed(PALETTE[hash % PALETTE.len()])
}

fn tags(meta: &Meta) -> impl Iterator<Item = &str> {
    meta.tags.split([' ', ',']).filter(|t| !t.is_empty())
}

/// A tag badge. On the selected row the selection background wins, so the
/// badge turns into coloured text there instead of black on dark.
fn badge(tag: &str, selected: bool) -> Span<'static> {
    let style = match selected {
        true => Style::new().fg(chip(tag)).bold(),
        false => Style::new().fg(Color::Black).bg(chip(tag)),
    };
    Span::styled(format!(" {tag} "), style)
}

fn human(bytes: u64) -> String {
    let mut n = bytes as f64;
    for unit in ["B", "K", "M", "G"] {
        if n < 1024.0 {
            return match unit {
                "B" => format!("{bytes} B"),
                _ => format!("{n:.1} {unit}"),
            };
        }
        n /= 1024.0;
    }
    format!("{n:.1} T")
}

/// TCP connect time; a LAN host answers in under a millisecond.
fn latency(ms: u128) -> String {
    match ms {
        0 => "<1 ms".to_string(),
        ms => format!("{ms} ms"),
    }
}

fn ago(ts: i64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    let secs = now.map_or(0, |d| d.as_secs() as i64).saturating_sub(ts);
    match secs {
        _ if ts == 0 => "never".into(),
        ..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86400),
    }
}

/// Every frame in the app: rounded corners, one cell of margin inside. Focus
/// is shown by colour alone (accent border and title), never by a heavier line.
fn panel<'a>(title: &str, focused: bool, th: &Theme) -> Block<'a> {
    let (border, text) = match focused {
        true => (th.accent, th.accent),
        false => (th.border, th.dim),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(text).bold(),
        ))
        .padding(Padding::horizontal(1))
}

fn selection(th: &Theme) -> Style {
    Style::new().bg(th.selection).add_modifier(Modifier::BOLD)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let (w, h) = (width.min(area.width), height.min(area.height));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

pub fn draw(app: &mut App, f: &mut Frame) {
    let [bar, body, keys, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    app.body = (body.width, body.height);
    f.render_widget(
        Block::new().style(Style::new().fg(app.cfg.theme.text)),
        f.area(),
    );
    header(app, f, bar);
    let th = &app.cfg.theme;
    match app.active.checked_sub(1).map(|i| &mut app.tabs[i].kind) {
        None => hosts(app, f, body),
        Some(Kind::Term { .. }) => sessions(app, f, body),
        Some(Kind::Sftp(view)) => files(view, th, f, body),
    }
    hints(app, f, keys);
    status_line(app, f, status);
    if app.leader != Leader::Off {
        which_key(app, f, body);
    }
    if let Some(p) = &app.popup {
        popup(p, &app.cfg.theme, f);
    }
}

// Mode colours of the status line, from the TokyoNight palette.
const BLUE: Color = Color::Rgb(122, 162, 247);
const GREEN: Color = Color::Rgb(158, 206, 106);
const TEAL: Color = Color::Rgb(26, 188, 156);
const YELLOW: Color = Color::Rgb(224, 175, 104);
const MAGENTA: Color = Color::Rgb(187, 154, 247);
/// Also the border of the panes that receive broadcast input.
const BROADCAST: Color = Color::Rgb(255, 158, 100);
/// Text on the coloured segments.
const INK: Color = Color::Rgb(26, 27, 38);

/// The session view: one pane per slot of the layout, the active session
/// focused. A single pane uses the whole area without a border.
fn sessions(app: &mut App, f: &mut Frame, area: Rect) {
    let shown = app.shown();
    let focused = app.active - 1;
    for (slot, mut rect) in app.layout.rects(area).into_iter().enumerate() {
        let th = &app.cfg.theme;
        // One empty column between neighbours, so their borders do not double up.
        if rect.right() < area.right() {
            rect.width = rect.width.saturating_sub(1);
        }
        let Some(&i) = shown.get(slot) else {
            let empty = Paragraph::new("no session").style(Style::new().fg(th.dim));
            f.render_widget(empty.block(panel("empty", false, th)), rect);
            continue;
        };
        let tab = &mut app.tabs[i];
        let mut inner = rect;
        if app.layout != panes::Layout::Single {
            let mark = if tab.closed { " ✗" } else { "" };
            let rec = if tab.log.is_some() { " ● rec" } else { "" };
            let title = format!("{} {}{mark}{rec}", i + 2, tab.alias);
            let mut block = panel(&title, i == focused, th);
            if app.broadcast && !tab.closed {
                block = block.border_style(Style::new().fg(BROADCAST));
            }
            inner = block.inner(rect);
            f.render_widget(block, rect);
        }
        term_pane(tab, inner, f, i == focused);
    }
}

fn term_pane(tab: &mut Tab, area: Rect, f: &mut Frame, focused: bool) {
    let Kind::Term { parser, tx } = &mut tab.kind else {
        return;
    };
    // Keep the remote PTY the size of the pane.
    let size = (NonZeroU16::new(area.height), NonZeroU16::new(area.width));
    if let (Some(rows), Some(cols)) = size {
        if parser.screen().size() != (rows, cols) {
            parser.set_size(rows, cols);
            let _ = tx.send(SessionCmd::Resize(area.width, area.height));
        }
    }
    let screen = parser.screen();
    term::render(screen, area, f.buffer_mut());
    if focused && !screen.hide_cursor() && screen.scrollback() == 0 {
        let (row, col) = screen.cursor_position();
        f.set_cursor_position((area.x + col, area.y + row));
    }
}

/// Where the host list is: `hosts › web1 › forwards › /query`.
fn breadcrumb(app: &App) -> String {
    let mut crumbs = vec!["hosts".to_string()];
    let panel = match app.focus {
        Focus::Forwards => Some("forwards"),
        Focus::Snippets => Some("snippets"),
        _ => None,
    };
    if let (Some(panel), Some(alias)) = (panel, app.alias()) {
        crumbs.extend([alias, panel.to_string()]);
    }
    if app.focus == Focus::Search {
        crumbs.push(format!("/{}▏", app.filter));
    } else if !app.filter.is_empty() {
        crumbs.push(format!("/{}", app.filter));
    }
    crumbs.join(" › ")
}

fn header(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    f.render_widget(Block::new().style(Style::new().bg(th.bar)), area);
    let brand = Style::new().fg(Color::Black).bg(th.accent).bold();
    let mut spans = vec![Span::styled(" vssh ", brand), Span::raw(" ")];
    let titles = std::iter::once(breadcrumb(app)).chain(app.tabs.iter().map(|t| {
        let kind = match t.kind {
            Kind::Sftp(_) => "sftp:",
            Kind::Term { .. } => "",
        };
        format!("{kind}{}{}", t.alias, if t.closed { " ✗" } else { "" })
    }));
    for (i, title) in titles.enumerate() {
        let (number, text) = match i == app.active {
            true => (th.accent, Style::new().fg(th.text).bold()),
            false => (th.dim, Style::new().fg(th.dim)),
        };
        spans.push(Span::styled(
            format!(" {} ", i + 1),
            Style::new().fg(number),
        ));
        spans.push(Span::styled(format!("{title} "), text));
    }
    f.render_widget(Line::from(spans), area);

    let mut status = vec![];
    let mut push = |text: String, colour| {
        status.push(Span::styled(text, Style::new().fg(colour)));
        status.push(Span::styled(" · ", Style::new().fg(th.dim)));
    };
    if !app.marked.is_empty() {
        push(format!("{} marked", app.marked.len()), th.warn);
    }
    if !app.fwds.is_empty() {
        push(format!("{} forwarding", app.fwds.len()), th.ok);
    }
    if app.vault.is_some() {
        push("vault unlocked".into(), th.ok);
    }
    status.pop();
    status.push(Span::raw(" "));
    f.render_widget(Line::from(status).alignment(Alignment::Right), area);
}

/// One host, two lines: status dot, pin, alias and latency; then the
/// address, group and tag badges in a quieter tone.
fn host_item<'a>(app: &'a App, alias: &'a str, width: usize, selected: bool) -> ListItem<'a> {
    let th = &app.cfg.theme;
    let meta = app.meta.get(alias);
    let quiet = Style::new().fg(if selected { th.text } else { th.dim });
    let (dot, colour, latency) = match app.reach.get(alias) {
        Some(Some(ms)) => ("●", th.ok, latency(*ms)),
        Some(None) => ("●", th.error, "down".to_string()),
        None => ("○", th.dim, String::new()),
    };
    let pin = if meta.is_some_and(|m| m.pinned) {
        "★"
    } else {
        " "
    };
    let mark = if app.marked.contains(alias) {
        "✓"
    } else {
        " "
    };
    let mut first = vec![
        Span::styled(format!(" {dot} "), Style::new().fg(colour)),
        Span::styled(format!("{pin}{mark} "), Style::new().fg(th.warn)),
        Span::styled(alias, Style::new().fg(th.text).bold()),
    ];
    // Right-align the latency.
    let used: usize = first.iter().map(Span::width).sum();
    let gap = width.saturating_sub(used + latency.chars().count());
    first.push(Span::raw(" ".repeat(gap)));
    let latency_style = match app.reach.get(alias) {
        Some(None) => Style::new().fg(th.error),
        _ => quiet,
    };
    first.push(Span::styled(latency, latency_style));

    let mut tail = vec![];
    if let Some(grp) = meta.map(|m| m.grp.as_str()).filter(|g| !g.is_empty()) {
        tail.push(Span::styled("  ·  ", quiet));
        tail.push(Span::styled(grp, Style::new().fg(chip(grp))));
    }
    for tag in meta.into_iter().flat_map(tags) {
        tail.push(Span::raw(" "));
        tail.push(badge(tag, selected));
    }
    // A long address gives way to the group and tags, not the other way round.
    const INDENT: usize = 7;
    let tail_width: usize = tail.iter().map(Span::width).sum();
    let room = width.saturating_sub(INDENT + tail_width).max(12);
    let address = shorten(&address(app, alias), room);
    let mut second = vec![Span::styled(format!("{:INDENT$}{address}", ""), quiet)];
    second.extend(tail);
    ListItem::new(vec![Line::from(first), Line::from(second)])
}

/// Cuts text to `max` characters, ending in `…` if anything was cut.
fn shorten(text: &str, max: usize) -> String {
    match text.chars().count() > max {
        true => text
            .chars()
            .take(max.saturating_sub(1))
            .chain(['…'])
            .collect(),
        false => text.to_string(),
    }
}

/// `user@host[:port]` as the ssh config resolves it.
fn address(app: &App, alias: &str) -> String {
    let get = |key| app.sshc.get(alias, key);
    let mut out = get("User").map(|u| format!("{u}@")).unwrap_or_default();
    out.push_str(get("HostName").unwrap_or(alias));
    if let Some(port) = get("Port").filter(|p| *p != "22") {
        out.push_str(&format!(":{port}"));
    }
    out
}

fn hosts(app: &mut App, f: &mut Frame, area: Rect) {
    let split = Layout::horizontal([Constraint::Percentage(44), Constraint::Min(24)]).spacing(1);
    let [left, right] = split.areas(area);
    // The scroll offset lives in the app; borrow it out while the rows borrow the app.
    let mut state = std::mem::take(&mut app.list);
    state.select(Some(app.sel));
    let th = &app.cfg.theme;
    let on_list = matches!(app.focus, Focus::List | Focus::Search);

    // Inside the borders: one cell for the selection bar, one of padding on the right.
    let width = usize::from(left.width.saturating_sub(4));
    let items = app
        .hosts
        .iter()
        .enumerate()
        .map(|(i, alias)| host_item(app, alias, width, i == app.sel));
    let title = format!("Hosts {}", app.hosts.len());
    let block = panel(&title, on_list, th).padding(Padding::right(1));
    let list = List::new(items)
        .block(block)
        .highlight_symbol(Line::styled("▎", th.accent))
        .repeat_highlight_symbol(true)
        .highlight_style(Style::new().bg(th.selection));
    f.render_stateful_widget(list, left, &mut state);
    app.list = state;

    let Some(alias) = app.alias() else {
        let k = &app.cfg.keys;
        let hint = match app.filter.is_empty() {
            true => format!(
                "No hosts yet.\n\n{} adds one to ~/.ssh/config.\n{} connects once without saving.",
                k.add, k.connect
            ),
            false => "No host matches the search.".to_string(),
        };
        let hint = Paragraph::new(hint).style(Style::new().fg(app.cfg.theme.dim));
        let block = panel("Details", false, &app.cfg.theme);
        return f.render_widget(hint.block(block.padding(Padding::uniform(1))), right);
    };
    match app.focus {
        Focus::Forwards => forwards(app, &alias, f, right),
        Focus::Snippets => snippets(app, &alias, f, right),
        _ => details(app, &alias, f, right),
    }
}

fn details(app: &App, alias: &str, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let meta = app.meta.get(alias).cloned().unwrap_or_default();
    let get = |key| app.sshc.get(alias, key).unwrap_or("").to_string();
    let or = |s: String, d: &str| if s.is_empty() { d.to_string() } else { s };
    let dim = Style::new().fg(th.dim);
    let section = |name: &str| Line::styled(name.to_string(), Style::new().fg(th.accent).bold());
    let row = |label: &str, value: Vec<Span<'static>>| {
        let mut spans = vec![Span::styled(format!("  {label:<11}"), dim)];
        spans.extend(value);
        Line::from(spans)
    };
    let text = |s: String| vec![Span::raw(s)];

    let (dot, state, colour) = match app.reach.get(alias) {
        Some(Some(ms)) => ("●", format!("reachable · {}", latency(*ms)), th.ok),
        Some(None) => ("●", "unreachable".to_string(), th.error),
        None => ("○", "not checked".to_string(), th.dim),
    };
    let mut lines = vec![
        Line::styled(address(app, alias), Style::new().fg(th.text).bold()),
        Line::styled(format!("{dot} {state}"), Style::new().fg(colour)),
        Line::default(),
        section("CONNECTION"),
        row("Address", text(or(get("HostName"), alias))),
        row("Port", text(or(get("Port"), "22"))),
        row("User", text(or(get("User"), "(current user)"))),
        row(
            "Key",
            text(or(get("IdentityFile"), "(agent / default keys)")),
        ),
    ];
    for key in ["ProxyJump", "ProxyCommand"] {
        if !get(key).is_empty() {
            let note = Span::styled("  not supported", Style::new().fg(th.error));
            lines.push(row(key, vec![Span::raw(get(key)), note]));
        }
    }

    lines.extend([Line::default(), section("ORGANISATION")]);
    let group = match meta.grp.is_empty() {
        true => Span::styled("—", dim),
        false => Span::styled(meta.grp.clone(), Style::new().fg(chip(&meta.grp)).bold()),
    };
    lines.push(row("Group", vec![group]));
    let mut badges: Vec<Span> = tags(&meta)
        .flat_map(|t| [badge(t, false), Span::raw(" ")])
        .collect();
    if badges.is_empty() {
        badges.push(Span::styled("—", dim));
    }
    lines.push(row("Tags", badges));
    if !meta.notes.is_empty() {
        lines.push(row("Notes", text(meta.notes.clone())));
    }

    lines.extend([Line::default(), section("ACTIVITY")]);
    lines.push(row("Last used", text(ago(meta.last_connected))));
    lines.push(row("Sessions", text(meta.connects.to_string())));
    let vault = match &app.vault {
        Some(v) if v.get(alias).is_some() => {
            Span::styled("password stored", Style::new().fg(th.ok))
        }
        Some(_) => Span::raw("no password"),
        None => Span::styled("locked", dim),
    };
    lines.push(row("Vault", vec![vault]));

    let forwards = app.forwards(alias);
    if !forwards.is_empty() {
        lines.extend([Line::default(), section("FORWARDS")]);
    }
    for (kind, value) in forwards {
        let state = match app.fwds.get(&fwd_key(alias, kind, &value)) {
            Some((_, true)) => Span::styled("● active", Style::new().fg(th.ok)),
            Some((_, false)) => Span::styled("◌ connecting", Style::new().fg(th.warn)),
            None => Span::styled("○ stopped", dim),
        };
        let spec = Span::raw(format!("  {kind} {value}  "));
        lines.push(Line::from(vec![spec, state]));
    }
    let block = panel(alias, false, th).padding(Padding::new(1, 1, 1, 0));
    let details = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(details.block(block), area);
}

fn sub_list(app: &App, title: &str, mut items: Vec<Line<'static>>, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    if items.is_empty() {
        items.push(Line::styled("nothing here yet, press a to add", th.dim));
    }
    let list = List::new(items)
        .block(panel(title, true, th))
        .highlight_style(selection(th));
    let mut state = ListState::default().with_selected(Some(app.sub_sel));
    f.render_stateful_widget(list, area, &mut state);
}

fn forwards(app: &App, alias: &str, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let items = app.forwards(alias).into_iter().map(|(kind, value)| {
        let (dot, state, colour) = match app.fwds.get(&fwd_key(alias, kind, &value)) {
            Some((_, true)) => ("●", "active", th.ok),
            Some((_, false)) => ("◌", "connecting", th.warn),
            None => ("○", "stopped", th.dim),
        };
        Line::from(vec![
            Span::styled(format!("{dot} "), Style::new().fg(colour)),
            Span::raw(format!("{kind} {value:<34}")),
            Span::styled(state, Style::new().fg(colour)),
        ])
    });
    sub_list(
        app,
        &format!("Forwards of {alias}"),
        items.collect(),
        f,
        area,
    );
}

fn snippets(app: &App, alias: &str, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let items = app.host_snippets(alias).into_iter().map(|s| {
        let scope = if s.host.is_empty() { "all " } else { "host" };
        Line::from(vec![
            Span::styled(format!("{scope} "), Style::new().fg(th.dim)),
            Span::styled(format!("{:<18} ", s.name), Style::new().fg(th.accent)),
            Span::raw(s.command.clone()),
        ])
    });
    let targets = match app.marked.len() {
        0 => alias.to_string(),
        n => format!("{n} marked hosts"),
    };
    sub_list(
        app,
        &format!("Snippets → {targets}"),
        items.collect(),
        f,
        area,
    );
}

fn files(view: &SftpView, th: &Theme, f: &mut Frame, area: Rect) {
    let [panes, gauge] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let split = Layout::horizontal([Constraint::Percentage(50); 2]).spacing(1);
    let halves: [Rect; 2] = split.areas(panes);
    for side in [LOCAL, REMOTE] {
        let items = view.lists[side].iter().map(|e| match e.is_dir {
            true => Line::styled(
                format!("{}/", clean(&e.name)),
                Style::new().fg(th.accent).bold(),
            ),
            false => Line::from(vec![
                Span::raw(clean(&e.name)),
                Span::styled(format!("  {}", human(e.size)), Style::new().fg(th.dim)),
            ]),
        });
        let name = ["local", "remote"][side];
        let title = format!("{name}  {}", clean(&view.dirs[side]));
        let list = List::new(items)
            .block(panel(&title, side == view.side, th))
            .highlight_style(match side == view.side {
                true => selection(th),
                false => Style::new(),
            });
        let mut state = ListState::default().with_selected(Some(view.sel[side]));
        f.render_stateful_widget(list, halves[side], &mut state);
    }
    if let Some((done, total)) = view.progress {
        let bar = Gauge::default()
            .gauge_style(Style::new().fg(th.accent).bg(th.selection))
            .ratio(if total == 0 {
                1.0
            } else {
                (done as f64 / total as f64).min(1.0)
            })
            .label(format!("{} / {}", human(done), human(total)));
        f.render_widget(bar, gauge);
    }
}

/// One key hint. `on` is false when the key does nothing right now.
struct Hint {
    key: String,
    text: &'static str,
    on: bool,
}

/// The keys of whatever has focus, in groups. English and Vietnamese sit
/// side by side so a new hint cannot be added in one language only.
fn hint_groups(app: &App) -> Vec<Vec<Hint>> {
    let k = &app.cfg.keys;
    let lang = app.cfg.lang;
    let hint = |key: &str, en: &'static str, vi: &'static str, on: bool| Hint {
        key: key.to_string(),
        text: lang.pick(en, vi),
        on,
    };
    let key = |c: char| match c {
        ' ' => "space".to_string(),
        c => c.to_string(),
    };
    let leader = format!("^{}", k.leader);
    match (
        app.active.checked_sub(1).map(|i| &app.tabs[i].kind),
        app.focus,
    ) {
        (Some(Kind::Term { .. }), _) => {
            let panes = app.terms().len().min(app.layout.panes());
            let live = app.live_sessions();
            let broadcast = match app.broadcast {
                true => hint("alt-b", "broadcast ON", "đang phát", true),
                false => hint("alt-b", "broadcast", "phát tới mọi phiên", live > 1),
            };
            vec![
                vec![hint(&leader, "menu", "menu", true)],
                vec![
                    hint("alt-hjkl", "pane", "khung", panes > 1),
                    hint(&format!("{leader} L"), "layout", "bố cục", true),
                    hint(&format!("{leader} d"), "close", "đóng", true),
                ],
                vec![broadcast],
                vec![
                    hint(&format!("{leader} tab"), "next tab", "tab kế", true),
                    hint(&format!("{leader} q"), "hosts", "máy chủ", true),
                ],
            ]
        }
        (Some(Kind::Sftp(view)), _) => {
            let entry = view.selected();
            let file = entry.is_some_and(|e| !e.is_dir) && view.progress.is_none();
            vec![
                vec![
                    hint("tab", "pane", "khung", true),
                    hint("enter", "open", "mở", entry.is_some_and(|e| e.is_dir)),
                    hint("h", "up", "lên", true),
                ],
                vec![
                    hint("c", "copy", "chép", file),
                    hint("m", "mkdir", "tạo thư mục", true),
                    hint("r", "rename", "đổi tên", entry.is_some()),
                    hint("d", "delete", "xoá", entry.is_some()),
                ],
                vec![
                    hint("q", "close", "đóng", true),
                    hint("Q", "quit", "thoát", true),
                ],
            ]
        }
        (None, Focus::Search) => vec![vec![
            hint("type", "filter", "lọc", true),
            hint("#tag", "tag or group", "thẻ hoặc nhóm", true),
            hint("enter", "keep", "giữ", true),
            hint("esc", "clear", "xoá", true),
        ]],
        (None, focus @ (Focus::Forwards | Focus::Snippets)) => {
            let alias = app.alias().unwrap_or_default();
            let (any, action) = match focus {
                Focus::Forwards => (
                    !app.forwards(&alias).is_empty(),
                    hint("enter", "start/stop", "bật/tắt", true),
                ),
                _ => (
                    !app.host_snippets(&alias).is_empty(),
                    hint("enter", "run", "chạy", true),
                ),
            };
            vec![
                vec![
                    hint("a", "add", "thêm", true),
                    hint("e", "edit", "sửa", any && focus == Focus::Snippets),
                    hint("d", "delete", "xoá", any),
                    Hint { on: any, ..action },
                ],
                vec![hint("esc", "back", "quay lại", true)],
            ]
        }
        (None, Focus::List) => {
            let host = app.alias().is_some();
            let marked = !app.marked.is_empty();
            let enter = match marked {
                true => hint("enter", "connect marked", "kết nối máy đã đánh dấu", true),
                false => hint("enter", "shell", "shell", host),
            };
            vec![
                vec![
                    enter,
                    hint(&key(k.sftp), "sftp", "sftp", host),
                    hint(&key(k.mark), "mark", "đánh dấu", host),
                    hint(&key(k.connect_tag), "by tag", "theo thẻ", host),
                    hint(&key(k.connect), "quick", "nhanh", true),
                ],
                vec![
                    hint(&key(k.forwards), "forwards", "chuyển cổng", host),
                    hint(&key(k.snippets), "snippets", "snippet", host),
                    hint(&key(k.add), "add", "thêm", true),
                    hint(&key(k.edit), "edit", "sửa", host),
                ],
                vec![
                    hint(&key(k.search), "search", "tìm", true),
                    hint(&key(k.ping), "ping", "kiểm tra", true),
                    hint(&key(k.help), "help", "trợ giúp", true),
                    hint(&key(k.quit), "quit", "thoát", true),
                ],
            ]
        }
    }
}

/// The line above the status line: a message if there is one, otherwise the
/// key hints. Hints that do nothing right now are faded.
fn hints(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    if !app.msg.is_empty() {
        let msg = format!(" {}", clean(&app.msg).replace('\n', " "));
        return f.render_widget(Line::styled(msg, th.accent), area);
    }
    let faded = Style::new().fg(th.dim).add_modifier(Modifier::DIM);
    let mut spans = vec![];
    for (i, group) in hint_groups(app).into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │", Style::new().fg(th.border)));
        }
        for hint in group {
            let (name, action) = match hint.on {
                true => (Style::new().fg(th.key).bold(), Style::new().fg(th.dim)),
                false => (faded, faded),
            };
            spans.push(Span::styled(format!(" {}", hint.key), name));
            spans.push(Span::styled(format!(" {} ", hint.text), action));
        }
    }
    f.render_widget(Line::from(spans), area);
}

/// Name and colour of the current mode, shown at the left of the status line.
fn mode(app: &App) -> (&'static str, Color) {
    let tab = app.active.checked_sub(1).map(|i| &app.tabs[i].kind);
    match (tab, app.focus) {
        _ if app.leader != Leader::Off => ("LEADER", MAGENTA),
        (Some(Kind::Term { .. }), _) if app.broadcast => ("BROADCAST", BROADCAST),
        (Some(Kind::Term { .. }), _) => ("TERMINAL", GREEN),
        (Some(Kind::Sftp(_)), _) => ("SFTP", TEAL),
        (None, Focus::Search) => ("SEARCH", YELLOW),
        (None, _) => ("NORMAL", BLUE),
    }
}

/// Bottom line, laid out like lualine: mode and context on the left, the
/// host and pane in the middle, tags, group and latency on the right.
fn status_line(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let lang = app.cfg.lang;
    let (name, colour) = mode(app);
    let badge = Style::new().fg(INK).bg(colour).bold();
    let section = Style::new().fg(th.text).bg(th.selection);
    let plain = Style::new().fg(th.dim).bg(th.bar);
    f.render_widget(Block::new().style(plain), area);

    // What the line is about: the focused tab, or the selected host.
    let tab = app.active.checked_sub(1).map(|i| &app.tabs[i]);
    let alias = tab.map(|t| t.alias.clone()).or_else(|| app.alias());
    let alias = alias.unwrap_or_default();
    let sessions = app.live_sessions();
    let (context, middle) = match tab.map(|t| &t.kind) {
        Some(Kind::Term { .. }) => {
            let terms = app.terms();
            let at = terms.iter().position(|i| i + 1 == app.active).unwrap_or(0);
            let shown = terms
                .len()
                .saturating_sub(app.window)
                .min(app.layout.panes());
            let pane = at.saturating_sub(app.window) + 1;
            let word = lang.pick("pane", "khung");
            (
                app.layout.name().to_string(),
                format!("{alias} : {word} {pane}/{shown}"),
            )
        }
        Some(Kind::Sftp(view)) => {
            let side = [
                lang.pick("local", "máy mình"),
                lang.pick("remote", "máy từ xa"),
            ];
            (side[view.side].to_string(), format!("{alias} : sftp"))
        }
        None => {
            let word = lang.pick("hosts", "máy chủ");
            (format!("{} {word}", app.hosts.len()), alias.clone())
        }
    };
    let mut left = vec![
        Span::styled(format!(" {name} "), badge),
        Span::styled(format!(" {context} "), section),
    ];
    if tab.is_some_and(|t| t.log.is_some()) {
        let rec = Style::new().fg(th.error).bg(th.bar).bold();
        left.push(Span::styled(" ● REC", rec));
    }
    if app.broadcast {
        let word = lang.pick("sessions", "phiên");
        // In a session the mode badge already says BROADCAST.
        let label = if name == "BROADCAST" {
            ""
        } else {
            "BROADCAST "
        };
        let text = format!(" {label}→ {sessions} {word} ");
        left.push(Span::styled(
            text,
            Style::new().fg(BROADCAST).bg(th.bar).bold(),
        ));
    }

    let meta = app.meta.get(&alias).filter(|_| tab.is_none_or(|t| t.saved));
    let mut right = vec![];
    if let Some(meta) = meta {
        let all: Vec<&str> = tags(meta).collect();
        if !all.is_empty() {
            right.push(Span::styled(format!("{} ", all.join(" ")), plain));
        }
        if !meta.grp.is_empty() {
            right.push(Span::styled(format!(" {} ", meta.grp), section));
        }
    }
    let reach = match app
        .reach
        .get(&alias)
        .filter(|_| tab.is_none_or(|t| t.saved))
    {
        Some(Some(ms)) => latency(*ms),
        Some(None) => lang.pick("down", "mất kết nối").to_string(),
        None => "—".to_string(),
    };
    right.push(Span::styled(format!(" {reach} "), badge));

    f.render_widget(
        Line::styled(middle, plain).alignment(Alignment::Center),
        area,
    );
    f.render_widget(Line::from(left), area);
    f.render_widget(Line::from(right).alignment(Alignment::Right), area);
}

/// The leader menu: key, English, Vietnamese. Keep in step with
/// `App::leader_key` and `App::window_key`.
fn leader_menu(app: &App) -> (&'static str, Vec<(String, &'static str, &'static str)>) {
    let leader = format!("^{}", app.cfg.keys.leader);
    let entry = |key: &str, en, vi| (key.to_string(), en, vi);
    match app.leader {
        Leader::Window => (
            "leader › window",
            vec![
                entry("h j k l", "focus pane", "chuyển khung"),
                entry("s", "single pane", "một khung"),
                entry("v", "2 columns", "2 cột"),
                entry("g", "2x2 grid", "lưới 2x2"),
                entry("t", "3 columns", "3 cột"),
                entry("L", "next layout", "bố cục kế"),
                entry("d", "close pane", "đóng khung"),
                entry("esc", "cancel", "huỷ"),
            ],
        ),
        _ => (
            "leader",
            vec![
                entry("← ↓ ↑ →", "focus pane", "chuyển khung"),
                entry("L", "next layout", "bố cục kế"),
                entry("w", "+window", "+cửa sổ"),
                entry("b", "broadcast", "phát tới mọi phiên"),
                entry("l", "log session", "ghi log phiên"),
                entry("s", "type a snippet", "gõ snippet"),
                entry("d", "close pane", "đóng khung"),
                entry("q", "host list", "danh sách máy chủ"),
                entry("tab n", "next tab", "tab kế tiếp"),
                entry("p", "previous tab", "tab trước"),
                entry("1-9", "go to tab", "tới tab số"),
                entry("pgup pgdn", "scroll", "cuộn"),
                entry(&leader, "send it to the remote", "gửi tới máy từ xa"),
                entry("esc", "cancel", "huỷ"),
            ],
        ),
    }
}

/// Which-key popup: after the leader, lists what the next key does. Sits at
/// the bottom right of the body, in two columns.
fn which_key(app: &App, f: &mut Frame, body: Rect) {
    let th = &app.cfg.theme;
    let (title, entries) = leader_menu(app);
    let rows = entries.len().div_ceil(2);
    let width = 76.min(body.width);
    let height = (rows as u16 + 2).min(body.height);
    let area = Rect::new(
        body.x + body.width - width,
        body.y + body.height - height,
        width,
        height,
    );
    let column = usize::from(width.saturating_sub(4)) / 2;
    let cell = |i: usize| -> Vec<Span> {
        let Some((key, en, vi)) = entries.get(i) else {
            return vec![];
        };
        let text = app.cfg.lang.pick(en, vi);
        // Groups, marked with `+`, open another menu.
        let colour = if text.starts_with('+') {
            th.accent
        } else {
            th.text
        };
        let used = key.chars().count() + 3 + text.chars().count();
        vec![
            Span::styled(key.clone(), Style::new().fg(th.key).bold()),
            Span::styled(" → ", Style::new().fg(th.dim)),
            Span::styled(text, Style::new().fg(colour)),
            Span::raw(" ".repeat(column.saturating_sub(used))),
        ]
    };
    let lines: Vec<Line> = (0..rows)
        .map(|r| Line::from([cell(r), cell(r + rows)].concat()))
        .collect();
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(panel(title, true, th)), area);
}

fn popup(popup: &Popup, th: &Theme, f: &mut Frame) {
    let screen = f.area();
    match popup {
        Popup::Confirm(text, _) => {
            let area = centered(screen, text.chars().count() as u16 + 8, 5);
            let lines = vec![
                Line::raw(""),
                Line::raw(text.as_str()),
                Line::from(vec![
                    Span::styled("y", Style::new().fg(th.key).bold()),
                    Span::styled(" yes   ", Style::new().fg(th.dim)),
                    Span::styled("n", Style::new().fg(th.key).bold()),
                    Span::styled(" no", Style::new().fg(th.dim)),
                ]),
            ];
            let body = Paragraph::new(lines).alignment(Alignment::Center);
            f.render_widget(Clear, area);
            f.render_widget(body.block(panel("Confirm", true, th)), area);
        }
        Popup::Text {
            title,
            body,
            scroll,
        } => {
            let area = centered(screen, screen.width * 9 / 10, screen.height * 9 / 10);
            let text = Paragraph::new(body.as_str())
                .wrap(Wrap { trim: false })
                .scroll((*scroll, 0));
            f.render_widget(Clear, area);
            f.render_widget(text.block(panel(title, true, th)), area);
        }
        Popup::Pick { items, sel } => {
            let height = (items.len() as u16 + 2).min(screen.height * 2 / 3);
            let area = centered(screen, 78, height);
            let rows = items.iter().map(|(name, command)| {
                Line::from(vec![
                    Span::styled(format!("{name:<20} "), Style::new().fg(th.accent)),
                    Span::raw(command.as_str()),
                ])
            });
            let list = List::new(rows)
                .block(panel("Type a snippet into the session", true, th))
                .highlight_style(selection(th));
            let mut state = ListState::default().with_selected(Some(*sel));
            f.render_widget(Clear, area);
            f.render_stateful_widget(list, area, &mut state);
        }
        Popup::Form(form) => {
            let label_w = form.fields.iter().map(|f| f.0.chars().count()).max();
            let label_w = label_w.unwrap_or(0) + 2;
            let area = centered(screen, 78, form.fields.len() as u16 + 5);
            // Borders, padding and one cell for the cursor.
            let room = usize::from(area.width).saturating_sub(label_w + 5);
            let mut cursor = (area.x, area.y);
            let mut lines = vec![Line::default()];
            for (i, (label, value)) in form.fields.iter().enumerate() {
                // Show the tail of a value that does not fit.
                let len = value.chars().count();
                let secret = form.masked && i + 1 == form.fields.len();
                let shown: String = match secret {
                    true => "•".repeat(len.min(room)),
                    false => value.chars().skip(len.saturating_sub(room)).collect(),
                };
                let style = match i == form.idx {
                    true => Style::new().fg(th.accent).bold(),
                    false => Style::new().fg(th.dim),
                };
                if i == form.idx {
                    let x = area.x + 2 + (label_w + shown.chars().count()) as u16;
                    cursor = (x, area.y + 2 + i as u16);
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{label:<label_w$}"), style),
                    Span::raw(shown),
                ]));
            }
            let hints: &[(&str, &str)] = match form.fields.len() {
                1 => &[("enter", "ok"), ("esc", "cancel")],
                _ => &[("tab", "next field"), ("enter", "ok"), ("esc", "cancel")],
            };
            lines.push(Line::default());
            lines.push(Line::from_iter(hints.iter().flat_map(|(name, action)| {
                [
                    Span::styled(*name, Style::new().fg(th.key).bold()),
                    Span::styled(format!(" {action}   "), Style::new().fg(th.dim)),
                ]
            })));
            f.render_widget(Clear, area);
            f.render_widget(
                Paragraph::new(lines).block(panel(&form.title, true, th)),
                area,
            );
            f.set_cursor_position(cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_and_ages() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1023), "1023 B");
        assert_eq!(human(1536), "1.5 K");
        assert_eq!(human(5 * 1024 * 1024), "5.0 M");
        assert_eq!(human(3 * 1024 * 1024 * 1024 * 1024), "3.0 T");
        assert_eq!((latency(0), latency(42)), ("<1 ms".into(), "42 ms".into()));
        assert_eq!(ago(0), "never");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now = now.as_secs() as i64;
        assert_eq!(ago(now), "just now");
        assert_eq!(ago(now - 150), "2 min ago");
        assert_eq!(ago(now - 7300), "2 h ago");
        assert_eq!(ago(now - 3 * 86400), "3 d ago");
    }

    #[test]
    fn chips_are_stable_and_popups_fit() {
        assert_eq!(chip("prod"), chip("prod"));
        assert_ne!(chip("prod"), chip("staging"));
        assert_eq!(shorten("root@web-01.example.com", 12), "root@web-01…");
        assert_eq!(shorten("short", 12), "short");
        assert_eq!(shorten("héllo wörld", 5), "héll…");
        let small = Rect::new(0, 0, 20, 4);
        assert_eq!(centered(small, 76, 11), small);
        assert_eq!(
            centered(Rect::new(0, 0, 100, 40), 76, 10),
            Rect::new(12, 15, 76, 10)
        );
    }
}
