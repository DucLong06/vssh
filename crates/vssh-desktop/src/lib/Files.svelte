<script module lang="ts">
  /** Numbers the transfers of every file view: an id is an event's name in the shell. */
  let serial = 0
</script>

<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount, tick } from 'svelte'
  import { api, explain, type Entry, type Shown } from './api'
  import FilePane from './FilePane.svelte'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'
  import { eta, human, join } from './paths'

  type Props = {
    /** The host of the ssh config whose files are on the right. */
    host: string
    /** Its tab is in front. */
    active: boolean
  }
  let { host, active }: Props = $props()

  /** How many transfers run at once: each is a connection of its own, and sshd lets only so many start together. */
  const AT_ONCE = 3

  type Side = 'local' | 'remote'
  const across = (side: Side): Side => (side === 'local' ? 'remote' : 'local')
  /** A failure as data, and in the words of the window's language once they are there. */
  type Failed = { error: unknown; why: Shown | null }
  type Pane = Failed & {
    /** In full; empty until the first answer. */
    path: string
    entries: Entry[]
    marked: string[]
    loading: boolean
  }
  const blank = (): Pane => ({ path: '', entries: [], marked: [], loading: true, error: null, why: null })
  const panes = $state<Record<Side, Pane>>({ local: blank(), remote: blank() })
  const views = $state<Partial<Record<Side, FilePane>>>({})
  let showHidden = $state(false)

  /** One file on its way: `up` to the host, else down from it. Both paths are in full. */
  type Copy = { up: boolean; name: string; local: string; remote: string }
  type Job = Copy & Failed & { id: string; done: number; total: number; bps: number; state: 'waiting' | 'running' | 'failed' }
  let transfers = $state<Job[]>([])
  let running = 0
  /** Set when the view goes: what still runs is stopped and reports to nobody. */
  let gone = false

  /** Something to say that is no failure. */
  let note = $state('')
  /** A change that was refused. */
  let refused = $state<Failed | null>(null)
  let menu = $state<{ x: number; y: number; side: Side; entry: Entry | null } | null>(null)
  let menuEl = $state<HTMLElement>()
  type Ask =
    | { kind: 'mkdir'; side: Side }
    | { kind: 'rename' | 'rm'; side: Side; entry: Entry }
    | { kind: 'overwrite'; names: string; go: () => void }
  let ask = $state<Ask | null>(null)
  let typed = $state('')
  let field = $state<HTMLInputElement>()
  /** The rows being dragged, and the pane they left. */
  let dragging: { side: Side; names: string[] } | null = null

  /** The same questions, to this machine and to the host. */
  const fs = {
    local: { list: api.localList, mkdir: api.localMkdir, rm: api.localRm, rename: api.localRename },
    remote: {
      async list(path: string) {
        // Where the host starts is asked once: from then on every path is in full.
        const full = path || (await api.sftpRealpath(host, ''))
        return { path: full, entries: await api.sftpList(host, full) }
      },
      mkdir: (path: string) => api.sftpMkdir(host, path),
      rm: (path: string, isDir: boolean) => api.sftpRm(host, path, isDir),
      rename: (from: string, to: string) => api.sftpRename(host, from, to),
    },
  }

  /** The last question to each side: an answer to an earlier one is dropped. */
  const asked = { local: 0, remote: 0 }

  /** Lists a directory; one that cannot be listed leaves the pane where it was, and says why. */
  async function load(side: Side, path = panes[side].path) {
    const pane = panes[side]
    const mine = ++asked[side]
    pane.loading = true
    try {
      const got = await fs[side].list(path)
      if (mine !== asked[side]) return
      const still = (name: string) => got.entries.some((entry) => entry.name === name)
      const marked = got.path === pane.path ? pane.marked.filter(still) : []
      Object.assign(pane, got, { marked, error: null, why: null })
    } catch (error) {
      if (mine === asked[side]) pane.error = error
    } finally {
      if (mine === asked[side]) pane.loading = false
    }
  }

  const has = (side: Side, name: string) => panes[side].entries.some((entry) => entry.name === name)

  /** Starts these copies, after a yes for those that would replace a file. */
  function send(copies: Copy[]) {
    // Without both paths there is no saying where a file would land.
    if (!panes.local.path || !panes.remote.path || !copies.length) return
    const go = () => {
      for (const copy of copies) {
        transfers.push({ ...copy, id: `f${++serial}`, done: 0, total: 0, bps: 0, state: 'waiting', error: null, why: null })
      }
      panes.local.marked = []
      panes.remote.marked = []
      pump()
    }
    // Never overwrite silently, as the TUI.
    const there = copies.filter((copy) => has(copy.up ? 'remote' : 'local', copy.name))
    if (there.length) ask = { kind: 'overwrite', names: there.map((copy) => copy.name).join(', '), go }
    else go()
  }

  /** Copies entries of a pane, by name, into the directory the other pane shows. Only files: the core copies no directories. */
  function copy(...from: [side: Side, names: string[]][]) {
    const picked = from.flatMap(([side, names]) =>
      panes[side].entries.filter((entry) => names.includes(entry.name)).map((entry) => ({ side, entry })),
    )
    const files = picked.filter(({ entry }) => !entry.is_dir)
    note = files.length < picked.length ? t('files_only') : ''
    send(
      files.map(({ side, entry: { name } }) => ({
        up: side === 'local',
        name,
        local: join(panes.local.path, name),
        remote: join(panes.remote.path, name),
      })),
    )
  }

  /** What is marked on a side, else the entry given. */
  const chosen = (side: Side, entry: Entry | null | undefined): [Side, string[]] => [
    side,
    panes[side].marked.length ? panes[side].marked : entry ? [entry.name] : [],
  ]

  /** `c`: everything marked, each to its other side; nothing marked, the selected entry. */
  function copyKey(side: Side, entry: Entry | null) {
    if (panes.local.marked.length + panes.remote.marked.length) copy(['local', panes.local.marked], ['remote', panes.remote.marked])
    else copy(chosen(side, entry))
  }

  const drop = (job: Job) => (transfers = transfers.filter((other) => other.id !== job.id))

  function pump() {
    for (const job of transfers) {
      if (running >= AT_ONCE) return
      if (job.state === 'waiting') void run(job)
    }
  }

  async function run(job: Job) {
    job.state = 'running'
    running++
    const started = Date.now()
    // Before the transfer is asked for: its first steps are not missed.
    const unlisten = await listen<{ done: number; total: number }>(`sftp-progress:${job.id}`, ({ payload }) => {
      const secs = (Date.now() - started) / 1000
      Object.assign(job, payload, { bps: secs > 0 ? payload.done / secs : 0 })
    })
    try {
      // Cancelled before it began.
      if (!transfers.includes(job)) return
      if (job.up) await api.sftpUpload(host, job.local, job.remote, job.id)
      else await api.sftpDownload(host, job.remote, job.local, job.id)
      drop(job)
    } catch (error) {
      // A cancelled transfer is taken off the list by then, and is no failure.
      if (transfers.includes(job)) Object.assign(job, { state: 'failed', error })
    } finally {
      unlisten()
      running--
      if (!gone) {
        // Whole or in part, something is where it went.
        void load(job.up ? 'remote' : 'local')
        pump()
      }
    }
  }

  /** Stops a transfer that runs, and takes its line off the list, as it does for one that waits or has failed. */
  function cancel(job: Job) {
    if (job.state === 'running') void api.sftpCancel(job.id)
    drop(job)
  }

  const name = $derived(typed.trim())
  /** The name typed is one that is there already. */
  const taken = $derived(
    ask !== null && (ask.kind === 'mkdir' || ask.kind === 'rename') && has(ask.side, name) && name !== (ask.kind === 'rename' ? ask.entry.name : ''),
  )
  const verb = $derived(
    ask && { mkdir: t('new_folder'), rename: t('rename'), rm: t('delete'), overwrite: t('overwrite') }[ask.kind],
  )
  /** Nothing to do with what is typed: no name, a path, or a name that is there. */
  const idle = $derived(
    ask !== null && (ask.kind === 'mkdir' || ask.kind === 'rename') && (!name || name.includes('/') || has(ask.side, name)),
  )

  async function open(next: Ask) {
    typed = next.kind === 'rename' ? next.entry.name : ''
    ask = next
    // Once the dialog is open: before that nothing in it takes the focus.
    await tick()
    field?.focus()
  }

  /** The yes to the question that is open. */
  async function answer() {
    const q = ask
    if (!q || idle) return
    ask = null
    if (q.kind === 'overwrite') return q.go()
    const dir = panes[q.side].path
    refused = null
    try {
      if (q.kind === 'mkdir') await fs[q.side].mkdir(join(dir, name))
      else if (q.kind === 'rename') await fs[q.side].rename(join(dir, q.entry.name), join(dir, name))
      else await fs[q.side].rm(join(dir, q.entry.name), q.entry.is_dir)
    } catch (error) {
      refused = { error, why: null }
    }
    await load(q.side)
  }

  onMount(() => {
    void load('local')
    void load('remote')
    // Files from the desktop's file manager: the shell knows their paths, the page does not.
    const dropped = listen<{ paths?: string[] }>('tauri://drag-drop', ({ payload }) => {
      if (!active || !payload.paths?.length) return
      note = ''
      send(
        payload.paths.map((local) => {
          const name = local.split('/').pop()!
          return { up: true, name, local, remote: join(panes.remote.path, name) }
        }),
      )
    })
    return () => {
      gone = true
      void dropped.then((unlisten) => unlisten())
      // Closing is at once, as for a terminal: nothing goes on behind a tab that is no more.
      for (const job of transfers) if (job.state === 'running') void api.sftpCancel(job.id)
    }
  })

  // A failure is kept as data: the words for it follow the language.
  $effect(() => {
    const lang = i18n.lang
    for (const item of [panes.local, panes.remote, refused, ...transfers]) {
      const error = item?.error
      if (!item || !error) continue
      void explain(error, lang).then((why) => {
        if (item.error === error) item.why = why
      })
    }
  })

  // In front, the keys are the local pane's first.
  $effect(() => {
    if (active) views.local?.focus()
  })

  // The menu takes the keys while it is open.
  $effect(() => menuEl?.querySelector('button')?.focus())

  function menukey(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key]
    if (!step || !menuEl) return
    e.preventDefault()
    const all = [...menuEl.querySelectorAll('button')]
    all[(all.indexOf(document.activeElement as HTMLButtonElement) + step + all.length) % all.length].focus()
  }

  /** Does what a menu item stands for, for the entry the menu was opened on. */
  function pick(act: (side: Side, entry: Entry) => void) {
    const { side, entry } = menu!
    views[side]?.focus()
    act(side, entry!)
  }

  const button = 'cursor-pointer rounded-md border border-edge px-3 py-1.5 hover:bg-selection'
  const arrow = 'cursor-pointer rounded-md border border-edge bg-panel px-2 py-1 text-magenta hover:bg-selection'
  const item = 'w-full cursor-pointer rounded-lg px-3 py-1 text-left hover:bg-selection focus-visible:bg-selection'
