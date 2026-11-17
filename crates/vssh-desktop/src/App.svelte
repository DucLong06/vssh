<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Host, type Shown, type Snippet } from './lib/api'
  import Dashboard from './lib/Dashboard.svelte'
  import { i18n, t } from './lib/i18n.svelte'
  import { live, poll, relabel } from './lib/live.svelte'
  import Palette from './lib/Palette.svelte'
  import QuickConnect from './lib/QuickConnect.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Statusbar from './lib/Statusbar.svelte'
  import { compact, parts } from './lib/stats'

  /** How often the hosts are asked for their figures, and every how many rounds pinged. */
  const EVERY = 5000
  const PING_ROUNDS = 12

  let hosts = $state<Host[]>([])
  let snippets = $state<Snippet[]>([])
  let sessions = $state<{ id: number; target: string }[]>([])
  let selected = $state<string | null>(null)
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

  function onkeydown(e: KeyboardEvent) {
    // Ctrl-K as desktop apps have it, Ctrl-A as the TUI's leader. Not over
    // an open dialog: in its fields Ctrl-A selects the text.
    if (modal === null && (e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'a')) {
      e.preventDefault()
      modal = 'palette'
    }
  }

  /** Opens a session to a host of the config. */
  async function open(alias: string) {
    try {
      sessions.push({ id: await api.connectHost(alias), target: alias })
    } catch (error) {
      await fail(error)
    }
  }

  /** Rejects with the refusal in words, for the form to show. */
  async function connect(target: string) {
    try {
      const id = await api.quickConnect(target)
      sessions.push({ id, target })
      modal = null
    } catch (error) {
      throw await explain(error, i18n.lang)
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="grid h-full grid-cols-[16rem_1fr] grid-rows-[1fr_auto]">
  <Sidebar {hosts} {selected} onselect={(alias) => (selected = alias)} />

  <main class="flex min-h-0 flex-col">
    {#if sessions.length}
      <!-- shortcut: a session is a chip with its host's figures; D3 makes it a terminal tab. -->
      <ul aria-label={t('sessions')} class="flex flex-wrap gap-2 border-b border-edge px-4 py-2">
        {#each sessions as session (session.id)}
          {@const peek = live[session.target]?.peek}
          <li title={t('session_note')} class="rounded-full border border-edge bg-black px-3 py-0.5">
            <span aria-hidden="true" class="text-green">●</span>
            <span class="text-white">#{session.id}</span>
            {session.target}
            {#if peek}
              <span class="text-white">{compact(parts(peek.stats, t('mem')))}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <Dashboard {hosts} error={failed} onconnect={open} onpalette={() => (modal = 'palette')} />
  </main>

  <Statusbar
    hosts={hosts.length}
    snippets={snippets.length}
    sessions={sessions.length}
    selected={hosts.find((host) => host.alias === selected)}
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
