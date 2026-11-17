<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount } from 'svelte'
  import { api, explain, type ConnError, type Forward, type ForwardState, type Shown } from './api'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    /** The host of the ssh config whose forwards these are. */
    host: string
    onclose: () => void
  }
  let { host, onclose }: Props = $props()

  let rows = $state<Forward[]>([])
  /** What went wrong last, as data and in words; `row`: the forward it was about. */
  let failed = $state<{ error: unknown; why: Shown | null; row?: Forward } | null>(null)
  /** The form: a forward to add. */
  let kind = $state<Forward['kind']>('L')
  let port = $state<number | null>(null)
  let remote = $state('')
  let portField = $state<HTMLInputElement>()

  const leads = (row: Forward) => (row.kind === 'D' ? '' : `${row.host}:${row.hport}`)

  /** Does what was asked; a refusal is said in the form. */
  async function attempt(act: () => Promise<void>, row?: Forward) {
    failed = null
    try {
      await act()
    } catch (error) {
      failed = { error, why: null, row }
    }
  }

  const load = () => attempt(async () => void (rows = await api.forwardsList(host)))

  const toggle = (row: Forward) =>
    attempt(async () => {
      const started = await api.forwardToggle(host, row.kind, row.value)
      // It may listen already: `forward-state` can be here before this answer.
      if (!started) row.state = 'off'
      else if (row.state === 'off') row.state = 'starting'
    }, row)

  const kill = (row: Forward) =>
    attempt(async () => {
      await api.forwardKill(host, row.kind, row.value)
      await load()
    }, row)

  const add = () =>
    attempt(async () => {
      await api.forwardAdd(host, kind, port!, kind === 'D' ? null : remote.trim())
      port = null
      remote = ''
      // Read again: the table is in the order the config has them.
      await load()
    })

  onMount(() => {
    void load()
    const heard = listen<ForwardState>('forward-state', ({ payload }) => {
      if (payload.host !== host) return
      const row = rows.find((row) => row.kind === payload.kind && row.value === payload.value)
      if (row) row.state = payload.state
      // It ended by itself: the host is down, or the port is somebody else's.
      if (payload.error) failed = { error: payload.error, why: null, row }
    })
    return () => void heard.then((unlisten) => unlisten())
  })

  // A failure is kept as data: the words for it follow the language.
  $effect(() => {
    const now = failed
    if (now) void explain(now.error, i18n.lang).then((why) => (now.why = why))
  })

  /** For a port that is taken: the next one up that no forward of this host has. */
  const free = $derived.by(() => {
    const fail = (failed?.error as ConnError | undefined)?.fail
    if (fail?.kind !== 'port_in_use') return null
    let next = Number(fail.addr.split(':').pop()) + 1
    while (rows.some((row) => row.port === next)) next++
    return next <= 65535 ? next : null
  })

  /** Fills the form for the port that is free: what was typed stays, else what the forward in question has. */
  function retry(next: number) {
    const row = failed?.row
    if (row) {
      kind = row.kind
      remote = leads(row)
    }
    port = next
    failed = null
    // There to be changed, or Enter adds it.
    portField?.focus()
  }

  const field = 'mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg placeholder:text-dim disabled:opacity-60'
  const kinds = ['L', 'R', 'D'] as const
  const mark = { on: '●', starting: '◌', off: '○' }
  const word = $derived({ on: t('on'), starting: t('starting'), off: t('off') })
  const tone = { on: 'text-green', starting: 'text-yellow', off: 'text-white' }
</script>

<Modal label="{t('forwards')} {host}" {onclose}>
  <div class="grid gap-3 p-4">
    <p>
      <span class="text-cyan">{t('forwards')}</span>
      <strong>{host}</strong>
    </p>
    {#if rows.length}
      <table class="w-full border-collapse text-left">
        <thead class="whitespace-nowrap text-white">
          <tr>
            <th scope="col" class="w-14 py-1 font-normal">{t('kind')}</th>
            <th scope="col" class="w-28 py-1 font-normal">{t('local_port')}</th>
            <th scope="col" class="py-1 font-normal">{t('remote')}</th>
            <th scope="col" class="w-32 py-1 font-normal">{t('status')}</th>
            <th scope="col" class="w-8 py-1"><span class="sr-only">{t('delete')}</span></th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.kind + row.value)}
            <tr data-state={row.state} class="border-t border-edge/50">
              <td class="py-1"><strong class="text-magenta">{row.kind}</strong></td>
              <td class="py-1">{row.port}</td>
              <td class="truncate py-1 {row.kind === 'D' ? 'text-white' : ''}">{leads(row) || '—'}</td>
              <td class="py-1">
                <!-- What it is, and the way to change it. -->
                <button
                  type="button"
                  role="switch"
                  aria-checked={row.state !== 'off'}
                  aria-label="{row.kind} {row.value}"
                  onclick={() => toggle(row)}
                  class="cursor-pointer rounded-md border border-edge px-2 whitespace-nowrap hover:bg-selection {tone[row.state]}"
                >
                  {mark[row.state]}
                  {word[row.state]}
                </button>
              </td>
              <td class="py-1 text-right">
                <button
                  type="button"
                  aria-label="{t('delete')} {row.kind} {row.value}"
                  onclick={() => kill(row)}
                  class="cursor-pointer rounded px-1.5 text-white hover:bg-selection hover:text-red"
                >
                  ×
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="text-white">{t('no_forwards')}</p>
    {/if}

    <form
      class="grid gap-2 border-t border-edge pt-3"
      onsubmit={(e) => {
        e.preventDefault()
        void add()
      }}
    >
      <div class="grid grid-cols-[auto_7rem_1fr_auto] items-end gap-2 text-white">
        <!-- Radio buttons, drawn as one control: as the dashboard's sort. -->
        <div role="radiogroup" aria-label={t('kind')} class="flex rounded-md border border-edge bg-panel p-0.5">
          {#each kinds as letter (letter)}
            <label
              class="cursor-pointer rounded px-2 py-1 text-white hover:text-fg has-checked:bg-selection has-checked:text-blue has-focus-visible:outline-2 has-focus-visible:outline-blue"
            >
              <input type="radio" bind:group={kind} value={letter} class="sr-only" />
              {letter}
            </label>
          {/each}
        </div>
        <label>
          {t('local_port')}
          <input bind:this={portField} bind:value={port} type="number" min="1" max="65535" required placeholder="8080" class={field} />
        </label>
        <label>
          {t('remote')}
          <input
            bind:value={remote}
            required={kind !== 'D'}
            disabled={kind === 'D'}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            placeholder={kind === 'D' ? 'SOCKS' : 'localhost:80'}
            class={field}
          />
        </label>
        <button
          type="submit"
          class="cursor-pointer rounded-md border border-blue bg-selection px-3 py-1.5 text-blue hover:bg-selection"
        >
          {t('add')}
        </button>
      </div>
      {#if failed?.why}
        <div class="flex flex-wrap items-center gap-2">
          <p role="alert" title={failed.why.chain} class="min-w-0 break-words">
            <strong class="text-red">✗ {failed.why.label}</strong>
            <span class="text-white">{failed.why.message}</span>
          </p>
          {#if free}
            <button
              type="button"
              onclick={() => retry(free)}
              class="cursor-pointer rounded-md border border-edge px-2 whitespace-nowrap text-blue hover:bg-selection"
            >
              {t('try_port').replace('{n}', String(free))}
            </button>
          {/if}
        </div>
      {/if}
    </form>
  </div>
</Modal>
