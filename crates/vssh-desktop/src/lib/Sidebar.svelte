<script lang="ts">
  import type { Host, VaultState } from './api'
  import { chip, hasTag, tagCounts } from './hosts'
  import { t } from './i18n.svelte'
  import Icon, { type IconName } from './Icon.svelte'
  import SessionDot from './SessionDot.svelte'
  import { addr } from './streamer.svelte'
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
    onnav?: (to: Nav) => void
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
    onnav,
  }: Props = $props()

  let list = $state<HTMLElement>()

  const tags = $derived(tagCounts(hosts))
  const shown = $derived(hosts.filter((host) => tag === null || hasTag(host, tag)))

  // A host chosen elsewhere (the palette) may be below the fold; so may the
  // selected one once the sessions take their share of the list's height.
  $effect(() => {
    void sessions.length
    if (selected) list?.querySelector('[aria-current]')?.scrollIntoView({ block: 'nearest' })
  })

  const nav =
    'flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-3 py-1.5 text-left font-medium text-white transition-colors hover:bg-panel hover:text-fg disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent aria-[current]:bg-selection aria-[current]:text-ink aria-[current]:inset-ring aria-[current]:inset-ring-edge [&[aria-current]>svg]:text-blue'
  const count = 'ml-auto rounded-full bg-page px-2 text-xs font-normal text-dim not-italic'
  const row = 'w-full cursor-pointer rounded-lg py-1 pr-2 pl-[42px] text-left hover:bg-panel aria-[current]:bg-selection'
</script>

<!-- One row of the rail: an icon, a name, and a number where there is one to say. -->
{#snippet item(to: Nav, icon: IconName, label: string, n?: number, off = false)}
  <button type="button" onclick={() => onnav?.(to)} disabled={off} aria-current={current === to ? 'page' : undefined} class={nav}>
    <Icon name={icon} />
    {label}
    {#if n !== undefined}<em class={count}>{n}</em>{/if}
  </button>
{/snippet}

<nav aria-label={t('hosts')} class="flex min-h-0 flex-col gap-0.5 border-r border-edge bg-bg px-3 pt-4 pb-3">
  <!-- The name is the same in every language: drawn, not said. -->
  <div aria-hidden="true" class="flex items-center gap-3 px-2 pb-4">
    <span
      class="grid size-9 place-items-center rounded-xl bg-linear-to-br from-blue to-magenta font-mono text-sm font-bold text-page shadow-lg shadow-blue/30"
    >
      &gt;_
    </span>
    <span class="text-base font-semibold tracking-tight text-ink after:content-['vssh']"></span>
  </div>

  {@render item('dashboard', 'grid', t('status'))}
  <h2>{@render item('hosts', 'server', t('hosts'), shown.length)}</h2>
  {#if hosts.length}
    <ul bind:this={list} aria-label={t('hosts')} class="min-h-12 flex-1 overflow-y-auto">
      {#each shown as host (host.alias)}
        <li>
          <button
            type="button"
            aria-current={host.alias === selected ? 'true' : undefined}
            onclick={() => onselect(host.alias)}
            class="{row} flex items-baseline gap-2"
          >
            <!-- One line a host: its name, and as much of its address as there is room for. -->
            <span class="flex max-w-[65%] shrink-0 gap-1.5 text-[13px]">
              {#if host.meta.pinned}
                <span role="img" aria-label={t('pinned')} class="text-yellow">★</span>
              {/if}
              <span class="truncate">{host.alias}</span>
            </span>
            {#if addr(host)}
              <span class="min-w-0 flex-1 truncate text-right font-mono text-[11px] text-dim">{addr(host)}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <!-- With none open there is nothing to go to, and no heading for a list that is not there. -->
  {#if sessions.length}
    <h2>{@render item('sessions', 'term', t('sessions'), sessions.length)}</h2>
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
  {:else}
    {@render item('sessions', 'term', t('sessions'), undefined, true)}
  {/if}
  <!-- A host's files and its forwards: the selected host's. -->
  {@render item('files', 'folder', t('files'), undefined, !selected)}
  {@render item('snippets', 'code', t('snippets'), snippets)}
  {@render item('forwards', 'fwd', t('forwards'), undefined, !selected)}
  {#if onvault}
    <button type="button" onclick={onvault} class={nav}>
      <Icon name="lock" class={vault === 'unlocked' ? 'text-green' : ''} />
      {t('vault')}
      {#if vault !== 'none'}
        <em class="{count} {vault === 'unlocked' ? 'text-green' : 'text-yellow'}">
          {vault === 'unlocked' ? t('vault_unlocked') : t('vault_locked')}
        </em>
      {/if}
    </button>
  {/if}

  <h2 class="mt-auto flex justify-between px-3 pt-4 pb-1 text-[11px] font-semibold tracking-widest text-dim uppercase">
    {t('tags')} <span>{tags.length}</span>
  </h2>
  {#if tags.length === 0}
    <p class="px-3 pb-2 text-[13px] text-white">{t('no_tags')}</p>
  {:else}
    <ul aria-label={t('tags')} class="flex max-h-24 shrink-0 flex-wrap gap-1.5 overflow-y-auto px-3 pb-2">
      {#each tags as [name, n] (name)}
        <li>
          <button
            type="button"
            aria-pressed={tag === name}
            onclick={() => (tag = tag === name ? null : name)}
            style="color: {chip(name)}"
            class="cursor-pointer rounded-full border border-edge px-2 text-xs leading-5 hover:bg-panel aria-pressed:border-current aria-pressed:bg-selection"
          >
            #{name} <span class="text-white">{n}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <!-- A moon for Night, a sun for Day. -->
  <button
    type="button"
    onclick={() => setTheme(theme.name === 'night' ? 'day' : 'night')}
    aria-label="{t('theme')}: {theme.name === 'night' ? t('theme_night') : t('theme_day')}"
    class="{nav} mt-1 border-t border-edge pt-2.5"
  >
    <Icon name={theme.name === 'night' ? 'moon' : 'sun'} />
    {t('theme')}
    <em class={count}>{theme.name === 'night' ? t('theme_night') : t('theme_day')}</em>
  </button>
</nav>
