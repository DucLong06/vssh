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
  import KeySetup from './lib/KeySetup.svelte'
  import { fromTarget } from './lib/hosts'
  import { i18n, t } from './lib/i18n.svelte'
  import { down, live, poll, relabel, up } from './lib/live.svelte'
  import Palette from './lib/Palette.svelte'
  import QuickConnect from './lib/QuickConnect.svelte'
  import Shortcuts from './lib/Shortcuts.svelte'
  import Sidebar, { type Nav } from './lib/Sidebar.svelte'
  import Batch from './lib/Batch.svelte'
  import Snippets from './lib/Snippets.svelte'
  import Statusbar from './lib/Statusbar.svelte'
  import { learn, streamer, target } from './lib/streamer.svelte'
  import Tabs, { type Tab } from './lib/Tabs.svelte'
  import Terminal from './lib/Terminal.svelte'
  import Topbar from './lib/Topbar.svelte'
  import { heard as forwarded, sync as forwarding_of, tunnels } from './lib/tunnels.svelte'
  import Vault from './lib/Vault.svelte'

  /** How often the hosts are asked for their figures, and every how many rounds pinged. */
  const EVERY = 5000
  const PING_ROUNDS = 12

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
  /** The tabs that are open together as panes, in their order; none: one tab at a time. */
  let split = $state<number[]>([])
  /** The panes are one over the other, not side by side. */
  let stacked = $state(false)
  /** A tab is being dragged, and the edge of the panes it is over. */
  type Edge = 'left' | 'right' | 'top' | 'bottom'
  let dragging = $state<number | null>(null)
  let edge = $state<Edge | null>(null)
  /** Focus mode: no rail and no status bar, and a tab's view from edge to edge. */
  let zen = $state(false)
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
  type Dialog = 'palette' | 'quick' | 'snippets' | 'batch' | 'forwards' | 'vault' | 'host' | 'delete' | 'keys' | 'shortcuts'
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
    const tick = async () => {
      await reload()
      poll(
        hosts.map((host) => host.alias),
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
    }, fail)
    const timer = setInterval(tick, EVERY)
    const forwards = listen<ForwardState>('forward-state', ({ payload }) => forwarded(payload))
    return () => {
      clearInterval(timer)
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
    void [zen, split, stacked, sessions.length]
    const id = untrack(() => front?.id)
    if (id !== undefined) terms[id]?.focus()
  })

  /** The tabs in order: the dashboard, the sessions, the file views. */
  const tabs = $derived<(number | null)[]>([null, ...[...sessions, ...files].map((tab) => tab.id)])
  const front = $derived(sessions.find((session) => session.id === active))
  /** The view in front, as the rail names it. */
  const current = $derived(active === null ? 'dashboard' : front ? 'sessions' : 'files')
  /** What a session is called on screen: a host's alias, or what a quick connect was typed as, which is an address. */
  const named = (title: string) => (hosts.some((host) => host.alias === title) ? title : target(title))
  /** Or the name someone gave its tab. */
  const called = (session: Session) => session.name ?? named(session.title)
  const listed = $derived(sessions.map((session) => ({ ...session, title: called(session) })))
  /** On screen: the tab in front, and the panes it is open together with. */
  const seen = (id: number) => id === active || (active !== null && split.includes(active) && split.includes(id))

  /** The head of the view in front: what it is called, and a line about it. */
  const head = $derived.by(() => {
    if (front) {
      // What a quick connect was typed as is an address: letter by letter.
      const said = { connecting: t('connecting'), up: t('connected'), closed: t('disconnected') }[front.state]
      return { title: called(front), sub: front.busy ? `${said} · ${t('running')} ${front.busy}` : said, mono: front.quick && !front.name }
    }
    const view = files.find((view) => view.id === active)
    if (view) return { title: t('files'), sub: view.host }
    const lost = hosts.filter((host) => down(live[host.alias])).length
    const met = hosts.filter((host) => up(live[host.alias])).length
    return { title: t('status'), sub: `${hosts.length} ${t('hosts')} · ${met} ${t('reachable')} · ${lost} ${t('down')}` }
  })

  /** What the rail asks for: a view, or the dialog of what has no view of its own. */
  function go(to: Nav) {
    if (to === 'dashboard' || to === 'hosts') active = null
    // The list of hosts is the dashboard's: there, at its search.
    if (to === 'hosts') void rendered().then(() => dashboard?.seek())
    else if (to === 'sessions') active = sessions.at(-1)?.id ?? active
    else if (to === 'files' && selected) browse(selected)
    else if (to === 'snippets') modal = 'snippets'
    else if (to === 'forwards' && selected) {
      forwarding = selected
      modal = 'forwards'
    }
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
      swap(e.key === '{' ? -1 : 1)
    } else if (e.key === 'z') {
      zen = !zen
    } else if (e.key === '?') {
      modal = 'shortcuts'
    }
  }

  // Before the terminal sees the key: these never reach the shell.
  function onkeydown(e: KeyboardEvent) {
    // Not over an open dialog, nor in a field: there Ctrl-A selects the text.
    if (modal !== null || failure !== null) return
    const ctrl = e.ctrlKey || e.metaKey
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement
    const mine = () => {
      e.preventDefault()
      e.stopPropagation()
    }
    if (leader) {
      // A modifier going down is not the key yet.
      if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return
      mine()
      leader = false
      command(e)
    } else if (ctrl && e.key === 'k') {
      mine()
      modal = 'palette'
    } else if (ctrl && e.key === 'a' && !(e.target instanceof HTMLInputElement)) {
      mine()
      leader = true
    } else if (ctrl && e.key === 'w' && active !== null) {
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
      modal = 'quick'
    } else if (e.key === 'F1' || (ctrl && e.key === '/') || (e.key === '?' && !typing)) {
      mine()
      modal = 'shortcuts'
    }
  }

  function add(id: number, title: string, quick = false) {
    sessions.push({ id, title, broadcast: false, quick, ...fresh })
    active = id
  }

  /** Puts a tab at an edge of what is in front: the two are panes of one split, or change places in it. */
  function join(id: number, at: Edge, beside = active) {
    if (beside === null) return
    const rest = (split.includes(beside) ? split : [beside]).filter((pane) => pane !== id)
    // On itself, alone: nothing to be beside.
    if (!rest.length) return
    stacked = at === 'top' || at === 'bottom'
    split = at === 'left' || at === 'top' ? [id, ...rest] : [...rest, id]
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
    split = split.filter((pane) => pane !== id)
    if (split.length < 2) split = []
  }

  /** The pane in front changes places with the one before it, or after. */
  function swap(by: number) {
    const at = active === null ? -1 : split.indexOf(active)
    const to = at + by
    if (at < 0 || to < 0 || to >= split.length) return
    ;[split[at], split[to]] = [split[to], split[at]]
  }

  /** The edge of the panes the pointer is nearest to; `null` once it is outside them. */
  function nearest(e: DragEvent): Edge | null {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = (e.clientX - box.left) / (box.width || 1)
    const y = (e.clientY - box.top) / (box.height || 1)
    if (x < 0 || x > 1 || y < 0 || y > 1) return null
    const far: [number, Edge][] = [[x, 'left'], [1 - x, 'right'], [y, 'top'], [1 - y, 'bottom']]
    return far.sort((a, b) => a[0] - b[0])[0][1]
  }

  /**
   * A tab in hand is over the panes: they are a place to drop it, and the half it would take is shown.
   * For entering as for moving: a web view asks at both, and over a terminal, where every row is an
   * element of its own, the last thing it asked before the button is let go is mostly the first.
   */
  function hover(e: DragEvent) {
    if (dragging === null) return
    e.preventDefault()
    edge = nearest(e)
  }

  /** A pane's own box: where it is among the others, and a click or the keys in it bring it to the front. */
  const pane = (id: number) => ({
    hidden: !seen(id),
    'data-front': (split.length > 1 && id === active) || undefined,
    'data-back': (split.length > 1 && id !== active) || undefined,
    style: `order: ${split.indexOf(id)}`,
    onfocusin: () => (active = id),
    onpointerdown: () => (active = id),
    class:
      "relative flex min-h-0 min-w-0 flex-1 basis-0 flex-col bg-page after:pointer-events-none after:absolute after:inset-x-0 after:top-0 after:z-10 after:h-0.5 data-[back]:*:opacity-70 data-[front]:after:bg-blue",
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

  /** Closes a tab at once, as the TUI does; the one that takes its place comes to the front. */
  function close(id: number) {
    const at = tabs.indexOf(id)
    if (at < 0) return
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
    for (const session of sessions.filter((session) => !session.quick && session.title === alias)) close(session.id)
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

<svelte:window onkeydowncapture={onkeydown} />

<div class="grid h-full grid-rows-[minmax(0,1fr)_auto] {zen ? 'grid-cols-[minmax(0,1fr)]' : 'grid-cols-[244px_minmax(0,1fr)]'}">
  {#if !zen}
  <Sidebar
    {hosts}
    {selected}
    bind:tag
    onselect={(alias) => (selected = alias)}
    sessions={listed}
    {active}
    onsession={(id) => (active = id)}
    {vault}
    onvault={() => (modal = 'vault')}
    {current}
    snippets={snippets.length}
    tunnels={tunnels.size}
    onnav={go}
    onzen={() => (zen = true)}
  />
  {/if}

  <!-- min-w-0: what is in it (a long path, a wide terminal) scrolls in its place and does not widen the window's page. -->
  <main class="flex min-h-0 min-w-0 flex-col bg-page">
    <!-- In focus mode a tab's view is all there is: the tabs are its head. -->
    {#if !zen || active === null}
      <Topbar {...head} onpalette={() => (modal = 'palette')} onshortcuts={() => (modal = 'shortcuts')} />
    {/if}
    <!-- A terminal or a file view is one frame with the tabs for its head; the dashboard is the page itself. -->
    <div
      data-framed={(active !== null && !zen) || undefined}
      class="group/frame flex min-h-0 flex-1 flex-col data-[framed]:mx-8 data-[framed]:mt-5 data-[framed]:mb-6 data-[framed]:overflow-hidden data-[framed]:rounded-xl data-[framed]:border data-[framed]:border-edge"
    >
    <!-- In focus mode the strip is also the way back to the rail. -->
    {#if sessions.length || files.length || zen}
      <!-- Over the dashboard the tabs are a strip of their own in the header's band. -->
      <div class={active === null ? 'bg-bg px-8 pb-4 *:rounded-xl *:border' : ''}>
        <Tabs
          sessions={listed}
          {files}
          {active}
          {split}
          {zen}
          leader={zen && leader}
          onpick={(id) => (active = id)}
          onclose={close}
          onnew={() => (modal = 'quick')}
          onsplit={divide}
          onunsplit={() => unsplit()}
          onzen={() => (zen = false)}
          onrename={rename}
          ondrag={(id) => {
            dragging = id
            edge = null
          }}
        />
      </div>
    {/if}
    <!-- Behind a terminal the dashboard keeps its search, its order and its place. -->
    <Dashboard
      bind:this={dashboard}
      {hosts}
      {tag}
      {selected}
      hidden={active !== null}
      error={failed}
      onselect={(alias) => (selected = alias)}
      onconnect={open}
      onfiles={browse}
      onforwards={(alias) => {
        forwarding = alias
        modal = 'forwards'
      }}
      onsnippets={() => (modal = 'snippets')}
      onbatch={() => {
        if (!picked.length) return
        job = { hosts: picked, command: '', params: {} }
        modal = 'batch'
      }}
      onpalette={() => (modal = 'palette')}
      onadd={() => edit()}
      onedit={edit}
      ondelete={(alias) => {
        editing = alias
        modal = 'delete'
      }}
      bind:marked
    />
    <!-- The panes: the tab in front, and those it is open together with. A tab dragged to an edge of them joins them there. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      hidden={active === null}
      data-panes
      ondragenter={hover}
      ondragover={hover}
      ondragleave={(e) => {
        // By where the pointer is: WebKit names no `relatedTarget`, and every pane and every row in it is left on the way across.
        if (!nearest(e)) edge = null
      }}
      ondrop={(e) => {
        e.preventDefault()
        // Where it is let go, not where it was last seen: a leave may have come in between.
        const at = nearest(e)
        if (dragging !== null && at) join(dragging, at)
        dragging = edge = null
      }}
      class="relative flex min-h-0 flex-1 gap-px bg-edge {stacked ? 'flex-col' : ''}"
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
          />
        </div>
      {/each}
      {#each files as view (view.id)}
        <div {...pane(view.id)}>
          <Files host={view.host} active={seen(view.id)} front={view.id === active} />
        </div>
      {/each}
      {#if edge}
        <!-- Where the tab will land: that half of the panes. -->
        <div
          data-edge={edge}
          class="pointer-events-none absolute z-20 rounded-lg border-2 border-blue bg-blue/15 data-[edge=bottom]:inset-[50%_0_0_0] data-[edge=left]:inset-[0_50%_0_0] data-[edge=right]:inset-[0_0_0_50%] data-[edge=top]:inset-[0_0_50%_0]"
        ></div>
      {/if}
    </div>
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

{#if modal === 'palette'}
  <Palette
    {hosts}
    onhost={(alias) => {
      // Its card is on the dashboard: from a terminal, that comes forward.
      selected = alias
      active = null
      modal = null
      // Also when it was the selected one already; once the palette is gone, which held the focus.
      void rendered().then(() => dashboard?.focus(alias))
    }}
    onquick={() => (modal = 'quick')}
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
      greeting = false
    }}
  />
{:else if modal === 'quick'}
  <QuickConnect onconnect={connect} onclose={() => (modal = null)} />
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
