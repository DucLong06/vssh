<script lang="ts">
  import type { Shown } from './api'
  import { t } from './i18n.svelte'
  import Modal from './Modal.svelte'
  import { streamer } from './streamer.svelte'

  /** `onconnect` rejects with the refusal, already in words. */
  type Props = { onconnect: (target: string) => Promise<void>; onclose: () => void }
  let { onconnect, onclose }: Props = $props()

  let host = $state('')
  let user = $state('')
  let port = $state<number | null>(null)
  let refusal = $state<Shown | null>(null)
  let busy = $state(false)

  /** `[user@]host[:port]`, the one line the TUI's quick connect takes. */
  function target() {
    const name = host.trim()
    // A bare IPv6 address needs its brackets before a port can follow.
    const bare = name.includes(':') && !name.startsWith('[') && !name.includes('@')
    const at = user.trim() ? `${user.trim()}@` : ''
    return port ? `${at}${bare ? `[${name}]` : name}:${port}` : `${at}${name}`
  }

  async function onsubmit(e: SubmitEvent) {
    e.preventDefault()
    if (!host.trim()) return
    busy = true
    refusal = null
    try {
      await onconnect(target())
    } catch (error) {
      refusal = error as Shown
    } finally {
      busy = false
    }
  }

  const field = 'mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg placeholder:text-dim'
</script>

<Modal label={t('quick_connect')} {onclose}>
  <form {onsubmit} class="grid gap-3 p-4">
    <p>
      <span class="text-cyan">{t('quick_connect')}</span>
      <span class="text-white">{t('quick_connect_note')}</span>
    </p>
    <div class="grid grid-cols-[2fr_1fr_5.5rem] gap-2 text-white">
      <label>
        {t('host')}
        <input bind:value={host} type={streamer.on ? 'password' : 'text'} required autocomplete="off" autocapitalize="off" spellcheck="false" class={field} />
      </label>
      <label>
        {t('user')}
        <input bind:value={user} autocomplete="off" autocapitalize="off" spellcheck="false" class={field} />
      </label>
      <label>
        {t('port')}
        <input bind:value={port} type="number" min="1" max="65535" placeholder="22" class={field} />
      </label>
    </div>
    {#if refusal}
      <p role="alert" class="text-red"><strong>{refusal.label}</strong> {refusal.message}</p>
    {/if}
    <div class="flex justify-end gap-2">
      <button type="button" onclick={onclose} class="cursor-pointer rounded-md border border-edge px-3 py-1.5 text-white">
        {t('cancel')}
      </button>
      <button
        type="submit"
        disabled={busy}
        class="cursor-pointer rounded-md border border-blue bg-selection px-3 py-1.5 text-blue disabled:opacity-60"
      >
        {t('connect')}
      </button>
    </div>
  </form>
</Modal>
