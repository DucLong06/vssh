<script lang="ts">
  import { api, type Host } from './api'
  import { t } from './i18n.svelte'
  import Keys from './Keys.svelte'
  import { KEY } from './keymap'
  import { addr } from './streamer.svelte'
  import { update } from './update.svelte'

  type Props = {
    hosts: number
    snippets: number
    sessions: number
    selected?: Host
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
    leader = false,
    broadcasting = 0,
    onsnippets,
    onsave,
  }: Props = $props()

  const button = 'cursor-pointer rounded-md px-1.5 hover:bg-selection hover:text-fg'
</script>

<!-- One line of figures. No keys: a button says its own in its tooltip, and the cheatsheet has them all. -->
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
  {#if selected}
    <span class="truncate"><span class="font-semibold text-ink">{selected.alias}</span> <span class="font-mono text-xs">{addr(selected)}</span></span>
  {/if}
  {#if leader}
    <!-- Half a chord: said until its second key comes, and where the rest of them are. -->
    <span role="status" class="truncate text-yellow">
      <Keys keys={KEY.leader[0]} />
      {t('leader_waiting')}
    </span>
  {/if}
  {#if update.tag}
    <!-- A newer version is out: one line, where it covers nothing. It opens the releases in the browser and fetches nothing itself; the × is for as long as the app runs. -->
    <span role="status" class="ml-auto flex shrink-0 items-center text-cyan">
      <button type="button" onclick={() => void api.openReleases().catch(() => {})} class={button}>
        {t('update_available').replace('{name}', update.tag)}
      </button>
      <button type="button" aria-label={t('dismiss')} title={t('dismiss')} onclick={() => (update.tag = null)} class={button}>×</button>
    </span>
  {/if}
  {#if onsave}
    <button type="button" onclick={onsave} class="{button} {update.tag ? '' : 'ml-auto'} shrink-0 text-cyan">{t('save_this_host')}</button>
  {/if}
</footer>