</script>

<svelte:window
  onclick={() => (menu = null)}
  onkeydown={(e) => {
    if (e.key !== 'Escape' || !menu) return
    views[menu.side]?.focus()
    menu = null
  }}
/>

{#snippet pane(side: Side)}
  <FilePane
    bind:this={views[side]}
    {side}
    label={side === 'local' ? t('local') : host}
    path={panes[side].path}
    entries={panes[side].entries}
    error={panes[side].error ? panes[side].why : null}
    loading={panes[side].loading}
    bind:showHidden
    bind:marked={panes[side].marked}
    oncd={(path) => load(side, path)}
    oncopy={(entry) => copyKey(side, entry)}
    onmenu={(at, entry) => (menu = { ...at, side, entry })}
    onretry={() => load(side)}
    ondragnames={(names) => (dragging = { side, names })}
    ondropped={() => {
      if (dragging && dragging.side !== side) copy([dragging.side, dragging.names])
      dragging = null
    }}
  />
{/snippet}

<div role="tabpanel" aria-label="{t('files')} {host}" hidden={!active} class="min-h-0 flex-1 bg-bg p-3">
  <div class="flex h-full flex-col gap-2">
    <div class="flex min-h-0 flex-1 gap-2">
      {@render pane('local')}
      <div class="flex flex-col justify-center gap-2">
        <button
          type="button"
          aria-label={t('upload')}
          title={t('upload')}
          onclick={() => copy(chosen('local', views.local?.current()))}
          class={arrow}
        >
          →
        </button>
        <button
          type="button"
          aria-label={t('download')}
          title={t('download')}
          onclick={() => copy(chosen('remote', views.remote?.current()))}
          class={arrow}
        >
          ←
        </button>
      </div>
      {@render pane('remote')}
    </div>

    {#if note}
      <p role="status" class="px-1 text-yellow">{note}</p>
    {/if}
    {#if refused?.why}
      <div class="flex items-center gap-2 px-1">
        <p role="alert" title={refused.why.chain} class="min-w-0 truncate">
          <strong class="text-red">✗ {refused.why.label}</strong>
          <span class="text-white">{refused.why.message}</span>
        </p>
        <button
          type="button"
          aria-label={t('close')}
          onclick={() => (refused = null)}
          class="cursor-pointer rounded px-1.5 text-dim hover:bg-selection hover:text-fg"
        >
          ×
        </button>
      </div>
    {/if}

    {#if transfers.length}
      <!-- One line per transfer, the newest at the bottom; many of them scroll. -->
      <ul aria-label={t('transfers')} class="grid max-h-40 shrink-0 gap-1 overflow-y-auto">
        {#each transfers as job (job.id)}
          {@const pct = job.total ? Math.floor((job.done * 100) / job.total) : 0}
          <li class="flex items-center gap-3 rounded-xl border border-edge bg-panel px-3 py-1">
            <span role="img" aria-label={job.up ? t('upload') : t('download')} class="text-cyan">
              {job.up ? '↑' : '↓'}
            </span>
            <span class="w-48 shrink-0 truncate" title={job.up ? job.remote : job.local}>{job.name}</span>
            {#if job.state === 'failed'}
              <p role="alert" title={job.why?.chain} class="min-w-0 flex-1 truncate">
                <strong class="text-red">✗ {job.why?.label}</strong>
                <span class="text-white">{job.why?.message}</span>
              </p>
              <button
                type="button"
                aria-label="{t('close')} {job.name}"
                onclick={() => cancel(job)}
                class="cursor-pointer rounded px-1.5 text-dim hover:bg-selection hover:text-fg"
              >
                ×
              </button>
            {:else}
              <div
                role="progressbar"
                aria-label={job.name}
                aria-valuenow={pct}
                aria-valuemin={0}
                aria-valuemax={100}
                class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-bar"
              >
                <div class="h-full rounded-full bg-blue" style="width: {pct}%"></div>
              </div>
              <span class="w-10 shrink-0 text-right">{pct}%</span>
              <span class="w-20 shrink-0 text-right text-white">
                {job.state === 'waiting' ? t('waiting') : `${human(Math.round(job.bps))}/s`}
              </span>
              <span class="w-14 shrink-0 text-right text-white">
                {job.bps ? eta((job.total - job.done) / job.bps) : '—'}
              </span>
              <button
                type="button"
                aria-label="{t('cancel')} {job.name}"
                onclick={() => cancel(job)}
                class="shrink-0 cursor-pointer rounded-md border border-edge px-2 text-red hover:bg-selection"
              >
                {t('cancel')}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

{#if menu}
  {@const entry = menu.entry}
  <!-- At the pointer, and inside the window. -->
  <ul
    bind:this={menuEl}
    role="menu"
    aria-label={entry?.name ?? t('files')}
    onkeydown={menukey}
    style="left: {Math.min(menu.x, innerWidth - 176)}px; top: {Math.min(menu.y, innerHeight - 152)}px"
    class="fixed z-30 min-w-40 rounded-xl border border-edge bg-black p-1 shadow-2xl"
  >
    {#if entry && !entry.is_dir}
      <li role="none">
        <button type="button" role="menuitem" onclick={() => pick((side, entry) => copy([side, [entry.name]]))} class={item}>
          {t('copy')}
        </button>
      </li>
    {/if}
    <li role="none">
      <button type="button" role="menuitem" onclick={() => pick((side) => void open({ kind: 'mkdir', side }))} class={item}>
        {t('new_folder')}
      </button>
    </li>
    {#if entry}
      <li role="none">
        <button
          type="button"
          role="menuitem"
          onclick={() => pick((side, entry) => void open({ kind: 'rename', side, entry }))}
          class={item}
        >
          {t('rename')}
        </button>
      </li>
      <li role="none">
        <button
          type="button"
          role="menuitem"
          onclick={() => pick((side, entry) => void open({ kind: 'rm', side, entry }))}
          class="{item} text-red"
        >
          {t('delete')}
        </button>
      </li>
    {/if}
  </ul>
{/if}

{#if ask}
  {@const q = ask}
  <Modal label={verb ?? ''} onclose={() => (ask = null)}>
    <form
      class="grid gap-3 p-4"
      onsubmit={(e) => {
        e.preventDefault()
        void answer()
      }}
    >
      {#if q.kind === 'mkdir' || q.kind === 'rename'}
        <label class="text-white">
          {t('sort_name')}
          <input
            bind:this={field}
            bind:value={typed}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            class="mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg"
          />
        </label>
        {#if taken}
          <p class="text-yellow">{t('exists').replace('{name}', name)}</p>
        {/if}
      {:else if q.kind === 'overwrite'}
        <p class="break-words">{t('overwrite_ask').replace('{name}', q.names)}</p>
      {:else}
        <p>{t('delete_ask').replace('{name}', q.entry.name)}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button type="button" onclick={() => (ask = null)} class="{button} text-white">{t('cancel')}</button>
        <button
          type="submit"
          disabled={idle}
          class="{button} disabled:opacity-60 {q.kind === 'rm' ? 'border-red text-red' : 'border-blue bg-selection text-blue'}"
        >
          {verb}
        </button>
      </div>
    </form>
  </Modal>
{/if}
