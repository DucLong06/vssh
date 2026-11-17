<script lang="ts">
  import type { ConnError, Fail, Shown } from './api'
  import { t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type HostKey = Extract<Fail, { kind: 'host_key' }>
  type Props = {
    /** The session that could not connect, and why: as data, and in words. */
    title: string
    error: ConnError
    why: Shown
    /** A yes to the key that was presented: for this connection, or for good. */
    ontrust: (key: HostKey, forever: boolean) => void
    onretry: () => void
    onclose: () => void
  }
  let { title, error, why, ontrust, onretry, onclose }: Props = $props()

  /** The key that does not match, the host's own or a jump host's. */
  const key = $derived.by(() => {
    const fail = error.fail?.kind === 'jump' ? error.fail.cause : error.fail
    return fail?.kind === 'host_key' ? fail : null
  })
  const button = 'cursor-pointer rounded-[10px] border border-edge px-3.5 py-1.5 font-medium hover:bg-selection first-letter:uppercase'
</script>

<Modal label="{why.label}: {title}" {onclose}>
  <div class="grid gap-4 p-6">
    <p><strong class="text-red">✗ {why.label}</strong> <span class="text-base font-semibold text-ink">{title}</span></p>
    <p role="alert" class="break-words">{why.message}</p>
    {#if key}
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-lg bg-bg px-3 py-2 text-[13px] text-white">
        <dt>{t('fingerprint')}</dt>
        <dd class="font-mono text-[13px] break-all text-fg">{key.fingerprint}</dd>
        <dt>{t('on_record')}</dt>
        <dd class="font-mono text-[13px] break-all text-fg">{key.file}:{key.line}</dd>
      </dl>
      <p class="text-yellow">{t('host_key_warning')}</p>
    {:else if why.chain !== why.message}
      <!-- The libraries' own words, for whoever has to look it up. -->
      <p class="rounded-lg bg-bg px-3 py-2 font-mono text-xs break-words text-dim">{why.chain}</p>
    {/if}
    <div class="flex flex-wrap justify-end gap-2">
      <button type="button" onclick={onclose} class="{button} text-white">{t('cancel')}</button>
      {#if key}
        <button type="button" onclick={() => ontrust(key, false)} class="{button} text-yellow">
          {t('accept_once')}
        </button>
        <button type="button" onclick={() => ontrust(key, true)} class="{button} border-red text-red">
          {t('trust')}
        </button>
      {:else}
        <button type="button" onclick={onretry} class="{button} border-blue bg-blue! font-semibold text-page hover:brightness-110">
          {t('try_again')}
        </button>
      {/if}
    </div>
  </div>
</Modal>
