<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements'
  import type { Host } from './api'
  import { tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import { critical, down, type Live, type Trail } from './live.svelte'
  import { KEY, tip } from './keymap'
  import Sparkline from './Sparkline.svelte'
  import { compact, parts } from './stats'
  import { addr } from './streamer.svelte'

  /** `info`: what is known about the host. (Not `state`: next to a rune that name reads as a store.) */
  type Props = {
    host: Host
    info?: Live
    /** One of several chosen to connect to together. */
    marked?: boolean
    onconnect?: () => void
    /** Opens the host's files. */
    onfiles?: () => void
    /** Opens the host's port forwards. */
    onforwards?: () => void
    /** Opens the host's form. */
    onedit?: () => void
    /** Asks a host that is down once more. */
    onretry?: () => void
    /** Opens its quick peek, which asks a host nobody asked yet. */
    onpeek?: () => void
    /** Watches the host, or stops: its switch. */
    onwatch?: (watched: boolean) => void
    /** No host is watched: this one neither, whatever its own switch says. */
    paused?: boolean
    /** Its last rounds, for the sparklines; without them the figure of now is the one point there is. */
    trail?: Trail
  } & HTMLAttributes<HTMLElement>
  let {
    host,
    info,
    marked = false,
    onconnect,
    onfiles,
    onforwards,
    onedit,
    onretry,
    onpeek,
    onwatch,
    paused = false,
    trail,
    class: extra = '',
    ...rest
  }: Props = $props()

  const list = $derived(info?.peek ? parts(info.peek.stats, t('mem')) : [])
  /** Nobody asks it on the dashboard's own: what the card shows is what was last heard, and it says so. */
  const stopped = $derived(paused || host.meta.paused)
  const kind = $derived(
    down(info) ? 'down' : critical(info) ? 'crit' : info?.peek ? 'ok' : info?.peek === null || stopped ? 'unchecked' : 'asking',
  )
  // The TUI's `latency`.
  const latency = $derived(
    stopped ? t('watch_stopped') : info?.ms === undefined ? '—' : info.ms === null ? t('down') : info.ms === 0 ? '<1 ms' : `${info.ms} ms`,
  )
  const reached = $derived(typeof info?.ms === 'number')
  /** A host that needs no looking at is three short lines: its name, where it is and its tags, and its three figures. One that is down or past a mark keeps the room to say why. */
  const low = $derived(kind !== 'down' && kind !== 'crit')
  const [green, yellow, red] = ['green', 'yellow', 'red'].map((name) => `var(--color-${name})`)
  const fill = {
    ok: green,
    warn: `linear-gradient(to right, ${green}, ${yellow})`,
    crit: `linear-gradient(to right, ${green}, ${yellow}, ${red})`,
  }
  // A healthy figure is the concept's white; past a mark it takes the mark's colour.
  const tone = { ok: 'text-ink', warn: 'text-yellow', crit: 'text-red' }
  const ghost =
    'min-h-9 cursor-pointer rounded-ctl border border-edge px-3 py-1 text-sm font-medium whitespace-nowrap hover:border-dim/50 hover:bg-selection first-letter:uppercase'
  const mini = 'cursor-pointer rounded-lg px-2 py-0.5 text-xs font-medium text-cyan hover:bg-selection'
</script>

<!-- A figure's bar: the track is always there, so a missing figure leaves no hole. -->
{#snippet meter(label: string, part: (typeof list)[number] | undefined, shape: string)}
  <div
    role={part ? 'meter' : undefined}
    aria-label={part ? label : undefined}
    aria-valuenow={part?.pct}
    aria-valuemin={part ? 0 : undefined}
    aria-valuemax={part ? 100 : undefined}
    data-level={part?.level}
    class="overflow-hidden bg-bar {shape}"
  >
    {#if part}
      <!-- Green, into yellow past the warning mark, into red past the critical one. -->
      <div class="h-full rounded-full" style="width: {part.pct}%; background: {fill[part.level]}"></div>
    {/if}
  </div>
{/snippet}

{#snippet tags()}
  {#if tagsOf(host).length}
    <ul aria-label={t('tags')} class="flex gap-1.5 {low ? 'overflow-hidden' : 'flex-wrap'}">
      {#each tagsOf(host) as tag (tag)}
        <li class="shrink-0 rounded-md bg-bar px-1.5 text-2xs leading-5 text-dim">{tag}</li>
      {/each}
    </ul>
  {/if}
{/snippet}

<!-- Over the tags of a tall card, and only when it is pointed at or has the focus: twenty of these at once would be all one sees. -->
{#snippet tools()}
  {#if onfiles || onforwards || onedit}
    <div class="absolute right-4.5 z-1 flex rounded-ctl border border-edge bg-panel p-0.5 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
      {#if onedit}
        <button type="button" onclick={onedit} aria-keyshortcuts="e" title={tip(t('edit'), KEY.edit)} class={mini}>{t('edit')}</button>
      {/if}
      {#if onforwards}
        <button type="button" onclick={onforwards} aria-keyshortcuts="l" title={tip(t('forwards'), KEY.forwards)} class={mini}>{t('forwards')}</button>
      {/if}
      {#if onfiles}
        <button type="button" onclick={onfiles} aria-keyshortcuts="f" title={tip(t('files'), KEY.files)} class={mini}>{t('files')}</button>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet connect()}
  {#if onconnect}
    <button type="button" onclick={onconnect} title={tip(t('connect'), KEY.connect)} class="{ghost} shrink-0 text-fg">{t('connect')}</button>
  {/if}
{/snippet}

<article
  {...rest}
  aria-label={host.alias}
  data-state={kind}
  data-marked={marked || undefined}
  data-low={low || undefined}
  class="group relative flex h-full flex-col rounded-2xl border border-edge bg-panel transition-colors {low ? 'gap-1.5 px-4.5 py-3' : 'gap-3 p-4.5'} select-none hover:border-dim/60 focus-visible:outline-[1.5px] focus-visible:-outline-offset-1 motion-reduce:transition-none data-[state=crit]:border-red/45 data-[state=down]:border-red/30 aria-[current]:outline-[1.5px] aria-[current]:-outline-offset-1 aria-[current]:outline-blue data-[marked]:border-magenta {extra}"
>
  <header class="flex items-center gap-2">
    <h3 class="truncate text-base font-semibold tracking-tight text-ink {low ? 'min-w-0' : 'shrink-0'}">{host.alias}</h3>
    {#if marked}
      <span role="img" aria-label={t('selected')} class="text-magenta">✓</span>
    {/if}
    {#if host.meta.pinned}
      <span role="img" aria-label={t('pinned')} class="text-yellow">★</span>
    {/if}
    <!-- The concept's badge, in the TUI's words: how long the host takes to answer, or that it does not. -->
    {#if onwatch}
      <!-- Its own switch, on until someone turns it off: off, the dashboard asks the host for nothing and keeps no connection to it. -->
      <!-- Drawn 24 px; what answers the pointer is 36 (the ::before). -->
      <button
        type="button"
        role="switch"
        aria-checked={!host.meta.paused}
        aria-label={t('watch')}
        title={t('watch')}
        onclick={() => onwatch(host.meta.paused)}
        class="relative ml-auto grid size-6 shrink-0 cursor-pointer place-items-center rounded-md before:absolute before:-inset-1.5 hover:bg-selection hover:text-fg {host.meta.paused ? 'text-yellow' : 'text-dim'}"
      >
        <Icon name={host.meta.paused ? 'blind' : 'eye'} class="size-4" />
      </button>
    {/if}
    <span
      data-stopped={stopped || undefined}
      class="{onwatch ? '' : 'ml-auto'} inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full px-2.5 text-xs font-semibold whitespace-nowrap {stopped || info?.ms === undefined ? 'bg-bar text-dim' : info.ms === null ? 'text-red inset-ring inset-ring-red/50' : kind === 'down' ? 'bg-bar text-white' : 'bg-green/12 text-green'}"
    >
      <!-- Whether the host answers, and nothing else: one that is overloaded
           says so in its figures and its border. A ring until it is known. -->
      <i
        aria-hidden="true"
        class="size-1.5 rounded-full {kind === 'asking' || (kind === 'unchecked' && !reached) ? 'ring-1 ring-current' : 'bg-current'} {kind === 'down' ? 'text-red' : info?.peek || reached ? 'text-green' : 'text-dim'}"
      ></i>
      {latency}
    </span>
    {#if low}{@render connect()}{/if}
  </header>
  {#if !low}
    <div class="-mt-1 flex min-h-6 items-start">
      {@render tags()}
      {@render tools()}
    </div>
  {:else if addr(host) || tagsOf(host).length}
    <!-- Who it is, under its name: user@host, and its tags as far as there is room. No buttons over a low card:
         they would lie on its name, and where it is held to be moved. The right button's menu and the keys have them. -->
    <div class="-mt-1 flex h-5 min-w-0 items-center gap-2">
      {#if addr(host)}
        <p title={addr(host)} class="max-w-[62%] shrink-0 truncate font-mono text-xs text-white">{addr(host)}</p>
      {/if}
      <div class="flex min-w-0 flex-1 overflow-hidden">{@render tags()}</div>
    </div>
  {/if}

  {#if info?.shown}
    <!-- The concept's box for a host that is down: which kind of failure, and what was said. -->
    <p
      title={info.shown.chain}
      class="flex flex-1 flex-col justify-center gap-1 rounded-xl border border-dashed border-red/40 bg-bg px-4 py-3 break-words"
    >
      <strong class="font-semibold text-red">✗ {info.shown.label}</strong>
      <span class="line-clamp-3 text-sm text-white">{info.shown.message}</span>
    </p>
  {:else if info?.peek && low}
    <!-- Three figures on one line, each with its bar: enough to see that nothing is wrong. -->
    <div role="group" aria-label={compact(list)} class="grid grid-cols-3 gap-3 text-xs">
      {#each ['CPU', t('mem'), '/'] as label (label)}
        {@const part = list.find((part) => part.label === label)}
        <div class="flex min-w-0 items-center gap-1.5">
          <span class="text-dim">{label}</span>
          {@render meter(label, part, 'h-1.5 min-w-3 flex-1 rounded-full')}
          <span class="truncate font-semibold {part ? tone[part.level] : 'text-dim'}">{part?.value ?? '—'}</span>
        </div>
      {/each}
    </div>
  {:else if info?.peek}
    <div role="group" aria-label={compact(list)} class="grid grid-cols-2 gap-x-3 gap-y-2.5">
      {#each ['CPU', t('mem'), '/'] as label (label)}
        {@const part = list.find((part) => part.label === label)}
        {#if label === '/'}
          <div class="col-span-2 flex items-center gap-3 text-xs">
            <span class="text-dim">{label}</span>
            <span class="order-last min-w-8 text-right font-semibold {part ? tone[part.level] : 'text-dim'}">
              {part?.value ?? '—'}
            </span>
            {@render meter(label, part, 'h-1.5 flex-1 rounded-full')}
          </div>
        {:else}
          {@const past = label === 'CPU' ? trail?.cpu : trail?.mem}
          <!-- A well in the card: the figure, the minute behind it, and its bar for a floor. -->
          <div class="overflow-hidden rounded-xl bg-bg">
            <p class="flex flex-wrap items-baseline justify-between gap-x-1 px-3 pt-2 text-xs text-dim">
              <span>{label}</span>
              <span class="text-md font-semibold tracking-tight {part ? tone[part.level] : 'text-dim'}">
                {part?.value ?? '—'}
              </span>
            </p>
            <Sparkline
              points={past?.length ? past : part ? [part.pct] : []}
              class="mt-0.5 w-full {label === 'CPU' ? 'text-blue' : 'text-teal'}"
            />
            {@render meter(label, part, 'h-0.75')}
          </div>
        {/if}
      {/each}
    </div>
  {:else if info?.peek === null || stopped}
    <p class="text-sm text-white {low ? 'leading-5' : 'flex-1 rounded-xl bg-bg px-3 py-2.5'}">
      {t('not_checked')}
      {#if onpeek}
        <button type="button" onclick={onpeek} aria-keyshortcuts="v" title={tip(t('quick_peek'), KEY.peek)} class="{mini} ml-1">
          {t('quick_peek')}
        </button>
      {/if}
    </p>
  {:else}
    <p class="text-sm text-white {low ? 'leading-5' : 'flex-1 rounded-xl bg-bg px-3 py-2.5'}">{t('asking')} {host.alias}…</p>
  {/if}

  {#if !low}
    <!-- The foot of the card. No <footer>: the window has one, the status bar. -->
    <div class="mt-auto flex min-h-7 items-center gap-2">
      {#if addr(host)}
        <p class="min-w-0 flex-1 truncate font-mono text-xs text-white">{addr(host)}</p>
      {/if}
      <span class="ml-auto flex shrink-0 gap-1.5">
        {#if onretry && kind === 'down'}
          <button type="button" onclick={onretry} class={ghost}>{t('try_again')}</button>
        {/if}
        {@render connect()}
      </span>
    </div>
  {/if}
</article>
