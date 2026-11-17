<script lang="ts">
  import type { Host } from './api'
  import { fuzzy } from './fuzzy'
  import { address, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { KEY } from './keymap'
  import { addr } from './streamer.svelte'

  type Props = {
    hosts: Host[]
    onhost: (alias: string) => void
    onquick: () => void
    onsnippets: () => void
    /** The host form, empty, and the vault: where the window has them. */
    onadd?: () => void
    onvault?: () => void
    /** The cheatsheet. */
    onshortcuts?: () => void
    onclose: () => void
  }
  let { hosts, onhost, onquick, onsnippets, onadd, onvault, onshortcuts, onclose }: Props = $props()

  /** `keys`: an action's own key, for the next time. */
  type Item = { key: string; label: string; detail: string; keys?: readonly string[]; action: boolean; run: () => void }

  let query = $state('')
  let at = $state(0)

  /** The loosest a match may be found by: the best of these, `null` if none. */
  const best = (...scores: (number | null)[]) =>
    scores.reduce((a, b) => (a === null ? b : b === null ? a : Math.min(a, b)), null)

  const items = $derived.by(() => {
    // `en`: an action's name whatever language its label is in ("qc" finds the first).
    const actions = [
      { key: 'quick-connect', label: t('quick_connect'), detail: 'user@host[:port]', keys: KEY.newTab, en: 'quick connect', run: onquick },
      { key: 'snippets', label: t('snippets'), detail: '', keys: KEY.snippets, en: 'snippets', run: onsnippets },
      ...(onadd ? [{ key: 'add-host', label: t('add_host'), detail: '', keys: KEY.add, en: 'add host', run: onadd }] : []),
      ...(onvault ? [{ key: 'vault', label: t('vault'), detail: '', en: 'vault', run: onvault }] : []),
      ...(onshortcuts ? [{ key: 'shortcuts', label: t('shortcuts'), detail: '', keys: KEY.shortcuts, en: 'keyboard shortcuts', run: onshortcuts }] : []),
    ].map(({ en, ...item }): { item: Item; en: string } => ({ item: { ...item, action: true }, en }))
    const rows = hosts.map((host) => {
      // Found by its real address, shown at the one the window may show.
      const detail = address(host)
      const item: Item = { key: host.alias, label: host.alias, detail: addr(host), action: false, run: () => onhost(host.alias) }
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
  <!-- The concept's search: a glass, the field, and the key that closes it. -->
  <label class="flex items-center gap-3 px-5 text-dim">
  <Icon name="search" />
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
    class="w-full bg-transparent py-4 text-base text-fg outline-none placeholder:text-dim"
  />
  <kbd>Esc</kbd>
  </label>
  <ul
    id="palette-options"
    role="listbox"
    aria-label={t('palette')}
    class="max-h-[50vh] overflow-y-auto border-t border-edge p-2 empty:hidden"
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
        class="flex cursor-pointer items-baseline gap-3 rounded-[10px] px-3 py-2 aria-selected:bg-selection aria-selected:inset-ring aria-selected:inset-ring-edge"
      >
        <span class="font-medium {item.action ? 'text-magenta' : 'text-ink'}">{item.label}</span>
        <span class="truncate font-mono text-xs text-dim">{item.detail}</span>
        {#if item.keys}
          <span class="ml-auto flex shrink-0 gap-1 self-center">
            {#each item.keys as key (key)}<kbd>{key}</kbd>{' '}{/each}
          </span>
        {/if}
      </li>
    {/each}
  </ul>
  {#if items.length === 0}
    <p role="status" class="border-t border-edge px-5 py-4 text-white">{t('no_match')}</p>
  {/if}
  <footer class="flex gap-4 border-t border-edge px-5 py-3 text-[13px] text-white">
    <span><kbd>↑</kbd> <kbd>↓</kbd> <kbd>Enter</kbd> {t('select')}</span>
    <span><kbd>Esc</kbd> {t('close')}</span>
  </footer>
</Modal>
