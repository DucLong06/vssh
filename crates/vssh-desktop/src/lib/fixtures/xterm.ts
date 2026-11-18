// xterm.js for the tests (aliased in vite.config.ts): jsdom has no layout to
// draw a terminal in. Each terminal keeps what was written to it, and a test
// is the person at it: `type`, `resize`, `select`. The real one is run by
// scripts/gui-smoke.sh.
type Fn<T> = (value: T) => void

/** Every terminal that is open. */
export const terminals: Terminal[] = []

export class Terminal {
  cols = 80
  rows = 24
  /** Everything written, as text. */
  written = ''
  focused = false
  unicode = { activeVersion: '6' }
  /** The handlers for the escape sequences xterm leaves to its host. */
  osc = new Map<number, (data: string) => boolean>()
  parser = { registerOscHandler: (code: number, fn: (data: string) => boolean) => void this.osc.set(code, fn) }
  private selection = ''
  private typed: Fn<string>[] = []
  private resized: Fn<{ cols: number; rows: number }>[] = []
  private selected: Fn<void>[] = []

  constructor(readonly options: Record<string, unknown> = {}) {
    terminals.push(this)
  }

  /** The screen as xterm exposes it: lines, and the cursor after the last thing written. */
  get buffer() {
    const lines = this.written.split('\r\n')
    return {
      active: {
        baseY: 0,
        cursorY: lines.length - 1,
        cursorX: lines[lines.length - 1].length,
        getLine: (y: number) =>
          y < lines.length ? { translateToString: (trim: boolean) => (trim ? lines[y].trimEnd() : lines[y]) } : undefined,
      },
    }
  }

  loadAddon() {}
  open() {}
  write(data: Uint8Array | string) {
    // As the real one: a switch of its modes is obeyed, and nothing of it is on the screen.
    this.written += (typeof data === 'string' ? data : new TextDecoder().decode(data)).replace(/\x1b\[\?\d+[hl]/g, '')
  }
  focus() {
    for (const other of terminals) other.focused = other === this
  }
  dispose() {
    terminals.splice(terminals.indexOf(this), 1)
  }
  getSelection = () => this.selection
  /** As the real one: what is pasted is typed. */
  paste = (text: string) => this.type(text)
  selectAll = () => this.select(this.written)
  onData = (fn: Fn<string>) => void this.typed.push(fn)
  onResize = (fn: Fn<{ cols: number; rows: number }>) => void this.resized.push(fn)
  onSelectionChange = (fn: Fn<void>) => void this.selected.push(fn)

  type(text: string) {
    for (const fn of this.typed) fn(text)
  }
  resize(cols: number, rows: number) {
    Object.assign(this, { cols, rows })
    for (const fn of this.resized) fn({ cols, rows })
  }
  select(text: string) {
    this.selection = text
    for (const fn of this.selected) fn()
  }
}

export class FitAddon {
  fit() {}
}
export class Unicode11Addon {}
