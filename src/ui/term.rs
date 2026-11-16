//! Glue between the vt100 emulator and the terminal we run in: key events to
//! bytes for the remote PTY, and the emulated screen into a ratatui buffer.

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
