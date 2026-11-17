<script lang="ts">
  import type { Host } from './api'
  import { i18n, setLang, t } from './i18n.svelte'
  import { addr, setStreamer, streamer } from './streamer.svelte'
  import { setTheme, theme } from './theme.svelte'

  type Props = {
    hosts: number
    snippets: number
    sessions: number
    selected?: Host
    /** A terminal is in front: the keys are its keys. */
    terminal?: boolean
    /** A file view is in front. */
    files?: boolean
    /** Ctrl-A was pressed and the next key is a command. */
    leader?: boolean
    /** How many tabs are broadcasting. */
    broadcasting?: number
    onsnippets: () => void
    onpalette: () => void
    /** The session in front is a quick connect: keeps it as a host. */
    onsave?: () => void
  }
  let {
    hosts,
    snippets,
    sessions,
    selected,
    terminal = false,
    files = false,
    leader = false,
    broadcasting = 0,
    onsnippets,
    onpalette,
    onsave,
  }: Props = $props()

  const button = 'cursor-pointer rounded px-1.5 hover:bg-selection'
</script>

<!-- One line: what has no room is the selected host's address, which its card has too. -->
<footer class="col-span-2 flex items-center gap-4 border-t border-edge bg-bar px-3 py-1 whitespace-nowrap text-white">
  <span class="shrink-0">{t('hosts')} <span class="text-fg">{hosts}</span></span>
  <!-- The way to them for the pointer; `s` on the dashboard is the key. -->
  <button type="button" onclick={onsnippets} aria-keyshortcuts="s" class="{button} -mx-1.5 shrink-0">
    {t('snippets')} <span class="text-fg">{snippets}</span>
  </button>
  <span class="shrink-0">{t('sessions')} <span class="text-fg">{sessions}</span></span>
  {#if broadcasting}
    <strong class="shrink-0 text-yellow">{t('broadcasting').replace('{n}', String(broadcasting))}</strong>
  {/if}
  {#if terminal || leader}
    <!-- The TUI's leader menu. Lit while the next key is a command. -->
    <span role="status" class="truncate {leader ? 'text-yellow' : ''}">
      <kbd class="text-magenta">Ctrl-A</kbd>
      {t('leader_menu')}
    </span>
  {:else if files}
    <span role="status" class="truncate">
      <kbd class="text-magenta">Space</kbd>
      {t('select')} ·
      <kbd class="text-magenta">c</kbd>
      {t('copy')} ·
      <kbd class="text-magenta">/</kbd>
      {t('filter')} ·
      <kbd class="text-magenta">.</kbd>
      {t('hidden_files')} ·
      {t('right_click_more')}
    </span>
  {:else if selected}
    <span class="truncate"><span class="text-cyan">{selected.alias}</span> {addr(selected)}</span>
    <!-- The keys of the selected card: its Connect button shows only under the pointer or the focus. -->
    <span class="shrink-0">
      <kbd class="text-magenta">Enter</kbd>
      {t('connect')} ·
      <kbd class="text-magenta">Space</kbd>
      {t('select')} ·
      <kbd class="text-magenta">v</kbd>
      {t('quick_peek')} ·
      <kbd class="text-magenta">f</kbd>
      {t('files')} ·
      <kbd class="text-magenta">x</kbd>
      {t('run')} ·
      <kbd class="text-magenta">L</kbd>
      {t('forwards')} ·
      <kbd class="text-magenta">e</kbd>
      {t('edit')}
    </span>
  {/if}
  {#if onsave}
    <button type="button" onclick={onsave} class="{button} shrink-0 text-cyan">{t('save_this_host')}</button>
  {/if}
  <button type="button" onclick={onpalette} aria-label={t('palette')} class="{button} ml-auto shrink-0 text-magenta">
    Ctrl-K
  </button>
  <!-- An eye, struck through while the addresses on screen are made up. -->
  <button
    type="button"
    onclick={() => setStreamer(!streamer.on)}
    aria-pressed={streamer.on}
    aria-label={t('streamer_mode')}
    title={t('streamer_mode')}
    class="{button} shrink-0 aria-pressed:bg-selection aria-pressed:text-magenta"
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
    class="{button} shrink-0"
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
  <button
    type="button"
    onclick={() => setLang(i18n.lang === 'en' ? 'vi' : 'en')}
    aria-label="{t('language')}: {i18n.lang.toUpperCase()}"
    class="{button} shrink-0 text-fg"
  >
    {i18n.lang.toUpperCase()}
  </button>
</footer>
