<script lang="ts">
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { sheet, type Group, type View } from './keymap'

  // The cheatsheet: every key the window has, by where it works. The keys of
  // the view in front come first; the field narrows them, by name or by key.
  type Props = { view?: View; onclose: () => void }
  let { view, onclose }: Props = $props()

  let query = $state('')

  const groups = $derived.by(() => {
    const words = query.trim().toLowerCase()
    // A stable sort: behind the view's own group the others are in the sheet's order.
    const mine = (group: Group) => Number(view !== undefined && group.view === view)
    return sheet()
      .sort((a, b) => mine(b) - mine(a))
      .map((group) => ({
        ...group,
        rows: group.rows.filter(([keys, label]) => [label, ...keys].some((text) => text.toLowerCase().includes(words))),
      }))
      .filter((group) => group.rows.length)
  })
</script>

<Modal label={t('shortcuts')} wide {onclose}>
  <!-- The palette's head: a glass, the field, and the key that closes it. -->
  <label class="flex items-center gap-3 px-5 text-dim">
    <Icon name="search" />
    <input
      bind:value={query}
      aria-label={t('search_shortcuts')}
      placeholder={t('search_shortcuts')}
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      class="w-full bg-transparent py-4 text-base text-fg outline-none placeholder:text-dim"
    />
    <kbd>Esc</kbd>
  </label>
  <div class="max-h-[62vh] columns-2 gap-10 overflow-y-auto border-t border-edge px-6 pt-5 pb-2">
    {#each groups as group (group.name)}
      <section aria-label={group.name} class="mb-5 break-inside-avoid">
        <h3 class="pb-1 text-[11px] font-semibold tracking-widest text-dim uppercase">{group.name}</h3>
        <dl>
          {#each group.rows as [keys, label] (label)}
            <div class="flex items-center justify-between gap-4 border-b border-edge/50 py-1 last:border-0">
              <dt class="min-w-0 text-fg first-letter:uppercase">{label}</dt>
              <dd class="flex shrink-0 gap-1">
                <!-- The space is for whoever reads the text: J K, not JK. -->
                {#each keys as key (key)}<kbd>{key}</kbd>{' '}{/each}
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {:else}
      <p role="status" class="pb-3 text-white">{t('no_shortcut')}</p>
    {/each}
  </div>
</Modal>
