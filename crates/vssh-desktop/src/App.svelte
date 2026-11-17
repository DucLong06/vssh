<script lang="ts">
  import { onMount, tick as rendered } from 'svelte'
  import {
    api,
    explain,
    type ConnError,
    type Host,
    type PtyState,
    type Shown,
    type Snippet,
    type VaultState,
  } from './lib/api'
  import Dashboard from './lib/Dashboard.svelte'
  import Failure from './lib/Failure.svelte'
  import Files from './lib/Files.svelte'
  import Forwards from './lib/Forwards.svelte'
  import { i18n, t } from './lib/i18n.svelte'
  import { poll, relabel } from './lib/live.svelte'
  import Palette from './lib/Palette.svelte'
  import QuickConnect from './lib/QuickConnect.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Batch from './lib/Batch.svelte'
  import Snippets from './lib/Snippets.svelte'
  import Statusbar from './lib/Statusbar.svelte'
  import Tabs, { type Tab } from './lib/Tabs.svelte'
  import Terminal from './lib/Terminal.svelte'
  import Vault from './lib/Vault.svelte'

  /** How often the hosts are asked for their figures, and every how many rounds pinged. */
  const EVERY = 5000
  const PING_ROUNDS = 12

  let hosts = $state<Host[]>([])
  let snippets = $state<Snippet[]>([])
  /** A tab's session. `met`: its shell came up since it was last started. */
  type Session = Tab & { met: boolean; error: ConnError | null; why: Shown | null }
  const fresh = { state: 'connecting', busy: null, met: false, error: null, why: null } as const
  let sessions = $state<Session[]>([])
  /** The session whose failure to connect is in a dialog. */
  let failure = $state<number | null>(null)
  const failed_session = $derived(sessions.find((session) => session.id === failure))
  /** The file views, one per host. Their ids count down from -1: a session's is never below zero. */
  let files = $state<{ id: number; host: string }[]>([])
  /** The session or the file view whose tab is in front; `null` = the dashboard. */
  let active = $state<number | null>(null)
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
  let modal = $state<'palette' | 'quick' | 'snippets' | 'batch' | 'forwards' | 'vault' | null>(null)
  /** Unlocked, the vault stays so until the app is closed: the shell holds it, and nothing of it is written down. */
  let vault = $state<VaultState>('none')
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
    // A vault that is there is asked to be opened first thing; it may be left shut.
    api.vaultState().then((now) => {
      vault = now
      if (now === 'locked') modal ??= 'vault'
    }, fail)
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

  /** The tabs in order: the dashboard, the sessions, the file views. */
  const tabs = $derived<(number | null)[]>([null, ...[...sessions, ...files].map((tab) => tab.id)])
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
      if (active !== null) close(active)
    } else if (e.key === 'b') {
      if (front) front.broadcast = !front.broadcast
    } else if (e.key === 'B') {
      // Every tab at once, as the TUI's broadcast; off again if all are on.
      const on = sessions.some((session) => !session.broadcast)
      for (const session of sessions) session.broadcast = on
    }
  }

  // Before the terminal sees the key: these never reach the shell.
  function onkeydown(e: KeyboardEvent) {
    // Not over an open dialog, nor in a field: there Ctrl-A selects the text.
    if (modal !== null || failure !== null) return
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
    } else if (ctrl && e.key === 'w' && active !== null) {
      mine()
      close(active)
    }
  }

  function add(id: number, title: string) {
    sessions.push({ id, title, broadcast: false, ...fresh })
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
    if (id < 0) {
      files = files.filter((view) => view.id !== id)
    } else {
      sessions = sessions.filter((session) => session.id !== id)
      delete terms[id]
      void api.sessionClose(id)
    }
    // `tabs` is the tabs that are left now.
    if (active === id) active = tabs[at] ?? tabs[at - 1] ?? null
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

<div class="grid h-full grid-cols-[16rem_1fr] grid-rows-[1fr_auto]">
  <Sidebar
    {hosts}
    {selected}
    bind:tag
    onselect={(alias) => (selected = alias)}
    {sessions}
    {active}
    onsession={(id) => (active = id)}
    {vault}
    onvault={() => (modal = 'vault')}
  />

  <!-- min-w-0: what is in it (a long path, a wide terminal) scrolls in its place and does not widen the window's page. -->
  <main class="flex min-h-0 min-w-0 flex-col">
    {#if sessions.length || files.length}
      <Tabs {sessions} {files} {active} onpick={(id) => (active = id)} onclose={close} />
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
      bind:marked
    />
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
    {#each files as view (view.id)}
      <Files host={view.host} active={view.id === active} />
    {/each}
  </main>

  <Statusbar
    hosts={hosts.length}
    snippets={snippets.length}
    sessions={sessions.length}
    selected={hosts.find((host) => host.alias === selected)}
    terminal={front !== undefined}
    files={active !== null && !front}
    broadcasting={sessions.filter((session) => session.broadcast).length}
    {leader}
    onsnippets={() => (modal = 'snippets')}
    onpalette={() => (modal = 'palette')}
  />
</div>

{#if failed_session?.error && failed_session.why}
  {@const session = failed_session}
  <Failure
    title={session.title}
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
    onvault={() => (modal = 'vault')}
    onclose={() => (modal = null)}
  />
{:else if modal === 'vault'}
  <Vault bind:vault hosts={hosts.length} onclose={() => (modal = null)} />
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
