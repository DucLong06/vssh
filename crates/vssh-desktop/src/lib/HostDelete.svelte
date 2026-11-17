<script lang="ts">
  import { api, explain, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    alias: string
    /** It is gone from the config: what the window has of it goes too. */
    ondeleted: (alias: string) => void
    onclose: () => void
  }
  let { alias, ondeleted, onclose }: Props = $props()

  let failed = $state<Shown | null>(null)
  const ask = $derived(t('delete_ask').replace('{name}', alias))

  async function remove(e: SubmitEvent) {
    e.preventDefault()
    try {
      await api.hostsDelete(alias)
      ondeleted(alias)
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  }

  const button = 'cursor-pointer rounded-[10px] border border-edge px-3.5 py-1.5 font-medium hover:bg-selection'
</script>

<Modal label={ask} {onclose}>
  <form onsubmit={remove} class="grid gap-4 p-6">
    <p>{ask}</p>
    <p class="text-white">{t('delete_host_note')}</p>
    {#if failed}
      <p role="alert" title={failed.chain} class="break-words">
        <strong class="text-red">✗ {failed.label}</strong>
        <span class="text-white">{failed.message}</span>
      </p>
    {/if}
    <!-- Cancel comes first and has the focus: Enter is not a yes. -->
    <div class="flex justify-end gap-2">
      <button type="button" onclick={onclose} class="{button} text-white">{t('cancel')}</button>
      <button type="submit" class="{button} border-red text-red">{t('delete')}</button>
    </div>
  </form>
</Modal>
