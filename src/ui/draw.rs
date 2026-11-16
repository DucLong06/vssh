//! Rendering. k9s-style frame: tab bar, body, one-line footer.

use std::num::NonZeroU16;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::files::{SftpView, LOCAL, REMOTE};
use super::{clean, fwd_key, term, App, Focus, Kind, Popup};
use crate::config::Theme;
use crate::ssh::SessionCmd;

/// Stable colour per group or tag name.
fn chip(name: &str) -> Color {
    const PALETTE: [Color; 6] = [
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Green,
        Color::Blue,
        Color::LightRed,
    ];
    PALETTE[name.bytes().map(usize::from).sum::<usize>() % PALETTE.len()]
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

fn panel<'a>(title: String, focused: bool, th: &Theme) -> Block<'a> {
    let (border, text) = match focused {
        true => (th.accent, th.accent),
        false => (th.border, th.dim),
    };
    Block::bordered()
        .border_style(Style::new().fg(border))
        .title(Span::styled(title, Style::new().fg(text).bold()))
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
    let [bar, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    app.body = (body.width, body.height);
    f.render_widget(
        Block::new().style(Style::new().fg(app.cfg.theme.text)),
        f.area(),
    );
    tab_bar(app, f, bar);
    let th = &app.cfg.theme;
    match app.active.checked_sub(1).map(|i| &mut app.tabs[i].kind) {
        None => hosts(app, f, body),
        Some(Kind::Term { parser, tx }) => {
            // Keep the remote PTY the size of the pane.
            let size = (NonZeroU16::new(body.height), NonZeroU16::new(body.width));
            if let (Some(rows), Some(cols)) = size {
                if parser.screen().size() != (rows, cols) {
                    parser.set_size(rows, cols);
                    let _ = tx.send(SessionCmd::Resize(body.width, body.height));
                }
            }
            let screen = parser.screen();
            term::render(screen, body, f.buffer_mut());
            if !screen.hide_cursor() && screen.scrollback() == 0 {
                let (row, col) = screen.cursor_position();
                f.set_cursor_position((body.x + col, body.y + row));
            }
        }
        Some(Kind::Sftp(view)) => files(view, th, f, body),
    }
    footer(app, f, foot);
    if let Some(p) = &app.popup {
        popup(p, &app.cfg.theme, f);
    }
}

fn tab_bar(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let on = Style::new().fg(Color::Black).bg(th.accent).bold();
    let mut spans = vec![Span::styled(" vssh ", Style::new().fg(th.accent).bold())];
    let titles = std::iter::once("hosts".to_string()).chain(app.tabs.iter().map(|t| {
        let kind = match t.kind {
            Kind::Sftp(_) => "sftp:",
            Kind::Term { .. } => "",
        };
        format!("{kind}{}{}", t.alias, if t.closed { " ✗" } else { "" })
    }));
    for (i, title) in titles.enumerate() {
        let style = match i == app.active {
            true => on,
            false => Style::new().fg(th.dim),
        };
        spans.push(Span::styled(format!(" {} {title} ", i + 1), style));
    }
    f.render_widget(Line::from(spans), area);
    let mut status = vec![];
    if !app.marked.is_empty() {
        status.push(format!("{} marked", app.marked.len()));
    }
    if app.vault.is_some() {
        status.push("vault unlocked".into());
    }
    let status = Line::styled(status.join(" · "), Style::new().fg(th.ok));
    f.render_widget(status.alignment(Alignment::Right), area);
}

fn hosts(app: &mut App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Min(20)]).areas(area);

    let items = app.hosts.iter().map(|alias| {
        let meta = app.meta.get(alias);
        let (dot, colour) = match app.reach.get(alias) {
            Some(Some(_)) => ("●", th.ok),
            Some(None) => ("●", th.error),
            None => ("○", th.dim),
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
        let mut spans = vec![
            Span::styled(dot, Style::new().fg(colour)),
            Span::styled(format!(" {pin}{mark} "), Style::new().fg(th.accent)),
            Span::raw(alias.as_str()),
        ];
        if let Some(grp) = meta.map(|m| m.grp.as_str()).filter(|g| !g.is_empty()) {
            spans.push(Span::styled(format!("  {grp}"), Style::new().fg(chip(grp))));
        }
        ListItem::new(Line::from(spans))
    });
    let search = match (app.focus, app.filter.is_empty()) {
        (Focus::Search, _) => format!("/{}▏", app.filter),
        (_, false) => format!("/{} ", app.filter),
        _ => String::new(),
    };
    let title = format!(" Hosts ({}) {search}", app.hosts.len());
    let focused = matches!(app.focus, Focus::List | Focus::Search);
    let list = List::new(items)
        .block(panel(title, focused, th))
        .highlight_style(selection(th));
    app.list.select(Some(app.sel));
    f.render_stateful_widget(list, left, &mut app.list);

    let Some(alias) = app.alias() else {
        let hint = match app.filter.is_empty() {
            true => "No hosts yet. Press the add key (a) to create one.",
            false => "No host matches the search.",
        };
        let block = panel(" Details ".into(), false, th);
        return f.render_widget(Paragraph::new(hint).block(block), right);
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
    let row = |label: &str, value: Vec<Span<'static>>| {
        let mut spans = vec![Span::styled(
            format!(" {label:<11}"),
            Style::new().fg(th.dim),
        )];
        spans.extend(value);
        Line::from(spans)
    };
    let text = |s: String| vec![Span::raw(s)];
    let or_default = |s: String, d: &str| if s.is_empty() { d.to_string() } else { s };

    let mut lines = vec![
        row("HostName", text(or_default(get("HostName"), alias))),
        row("User", text(or_default(get("User"), "(current user)"))),
        row("Port", text(or_default(get("Port"), "22"))),
        row(
            "Identity",
            text(or_default(get("IdentityFile"), "(agent / default keys)")),
        ),
    ];
    for key in ["ProxyJump", "ProxyCommand"] {
        if !get(key).is_empty() {
            let warn = Style::new().fg(th.error);
            let value = vec![Span::raw(get(key)), Span::styled("  not supported", warn)];
            lines.push(row(key, value));
        }
    }
    lines.push(Line::default());
    if !meta.grp.is_empty() {
        let style = Style::new().fg(chip(&meta.grp)).bold();
        lines.push(row("Group", vec![Span::styled(meta.grp.clone(), style)]));
    }
    let tags: Vec<Span> = meta
        .tags
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .flat_map(|t| {
            let style = Style::new().fg(Color::Black).bg(chip(t));
            [Span::styled(format!(" {t} "), style), Span::raw(" ")]
        })
        .collect();
    if !tags.is_empty() {
        lines.push(row("Tags", tags));
    }
    let status = match app.reach.get(alias) {
        Some(Some(ms)) => Span::styled(format!("reachable ({ms} ms)"), Style::new().fg(th.ok)),
        Some(None) => Span::styled("unreachable", Style::new().fg(th.error)),
        None => Span::styled("not checked", Style::new().fg(th.dim)),
    };
    lines.push(row("Status", vec![status]));
    let used = format!(
        "{} ({} connections)",
        ago(meta.last_connected),
        meta.connects
    );
    lines.push(row("Last used", text(used)));
    let vault = match &app.vault {
        Some(v) if v.get(alias).is_some() => "password stored",
        Some(_) => "no password",
        None => "locked",
    };
    lines.push(row("Vault", text(vault.into())));
    for (kind, value) in app.forwards(alias) {
        let state = match app.fwds.get(&fwd_key(alias, kind, &value)) {
            Some((_, true)) => Span::styled("  ● active", Style::new().fg(th.ok)),
            Some((_, false)) => Span::styled("  ◌ connecting", Style::new().fg(th.accent)),
            None => Span::raw(""),
        };
        lines.push(row(
            "Forward",
            vec![Span::raw(format!("{kind} {value}")), state],
        ));
    }
    if !meta.notes.is_empty() {
        lines.push(Line::default());
        lines.push(row("Notes", text(meta.notes)));
    }
    let block = panel(format!(" {alias} "), false, th);
    let details = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(details.block(block), area);
}

fn sub_list(app: &App, title: String, mut items: Vec<Line<'static>>, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    if items.is_empty() {
        items.push(Line::styled(" nothing here yet, press a to add", th.dim));
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
            Some((_, false)) => ("◌", "connecting", th.accent),
            None => ("○", "stopped", th.dim),
        };
        Line::from(vec![
            Span::styled(format!(" {dot} "), Style::new().fg(colour)),
            Span::raw(format!("{kind} {value:<34}")),
            Span::styled(state, Style::new().fg(colour)),
        ])
    });
    let title = format!(" Forwards of {alias} ");
    sub_list(app, title, items.collect(), f, area);
}

