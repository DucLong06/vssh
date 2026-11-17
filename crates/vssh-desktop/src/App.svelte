<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type ConnError, type Host, type PtyState, type Shown, type Snippet } from './lib/api'
  import Dashboard from './lib/Dashboard.svelte'
  import { i18n, t } from './lib/i18n.svelte'
  import { poll, relabel } from './lib/live.svelte'
  import Palette from './lib/Palette.svelte'
  import QuickConnect from './lib/QuickConnect.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Statusbar from './lib/Statusbar.svelte'
  import Tabs, { type Tab } from './lib/Tabs.svelte'
  import Terminal from './lib/Terminal.svelte'

  /** How often the hosts are asked for their figures, and every how many rounds pinged. */
  const EVERY = 5000
  const PING_ROUNDS = 12

  let hosts = $state<Host[]>([])
  let snippets = $state<Snippet[]>([])
  /** A tab's session. `met`: its shell came up since it was last started. */
  type Session = Tab & { met: boolean; error: ConnError | null; why: Shown | null }
  const fresh = { state: 'connecting', busy: null, met: false, error: null, why: null } as const
  let sessions = $state<Session[]>([])
  /** The session whose tab is in front; `null` = the dashboard. */
  let active = $state<number | null>(null)
  /** Ctrl-A was pressed: the next key is a command, as in the TUI. */
  let leader = $state(false)
  /** The terminals, by session. */
  const terms: Record<number, Terminal> = {}
  let selected = $state<string | null>(null)
  /** The tag the sidebar narrowed the hosts to. */
  let tag = $state<string | null>(null)
  let modal = $state<'palette' | 'quick' | null>(null)
  /** Why the hosts could not be read. */
  let failed = $state<Shown | null>(null)

  const fail = async (error: unknown) => (failed = await explain(error, i18n.lang))

  /** Reads the hosts again: the ssh config or the TUI's database may have changed. */
  async function reload() {
    try {
      const next = await api.listHosts()
      // shortcut: asked every round instead of watching the two files; a
      // change shows up within EVERY. Watch them (notify + an event) if that lag matters.
      if (JSON.stringify(next) !== JSON.stringify(hosts)) hosts = next
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
    const timer = setInterval(tick, EVERY)
    return () => clearInterval(timer)
  })

  // A failure is kept as data: the words for it follow the language.
  $effect(() => void relabel(i18n.lang))

  // Why a session ended, in the window's language.
  $effect(() => {
    for (const session of sessions) {
      const error = session.error
      if (!error) continue
      void explain(error, i18n.lang).then((why) => {
        if (session.error === error) session.why = why
      })
    }
  })

  /** The tabs in order: the dashboard, then the sessions. */
  const tabs = $derived<(number | null)[]>([null, ...sessions.map((session) => session.id)])
  const front = $derived(sessions.find((session) => session.id === active))

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
    } else if (e.key === 'Tab' || e.key === 'n') {
      step(e.shiftKey ? -1 : 1)
    } else if (e.key === 'p') {
      step(-1)
    } else if (e.key === 'q') {
      active = null
    } else if (e.key === 'y') {
      if (front) terms[front.id]?.copyLast()
    } else if (e.key === 'x' || e.key === 'd') {
      if (front) close(front.id)
    }
  }

  // Before the terminal sees the key: these never reach the shell.
  function onkeydown(e: KeyboardEvent) {
    // Not over an open dialog, nor in a field: there Ctrl-A selects the text.
    if (modal !== null) return
    const ctrl = e.ctrlKey || e.metaKey
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
    } else if (ctrl && e.key === 'w' && front) {
      mine()
      close(front.id)
    }
  }

  function add(id: number, title: string) {
    sessions.push({ id, title, ...fresh })
    active = id
  }

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
      add(await api.quickConnect(target), target)
      modal = null
    } catch (error) {
      throw await explain(error, i18n.lang)
    }
  }

  /** Closes a tab at once, as the TUI does; the one that takes its place comes to the front. */
  function close(id: number) {
    const at = sessions.findIndex((session) => session.id === id)
    if (at < 0) return
    sessions.splice(at, 1)
    delete terms[id]
    void api.sessionClose(id)
    if (active === id) active = (sessions[at] ?? sessions[at - 1])?.id ?? null
  }

  /** What was typed into a session's terminal. */
  function input(session: Session, data: string) {
    void api.sessionWrite(session.id, data)
  }

  function heard(session: Session, said: PtyState) {
    if (said.state === 'up') Object.assign(session, { state: 'up', met: true, error: null, why: null })
    else if (said.state === 'busy') session.busy = said.command
    else Object.assign(session, { state: 'closed', busy: null, error: said.error })
  }

  function reconnect(session: Session) {
    Object.assign(session, fresh)
    void terms[session.id]?.start()
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="grid h-full grid-cols-[16rem_1fr] grid-rows-[1fr_auto]">
  <Sidebar
    {hosts}
    {selected}
    bind:tag
    onselect={(alias) => (selected = alias)}
    {sessions}
    {active}
    onsession={(id) => (active = id)}
  />

  <main class="flex min-h-0 flex-col">
    {#if sessions.length}
      <Tabs {sessions} {active} onpick={(id) => (active = id)} onclose={close} />
    {/if}
    <!-- Behind a terminal the dashboard keeps its search, its order and its place. -->
    <div hidden={active !== null} class="flex min-h-0 flex-1 flex-col">
      <Dashboard
        {hosts}
        {tag}
        {selected}
        error={failed}
        onselect={(alias) => (selected = alias)}
        onconnect={open}
        onpalette={() => (modal = 'palette')}
      />
    </div>
    {#each sessions as session (session.id)}
      <Terminal
        bind:this={terms[session.id]}
        id={session.id}
        label={session.title}
        active={session.id === active}
        closed={session.state === 'closed'}
        why={session.why}
        oninput={(data) => input(session, data)}
        onstate={(said) => heard(session, said)}
        onreconnect={() => reconnect(session)}
      />
    {/each}
  </main>

  <Statusbar
    hosts={hosts.length}
    snippets={snippets.length}
    sessions={sessions.length}
    selected={hosts.find((host) => host.alias === selected)}
    terminal={active !== null}
    {leader}
    onpalette={() => (modal = 'palette')}
  />
</div>

{#if modal === 'palette'}
  <Palette
    {hosts}
    onhost={(alias) => {
      selected = alias
      modal = null
    }}
    onquick={() => (modal = 'quick')}
    onclose={() => (modal = null)}
  />
{:else if modal === 'quick'}
  <QuickConnect onconnect={connect} onclose={() => (modal = null)} />
{/if}
