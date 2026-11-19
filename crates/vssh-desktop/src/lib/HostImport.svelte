<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Host, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import { addr } from './streamer.svelte'

  type Props = {
    /** The aliases that were taken over. */
    onimported: (aliases: string[]) => void
    oncancel: () => void
  }
  let { onimported, oncancel }: Props = $props()

  /** The file to read; empty is `~/.ssh/config`. */
  let path = $state('')
  /** What it has that the app has not; `null` until it is read. */
  let found = $state<Host[] | null>(null)
  /** Those of them to take. */
  let ticked = $state<string[]>([])
  let failed = $state<Shown | null>(null)
  let field = $state<HTMLInputElement>()

  async function attempt(act: () => Promise<void>) {
    failed = null
    try {
      await act()
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  }

  const preview = () =>
    attempt(async () => {
      found = null
      found = await api.hostsImportPreview(path.trim() || null)
      ticked = found.map((host) => host.alias)
    })

  /** Another config, chosen in the desktop's dialog, and read at once. */
  const choose = () =>
    attempt(async () => {
      const chosen = await api.pickFile(t('choose_ssh_config'))
      if (!chosen) return
      path = chosen
      await preview()
    })

  const take = () => attempt(async () => onimported(await api.hostsImport(path.trim() || null, ticked)))

  onMount(() => {
    field?.focus()
    void preview()
  })

  const button = 'cursor-pointer rounded-[10px] border border-edge px-3.5 py-1.5 font-medium whitespace-nowrap hover:bg-selection'
</script>

<div class="grid gap-4 p-6 text-[13px] text-white">
  <p class="text-base font-semibold text-ink">{t('import_ssh_config')}</p>
  <form
    class="grid grid-cols-[1fr_auto_auto] items-end gap-2"
    onsubmit={(e) => {
      e.preventDefault()
      void preview()
    }}
  >
    <label>
      {t('path')}
      <input
        bind:this={field}
        bind:value={path}
        placeholder="~/.ssh/config"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        class="mt-1 w-full rounded-lg border border-edge bg-bg px-3 py-2 font-mono text-[13px] text-fg placeholder:text-dim"
      />
    </label>
    <button
      type="button"
      aria-label={t('choose_ssh_config')}
      title={t('choose_ssh_config')}
      onclick={choose}
      class="grid size-[38px] cursor-pointer place-items-center rounded-lg border border-edge text-dim hover:bg-selection hover:text-fg"
    >
      <Icon name="folder" />
    </button>
    <button type="submit" class="{button} text-fg">{t('preview')}</button>
  </form>

  {#if failed}
    <p role="alert" title={failed.chain} class="break-words">
      <strong class="text-red">✗ {failed.label}</strong>
      <span>{failed.message}</span>
    </p>
  {:else if found?.length === 0}
    <p>{t('import_none')}</p>
  {:else if found}
    <ul aria-label={t('hosts')} class="flex max-h-[40vh] flex-col overflow-y-auto">
      {#each found as host (host.alias)}
        <li>
          <label class="flex cursor-pointer items-baseline gap-2 rounded-lg px-2 py-1.5 hover:bg-selection">
            <input type="checkbox" bind:group={ticked} value={host.alias} class="accent-blue" />
            <span class="font-medium text-ink">{host.alias}</span>
            <span class="truncate font-mono text-xs">{addr(host)}</span>
          </label>
        </li>
      {/each}
    </ul>
    <p>{t('import_note')}</p>
  {/if}

  <div class="flex justify-end gap-2">
    <button type="button" onclick={oncancel} class={button}>{t('cancel')}</button>
    {#if found?.length}
      <button
        type="button"
        onclick={take}
        disabled={ticked.length === 0}
        class="{button} border-blue bg-blue! font-semibold text-page hover:brightness-110 disabled:opacity-60"
      >
        {t('import_n').replace('{n}', String(ticked.length))}
      </button>
    {/if}
  </div>
</div>
