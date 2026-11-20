<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { api, explain, type Host, type Shown, type Snippet } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    /** The saved commands of the TUI's database; kept up with what is changed here. */
    snippets: Snippet[]
    hosts: Host[]
    /** The hosts chosen on the dashboard: ticked when a snippet is run. */
    picked?: string[]
    /** A snippet's command, the hosts it goes to, and a value for each of its `{{name}}`s. */
    onrun: (hosts: string[], command: string, params: Record<string, string>) => void
    onclose: () => void
  }
  let { snippets = $bindable(), hosts, picked = [], onrun, onclose }: Props = $props()

  /** What the dialog shows: the list, or one snippet being changed, deleted or run. */
  type View =
    | { kind: 'list' }
    /** `id` 0: a snippet that is not saved yet. */
    | { kind: 'edit'; id: number }
    | { kind: 'rm' | 'run'; snippet: Snippet }
  let view = $state<View>({ kind: 'list' })
  /** The form's fields. `scope`: the host the snippet is for; empty, every host. */
  let name = $state('')
  let command = $state('')
  let scope = $state('')
  /** A value for each parameter of the snippet being run, and the hosts ticked for it. */
  let values = $state<Record<string, string>>({})
  let ticked = $state<string[]>([])
  let refusal = $state<Shown | null>(null)
  let root = $state<HTMLElement>()

  // The TUI writes to the same database: what it saved since the window started is read now.
  onMount(() => void api.listSnippets().then((list) => (snippets = list), () => {}))

  /** A command's parameters, each once: `{{name}}`, as the shell fills them in. */
  const params = (command: string) => [...new Set([...command.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((found) => found[1]))]

  async function show(next: View) {
    refusal = null
    view = next
    // What had the focus is gone with the last view: the first field of this one, Cancel under a question.
    await tick()
    root?.querySelector<HTMLElement>('input, button')?.focus()
  }

  function edit(snippet?: Snippet) {
    ;({ name, command, host: scope } = snippet ?? { name: '', command: '', host: '' })
    void show({ kind: 'edit', id: snippet?.id ?? 0 })
  }

  function start(snippet: Snippet) {
    values = {}
    // One that is for a host is run there, whatever is chosen on the dashboard.
    ticked = snippet.host ? [snippet.host] : picked
    void show({ kind: 'run', snippet })
  }

  /** Does a change, reads the list again and goes back to it; a refusal stays in the form. */
  async function change(act: Promise<unknown>) {
    try {
      await act
      snippets = await api.listSnippets()
      void show({ kind: 'list' })
    } catch (error) {
      refusal = await explain(error, i18n.lang)
    }
  }

  const submit = (act: () => void) => (e: SubmitEvent) => {
    e.preventDefault()
    act()
  }

  const button = 'cursor-pointer rounded-ctl border border-edge min-h-9 px-3.5 py-1.5 font-medium hover:bg-selection'
  const small = 'cursor-pointer rounded-lg border border-edge px-2.5 py-1 text-sm font-medium hover:bg-selection'
  const primary = `${button} border-blue bg-blue! font-semibold text-page hover:brightness-110 disabled:opacity-60`
  const field = 'mt-1 w-full rounded-lg border border-edge bg-bg px-3 py-2 text-fg placeholder:text-dim'
  const chip =
    'cursor-pointer rounded-lg border border-edge px-2 py-0.5 text-white hover:bg-selection has-focus-visible:outline-2 has-focus-visible:outline-blue'
</script>

{#snippet refused()}
  {#if refusal}
    <p role="alert" title={refusal.chain} class="break-words">
      <strong class="text-red">✗ {refusal.label}</strong>
      <span class="text-white">{refusal.message}</span>
    </p>
  {/if}
{/snippet}

<Modal label={t('snippets')} {onclose}>
  <div bind:this={root}>
    {#if view.kind === 'list'}
      <div class="grid gap-4 p-7">
        <p class="flex items-center">
          <span class="text-base font-semibold text-ink">{t('snippets')}</span>
          <button type="button" onclick={() => edit()} class="{small} ml-auto text-blue">{t('add')}</button>
        </p>
        {#if snippets.length}
          <!-- A column, not a grid: a row is as wide as the dialog and its command is cut to fit. -->
          <ul aria-label={t('snippets')} class="flex max-h-[50vh] flex-col gap-1 overflow-y-auto">
            {#each snippets as snippet (snippet.id)}
              <li class="flex items-center gap-2 rounded-xl bg-bg px-3 py-2.5 hover:bg-selection">
                <div class="min-w-0 flex-1">
                  <p class="flex items-baseline gap-2">
                    <strong class="truncate font-semibold text-ink">{snippet.name}</strong>
                    <span class="shrink-0 {snippet.host ? 'text-cyan' : 'text-white'}">
                      {snippet.host || t('all_hosts')}
                    </span>
                  </p>
                  <p title={snippet.command} class="truncate font-mono text-xs text-white">{snippet.command}</p>
                </div>
                <button
                  type="button"
                  aria-label="{t('run')} {snippet.name}"
                  onclick={() => start(snippet)}
                  class="{small} text-magenta"
                >
                  {t('run')}
                </button>
                <button
                  type="button"
                  aria-label="{t('edit')} {snippet.name}"
                  onclick={() => edit(snippet)}
                  class="{small} text-white"
                >
                  {t('edit')}
                </button>
                <!-- Red under the pointer only: a red word on every row would be all one sees. -->
                <button
                  type="button"
                  aria-label="{t('delete')} {snippet.name}"
                  onclick={() => show({ kind: 'rm', snippet })}
                  class="{small} text-white hover:text-red"
                >
                  {t('delete')}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="text-white">{t('no_snippets')}</p>
        {/if}
      </div>
    {:else if view.kind === 'edit'}
      {@const id = view.id}
      <form
        class="grid gap-4 p-7"
        onsubmit={submit(
          () => void change(id ? api.snippetsUpdate(id, name, command, scope) : api.snippetsCreate(name, command, scope)),
        )}
      >
        <label class="text-sm text-white">
          {t('sort_name')}
          <input bind:value={name} required autocomplete="off" autocapitalize="off" spellcheck="false" class={field} />
        </label>
        <label class="text-sm text-white">
          {t('command')}
          <input
            bind:value={command}
            required
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            placeholder={'systemctl restart {{unit}}'}
            class="{field} font-mono text-sm"
          />
        </label>
        <!-- Radio buttons drawn as chips: a native select opens the toolkit's own light menu. -->
        <div role="radiogroup" aria-label={t('host')} class="flex max-h-28 flex-wrap gap-1 overflow-y-auto">
          {#each ['', ...hosts.map((host) => host.alias)] as alias (alias)}
            <label class="{chip} has-checked:border-blue has-checked:bg-selection has-checked:text-blue">
              <input type="radio" bind:group={scope} value={alias} class="sr-only" />
              {alias || t('all_hosts')}
            </label>
          {/each}
        </div>
        {@render refused()}
        <div class="flex justify-end gap-2">
          <button type="button" onclick={() => show({ kind: 'list' })} class="{button} text-white">{t('cancel')}</button>
          <button type="submit" class={primary}>{t('save')}</button>
        </div>
      </form>
    {:else if view.kind === 'rm'}
      {@const snippet = view.snippet}
      <form class="grid gap-4 p-7" onsubmit={submit(() => void change(api.snippetsDelete(snippet.id)))}>
        <p class="break-words">{t('delete_ask').replace('{name}', snippet.name)}</p>
        {@render refused()}
        <div class="flex justify-end gap-2">
          <button type="button" onclick={() => show({ kind: 'list' })} class="{button} text-white">{t('cancel')}</button>
          <button type="submit" class="{button} border-red text-red">{t('delete')}</button>
        </div>
      </form>
    {:else}
      {@const snippet = view.snippet}
      {@const names = hosts.map((host) => host.alias).filter((alias) => !snippet.host || alias === snippet.host)}
      <!-- In the order the hosts have, not the one they were ticked in. -->
      {@const going = names.filter((alias) => ticked.includes(alias))}
      {@const title = t('run_on').replace('{n}', String(going.length))}
      <form class="grid gap-4 p-7" onsubmit={submit(() => onrun(going, snippet.command, { ...values }))}>
        <p>
          <strong class="text-base font-semibold text-ink">{snippet.name}</strong>
          <span class="font-mono text-sm break-words text-white">{snippet.command}</span>
        </p>
        {#each params(snippet.command) as param (param)}
          <label class="text-sm text-white">
            {param}
            <input
              bind:value={values[param]}
              aria-label={param}
              required
              autocomplete="off"
              autocapitalize="off"
              spellcheck="false"
              placeholder={t('param_hint').replace('{name}', param)}
              class={field}
            />
          </label>
        {/each}
        <div role="group" aria-label={title} class="grid gap-1">
          <p class="text-white">{title}</p>
          <div class="flex max-h-28 flex-wrap gap-1 overflow-y-auto">
            {#each names as alias (alias)}
              <!-- Marked as a card is on the dashboard. -->
              <label class="{chip} has-checked:border-magenta has-checked:text-magenta">
                <input type="checkbox" bind:group={ticked} value={alias} aria-label={alias} class="peer sr-only" />
                {alias}
                <span aria-hidden="true" class="hidden peer-checked:inline">✓</span>
              </label>
            {/each}
          </div>
        </div>
        <div class="flex justify-end gap-2">
          <button type="button" onclick={() => show({ kind: 'list' })} class="{button} text-white">{t('cancel')}</button>
          <button type="submit" disabled={!going.length} class={primary}>{t('run')}</button>
        </div>
      </form>
    {/if}
  </div>
</Modal>
