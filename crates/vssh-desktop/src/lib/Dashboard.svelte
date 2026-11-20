<script lang="ts">
  import { tick } from 'svelte'
  import { api, type Host, type Shown } from './api'
  import HostCard from './HostCard.svelte'
  import { address, chip, groupOf, hasTag, tagCounts, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import { ask, critical, down, live, sick, trail, up } from './live.svelte'
  import QuickPeek from './QuickPeek.svelte'
  import Keys from './Keys.svelte'
  import { KEY, tip } from './keymap'
  import Menu, { pointed, type MenuItem } from './Menu.svelte'
  import Sparkline from './Sparkline.svelte'
  import { streamer, target } from './streamer.svelte'

  type Props = {
    hosts: Host[]
    /** Why there are no hosts to show, if they could not be read. */
    error?: Shown | null
    /** Only hosts with this tag or group: chosen here, in the row under the search, or in the rail. */
    tag?: string | null
    selected?: string | null
    /** A terminal tab is in front of it. */
    hidden?: boolean
    onselect?: (alias: string) => void
    onconnect: (alias: string) => void
    /** Connects to what the search holds, `[user@]host[:port]`, and saves nothing; rejects with the refusal, already in words. */
    onquick?: (target: string) => Promise<void>
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
    /** The hosts as a JSON file: out to one, in from one. */
    onexport?: () => void
    onimport?: () => void
    /** The cards chosen to act on together, as Space marks hosts in the TUI. */
    marked?: string[]
    /** No host is watched; and a card's own switch, and the one over them all. */
    paused?: boolean
    onwatch?: (alias: string, watched: boolean) => void
    onpause?: (paused: boolean) => void
  }
  let {
    hosts,
    error = null,
    tag = $bindable(null),
    selected = null,
    hidden = false,
    onselect,
    onconnect,
    onquick,
    onfiles,
    onforwards,
    onsnippets,
    onbatch,
    onpalette,
    onadd,
    onedit,
    ondelete,
    onexport,
    onimport,
    marked = $bindable([]),
    paused = false,
    onwatch,
    onpause,
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
  /** The card being dragged, and the one it is over: where it would land. */
  let dragging: string | null = null
  let over = $state<string | null>(null)
  /** The card whose quick peek is open, and whether the pointer opened it. */
  let peeking = $state<{ alias: string; hover: boolean } | null>(null)
  let resting: ReturnType<typeof setTimeout> | undefined
  /** The right button's menu, and the card it is about. */
  let menu = $state<{ x: number; y: number; alias: string } | null>(null)
  let search = $state<HTMLInputElement>()
  /** Why what the search held could not be connected to. */
  let refusal = $state<Shown | null>(null)
  let busy = false
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

  const words = $derived(query.trim().toLowerCase())
  const found = (host: Host) =>
    // As `/#tag` in the TUI: the start of a tag or a group.
    words.startsWith('#')
      ? tagsOf(host).some((name) => name.toLowerCase().startsWith(words.slice(1)))
      : [host.alias, address(host)].some((field) => field.toLowerCase().includes(words))

  /** The cards on screen: filtered, and whoever needs looking at first. */
  const shown = $derived.by(() => {
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

  const tags = $derived(tagCounts(hosts))
  /** What the search holds is an address to connect to as it stands: it has an @, or it is the name of no host. As in the palette. */
  const quick = $derived(!!onquick && words !== '' && !/[\s#]/.test(words) && (words.includes('@') || !hosts.some(found)))

  /** Connects to it; the search is empty again for the hosts to come back to. */
  async function direct() {
    if (busy || !onquick) return
    busy = true
    refusal = null
    try {
      await onquick(query.trim())
      query = ''
    } catch (error) {
      refusal = error as Shown
    } finally {
      busy = false
    }
  }

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
    // Not from the card's own button: there Enter is a click already. Nor with
    // Ctrl: Ctrl+F on a card is the search, not the card's f.
    if (e.target !== e.currentTarget || e.ctrlKey || e.metaKey) return
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
    } else if (e.key === 'l' || e.key === 'L') {
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
    // The desktop's key for what `/` is, from a field too.
    if ((e.ctrlKey || e.metaKey) && e.key === 'f' && !away) {
      e.preventDefault()
      search?.focus()
    }
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

  /** What the right button offers on a card: what its keys do, and its address for the clipboard. */
  const items = $derived.by((): MenuItem[] => {
    if (!menu) return []
    const alias = menu.alias
    const host = hosts.find((host) => host.alias === alias)
    return [
      { label: t('connect'), keys: KEY.connect[0], run: () => onconnect(alias) },
      ...(onfiles ? [{ label: t('files'), keys: KEY.files[0], run: () => onfiles(alias) }] : []),
      ...(onforwards ? [{ label: t('forwards'), keys: KEY.forwards[0], run: () => onforwards(alias) }] : []),
      ...(onedit ? [{ label: t('edit'), keys: KEY.edit[0], run: () => onedit(alias) }] : []),
      // The address as the config has it, whatever the window shows of it.
      { label: t('copy_address'), off: !host || !address(host), run: () => void api.copy(address(host!)) },
      null,
      { label: t('palette'), keys: KEY.palette[0], run: onpalette },
      ...(ondelete ? [null, { label: t('delete'), keys: 'D', danger: true, run: () => ondelete(alias) }] : []),
    ]
  })

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
  // A tag of the row under the search: grey until the cards are narrowed to it.
  const pick =
    'cursor-pointer rounded-lg bg-bar px-2.5 text-xs leading-7 whitespace-nowrap text-dim hover:text-fg aria-pressed:text-ink aria-pressed:inset-ring aria-pressed:inset-ring-current'
  const tile = 'min-w-0 rounded-2xl border border-edge bg-panel px-4.5 pt-3.5'
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
          class="grid size-14 place-items-center overflow-hidden rounded-2xl border border-edge bg-page"
        >
          <!-- The brand mark, as the rail draws it. -->
          <svg viewBox="0 0 512 512" class="h-14 w-14">
            <defs>
              <linearGradient id="brand-empty-g" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="#7aa2f7" />
                <stop offset="1" stop-color="#7dcfff" />
              </linearGradient>
            </defs>
            <path
              d="M 150 140 L 290 248 L 150 356"
              fill="none"
              stroke="url(#brand-empty-g)"
              stroke-width="46"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <text
              x="248"
              y="418"
              font-family="ui-monospace, monospace"
              font-size="112"
              font-weight="700"
              fill="#7dcfff"
              letter-spacing="2">ssh</text>
          </svg>
        </p>
        <h2 class="text-lg font-semibold text-ink">{t('no_hosts')}</h2>
        <p class="text-white">{t('no_hosts_hint')}</p>
        <div class="mt-2 flex gap-2">
          <button type="button" onclick={onpalette} aria-label={t('palette')} aria-keyshortcuts="Control+K" class="{control} flex items-center gap-2">
            {t('palette')}
            <Keys keys={KEY.palette[0]} />
          </button>
          {#if onimport}
            <button type="button" onclick={onimport} class={control}>{t('import_hosts_json')}</button>
          {/if}
          {#if onadd}
            <button type="button" onclick={onadd} class="{control} border-blue bg-blue font-semibold text-page hover:bg-blue/90">
              {t('add_host')}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  {:else}
    <!-- Two rows, from one margin: the search as wide as the view, with what it connects to and the new host;
         under it the tags, one row of them however many there are, and how the cards are laid out. -->
    <div class="sticky top-0 z-20 -mx-8 grid gap-2.5 border-b border-edge bg-bg px-8 pt-4 pb-4">
      <!-- It wraps in a window too narrow for it, as the row under it does. -->
      <div class="flex flex-wrap items-center gap-2.5">
        <input
          bind:this={search}
          bind:value={query}
          type="search"
          aria-label={t('search_hosts')}
          aria-keyshortcuts="/ Control+F"
          title={tip(t('search_hosts'), KEY.search)}
          placeholder={t('search_hosts')}
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          data-masked={(streamer.on && /[.:@]/.test(query)) || undefined}
          oninput={() => (refusal = null)}
          onkeydown={(e) => {
            if (e.key === 'Escape') {
              query = ''
              refusal = null
              search?.blur()
            } else if (e.key === 'ArrowDown' || e.key === 'Enter') {
              // To the first card found, whose Enter connects; with none, Enter is for the address typed.
              if (shown.length) card(shown[0].alias)?.focus()
              else if (e.key === 'Enter' && quick) void direct()
              else return
              e.preventDefault()
            }
          }}
          class="h-10 min-w-40 flex-1 rounded-full border border-edge bg-panel px-4 text-fg placeholder:text-dim focus:border-blue data-[masked]:[-webkit-text-security:disc]"
        />
        {#if quick}
          <button type="button" onclick={direct} title="{t('quick_connect')} · {t('quick_connect_note')}" class="{control} max-w-[40%] truncate border-cyan/60 text-cyan">
            {t('connect_to').replace('{target}', target(query.trim()))}
          </button>
        {/if}
        <!-- Not while an address is being typed: the row is that connection's. -->
        {#if onexport && !quick}
          <button type="button" onclick={onexport} class={control}>{t('export_hosts_json')}</button>
        {/if}
        {#if onimport && !quick}
          <button type="button" onclick={onimport} class={control}>{t('import_hosts_json')}</button>
        {/if}
        {#if onadd}
          <!-- The concept's one filled button. -->
          <button
            type="button"
            aria-keyshortcuts="a"
            title={tip(t('add_host'), KEY.add)}
            onclick={onadd}
            class="{control} border-blue bg-blue font-semibold text-page hover:bg-blue/90"
          >
            {t('add_host')}
          </button>
        {/if}
      </div>
      {#if refusal}
        <p role="alert" class="px-4 break-words text-red"><strong>{refusal.label}</strong> {refusal.message}</p>
      {/if}
      <div class="flex flex-wrap items-center gap-2.5">
        <!-- One row: past its end the tags are scrolled to, and the cards do not start lower for them. -->
        <ul
          aria-label={t('tags')}
          class="flex min-w-40 flex-1 items-center gap-1.5 overflow-x-auto py-0.5 pr-6 [mask-image:linear-gradient(to_right,black_calc(100%-1.5rem),transparent)] [scrollbar-width:none]"
        >
          {#if tags.length}
            <li class="flex shrink-0">
              <button type="button" aria-pressed={tag === null} onclick={() => (tag = null)} class={pick}>{t('all_tags')}</button>
            </li>
          {/if}
          {#each tags as [name] (name)}
            <li class="flex shrink-0">
              <button
                type="button"
                aria-pressed={tag === name}
                onclick={() => (tag = tag === name ? null : name)}
                style:color={tag === name ? chip(name) : undefined}
                class={pick}
              >
                #{name}
              </button>
            </li>
          {/each}
        </ul>
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
            title={tip(t('run_command'), KEY.run)}
            onclick={() => onbatch?.()}
            class="{control} border-magenta/70 text-magenta"
          >
            {t('run')} {marked.length}
          </button>
        {/if}
        <!-- Radio buttons, drawn as one control: a native select opens the
             toolkit's own light menu over the dark window. -->
        <div role="radiogroup" aria-label={t('sort')} class="flex h-10 items-center rounded-xl border border-edge bg-page p-1">
          {#each sorts as [value, label] (value)}
            <label
              class="cursor-pointer rounded-lg px-2.5 py-1 text-sm whitespace-nowrap text-dim hover:text-fg has-checked:bg-selection has-checked:font-medium has-checked:text-ink has-focus-visible:outline-2 has-focus-visible:outline-blue"
            >
              <input type="radio" bind:group={sort} {value} class="sr-only" />
              {label}
            </label>
          {/each}
        </div>
      </div>
    </div>

    {#if paused}
      <!-- Said where it is seen: the figures below are what was last heard, and stay so. -->
      <p role="status" class="mt-5 flex items-center gap-3 rounded-xl border border-dashed border-yellow/50 bg-bg px-4 py-2 text-yellow">
        <strong class="shrink-0 font-semibold">{t('watch_pause')}</strong>
        <span class="min-w-0 flex-1 text-white">{t('watch_pause_note')}</span>
        {#if onpause}
          <button type="button" onclick={() => onpause(false)} class="{control} shrink-0">{t('watch')}</button>
        {/if}
      </p>
    {/if}

    <!-- The concept's row of figures. A picture of the cards below: what it says, they say to a reader of the screen. -->
    <div aria-hidden="true" class="grid grid-cols-4 gap-4 pt-5">
      <div class="{tile} pb-3.5">
        <p class="truncate text-sm text-white first-letter:uppercase">{t('reachable')}</p>
        <p class={figure}>{fleet.met}<span class="text-md font-medium text-dim">&nbsp;/ {hosts.length}</span></p>
        <p class="mt-2 flex gap-1">
          {#each fleet.states as host (host.alias)}
            <i class="h-2 max-w-6 flex-1 rounded {segment[host.state]}"></i>
          {/each}
        </p>
      </div>
      {#each [['CPU', fleet.cpu, 'text-blue'], [t('mem'), fleet.mem, 'text-teal']] as const as [label, past, colour] (label)}
        <div class="{tile} overflow-hidden">
          <p class="text-sm text-white">{label}</p>
          <p class={figure}>
            {#if past.length}{past.at(-1)}<span class="text-md font-medium text-dim">%</span>{:else}—{/if}
          </p>
          <Sparkline points={[...past]} class="-mx-4.5 w-[calc(100%+2.25rem)] {colour}" />
        </div>
      {/each}
      <div class="{tile} pb-3.5">
        <p class="truncate text-sm text-white first-letter:uppercase">{t('down')}</p>
        <p class="{figure} {fleet.lost.length ? 'text-red' : ''}">{fleet.lost.length}</p>
        <p class="mt-1 truncate text-xs text-dim">{fleet.lost.join(', ')}</p>
      </div>
    </div>

    <div bind:this={grid} class="pt-5">
      {#each sections as { name, list } (name)}
        {#if grouped}
          <h2 class="flex items-baseline gap-2 pt-1 pb-3 text-lg font-semibold">
            <span class="text-white">{name || t('no_tags')}</span>
            <span class="text-md font-normal text-dim">({list.length})</span>
          </h2>
        {/if}
        <!-- One column, two past 800 px of window, three past 1200. -->
        <ul
          aria-label={name || t('no_tags')}
          class="grid grid-cols-1 gap-3 pb-5 min-[801px]:grid-cols-2 min-[1201px]:grid-cols-3"
        >
          {#each list as host, at (host.alias)}
            <!-- Resting on a card shows what is known. It does not ask: a host
                 never connected to is not connected to by a pointer passing. -->
            <!-- The hosts that need looking at come first and are tall; the first that is well starts a row of its own, of low ones. -->
            <li
              class="relative {at > 0 && !sick(live[host.alias]) && sick(live[list[at - 1].alias]) ? 'col-start-1' : ''}"
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
                onpeek={() => peek(host.alias)}
                onwatch={onwatch && ((watched) => onwatch(host.alias, watched))}
                {paused}
                trail={trail[host.alias]}
                tabindex={0}
                aria-current={host.alias === selected ? 'true' : undefined}
                aria-keyshortcuts="Enter Space v f l e d Delete Alt+ArrowLeft Alt+ArrowRight"
                onfocus={() => {
                  onselect?.(host.alias)
                  if (peeking && peeking.alias !== host.alias) peeking = null
                }}
                onkeydown={(e) => onkeydown(e, host.alias)}
                ondblclick={(e) => {
                  // Not twice over for a button of the card's own.
                  if (!(e.target as HTMLElement).closest('button')) onconnect(host.alias)
                }}
                oncontextmenu={(e) => {
                  e.preventDefault()
                  clearTimeout(resting)
                  peeking = null
                  onselect?.(host.alias)
                  menu = { ...pointed(e), alias: host.alias }
                }}
                draggable="true"
                ondragstart={(e) => {
                  clearTimeout(resting)
                  peeking = null
                  dragging = host.alias
                  // Some webviews do not start a drag that carries nothing.
                  e.dataTransfer?.setData('text/plain', host.alias)
                }}
                data-drop={over === host.alias || undefined}
                ondragenter={(e) => e.preventDefault()}
                ondragover={(e) => {
                  // A web view lets go only where it was let in and kept: at the entering as at the moving.
                  e.preventDefault()
                  // No peek opens under a card in hand: it would be what the card is let go on.
                  clearTimeout(resting)
                  if (dragging && dragging !== host.alias) over = host.alias
                }}
                ondragleave={(e) => {
                  // By where the pointer is: a web view says a leave for every part of the card on the way across it.
                  const box = e.currentTarget.getBoundingClientRect()
                  if (e.clientX < box.left || e.clientX >= box.right || e.clientY < box.top || e.clientY >= box.bottom) over = null
                }}
                ondrop={(e) => {
                  e.preventDefault()
                  if (dragging) place(dragging, host.alias)
                  dragging = over = null
                }}
                ondragend={() => (dragging = over = null)}
                class="cursor-grab scroll-mt-20 active:cursor-grabbing data-[drop]:border-blue data-[drop]:bg-blue/8"
              />
              {#if peeking?.alias === host.alias}
                <QuickPeek {host} info={live[host.alias]} trail={trail[host.alias]} onrefresh={() => void ask(host.alias, false)} />
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

{#if menu}
  <Menu
    label={menu.alias}
    x={menu.x}
    y={menu.y}
    {items}
    onclose={(escaped) => {
      // Esc gives the keys back to the card it was about.
      if (escaped && menu) card(menu.alias)?.focus()
      menu = null
    }}
  />
{/if}
