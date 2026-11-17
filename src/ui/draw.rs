//! Rendering: header bar, body (host list + details, a session, or SFTP
//! panes), bottom bar with the keys of the focused view.

use std::num::NonZeroU16;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::Frame;

use super::files::{SftpView, LOCAL, REMOTE};
use super::panes;
use super::panes::Shape;
use super::stats::Level;
use super::{clean, fwd_key, term, App, Focus, Kind, Leader, Popup, Tab};
use crate::config::{Lang, Theme};
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

fn ago(ts: i64, lang: Lang) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    let secs = now.map_or(0, |d| d.as_secs() as i64).saturating_sub(ts);
    let unit = |n: i64, en: &str, vi: &str| format!("{n} {}", lang.pick(en, vi));
    match secs {
        _ if ts == 0 => lang.pick("never", "chưa lần nào").into(),
        ..60 => lang.pick("just now", "vừa xong").into(),
        60..3600 => unit(secs / 60, "min ago", "phút trước"),
        3600..86400 => unit(secs / 3600, "h ago", "giờ trước"),
        _ => unit(secs / 86400, "d ago", "ngày trước"),
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

/// A rounded badge for a one-line bar: `text` in the `badge` style between
/// two half circles. `sides` is what lies left and right of it, so the caps
/// blend into the bar. The caps stand where a square badge has its padding:
/// a pill is exactly as wide as ` text `.
fn pill(text: &str, badge: Style, sides: [Color; 2], nerd: bool) -> [Span<'static>; 3] {
    let (left, right) = match nerd {
        true => ("\u{e0b6}", "\u{e0b4}"),
        false => ("◖", "◗"),
    };
    let cap = |bg| Style::new().fg(badge.bg.unwrap_or(Color::Reset)).bg(bg);
    [
        Span::styled(left, cap(sides[0])),
        Span::styled(text.to_string(), badge),
        Span::styled(right, cap(sides[1])),
    ]
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

/// Frames a tab takes to slide in when it is switched to.
pub const SLIDE: u16 = 3;
/// How long one of those frames stays up: three frames, 120 ms.
pub const FRAME: Duration = Duration::from_millis(40);

/// Takes one frame off the countdown and returns the columns to shift it by:
/// 2, 1, then 0 for good. While frames are left the main loop redraws on a
/// timer, so the countdown must only reach zero with the unshifted frame.
fn slide_step(left: &mut u16) -> u16 {
    *left = left.saturating_sub(1);
    *left
}

pub fn draw(app: &mut App, f: &mut Frame) {
    let screen = f.area();
    app.shape = Shape::of(screen.width, screen.height, app.cfg.layout);
    // The hints take a second row when they need one and the window can spare it.
    let rows = if screen.height >= 16 { 2 } else { 1 };
    let hint_rows = flow(hint_groups(app), screen.width.into(), rows);
    let [bar, body, keys, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(hint_rows.len().max(1) as u16),
        Constraint::Length(1),
    ])
    .areas(screen);
    app.body = (body.width, body.height);
    f.render_widget(
        Block::new().style(Style::new().fg(app.cfg.theme.text)),
        screen,
    );
    header(app, f, bar);
    let th = &app.cfg.theme;
    let tall = app.shape == Shape::Tall;
    // A tab that was just switched to slides in from the right. Only the
    // drawing moves: the sizes the rest of the app works with stay `body`.
    let shift = slide_step(&mut app.slide).min(body.width.saturating_sub(1));
    let view = Rect {
        x: body.x + shift,
        width: body.width - shift,
        ..body
    };
    match app.active.checked_sub(1).map(|i| &mut app.tabs[i].kind) {
        None => hosts(app, f, view),
        Some(Kind::Term { .. }) => sessions(app, f, view, shift == 0),
        Some(Kind::Sftp(files_view)) => files(files_view, th, app.cfg.lang, tall, f, view),
    }
    hints(app, hint_rows, f, keys);
    status_line(app, f, status);
    if app.leader != Leader::Off {
        which_key(app, f, body);
    }
    if let Some(p) = &app.popup {
        popup(p, &app.cfg.theme, app.cfg.lang, f);
    }
}

// Mode colours of the status line, from the TokyoNight palette.
const BLUE: Color = Color::Rgb(122, 162, 247);
const GREEN: Color = Color::Rgb(158, 206, 106);
const TEAL: Color = Color::Rgb(26, 188, 156);
const YELLOW: Color = Color::Rgb(224, 175, 104);
const MAGENTA: Color = Color::Rgb(187, 154, 247);
const RED: Color = Color::Rgb(247, 118, 142);
/// Also the border of the panes that receive broadcast input.
const BROADCAST: Color = Color::Rgb(255, 158, 100);
/// Text on the coloured segments.
const INK: Color = Color::Rgb(26, 27, 38);

/// The session view: one pane per slot of the layout, the active session
/// focused. A single pane uses the whole area without a border. `settled` is
/// false for the frames of a slide, which must not resize the remote PTYs.
fn sessions(app: &mut App, f: &mut Frame, area: Rect, settled: bool) {
    let shown = app.shown();
    let focused = app.active - 1;
    // Portrait: every pane gets the full width, one above the other.
    let stacked = app.shape == Shape::Tall;
    for (slot, mut rect) in app.layout.rects(area, stacked).into_iter().enumerate() {
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
        term_pane(tab, inner, f, i == focused, settled);
    }
}

fn term_pane(tab: &mut Tab, area: Rect, f: &mut Frame, focused: bool, settled: bool) {
    let Kind::Term { parser, tx } = &mut tab.kind else {
        return;
    };
    // Keep the remote PTY the size of the pane.
    let size = (NonZeroU16::new(area.height), NonZeroU16::new(area.width));
    if let (Some(rows), Some(cols), true) = (size.0, size.1, settled) {
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
    let lang = app.cfg.lang;
    let mut crumbs = vec![lang.pick("hosts", "máy chủ").to_string()];
    let panel = match app.focus {
        Focus::Forwards => Some(lang.pick("forwards", "chuyển tiếp cổng")),
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

/// Index of the first tab to draw so that the active one is on screen:
/// tabs before it are dropped from the left until it fits in `room` cells.
fn tab_window(widths: &[usize], active: usize, room: usize) -> usize {
    let mut first = 0;
    while first < active && widths[first..=active].iter().sum::<usize>() > room {
        first += 1;
    }
    first
}

fn header(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    f.render_widget(Block::new().style(Style::new().bg(th.bar)), area);
    let brand = Style::new().fg(Color::Black).bg(th.accent).bold();
    let mut spans = pill("vssh", brand, [th.bar; 2], app.cfg.nerd_font).to_vec();
    spans.push(Span::raw(" "));
    let titles = std::iter::once(breadcrumb(app)).chain(app.tabs.iter().map(|t| {
        let kind = match t.kind {
            Kind::Sftp(_) => "sftp:",
            Kind::Term { .. } => "",
        };
        format!("{kind}{}{}", t.alias, if t.closed { " ✗" } else { "" })
    }));
    let tabs: Vec<(String, String)> = titles
        .enumerate()
        .map(|(i, title)| (format!(" {} ", i + 1), format!("{title} ")))
        .collect();
    // On a narrow screen the tab bar scrolls to keep the active tab visible.
    let widths: Vec<usize> = tabs
        .iter()
        .map(|(n, t)| n.chars().count() + t.chars().count())
        .collect();
    let room = usize::from(area.width).saturating_sub(9);
    let first = tab_window(&widths, app.active.min(widths.len() - 1), room);
    if first > 0 {
        spans.push(Span::styled("‹", Style::new().fg(th.dim)));
    }
    for (i, (number, title)) in tabs.into_iter().enumerate().skip(first) {
        // The active tab is a band of its own, like the selected row of a list.
        let (number_style, text) = match i == app.active {
            true => (
                Style::new().fg(th.accent).bg(th.selection).bold(),
                Style::new().fg(th.text).bg(th.selection).bold(),
            ),
            false => (Style::new().fg(th.dim), Style::new().fg(th.dim)),
        };
        spans.push(Span::styled(number, number_style));
        spans.push(Span::styled(title, text));
    }
    let used: usize = spans.iter().map(Span::width).sum();
    f.render_widget(Line::from(spans), area);

    let mut status = vec![];
    let mut push = |text: String, colour| {
        status.push(Span::styled(text, Style::new().fg(colour)));
        status.push(Span::styled(" · ", Style::new().fg(th.dim)));
    };
    if !app.marked.is_empty() {
        let word = app.cfg.lang.pick("marked", "đã đánh dấu");
        push(format!("{} {word}", app.marked.len()), th.warn);
    }
    if !app.fwds.is_empty() {
        let word = app.cfg.lang.pick("forwarding", "cổng đang chuyển tiếp");
        push(format!("{} {word}", app.fwds.len()), th.ok);
    }
    if app.vault.is_some() {
        push(
            app.cfg.lang.pick("vault unlocked", "kho đã mở").into(),
            th.ok,
        );
    }
    status.pop();
    status.push(Span::raw(" "));
    // Only where it does not cover the tabs.
    let wanted: usize = status.iter().map(Span::width).sum();
    if used + wanted <= usize::from(area.width) {
        f.render_widget(Line::from(status).alignment(Alignment::Right), area);
    }
}

/// One host. Two lines where there is room (status dot, pin, alias and
/// latency; then address, group and tag badges in a quieter tone), one line
/// when `compact`. Group and tags always win over the address for space.
fn host_item<'a>(
    app: &'a App,
    alias: &'a str,
    width: usize,
    selected: bool,
    compact: bool,
) -> ListItem<'a> {
    let th = &app.cfg.theme;
    let meta = app.meta.get(alias);
    let quiet = Style::new().fg(if selected { th.text } else { th.dim });
    let (dot, colour, latency) = match app.reach.get(alias) {
        Some(Some(ms)) => ("●", th.ok, latency(*ms)),
        Some(None) => (
            "●",
            th.error,
            app.cfg.lang.pick("down", "mất kết nối").to_string(),
        ),
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
    let latency_style = match app.reach.get(alias) {
        Some(None) => Style::new().fg(th.error),
        _ => quiet,
    };
    // Group, then one piece per tag: dropped from the end when space runs out.
    let mut tail: Vec<[Span; 2]> = vec![];
    if let Some(grp) = meta.map(|m| m.grp.as_str()).filter(|g| !g.is_empty()) {
        let dot = Span::styled("  ·  ", quiet);
        tail.push([dot, Span::styled(grp, Style::new().fg(chip(grp)))]);
    }
    for tag in meta.into_iter().flat_map(tags) {
        tail.push([Span::raw(" "), badge(tag, selected)]);
    }
    let size = |tail: &[[Span; 2]]| tail.iter().flatten().map(Span::width).sum::<usize>();
    let used: usize = first.iter().map(Span::width).sum();
    let latency_width = latency.chars().count();

    if compact {
        while !tail.is_empty() && used + size(&tail) + 1 + latency_width > width {
            tail.pop();
        }
        first.extend(std::mem::take(&mut tail).into_iter().flatten());
    }
    // Right-align the latency.
    let used: usize = first.iter().map(Span::width).sum();
    let gap = width.saturating_sub(used + latency_width);
    first.push(Span::raw(" ".repeat(gap)));
    first.push(Span::styled(latency, latency_style));
    if compact {
        return ListItem::new(Line::from(first));
    }

    const INDENT: usize = 7;
    let room = width.saturating_sub(INDENT);
    while tail.len() > 1 && size(&tail) > room {
        tail.pop();
    }
    // The address gets what is left, and is dropped rather than cut to a stub.
    let left = room.saturating_sub(size(&tail));
    let address = match left >= 8 {
        true => shorten(&address(app, alias), left),
        false => String::new(),
    };
    let mut second = vec![Span::styled(format!("{:INDENT$}{address}", ""), quiet)];
    second.extend(tail.into_iter().flatten());
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

/// The host tab. Landscape: list left, details right. Portrait: list on top
/// with one-line rows, details below. Small windows: one panel at a time,
/// with the details (or forwards / snippets) taking the place of the list.
fn hosts(app: &mut App, f: &mut Frame, area: Rect) {
    let side_up = app.info || matches!(app.focus, Focus::Forwards | Focus::Snippets);
    let (list_area, side_area) = match app.shape {
        Shape::Wide => {
            let split = [Constraint::Percentage(44), Constraint::Min(24)];
            let [left, right] = Layout::horizontal(split).spacing(1).areas(area);
            (Some(left), Some(right))
        }
        Shape::Tall => {
            // The list takes what its rows need, up to two fifths of the height.
            let most = (area.height / 5 * 2).max(5);
            let rows = (app.hosts.len() as u16).saturating_add(2).clamp(5, most);
            let split = [Constraint::Length(rows), Constraint::Min(0)];
            let [top, bottom] = Layout::vertical(split).areas(area);
            (Some(top), Some(bottom))
        }
        Shape::Compact if side_up => (None, Some(area)),
        Shape::Compact => (Some(area), None),
    };

    if let Some(left) = list_area {
        // The scroll offset lives in the app; borrow it out while the rows borrow the app.
        let mut state = std::mem::take(&mut app.list);
        state.select(Some(app.sel));
        let th = &app.cfg.theme;
        let on_list = matches!(app.focus, Focus::List | Focus::Search);
        let compact = app.shape != Shape::Wide;
        // Inside the borders: one cell for the selection bar, one of padding on the right.
        let width = usize::from(left.width.saturating_sub(4));
        let items = app
            .hosts
            .iter()
            .enumerate()
            .map(|(i, alias)| host_item(app, alias, width, i == app.sel, compact));
        let title = format!(
            "{} {}",
            app.cfg.lang.pick("Hosts", "Máy chủ"),
            app.hosts.len()
        );
        let block = panel(&title, on_list, th).padding(Padding::right(1));
        let list = List::new(items)
            .block(block)
            .highlight_symbol(Line::styled("▎", th.accent))
            .repeat_highlight_symbol(true)
            .highlight_style(Style::new().bg(th.selection));
        f.render_stateful_widget(list, left, &mut state);
        app.list = state;
    }

    let Some(right) = side_area else { return };
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
        let mut spans = vec![Span::styled(format!("  {label:<12}"), dim)];
        spans.extend(value);
        Line::from(spans)
    };
    let text = |s: String| vec![Span::raw(s)];
    let lang = app.cfg.lang;
    let t = |en: &'static str, vi: &'static str| lang.pick(en, vi);

    let (dot, state, colour) = match app.reach.get(alias) {
        Some(Some(ms)) => {
            let word = t("reachable", "kết nối được");
            ("●", format!("{word} · {}", latency(*ms)), th.ok)
        }
        Some(None) => (
            "●",
            t("unreachable", "không kết nối được").to_string(),
            th.error,
        ),
        None => ("○", t("not checked", "chưa kiểm tra").to_string(), th.dim),
    };
    let mut lines = vec![
        Line::styled(address(app, alias), Style::new().fg(th.text).bold()),
        Line::styled(format!("{dot} {state}"), Style::new().fg(colour)),
    ];
    // Why the last attempt failed, until one succeeds.
    if let Some(fail) = app.fails.get(alias) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("✗ {} ", fail.class.label(lang)),
                Style::new().fg(RED).bold(),
            ),
            Span::styled(clean(&fail.message), dim),
        ]));
    }
    lines.extend([
        Line::default(),
        section(t("CONNECTION", "KẾT NỐI")),
        row(t("Address", "Địa chỉ"), text(or(get("HostName"), alias))),
        row(t("Port", "Cổng"), text(or(get("Port"), "22"))),
        row(
            t("User", "Người dùng"),
            text(or(
                get("User"),
                t("(current user)", "(người dùng hiện tại)"),
            )),
        ),
        row(
            t("Key", "Khoá"),
            text(or(
                get("IdentityFile"),
                t("(agent / default keys)", "(agent / khoá mặc định)"),
            )),
        ),
    ]);
    for key in ["ProxyJump", "ProxyCommand"] {
        if !get(key).is_empty() {
            let note = Span::styled("  not supported", Style::new().fg(th.error));
            lines.push(row(key, vec![Span::raw(get(key)), note]));
        }
    }

    lines.extend([Line::default(), section(t("ORGANISATION", "PHÂN LOẠI"))]);
    let group = match meta.grp.is_empty() {
        true => Span::styled("—", dim),
        false => Span::styled(meta.grp.clone(), Style::new().fg(chip(&meta.grp)).bold()),
    };
    lines.push(row(t("Group", "Nhóm"), vec![group]));
    let mut badges: Vec<Span> = tags(&meta)
        .flat_map(|t| [badge(t, false), Span::raw(" ")])
        .collect();
    if badges.is_empty() {
        badges.push(Span::styled("—", dim));
    }
    lines.push(row(t("Tags", "Thẻ"), badges));
    if !meta.notes.is_empty() {
        lines.push(row(t("Notes", "Ghi chú"), text(meta.notes.clone())));
    }

    lines.extend([Line::default(), section(t("ACTIVITY", "HOẠT ĐỘNG"))]);
    let last = text(ago(meta.last_connected, lang));
    lines.push(row(t("Last used", "Lần cuối"), last));
    lines.push(row(
        t("Sessions", "Số phiên"),
        text(meta.connects.to_string()),
    ));
    let vault = match &app.vault {
        Some(v) if v.get(alias).is_some() => Span::styled(
            t("password stored", "đã lưu mật khẩu"),
            Style::new().fg(th.ok),
        ),
        Some(_) => Span::raw(t("no password", "chưa có mật khẩu")),
        None => Span::styled(t("locked", "đang khoá"), dim),
    };
    lines.push(row(t("Vault", "Kho"), vec![vault]));

    let forwards = app.forwards(alias);
    if !forwards.is_empty() {
        lines.extend([Line::default(), section(t("FORWARDS", "CHUYỂN TIẾP CỔNG"))]);
    }
    for (kind, value) in forwards {
        let state = match app.fwds.get(&fwd_key(alias, kind, &value)) {
            Some((_, true)) => Span::styled(t("● active", "● đang chạy"), Style::new().fg(th.ok)),
            Some((_, false)) => Span::styled(
                t("◌ connecting", "◌ đang kết nối"),
                Style::new().fg(th.warn),
            ),
            None => Span::styled(t("○ stopped", "○ đã dừng"), dim),
        };
        let spec = Span::raw(format!("  {kind} {value}  "));
        lines.push(Line::from(vec![spec, state]));
    }
    // Outside landscape every row counts: no blank lines between the sections.
    let dense = app.shape != Shape::Wide;
    if dense {
        lines.retain(|line| line.width() > 0);
    }
    // In a small window this panel is on top of the list, so it looks focused.
    let on_top = app.shape == Shape::Compact;
    let block = panel(alias, on_top, th).padding(Padding::new(1, 1, u16::from(!dense), 0));
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
            Some((_, true)) => ("●", app.cfg.lang.pick("active", "đang chạy"), th.ok),
            Some((_, false)) => (
                "◌",
                app.cfg.lang.pick("connecting", "đang kết nối"),
                th.warn,
            ),
            None => ("○", app.cfg.lang.pick("stopped", "đã dừng"), th.dim),
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

