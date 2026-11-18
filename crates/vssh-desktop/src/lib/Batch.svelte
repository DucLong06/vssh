<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount, tick, untrack } from 'svelte'
  import { api, explain, type Answered, type Host, type Lang, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'
  import { KEY, tip } from './keymap'

  type Props = {
    /** The hosts the command is run on. */
    hosts: Host[]
    /** A snippet's command, run at once, with a value for each of its `{{name}}`s; none: one is typed here. */
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
  /** The command line: what is run on Enter. It starts as the snippet's. */
  // svelte-ignore state_referenced_locally
  let typed = $state(command)
  /** Only the hosts whose name has this in it are shown. */
  let filter = $state('')
  let filtering = $state(false)
  let copied = $state(false)
  let list = $state<HTMLElement>()
  let search = $state<HTMLInputElement>()

  const title = $derived(t('run_on').replace('{n}', String(hosts.length)))
  const all = $derived(hosts.map((host) => ({ host, answer: answers[`${batch} ${host.alias}`] })))
  const rows = $derived(all.filter(({ host }) => host.alias.toLowerCase().includes(filter.toLowerCase())))
  const done = $derived(all.filter((row) => row.answer).length)

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
    if (!typed.trim()) return
    refusal = null
    // The answers of the last command are not this one's.
    batch = null
    copied = false
    // The keys are the answers' from here on.
    list?.focus()
    await heard
    try {
      batch = await api.snippetRun(
        hosts.map((host) => host.alias),
        typed,
        params,
      )
    } catch (error) {
      refusal = await explain(error, i18n.lang)
    }
  }

  onMount(() => {
    void run()
    return () => void heard.then((unlisten) => unlisten())
  })

  /** What the hosts on screen answered, a failure in its words. */
  async function copy() {
    const parts = rows.flatMap(({ host, answer }) => {
      if (!answer) return []
      const said = answer.error ? `✗ ${answer.why?.label} ${answer.why?.message}` : answer.output?.trimEnd()
      return [`# ${host.alias}\n${said}\n`]
    })
    if (!parts.length) return
    await api.copy(parts.join('\n'))
    copied = true
  }

  /** How far a j or a k goes: a few lines. */
  const STEP = 80
  const keys: Record<string, () => void> = {
    j: () => list && (list.scrollTop += STEP),
    k: () => list && (list.scrollTop = Math.max(0, list.scrollTop - STEP)),
    y: () => void copy(),
    '/': async () => {
      filtering = true
      await tick()
      search?.focus()
    },
  }

  function onkeydown(e: KeyboardEvent) {
    // Ctrl-F is the desktop's key for what `/` is, from a field too.
    const find = (e.ctrlKey || e.metaKey) && e.key === 'f'
    // In a field a letter is a letter.
    const other = e.target instanceof HTMLInputElement || e.ctrlKey || e.metaKey || e.altKey || !(e.key in keys)
    if (other && !find) return
    e.preventDefault()
    keys[find ? '/' : e.key]()
  }

  const field = 'w-full rounded-lg border border-edge bg-bg px-3 py-2 text-fg placeholder:text-dim'
  const action = 'cursor-pointer rounded-lg border border-edge px-3 py-0.5 font-medium text-fg hover:bg-selection'
</script>

<Modal label={title} wide {onclose}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grid gap-4 p-6" {onkeydown}>
    <p class="flex items-baseline gap-2">
      <span class="text-base font-semibold text-ink">{title}</span>
      {#if batch !== null}
        <span class="ml-auto rounded-full bg-bg px-2.5 text-[13px] text-white">{done}/{hosts.length}</span>
      {/if}
    </p>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        void run()
      }}
    >
      <input
        bind:value={typed}
        aria-label={t('command')}
        placeholder="uptime"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        class="{field} font-mono text-[13px]"
      />
    </form>
    {#if refusal}
      <p role="alert" title={refusal.chain} class="break-words">
        <strong class="text-red">✗ {refusal.label}</strong>
        <span class="text-white">{refusal.message}</span>
      </p>
    {/if}
    {#if filtering || filter}
      <input
        bind:this={search}
        bind:value={filter}
        type="search"
        aria-label={t('filter')}
        placeholder={t('filter')}
        onkeydown={(e) => {
          if (e.key !== 'Enter') return
          filtering = false
          list?.focus()
        }}
        class={field}
      />
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <ul
      bind:this={list}
      tabindex="0"
      aria-label={t('hosts')}
      aria-keyshortcuts="j k / Control+F y"
      class="flex max-h-[45vh] flex-col gap-2 overflow-y-auto rounded-xl outline-none"
    >
      {#each rows as { host, answer } (host.alias)}
        <li data-host={host.alias} class="shrink-0 rounded-xl border border-edge bg-bg px-3 py-2">
          <p class="flex items-center gap-2">
            <!-- No colour of its own: in a list of answers colour is read as the answer. -->
            <strong class="text-ink">{host.alias}</strong>
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
            <pre class="mt-2 rounded-lg bg-page px-3 py-2 font-mono text-[13px] break-words whitespace-pre-wrap text-fg">{answer.output.trimEnd() || t('no_output')}</pre>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
  <footer class="flex items-center gap-4 border-t border-edge px-6 py-3 text-[13px] text-white">
    <button
      type="button"
      aria-pressed={filtering || filter !== ''}
      aria-keyshortcuts="/ Control+F"
      title={tip(t('filter'), KEY.search)}
      onclick={keys['/']}
      class="{action} aria-pressed:bg-selection aria-pressed:text-blue"
    >
      {t('filter')}
    </button>
    <span role="status" class="ml-auto text-green">{copied ? t('copied') : ''}</span>
    <button
      type="button"
      aria-keyshortcuts="y"
      title={tip(t('copy_all'), KEY.copyAll)}
      disabled={!done}
      onclick={copy}
      class="{action} disabled:cursor-default disabled:opacity-60"
    >
      {t('copy_all')}
    </button>
  </footer>
</Modal>
