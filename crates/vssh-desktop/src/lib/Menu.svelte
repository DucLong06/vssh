<script lang="ts" module>
  /** A row of a menu; `null`: a line between two groups of them. */
  export type MenuItem = { label: string; keys?: string; danger?: boolean; off?: boolean; run: () => void } | null
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
  /** A row's height and the box's own, to keep all of it inside the window. */
  const tall = $derived(items.reduce((sum, item) => sum + (item ? 28 : 9), 10))

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
  style="left: {Math.max(4, Math.min(x, innerWidth - 228))}px; top: {Math.max(4, Math.min(y, innerHeight - tall - 4))}px"
  class="fixed z-40 min-w-48 max-w-56 rounded-xl border border-edge bg-panel p-1 text-[13px] text-fg shadow-float"
>
  {#each items as item, at (at)}
    {#if item}
      <li role="none">
        <button
          type="button"
          role="menuitem"
          disabled={item.off}
          onclick={item.run}
          class="flex w-full cursor-pointer items-center gap-4 rounded-lg px-3 py-1 text-left hover:bg-selection focus-visible:bg-selection focus-visible:outline-none disabled:cursor-default disabled:text-dim disabled:hover:bg-transparent {item.danger ? 'text-red' : ''}"
        >
          <span class="min-w-0 flex-1 truncate">{item.label}</span>
          {#if item.keys}<span aria-hidden="true"><Keys keys={item.keys} /></span>{/if}
        </button>
      </li>
    {:else}
      <li role="separator" class="mx-2 my-1 border-t border-edge"></li>
    {/if}
  {/each}
</ul>
