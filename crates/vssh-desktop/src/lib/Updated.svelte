<script lang="ts">
  import { api } from './api'
  import { t } from './i18n.svelte'
  import Modal from './Modal.svelte'
  import { update } from './update.svelte'

  // How an update that was asked for ended: installed, and the app is to be
  // started again; or fetched only, with the line that installs it by hand.
  const named = (words: string) => words.replace('{name}', update.tag ?? '')
  const close = () => (update.dialog = false)
  // One word to the shell, whatever is in the path.
  const command = $derived(`sudo apt install '${update.file.replaceAll("'", "'\\''")}'`)

  const button = 'cursor-pointer rounded-[10px] border border-edge px-3.5 py-1.5 font-medium hover:bg-selection'
</script>

<Modal label={t('update_now')} onclose={close}>
  <div class="grid gap-4 p-6">
    {#if update.phase === 'installed'}
      <p>{named(t('update_installed'))}</p>
      <!-- Later comes first and has the focus: Enter does not end the open sessions. -->
      <div class="flex justify-end gap-2">
        <button type="button" onclick={close} class="{button} text-white">{t('later')}</button>
        <button type="button" onclick={() => void api.updateRestart().catch(() => {})} class="{button} border-cyan text-cyan">{t('update_restart')}</button>
      </div>
    {:else}
      <p>{named(t('update_by_hand'))}</p>
      <code class="rounded-md bg-black px-3 py-2 font-mono text-xs break-all select-all">{command}</code>
      <div class="flex justify-end">
        <button type="button" onclick={close} class="{button} text-white">{t('close')}</button>
      </div>
    {/if}
  </div>
</Modal>
