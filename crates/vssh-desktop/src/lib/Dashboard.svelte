<script lang="ts">
  import { tick } from 'svelte'
  import type { Host, Shown } from './api'
  import HostCard from './HostCard.svelte'
  import { address, chip, groupOf, hasTag, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import { ask, critical, down, live, sick, trail, up } from './live.svelte'
  import QuickPeek from './QuickPeek.svelte'
  import Sparkline from './Sparkline.svelte'

  type Props = {
    hosts: Host[]
    /** Why there are no hosts to show, if they could not be read. */
    error?: Shown | null
    /** Only hosts with this tag or group, as chosen in the sidebar. */
    tag?: string | null
    selected?: string | null
    /** A terminal tab is in front of it. */
    hidden?: boolean
    onselect?: (alias: string) => void
    onconnect: (alias: string) => void
    /** Opens a host's files. */
    onfiles?: (alias: string) => void
    /** Opens a host's port forwards. */
    onforwards?: (alias: string) => void
    /** `s`: the snippets. */
    onsnippets?: () => void
    /** `x`: a command for the marked hosts. */
    onbatch?: () => void
    onpalette: () => void
    /** `a`: the form for a new host; `e` and `d` on a card: its form, and the question before it is deleted. */
    onadd?: () => void
    onedit?: (alias: string) => void
    ondelete?: (alias: string) => void
    /** The cards chosen to act on together, as Space marks hosts in the TUI. */
    marked?: string[]
  }
  let {
    hosts,
    error = null,
    tag = null,
    selected = null,
    hidden = false,
    onselect,
    onconnect,
    onfiles,
    onforwards,
    onsnippets,
    onbatch,
    onpalette,
    onadd,
    onedit,
    ondelete,
    marked = $bindable([]),
  }: Props = $props()

  /** How long the pointer rests on a card before its quick peek opens, and how long an answer is good for. */
  const REST = 600
  const FRESH = 30_000

  type Sort = 'order' | 'name' | 'latency' | 'cpu'
  let sort = $state<Sort>('order')
  let grouped = $state(true)
  let query = $state('')
  /** The user's own order: aliases, as they were last dragged. */
  let order = $state<string[]>(saved())
  /** The card being dragged. */
  let dragging: string | null = null
  /** The card whose quick peek is open, and whether the pointer opened it. */
  let peeking = $state<{ alias: string; hover: boolean } | null>(null)
  let resting: ReturnType<typeof setTimeout> | undefined
  let search = $state<HTMLInputElement>()
  let grid = $state<HTMLElement>()

  function saved(): string[] {
    try {
      const list: unknown = JSON.parse(localStorage.getItem('vssh.order') ?? '[]')
      return Array.isArray(list) ? list.map(String) : []
    } catch {
      return []
    }
  }

  /** What a host is sorted by; lower comes first, and what is not known comes last. */
  const rank: Record<Sort, (host: Host, at: number) => number | string> = {
    // Hosts the order has not met come after it, as the config has them.
    order: (host, at) => (order.indexOf(host.alias) + 1 || order.length + 1 + at) - 1,
    name: (host) => host.alias,
    latency: (host) => live[host.alias]?.ms ?? Infinity,
    cpu: (host) => -(live[host.alias]?.peek?.stats.cpu ?? -1),
  }

  /** Every host, in the order asked for. */
  const sorted = $derived.by(() => {
    const keyed = hosts.map((host, at) => ({ host, key: rank[sort](host, at) }))
    const byKey = (a: (typeof keyed)[0], b: (typeof keyed)[0]) =>
      typeof a.key === 'string' ? a.key.localeCompare(b.key as string) : a.key - (b.key as number)
    return keyed.sort(byKey).map((row) => row.host)
  })

  /** The cards on screen: filtered, and whoever needs looking at first. */
  const shown = $derived.by(() => {
    const words = query.trim().toLowerCase()
    const found = (host: Host) =>
      // As `/#tag` in the TUI: the start of a tag or a group.
      words.startsWith('#')
        ? tagsOf(host).some((name) => name.toLowerCase().startsWith(words.slice(1)))
        : [host.alias, address(host)].some((field) => field.toLowerCase().includes(words))
    const kept = sorted.filter((host) => (tag === null || hasTag(host, tag)) && found(host))
    // A stable sort: within the sick and within the well, the order asked for.
    return kept.sort((a, b) => Number(sick(live[b.alias])) - Number(sick(live[a.alias])))
  })

  /** The cards by group, each group where its first card is; one nameless group when flat. */
  const sections = $derived.by(() => {
    if (!grouped) return shown.length ? [{ name: '', list: shown }] : []
    const groups = new Map<string, Host[]>()
    for (const host of shown) groups.set(groupOf(host), [...(groups.get(groupOf(host)) ?? []), host])
    return [...groups].map(([name, list]) => ({ name, list }))
  })

  const card = (alias: string) =>
    [...(grid?.querySelectorAll('article') ?? [])].find((el) => el.getAttribute('aria-label') === alias)

  /** For a host chosen again while it is the selected one: nothing changed for the effect below to hear of. */
  export const focus = (alias: string) => card(alias)?.focus()

  /** The search, for whoever asks for the hosts from elsewhere (the rail). */
  export const seek = () => search?.focus()

  // A host chosen elsewhere (the palette, the sidebar) is brought into view.
  $effect(() => {
    // Also when the dashboard comes back to the front.
    if (selected && !hidden) card(selected)?.focus()
  })

  /** Puts a card where another one is, in what is on screen now, and keeps that as the user's order. */
  function place(alias: string, onto: string) {
    const names = sorted.map((host) => host.alias)
    const [from, to] = [names.indexOf(alias), names.indexOf(onto)]
    if (from < 0 || to < 0 || from === to) return
    // Up the list it lands before the other card, down the list after it.
    names.splice(to, 0, ...names.splice(from, 1))
    order = names
    sort = 'order'
    localStorage.setItem('vssh.order', JSON.stringify(names))
  }

  /** Opens a quick peek as a person asking for it: the host is asked, unless its answer is fresh. */
  function peek(alias: string) {
    peeking = { alias, hover: false }
    const host = live[alias]
    const fresh = host?.at !== undefined && (host.peek || host.error) && Date.now() - host.at < FRESH
    if (!fresh) void ask(alias, false)
  }

  /** Connects to every marked host, in the order on screen; to this one if none is marked. */
  function connect(alias?: string) {
    const all = shown.map((host) => host.alias).filter((name) => marked.includes(name))
    marked = []
    for (const name of all.length || !alias ? all : [alias]) onconnect(name)
  }

  async function onkeydown(e: KeyboardEvent, alias: string) {
    // Not from the card's own button: there Enter is a click already.
    if (e.target !== e.currentTarget) return
    const step = e.altKey && { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[e.key]
    if (e.key === 'Enter') {
      connect(alias)
    } else if (e.key === ' ') {
      // Not a scroll of the page.
      e.preventDefault()
      marked = marked.includes(alias) ? marked.filter((name) => name !== alias) : [...marked, alias]
    } else if (e.key === 'v') {
      peek(alias)
    } else if (e.key === 'f') {
      onfiles?.(alias)
    } else if (e.key === 'L') {
      // Stopped here: the dialog has the focus before the key is done, and the key is not for it.
      e.preventDefault()
      onforwards?.(alias)
    } else if (e.key === 'e') {
      // As L: the letter is not for the form that opens.
      e.preventDefault()
      onedit?.(alias)
    } else if (e.key === 'd' || e.key === 'Delete') {
      e.preventDefault()
      ondelete?.(alias)
    } else if (step) {
      // Drag and drop for the keyboard: one place along, among the cards on screen.
      e.preventDefault()
      const next = shown[shown.findIndex((host) => host.alias === alias) + step]
      if (!next) return
      place(alias, next.alias)
      // A card that moved in the document lost the focus on the way.
      await tick()
      card(alias)?.focus()
    }
  }

  function onwindowkey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement
    // The TUI's keys. Not while typing, with Ctrl (Ctrl-R), or under a dialog.
    // Nor behind a terminal tab.
    const away = hidden || document.querySelector('dialog[open]')
    if (typing || e.ctrlKey || e.metaKey || e.altKey || away) return
    if (e.key === '/') {
      e.preventDefault()
      search?.focus()
    } else if (e.key === 's') {
      e.preventDefault()
      onsnippets?.()
    } else if (e.key === 'x') {
      // Or the command line that opens would start with an x.
      e.preventDefault()
      onbatch?.()
    } else if (e.key === 'a') {
      e.preventDefault()
      onadd?.()
    } else if (peeking && e.key === 'Escape') {
      peeking = null
    } else if (peeking && e.key === 'r') {
      // A key is a person asking, even over a popover the pointer opened.
      void ask(peeking.alias, false)
    }
  }

  const sorts = $derived<[Sort, string][]>([
    ['order', t('sort_order')],
    ['name', t('sort_name')],
    ['latency', t('sort_latency')],
    ['cpu', 'CPU'],
  ])

  /** The figures over the cards: how many answer, who does not, and all the hosts' CPU and memory as one. */
  const fleet = $derived.by(() => {
    const state = (host: Host): keyof typeof segment => {
      const info = live[host.alias]
      return down(info) ? 'down' : critical(info) ? 'crit' : up(info) ? 'ok' : 'unknown'
    }
    const states = hosts.map((host) => ({ alias: host.alias, state: state(host) }))
    /** Round by round from the oldest, the mean of the hosts that have that round. */
    const mean = (key: 'cpu' | 'mem') => {
      const trails = hosts.map((host) => trail[host.alias]?.[key] ?? []).filter((list) => list.length)
      const rounds = Math.max(0, ...trails.map((list) => list.length))
      return Array.from({ length: rounds }, (_, at) => {
        const round = trails.flatMap((list) => list.at(at - rounds) ?? [])
        return Math.round(round.reduce((sum, pct) => sum + pct, 0) / round.length)
      })
    }
    return {
      states,
      met: states.filter((host) => host.state === 'ok' || host.state === 'crit').length,
      lost: states.filter((host) => host.state === 'down').map((host) => host.alias),
      cpu: mean('cpu'),
      mem: mean('mem'),
    }
  })

  const control =
    'h-10 cursor-pointer rounded-xl border border-edge px-3.5 font-medium whitespace-nowrap text-fg hover:border-dim/50 hover:bg-selection'
  const tile = 'min-w-0 rounded-2xl bg-panel px-4 pt-3 shadow-card'
  const figure = 'text-2xl leading-tight font-semibold tracking-tight text-ink'
  const segment = { ok: 'bg-green', crit: 'bg-red', down: 'inset-ring-[1.5px] inset-ring-red', unknown: 'bg-bar' }
</script>

<svelte:window onkeydown={onwindowkey} />

<section {hidden} aria-label={t('hosts')} class="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
  {#if error}
    <p role="alert" class="mt-5 rounded-xl border border-dashed border-red/40 bg-bg px-4 py-3 break-words text-red">
      <strong>{error.label}</strong>
      {error.message}
    </p>
  {/if}
  {#if hosts.length === 0}
    {#if !error}
      <div class="grid h-full place-content-center justify-items-center gap-3 text-center">
        <p
          aria-hidden="true"
          class="grid size-14 place-items-center rounded-2xl bg-linear-to-br from-blue to-magenta font-mono text-xl font-bold text-page shadow-lg shadow-blue/30"
        >
          &gt;_
        </p>
        <h2 class="text-lg font-semibold text-ink">{t('no_hosts')}</h2>
        <p class="text-white">{t('no_hosts_hint')}</p>
        <div class="mt-2 flex gap-2">
          <button type="button" onclick={onpalette} aria-label={t('palette')} class="{control} font-mono text-[13px]">
            Ctrl-K
          </button>
          {#if onadd}
            <button type="button" onclick={onadd} class="{control} border-blue bg-blue font-semibold text-page hover:bg-blue/90">
              {t('add_host')}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  {:else}
    <!-- The search, and the controls as one group: where they have no room
         beside it (Vietnamese, with cards marked) they all go to a second
         row, from the same margin, and the cards start below it. -->
    <div class="sticky top-0 z-20 -mx-8 flex flex-wrap items-center gap-2.5 border-b border-edge bg-bg px-8 pb-4">
      <input
        bind:this={search}
        bind:value={query}
        type="search"
        aria-label={t('search_hosts')}
        aria-keyshortcuts="/"
        placeholder="/  {t('search_hosts')}"
        onkeydown={(e) => {
          if (e.key !== 'Escape') return
          query = ''
          search?.blur()
        }}
        class="h-10 min-w-40 flex-1 rounded-full border border-edge bg-panel px-4 text-fg placeholder:text-dim focus:border-blue"
      />
      <div class="flex flex-wrap items-center gap-2.5">
        <button
          type="button"
          aria-pressed={grouped}
          onclick={() => (grouped = !grouped)}
          class="{control} aria-pressed:border-dim/50 aria-pressed:bg-selection aria-pressed:text-ink"
        >
          {t('group_by_tag')}
        </button>
        {#if marked.length}
          <button
            type="button"
            onclick={() => connect()}
            class="{control} border-magenta/70 text-magenta"
          >
            {t('connect')} {marked.length}
          </button>
          <button
            type="button"
            aria-keyshortcuts="x"
            onclick={() => onbatch?.()}
            class="{control} border-magenta/70 text-magenta"
          >
            {t('run')} {marked.length}
          </button>
        {/if}
        {#if onadd}
          <!-- The concept's one filled button. -->
          <button
            type="button"
            aria-keyshortcuts="a"
            onclick={onadd}
            class="{control} border-blue bg-blue font-semibold text-page shadow-lg shadow-blue/20 hover:bg-blue/90"
          >
            {t('add_host')}
          </button>
        {/if}
        <!-- Radio buttons, drawn as one control: a native select opens the
             toolkit's own light menu over the dark window. -->
        <div role="radiogroup" aria-label={t('sort')} class="flex h-10 items-center rounded-xl border border-edge bg-page p-1">
          {#each sorts as [value, label] (value)}
            <label
              class="cursor-pointer rounded-lg px-2.5 py-1 text-[13px] whitespace-nowrap text-dim hover:text-fg has-checked:bg-selection has-checked:font-medium has-checked:text-ink has-focus-visible:outline-2 has-focus-visible:outline-blue"
            >
              <input type="radio" bind:group={sort} {value} class="sr-only" />
              {label}
            </label>
          {/each}
        </div>
      </div>
    </div>

    <!-- The concept's row of figures. A picture of the cards below: what it says, they say to a reader of the screen. -->
    <div aria-hidden="true" class="grid grid-cols-4 gap-4 pt-5">
      <div class="{tile} pb-3">
        <p class="truncate text-[13px] text-white first-letter:uppercase">{t('reachable')}</p>
        <p class={figure}>{fleet.met}<span class="text-sm font-medium text-dim">&nbsp;/ {hosts.length}</span></p>
        <p class="mt-2 flex gap-1">
          {#each fleet.states as host (host.alias)}
            <i class="h-2 max-w-6 flex-1 rounded {segment[host.state]}"></i>
          {/each}
        </p>
      </div>
      {#each [['CPU', fleet.cpu, 'text-blue'], [t('mem'), fleet.mem, 'text-teal']] as const as [label, past, colour] (label)}
        <div class="{tile} overflow-hidden">
          <p class="text-[13px] text-white">{label}</p>
          <p class={figure}>
            {#if past.length}{past.at(-1)}<span class="text-sm font-medium text-dim">%</span>{:else}—{/if}
          </p>
          <Sparkline points={[...past]} class="-mx-4 w-[calc(100%+2rem)] {colour}" />
        </div>
      {/each}
      <div class="{tile} pb-3">
        <p class="truncate text-[13px] text-white first-letter:uppercase">{t('down')}</p>
        <p class="{figure} {fleet.lost.length ? 'text-red' : ''}">{fleet.lost.length}</p>
        <p class="mt-1 truncate text-xs text-dim">{fleet.lost.join(', ')}</p>
      </div>
    </div>

    <div bind:this={grid} class="pt-5">
      {#each sections as { name, list } (name)}
        {#if grouped}
          <h2 class="flex items-baseline gap-2 pt-1 pb-3 text-lg font-semibold">
            {#if name}
              <span style="color: {chip(name)}">#{name}</span>
            {:else}
              <span class="text-white">{t('no_tags')}</span>
            {/if}
            <span class="text-sm font-normal text-dim">{list.length}</span>
          </h2>
        {/if}
        <!-- One column, two past 800 px of window, three past 1200. -->
        <ul
          aria-label={name || t('no_tags')}
          class="grid grid-cols-1 gap-5 pb-6 min-[801px]:grid-cols-2 min-[1201px]:grid-cols-3"
        >
          {#each list as host (host.alias)}
            <!-- Resting on a card shows what is known. It does not ask: a host
                 never connected to is not connected to by a pointer passing. -->
            <li
              class="relative"
              onpointerenter={() => {
                resting = setTimeout(() => (peeking ??= { alias: host.alias, hover: true }), REST)
              }}
              onpointerleave={() => {
                clearTimeout(resting)
                if (peeking?.hover && peeking.alias === host.alias) peeking = null
              }}
            >
              <HostCard
                {host}
                info={live[host.alias]}
                marked={marked.includes(host.alias)}
                onconnect={() => onconnect(host.alias)}
                onfiles={onfiles && (() => onfiles(host.alias))}
                onforwards={onforwards && (() => onforwards(host.alias))}
                onedit={onedit && (() => onedit(host.alias))}
                onretry={() => void ask(host.alias, false)}
                trail={trail[host.alias]}
                tabindex={0}
                aria-current={host.alias === selected ? 'true' : undefined}
                aria-keyshortcuts="Enter Space v f Shift+L e d Alt+ArrowLeft Alt+ArrowRight"
                onfocus={() => {
                  onselect?.(host.alias)
                  if (peeking && peeking.alias !== host.alias) peeking = null
                }}
                onkeydown={(e) => onkeydown(e, host.alias)}
                draggable="true"
                ondragstart={(e) => {
                  clearTimeout(resting)
                  peeking = null
                  dragging = host.alias
                  // Some webviews do not start a drag that carries nothing.
                  e.dataTransfer?.setData('text/plain', host.alias)
                }}
                ondragover={(e) => e.preventDefault()}
                ondrop={(e) => {
                  e.preventDefault()
                  if (dragging) place(dragging, host.alias)
                  dragging = null
                }}
                ondragend={() => (dragging = null)}
                class="cursor-grab scroll-mt-20"
              />
              {#if peeking?.alias === host.alias}
                <QuickPeek {host} info={live[host.alias]} />
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="text-white">{t('no_match')}</p>
      {/each}
    </div>
  {/if}
</section>
