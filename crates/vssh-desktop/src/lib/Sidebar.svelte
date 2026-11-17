<script lang="ts">
  import type { Host, VaultState } from './api'
  import { chip, hasTag, tagCounts } from './hosts'
  import { t } from './i18n.svelte'
  import SessionDot from './SessionDot.svelte'
  import { addr, setStreamer, streamer } from './streamer.svelte'
  import type { Tab } from './Tabs.svelte'
  import { setTheme, theme } from './theme.svelte'

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

  const heading = 'flex justify-between px-3 pt-3 pb-1 text-cyan'
  const toggle = 'cursor-pointer px-2 text-white hover:bg-bar hover:text-fg'
</script>

<nav aria-label={t('hosts')} class="flex min-h-0 flex-col border-r border-edge bg-black">
  <h2 class={heading}>{t('hosts')} <span class="text-white">{shown.length}</span></h2>
  {#if hosts.length}
    <ul bind:this={list} aria-label={t('hosts')} class="min-h-0 flex-1 overflow-y-auto">
      {#each shown as host (host.alias)}
        <li>
          <button
            type="button"
            aria-current={host.alias === selected ? 'true' : undefined}
            onclick={() => onselect(host.alias)}
            class="block w-full cursor-pointer px-3 py-1 text-left hover:bg-bar aria-[current]:bg-selection"
          >
            <span class="flex gap-1.5">
              {#if host.meta.pinned}
                <span role="img" aria-label={t('pinned')} class="text-yellow">★</span>
              {/if}
              <span class="truncate">{host.alias}</span>
            </span>
            {#if addr(host)}
              <span class="block truncate text-white">{addr(host)}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if sessions.length}
    <h2 class="{heading} border-t border-edge">{t('sessions')} <span class="text-white">{sessions.length}</span></h2>
    <ul aria-label={t('sessions')} class="max-h-40 shrink-0 overflow-y-auto pb-1">
      {#each sessions as session (session.id)}
        <li>
          <button
            type="button"
            aria-current={session.id === active ? 'true' : undefined}
            onclick={() => onsession?.(session.id)}
            class="flex w-full cursor-pointer gap-1.5 px-3 py-1 text-left hover:bg-bar aria-[current]:bg-selection"
          >
            <SessionDot {session} />
            <span class="truncate">{session.title}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <h2 class="{heading} mt-auto border-t border-edge">{t('tags')} <span class="text-white">{tags.length}</span></h2>
  {#if tags.length === 0}
    <p class="px-3 pb-3 text-white">{t('no_tags')}</p>
  {:else}
    <ul aria-label={t('tags')} class="flex max-h-40 flex-wrap gap-1.5 overflow-y-auto px-3 pb-3">
      {#each tags as [name, count] (name)}
        <li>
          <button
            type="button"
            aria-pressed={tag === name}
            onclick={() => (tag = tag === name ? null : name)}
            style="color: {chip(name)}"
            class="cursor-pointer rounded-full border border-edge px-2 py-0.5 aria-pressed:border-current aria-pressed:bg-selection"
          >
            #{name} <span class="text-white">{count}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <!-- The vault, and how the window looks: streamer mode and the theme. Here, where there is room in every language. -->
  <div class="flex border-t border-edge">
    {#if onvault}
      <button
        type="button"
        onclick={onvault}
        class="flex min-w-0 flex-1 cursor-pointer justify-between gap-2 px-3 py-2 text-left text-cyan hover:bg-bar"
      >
        {t('vault')}
        {#if vault !== 'none'}
          <span class={vault === 'unlocked' ? 'text-green' : 'text-yellow'}>
            {vault === 'unlocked' ? t('vault_unlocked') : t('vault_locked')}
          </span>
        {/if}
      </button>
    {/if}
    <!-- An eye, struck through while the addresses on screen are made up. -->
    <button
      type="button"
      onclick={() => setStreamer(!streamer.on)}
      aria-pressed={streamer.on}
      aria-label={t('streamer_mode')}
      title={t('streamer_mode')}
      class="{toggle} ml-auto py-2 aria-pressed:text-magenta"
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" class="size-4 fill-none stroke-current stroke-2">
        <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z" />
        <circle cx="12" cy="12" r="3" />
        {#if streamer.on}
          <path d="M4 4l16 16" />
        {/if}
      </svg>
    </button>
    <!-- A moon for Night, a sun for Day. -->
    <button
      type="button"
      onclick={() => setTheme(theme.name === 'night' ? 'day' : 'night')}
      aria-label="{t('theme')}: {theme.name === 'night' ? t('theme_night') : t('theme_day')}"
      title={t('theme')}
      class="{toggle} py-2"
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" class="size-4 fill-none stroke-current stroke-2">
        {#if theme.name === 'night'}
          <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" />
        {:else}
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" />
        {/if}
      </svg>
    </button>
  </div>
</nav>
