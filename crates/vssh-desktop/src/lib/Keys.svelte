<script lang="ts">
  // A key as the window draws it: a cap for each key of a chord. The + between
  // them is said to whoever reads the text, and not drawn.
  let { keys, class: extra = '' }: { keys: string; class?: string } = $props()
</script>

<kbd class={extra}>{#each keys.split('+') as key, at (at)}{#if at}<span class="sr-only">+</span>{/if}<span>{key}</span>{/each}</kbd>
