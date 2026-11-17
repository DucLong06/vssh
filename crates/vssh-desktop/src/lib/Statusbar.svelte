<script lang="ts">
  import type { Host } from './api'
  import { address } from './hosts'
  import { i18n, setLang, t } from './i18n.svelte'

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
  }: Props = $props()

  const button = 'cursor-pointer rounded px-1.5 hover:bg-selection'
</script>

<footer class="col-span-2 flex items-center gap-4 border-t border-edge bg-bar px-3 py-1 text-white">
  <span>{t('hosts')} <span class="text-fg">{hosts}</span></span>
  <!-- The way to them for the pointer; `s` on the dashboard is the key. -->
  <button type="button" onclick={onsnippets} aria-keyshortcuts="s" class="{button} -mx-1.5">
    {t('snippets')} <span class="text-fg">{snippets}</span>
  </button>
  <span>{t('sessions')} <span class="text-fg">{sessions}</span></span>
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
    <span class="truncate"><span class="text-cyan">{selected.alias}</span> {address(selected)}</span>
    <!-- The keys of the selected card: its Connect button shows only under the pointer or the focus. -->
    <span class="shrink-0">
      <kbd class="text-magenta">Enter</kbd>
      {t('connect')} ·
      <kbd class="text-magenta">Space</kbd>
      {t('select')} ·
      <kbd class="text-magenta">v</kbd>
      {t('quick_peek')} ·
      <kbd class="text-magenta">f</kbd>
      {t('files')}
    </span>
  {/if}
  <button type="button" onclick={onpalette} aria-label={t('palette')} class="{button} ml-auto text-magenta">
    Ctrl-K
  </button>
  <button
    type="button"
    onclick={() => setLang(i18n.lang === 'en' ? 'vi' : 'en')}
    aria-label="{t('language')}: {i18n.lang.toUpperCase()}"
    class="{button} text-fg"
  >
    {i18n.lang.toUpperCase()}
  </button>
</footer>
