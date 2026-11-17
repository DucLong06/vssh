<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { api, explain, type Shown, type VaultState } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'
  import { join } from './paths'

  type Props = {
    /** What the vault is; the dialog changes it by unlocking or making one. */
    vault: VaultState
    /** How many hosts the config has. */
    hosts: number
    /** It was opened to ask for the passphrase and for nothing else: unlocked, it closes. */
    once?: boolean
    onclose: () => void
  }
  let { vault = $bindable(), hosts, once = false, onclose }: Props = $props()

  /** The passphrase fields: the vault's own, a new one, and that again. They are emptied once sent. */
  let pass = $state('')
  let next = $state('')
  let again = $state('')
  /** The two new ones were not the same when last sent. */
  let mismatch = $state(false)
  /** The shell's refusal, and what went well; one of them at most. */
  let failed = $state<Shown | null>(null)
  let note = $state('')
  /** Passphrases refused since the dialog opened. Nobody is locked out: it is only said. */
  let wrong = $state(0)
  /** The hosts the vault has a password for. */
  let stored = $state<string[]>([])
  /** Where a backup goes. */
  let path = $state('')
  let first = $state<HTMLInputElement>()
  let root = $state<HTMLElement>()

  /** Does what was asked, unless the new passphrase was not typed the same twice. */
  async function attempt(twice: boolean, act: () => Promise<string>) {
    failed = null
    note = ''
    mismatch = twice && (vault === 'none' ? pass : next) !== again
    if (mismatch) return false
    try {
      note = await act()
      return true
    } catch (error) {
      failed = await explain(error, i18n.lang)
      return false
    }
  }

  async function unlock(e: SubmitEvent) {
    e.preventDefault()
    const opened = await attempt(vault === 'none', async () => {
      await api.vaultUnlock(pass)
      return ''
    })
    if (mismatch) return
    pass = again = ''
    if (opened) vault = 'unlocked'
    else wrong++
    if (opened && once) return onclose()
    // The field that had the focus is gone or emptied: the first one there is now gets it.
    await tick()
    root?.querySelector('input')?.focus()
  }

  async function change(e: SubmitEvent) {
    e.preventDefault()
    const changed = await attempt(true, async () => {
      await api.vaultPassChange(pass, next)
      return t('passphrase_changed')
    })
    if (changed) pass = next = again = ''
  }

  function save(e: SubmitEvent) {
    e.preventDefault()
    void attempt(false, async () => {
      await api.vaultExport(path)
      return t('exported').replace('{name}', path)
    })
  }

  // The dialog is open by now: the passphrase can be typed at once.
  onMount(() => first?.focus())

  // Once it is open: what it holds, and a place for a backup.
  $effect(() => {
    if (vault !== 'unlocked') return
    void api.vaultHosts().then((names) => (stored = names))
    void api.localList('').then((home) => (path ||= join(home.path, 'vssh-vault.backup')))
  })

  const field = 'mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg placeholder:text-dim aria-invalid:border-red'
  const button = 'cursor-pointer rounded-md border border-edge px-3 py-1.5 whitespace-nowrap hover:bg-selection'
  const go = `${button} border-blue bg-selection text-blue`
</script>

{#snippet secret(label: string, get: () => string, set: (value: string) => void, bad = false)}
  <label>
    {label}
    <input
      type="password"
      bind:value={get, set}
      required
      autocomplete="off"
      aria-invalid={bad || undefined}
      class={field}
    />
  </label>
{/snippet}

{#snippet said()}
  {#if mismatch}
    <p role="alert" class="text-red">{t('passphrase_mismatch')}</p>
  {:else if failed}
    <p role="alert" title={failed.chain} class="break-words">
      <strong class="text-red">✗ {failed.label}</strong>
      <span class="text-white">{failed.message}</span>
    </p>
  {:else if note}
    <p role="status" class="break-words text-green">{note}</p>
  {/if}
{/snippet}

<Modal label={t('vault')} {onclose}>
  <div bind:this={root} class="grid gap-3 p-4">
    <p class="flex flex-wrap gap-x-4">
      <span>
        <span class="text-cyan">{t('vault')}</span>
        {#if vault !== 'none'}
          <strong class={vault === 'unlocked' ? 'text-green' : 'text-yellow'}>
            {vault === 'unlocked' ? t('vault_unlocked') : t('vault_locked')}
          </strong>
        {/if}
      </span>
      {#if vault === 'unlocked'}
        <span class="text-white">{t('hosts')} <span class="text-fg">{hosts}</span></span>
        <span class="text-white">{t('passwords')} <span class="text-fg">{stored.length}</span></span>
      {/if}
    </p>

    {#if vault !== 'unlocked'}
      <form onsubmit={unlock} class="grid gap-3 text-white">
        {#if vault === 'none'}
          <p>{t('vault_none')}</p>
        {/if}
        <label>
          {t('passphrase')}
          <input bind:this={first} type="password" bind:value={pass} required autocomplete="off" class={field} />
        </label>
        {#if vault === 'none'}
          {@render secret(t('repeat_passphrase'), () => again, (value) => (again = value), mismatch)}
        {/if}
        {@render said()}
        {#if wrong >= 3}
          <p role="status" class="text-yellow">{t('wrong_attempts').replace('{n}', String(wrong))}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button type="button" onclick={onclose} class={button}>{t('cancel')}</button>
          <button type="submit" class={go}>{vault === 'none' ? t('create_vault') : t('unlock')}</button>
        </div>
      </form>
    {:else}
      <form onsubmit={change} class="grid grid-cols-3 items-end gap-2 border-t border-edge pt-3 text-white">
        {@render secret(t('old_passphrase'), () => pass, (value) => (pass = value))}
        {@render secret(t('new_passphrase'), () => next, (value) => (next = value))}
        {@render secret(t('repeat_passphrase'), () => again, (value) => (again = value), mismatch)}
        <button type="submit" class="{button} col-span-3 justify-self-end text-fg">{t('change_passphrase')}</button>
      </form>
      <form onsubmit={save} class="grid grid-cols-[1fr_auto] items-end gap-2 border-t border-edge pt-3 text-white">
        <p class="col-span-2">{t('export_note')}</p>
        <label>
          {t('path')}
          <input bind:value={path} required autocomplete="off" autocapitalize="off" spellcheck="false" class={field} />
        </label>
        <button type="submit" class="{button} text-fg">{t('export_backup')}</button>
      </form>
      {@render said()}
      <div class="flex justify-end">
        <button type="button" onclick={onclose} class={button}>{t('close')}</button>
      </div>
    {/if}
  </div>
</Modal>