fn snippets(app: &App, alias: &str, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let items = app.host_snippets(alias).into_iter().map(|s| {
        let scope = if s.host.is_empty() { "all " } else { "host" };
        Line::from(vec![
            Span::styled(format!(" {scope} "), Style::new().fg(th.dim)),
            Span::styled(format!("{:<18} ", s.name), Style::new().fg(th.accent)),
            Span::raw(s.command.clone()),
        ])
    });
    let targets = match app.marked.len() {
        0 => alias.to_string(),
        n => format!("{n} marked hosts"),
    };
    let title = format!(" Snippets → {targets} ");
    sub_list(app, title, items.collect(), f, area);
}

fn files(view: &SftpView, th: &Theme, f: &mut Frame, area: Rect) {
    let [panes, gauge] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let halves: [Rect; 2] = Layout::horizontal([Constraint::Percentage(50); 2]).areas(panes);
    for side in [LOCAL, REMOTE] {
        let items = view.lists[side].iter().map(|e| match e.is_dir {
            true => Line::styled(format!(" {}/", clean(&e.name)), th.accent),
            false => Line::from(vec![
                Span::raw(format!(" {}", clean(&e.name))),
                Span::styled(format!("  {}", human(e.size)), Style::new().fg(th.dim)),
            ]),
        });
        let name = ["local", "remote"][side];
        let title = format!(" {name}: {} ", clean(&view.dirs[side]));
        let list = List::new(items)
            .block(panel(title, side == view.side, th))
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

fn footer(app: &App, f: &mut Frame, area: Rect) {
    let th = &app.cfg.theme;
    let k = &app.cfg.keys;
    let line = if app.leader {
        let keys = "LEADER  Tab/n next · p prev · 1-9 tab · h hosts · x close · k/j scroll";
        Line::styled(
            format!(" {keys}"),
            Style::new().fg(Color::Black).bg(th.accent),
        )
    } else if !app.msg.is_empty() {
        Line::styled(
            format!(" {}", clean(&app.msg).replace('\n', " ")),
            th.accent,
        )
    } else {
        let hints = match (app.active.checked_sub(1).map(|i| &app.tabs[i].kind), app.focus) {
            (Some(Kind::Term { .. }), _) => {
                format!("Ctrl-{} then: Tab next tab · h hosts · x close · k/j scroll", k.leader)
            }
            (Some(Kind::Sftp(_)), _) => {
                "Tab pane · Enter open · h up · c copy · m mkdir · r rename · d delete · q close"
                    .into()
            }
            (None, Focus::Search) => "type to filter · Enter keep · Esc clear".into(),
            (None, Focus::Forwards) => "a add · d delete · Enter start/stop · Esc back".into(),
            (None, Focus::Snippets) => "a add · e edit · d delete · Enter run · Esc back".into(),
            (None, Focus::List) => format!(
                "Enter shell · {} sftp · {} forwards · {} snippets · {} search · {} add · {} edit · {} help · {} quit",
                k.sftp, k.forwards, k.snippets, k.search, k.add, k.edit, k.help, k.quit
            ),
        };
        Line::styled(format!(" {hints}"), th.dim)
    };
    f.render_widget(line, area);
}

fn popup(popup: &Popup, th: &Theme, f: &mut Frame) {
    let screen = f.area();
    let frame = |title: &str| panel(format!(" {title} "), true, th);
    match popup {
        Popup::Confirm(text, _) => {
            let area = centered(screen, text.chars().count() as u16 + 6, 5);
            let lines = vec![
                Line::raw(""),
                Line::raw(text.as_str()),
                Line::styled("y yes · n no", th.dim),
            ];
            let body = Paragraph::new(lines).alignment(Alignment::Center);
            f.render_widget(Clear, area);
            f.render_widget(body.block(frame("Confirm")), area);
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
            f.render_widget(text.block(frame(title)), area);
        }
        Popup::Form(form) => {
            let label_w = form.fields.iter().map(|f| f.0.chars().count()).max();
            let label_w = label_w.unwrap_or(0) + 2;
            let area = centered(screen, 76, form.fields.len() as u16 + 3);
            let room = (area.width as usize).saturating_sub(label_w + 4);
            let mut cursor = (area.x, area.y);
            let mut lines = vec![];
            for (i, (label, value)) in form.fields.iter().enumerate() {
                // Show the tail of a value that does not fit.
                let len = value.chars().count();
                let shown: String = match form.masked {
                    true => "•".repeat(len.min(room)),
                    false => value.chars().skip(len.saturating_sub(room)).collect(),
                };
                let style = match i == form.idx {
                    true => Style::new().fg(th.accent).bold(),
                    false => Style::new().fg(th.dim),
                };
                if i == form.idx {
                    let x = area.x + 2 + (label_w + shown.chars().count()) as u16;
                    cursor = (x, area.y + 1 + i as u16);
                }
                lines.push(Line::from(vec![
                    Span::styled(format!(" {label:<label_w$}"), style),
                    Span::raw(shown),
                ]));
            }
            let hint = match form.fields.len() {
                1 => " Enter ok · Esc cancel",
                _ => " Tab/↑↓ field · Enter save · Esc cancel",
            };
            lines.push(Line::styled(hint, th.dim));
            f.render_widget(Clear, area);
            f.render_widget(Paragraph::new(lines).block(frame(&form.title)), area);
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
        let small = Rect::new(0, 0, 20, 4);
        assert_eq!(centered(small, 76, 11), small);
        assert_eq!(
            centered(Rect::new(0, 0, 100, 40), 76, 10),
            Rect::new(12, 15, 76, 10)
        );
    }
}
