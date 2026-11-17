<script lang="ts">
  import type { Host } from './api'
  import { fuzzy } from './fuzzy'
  import { address, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import Modal from './Modal.svelte'

  type Props = {
    hosts: Host[]
    onhost: (alias: string) => void
    onquick: () => void
    onsnippets: () => void
    onclose: () => void
  }
  let { hosts, onhost, onquick, onsnippets, onclose }: Props = $props()

  type Item = { key: string; label: string; detail: string; action: boolean; run: () => void }

  let query = $state('')
  let at = $state(0)

  /** The loosest a match may be found by: the best of these, `null` if none. */
  const best = (...scores: (number | null)[]) =>
    scores.reduce((a, b) => (a === null ? b : b === null ? a : Math.min(a, b)), null)

  const items = $derived.by(() => {
    // `en`: an action's name whatever language its label is in ("qc" finds the first).
    const actions = [
      { key: 'quick-connect', label: t('quick_connect'), detail: 'user@host[:port]', en: 'quick connect', run: onquick },
      { key: 'snippets', label: t('snippets'), detail: 's', en: 'snippets', run: onsnippets },
    ].map(({ en, ...item }) => ({ item: { ...item, action: true }, en }))
    const rows = hosts.map((host) => {
      const detail = address(host)
      const item = { key: host.alias, label: host.alias, detail, action: false, run: () => onhost(host.alias) }
      // The name counts most; the address and the tags find a host too, each
      // on its own: letters scattered over all of them match nearly anything.
      const rest = best(...[detail, ...tagsOf(host)].map((field) => fuzzy(query, field)))
      return { item, score: best(fuzzy(query, host.alias), rest === null ? null : rest + 100) }
    })
    if (!query.trim()) return [...actions, ...rows].map((row) => row.item)
    const found = actions.map(({ item, en }) => ({ item, score: best(fuzzy(query, item.label), fuzzy(query, en)) }))
    return [...rows, ...found]
      .filter((row) => row.score !== null)
      .sort((a, b) => a.score! - b.score!)
      .map((row) => row.item)
  })

  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key]
    if (step && items.length) {
      e.preventDefault()
      at = (at + step + items.length) % items.length
    } else if (e.key === 'Enter') {
      e.preventDefault()
      items[at]?.run()
    }
  }
</script>

<Modal label={t('palette')} {onclose}>
  <input
    bind:value={query}
    oninput={() => (at = 0)}
    {onkeydown}
    role="combobox"
    aria-label={t('search')}
    aria-expanded="true"
    aria-controls="palette-options"
    aria-activedescendant={items.length ? `palette-option-${at}` : undefined}
    placeholder={t('search')}
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
    class="w-full bg-transparent px-4 py-3 text-fg outline-none placeholder:text-dim"
  />
  <ul
    id="palette-options"
    role="listbox"
    aria-label={t('palette')}
    class="max-h-[50vh] overflow-y-auto border-t border-edge py-1 empty:hidden"
  >
    {#each items as item, i (item.key)}
      <!-- The keyboard chooses from the input above: aria-activedescendant. -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id="palette-option-{i}"
        role="option"
        tabindex="-1"
        aria-selected={i === at}
        onclick={item.run}
        onpointermove={() => (at = i)}
        class="flex cursor-pointer items-baseline gap-3 px-4 py-1.5 aria-selected:bg-selection"
      >
        <span class={item.action ? 'text-magenta' : 'text-fg'}>{item.label}</span>
        <span class="truncate text-white">{item.detail}</span>
      </li>
    {/each}
  </ul>
  {#if items.length === 0}
    <p role="status" class="border-t border-edge px-4 py-3 text-white">{t('no_match')}</p>
  {/if}
  <footer class="flex gap-4 border-t border-edge px-4 py-2 text-white">
    <span><kbd class="text-magenta">↑↓ enter</kbd> {t('select')}</span>
    <span><kbd class="text-magenta">esc</kbd> {t('close')}</span>
  </footer>
</Modal>
