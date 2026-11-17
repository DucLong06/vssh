<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount, untrack } from 'svelte'
  import { api, explain, type Answered, type Host, type Lang, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    /** The hosts the command is run on. */
    hosts: Host[]
    /** A snippet's command, run at once, with a value for each of its `{{name}}`s. */
    command?: string
    params?: Record<string, string>
    onclose: () => void
  }
  let { hosts, command = '', params = {}, onclose }: Props = $props()

  /** A host's answer, and its failure in words once they are there. */
  type Answer = Answered & { why: Shown | null }
  /** The batch that was started; its hosts answer under its number. */
  let batch = $state<number | null>(null)
  /** Every answer heard, by batch and host: one may come before the batch has its number. */
  const answers = $state<Record<string, Answer>>({})
  /** Why the batch could not be started. */
  let refusal = $state<Shown | null>(null)

  const title = $derived(t('run_on').replace('{n}', String(hosts.length)))
  const rows = $derived(hosts.map((host) => ({ host, answer: answers[`${batch} ${host.alias}`] })))
  const done = $derived(rows.filter((row) => row.answer).length)

  /** A failure is kept as data: the words for it follow the language. */
  const say = (answer: Answer, lang: Lang) =>
    answer.error && void explain(answer.error, lang).then((why) => (answer.why = why))

  // Before a batch is asked for: its first answers are not missed.
  const heard = listen<Answered>('batch-progress', ({ payload }) => {
    const key = `${payload.id} ${payload.host}`
    answers[key] = { ...payload, why: null }
    say(answers[key], i18n.lang)
  })

  $effect(() => {
    const lang = i18n.lang
    untrack(() => Object.values(answers).forEach((answer) => say(answer, lang)))
  })

  async function run() {
    refusal = null
    await heard
    try {
      batch = await api.snippetRun(
        hosts.map((host) => host.alias),
        command,
        params,
      )
    } catch (error) {
      refusal = await explain(error, i18n.lang)
    }
  }

  onMount(() => {
    if (command) void run()
    return () => void heard.then((unlisten) => unlisten())
  })
</script>

<Modal label={title} wide {onclose}>
  <div class="grid gap-3 p-4">
    <p class="flex items-baseline gap-2">
      <span class="text-cyan">{title}</span>
      {#if batch !== null}
        <span class="ml-auto text-white">{done}/{hosts.length}</span>
      {/if}
    </p>
    <p class="break-words text-white">{command}</p>
    {#if refusal}
      <p role="alert" title={refusal.chain} class="break-words">
        <strong class="text-red">✗ {refusal.label}</strong>
        <span class="text-white">{refusal.message}</span>
      </p>
    {/if}
    <ul aria-label={t('hosts')} class="grid max-h-[50vh] gap-2 overflow-y-auto">
      {#each rows as { host, answer } (host.alias)}
        <li data-host={host.alias} class="rounded-xl border border-edge bg-panel px-3 py-2">
          <p class="flex items-center gap-2">
            <strong>{host.alias}</strong>
            {#if answer?.error}
              <!-- The class of the failure, as a card says it: a host that failed is not left out. -->
              <span role="alert" title={answer.why?.chain} class="min-w-0 break-words">
                <strong class="text-red">✗ {answer.why?.label}</strong>
                <span class="text-white">{answer.why?.message}</span>
              </span>
            {:else if answer}
              <span aria-hidden="true" class="text-green">✓</span>
            {:else if batch !== null}
              <span
                role="img"
                aria-label={t('running')}
                class="size-3 animate-spin rounded-full border-2 border-edge border-t-blue"
              ></span>
            {/if}
          </p>
          {#if answer && answer.output !== null}
            <pre class="mt-1 overflow-x-auto whitespace-pre-wrap text-fg">{answer.output.trimEnd() || t('no_output')}</pre>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</Modal>
