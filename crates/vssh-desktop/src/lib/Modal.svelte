<script lang="ts">
  import type { Snippet } from 'svelte'

  type Props = { label: string; onclose: () => void; children: Snippet }
  let { label, onclose, children }: Props = $props()
  let dialog: HTMLDialogElement

  // A modal <dialog> brings the focus trap, Esc and the inert page with it.
  $effect(() => dialog.showModal())
</script>

<!-- A click on the backdrop closes, as Esc does for the keyboard. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  aria-label={label}
  {onclose}
  onclick={(e) => e.target === dialog && dialog.close()}
  class="m-auto mt-[14vh] w-[36rem] max-w-[90vw] rounded-xl border border-edge bg-black p-0 text-fg shadow-2xl backdrop:bg-bg/70"
>
  {@render children()}
</dialog>
