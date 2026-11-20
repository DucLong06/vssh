<script lang="ts">
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Keys from './Keys.svelte'
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
    <Keys keys="Esc" />
  </label>
  <!-- The scroll is the outer box's: columns with a height of their own would overflow sideways. -->
  <div class="max-h-[62vh] overflow-y-auto border-t border-edge">
  <div class="columns-2 gap-10 px-7 pt-5 pb-2">
    {#each groups as group (group.name)}
      <section aria-label={group.name} class="mb-5 break-inside-avoid">
        <h3 class="pb-1 text-2xs font-semibold tracking-widest text-dim uppercase">{group.name}</h3>
        <dl>
          {#each group.rows as [keys, label] (label)}
            <div class="flex items-center justify-between gap-4 border-b border-edge/50 py-1.5 last:border-0">
              <dt class="min-w-0 text-fg first-letter:uppercase">{label}</dt>
              <dd class="flex shrink-0 items-center gap-1.5">
                <!-- The first is the key; the others do the same, and say so: Tab / N is not Tab then N. -->
                {#each keys as key, at (key)}{#if at}<span aria-hidden="true" class="text-dim">/</span>{' '}{/if}<Keys keys={key} class={at ? 'opacity-60' : ''} />{' '}{/each}
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {:else}
      <p role="status" class="pb-3 text-white">{t('no_shortcut')}</p>
    {/each}
  </div>
  </div>
</Modal>
