<script lang="ts">
  import type { Host, Shown } from './api'
  import { fuzzy } from './fuzzy'
  import { address, tagsOf } from './hosts'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Keys from './Keys.svelte'
  import Modal from './Modal.svelte'
  import { KEY } from './keymap'
  import { addr, streamer, target } from './streamer.svelte'

  type Props = {
    hosts: Host[]
    onhost: (alias: string) => void
    /** Connects to what was typed, `[user@]host[:port]`, and saves nothing; rejects with the refusal, already in words. */
    onconnect: (target: string) => Promise<void>
    onsnippets: () => void
    /** The host form, empty, and the vault: where the window has them. */
    onadd?: () => void
    onvault?: () => void
    /** The cheatsheet. */
    onshortcuts?: () => void
    /** Focus mode, on or off; and, with a tab in front, another shell beside it or (`down`) under it. */
    onzen?: () => void
    onsplit?: (down: boolean) => void
    /** A question whose answer is a host ("Files: which host?"): the hosts are all it offers, in the order given. */
    pick?: string
    onclose: () => void
  }
  let { hosts, onhost, onconnect, onsnippets, onadd, onvault, onshortcuts, onzen, onsplit, pick, onclose }: Props = $props()

  /** `keys`: an action's own key, for the next time. */
  type Item = { key: string; label: string; detail: string; keys?: readonly string[]; action: boolean; run: () => void }

  let query = $state('')
  let at = $state(0)
  /** Why what was typed could not be connected to. */
  let refusal = $state<Shown | null>(null)
  let busy = false

  async function connect(to: string) {
    if (busy) return
    busy = true
    refusal = null
    try {
      await onconnect(to)
    } catch (error) {
      refusal = error as Shown
    } finally {
      busy = false
    }
  }

  /** The loosest a match may be found by: the best of these, `null` if none. */
  const best = (...scores: (number | null)[]) =>
    scores.reduce((a, b) => (a === null ? b : b === null ? a : Math.min(a, b)), null)

  const items = $derived.by(() => {
    // `en`: an action's name whatever language its label is in ("qc" finds the first).
    const actions = [
      { key: 'snippets', label: t('snippets'), detail: '', keys: KEY.snippets, en: 'snippets', run: onsnippets },
      ...(onadd ? [{ key: 'add-host', label: t('add_host'), detail: '', keys: KEY.add, en: 'add host', run: onadd }] : []),
      ...(onvault ? [{ key: 'vault', label: t('vault'), detail: '', en: 'vault', run: onvault }] : []),
      ...(onshortcuts ? [{ key: 'shortcuts', label: t('shortcuts'), detail: '', keys: KEY.shortcuts, en: 'keyboard shortcuts', run: onshortcuts }] : []),
      ...(onsplit
        ? [
            { key: 'split-right', label: t('split_right'), detail: '', keys: KEY.splitRight, en: 'split right', run: () => onsplit(false) },
            { key: 'split-down', label: t('split_down'), detail: '', keys: KEY.splitDown, en: 'split down', run: () => onsplit(true) },
          ]
        : []),
      ...(onzen ? [{ key: 'focus', label: t('focus_mode'), detail: '', keys: KEY.focus, en: 'focus mode', run: onzen }] : []),
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
    const typed = query.trim()
    if (pick) {
      const asked = typed ? rows.filter((row) => row.score !== null).sort((a, b) => a.score! - b.score!) : rows
      return asked.map((row) => row.item)
    }
    if (!typed) return [...actions, ...rows].map((row) => row.item)
    const found = actions.map(({ item, en }) => ({ item, score: best(fuzzy(query, item.label), fuzzy(query, en)) }))
    const matched = [...rows, ...found]
      .filter((row) => row.score !== null)
      .sort((a, b) => a.score! - b.score!)
      .map((row) => row.item)
    if (/\s/.test(typed)) return matched
    const direct = {
      key: 'connect-to',
      label: t('connect_to').replace('{target}', target(typed)),
      detail: `${t('quick_connect')} · ${t('quick_connect_note')}`,
      keys: KEY.connect,
      action: true,
      run: () => void connect(typed),
    }
    // What is typed is the name of nothing here: it is an address to connect to, as it stands.
    if (!matched.length) return [direct]
    // An address may look like a host's and be another: the way to it is there, after what was found.
    return /[.:@]/.test(typed) ? [...matched, direct] : matched
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

<Modal label={pick ?? t('palette')} {onclose}>
  <!-- The concept's search: a glass, the field, and the key that closes it. -->
  <label class="flex items-center gap-3 px-5 text-dim">
  <Icon name="search" />
  <input
    bind:value={query}
    oninput={() => {
      at = 0
      refusal = null
    }}
    data-masked={(streamer.on && /[.:@]/.test(query)) || undefined}
    {onkeydown}
    role="combobox"
    aria-label={pick ?? t('search')}
    aria-expanded="true"
    aria-controls="palette-options"
    aria-activedescendant={items.length ? `palette-option-${at}` : undefined}
    placeholder={pick ?? t('search')}
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
    class="w-full bg-transparent py-4 text-base text-fg outline-none placeholder:text-dim data-[masked]:[-webkit-text-security:disc]"
  />
  <Keys keys="Esc" />
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
        class="flex cursor-pointer items-baseline gap-3 rounded-ctl px-3 py-2 aria-selected:bg-selection aria-selected:inset-ring aria-selected:inset-ring-edge"
      >
        <span class="font-medium {item.action ? 'text-magenta' : 'text-ink'}">{item.label}</span>
        <span class="truncate font-mono text-xs text-dim">{item.detail}</span>
        {#if item.keys}
          <span class="ml-auto flex shrink-0 gap-2 self-center">
            {#each item.keys as key (key)}<Keys keys={key} />{' '}{/each}
          </span>
        {/if}
      </li>
    {/each}
  </ul>
  {#if items.length === 0}
    <p role="status" class="border-t border-edge px-5 py-4 text-white">{t('no_match')}</p>
  {/if}
  {#if refusal}
    <p role="alert" class="border-t border-edge px-5 py-3 text-red"><strong>{refusal.label}</strong> {refusal.message}</p>
  {/if}
  <footer class="flex gap-4 border-t border-edge px-5 py-3 text-sm text-white">
    <span><Keys keys="↑" /> <Keys keys="↓" /> <Keys keys="Enter" /> {t('select')}</span>
    <span><Keys keys="Esc" /> {t('close')}</span>
  </footer>
</Modal>
