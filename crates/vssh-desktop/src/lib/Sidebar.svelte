<script lang="ts">
  import type { Host, VaultState } from './api'
  import { chip, hasTag, tagCounts } from './hosts'
  import { i18n, setLang, t } from './i18n.svelte'
  import Icon, { type IconName } from './Icon.svelte'
  import Keys from './Keys.svelte'
  import SessionDot from './SessionDot.svelte'
  import { KEY, tip } from './keymap'
  import Menu, { tall, type MenuItem } from './Menu.svelte'
  import { addr, setStreamer, streamer } from './streamer.svelte'
  import type { Tab } from './Tabs.svelte'
  import { setTheme, theme } from './theme.svelte'

  /** Where the rail leads: the three views, and the dialogs that have no view of their own. */
  export type Nav = 'dashboard' | 'hosts' | 'sessions' | 'files' | 'snippets' | 'forwards'

  type Props = {
    hosts: Host[]
    selected: string | null
    onselect: (alias: string) => void
    /** The tag the list is narrowed to; the dashboard follows it. */
    tag?: string | null
    /** The open sessions, the one in front, and what a click on one does. */
    sessions?: Tab[]
    active?: number | null
    onsession?: (id: number) => void
    /** What the vault is, and the way to it. */
    vault?: VaultState
    onvault?: () => void
    /** The view in front, how many snippets there are, and what a click on the rail asks for. */
    current?: Nav
    snippets?: number
    /** How many forwards listen now, of all hosts. */
    tunnels?: number
    onnav?: (to: Nav) => void
    /** Icons only, a quarter as wide; and what its corner's button asks for. */
    compact?: boolean
    oncompact?: (on: boolean) => void
    /** The palette, which finds hosts and actions alike: every view has the way to it here. */
    onpalette?: () => void
    /** The cheatsheet; and focus mode: the rail goes, and the tabs have the way back. Both from the settings. */
    onshortcuts?: () => void
    onzen?: () => void
  }
  let {
    hosts,
    selected,
    onselect,
    tag = $bindable(null),
    sessions = [],
    active = null,
    onsession,
    vault = 'none',
    onvault,
    current,
    snippets,
    tunnels = 0,
    onnav,
    compact = false,
    oncompact,
    onpalette,
    onshortcuts,
    onzen,
  }: Props = $props()

  let list = $state<HTMLElement>()
  /** The settings' menu, and where: over the button that opens it. */
  let menu = $state<{ x: number; y: number } | null>(null)

  /** What is set once and left: how the window speaks, what it looks like, what it shows of an address. And the two ways to its keys. */
  const settings = $derived.by((): MenuItem[] => [
    { label: t('language'), note: i18n.lang.toUpperCase(), run: () => setLang(i18n.lang === 'en' ? 'vi' : 'en') },
    { label: t('theme'), note: theme.name === 'night' ? t('theme_night') : t('theme_day'), run: () => setTheme(theme.name === 'night' ? 'day' : 'night') },
    { label: t('streamer_mode'), note: streamer.on ? '✓' : undefined, run: () => setStreamer(!streamer.on) },
    ...(onzen || onshortcuts ? [null] : []),
    ...(onzen ? [{ label: t('focus_mode'), keys: KEY.focus[0], run: onzen }] : []),
    ...(onshortcuts ? [{ label: t('shortcuts'), keys: KEY.shortcuts[0], run: onshortcuts }] : []),
  ])

  const tags = $derived(tagCounts(hosts))
  const shown = $derived(hosts.filter((host) => tag === null || hasTag(host, tag)))

  // A host chosen elsewhere (the palette) may be below the fold; so may the
  // selected one once the sessions take their share of the list's height.
  $effect(() => {
    void sessions.length
    if (selected) list?.querySelector('[aria-current]')?.scrollIntoView({ block: 'nearest' })
  })

  // A row of the rail; narrow, a square with its icon, the number at its corner.
  const nav = $derived(
    `cursor-pointer rounded-[10px] font-medium text-white transition-colors hover:bg-panel hover:text-fg disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent aria-[current]:bg-selection aria-[current]:text-ink aria-[current]:inset-ring aria-[current]:inset-ring-edge [&[aria-current]>svg]:text-blue ${
      compact ? 'relative grid size-10 shrink-0 place-items-center' : 'flex w-full items-center gap-3 px-3 py-1.5 text-left'
    }`,
  )
  const count = $derived(
    compact
      ? 'absolute -top-0.5 -right-0.5 min-w-4 rounded-full bg-page px-1 text-center text-[10px] leading-4 font-normal text-dim not-italic'
      : 'ml-auto rounded-full bg-page px-2 text-xs font-normal text-dim not-italic',
  )
  const row = 'w-full cursor-pointer rounded-lg py-0.5 pr-2 pl-[42px] text-left hover:bg-panel aria-[current]:bg-selection'
