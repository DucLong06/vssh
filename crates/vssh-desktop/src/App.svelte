<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Host, type Shown, type Snippet } from './lib/api'
  import { i18n, t } from './lib/i18n.svelte'
  import Palette from './lib/Palette.svelte'
  import QuickConnect from './lib/QuickConnect.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Statusbar from './lib/Statusbar.svelte'

  let hosts = $state<Host[]>([])
  let snippets = $state<Snippet[]>([])
  let sessions = $state<{ id: number; target: string }[]>([])
  let selected = $state<string | null>(null)
  let modal = $state<'palette' | 'quick' | null>(null)
  /** Why the hosts could not be read. */
  let failed = $state<Shown | null>(null)

  onMount(async () => {
    try {
      ;[hosts, snippets] = await Promise.all([api.listHosts(), api.listSnippets()])
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  })

  function onkeydown(e: KeyboardEvent) {
    // Ctrl-K as desktop apps have it, Ctrl-A as the TUI's leader. Not over
    // an open dialog: in its fields Ctrl-A selects the text.
    if (modal === null && (e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'a')) {
      e.preventDefault()
      modal = 'palette'
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
  <Sidebar {hosts} {selected} error={failed} onselect={(alias) => (selected = alias)} />

  <main class="min-h-0 overflow-y-auto">
    {#if sessions.length === 0}
      <div class="grid h-full place-content-center justify-items-center gap-2 text-center">
        <p aria-hidden="true" class="text-4xl text-edge">&gt;_</p>
        <h1 class="text-base">{t('no_sessions')}</h1>
        <p class="text-white">{t('no_sessions_hint')}</p>
        <button
          type="button"
          onclick={() => (modal = 'palette')}
          aria-label={t('palette')}
          class="mt-2 cursor-pointer rounded-md border border-edge px-3 py-1 text-magenta hover:bg-selection"
        >
          Ctrl-K
        </button>
      </div>
    {:else}
      <ul aria-label={t('sessions')} class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3 p-4">
        {#each sessions as session (session.id)}
          <li class="rounded-xl border border-edge bg-black p-3">
            <p class="truncate">
              <span aria-hidden="true" class="text-green">●</span>
              <span class="text-white">#{session.id}</span>
              {session.target}
            </p>
            <p class="pt-1 text-white">{t('session_note')}</p>
          </li>
        {/each}
      </ul>
    {/if}
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
