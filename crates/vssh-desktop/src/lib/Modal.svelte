<script lang="ts">
  import type { Snippet } from 'svelte'

  /**
   * `size`: `md` for a question, `wide` for what is read in it more than typed (a batch's
   * answers), `xl` for a form with two columns. `wide` is the older word for the second.
   * `locked`: what runs in it must be seen to its end; neither Esc nor a
   * click beside it closes it.
   */
  type Size = 'md' | 'wide' | 'xl'
  type Props = { label: string; size?: Size; wide?: boolean; locked?: boolean; onclose: () => void; children: Snippet }
  let { label, size, wide = false, locked = false, onclose, children }: Props = $props()
  // No dialog is taller than the window: 6vh under each, as much over an xl, 12vh over the others.
  const SIZES: Record<Size, string> = {
    md: 'mt-[12vh] max-h-[82vh] w-[38rem] max-w-[92vw]',
    wide: 'mt-[12vh] max-h-[82vh] w-[56rem] max-w-[92vw]',
    xl: 'mt-[6vh] max-h-[88vh] w-[min(78rem,94vw)]',
  }
  let dialog: HTMLDialogElement

  // A modal <dialog> brings the focus trap, Esc and the inert page with it.
  $effect(() => dialog.showModal())
</script>

<!-- A column, open: what is in it and may shrink (`min-h-0`, with a part that scrolls) keeps its
     head and its foot in view; what may not is scrolled as a whole, here.
     A click on the backdrop closes, as Esc does for the keyboard. Esc is a
     `cancel`, refused while locked; a webview that closes the dialog all the
     same (Chromium, at the second Esc) gets it opened again. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  aria-label={label}
  oncancel={(e) => locked && e.preventDefault()}
  onclose={() => (locked ? dialog.showModal() : onclose())}
  onclick={(e) => !locked && e.target === dialog && dialog.close()}
  class="m-auto {SIZES[size ?? (wide ? 'wide' : 'md')]} flex-col overflow-y-auto open:flex portrait:w-[94vw] portrait:max-w-none animate-slide rounded-2xl border border-edge bg-panel p-0 text-fg shadow-float backdrop:bg-page/70 motion-reduce:animate-none"
>
  {@render children()}
</dialog>