fn files(view: &SftpView, th: &Theme, lang: Lang, tall: bool, f: &mut Frame, area: Rect) {
    let [panes, gauge] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    // Portrait: local above remote, each with the full width.
    let split = match tall {
        true => Layout::vertical([Constraint::Percentage(50); 2]),
        false => Layout::horizontal([Constraint::Percentage(50); 2]).spacing(1),
    };
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
        let name = [
            lang.pick("local", "máy mình"),
            lang.pick("remote", "máy từ xa"),
        ][side];
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
#[derive(Clone)]
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
            let mut connect = vec![enter];
            if app.shape == Shape::Compact {
                // Small windows keep the details off screen until asked for.
                connect.push(hint(&key(k.info), "details", "chi tiết", host));
            }
            connect.extend([
                hint(&key(k.sftp), "sftp", "sftp", host),
                hint(&key(k.mark), "mark", "đánh dấu", host),
                hint(&key(k.connect_tag), "by tag", "theo thẻ", host),
                hint(&key(k.connect), "quick", "nhanh", true),
            ]);
            vec![
                connect,
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

/// Greedy line filling: hints in order, a new row when the next one does not
/// fit. `None` marks the gap between two groups.
fn lay(groups: &[Vec<Hint>], width: usize) -> Vec<Vec<Option<Hint>>> {
    let mut rows: Vec<Vec<Option<Hint>>> = vec![vec![]];
    let mut used = 0;
    for group in groups {
        for (i, hint) in group.iter().enumerate() {
            let cells = hint.key.chars().count() + hint.text.chars().count() + 3;
            let gap = if i == 0 && used > 0 { 2 } else { 0 };
            if used > 0 && used + gap + cells > width {
                rows.push(vec![]);
                used = 0;
            } else if gap > 0 {
                rows.push_within(None);
                used += gap;
            }
            rows.push_within(Some(hint.clone()));
            used += cells;
        }
    }
    rows
}

/// `Vec<Vec<T>>` as rows being filled: push onto the last row.
trait PushWithin<T> {
    fn push_within(&mut self, item: T);
}

impl<T> PushWithin<T> for Vec<Vec<T>> {
    fn push_within(&mut self, item: T) {
        if let Some(row) = self.last_mut() {
            row.push(item);
        }
    }
}

/// Fits the hints into at most `rows` rows of `width` cells instead of
/// cutting them off at the edge. If they still do not fit, the hints that do
/// nothing right now are left out; only what remains after that is clipped.
fn flow(mut groups: Vec<Vec<Hint>>, width: usize, rows: usize) -> Vec<Vec<Option<Hint>>> {
    if lay(&groups, width).len() > rows {
        for group in &mut groups {
            group.retain(|hint| hint.on);
        }
        groups.retain(|group| !group.is_empty());
    }
    let mut out = lay(&groups, width);
    while out.len() > rows.max(1) {
        let extra = out.pop().unwrap_or_default();
        out.push_within(None);
        if let Some(last) = out.last_mut() {
            last.extend(extra);
        }
    }
    out
}

/// The lines above the status line: a message if there is one, otherwise the
/// key hints. Hints that do nothing right now are faded.
fn hints(app: &App, rows: Vec<Vec<Option<Hint>>>, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let line = |y: u16| Rect::new(area.x, area.y + y, area.width, 1.min(area.height));
    if !app.msg.is_empty() {
        let msg = format!(" {}", clean(&app.msg).replace('\n', " "));
        return f.render_widget(Line::styled(msg, th.accent), line(0));
    }
    let faded = Style::new().fg(th.dim).add_modifier(Modifier::DIM);
    for (y, row) in rows.into_iter().enumerate().take(area.height.into()) {
        let mut spans = vec![];
        for item in row {
            let Some(hint) = item else {
                spans.push(Span::styled(" │", Style::new().fg(th.border)));
                continue;
            };
            let (name, action) = match hint.on {
                true => (Style::new().fg(th.key).bold(), Style::new().fg(th.dim)),
                false => (faded, faded),
            };
            spans.push(Span::styled(format!(" {}", hint.key), name));
            spans.push(Span::styled(format!(" {} ", hint.text), action));
        }
        f.render_widget(Line::from(spans), line(y as u16));
    }
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
    let nerd = app.cfg.nerd_font;
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
                app.layout.name(app.shape == Shape::Tall).to_string(),
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
    let meta = app.meta.get(&alias).filter(|_| tab.is_none_or(|t| t.saved));
    if app.shape != Shape::Wide {
        // Portrait and small windows: mode, host, one tag. Recording and
        // broadcast stay, as marks: they change what typing does.
        let mut left = pill(name, badge, [th.bar; 2], nerd).to_vec();
        if tab.is_some_and(|t| t.log.is_some()) {
            left.push(Span::styled(" ●", Style::new().fg(th.error).bg(th.bar)));
        }
        if app.broadcast {
            let mark = Style::new().fg(BROADCAST).bg(th.bar).bold();
            left.push(Span::styled(format!(" →{sessions}"), mark));
        }
        left.push(Span::styled(format!(" {middle}"), plain.fg(th.text)));
        f.render_widget(Line::from(left), area);
        let tag = meta.and_then(|m| tags(m).next().or(Some(m.grp.as_str())));
        if let Some(tag) = tag.filter(|t| !t.is_empty()) {
            let tag = Line::styled(format!(" {tag} "), section);
            f.render_widget(tag.alignment(Alignment::Right), area);
        }
        return;
    }
    let mut left = pill(name, badge, [th.bar, th.selection], nerd).to_vec();
    left.push(Span::styled(format!(" {context} "), section));
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

    let mut right = vec![];
    let mut tail = vec![];
    // What the latency pill's left cap sits on: the group segment, if there is one.
    let mut before = th.bar;
    if let Some(meta) = meta {
        let all: Vec<&str> = tags(meta).collect();
        if !all.is_empty() {
            tail.push(Span::styled(format!("{} ", all.join(" ")), plain));
        }
        if !meta.grp.is_empty() {
            tail.push(Span::styled(format!(" {} ", meta.grp), section));
            before = th.selection;
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
    tail.extend(pill(&reach, badge, [before, th.bar], nerd));

    // Server metrics, in what is free between the centred text and the tags:
    // they never push anything else off the line.
    let free = usize::from(area.width).saturating_sub(middle.chars().count()) / 2;
    let taken: usize = tail.iter().map(Span::width).sum();
    let room = free.saturating_sub(taken + 4);
    let mem = lang.pick("MEM", "RAM");
    let stats = app.stats.get(&alias).and_then(Option::as_ref);
    let parts = stats.map(|s| s.compact(mem, room, app.shape));
    for (i, (label, value, level)) in parts.unwrap_or_default().into_iter().enumerate() {
        let colour = match level {
            Level::Ok => GREEN,
            Level::Warn => YELLOW,
            Level::Crit => RED,
        };
        let lead = if i == 0 { "" } else { " · " };
        right.push(Span::styled(format!("{lead}{label} "), plain));
        right.push(Span::styled(value, plain.fg(colour)));
    }
    if !right.is_empty() {
        right.push(Span::styled("  ", plain));
    }
    right.extend(tail);

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
    // Two columns where they fit, one on a narrow screen.
    let columns = if body.width >= 64 { 2 } else { 1 };
    let rows = entries.len().div_ceil(columns);
    let width = 76.min(body.width);
    let height = (rows as u16 + 2).min(body.height);
    let area = Rect::new(
        body.x + body.width - width,
        body.y + body.height - height,
        width,
        height,
    );
    let column = usize::from(width.saturating_sub(4)) / columns;
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
        .map(|r| {
            Line::from(
                (0..columns)
                    .flat_map(|c| cell(r + c * rows))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(panel(title, true, th)), area);
}

fn popup(popup: &Popup, th: &Theme, lang: Lang, f: &mut Frame) {
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
            let area = centered(screen, 78, form.rows() as u16 + 5);
            // Borders, padding, and the margins of the input band: the right
            // one is where the cursor stands after a value that fills it.
            let room = usize::from(area.width).saturating_sub(label_w + 6);
            let input = Style::new().bg(th.selection);
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
                    let x = area.x + 3 + (label_w + shown.chars().count()) as u16;
                    cursor = (x, area.y + 2 + i as u16);
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{label:<label_w$}"), style),
                    Span::styled(format!(" {shown:<room$} "), input),
                ]));
            }
            if let Some(on) = form.save {
                let focused = form.idx == form.fields.len();
                let style = match (on, focused) {
                    (_, true) => Style::new().fg(th.accent).bold(),
                    (true, false) => Style::new().fg(th.ok),
                    (false, false) => Style::new().fg(th.dim),
                };
                let text = lang.pick(
                    "save host to ~/.ssh/config, password to the vault",
                    "lưu máy vào ~/.ssh/config, mật khẩu vào kho",
                );
                let tick = if on { "x" } else { " " };
                if focused {
                    cursor = (area.x + 3, area.y + 2 + form.fields.len() as u16);
                }
                lines.push(Line::styled(format!("[{tick}] {text}"), style));
            }
            let field = lang.pick("next field", "ô kế tiếp");
            let cancel = lang.pick("cancel", "huỷ");
            let save = lang.pick("save on/off", "bật/tắt lưu");
            let mut hints = vec![("enter", "ok"), ("esc", cancel)];
            if form.rows() > 1 {
                hints.insert(0, ("tab", field));
            }
            if form.save.is_some() {
                hints.push(("ctrl-s", save));
            }
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
        let ago = |ts| ago(ts, Lang::En);
        assert_eq!(ago(0), "never");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now = now.as_secs() as i64;
        assert_eq!(ago(now), "just now");
        assert_eq!(ago(now - 150), "2 min ago");
        assert_eq!(ago(now - 7300), "2 h ago");
        assert_eq!(ago(now - 3 * 86400), "3 d ago");
        assert_eq!(super::ago(now - 7300, Lang::Vi), "2 giờ trước");
        assert_eq!(super::ago(0, Lang::Vi), "chưa lần nào");
    }

    #[test]
    fn chips_are_stable_and_popups_fit() {
        assert_eq!(chip("prod"), chip("prod"));
        assert_ne!(chip("prod"), chip("staging"));
        assert_eq!(shorten("root@web-01.example.com", 12), "root@web-01…");
        assert_eq!(shorten("short", 12), "short");
        assert_eq!(shorten("héllo wörld", 5), "héll…");
        // A tab switch shifts three frames by 2, 1 and 0 columns, then stays put.
        let mut left = SLIDE;
        let offsets: Vec<u16> = (0..5)
            .map(|_| {
                let offset = slide_step(&mut left);
                // A shifted frame always has a timed redraw coming after it.
                assert_eq!(offset > 0, left > 0);
                offset
            })
            .collect();
        assert_eq!(offsets, [2, 1, 0, 0, 0]);
        // A pill is as wide as the square badge it replaces, in either font.
        let badge = Style::new().fg(INK).bg(BLUE);
        for nerd in [false, true] {
            let spans = pill("NORMAL", badge, [Color::Black, Color::Red], nerd);
            assert_eq!(
                spans.iter().map(Span::width).sum::<usize>(),
                " NORMAL ".len()
            );
            let caps = [&spans[0], &spans[2]].map(|s| (s.style.fg, s.style.bg));
            assert_eq!(caps[0], (Some(BLUE), Some(Color::Black)));
            assert_eq!(caps[1], (Some(BLUE), Some(Color::Red)));
        }
        let small = Rect::new(0, 0, 20, 4);
        assert_eq!(centered(small, 76, 11), small);
        assert_eq!(
            centered(Rect::new(0, 0, 100, 40), 76, 10),
            Rect::new(12, 15, 76, 10)
        );
    }
}
