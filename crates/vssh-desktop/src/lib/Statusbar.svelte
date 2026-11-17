<script lang="ts">
  import type { Host } from './api'
  import { address } from './hosts'
  import { i18n, setLang, t } from './i18n.svelte'

  type Props = { hosts: number; snippets: number; sessions: number; selected?: Host; onpalette: () => void }
  let { hosts, snippets, sessions, selected, onpalette }: Props = $props()

  const button = 'cursor-pointer rounded px-1.5 hover:bg-selection'
</script>

<footer class="col-span-2 flex items-center gap-4 border-t border-edge bg-bar px-3 py-1 text-white">
  <span>{t('hosts')} <span class="text-fg">{hosts}</span></span>
  <span>{t('snippets')} <span class="text-fg">{snippets}</span></span>
  <span>{t('sessions')} <span class="text-fg">{sessions}</span></span>
  {#if selected}
    <span class="truncate"><span class="text-cyan">{selected.alias}</span> {address(selected)}</span>
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
