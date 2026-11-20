<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { FitAddon } from '@xterm/addon-fit'
  import { Unicode11Addon } from '@xterm/addon-unicode11'
  import { Terminal } from '@xterm/xterm'
  import '@xterm/xterm/css/xterm.css'
  import { onMount } from 'svelte'
  import { api, type PtyState, type Shown } from './api'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import { KEY, tip } from './keymap'
  import Menu, { pointed, type MenuItem } from './Menu.svelte'
  import { scale } from './scale.svelte'
  import { FONT, lastOutput, osc52, THEME } from './term'

  type Props = {
    /** The session this is the terminal of. */
    id: number
    label: string
    /** It is on screen: its tab is in front, or it is a pane beside the one that is. */
    active: boolean
    /** The keys are its own: of the panes on screen, the one in front. */
    front?: boolean
    /** Its shell ended, and why, if it said. */
    closed: boolean
    why: Shown | null
    /** What was typed, for the caller to send where it goes. */
    oninput: (data: string) => void
    onstate: (state: PtyState) => void
    onreconnect: () => void
    /** While some session broadcasts: whether this one hears it and is heard. Not said: nothing broadcasts, and there is no switch for it. */
    broadcast?: boolean
    onbroadcast?: () => void
  }
  let { id, label, active, front = true, closed, why, oninput, onstate, onreconnect, broadcast, onbroadcast }: Props = $props()

  let box: HTMLElement
  let term: Terminal
  /** The right button's menu, and where. */
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null)
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
    // Its cursor from the start, and not from the first time it has the keys: a pane beside the one in
    // front is seen to be alive. (Leaving the other screen, which it is not on, is what xterm takes for a start.)
    term.write('\x1b[?1047l')
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

  // A tab that comes to the front takes the keyboard, at the size it now has. So when the window is
  // drawn bigger or smaller: the letters here are as they were, and their room is not.
  $effect(() => {
    void scale.percent
    if (!active) return
    fit.fit()
    if (front) term.focus()
  })

  /** What a pointer with no keys can do here: the selection to the clipboard, the clipboard to the shell, all of the screen selected. */
  function asked(e: MouseEvent) {
    e.preventDefault()
    const text = term.getSelection()
    menu = {
      ...pointed(e),
      items: [
        { label: t('copy'), off: !text, run: () => void api.copy(text) },
        // As typed: xterm wraps it for a shell that asked for pastes to be marked.
        { label: t('paste'), keys: KEY.paste[0], off: closed, run: paste },
        null,
        { label: t('select_all'), run: () => term.selectAll() },
      ],
    }
  }

  /** The clipboard, typed here; into every broadcasting session, if this is one. */
  const paste = () => void api.paste().then((text) => text && term.paste(text))

  /** Ctrl+Shift+V, before xterm has the key: it would send the shell a Ctrl-V, and the web view would paste on its own, asking first. */
  function onkeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'v')) return
    e.preventDefault()
    e.stopPropagation()
    if (!closed) paste()
  }

  /** The keys back to it, from wherever they were. */
  export const focus = () => term.focus()

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

<!-- Dark in both themes, as the terminal in it is; in Day the theme's border is around it. -->
<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  role="tabpanel"
  aria-label={label}
  aria-keyshortcuts="Control+Shift+V"
  hidden={!active}
  oncontextmenu={asked}
  onkeydowncapture={onkeydown}
  class="relative min-h-0 flex-1 bg-term px-4 py-3 day:border day:border-edge"
>
  <div bind:this={box} class="h-full"></div>
  <div class="absolute top-3 right-4 z-10 flex max-w-[80%] items-start gap-2">
  {#if broadcast !== undefined}
    <!-- This pane's own switch, while broadcast is on: out of it, and in again. The keys stay the terminal's. -->
    <button
      type="button"
      aria-pressed={broadcast}
      aria-label={t('broadcast_pane')}
      title={tip(t('broadcast_pane'), [`${KEY.leader[0]} B`])}
      onclick={() => {
        onbroadcast?.()
        term.focus()
      }}
      class="flex h-8 shrink-0 cursor-pointer items-center gap-1.5 rounded-lg border border-edge bg-panel px-2 text-xs font-medium text-dim line-through hover:text-fg aria-pressed:border-yellow aria-pressed:text-yellow aria-pressed:no-underline"
    >
      <Icon name="cast" class="size-4!" />
      <span class="first-letter:uppercase">{t('broadcast')}</span>
    </button>
  {/if}
  {#if closed}
    <p
      role="status"
      class="flex min-w-0 items-center gap-3 rounded-xl border border-red bg-panel py-1.5 pr-1.5 pl-3 text-sm"
    >
      <span class="shrink-0 text-red">● {t('disconnected')}</span>
      {#if why}
        <span class="truncate text-white" title={why.message}><strong>{why.label}</strong> {why.message}</span>
      {/if}
      <button
        type="button"
        onclick={onreconnect}
        class="shrink-0 cursor-pointer rounded-lg border border-edge px-3 py-1 font-medium text-fg hover:bg-selection"
      >
        {t('reconnect')}
      </button>
    </p>
  {/if}
  </div>
</div>

{#if menu}
  <Menu
    {label}
    x={menu.x}
    y={menu.y}
    items={menu.items}
    onclose={() => {
      menu = null
      term.focus()
    }}
  />
{/if}
