<script lang="ts">
  import type { Host, Shown } from './api'
  import HostCard from './HostCard.svelte'
  import { t } from './i18n.svelte'
  import { live } from './live.svelte'

  type Props = {
    hosts: Host[]
    /** Why there are no hosts to show, if they could not be read. */
    error?: Shown | null
    onconnect: (alias: string) => void
    onpalette: () => void
  }
  let { hosts, error = null, onconnect, onpalette }: Props = $props()

  function onkeydown(e: KeyboardEvent, alias: string) {
    // Not from the card's own button: there Enter is a click already.
    if (e.target !== e.currentTarget) return
    if (e.key === 'Enter') onconnect(alias)
  }
</script>

<section aria-label={t('hosts')} class="min-h-0 flex-1 overflow-y-auto p-4">
  {#if error}
    <p role="alert" class="break-words text-red"><strong>{error.label}</strong> {error.message}</p>
  {/if}
  {#if hosts.length === 0 && !error}
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
  {:else}
    <ul class="grid grid-cols-[repeat(auto-fill,minmax(17rem,1fr))] gap-3">
      {#each hosts as host (host.alias)}
        <li>
          <HostCard
            {host}
            state={live[host.alias]}
            onconnect={() => onconnect(host.alias)}
            tabindex={0}
            onkeydown={(e) => onkeydown(e, host.alias)}
          />
        </li>
      {/each}
    </ul>
  {/if}
</section>
