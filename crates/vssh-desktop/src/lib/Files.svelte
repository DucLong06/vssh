<script lang="ts">
  import { onMount } from 'svelte'
  import { api, explain, type Entry, type Shown } from './api'
  import FilePane from './FilePane.svelte'
  import { i18n, t } from './i18n.svelte'

  type Props = {
    /** The host of the ssh config whose files are on the right. */
    host: string
    /** Its tab is in front. */
    active: boolean
  }
  let { host, active }: Props = $props()

  type Side = 'local' | 'remote'
  type Pane = {
    /** In full; empty until the first answer. */
    path: string
    entries: Entry[]
    marked: string[]
    loading: boolean
    /** Why the last question failed, as data and in words. */
    error: unknown
    why: Shown | null
  }
  const blank = (): Pane => ({ path: '', entries: [], marked: [], loading: true, error: null, why: null })
  const panes = $state<Record<Side, Pane>>({ local: blank(), remote: blank() })
  const views = $state<Partial<Record<Side, FilePane>>>({})
  let showHidden = $state(false)

  /** The same questions, to this machine and to the host. */
  const fs = {
    local: { list: api.localList },
    remote: {
      async list(path: string) {
        // Where the host starts is asked once: from then on every path is in full.
        const full = path || (await api.sftpRealpath(host, ''))
        return { path: full, entries: await api.sftpList(host, full) }
      },
    },
  }

  /** The last question to each side: an answer to an earlier one is dropped. */
  const asked = { local: 0, remote: 0 }

  /** Lists a directory; one that cannot be listed leaves the pane where it was, and says why. */
  async function load(side: Side, path = panes[side].path) {
    const pane = panes[side]
    const mine = ++asked[side]
    pane.loading = true
    try {
      const got = await fs[side].list(path)
      if (mine !== asked[side]) return
      const still = (name: string) => got.entries.some((entry) => entry.name === name)
      const marked = got.path === pane.path ? pane.marked.filter(still) : []
      Object.assign(pane, got, { marked, error: null, why: null })
    } catch (error) {
      if (mine === asked[side]) pane.error = error
    } finally {
      if (mine === asked[side]) pane.loading = false
    }
  }

  onMount(() => {
    void load('local')
    void load('remote')
  })

  // A failure is kept as data: the words for it follow the language.
  $effect(() => {
    const lang = i18n.lang
    for (const item of [panes.local, panes.remote]) {
      const error = item.error
      if (!error) continue
      void explain(error, lang).then((why) => {
        if (item.error === error) item.why = why
      })
    }
  })

  // In front, the keys are the local pane's first.
  $effect(() => {
    if (active) views.local?.focus()
  })
</script>

{#snippet pane(side: Side)}
  <FilePane
    bind:this={views[side]}
    {side}
    label={side === 'local' ? t('local') : host}
    path={panes[side].path}
    entries={panes[side].entries}
    error={panes[side].error ? panes[side].why : null}
    loading={panes[side].loading}
    bind:showHidden
    bind:marked={panes[side].marked}
    oncd={(path) => load(side, path)}
    oncopy={() => {}}
    onmenu={() => {}}
    onretry={() => load(side)}
    ondragnames={() => {}}
    ondropped={() => {}}
  />
{/snippet}

<div role="tabpanel" aria-label="{t('files')} {host}" hidden={!active} class="min-h-0 flex-1 bg-bg p-3">
  <div class="flex h-full flex-col gap-2">
    <div class="flex min-h-0 flex-1 gap-2">
      {@render pane('local')}
      <div class="flex flex-col justify-center gap-2">
        <button type="button" aria-label={t('upload')} title={t('upload')} class="cursor-pointer rounded-md border border-edge bg-panel px-2 py-1 text-magenta hover:bg-selection">→</button>
        <button type="button" aria-label={t('download')} title={t('download')} class="cursor-pointer rounded-md border border-edge bg-panel px-2 py-1 text-magenta hover:bg-selection">←</button>
      </div>
      {@render pane('remote')}
    </div>
  </div>
</div>