</script>

<!-- One row of the rail: an icon, a name, and a number where there is one to say. Narrow, the name is its tooltip. -->
{#snippet item(to: Nav, icon: IconName, label: string, n?: number, off = false, tone = '')}
  <button type="button" onclick={() => onnav?.(to)} disabled={off} aria-current={current === to ? 'page' : undefined} title={compact ? label : undefined} class={nav}>
    <Icon name={icon} />
    <span class={compact ? 'sr-only' : ''}>{label}</span>
    {#if n !== undefined}<em class="{count} {tone}">{n}</em>{/if}
  </button>
{/snippet}

<nav
  aria-label={t('hosts')}
  data-compact={compact || undefined}
  class="flex min-h-0 flex-col gap-0.5 overflow-hidden border-r border-edge bg-bg pt-4 pb-3 {compact ? 'items-center px-2' : 'px-3'}"
>
  <!-- The name is the same in every language: drawn, not said. -->
  <div class="flex items-center pb-3 {compact ? 'flex-col gap-2' : 'gap-3 px-2'}">
    <span
      aria-hidden="true"
      class="grid size-9 shrink-0 place-items-center overflow-hidden rounded-xl border border-edge bg-page"
    >
      <!-- The brand mark: a > whose V is lying down, and "ssh" as its command. -->
      <svg viewBox="0 0 512 512" aria-hidden="true" class="h-9 w-9">
        <defs>
          <linearGradient id="brand-g" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#7aa2f7" />
            <stop offset="1" stop-color="#7dcfff" />
          </linearGradient>
        </defs>
        <path
          d="M 150 140 L 290 248 L 150 356"
          fill="none"
          stroke="url(#brand-g)"
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
    </span>
    {#if !compact}
      <span aria-hidden="true" class="text-base font-semibold tracking-tight text-ink after:content-['vssh']"></span>
    {/if}
    {#if oncompact}
      <!-- The rail's own switch: all of it, or its icons. Focus mode, which takes it away, is a key and a row of the settings. -->
      <button
        type="button"
        onclick={() => oncompact(!compact)}
        aria-label={compact ? t('rail_full') : t('rail_compact')}
        aria-pressed={compact}
        title={compact ? t('rail_full') : t('rail_compact')}
        class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-lg text-dim hover:bg-panel hover:text-fg {compact ? '' : 'ml-auto'}"
      >
        <Icon name="rail" />
      </button>
    {/if}
  </div>

  {#if onpalette}
    <!-- The one search every view has: the palette. The dashboard's own field filters its cards. -->
    <button
      type="button"
      onclick={onpalette}
      aria-label={t('palette')}
      aria-keyshortcuts="Control+K"
      title={compact ? tip(t('palette'), KEY.palette) : undefined}
      class="mb-2 flex shrink-0 cursor-pointer items-center gap-2.5 rounded-[10px] border border-edge bg-panel text-left text-dim hover:border-dim/50 hover:text-fg {compact
        ? 'size-10 justify-center'
        : 'h-9 px-3'}"
    >
      <Icon name="search" />
      {#if !compact}
        <span class="min-w-0 flex-1 truncate text-[13px]">{t('search')}</span>
        <Keys keys={KEY.palette[0]} />
      {/if}
    </button>
  {/if}

  {@render item('dashboard', 'grid', t('status'))}
  <h2 class={compact ? 'contents' : ''}>{@render item('hosts', 'server', t('hosts'), shown.length)}</h2>
  {#if hosts.length && !compact}
    <!-- As tall as its hosts, and no taller than there is room: what follows it follows it. -->
    <ul bind:this={list} aria-label={t('hosts')} class="min-h-12 overflow-y-auto">
      {#each shown as host (host.alias)}
        <li>
          <!-- One short line a host: its name. Its address is said to the pointer that rests on it. -->
          <button
            type="button"
            aria-current={host.alias === selected ? 'true' : undefined}
            title={addr(host) || undefined}
            onclick={() => onselect(host.alias)}
            class="{row} flex gap-1.5 text-[13px]"
          >
            {#if host.meta.pinned}
              <span role="img" aria-label={t('pinned')} class="text-yellow">★</span>
            {/if}
            <span class="truncate">{host.alias}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <!-- With none open there is nothing to go to, and no heading for a list that is not there. -->
  {#if sessions.length}
    <h2 class={compact ? 'contents' : ''}>{@render item('sessions', 'term', t('sessions'), sessions.length)}</h2>
    {#if !compact}
      <ul aria-label={t('sessions')} class="max-h-28 shrink-0 overflow-y-auto">
        {#each sessions as session (session.id)}
          <li>
            <button
              type="button"
              aria-current={session.id === active ? 'true' : undefined}
              onclick={() => onsession?.(session.id)}
              class="{row} flex gap-1.5 text-[13px]"
            >
              <SessionDot {session} />
              <span class="truncate">{session.title}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    {@render item('sessions', 'term', t('sessions'), undefined, true)}
  {/if}
  <!-- A host's files and its forwards: the selected host's. -->
  {@render item('files', 'folder', t('files'), undefined, !selected)}
  {@render item('snippets', 'code', t('snippets'), snippets)}
  <!-- What listens goes on listening behind a dialog that is shut: said here, in the colour of what is up. -->
  {@render item('forwards', 'fwd', t('forwards'), tunnels || undefined, !selected, 'text-green')}
  {#if onvault}
    {@const state = vault === 'none' ? '' : vault === 'unlocked' ? t('vault_unlocked') : t('vault_locked')}
    <button type="button" onclick={onvault} title={compact ? `${t('vault')} ${state}`.trim() : undefined} class={nav}>
      <Icon name="lock" class={vault === 'unlocked' ? 'text-green' : vault === 'locked' && compact ? 'text-yellow' : ''} />
      <span class={compact ? 'sr-only' : ''}>{t('vault')}</span>
      {#if state}
        <em class={compact ? 'sr-only' : `${count} ${vault === 'unlocked' ? 'text-green' : 'text-yellow'}`}>{state}</em>
      {/if}
    </button>
  {/if}

  {#if !compact}
    <h2 class="mt-auto flex justify-between px-3 pt-4 pb-1 text-[11px] font-semibold tracking-widest text-dim uppercase">
      {t('tags')} <span>{tags.length}</span>
    </h2>
    {#if tags.length === 0}
      <p class="px-3 pb-2 text-[13px] text-white">{t('no_tags')}</p>
    {:else}
      <!-- Small: three whole rows of them (18 px each, 4 between, 8 under) and no half of a fourth. -->
      <ul aria-label={t('tags')} class="flex max-h-[70px] shrink-0 flex-wrap gap-1 overflow-y-auto px-3 pb-2">
        {#each tags as [name, n] (name)}
          <!-- flex: as tall as its button, not as a line of text. -->
          <li class="flex">
            <button
              type="button"
              aria-pressed={tag === name}
              onclick={() => (tag = tag === name ? null : name)}
              style:color={tag === name ? chip(name) : undefined}
              class="cursor-pointer rounded bg-bar px-1 text-[10.5px] leading-[18px] text-dim hover:text-fg aria-pressed:inset-ring aria-pressed:inset-ring-current"
            >
              #{name} <span class="opacity-70">{n}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  <!-- What is set once and left, in one place: the language, the theme, streamer mode, and the ways to the keys. -->
  <button
    type="button"
    aria-haspopup="menu"
    aria-expanded={menu !== null}
    title={compact ? t('settings') : undefined}
    onclick={(e) => {
      // A click anywhere closes a menu: this one opens it after that.
      e.stopPropagation()
      const box = e.currentTarget.getBoundingClientRect()
      menu = menu ? null : { x: box.left, y: box.top - tall(settings) - 4 }
    }}
    class="{nav} {compact ? 'mt-auto' : 'mt-1 border-t border-edge pt-2.5'}"
  >
    <Icon name="cog" />
    <span class={compact ? 'sr-only' : ''}>{t('settings')}</span>
  </button>
</nav>

{#if menu}
  <Menu label={t('settings')} x={menu.x} y={menu.y} items={settings} onclose={() => (menu = null)} />
{/if}
