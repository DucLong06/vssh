<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount, tick as rendered, untrack } from 'svelte'
  import {
    api,
    explain,
    type ConnError,
    type ForwardState,
    type Host,
    type HostForm as Form,
    type PtyState,
    type Shown,
    type Snippet,
    type VaultState,
  } from './lib/api'
  import Dashboard from './lib/Dashboard.svelte'
  import Failure from './lib/Failure.svelte'
  import Files from './lib/Files.svelte'
  import Forwards from './lib/Forwards.svelte'
  import HostDelete from './lib/HostDelete.svelte'
  import HostForm from './lib/HostForm.svelte'
  import HostsJson from './lib/HostsJson.svelte'
  import KeySetup from './lib/KeySetup.svelte'
  import { fromTarget } from './lib/hosts'
  import { i18n, t } from './lib/i18n.svelte'
  import { down, live, poll, relabel, up } from './lib/live.svelte'
  import Modal from './lib/Modal.svelte'
  import Palette from './lib/Palette.svelte'
  import { setScale, zoom } from './lib/scale.svelte'
  import { bars, half, ids, none, place, mapped, rects, resize, shift, without, zone, type Bar, type Layout, type Rect, type Zone } from './lib/panes'
  import Shortcuts from './lib/Shortcuts.svelte'
  import Sidebar, { type Nav } from './lib/Sidebar.svelte'
  import Batch from './lib/Batch.svelte'
  import Snippets from './lib/Snippets.svelte'
  import Statusbar from './lib/Statusbar.svelte'
  import { learn, streamer, target } from './lib/streamer.svelte'
  import Tabs, { type Tab } from './lib/Tabs.svelte'
  import Terminal from './lib/Terminal.svelte'
  import Topbar from './lib/Topbar.svelte'
  import { check, update } from './lib/update.svelte'
  import Updated from './lib/Updated.svelte'
  import { heard as forwarded, sync as forwarding_of, tunnels } from './lib/tunnels.svelte'
  import Vault from './lib/Vault.svelte'

  /** How often the hosts are asked for their figures, and every how many rounds pinged. */
  const EVERY = 5000
  const PING_ROUNDS = 12
  /** How long the app is left to start before it asks whether a newer version is out. */
  const NEWS_AFTER = 5000

  let hosts = $state<Host[]>([])
  let snippets = $state<Snippet[]>([])
  /** A tab's session. `met`: its shell came up since it was last started. `quick`: on no host of the config. */
  type Session = Tab & { met: boolean; error: ConnError | null; why: Shown | null; quick: boolean; name?: string }
  const fresh = { state: 'connecting', busy: null, met: false, error: null, why: null } as const
  let sessions = $state<Session[]>([])
  /** The session whose failure to connect is in a dialog. */
  let failure = $state<number | null>(null)
  const failed_session = $derived(sessions.find((session) => session.id === failure))
  /** The file views, one per host. Their ids count down from -1: a session's is never below zero. */
  let files = $state<{ id: number; host: string }[]>([])
  /** The session or the file view whose tab is in front; `null` = the dashboard. */
  let active = $state<number | null>(null)
  /** The tabs that are open together as panes, and where each is; none: one tab at a time. */
  let layout = $state<Layout>(none())
  const split = $derived(ids(layout))
  /** The tabs as somebody put them in the strip; one never moved comes after them, as it was opened. */
  let arranged = $state<number[]>([])
  /** A tab is being dragged, and the pane it is over: at an edge of it, or in its middle. */
  let dragging = $state<number | null>(null)
  let aim = $state<{ id: number; zone: Zone } | null>(null)
  /** The line between two panes that the pointer holds: where it took it, and what the panes were then. */
  let sizing: { bar: Bar; from: number; start: Layout } | null = null
  let frame = $state<HTMLElement>()
  /** Focus mode: no rail and no status bar, and a tab's view from edge to edge. */
  let zen = $state(false)
  /** The rail as icons only; kept by the web view, as the theme is. */
  let compact = $state(localStorage.getItem('vssh.rail') === 'compact')
  /** What was open is open again, or there was nothing: from here on what is open is written down. */
  let restored = $state(false)
  /** The tab somebody is asked about before it goes: its shell is running something. */
  let closing = $state<number | null>(null)
  /** Ctrl-A was pressed: the next key is a command, as in the TUI. */
  let leader = $state(false)
  /** The terminals, by session. */
  const terms: Record<number, Terminal> = {}
  let selected = $state<string | null>(null)
  /** The tag the sidebar narrowed the hosts to. */
  let tag = $state<string | null>(null)
  /** The hosts marked on the dashboard. */
  let marked = $state<string[]>([])
  /** Whom a command is for: the marked hosts, else the selected one, as in the TUI; in the order of the config. */
  const picked = $derived(hosts.filter((host) => (marked.length ? marked.includes(host.alias) : host.alias === selected)))
  let dashboard = $state<Dashboard>()
  /** `newtab`: the palette, asked for a tab: the host chosen there is connected to. `pick-`: asked whose files, whose forwards. */
  type Dialog = 'palette' | 'newtab' | 'pick-files' | 'pick-forwards' | 'snippets' | 'batch' | 'forwards' | 'vault' | 'host' | 'delete' | 'keys' | 'shortcuts' | 'export' | 'import'
  let modal = $state<Dialog | null>(null)
  /** The host form's host (none: a new one), and what a new one's form opens with. Also the host to delete, or to give a key. */
  let editing = $state<string>()
  let start = $state<Partial<Form>>()
  /** The quick connect that the form is to keep as a host. */
  let keeping: Session | undefined
  /** Unlocked, the vault stays so until the app is closed: the shell holds it, and nothing of it is written down. */
  let vault = $state<VaultState>('none')
  /** The vault's dialog is the question the app opens with, not the manager someone asked for. */
  let greeting = $state(false)
  /** The host whose forwards are open. */
  let forwarding = $state('')
  /** The command the batch dialog runs, and where. */
  let job = $state<{ hosts: Host[]; command: string; params: Record<string, string> }>()
  /** Why the hosts could not be read. */
  let failed = $state<Shown | null>(null)
  /** No host is watched: the dashboard asks none for anything. The database keeps it, as it keeps each host's own switch. */
  let paused = $state(false)

  const fail = async (error: unknown) => (failed = await explain(error, i18n.lang))

  /** Reads the hosts again: the ssh config or the TUI's database may have changed. */
  async function reload() {
    try {
      const next = await api.listHosts()
      // shortcut: asked every round instead of watching the two files; a
      // change shows up within EVERY. Watch them (notify + an event) if that lag matters.
      if (JSON.stringify(next) !== JSON.stringify(hosts)) learn((hosts = next))
      failed = null
    } catch (error) {
      await fail(error)
    }
  }

  onMount(() => {
    let round = 0
    // Known before the first round: what was stopped when the app was closed is not asked once on the way in.
    const told = api.watchPaused().then((now) => (paused = now), fail)
    const tick = async () => {
      await reload()
      await told
      // A host that is not watched is asked for nothing, a ping neither: the shell opens and keeps no connection to it.
      const watched = paused ? [] : hosts.filter((host) => !host.meta.paused)
      poll(
        watched.map((host) => host.alias),
        round++ % PING_ROUNDS === 0,
      )
    }
    void tick()
    api.listSnippets().then((list) => (snippets = list), fail)
    // A vault that is there is asked to be opened first thing; it may be left shut.
    api.vaultState().then((now) => {
      vault = now
      greeting = now === 'locked' && modal === null
      if (greeting) modal = 'vault'
      // What was open comes back once the vault has been asked about: a session may need a password of it.
      else void restore()
    }, fail)
    const timer = setInterval(tick, EVERY)
    const news = setTimeout(check, NEWS_AFTER)
    const forwards = listen<ForwardState>('forward-state', ({ payload }) => forwarded(payload))
    return () => {
      clearInterval(timer)
      clearTimeout(news)
      void forwards.then((unlisten) => unlisten())
    }
  })

  // A failure is kept as data: the words for it follow the language, and
  // streamer mode, which takes the addresses out of them.
  $effect(() => {
    void streamer.on
    void relabel(i18n.lang)
  })

  // Why a session ended, in the window's language.
  $effect(() => {
    void streamer.on
    for (const session of sessions) {
      const error = session.error
      if (!error) continue
      void explain(error, i18n.lang).then((why) => {
        if (session.error === error) session.why = why
      })
    }
  })

  // The window's shape changed, by a button that went with it (the rail's) or that
  // kept the focus (the strip's, a tab let go of): the keys are the terminal's in front again.
  $effect(() => {
    void [zen, shape, sessions.length, closing]
    const id = untrack(() => front?.id)
    if (id !== undefined) terms[id]?.focus()
  })

  /** The tabs as they stand in the strip: the sessions, then the file views, unless somebody moved them. */
  const order = $derived.by(() => {
    const all = [...sessions, ...files].map((tab) => tab.id)
    return [...arranged.filter((id) => all.includes(id)), ...all.filter((id) => !arranged.includes(id))]
  })
  /** The tabs in order, the dashboard first. */
  const tabs = $derived<(number | null)[]>([null, ...order])
  /** Which panes there are and how they lie, not how big each is: a line moved is no new split. */
  const shape = $derived(`${layout.stacked} ${layout.groups.map((group) => group.panes.join()).join('|')}`)
  /** What is on screen, and where: the panes of the split the tab in front is in, or that tab alone. */
  const where = $derived<Record<number, Rect>>(
    active === null ? {} : split.includes(active) ? rects(layout) : { [active]: { x: 0, y: 0, w: 1, h: 1 } },
  )
  const front = $derived(sessions.find((session) => session.id === active))
  /** The view in front, as the rail names it. */
  const current = $derived(active === null ? 'dashboard' : front ? 'sessions' : 'files')
  /** What a session is called on screen: a host's alias, or what a quick connect was typed as, which is an address. */
  const named = (title: string) => (hosts.some((host) => host.alias === title) ? title : target(title))
  /** Or the name someone gave its tab. */
  const called = (session: Session) => session.name ?? named(session.title)
  const listed = $derived(sessions.map((session) => ({ ...session, title: called(session) })))
  /** On screen: the tab in front, and the panes it is open together with. */
  const seen = (id: number) => id in where

  /** The head of the dashboard: a line about its hosts. A tab's own head is its tab. */
  const head = $derived.by(() => {
    const lost = hosts.filter((host) => down(live[host.alias])).length
    const met = hosts.filter((host) => up(live[host.alias])).length
    return { title: t('status'), sub: `${hosts.length} ${t('hosts')} · ${met} ${t('reachable')} · ${lost} ${t('down')}` }
  })

  // --- What is open, kept for the next time the app is: the tabs of the config's hosts, their names, the split, focus mode.
  // A quick connect is not kept: nothing of one is written down.
  const KEPT = 'vssh.workspace'
  type Kept = { zen: boolean; tabs: { host: string; files?: boolean; name?: string }[]; layout: Layout; active: number }

  $effect(() => {
    if (!restored) return
    const kept = order.filter((id) => id < 0 || sessions.some((session) => session.id === id && !session.quick))
    const tabs = kept.map((id) => {
      const session = sessions.find((session) => session.id === id)
      return session ? { host: session.title, name: session.name } : { host: files.find((view) => view.id === id)!.host, files: true }
    })
    // By place in the strip: the ids are this run's.
    const at = (id: number) => (kept.includes(id) ? kept.indexOf(id) : undefined)
    const now: Kept = { zen, tabs, layout: mapped(layout, at), active: active === null ? -1 : (at(active) ?? -1) }
    localStorage.setItem(KEPT, JSON.stringify(now))
  })

  /** Opens again what was open when the app was closed. A host that is gone since is left out, and its pane with it. */
  async function restore() {
    try {
      const kept = JSON.parse(localStorage.getItem(KEPT) ?? 'null') as Kept | null
      if (kept) {
        const ids: (number | undefined)[] = []
        for (const tab of kept.tabs) {
          if (tab.files) {
            browse(tab.host)
            ids.push(files.find((view) => view.host === tab.host)!.id)
          } else {
            const id = await api.sessionOpen(tab.host).catch(() => undefined)
            if (id !== undefined) sessions.push({ id, title: tab.host, name: tab.name, broadcast: false, quick: false, ...fresh })
            ids.push(id)
          }
        }
        layout = mapped(kept.layout, (at) => ids[at])
        active = ids[kept.active] ?? null
        zen = !!kept.zen
        // A terminal in front has the keys, as it had when the app was closed.
        const id = active
        if (id !== null) void rendered().then(() => terms[id]?.focus())
      }
    } catch {
      // Not what this wrote: nothing is opened.
    }
    restored = true
  }

  /**
   * Whose files, whose forwards: the rail has no host of its own to answer with, so it asks. First the
   * host that is looked at (the session's in front, the card chosen), then those with a session, then the rest.
   */
  const asked = $derived.by(() => {
    const looked = active === null ? selected : front ? (front.quick ? null : front.title) : files.find((view) => view.id === active)?.host
    const open = [looked, ...sessions.filter((session) => !session.quick).map((session) => session.title)]
    const first = [...new Set(open)].flatMap((alias) => hosts.find((host) => host.alias === alias) ?? [])
    return [...first, ...hosts.filter((host) => !first.includes(host))]
  })

  /** A card's switch, or the one over every host (no alias): said to the shell, which closes what it kept, and read back. */
  async function watch(alias: string | null, off: boolean) {
    try {
      await api.watchSet(alias, off)
      if (alias === null) paused = off
      await reload()
    } catch (error) {
      await fail(error)
    }
  }

  /** A host's forwards, in their dialog. */
  function tunnel(alias: string) {
    forwarding = alias
    modal = 'forwards'
  }

  /** What the rail asks for: a view, or the dialog of what has no view of its own. */
  function go(to: Nav) {
    if (to === 'dashboard' || to === 'hosts') active = null
    // The hosts are the dashboard's: there, at its search.
    if (to === 'hosts') void rendered().then(() => dashboard?.seek())
    else if (to === 'sessions') active = sessions.at(-1)?.id ?? active
    else if (to === 'files') modal = 'pick-files'
    else if (to === 'snippets') modal = 'snippets'
    else if (to === 'forwards') modal = 'pick-forwards'
  }

  function step(by: number) {
    active = tabs[(tabs.indexOf(active) + by + tabs.length) % tabs.length]
  }

  /** The key after the leader: the TUI's own. */
  function command(e: KeyboardEvent) {
    if (e.ctrlKey && /^[a-z]$/i.test(e.key)) {
      // The key itself, for the shell: Ctrl-A Ctrl-A is Ctrl-A, and so for
      // the other keys the window keeps (Ctrl-W, Ctrl-K).
      if (front) input(front, String.fromCharCode(e.key.toUpperCase().charCodeAt(0) - 64))
    } else if (/^[0-9]$/.test(e.key)) {
      if (Number(e.key) < tabs.length) active = tabs[Number(e.key)]
    } else if (e.code === 'Tab' || e.key === 'n') {
      // By its place on the board: with Shift, WebKitGTK has no name for the key (ISO_Left_Tab).
      step(e.shiftKey ? -1 : 1)
    } else if (e.key === 'p') {
      step(-1)
    } else if (e.key === 'q') {
      active = null
    } else if (e.key === 'y') {
      if (front) terms[front.id]?.copyLast()
    } else if (e.key === 'x' || e.key === 'd') {
      if (active !== null) close(active)
    } else if (e.key === 'b') {
      if (front) front.broadcast = !front.broadcast
    } else if (e.key === 'B') {
      // Every tab at once, as the TUI's broadcast; off again if all are on.
      const on = sessions.some((session) => !session.broadcast)
      for (const session of sessions) session.broadcast = on
    } else if (e.key === '|' || e.key === '%') {
      void divide(false)
    } else if (e.key === '-' || e.key === '"') {
      void divide(true)
    } else if (e.key === '!') {
      unsplit()
    } else if (e.key === '{' || e.key === '}') {
      if (active !== null) layout = shift(layout, active, e.key === '{' ? -1 : 1)
    } else if (e.key === 'z') {
      zen = !zen
    } else if (e.key === '?') {
      modal = 'shortcuts'
    }
  }

  /** The keys of the window's size, with Ctrl: a step bigger, a step smaller, or (0) as it was made. */
  const SIZES = new Map<string, 1 | -1 | 0>([['=', 1], ['+', 1], ['-', -1], ['0', 0]])

  // Before the terminal sees the key: these never reach the shell.
  function onkeydown(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey
    const mine = () => {
      e.preventDefault()
      e.stopPropagation()
    }
    // The window's size, over a dialog and in a terminal too: no shell is sent these. (Ctrl-_, a shell's undo, is another key.)
    const size = ctrl && !e.altKey ? SIZES.get(e.key) : undefined
    if (size !== undefined) {
      mine()
      if (size) zoom(size)
      else setScale(100)
      return
    }
    // Not over an open dialog, nor in a field: there Ctrl-A selects the text.
    if (modal !== null || failure !== null || closing !== null || update.dialog) return
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement
    if (leader) {
      // A modifier going down is not the key yet.
      if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return
      mine()
      leader = false
      command(e)
    } else if (ctrl && e.key.toLowerCase() === 'k' && (e.shiftKey || !front)) {
      // In a terminal the plain key is the shell's (readline cuts to the end of the line): with Shift it is the window's.
      mine()
      modal = 'palette'
    } else if (ctrl && e.key === 'a' && !(e.target instanceof HTMLInputElement)) {
      mine()
      leader = true
    } else if (ctrl && e.key.toLowerCase() === 'w' && active !== null && (e.shiftKey || !front)) {
      // As Ctrl-K: in a terminal Ctrl-W is the word before the cursor, and not the end of the session.
      mine()
      close(active)
    } else if (e.ctrlKey && e.code === 'Tab') {
      // No shell is sent this one: it is the window's in a terminal too.
      mine()
      step(e.shiftKey ? -1 : 1)
    } else if (e.ctrlKey && e.shiftKey && ['B', 'D', 'E'].includes(e.key.toUpperCase())) {
      // With Shift a shell has no use for them: the window's, in a terminal too. (Caps Lock turns the letter over.)
      mine()
      if (e.key.toUpperCase() === 'B') zen = !zen
      else void divide(e.key.toUpperCase() === 'E')
    } else if (ctrl && e.key.toLowerCase() === 'b' && !front) {
      // Plain Ctrl-B is a shell's (and the prefix of a tmux over there): not from a terminal.
      mine()
      zen = !zen
    } else if (zen && e.key === 'Escape' && !front && !typing) {
      // Esc is vim's in a terminal; anywhere else it is also the way out. Not `mine`: what else it closes, it closes.
      zen = false
    } else if (front) {
      // The rest are a shell's own keys (Ctrl-F pages in less, F1 is htop's
      // help): from a terminal the cheatsheet is Ctrl-A ?, a new tab the palette.
    } else if (ctrl && e.key === 't') {
      mine()
      modal = 'newtab'
    } else if (e.key === 'F1' || (ctrl && e.key === '/') || (e.key === '?' && !typing)) {
      mine()
      modal = 'shortcuts'
    }
  }

  function add(id: number, title: string, quick = false) {
    sessions.push({ id, title, broadcast: false, quick, ...fresh })
    active = id
  }

  /** Puts a tab at an edge of a pane (the one in front, if none is named), or in its place: the two are panes of one split. */
  function join(id: number, at: Zone, beside = active) {
    if (beside === null || id === beside) return
    layout = place(layout, id, at, beside)
    active = id
  }

  /** Another shell on the host of the tab in front, beside it or under it; beside a host's files, its shell. */
  async function divide(down: boolean) {
    // As they are now: by the time the shell answers, another tab may be in front.
    const beside = active
    const quick = front?.quick
    const title = front?.title ?? files.find((view) => view.id === beside)?.host
    if (beside === null || title === undefined) return
    try {
      const id = await (quick ? api.quickConnect(title) : api.sessionOpen(title))
      add(id, title, quick)
      join(id, down ? 'bottom' : 'right', beside)
    } catch (error) {
      await fail(error)
    }
  }

  /** The pane in front leaves the split for a tab of its own; two are needed for a split. */
  function unsplit(id = active) {
    if (id !== null) layout = without(layout, id)
  }

  /** A tab let go on another in the strip: it stands before that one now, or after it. */
  function move(id: number, onto: number, after: boolean) {
    const next = order.filter((tab) => tab !== id)
    next.splice(next.indexOf(onto) + Number(after), 0, id)
    arranged = next
  }

  /** The pane the pointer is in, and where in it; `null` once it is outside them all. */
  function under(e: DragEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect()
    return zone(where, (e.clientX - box.left) / (box.width || 1), (e.clientY - box.top) / (box.height || 1))
  }

  /**
   * A tab in hand is over the panes: they are a place to drop it, and the part it would take is shown.
   * For entering as for moving: a web view asks at both, and over a terminal, where every row is an
   * element of its own, the last thing it asked before the button is let go is mostly the first.
   */
  function hover(e: DragEvent) {
    if (dragging === null) return
    e.preventDefault()
    const at = under(e)
    // Over its own pane it would land where it is.
    aim = at && at.id !== dragging ? at : null
  }

  /** A line between two panes, held: it follows the pointer, and the panes on either side of it give and take. */
  function size(e: PointerEvent, bar: Bar) {
    if (e.type === 'pointerdown') {
      ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
      sizing = { bar, from: bar.upright ? e.clientX : e.clientY, start: layout }
    } else if (e.type === 'pointermove' && sizing) {
      const box = frame!.getBoundingClientRect()
      const by = ((bar.upright ? e.clientX : e.clientY) - sizing.from) / ((bar.upright ? box.width : box.height) || 1)
      layout = resize(sizing.start, sizing.bar, by)
    } else if (e.type !== 'pointermove' && sizing) {
      sizing = null
      // The line took the keys when it was pressed.
      if (front) terms[front.id]?.focus()
    }
  }

  /** The same by the arrows, a twentieth at a time. */
  function nudge(e: KeyboardEvent, bar: Bar) {
    const by = ((bar.upright ? { ArrowLeft: -1, ArrowRight: 1 } : { ArrowUp: -1, ArrowDown: 1 }) as Record<string, number>)[e.key]
    if (!by) return
    e.preventDefault()
    layout = resize(layout, bar, by / 20)
  }

  const pct = (r: Rect) => `left: ${r.x * 100}%; top: ${r.y * 100}%; width: ${r.w * 100}%; height: ${r.h * 100}%`

  /** A pane's own box: where it is among the others, and a click or the keys in it bring it to the front. */
  const pane = (id: number) => ({
    hidden: !seen(id),
    'data-front': (split.length > 1 && id === active) || undefined,
    'data-back': (split.length > 1 && id !== active) || undefined,
    style: seen(id) ? pct(where[id]) : undefined,
    onfocusin: () => (active = id),
    onpointerdown: () => (active = id),
    class:
      "absolute flex min-h-0 min-w-0 flex-col bg-page after:pointer-events-none after:absolute after:inset-x-0 after:top-0 after:z-10 after:h-0.5 data-[back]:*:opacity-70 data-[front]:after:bg-blue",
  })

  /** Opens a tab on a host of the config. Its terminal connects it. */
  async function open(alias: string) {
    try {
      add(await api.sessionOpen(alias), alias)
    } catch (error) {
      await fail(error)
    }
  }

  /** Rejects with the refusal in words, for the form to show. */
  async function connect(target: string) {
    try {
      add(await api.quickConnect(target), target, true)
      modal = null
    } catch (error) {
      throw await explain(error, i18n.lang)
    }
  }

  /** A host's files, in the tab they are already open in if there is one. */
  function browse(alias: string) {
    const open = files.find((view) => view.host === alias)
    const id = open?.id ?? Math.min(0, ...files.map((view) => view.id)) - 1
    if (!open) files.push({ id, host: alias })
    active = id
  }

  /** Closes a tab; the one that takes its place comes to the front. One whose shell runs something is asked about first, unless `sure`. */
  function close(id: number, sure = false) {
    const at = tabs.indexOf(id)
    if (at < 0) return
    const session = sessions.find((session) => session.id === id)
    if (!sure && session?.busy && session.state === 'up') {
      closing = id
      return
    }
    // A pane that goes leaves the front to one of those it was with.
    const kin = split.includes(id) ? split.find((pane) => pane !== id) : undefined
    unsplit(id)
    if (id < 0) {
      files = files.filter((view) => view.id !== id)
    } else {
      sessions = sessions.filter((session) => session.id !== id)
      delete terms[id]
      void api.sessionClose(id)
    }
    // `tabs` is the tabs that are left now.
    if (active === id) active = kin ?? tabs[at] ?? tabs[at - 1] ?? null
  }

  /** A tab's own name; none: its host's again. The keys go back to its terminal. */
  function rename(id: number, name: string) {
    const session = sessions.find((session) => session.id === id)
    if (session) session.name = name || undefined
    if (id === active) terms[id]?.focus()
  }

  /** Opens the host form: on a host, or empty, or with what a quick connect was typed as. */
  function edit(alias?: string, from?: Session) {
    editing = alias
    start = from && fromTarget(from.title)
    keeping = from
    modal = 'host'
  }

  /** The hosts changed here and now: read at once, and this one is the selected one. */
  async function changed(alias?: string) {
    modal = null
    // The quick connect is a host's session from now on.
    if (keeping) keeping.quick = false
    await reload()
    if (alias) selected = alias
  }

  /** A host is gone: so are its sessions and its files, as the tabs of a host that is not there. */
  async function deleted(alias: string) {
    for (const session of sessions.filter((session) => !session.quick && session.title === alias)) close(session.id, true)
    for (const view of files.filter((view) => view.host === alias)) close(view.id)
    marked = marked.filter((name) => name !== alias)
    forwarding_of(alias, [])
    if (selected === alias) selected = null
    await changed()
  }

  /** What was typed into a session's terminal. */
  function input(session: Session, data: string) {
    // From a broadcasting tab, to every broadcasting tab.
    const to = session.broadcast ? sessions.filter((other) => other.broadcast) : [session]
    for (const { id } of to) void api.sessionWrite(id, data)
  }

  function heard(session: Session, said: PtyState) {
    if (said.state === 'up') Object.assign(session, { state: 'up', met: true, error: null, why: null })
    else if (said.state === 'busy') session.busy = said.command
    else {
      // One that never got through says why in a dialog; one that was up
      // and dropped keeps its screen, and says it there.
      if (said.error && !session.met) failure = active = session.id
      Object.assign(session, { state: 'closed', busy: null, error: said.error })
    }
  }

  const answer = 'cursor-pointer rounded-ctl border border-edge min-h-9 px-3.5 py-1.5 font-medium hover:bg-selection'

  function reconnect(session: Session) {
    failure = null
    Object.assign(session, fresh)
    void terms[session.id]?.start()
  }

  /** The user's yes to a host key, then the connection it was needed for. */
  async function trust(session: Session, host: string, fingerprint: string, forever: boolean) {
    try {
      await api.sessionTrust(session.id, host, fingerprint, forever)
      reconnect(session)
    } catch (error) {
      session.why = await explain(error, i18n.lang)
    }
  }
