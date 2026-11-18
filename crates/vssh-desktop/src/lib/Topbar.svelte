<script lang="ts">
  // The head of the dashboard: what it is, and a line about it, for a reader of the screen.
  // Not drawn: the top row of the view is its search, and the figures under it say the same to the eye.
  // The way to the palette and the settings are the rail's, which every view has.
  type Props = {
    title: string
    /** A line under the title: what the view holds. */
    sub?: string
  }
  let { title, sub = '' }: Props = $props()
</script>

<header class="sr-only">
  <h1>{title}</h1>
  {#if sub}
    <p>{sub}</p>
  {/if}
</header>
