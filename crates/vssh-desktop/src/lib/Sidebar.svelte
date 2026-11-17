<script lang="ts">
  import type { Host, Shown } from './api'
  import { address, hasTag, tagCounts } from './hosts'
  import { t } from './i18n.svelte'

  type Props = {
    hosts: Host[]
    selected: string | null
    onselect: (alias: string) => void
    /** Why there are no hosts to show, if they could not be read. */
    error?: Shown | null
  }
  let { hosts, selected, onselect, error = null }: Props = $props()

  let tag = $state<string | null>(null)
  let list = $state<HTMLElement>()

  const tags = $derived(tagCounts(hosts))
  const shown = $derived(hosts.filter((host) => tag === null || hasTag(host, tag)))

  // A host chosen elsewhere (the palette) may be below the fold.
  $effect(() => {
    if (selected) list?.querySelector('[aria-current]')?.scrollIntoView({ block: 'nearest' })
  })

  const heading = 'flex justify-between px-3 pt-3 pb-1 text-cyan'
</script>

<nav aria-label={t('hosts')} class="flex min-h-0 flex-col border-r border-edge bg-black">
  <h2 class={heading}>{t('hosts')} <span class="text-white">{shown.length}</span></h2>
  {#if error}
    <p role="alert" class="px-3 break-words text-red"><strong>{error.label}</strong> {error.message}</p>
  {:else if hosts.length === 0}
    <p class="px-3">{t('no_hosts')}</p>
    <p class="px-3 pt-1 text-white">{t('no_hosts_hint')}</p>
  {:else}
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
            {#if address(host)}
              <span class="block truncate text-white">{address(host)}</span>
            {/if}
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
            class="cursor-pointer rounded-full border border-edge px-2 py-0.5 text-magenta aria-pressed:border-magenta aria-pressed:bg-selection"
          >
            #{name} <span class="text-white">{count}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</nav>
