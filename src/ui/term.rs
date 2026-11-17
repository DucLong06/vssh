//! Glue between the vt100 emulator and the terminal we run in: key events to
//! bytes for the remote PTY, the emulated screen into a ratatui buffer, and
//! text out to the clipboard of whoever owns that terminal.

use std::io::{stdout, Write};
use std::process::{Command, Stdio};

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// What a real xterm would send for this key.
pub fn encode_key(key: KeyEvent, application_cursor: bool) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let cursor = |c: char| match (ctrl, application_cursor) {
        (true, _) => format!("\x1b[1;5{c}"),
        (false, true) => format!("\x1bO{c}"),
        (false, false) => format!("\x1b[{c}"),
    };
    let seq: String = match key.code {
        // Ctrl-a..z, Ctrl-[ \ ] ^ _ and Ctrl-space/@ map onto 0x00..0x1f.
        KeyCode::Char(c) if ctrl && c.is_ascii() => ((c as u8 & 0x1f) as char).into(),
        KeyCode::Char(c) => c.into(),
        KeyCode::Enter => "\r".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Up => cursor('A'),
        KeyCode::Down => cursor('B'),
        KeyCode::Right => cursor('C'),
        KeyCode::Left => cursor('D'),
        KeyCode::Home => cursor('H'),
        KeyCode::End => cursor('F'),
        KeyCode::Insert => "\x1b[2~".into(),
        KeyCode::Delete => "\x1b[3~".into(),
        KeyCode::PageUp => "\x1b[5~".into(),
        KeyCode::PageDown => "\x1b[6~".into(),
        KeyCode::F(n @ 1..=4) => format!("\x1bO{}", (b'P' + n - 1) as char),
        KeyCode::F(n @ 5..=12) => {
            format!("\x1b[{}~", [15, 17, 18, 19, 20, 21, 23, 24][n as usize - 5])
        }
        _ => return vec![],
    };
    let mut out = Vec::with_capacity(seq.len() + 1);
    if key.modifiers.contains(KeyModifiers::ALT) {
        out.push(0x1b);
    }
    out.extend_from_slice(seq.as_bytes());
    out
}

/// Collects answers to the status queries programs send to the terminal.
/// Without them vim waits seconds at startup for a reply that never comes.
#[derive(Default)]
pub struct Replies(pub Vec<u8>);

impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        intermediate: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let param = params.first().and_then(|p| p.first()).copied();
        let reply = match (intermediate, c, param.unwrap_or(0)) {
            // Primary device attributes: a VT220 with ANSI colour.
            (None, 'c', 0) => "\x1b[?62;22c".to_string(),
            // Secondary: version 0, so nobody assumes xterm-only features.
            (Some(b'>'), 'c', 0) => "\x1b[>0;0;0c".to_string(),
            // Device status: OK.
            (None, 'n', 5) => "\x1b[0n".to_string(),
            // Cursor position report, 1-based.
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                format!("\x1b[{};{}R", row + 1, col + 1)
            }
            _ => return,
        };
        self.0.extend_from_slice(reply.as_bytes());
    }
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Standard base64 with padding. Twelve lines beat a dependency.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            // A chunk of 1 or 2 bytes fills 2 or 3 characters; the rest is padding.
            let sextet = ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char;
            out.push(if i <= chunk.len() { sextet } else { '=' });
        }
    }
    out
}

/// OSC 52: asks the terminal to put `text` on the system clipboard. The text
/// travels as base64, so nothing in it can act as an escape sequence.
fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Puts `text` on the clipboard of the terminal vssh runs in, with no
/// clipboard library: the OSC 52 escape (unless `escape` is off), and inside
/// tmux also tmux's own buffer, because tmux drops an application's OSC 52
/// unless it runs with `set-clipboard on`.
pub fn copy(text: &str, escape: bool) {
    if escape {
        let mut out = stdout();
        let _ = out.write_all(osc52(text).as_bytes());
        let _ = out.flush();
    }
    if std::env::var_os("TMUX").is_none() {
        return;
    }
    // `-w` (tmux 3.2 and later) also sends it on to the outer terminal.
    for args in [&["load-buffer", "-w", "-"][..], &["load-buffer", "-"]] {
        let tmux = Command::new("tmux")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut tmux) = tmux else { return };
        if let Some(mut stdin) = tmux.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if tmux.wait().is_ok_and(|status| status.success()) {
            return;
        }
    }
}

/// The output of the last command on screen: the lines between the prompt
/// the cursor is on and the one before it. Prompts are recognised by being
/// the same text; when the prompt changes between commands, or something is
/// already typed at it, there is nothing to recognise and the answer is
/// everything above the cursor.
// shortcut: visible screen only, and a command line that wraps counts as
// output; read shell integration marks (OSC 133) if that ever matters.
pub fn last_output(screen: &vt100::Screen) -> String {
    let (row, col) = screen.cursor_position();
    let rows: Vec<String> = screen.rows(0, screen.size().1.get()).collect();
    let above = &rows[..usize::from(row).min(rows.len())];
    let prompt: String = rows
        .get(usize::from(row))
        .map_or("", |r| r.as_str())
        .chars()
        .take(col.into())
        .collect();
    let prompt = prompt.trim_end();
    let before = above
        .iter()
        .rposition(|r| !prompt.is_empty() && r.starts_with(prompt));
    let output = &above[before.map_or(0, |p| p + 1)..];
    let lines: Vec<&str> = output.iter().map(|r| r.trim_end()).collect();
    lines.join("\n").trim_matches('\n').to_string()
}

