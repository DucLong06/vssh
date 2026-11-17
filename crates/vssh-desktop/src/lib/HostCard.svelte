<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements'
  import type { Host } from './api'
  import { address, chip, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import { critical, down, type Live } from './live.svelte'
  import { compact, parts } from './stats'

  type Props = { host: Host; state?: Live; onconnect?: () => void } & HTMLAttributes<HTMLElement>
  let { host, state, onconnect, class: extra = '', ...rest }: Props = $props()

  const list = $derived(state?.peek ? parts(state.peek.stats, t('mem')) : [])
  const kind = $derived(
    down(state) ? 'down' : critical(state) ? 'crit' : state?.peek ? 'ok' : state?.peek === null ? 'unchecked' : 'asking',
  )
  // The TUI's `latency`.
  const latency = $derived(
    state?.ms === undefined ? '—' : state.ms === null ? t('down') : state.ms === 0 ? '<1 ms' : `${state.ms} ms`,
  )
  const reached = $derived(typeof state?.ms === 'number')
  const tone = { ok: 'text-green', warn: 'text-yellow', crit: 'text-red' }
</script>

<article
  {...rest}
  aria-label={host.alias}
  data-state={kind}
  class="group relative h-full rounded-2xl border border-edge bg-panel p-3 data-[state=crit]:border-red data-[state=down]:border-red/50 {extra}"
>
  {#if onconnect}
    <!-- Over the address, and only when the card is pointed at or has the
         focus: twenty of these at once would be all one sees. -->
    <button
      type="button"
      onclick={onconnect}
      class="absolute top-8 right-3 z-1 cursor-pointer rounded-md border border-edge bg-panel px-2 text-magenta opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-selection focus-visible:opacity-100"
    >
      {t('connect')}
    </button>
  {/if}
  <!-- An offline card dims, but not the line that says why. -->
  <div class={kind === 'down' ? 'opacity-55' : ''}>
    <header class="flex items-center gap-1.5">
      <span
        aria-hidden="true"
        class={kind === 'down' || kind === 'crit' ? 'text-red' : kind === 'ok' || reached ? 'text-green' : 'text-dim'}
      >
        {kind === 'asking' || (kind === 'unchecked' && !reached) ? '○' : '●'}
      </span>
      <h3 class="truncate font-bold">{host.alias}</h3>
      {#if host.meta.pinned}
        <span role="img" aria-label={t('pinned')} class="text-yellow">★</span>
      {/if}
      <span
        class="ml-auto shrink-0 rounded-full bg-bar px-2 {state?.ms === null ? 'text-red' : state?.ms === undefined ? 'text-dim' : 'text-green'}"
      >
        {latency}
      </span>
    </header>
    {#if address(host)}
      <p class="truncate pl-4 text-white">{address(host)}</p>
    {/if}
    {#if tagsOf(host).length}
      <ul aria-label={t('tags')} class="flex flex-wrap gap-1 pt-2 pl-4">
        {#each tagsOf(host) as tag (tag)}
          <li
            class="rounded-full border px-2 text-xs leading-5"
            style="color: {chip(tag)}; border-color: color-mix(in srgb, {chip(tag)} 40%, transparent); background: color-mix(in srgb, {chip(tag)} 12%, transparent)"
          >{tag}</li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="pt-3">
    {#if state?.shown}
      <p title={state.shown.chain} class="line-clamp-2 break-words">
        <strong class="text-red">✗ {state.shown.label}</strong>
        <span class="text-white">{state.shown.message}</span>
      </p>
    {:else if state?.peek}
      <div role="group" aria-label={compact(list)} class="grid grid-cols-3 gap-3">
        {#each ['CPU', t('mem'), '/'] as label (label)}
          {@const part = list.find((part) => part.label === label)}
          <div>
            <p class="flex justify-between gap-1 text-xs">
              <span class="text-white">{label}</span>
              <span class={part ? tone[part.level] : 'text-dim'}>{part?.value ?? '—'}</span>
            </p>
            <!-- The track is always there, so a missing figure leaves no hole. -->
            <div
              role={part ? 'meter' : undefined}
              aria-label={part ? label : undefined}
              aria-valuenow={part?.pct}
              aria-valuemin={part ? 0 : undefined}
              aria-valuemax={part ? 100 : undefined}
              data-level={part?.level}
              class="mt-1 h-2 overflow-hidden rounded-full bg-bar"
            >
              {#if part}
                <!-- Green up to the warning mark, yellow at the critical one, red at the end:
                     the gradient spans the track, the bar shows as much of it as it is long. -->
                <div
                  class="h-full rounded-full"
                  style="width: {part.pct}%; background: linear-gradient(to right, var(--color-green) {part.warn}%, var(--color-yellow) {part.crit}%, var(--color-red)) 0 / {part.pct ? 10000 / part.pct : 100}% 100%"
                ></div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {:else if state?.peek === null}
      <p class="text-white">{t('not_checked')}</p>
    {:else}
      <p class="text-white">{t('asking')} {host.alias}…</p>
    {/if}
  </div>
</article>