</script>

<svelte:window
  onkeydowncapture={onkeydown}
  oncontextmenu={(e) => {
    // The web view's own menu has Reload in it, which would end every session. A field keeps its own: cut, copy, paste.
    if (!(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) e.preventDefault()
  }}
/>

<!-- The rail's column goes from all of it to its icons without a jump. -->
<div
  class="grid h-full grid-rows-[minmax(0,1fr)_auto] transition-[grid-template-columns] duration-200 motion-reduce:transition-none {zen
    ? 'grid-cols-[minmax(0,1fr)]'
    : compact
      ? 'grid-cols-[3.5rem_minmax(0,1fr)]'
      : 'grid-cols-[15.25rem_minmax(0,1fr)]'}"
>
  {#if !zen}
  <Sidebar
    {hosts}
    bind:tag
    sessions={listed}
    {active}
    onsession={(id) => (active = id)}
    {vault}
    onvault={() => (modal = 'vault')}
    {current}
    snippets={snippets.length}
    tunnels={tunnels.size}
    onnav={go}
    {compact}
    oncompact={(on) => {
      compact = on
      localStorage.setItem('vssh.rail', on ? 'compact' : 'full')
    }}
    onpalette={() => (modal = 'palette')}
    onshortcuts={() => (modal = 'shortcuts')}
    onzen={() => (zen = true)}
    {paused}
    onpause={(off) => void watch(null, off)}
  />
  {/if}

  <!-- min-w-0: what is in it (a long path, a wide terminal) scrolls in its place and does not widen the window's page. -->
  <main class="flex min-h-0 min-w-0 flex-col bg-page">
    <!-- The tabs are the top row of every view, and a tab's view has no other head: no band, no margin, no frame around the frame. -->
    <!-- In focus mode the strip is also the way back to the rail. -->
    {#if sessions.length || files.length || zen}
        <Tabs
          sessions={listed}
          {files}
          {order}
          {active}
          {split}
          {zen}
          leader={zen && leader}
          onpick={(id) => (active = id)}
          onclose={close}
          onnew={() => (modal = 'newtab')}
          onsplit={divide}
          onunsplit={() => unsplit()}
          onzen={() => (zen = false)}
          onrename={rename}
          ondrag={(id) => {
            dragging = id
            aim = null
          }}
          onmove={move}
          oncloseothers={(id) => {
            // One whose shell is busy is asked about, and stays until somebody says yes.
            for (const other of order) if (other !== id) close(other)
            active = id
          }}
          onbroadcast={(on) => {
            // Every session, as the TUI's broadcast; the keys go back to the terminal that is typed in.
            for (const session of sessions) session.broadcast = on
            if (front) terms[front.id]?.focus()
          }}
        />
    {/if}
    {#if active === null}
      <Topbar {...head} />
    {/if}
    <!-- Behind a terminal the dashboard keeps its search, its order and its place. -->
    <Dashboard
      bind:this={dashboard}
      {hosts}
      bind:tag
      {selected}
      hidden={active !== null}
      error={failed}
      onselect={(alias) => (selected = alias)}
      onconnect={open}
      onquick={connect}
      onfiles={browse}
      onforwards={tunnel}
      onsnippets={() => (modal = 'snippets')}
      onbatch={() => {
        if (!picked.length) return
        job = { hosts: picked, command: '', params: {} }
        modal = 'batch'
      }}
      onpalette={() => (modal = 'palette')}
      onadd={() => edit()}
      onexport={() => (modal = 'export')}
      onimport={() => (modal = 'import')}
      onedit={edit}
      ondelete={(alias) => {
        editing = alias
        modal = 'delete'
      }}
      bind:marked
      {paused}
      onwatch={(alias, watched) => void watch(alias, !watched)}
      onpause={(off) => void watch(null, off)}
    />
    <!-- The panes: the tab in front, and those it is open together with. A tab dragged to an edge of one joins it there; to its middle, takes its place. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={frame}
      hidden={active === null}
      data-panes
      data-stacked={(split.length > 1 && layout.stacked) || undefined}
      ondragenter={hover}
      ondragover={hover}
      ondragleave={(e) => {
        // By where the pointer is: WebKit names no `relatedTarget`, and every pane and every row in it is left on the way across.
        if (!under(e)) aim = null
      }}
      ondrop={(e) => {
        e.preventDefault()
        // Where it is let go, not where it was last seen: a leave may have come in between.
        const at = under(e)
        if (dragging !== null && at) join(dragging, at.zone, at.id)
        dragging = aim = null
      }}
      class="relative min-h-0 flex-1 bg-page"
    >
      {#each sessions as session (session.id)}
        <div {...pane(session.id)}>
          <Terminal
            bind:this={terms[session.id]}
            id={session.id}
            label={called(session)}
            active={seen(session.id)}
            front={session.id === active}
            closed={session.state === 'closed'}
            why={session.why}
            oninput={(data) => input(session, data)}
            onstate={(said) => heard(session, said)}
            onreconnect={() => reconnect(session)}
            broadcast={sessions.length > 1 && sessions.some((other) => other.broadcast) ? session.broadcast : undefined}
            onbroadcast={() => (session.broadcast = !session.broadcast)}
          />
        </div>
      {/each}
      {#each files as view (view.id)}
        <div {...pane(view.id)}>
          <Files host={view.host} active={seen(view.id)} front={view.id === active} />
        </div>
      {/each}
      {#if active !== null && split.includes(active)}
        <!-- The lines between the panes: each can be held and moved, by the pointer or the arrows. -->
        {#each bars(layout) as bar (`${bar.group} ${bar.g} ${bar.at}`)}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <div
            role="separator"
            aria-label={t('resize_panes')}
            aria-orientation={bar.upright ? 'vertical' : 'horizontal'}
            tabindex="0"
            style={pct(bar.rect)}
            onpointerdown={(e) => size(e, bar)}
            onpointermove={(e) => size(e, bar)}
            onpointerup={(e) => size(e, bar)}
            onpointercancel={(e) => size(e, bar)}
            onkeydown={(e) => nudge(e, bar)}
            class="absolute z-10 touch-none outline-none before:absolute before:bg-edge hover:before:bg-blue focus-visible:before:bg-blue active:before:bg-blue {bar.upright
              ? '-ml-1 min-w-2 cursor-col-resize before:inset-y-0 before:left-[calc(50%-0.5px)] before:w-px hover:before:w-0.5 active:before:w-0.5'
              : '-mt-1 min-h-2 cursor-row-resize before:inset-x-0 before:top-[calc(50%-0.5px)] before:h-px hover:before:h-0.5 active:before:h-0.5'}"
          ></div>
        {/each}
      {/if}
      {#if aim && where[aim.id]}
        <!-- Where the tab will land: that half of the pane, or all of it for a change of places. A line and a tint, no shadow. -->
        <div data-edge={aim.zone} style={pct(half(where[aim.id], aim.zone))} class="pointer-events-none absolute z-20 rounded-lg border-2 border-blue bg-blue/15"></div>
      {/if}
    </div>
  </main>

  {#if !zen}
  <Statusbar
    hosts={hosts.length}
    snippets={snippets.length}
    sessions={sessions.length}
    selected={active === null ? hosts.find((host) => host.alias === selected) : undefined}
    broadcasting={sessions.filter((session) => session.broadcast).length}
    {leader}
    onsnippets={() => (modal = 'snippets')}
    onsave={front?.quick ? () => edit(undefined, front) : undefined}
  />
  {/if}
</div>

{#if failed_session?.error && failed_session.why}
  {@const session = failed_session}
  <Failure
    title={called(session)}
    error={failed_session.error}
    why={failed_session.why}
    ontrust={(key, forever) => trust(session, key.host, key.fingerprint, forever)}
    onretry={() => reconnect(session)}
    onclose={() => (failure = null)}
  />
{/if}

{#if update.dialog}
  <Updated />
{/if}

{#if closing !== null}
  {@const session = sessions.find((session) => session.id === closing)}
  <Modal label={t('close_tab')} onclose={() => (closing = null)}>
    <form
      class="grid gap-4 p-7"
      onsubmit={(e) => {
        e.preventDefault()
        const id = closing
        closing = null
        if (id !== null) close(id, true)
      }}
    >
      <p>{t('close_busy').replace('{command}', session?.busy ?? '').replace('{name}', session ? called(session) : '')}</p>
      <!-- Cancel comes first and has the focus: Enter is not a yes. -->
      <div class="flex justify-end gap-2">
        <button type="button" onclick={() => (closing = null)} class="{answer} text-white">{t('cancel')}</button>
        <button type="submit" class="{answer} border-red text-red">{t('close_tab')}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if modal === 'palette' || modal === 'newtab' || modal === 'pick-files' || modal === 'pick-forwards'}
  {@const whose = modal === 'pick-files' ? t('files') : modal === 'pick-forwards' ? t('forwards') : undefined}
  <Palette
    hosts={whose ? asked : hosts}
    pick={whose && `${whose}: ${t('which_host')}`}
    onhost={(alias) => {
      const was = modal
      modal = null
      // Asked for a tab: the host is connected to. Asked whose: its files, its forwards.
      if (was === 'newtab') return void open(alias)
      if (was === 'pick-files') return browse(alias)
      if (was === 'pick-forwards') return tunnel(alias)
      // Its card is on the dashboard: from a terminal, that comes forward.
      selected = alias
      active = null
      // Also when it was the selected one already; once the palette is gone, which held the focus.
      void rendered().then(() => dashboard?.focus(alias))
    }}
    onconnect={connect}
    onsnippets={() => (modal = 'snippets')}
    onadd={() => edit()}
    onvault={() => (modal = 'vault')}
    onshortcuts={() => (modal = 'shortcuts')}
    onzen={() => {
      zen = !zen
      modal = null
    }}
    onsplit={active === null
      ? undefined
      : (down) => {
          modal = null
          void divide(down)
        }}
    onclose={() => (modal = null)}
  />
{:else if modal === 'shortcuts'}
  <Shortcuts view={current} onclose={() => (modal = null)} />
{:else if modal === 'host'}
  <!-- Keyed: the form of another host is another form. -->
  {#key editing}
    <HostForm
      {hosts}
      alias={editing}
      {start}
      {vault}
      onsaved={(host) => changed(host.alias)}
      ondelete={() => (modal = 'delete')}
      onkeys={() => (modal = 'keys')}
      onimported={(aliases) => changed(aliases[0])}
      onclose={() => (modal = null)}
    />
  {/key}
{:else if modal === 'export' || modal === 'import'}
  <HostsJson mode={modal} onimported={(aliases) => changed(aliases[0])} onclose={() => (modal = null)} />
{:else if modal === 'delete' && editing}
  <HostDelete alias={editing} ondeleted={deleted} onclose={() => (modal = null)} />
{:else if modal === 'keys' && editing}
  <!-- The shell names the key in the host's block: the hosts are read again. -->
  <KeySetup host={editing} onclose={() => changed()} />
{:else if modal === 'vault'}
  <Vault
    bind:vault
    hosts={hosts.length}
    once={greeting}
    onclose={() => {
      modal = null
      // Asked as the app opened: answered or left, what was open comes back now.
      if (greeting) void restore()
      greeting = false
    }}
  />
{:else if modal === 'snippets'}
  <Snippets
    bind:snippets
    {hosts}
    picked={picked.map((host) => host.alias)}
    onrun={(names, command, params) => {
      job = { hosts: hosts.filter((host) => names.includes(host.alias)), command, params }
      modal = 'batch'
    }}
    onclose={() => (modal = null)}
  />
{:else if modal === 'batch' && job}
  <Batch {...job} onclose={() => (modal = null)} />
{:else if modal === 'forwards'}
  <Forwards host={forwarding} onclose={() => (modal = null)} />
{/if}