pub fn render(screen: &vt100::Screen, area: Rect, buf: &mut Buffer) {
    for row in 0..area.height {
        for col in 0..area.width {
            // The right half of a wide character belongs to the cell before it.
            let Some(cell) = screen.cell(row, col).filter(|c| !c.is_wide_continuation()) else {
                continue;
            };
            let mut style = Style::new()
                .fg(color(cell.fgcolor()))
                .bg(color(cell.bgcolor()));
            for (on, modifier) in [
                (cell.bold(), Modifier::BOLD),
                (cell.dim(), Modifier::DIM),
                (cell.italic(), Modifier::ITALIC),
                (cell.underline(), Modifier::UNDERLINED),
                (cell.inverse(), Modifier::REVERSED),
            ] {
                if on {
                    style = style.add_modifier(modifier);
                }
            }
            let symbol = if cell.has_contents() {
                cell.contents()
            } else {
                " "
            };
            buf[(area.x + col, area.y + row)]
                .set_symbol(symbol)
                .set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
        encode_key(KeyEvent::new(code, mods), false)
    }

    #[test]
    fn encodes_keys_like_xterm() {
        let none = KeyModifiers::NONE;
        assert_eq!(enc(KeyCode::Char('a'), none), b"a");
        assert_eq!(enc(KeyCode::Char('é'), none), "é".as_bytes());
        assert_eq!(enc(KeyCode::Char('c'), KeyModifiers::CONTROL), [3]);
        assert_eq!(enc(KeyCode::Char('['), KeyModifiers::CONTROL), [0x1b]);
        assert_eq!(enc(KeyCode::Char(' '), KeyModifiers::CONTROL), [0]);
        assert_eq!(enc(KeyCode::Char('f'), KeyModifiers::ALT), b"\x1bf");
        assert_eq!(enc(KeyCode::Enter, none), b"\r");
        assert_eq!(enc(KeyCode::Backspace, none), [0x7f]);
        assert_eq!(enc(KeyCode::Up, none), b"\x1b[A");
        assert_eq!(enc(KeyCode::Right, KeyModifiers::CONTROL), b"\x1b[1;5C");
        assert_eq!(enc(KeyCode::Delete, none), b"\x1b[3~");
        assert_eq!(enc(KeyCode::F(1), none), b"\x1bOP");
        assert_eq!(enc(KeyCode::F(5), none), b"\x1b[15~");
        assert_eq!(enc(KeyCode::F(12), none), b"\x1b[24~");
        assert_eq!(enc(KeyCode::CapsLock, none), b"");
        // vim and less switch the cursor keys to application mode.
        let up = KeyEvent::new(KeyCode::Up, none);
        assert_eq!(encode_key(up, true), b"\x1bOA");
    }

    #[test]
    fn answers_terminal_queries() {
        let size = std::num::NonZeroU16::new(10).unwrap();
        let mut parser = vt100::Parser::new_with_callbacks(size, size, 0, Replies::default());
        parser.process(b"ab\r\ncd\x1b[c\x1b[>c\x1b[5n\x1b[6n\x1b[?1049h\x1b[2J\x1b[99n");
        assert_eq!(
            parser.callbacks().0,
            b"\x1b[?62;22c\x1b[>0;0;0c\x1b[0n\x1b[2;3R"
        );
    }

    #[test]
    fn osc52_carries_the_text_as_base64() {
        // RFC 4648 test vectors: every padding length.
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
        assert_eq!(base64("xin chào".as_bytes()), "eGluIGNow6Bv");
        assert_eq!(osc52("ssh web-01"), "\x1b]52;c;c3NoIHdlYi0wMQ==\x07");
        // Whatever is copied, the sequence holds no control byte but its own frame.
        let hostile = osc52("\x1b]52;c;evil\x07\x1b[2J");
        let inner = &hostile[7..hostile.len() - 1];
        assert!(inner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)));
    }

    #[test]
    fn last_output_is_what_lies_between_two_prompts() {
        let size = std::num::NonZeroU16::new(12).unwrap();
        let screen = |bytes: &str| {
            let mut parser = vt100::Parser::new(size, size.saturating_add(28), 0);
            parser.process(bytes.as_bytes());
            last_output(parser.screen())
        };
        let session = "web:~# ls\r\nold  news\r\nweb:~# df -h /\r\nFilesystem  Use%\r\n/dev/sda1   61%\r\n\r\nweb:~# ";
        assert_eq!(screen(session), "Filesystem  Use%\n/dev/sda1   61%");
        // First command of the session: nothing above but the output.
        assert_eq!(
            screen("web:~# uptime\r\n up 3 days\r\nweb:~# "),
            " up 3 days"
        );
        // Something typed at the prompt, or a prompt that changes: everything above the cursor.
        let typed = "web:~# ls\r\na  b\r\nweb:~# ech";
        assert_eq!(screen(typed), "web:~# ls\na  b");
        let clock = "10:01 $ ls\r\na  b\r\n10:02 $ ";
        assert_eq!(screen(clock), "10:01 $ ls\na  b");
        assert_eq!(screen(""), "");
        assert_eq!(screen("web:~# "), "");
    }

    #[test]
    fn renders_text_colours_and_wide_chars() {
        let size = std::num::NonZeroU16::new(4).unwrap();
        let mut parser = vt100::Parser::new(size, size.saturating_add(6), 0);
        parser.process("a\x1b[1;31mb\x1b[0m\r\n世x".as_bytes());
        let area = Rect::new(2, 1, 10, 4);
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 5));
        render(parser.screen(), area, &mut buf);

        assert_eq!(buf[(2, 1)].symbol(), "a");
        assert_eq!(buf[(3, 1)].symbol(), "b");
        assert_eq!(buf[(3, 1)].fg, Color::Indexed(1));
        assert!(buf[(3, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(2, 1)].fg, Color::Reset);
        assert_eq!(buf[(2, 2)].symbol(), "世");
        assert_eq!(buf[(4, 2)].symbol(), "x");
    }
}
