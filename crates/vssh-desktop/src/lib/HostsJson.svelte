<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Offered, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { addr } from './streamer.svelte'

  type Props = {
    /** The hosts out to a file, or in from one. */
    mode: 'export' | 'import'
    /** The aliases that were written. */
    onimported: (aliases: string[]) => void
    onclose: () => void
  }
  let { mode, onimported, onclose }: Props = $props()

  /** `~` is the home, which the shell reads: no user's name on screen. */
  let path = $state('~/vssh-hosts.json')
  /** What the file holds; `null` until it is read. */
  let found = $state<Offered[] | null>(null)
  /** Those of them to take. */
  let ticked = $state<string[]>([])
  /** Some of those are hosts the app has: asked about before anything is written. */
  let asking = $state(false)
  /** What an export did. */
  let said = $state('')
  let failed = $state<Shown | null>(null)
  let field = $state<HTMLInputElement>()

  const title = $derived(t(mode === 'export' ? 'export_hosts_json' : 'import_hosts_json'))
  const clash = $derived((found ?? []).filter((host) => host.taken && ticked.includes(host.alias)).map((host) => host.alias))

  async function attempt(act: () => Promise<void>) {
    failed = null
    try {
      await act()
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  }

  const save = () =>
    attempt(async () => {
      const n = await api.exportHostsJson(path.trim())
      said = t('exported_hosts').replace('{n}', String(n)).replace('{name}', path.trim())
    })

  const preview = () =>
    attempt(async () => {
      found = await api.importHostsJsonPreview(path.trim())
      ticked = found.map((host) => host.alias)
    })

  /** The file to read, chosen in the desktop's dialog, and read at once. */
  const choose = () =>
    attempt(async () => {
      const chosen = await api.pickFile(t('choose_file'))
      if (!chosen) return
      path = chosen
      found = null
      await preview()
    })

  const take = (overwrite: boolean) =>
    attempt(async () => onimported(await api.importHostsJson(path.trim(), ticked, overwrite)))

  onMount(() => field?.focus())

  const button = 'cursor-pointer rounded-ctl border border-edge min-h-9 px-3.5 py-1.5 font-medium whitespace-nowrap hover:bg-selection'
</script>

<Modal label={title} {onclose}>
  <div class="grid gap-4 p-7 text-sm text-white">
    <p class="text-base font-semibold text-ink">{title}</p>
    {#if mode === 'export'}
      <p>{t('export_hosts_note')}</p>
    {/if}
    <form
      class="grid {mode === 'import' ? 'grid-cols-[1fr_auto_auto]' : 'grid-cols-[1fr_auto]'} items-end gap-2"
      onsubmit={(e) => {
        e.preventDefault()
        void (mode === 'export' ? save() : preview())
      }}
    >
      <label>
        {t('path')}
        <!-- Another path is another file: what was read of the last one is not what would be taken. -->
        <input
          bind:this={field}
          bind:value={path}
          required
          oninput={() => {
            found = null
            asking = false
            said = ''
          }}
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          class="mt-1 w-full rounded-lg border border-edge bg-bg px-3 py-2 font-mono text-sm text-fg placeholder:text-dim"
        />
      </label>
      <!-- To read a file that is there: one to write is named, and must not be there yet. -->
      {#if mode === 'import'}
        <button
          type="button"
          aria-label={t('choose_file')}
          title={t('choose_file')}
          onclick={choose}
          class="grid size-10 cursor-pointer place-items-center rounded-lg border border-edge text-dim hover:bg-selection hover:text-fg"
        >
          <Icon name="folder" />
        </button>
      {/if}
      <button type="submit" class="{button} text-fg">{mode === 'export' ? t('export') : t('preview')}</button>
    </form>

    {#if failed}
      <p role="alert" title={failed.chain} class="break-words">
        <strong class="text-red">✗ {failed.label}</strong>
        <span>{failed.message}</span>
      </p>
    {:else if said}
      <p role="status" class="break-words text-green">{said}</p>
    {:else if found?.length === 0}
      <p>{t('import_empty')}</p>
    {:else if found}
      <ul aria-label={t('hosts')} onchange={() => (asking = false)} class="flex max-h-[40vh] flex-col overflow-y-auto">
        {#each found as host (host.alias)}
          <li>
            <label class="flex cursor-pointer items-baseline gap-2 rounded-lg px-2 py-1.5 hover:bg-selection">
              <input type="checkbox" bind:group={ticked} value={host.alias} class="accent-blue" />
              <span class="font-medium text-ink">{host.alias}</span>
              <span class="truncate font-mono text-xs">{addr(host)}</span>
              {#if host.taken}
                <span class="ml-auto shrink-0 text-yellow">{t('import_taken')}</span>
              {/if}
            </label>
          </li>
        {/each}
      </ul>
      {#if asking}
        <p role="alert" class="break-words text-yellow">
          {t('import_taken_ask').replace('{n}', String(clash.length)).replace('{name}', clash.join(', '))}
        </p>
      {/if}
    {/if}

    <!-- Cancel comes first; to write over a host is the one answer in red. -->
    <div class="flex justify-end gap-2">
      <button type="button" onclick={onclose} class={button}>{t('cancel')}</button>
      {#if asking}
        <button type="button" onclick={() => take(false)} class="{button} text-fg">{t('skip')}</button>
        <button type="button" onclick={() => take(true)} class="{button} border-red text-red">{t('overwrite')}</button>
      {:else if found?.length}
        <button
          type="button"
          onclick={() => (clash.length ? (asking = true) : take(false))}
          disabled={ticked.length === 0}
          class="{button} border-blue bg-blue! font-semibold text-page hover:brightness-110 disabled:opacity-60"
        >
          {t('import_n').replace('{n}', String(ticked.length))}
        </button>
      {/if}
    </div>
  </div>
</Modal>
