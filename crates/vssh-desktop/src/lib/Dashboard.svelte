<script lang="ts">
  import { tick } from 'svelte'
  import type { Host, Shown } from './api'
  import HostCard from './HostCard.svelte'
  import { address, chip, groupOf, hasTag, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import { ask, live, sick } from './live.svelte'
  import QuickPeek from './QuickPeek.svelte'

  type Props = {
    hosts: Host[]
    /** Why there are no hosts to show, if they could not be read. */
    error?: Shown | null
    /** Only hosts with this tag or group, as chosen in the sidebar. */
    tag?: string | null
    selected?: string | null
    onselect?: (alias: string) => void
    onconnect: (alias: string) => void
    onpalette: () => void
  }
  let { hosts, error = null, tag = null, selected = null, onselect, onconnect, onpalette }: Props = $props()

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
  let root = $state<HTMLElement>()

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

  // A host chosen elsewhere (the palette, the sidebar) is brought into view.
  $effect(() => {
    if (selected) card(selected)?.focus()
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

  async function onkeydown(e: KeyboardEvent, alias: string) {
    // Not from the card's own button: there Enter is a click already.
    if (e.target !== e.currentTarget) return
    const step = e.altKey && { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[e.key]
    if (e.key === 'Enter') {
      onconnect(alias)
    } else if (e.key === 'v') {
      peek(alias)
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
    const away = document.querySelector('dialog[open]') || root?.closest('[hidden]')
    if (typing || e.ctrlKey || e.metaKey || e.altKey || away) return
    if (e.key === '/') {
      e.preventDefault()
      search?.focus()
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
  const control = 'rounded-md border border-edge bg-panel px-2 py-1 text-white'
</script>

<svelte:window onkeydown={onwindowkey} />

<section bind:this={root} aria-label={t('hosts')} class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
  {#if error}
    <p role="alert" class="pt-4 break-words text-red"><strong>{error.label}</strong> {error.message}</p>
  {/if}
  {#if hosts.length === 0}
    {#if !error}
      <div class="grid h-full place-content-center justify-items-center gap-2 text-center">
        <p aria-hidden="true" class="text-4xl text-edge">&gt;_</p>
        <h1 class="text-base">{t('no_hosts')}</h1>
        <p class="text-white">{t('no_hosts_hint')}</p>
        <button
          type="button"
          onclick={onpalette}
          aria-label={t('palette')}
          class="mt-2 cursor-pointer rounded-md border border-edge px-3 py-1 text-magenta hover:bg-selection"
        >
          Ctrl-K
        </button>
      </div>
    {/if}
  {:else}
    <div class="sticky top-0 z-20 flex flex-wrap items-center gap-2 bg-bg py-3">
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
        class="{control} min-w-40 flex-1 text-fg placeholder:text-dim"
      />
      <button
        type="button"
        aria-pressed={grouped}
        onclick={() => (grouped = !grouped)}
        class="{control} cursor-pointer hover:bg-selection aria-pressed:bg-selection aria-pressed:text-blue"
      >
        {t('group_by_tag')}
      </button>
      <!-- Radio buttons, drawn as one control: a native select opens the
           toolkit's own light menu over the dark window. -->
      <div role="radiogroup" aria-label={t('sort')} class="flex rounded-md border border-edge bg-panel p-0.5">
        {#each sorts as [value, label] (value)}
          <label
            class="cursor-pointer rounded px-2 py-0.5 text-white hover:text-fg has-checked:bg-selection has-checked:text-blue has-focus-visible:outline-2 has-focus-visible:outline-blue"
          >
            <input type="radio" bind:group={sort} {value} class="sr-only" />
            {label}
          </label>
        {/each}
      </div>
    </div>

    <div bind:this={grid}>
      {#each sections as { name, list } (name)}
        {#if grouped}
          <h2 class="flex items-baseline gap-2 pt-2 pb-2 first:pt-0">
            {#if name}
              <span style="color: {chip(name)}">#{name}</span>
            {:else}
              <span class="text-white">{t('no_tags')}</span>
            {/if}
            <span class="text-dim">{list.length}</span>
          </h2>
        {/if}
        <ul
          aria-label={name || t('no_tags')}
          class="grid grid-cols-[repeat(auto-fill,minmax(17rem,1fr))] gap-3 pb-3"
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
                onconnect={() => onconnect(host.alias)}
                tabindex={0}
                aria-current={host.alias === selected ? 'true' : undefined}
                aria-keyshortcuts="Enter v Alt+ArrowLeft Alt+ArrowRight"
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
                class="cursor-grab"
              />
              {#if peeking?.alias === host.alias}
                <QuickPeek {host} info={live[host.alias]} />
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="pt-2 text-white">{t('no_match')}</p>
      {/each}
    </div>
  {/if}
</section>
