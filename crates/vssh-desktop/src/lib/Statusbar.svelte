<script lang="ts">
  import type { Host } from './api'
  import { t } from './i18n.svelte'
  import { addr } from './streamer.svelte'

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
    onsave,
  }: Props = $props()

  const button = 'cursor-pointer rounded-md px-1.5 hover:bg-selection hover:text-fg'
</script>

<!-- One line: what has no room is the selected host's address, which its card has too. -->
<footer class="col-span-2 flex h-9 items-center gap-4 border-t border-edge bg-black px-5 text-[12.5px] whitespace-nowrap text-white">
  <span class="shrink-0">{t('hosts')} <span class="font-semibold text-ink">{hosts}</span></span>
  <!-- The way to them for the pointer; `s` on the dashboard is the key. -->
  <button type="button" onclick={onsnippets} aria-keyshortcuts="s" class="{button} -mx-1.5 shrink-0">
    {t('snippets')} <span class="font-semibold text-ink">{snippets}</span>
  </button>
  <span class="shrink-0">{t('sessions')} <span class="font-semibold text-ink">{sessions}</span></span>
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
    <span class="truncate"><span class="font-semibold text-ink">{selected.alias}</span> <span class="font-mono text-xs">{addr(selected)}</span></span>
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
    <button type="button" onclick={onsave} class="{button} ml-auto shrink-0 text-cyan">{t('save_this_host')}</button>
  {/if}
</footer>
