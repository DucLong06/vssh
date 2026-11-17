<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Host, type Shown } from './api'
  import { address } from './hosts'
  import { i18n, t } from './i18n.svelte'

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

  const take = () => attempt(async () => onimported(await api.hostsImport(path.trim() || null, ticked)))

  onMount(() => {
    field?.focus()
    void preview()
  })

  const button = 'cursor-pointer rounded-md border border-edge px-3 py-1.5 whitespace-nowrap hover:bg-selection'
</script>

<div class="grid gap-3 p-4 text-white">
  <p class="text-cyan">{t('import_ssh_config')}</p>
  <form
    class="grid grid-cols-[1fr_auto] items-end gap-2"
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
        class="mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg placeholder:text-dim"
      />
    </label>
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
          <label class="flex cursor-pointer items-baseline gap-2 rounded px-2 py-1 hover:bg-selection/50">
            <input type="checkbox" bind:group={ticked} value={host.alias} class="accent-blue" />
            <span class="text-fg">{host.alias}</span>
            <span class="truncate">{address(host)}</span>
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
        class="{button} border-blue bg-selection text-blue disabled:opacity-60"
      >
        {t('import_n').replace('{n}', String(ticked.length))}
      </button>
    {/if}
  </div>
</div>
