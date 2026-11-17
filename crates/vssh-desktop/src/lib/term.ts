// What the terminal tabs need that is not xterm's: the palette, and the two
// ways text gets from a session to the clipboard.

/** TokyoNight Night, as app.css and the terminal app have it (docs/img/shots.py): the 16 ANSI colours. */
export const THEME = {
  background: '#1a1b26',
  foreground: '#c0caf5',
  cursor: '#c0caf5',
  cursorAccent: '#1a1b26',
  selectionBackground: '#414868',
  black: '#15161e',
  red: '#f7768e',
  green: '#9ece6a',
  yellow: '#e0af68',
  blue: '#7aa2f7',
  magenta: '#bb9af7',
  cyan: '#7dcfff',
  white: '#a9b1d6',
  brightBlack: '#414868',
  brightRed: '#f7768e',
  brightGreen: '#9ece6a',
  brightYellow: '#e0af68',
  brightBlue: '#7aa2f7',
  brightMagenta: '#bb9af7',
  brightCyan: '#7dcfff',
  brightWhite: '#c0caf5',
}

export const FONT = '"Maple Mono NF", "Maple Mono", ui-monospace, monospace'

/**
 * The output of the last command: the lines between the prompt the cursor is
 * on and the one before it, as the TUI's `last_output`. Prompts are
 * recognised by being the same text; when the prompt changes between
 * commands, or something is typed at it, the answer is everything above the
 * cursor. `rows` are the lines down to the cursor's, which is `row`.
 */
// shortcut: a command line that wraps counts as output, as in the TUI; read
// shell integration marks (OSC 133) if that ever matters.
export function lastOutput(rows: string[], row: number, col: number): string {
  const above = rows.slice(0, row)
  const prompt = (rows[row] ?? '').slice(0, col).trimEnd()
  const before = prompt ? above.map((line) => line.startsWith(prompt)).lastIndexOf(true) : -1
  const lines = above.slice(before + 1).map((line) => line.trimEnd())
  return lines.join('\n').replace(/^\n+|\n+$/g, '')
}

/**
 * The text an OSC 52 from the remote side puts on the clipboard: its data is
 * `<selection>;<base64>`. `null` for a query (`?`): nothing over there gets
 * to read the clipboard; and for what is not base64.
 */
export function osc52(data: string): string | null {
  const at = data.indexOf(';')
  const payload = data.slice(at + 1)
  if (at < 0 || payload === '?') return null
  try {
    const bytes = Uint8Array.from(atob(payload), (letter) => letter.charCodeAt(0))
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
  } catch {
    return null
  }
}
