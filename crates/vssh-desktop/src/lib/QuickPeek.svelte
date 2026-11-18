<script lang="ts">
  import type { Host } from './api'
  import { i18n, t } from './i18n.svelte'
  import type { Live } from './live.svelte'
  import { capital, KEY, tip } from './keymap'
  import { parts, uptime } from './stats'
  import { addr, fake, streamer } from './streamer.svelte'

  // The TUI's quick peek, as a panel that slides out from under its card:
  // what the host said about itself the last time it was asked, and how long
  // ago that was.
  // `onrefresh`: asks the host again, as `r` does.
  type Props = { host: Host; info?: Live; onrefresh?: () => void }
  let { host, info, onrefresh }: Props = $props()

  let box: HTMLElement
  let now = $state(Date.now())

  $effect(() => {
    const clock = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(clock)
  })

  // Under a card of the last row it opens below the fold, and it grows
  // when the answer comes.
  $effect(() => {
    void [info?.peek, info?.shown]
    box.scrollIntoView({ block: 'nearest' })
  })

  const age = $derived(
    info?.at === undefined ? '' : t('secs_ago').replace('{n}', String(Math.max(0, Math.floor((now - info.at) / 1000)))),
  )
  // The TUI's line under a host's address.
  const reach = $derived(
    info?.ms === undefined
      ? { dot: '○', text: t('not_checked'), tone: 'text-dim' }
      : info.ms === null
        ? { dot: '●', text: t('unreachable'), tone: 'text-red' }
        : { dot: '●', text: `${t('reachable')} · ${info.ms === 0 ? '<1 ms' : `${info.ms} ms`}`, tone: 'text-green' },
  )
  const tone = { ok: 'text-green', warn: 'text-yellow', crit: 'text-red' }
  /** Label, value and colour of every row; a value the host did not give is `undefined`. */
  const rows = $derived.by(() => {
    const peek = info?.peek
    if (!peek) return []
    const list = parts(peek.stats, t('mem'))
    const figure = (label: string, as = label): [string, string?, string?] => {
      const part = list.find((part) => part.label === label)
      return [as, part?.value, part && tone[part.level]]
    }
    const text = (label: string, value: string | null): [string, string?, string?] => [label, value ?? undefined]
    return [
      // What the host calls itself is an address too.
      text(t('hostname'), peek.host && streamer.on ? fake(host.alias) : peek.host),
      text(t('system'), peek.os),
      text(t('uptime'), peek.uptime && uptime(peek.uptime, i18n.lang)),
      text(t('load'), peek.load),
      figure('CPU'),
      figure(t('mem')),
      figure('/', t('disk')),
    ]
  })
</script>

<!-- before: the gap under the card is the popover's too, so a pointer on its way to the button has not left the card. -->
<div
  bind:this={box}
  role="dialog"
  aria-label="{t('quick_peek')}: {host.alias}"
  class="absolute top-full right-0 left-0 z-10 mt-2 animate-slide cursor-default rounded-2xl border border-blue/40 bg-panel p-4 text-[13px] shadow-card before:absolute before:inset-x-0 before:-top-2 before:h-2 motion-reduce:animate-none"
>
  <p class="flex items-center gap-2">
    <strong class="truncate text-sm font-semibold text-ink">{t('quick_peek')}: {host.alias}</strong>
    <span class="ml-auto shrink-0 text-xs text-dim">{age}</span>
    {#if onrefresh}
      <button
        type="button"
        onclick={onrefresh}
        aria-keyshortcuts="r"
        title={tip(capital(info?.shown ? t('try_again') : t('refresh')), KEY.refresh)}
        class="shrink-0 cursor-pointer rounded-lg border border-edge px-2 py-0.5 text-xs font-medium text-fg first-letter:uppercase hover:bg-selection"
      >
        {info?.shown ? t('try_again') : t('refresh')}
      </button>
    {/if}
  </p>

  <div class="mt-3 rounded-xl bg-bg px-3 py-2">
    <h4 class="text-[11px] font-semibold tracking-widest text-dim">{t('connection')}</h4>
    {#if addr(host)}
      <p class="truncate font-mono text-xs">{addr(host)}</p>
    {/if}
    <p class={reach.tone}><span aria-hidden="true" class="text-[10px]">{reach.dot}</span> {reach.text}</p>
  </div>

  {#if info?.shown}
    <div class="mt-2 rounded-xl border border-dashed border-red/40 bg-bg px-3 py-2">
      <p class="break-words">
        <strong class="font-semibold text-red">✗ {info.shown.label}</strong>
        {info.shown.message}
      </p>
      <p class="pt-1 font-mono text-xs break-words text-white">{info.shown.chain}</p>
    </div>
  {:else if info?.peek}
    <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 px-3 pt-3">
      {#each rows as [label, value, colour] (label)}
        <dt class="text-dim">{label}</dt>
        <dd class="truncate {value === undefined ? 'text-dim' : (colour ?? '')}">{value ?? '—'}</dd>
      {/each}
    </dl>
  {:else if info?.peek === null && !info.asking}
    <p class="px-3 pt-3 text-white">{t('not_checked')}</p>
  {:else}
    <p class="px-3 pt-3 text-white">{t('asking')} {host.alias}…</p>
  {/if}
</div>
