<script lang="ts">
  import { tick } from 'svelte'
  import type { Entry, Shown } from './api'
  import { crumbs, human, join, parent, when } from './paths'
  import { t } from './i18n.svelte'

  type Props = {
    side: 'local' | 'remote'
    /** Whose files these are: this machine's, or a host's. */
    label: string
    /** In full; empty while nobody knows it yet. */
    path: string
    entries: Entry[]
    /** Why the directory could not be read. */
    error?: Shown | null
    loading?: boolean
    showHidden?: boolean
    /** The names chosen to be copied together. */
    marked?: string[]
    oncd: (path: string) => void
    /** `c`: the selected entry, if there is one. */
    oncopy: (entry: Entry | null) => void
    /** The right button or the menu key, and the entry it was on. */
    onmenu: (at: { x: number; y: number }, entry: Entry | null) => void
    onretry: () => void
    /** A drag of these names started here; something was dropped here. */
    ondragnames: (names: string[]) => void
    ondropped: () => void
  }
  let {
    side,
    label,
    path,
    entries,
    error = null,
    loading = false,
    showHidden = $bindable(false),
    marked = $bindable([]),
    oncd,
    oncopy,
    onmenu,
    onretry,
    ondragnames,
    ondropped,
  }: Props = $props()

  let filter = $state('')
  let filtering = $state(false)
  /** The selected entry, by name: it stays the selected one when the rows around it change. */
  let name = $state('')
  let grid = $state<HTMLElement>()
  let search = $state<HTMLInputElement>()
  let trail = $state<HTMLElement>()

  const visible = $derived(
    entries.filter(
      (entry) =>
        (showHidden || !entry.name.startsWith('.')) && entry.name.toLowerCase().includes(filter.toLowerCase()),
    ),
  )
  const at = $derived(Math.max(0, visible.findIndex((entry) => entry.name === name)))
  const selected = $derived<Entry | null>(visible[at] ?? null)

  export const current = () => selected
  export const focus = () => grid?.focus()

  $effect(() => {
    if (selected) grid?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  })

  /** The head of the path is out of sight. */
  let clipped = $state(false)

  // Of a path too long for its pane, the end is what is shown: where one is.
  $effect(() => {
    // Again when the label changes with the language: it takes room from the path.
    if (!path || !label || !trail) return
    const over = trail.scrollWidth - trail.clientWidth
    clipped = over > 0
    // From the first name that fits whole: a cut inside one reads as a directory that is not there.
    const whole = [...trail.querySelectorAll('button')].find((crumb) => crumb.offsetLeft >= over)
    trail.scrollLeft = clipped ? (whole?.offsetLeft ?? over) : 0
  })

  /** The last column: there from a pane of 20rem on. */
  const late = 'hidden px-2 py-0.5 @xs:table-cell'

  const move = (to: number) => (name = visible[Math.min(Math.max(to, 0), visible.length - 1)]?.name ?? '')

  function cd(to: string, then = '') {
    filter = ''
    filtering = false
    name = then
    oncd(to)
  }
  const enter = (entry: Entry | null) => entry?.is_dir && cd(join(path, entry.name))
  // Back up, the directory it came out of is the selected one.
  const up = () => cd(parent(path), path.replace(/\/+$/, '').split('/').pop())

  const mark = (entry: Entry) =>
    (marked = marked.includes(entry.name) ? marked.filter((other) => other !== entry.name) : [...marked, entry.name])

  const keys: Record<string, () => void> = {
    j: () => move(at + 1),
    ArrowDown: () => move(at + 1),
    k: () => move(at - 1),
    ArrowUp: () => move(at - 1),
    g: () => move(0),
    Home: () => move(0),
    G: () => move(visible.length - 1),
    End: () => move(visible.length - 1),
    Enter: () => enter(selected),
    l: () => enter(selected),
    ArrowRight: () => enter(selected),
    Backspace: up,
    u: up,
    h: up,
    ArrowLeft: up,
    ' ': () => {
      if (!selected) return
      mark(selected)
      move(at + 1)
    },
    c: () => oncopy(selected),
    '.': () => (showHidden = !showHidden),
    '/': async () => {
      filtering = true
      await tick()
      search?.focus()
    },
    Escape: () => (filter = ''),
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey || !(e.key in keys)) return
    e.preventDefault()
    keys[e.key]()
  }

  function menu(e: MouseEvent) {
    e.preventDefault()
    const target = e.target as HTMLElement
    const row = target.closest<HTMLElement>('tr[data-name]')
    if (row) name = row.dataset.name!
    // The menu key says nothing of where: under the row it is about.
    const box = (row ?? target).getBoundingClientRect()
    const entry = row || target === grid ? (visible.find((entry) => entry.name === name) ?? null) : null
    onmenu({ x: e.clientX || box.left + 24, y: e.clientY || box.top + 24 }, entry)
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section
  aria-label={label}
  data-side={side}
  oncontextmenu={menu}
  ondragover={(e) => e.preventDefault()}
  ondrop={(e) => {
    e.preventDefault()
    ondropped()
  }}
  class="@container flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-edge bg-panel focus-within:border-blue"
>
  <header class="flex items-center gap-2 border-b border-edge px-3 py-1.5">
    <strong class="shrink-0 text-cyan">{label}</strong>
    {#if clipped}<span aria-hidden="true" class="shrink-0 text-dim">…</span>{/if}
    <!-- It scrolls, to its end; a bar across the crumbs would say nothing more.
         relative: a crumb's offsetLeft is then its place in the trail. -->
    <nav
      bind:this={trail}
      aria-label={t('path')}
      class="relative flex min-w-0 flex-1 items-center overflow-x-auto whitespace-nowrap [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      {#each crumbs(path) as crumb, i (crumb.path)}
        {#if i > 1}<span aria-hidden="true" class="text-dim">/</span>{/if}
        <button
          type="button"
          onclick={() => cd(crumb.path)}
          aria-current={crumb.path === path ? 'location' : undefined}
          class="cursor-pointer rounded px-1 text-white hover:bg-selection hover:text-fg aria-[current]:text-fg"
        >
          {crumb.name}
        </button>
      {/each}
    </nav>
    <button
      type="button"
      aria-label={t('hidden_files')}
      aria-pressed={showHidden}
      aria-keyshortcuts="."
      title={t('hidden_files')}
      onclick={() => (showHidden = !showHidden)}
      class="shrink-0 cursor-pointer rounded-md border border-edge px-1.5 text-white hover:bg-selection aria-pressed:bg-selection aria-pressed:text-blue"
    >
      .*
    </button>
  </header>

  {#if filtering || filter}
    <input
      bind:this={search}
      bind:value={filter}
      type="search"
      aria-label={t('filter')}
      placeholder="/  {t('filter')}"
      onkeydown={(e) => {
        if (e.key !== 'Escape' && e.key !== 'Enter') return
        if (e.key === 'Escape') filter = ''
        filtering = false
        grid?.focus()
      }}
      class="border-b border-edge bg-bg px-3 py-1 text-fg placeholder:text-dim"
    />
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_to_interactive_role -->
    <table
      bind:this={grid}
      role="grid"
      tabindex="0"
      aria-label={label}
      aria-busy={loading}
      aria-keyshortcuts="j k g Shift+G Enter Backspace Space c / ."
      {onkeydown}
      class="w-full table-fixed border-collapse outline-none aria-busy:opacity-60"
    >
      <thead class="sticky top-0 bg-panel text-left whitespace-nowrap text-white">
        <tr>
          <th scope="col" class="px-3 py-1 font-normal">{t('sort_name')}</th>
          <th scope="col" class="w-24 px-2 py-1 text-right font-normal">{t('size')}</th>
          <!-- A narrow pane keeps the names. -->
          <th scope="col" class="{late} w-26 font-normal">{t('modified')}</th>
        </tr>
      </thead>
      {#if !error}
        <tbody>
          {#each visible as entry (entry.name)}
            {@const chosen = marked.includes(entry.name)}
            <tr
              data-name={entry.name}
              data-kind={entry.is_dir ? 'dir' : 'file'}
              data-marked={chosen || undefined}
              aria-selected={entry === selected}
              draggable="true"
              onclick={(e) => {
                name = entry.name
                if (e.ctrlKey || e.metaKey) mark(entry)
                grid?.focus()
              }}
              ondblclick={() => enter(entry)}
              ondragstart={(e) => {
                // Some webviews do not start a drag that carries nothing.
                e.dataTransfer?.setData('text/plain', entry.name)
                ondragnames(chosen ? marked : [entry.name])
              }}
              class="cursor-default select-none hover:bg-selection/50 aria-selected:bg-selection data-[marked]:text-magenta"
            >
              <td role="gridcell" class="px-3 py-0.5">
                <div class="flex items-center gap-2">
                  <svg
                    aria-hidden="true"
                    viewBox="0 0 16 16"
                    class="size-4 shrink-0 {entry.is_dir ? 'fill-blue' : 'fill-none stroke-dim'}"
                  >
                    {#if entry.is_dir}
                      <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h3.6l1.5 2h5.9A1.5 1.5 0 0 1 15 5.5v7a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 1 12.5z" />
                    {:else}
                      <path d="M3.5 1.5h6l3 3v10h-9zM9.5 1.5v3h3" />
                    {/if}
                  </svg>
                  <span title={entry.name} class="truncate {entry.is_dir ? 'font-bold' : ''}">{entry.name}</span>
                  {#if chosen}
                    <span role="img" aria-label={t('selected')}>✓</span>
                  {/if}
                </div>
              </td>
              <td role="gridcell" class="px-2 py-0.5 text-right whitespace-nowrap text-white">
                {entry.is_dir ? '—' : human(entry.size)}
              </td>
              <td role="gridcell" class="{late} whitespace-nowrap text-white">{when(entry.mtime)}</td>
            </tr>
          {/each}
        </tbody>
      {/if}
    </table>
    {#if error}
      <div class="grid justify-items-start gap-2 p-3">
        <p role="alert" class="break-words">
          <strong class="text-red">✗ {error.label}</strong>
          <span class="text-white">{error.message}</span>
        </p>
        <button
          type="button"
          onclick={onretry}
          class="cursor-pointer rounded-md border border-edge px-2 py-0.5 text-blue hover:bg-selection"
        >
          {t('try_again')}
        </button>
      </div>
    {:else if visible.length === 0 && !loading}
      <p class="p-3 text-white">{filter ? `/${filter}: 0` : t('empty_dir')}</p>
    {/if}
  </div>
</section>
