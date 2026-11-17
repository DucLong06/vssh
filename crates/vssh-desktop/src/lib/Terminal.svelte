<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { FitAddon } from '@xterm/addon-fit'
  import { Unicode11Addon } from '@xterm/addon-unicode11'
  import { Terminal } from '@xterm/xterm'
  import '@xterm/xterm/css/xterm.css'
  import { onMount } from 'svelte'
  import { api, type PtyState, type Shown } from './api'
  import { t } from './i18n.svelte'
  import { FONT, lastOutput, osc52, THEME } from './term'

  type Props = {
    /** The session this is the terminal of. */
    id: number
    label: string
    /** Its tab is in front. */
    active: boolean
    /** Its shell ended, and why, if it said. */
    closed: boolean
    why: Shown | null
    /** What was typed, for the caller to send where it goes. */
    oninput: (data: string) => void
    onstate: (state: PtyState) => void
    onreconnect: () => void
  }
  let { id, label, active, closed, why, oninput, onstate, onreconnect }: Props = $props()

  let box: HTMLElement
  let term: Terminal
  const fit = new FitAddon()

  onMount(() => {
    term = new Terminal({
      theme: THEME,
      fontFamily: FONT,
      fontSize: 13,
      scrollback: 5000,
      cursorBlink: true,
      // The unicode addon's API is "proposed".
      allowProposedApi: true,
    })
    term.loadAddon(fit)
    term.loadAddon(new Unicode11Addon())
    term.unicode.activeVersion = '11'
    term.open(box)
    term.onData((data) => oninput(data))
    term.onResize(({ cols, rows }) => void api.sessionResize(id, cols, rows))
    // Copying needs no modifier: what the mouse selects is on the clipboard.
    term.onSelectionChange(() => {
      const text = term.getSelection()
      if (text) void api.copy(text)
    })
    // And what a program over there copies (OSC 52, as the TUI itself sends).
    term.parser.registerOscHandler(52, (data) => {
      const text = osc52(data)
      if (text !== null) void api.copy(text)
      return true
    })
    const heard = Promise.all([
      listen<number[]>(`pty-data:${id}`, (e) => term.write(new Uint8Array(e.payload))),
      listen<PtyState>(`pty-state:${id}`, (e) => onstate(e.payload)),
    ])
    // Only now: nothing the shell says is lost on the way here.
    void heard.then(start)
    const watching = new ResizeObserver(() => active && fit.fit())
    watching.observe(box)
    return () => {
      watching.disconnect()
      void heard.then((offs) => offs.forEach((off) => off()))
      term.dispose()
    }
  })

  // A tab that comes to the front takes the keyboard, at the size it now has.
  $effect(() => {
    if (!active) return
    fit.fit()
    term.focus()
  })

  /** Connects the session, again if it was connected before, at the terminal's size. */
  export function start() {
    if (active) fit.fit()
    return api.sessionStart(id, term.cols, term.rows)
  }

  /** Puts the output of the last command on the clipboard. */
  export function copyLast() {
    const buffer = term.buffer.active
    const row = buffer.baseY + buffer.cursorY
    const rows = Array.from({ length: row + 1 }, (_, y) => buffer.getLine(y)?.translateToString(true) ?? '')
    const text = lastOutput(rows, row, buffer.cursorX)
    if (text) void api.copy(text)
  }
</script>

<div role="tabpanel" aria-label={label} hidden={!active} class="relative min-h-0 flex-1 bg-bg p-2">
  <div bind:this={box} class="h-full"></div>
  {#if closed}
    <p
      role="status"
      class="absolute top-2 right-4 z-10 flex max-w-[80%] items-center gap-2 rounded-md border border-red bg-black px-2 py-1"
    >
      <span class="shrink-0 text-red">● {t('disconnected')}</span>
      {#if why}
        <span class="truncate text-white" title={why.message}><strong>{why.label}</strong> {why.message}</span>
      {/if}
      <button
        type="button"
        onclick={onreconnect}
        class="shrink-0 cursor-pointer rounded border border-edge px-2 text-magenta hover:bg-selection"
      >
        {t('reconnect')}
      </button>
    </p>
  {/if}
</div>
