<script lang="ts">
  import type { Host } from './api'
  import { address } from './hosts'
  import { i18n, t } from './i18n.svelte'
  import type { Live } from './live.svelte'
  import { parts, uptime } from './stats'

  // The TUI's quick peek, as a popover under a card: what the host said
  // about itself the last time it was asked, and how long ago that was.
  type Props = { host: Host; info?: Live }
  let { host, info }: Props = $props()

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
      text(t('hostname'), peek.host),
      text(t('system'), peek.os),
      text(t('uptime'), peek.uptime && uptime(peek.uptime, i18n.lang)),
      text(t('load'), peek.load),
      figure('CPU'),
      figure(t('mem')),
      figure('/', t('disk')),
    ]
  })
</script>

<div
  bind:this={box}
  role="dialog"
  aria-label="{t('quick_peek')}: {host.alias}"
  class="absolute top-full right-0 left-0 z-10 mt-1 cursor-default rounded-xl border border-blue/50 bg-black p-3 shadow-2xl shadow-black"
>
  <p class="flex justify-between gap-2">
    <strong class="truncate text-magenta">{t('quick_peek')}: {host.alias}</strong>
    <span class="shrink-0 text-dim">{age}</span>
  </p>

  <h4 class="pt-2 text-xs text-cyan">{t('connection')}</h4>
  {#if address(host)}
    <p class="truncate">{address(host)}</p>
  {/if}
  <p class={reach.tone}><span aria-hidden="true">{reach.dot}</span> {reach.text}</p>

  {#if info?.shown}
    <p class="pt-2 break-words">
      <strong class="text-red">✗ {info.shown.label}</strong>
      {info.shown.message}
    </p>
    <p class="pt-1 text-xs break-words text-white">{info.shown.chain}</p>
  {:else if info?.peek}
    <dl class="grid grid-cols-[auto_1fr] gap-x-4 pt-2">
      {#each rows as [label, value, colour] (label)}
        <dt class="text-white">{label}</dt>
        <dd class="truncate {value === undefined ? 'text-dim' : (colour ?? '')}">{value ?? '—'}</dd>
      {/each}
    </dl>
  {:else if info?.peek === null && !info.asking}
    <p class="pt-2 text-white">{t('not_checked')}</p>
  {:else}
    <p class="pt-2 text-white">{t('asking')} {host.alias}…</p>
  {/if}

  <p class="pt-2 text-white">
    <kbd class="text-magenta">r</kbd>
    {info?.shown ? t('try_again') : t('refresh')} ·
    <kbd class="text-magenta">esc</kbd>
    {t('close')}
  </p>
</div>
