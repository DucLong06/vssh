<script lang="ts">
  import { i18n, setLang, t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import { setStreamer, streamer } from './streamer.svelte'

  // The head of every view: what is in front, the way to the palette, and
  // how the window speaks and what it shows of an address.
  type Props = {
    title: string
    /** A line under the title: what the view holds. */
    sub?: string
    /** The title is an address or a host's name: letter by letter. */
    mono?: boolean
    onpalette: () => void
  }
  let { title, sub = '', mono = false, onpalette }: Props = $props()

  const half = 'rounded-md px-2.5 py-0.5 text-xs font-semibold'
</script>

<header class="flex items-center gap-3 bg-bg px-8 pt-5 pb-4">
  <div class="mr-auto min-w-0">
    <h1 class="truncate text-2xl leading-tight font-semibold tracking-tight text-ink {mono ? 'font-mono' : ''}">{title}</h1>
    {#if sub}
      <p class="truncate text-[13px] text-white">{sub}</p>
    {/if}
  </div>
  <!-- The concept's search: here it is the palette, which finds hosts and actions alike. -->
  <button
    type="button"
    onclick={onpalette}
    aria-label={t('palette')}
    aria-keyshortcuts="Control+K"
    class="flex h-10 w-72 min-w-0 shrink cursor-pointer items-center gap-2.5 rounded-full border border-edge bg-panel px-4 text-left text-dim hover:border-dim/50"
  >
    <Icon name="search" />
    <span class="min-w-0 flex-1 truncate">{t('search')}</span>
    <kbd>Ctrl-K</kbd>
  </button>
  <!-- One button, drawn as the pair it chooses between. -->
  <button
    type="button"
    onclick={() => setLang(i18n.lang === 'en' ? 'vi' : 'en')}
    aria-label="{t('language')}: {i18n.lang.toUpperCase()}"
    class="flex shrink-0 cursor-pointer rounded-[10px] border border-edge bg-page p-0.5 text-dim"
  >
    {#each ['en', 'vi'] as const as lang (lang)}
      <span class="{half} {i18n.lang === lang ? 'bg-selection text-ink' : ''}">{lang.toUpperCase()}</span>
    {/each}
  </button>
  <!-- An eye, struck through while the addresses on screen are made up. -->
  <button
    type="button"
    onclick={() => setStreamer(!streamer.on)}
    aria-pressed={streamer.on}
    aria-label={t('streamer_mode')}
    title={t('streamer_mode')}
    class="grid size-10 shrink-0 cursor-pointer place-items-center rounded-full border border-edge text-white hover:bg-selection hover:text-fg aria-pressed:border-magenta/60 aria-pressed:bg-selection aria-pressed:text-magenta"
  >
    <Icon name={streamer.on ? 'blind' : 'eye'} />
  </button>
</header>
