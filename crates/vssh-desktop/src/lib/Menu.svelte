<script lang="ts" module>
  import { scaled } from './scale.svelte'

  /** A small button of a row that is a setting with steps: what it shows, what it is called, and its tooltip. Choosing one leaves the menu open. */
  type Part = { text: string; label: string; title?: string; off?: boolean; run: () => void }
  /** A row of a menu, or (`parts`) a name and its small buttons; `null`: a line between two groups of them. */
  export type MenuItem =
    | { label: string; keys?: string; note?: string; danger?: boolean; off?: boolean; run: () => void }
    | { label: string; parts: Part[] }
    | null
  /** How tall a menu of these is: a row's height and the box's own, at the window's size. For keeping it inside the window, and for opening it upward. */
  export const tall = (items: MenuItem[]) => scaled(items.reduce((sum, item) => sum + (item ? 32 : 9), 10))
  /** Where a menu opens, for what the right button or the menu key was on. */
  export const pointed = (e: MouseEvent) => {
    // The menu key says nothing of where: under what it is about.
    const box = (e.target as HTMLElement).getBoundingClientRect()
    return { x: e.clientX || box.left + 24, y: e.clientY || box.top + 24 }
  }
</script>

<script lang="ts">
  import Keys from './Keys.svelte'

  // The right button's menu, the window's own: at the pointer and inside the
  // window. It takes the keys while it is open; a click anywhere, Esc, or a
  // row chosen closes it.
  type Props = { label: string; x: number; y: number; items: MenuItem[]; onclose: (escaped?: boolean) => void }
  let { label, x, y, items, onclose }: Props = $props()

  let el = $state<HTMLElement>()

  $effect(() => el?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus())

  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key]
    if (!step || !el) return
    e.preventDefault()
    const all = [...el.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')]
    all[(all.indexOf(document.activeElement as HTMLButtonElement) + step + all.length) % all.length]?.focus()
  }
</script>

<svelte:window
  onclick={() => onclose()}
  onblur={() => onclose()}
  onkeydowncapture={(e) => {
    if (e.key !== 'Escape') return
    // The menu's, and nobody else's: what is under it stays as it is.
    e.stopPropagation()
    onclose(true)
  }}
/>

<ul
  bind:this={el}
  role="menu"
  aria-label={label}
  {onkeydown}
  oncontextmenu={(e) => e.preventDefault()}
  style="left: {Math.max(4, Math.min(x, innerWidth - scaled(288) - 4))}px; top: {Math.max(4, Math.min(y, innerHeight - tall(items) - 4))}px"
  class="fixed z-40 min-w-48 max-w-72 rounded-xl border border-edge bg-panel p-1 text-sm text-fg shadow-float"
>
  {#each items as item, at (at)}
    {#if item && 'parts' in item}
      <li role="none" class="flex items-center gap-1 py-1 pr-2 pl-3">
        <span class="min-w-0 flex-1 truncate">{item.label}</span>
        {#each item.parts as part (part.label)}
          <!-- Not `disabled`: at its end a button keeps the focus it has, and does nothing. -->
          <button
            type="button"
            role="menuitem"
            aria-label={part.label}
            aria-disabled={part.off || undefined}
            title={part.title}
            onclick={(e) => {
              // The window's click closes the menu: this one is not for it.
              e.stopPropagation()
              part.run()
            }}
            class="h-6 min-w-8 cursor-pointer rounded-md px-1.5 text-xs tabular-nums hover:bg-selection focus-visible:bg-selection focus-visible:outline-none aria-disabled:cursor-default aria-disabled:text-dim aria-disabled:hover:bg-transparent"
          >
            {part.text}
          </button>
        {/each}
      </li>
    {:else if item}
      <li role="none">
        <button
          type="button"
          role="menuitem"
          disabled={item.off}
          onclick={item.run}
          class="flex w-full cursor-pointer items-center gap-4 rounded-lg px-3 py-1.5 text-left hover:bg-selection focus-visible:bg-selection focus-visible:outline-none disabled:cursor-default disabled:text-dim disabled:hover:bg-transparent {item.danger ? 'text-red' : ''}"
        >
          <span class="min-w-0 flex-1 truncate">{item.label}</span>
          {#if item.note}<span class="text-dim">{item.note}</span>{/if}
          {#if item.keys}<span aria-hidden="true"><Keys keys={item.keys} /></span>{/if}
        </button>
      </li>
    {:else}
      <li role="separator" class="mx-2 my-1 border-t border-edge"></li>
    {/if}
  {/each}
</ul>
