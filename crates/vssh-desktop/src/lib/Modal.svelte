<script lang="ts">
  import type { Snippet } from 'svelte'

  /**
   * `wide`: for what is read in it more than typed, a batch's answers.
   * `locked`: what runs in it must be seen to its end; neither Esc nor a
   * click beside it closes it.
   */
  type Props = { label: string; wide?: boolean; locked?: boolean; onclose: () => void; children: Snippet }
  let { label, wide = false, locked = false, onclose, children }: Props = $props()
  let dialog: HTMLDialogElement

  // A modal <dialog> brings the focus trap, Esc and the inert page with it.
  $effect(() => dialog.showModal())
</script>

<!-- A click on the backdrop closes, as Esc does for the keyboard. Esc is a
     `cancel`, refused while locked; a webview that closes the dialog all the
     same (Chromium, at the second Esc) gets it opened again. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  aria-label={label}
  oncancel={(e) => locked && e.preventDefault()}
  onclose={() => (locked ? dialog.showModal() : onclose())}
  onclick={(e) => !locked && e.target === dialog && dialog.close()}
  class="m-auto mt-[14vh] {wide ? 'w-[52rem]' : 'w-[36rem]'} max-w-[90vw] rounded-xl border border-edge bg-black p-0 text-fg shadow-2xl backdrop:bg-bg/70"
>
  {@render children()}
</dialog>
