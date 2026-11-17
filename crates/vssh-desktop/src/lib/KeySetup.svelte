<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount } from 'svelte'
  import { api, explain, type KeyStep, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    /** The host of the ssh config that gets a key of its own. */
    host: string
    onclose: () => void
  }
  let { host, onclose }: Props = $props()

  /** The four steps a person follows; the shell's `backup` is the beginning of the second. */
  type Id = 'generate' | 'install' | 'verify' | 'disable'
  type Progress = 'pending' | KeyStep['state']
  const before = (): Record<Id, Progress> => ({ generate: 'pending', install: 'pending', verify: 'pending', disable: 'pending' })
  let at = $state(before())
  let phase = $state<'ready' | 'running' | 'done' | 'failed'>('ready')
  /** After a failure: the shell put sshd_config back. */
  let restored = $state(false)
  /** Where the private key is. */
  let key = $state('')
  let failed = $state<Shown | null>(null)
  /** The backup or the restore of sshd_config was refused: the core says so in English only, the note in both. */
  let refused = $state(false)
  let go = $state<HTMLButtonElement>()

  async function start() {
    at = before()
    restored = false
    refused = false
    failed = null
    phase = 'running'
    try {
      key = await api.sshKeySetup(host)
      phase = 'done'
    } catch (error) {
      failed = await explain(error, i18n.lang)
      phase = 'failed'
    }
  }

  onMount(() => {
    // Not the close button, which comes first: Enter starts.
    go?.focus()
    const heard = listen<KeyStep>('key-setup-step', ({ payload }) => {
      if (payload.host !== host) return
      if ((payload.step === 'backup' || payload.step === 'restore') && payload.state === 'failed') refused = true
      if (payload.step === 'restore') restored = payload.state === 'done'
      // The backup is taken: the step goes on, to the key on the host.
      else if (payload.step !== 'backup') at[payload.step] = payload.state
      else if (payload.state !== 'done') at.install = payload.state
    })
    return () => void heard.then((unlisten) => unlisten())
  })

  const steps = $derived<{ id: Id; name: string; note: string }[]>([
    { id: 'generate', name: t('key_step_generate'), note: t('key_step_generate_note') },
    { id: 'install', name: t('key_step_install'), note: t('key_step_install_note') },
    { id: 'verify', name: t('key_step_verify'), note: t('key_step_verify_note') },
    { id: 'disable', name: t('key_step_disable'), note: t('key_step_disable_note') },
  ])
  const tone = { pending: 'text-dim', running: 'text-yellow', done: 'text-green', failed: 'text-red' }
  const button = 'cursor-pointer rounded-md border border-edge px-3 py-1.5 hover:bg-selection disabled:opacity-60'
</script>

<!-- While it runs the shell goes on whatever the window does, to the end or to the restore: it stays to say which. -->
<Modal label="{t('key_setup')} {host}" locked={phase === 'running'} {onclose}>
  <div class="grid gap-3 p-4">
    <p>
      <span class="text-cyan">{t('key_setup')}</span>
      <strong>{host}</strong>
    </p>
    <ol class="grid gap-2">
      {#each steps as step, i (step.id)}
        {@const now = at[step.id]}
        <li data-state={now} class="grid grid-cols-[1.5rem_1fr] items-baseline gap-x-2">
          <span class="row-span-2 text-center {tone[now]}">
            {#if now === 'running'}
              <span
                role="progressbar"
                aria-label={t('running')}
                class="inline-block size-3 animate-spin rounded-full border-2 border-yellow border-t-transparent"
              ></span>
            {:else}
              <span aria-hidden="true">{now === 'done' ? '✓' : now === 'failed' ? '✗' : i + 1}</span>
            {/if}
          </span>
          <strong class={now === 'pending' ? 'font-normal' : tone[now]}>{step.name}</strong>
          <span class="text-white">{step.note}</span>
        </li>
      {/each}
    </ol>

    {#if failed}
      <p role="alert" title={failed.chain} class="break-words">
        <strong class="text-red">✗ {failed.label}</strong>
        <!-- A failure of the network has the core's words in both languages; the rest of the chain is in the title. -->
        <span class="text-white">{refused && failed.class === 'other' ? t('key_setup_note') : failed.message}</span>
      </p>
    {/if}
    {#if restored}
      <p role="status" class="text-green">{t('key_setup_restored')}</p>
    {:else if phase === 'done'}
      <p role="status" class="break-words">
        <span class="text-green">{t('key_setup_done').replace('{name}', host)}</span>
        <span class="text-white">{key}</span>
      </p>
    {:else if phase === 'ready'}
      <p class="text-yellow">{t('key_setup_note')}</p>
    {/if}
    <div class="flex justify-end gap-2">
      <button type="button" onclick={onclose} disabled={phase === 'running'} class="{button} text-white">
        {t('close')}
      </button>
      {#if phase === 'ready' || phase === 'failed'}
        <button bind:this={go} type="button" onclick={start} class="{button} border-blue bg-selection text-blue">
          {phase === 'ready' ? t('start') : t('try_again')}
        </button>
      {/if}
    </div>
  </div>
</Modal>
